//! Retain keyframes under the configured budget, not one full viewport per
//! sampling tick. A frame is removed only after proving all its known pixels
//! remain covered by the other retained frames.
use super::*;

pub(super) fn subtract(regions: Vec<ExcludedRect>, cover: ExcludedRect) -> Vec<ExcludedRect> {
    let mut remaining = vec![];
    for r in regions {
        let (x, y) = (r.x.max(cover.x), r.y.max(cover.y));
        let (right, bottom) = (
            (r.x + r.width).min(cover.x + cover.width),
            (r.y + r.height).min(cover.y + cover.height),
        );
        if right <= x || bottom <= y {
            remaining.push(r);
            continue;
        }
        for (a, b, w, h) in [
            (r.x, r.y, r.width, y - r.y),
            (r.x, bottom, r.width, r.y + r.height - bottom),
            (r.x, y, x - r.x, bottom - y),
            (right, y, r.x + r.width - right, bottom - y),
        ] {
            if w > 0 && h > 0 {
                remaining.push(ExcludedRect {
                    x: a,
                    y: b,
                    width: w,
                    height: h,
                });
            }
        }
    }
    remaining
}

struct CoverageTables {
    coverage: Vec<Vec<ExcludedRect>>,
    body_coverage: Vec<Vec<ExcludedRect>>,
    providers: Vec<Vec<ExcludedRect>>,
}

fn coverage_tables(
    doc: &Document,
    extra: Option<&Frame>,
    fixed: &fixed::Fixed,
) -> Result<CoverageTables> {
    let frames: Vec<_> = doc.frames.iter().chain(extra).collect();
    let layout = composition::Layout::new(doc, extra, fixed, false)?;
    let coverage: Vec<_> = frames
        .iter()
        .map(|f| layout.coverage(f, fixed, false))
        .collect();
    // Fixed boundaries can shrink as more moving text becomes visible. Preserve
    // the complete body mapping too, including pixels currently classified as
    // chrome, so a later correction cannot expose a hole in a deleted frame.
    let unfixed = fixed::Fixed::default();
    let body_layout = composition::Layout::new(doc, extra, &unfixed, false)?;
    let body_coverage: Vec<_> = frames
        .iter()
        .map(|f| body_layout.coverage(f, &unfixed, false))
        .collect();
    let providers: Vec<_> = frames
        .iter()
        .map(|f| {
            let verified = f.body_verified
                || extra.is_some_and(|inc| inc.body_verified && f.viewport == inc.viewport);
            if verified {
                layout.coverage(f, fixed, true)
            } else {
                vec![]
            }
        })
        .collect();
    Ok(CoverageTables {
        coverage,
        body_coverage,
        providers,
    })
}

fn is_redundant(i: usize, frames: &[&Frame], removed: &[bool], tables: &CoverageTables) -> bool {
    let mut others: Vec<_> = (0..frames.len())
        .filter(|&j| j != i && !removed[j])
        .collect();
    others.sort_by_key(|&j| frames[j].offset.abs_diff(frames[i].offset));
    // Other frames may only prove future body coverage with rows
    // already classified as body, never their still-fixed chrome.
    [
        (&tables.coverage[i], &tables.coverage),
        (&tables.body_coverage[i], &tables.providers),
    ]
    .into_iter()
    .all(|(candidate, coverage)| {
        let mut remaining = candidate.clone();
        for &j in &others {
            for &rect in &coverage[j] {
                remaining = subtract(remaining, rect);
                if remaining.is_empty() {
                    return true;
                }
            }
        }
        remaining.is_empty()
    })
}

/// Drop redundant clipped or out-of-bounds frames once all remaining pixels are
/// provably covered by other retained frames, while preserving unique boundary
/// donors needed for HUD-occluded strips.
pub(super) fn prune_clipped(doc: &mut Document) {
    if !doc
        .frames
        .iter()
        .any(|f| f.body_range.is_some() || f.offset < doc.minimum || f.offset > doc.maximum)
    {
        return;
    }
    let Ok(tables) = coverage_tables(doc, None, &doc.fixed) else {
        return;
    };
    let frames: Vec<_> = doc.frames.iter().collect();
    let mut removed = vec![false; frames.len()];
    for i in 0..frames.len() {
        let removable_candidate = frames[i].body_range.is_some()
            || frames[i].offset < doc.minimum
            || frames[i].offset > doc.maximum;
        let boundary_safe = !(frames[i].offset == doc.minimum || frames[i].offset == doc.maximum)
            || frames.iter().enumerate().any(|(j, f)| {
                j != i
                    && !removed[j]
                    && f.offset == frames[i].offset
                    && f.viewport == frames[i].viewport
            });
        if removable_candidate && boundary_safe && is_redundant(i, &frames, &removed, &tables) {
            removed[i] = true;
        }
    }
    for i in (0..removed.len()).rev() {
        if removed[i] {
            doc.bytes -= doc.frames[i].pixels.byte_len();
            doc.frames.remove(i);
        }
    }
}

pub(super) fn removals(
    doc: &Document,
    incoming: &Frame,
    fixed: &fixed::Fixed,
) -> Result<Vec<usize>> {
    let frames: Vec<_> = doc.frames.iter().chain(Some(incoming)).collect();
    let minimum = doc.minimum.min(incoming.offset);
    let maximum = doc.maximum.max(incoming.offset);
    let mut bytes = doc.bytes + incoming.pixels.byte_len();
    let mut count = frames.len();
    if count <= doc.limit && bytes <= MAX_DOCUMENT_BYTES {
        return Ok(vec![]);
    }
    let tables = coverage_tables(doc, Some(incoming), fixed)?;
    let mut removed = vec![false; frames.len()];
    while count > doc.limit || bytes > MAX_DOCUMENT_BYTES {
        let candidate = (0..frames.len())
            .filter(|&i| {
                !removed[i]
                    && (!(frames[i].offset == minimum || frames[i].offset == maximum)
                        || frames.iter().enumerate().any(|(j, f)| {
                            j != i
                                && !removed[j]
                                && f.offset == frames[i].offset
                                && f.viewport == frames[i].viewport
                        }))
            })
            .find(|&i| is_redundant(i, &frames, &removed, &tables));
        let Some(i) = candidate else {
            return Err(LimitReached::Frames.into());
        };
        removed[i] = true;
        count -= 1;
        bytes -= frames[i].pixels.byte_len();
    }
    Ok(removed
        .iter()
        .enumerate()
        .filter_map(|(i, &r)| r.then_some(i))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(offset: i64, masked: bool) -> Frame {
        Frame {
            offset,
            body_range: None,
            body_verified: true,
            viewport: None,
            pixels: FramePixels::Png(vec![0]),
            preview: Arc::new(RgbaImage::new(1, 1)),
            scaled_preview: std::sync::Mutex::new(None),
            excluded: if masked {
                vec![ExcludedRect {
                    x: 60,
                    y: 0,
                    width: 1,
                    height: 200,
                }]
            } else {
                vec![]
            },
        }
    }
    #[test]
    fn coverage_proof_preserves_the_only_known_pixels_under_other_frames_masks() {
        let mut doc = Document::new(Axis::Vertical, 3);
        doc.dimensions = Some((128, 200));
        doc.frames = vec![frame(0, true), frame(40, false), frame(80, true)];
        doc.maximum = 80;
        doc.bytes = 3;
        // The middle frame is the only source for a thin column. It must survive,
        // even though its geometric viewport is covered by its neighbours.
        assert_eq!(
            removals(&doc, &frame(120, true), &doc.fixed).unwrap(),
            vec![2]
        );
    }
    #[test]
    fn correcting_an_overestimated_title_cannot_expose_discarded_body_pixels() {
        let mut doc = Document::new(Axis::Vertical, 3);
        doc.dimensions = Some((128, 200));
        doc.frames = vec![frame(0, false), frame(40, false), frame(80, false)];
        doc.maximum = 80;
        doc.bytes = 3;
        doc.fixed.observe(100, 0, None, Axis::Vertical, 200);
        let mut incoming = frame(120, false);
        for (frame, y) in [(&mut doc.frames[0], 120), (&mut incoming, 0)] {
            frame.excluded = vec![ExcludedRect {
                x: 60,
                y,
                width: 1,
                height: 1,
            }];
        }
        doc.frames[2].excluded = vec![ExcludedRect {
            x: 60,
            y: 40,
            width: 1,
            height: 1,
        }];
        // Global body pixel (60,120) belongs only to frame40. While the title
        // estimate is100px it is cropped out of that frame's current body map.
        // A later, smaller title estimate needs it again.
        assert!(
            removals(&doc, &incoming, &doc.fixed)
                .unwrap_err()
                .is::<LimitReached>()
        );
        doc.fixed.observe(0, 0, None, Axis::Vertical, 200);
        assert_eq!(removals(&doc, &incoming, &doc.fixed).unwrap(), vec![2]);
    }
    #[test]
    fn newly_exposed_unverified_chrome_cannot_prove_old_body_redundant() {
        let mut doc = Document::new(Axis::Vertical, 2);
        doc.dimensions = Some((128, 200));
        doc.frames = vec![frame(0, false), frame(160, false)];
        doc.maximum = 160;
        doc.bytes = 2;
        let mut incoming = frame(200, false);
        // Frame160 needs the new viewport to cover its final160..360 range.
        // Until this geometry has a nonzero match, that provider might be title.
        incoming.body_verified = false;
        incoming.viewport = Some(crate::annotation::Bounds {
            x: 0.,
            y: -40.,
            width: 128.,
            height: 200.,
        });
        assert!(
            removals(&doc, &incoming, &doc.fixed)
                .unwrap_err()
                .is::<LimitReached>()
        );
        incoming.body_verified = true;
        assert_eq!(removals(&doc, &incoming, &doc.fixed).unwrap(), vec![1]);
    }
    #[test]
    fn genuinely_unique_content_reaches_capacity_without_mutating_the_document() {
        let mut doc = Document::new(Axis::Vertical, 2);
        doc.dimensions = Some((128, 200));
        doc.frames = vec![frame(0, false), frame(160, false)];
        doc.maximum = 160;
        doc.bytes = 2;
        let error = removals(&doc, &frame(320, false), &doc.fixed).unwrap_err();
        assert!(error.is::<LimitReached>());
        assert_eq!(doc.frames.len(), 2);
        assert_eq!(doc.bytes, 2);
    }
}
