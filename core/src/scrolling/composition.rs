//! Body pixels follow document displacement. Fixed chrome follows physical
//! geometry and can be recovered from any unoccluded frame, in either order.
use super::*;
use std::ops::Range;

struct Canvas {
    image: RgbaImage,
    priority: Vec<u16>,
}
impl Canvas {
    fn copy(
        &mut self,
        source: &RgbaImage,
        rows: Range<u32>,
        offset: i64,
        target: Range<u32>,
        excluded: &[ExcludedRect],
        only_missing: bool,
    ) {
        let w = self.image.width() as usize;
        let out_raw = self.image.as_mut();
        let src_raw = source.as_raw();
        for y in rows.clone() {
            let dest = offset + y as i64;
            if dest < target.start as i64 || dest >= target.end as i64 {
                continue;
            }
            let row_priority = if only_missing {
                u16::MAX
            } else {
                let edge_clearance = (y - rows.start).min(rows.end - 1 - y);
                1 + edge_clearance.min(u32::from(u16::MAX - 2)) as u16
            };
            let dest_row = dest as usize * w;
            let src_row = y as usize * w;
            for (a, b) in occlusion::spans(w as u32, y, excluded) {
                let (a, b) = (a as usize, b as usize);
                let (first, last) = (dest_row + a, dest_row + b);
                if !only_missing
                    && self.priority[first..last]
                        .iter()
                        .all(|&p| row_priority >= p)
                {
                    let from = (src_row + a) * 4;
                    out_raw[first * 4..last * 4]
                        .copy_from_slice(&src_raw[from..from + (b - a) * 4]);
                    self.priority[first..last].fill(row_priority);
                    continue;
                }
                for x in a..b {
                    let index = dest_row + x;
                    let take = if only_missing {
                        self.priority[index] == 0
                    } else {
                        row_priority >= self.priority[index]
                    };
                    if take {
                        let px = (src_row + x) * 4;
                        let dst = index * 4;
                        out_raw[dst..dst + 4].copy_from_slice(&src_raw[px..px + 4]);
                        self.priority[index] = row_priority;
                    }
                }
            }
        }
    }
}
impl Document {
    pub(super) fn compose(&self, preview: bool) -> Result<RgbaImage> {
        let layout = Layout::new(self, None, &self.fixed, preview)?;
        let (w, h, source_h, scale) = (
            layout.width,
            layout.height,
            layout.source_height,
            layout.scale,
        );
        let mut canvas = Canvas {
            image: RgbaImage::new(w, h),
            priority: vec![0; w as usize * h as usize],
        };
        let decoded: Vec<_> = self
            .frames
            .iter()
            .map(|frame| -> Result<_> {
                let image = if preview {
                    if frame.preview.dimensions() == (w, source_h) {
                        Arc::clone(&frame.preview)
                    } else {
                        let mut cached = frame.scaled_preview.lock().unwrap();
                        if let Some((cw, ch, ref img)) = *cached
                            && cw == w
                            && ch == source_h
                        {
                            Arc::clone(img)
                        } else {
                            let img = Arc::new(image::imageops::resize(
                                &*frame.preview,
                                w,
                                source_h,
                                image::imageops::FilterType::Triangle,
                            ));
                            *cached = Some((w, source_h, Arc::clone(&img)));
                            img
                        }
                    }
                } else {
                    frame.pixels.image()?
                };
                let excluded: Vec<_> = frame.excluded.iter().map(|r| r.scaled(scale)).collect();
                Ok((frame, image, excluded))
            })
            .collect::<Result<_>>()?;
        for in_range in [true, false] {
            for (frame, image, excluded) in &decoded {
                let inside = (self.minimum..=self.maximum).contains(&frame.offset);
                if inside != in_range {
                    continue;
                }
                for layer in layout.layers(frame, &self.fixed) {
                    if layer.fixed {
                        canvas.copy(
                            image,
                            layer.rows,
                            layer.offset,
                            layer.target,
                            excluded,
                            true,
                        );
                    }
                }
            }
        }
        let mut order: Vec<_> = (0..decoded.len()).collect();
        order.sort_by_key(|&i| decoded[i].0.offset);
        for i in order {
            let (frame, ref image, ref excluded) = decoded[i];
            for layer in layout.layers(frame, &self.fixed) {
                if !layer.fixed {
                    canvas.copy(
                        image,
                        layer.rows,
                        layer.offset,
                        layer.target,
                        excluded,
                        false,
                    );
                }
            }
        }
        // Coverage is independent of alpha, and includes genuine gaps between
        // body spans even when none of the source frames had an overlay.
        if !preview && canvas.priority.contains(&0) {
            return Err(t("Some content was covered by the toolbar. Scroll back slowly to fill the missing area.").into());
        }
        let output = canvas.image;
        Ok(match self.axis {
            Axis::Vertical => output,
            Axis::Horizontal => image::imageops::rotate270(&output),
        })
    }
}

/// One geometry plan is shared by rendering and lossless frame retention.
pub(super) struct Layout {
    width: u32,
    height: u32,
    source_height: u32,
    scale: f64,
    minimum: i64,
    first: Option<crate::annotation::Bounds>,
    last: Option<crate::annotation::Bounds>,
    top: u32,
    bottom: u32,
    axis: Axis,
    original_height: u32,
}
struct Layer {
    rows: Range<u32>,
    offset: i64,
    target: Range<u32>,
    fixed: bool,
}
impl Layout {
    pub(super) fn new(
        doc: &Document,
        extra: Option<&Frame>,
        fixed: &fixed::Fixed,
        preview: bool,
    ) -> Result<Self> {
        let (width, height) = doc
            .dimensions
            .ok_or_else(|| t("Scrolling capture requires 1 to 200 frames"))?;
        let minimum = extra.map_or(doc.minimum, |f| doc.minimum.min(f.offset));
        let maximum = extra.map_or(doc.maximum, |f| doc.maximum.max(f.offset));
        let first = doc.boundary_frame(extra, minimum).unwrap().viewport;
        let last = doc.boundary_frame(extra, maximum).unwrap().viewport;
        let extent = height + (maximum - minimum) as u32;
        let scale = if preview {
            (180. / width as f64).min(360. / extent as f64).min(1.)
        } else {
            1.
        };
        Ok(Self {
            width: (width as f64 * scale).round().max(1.) as u32,
            height: (extent as f64 * scale).round().max(1.) as u32,
            source_height: (height as f64 * scale).round().max(1.) as u32,
            top: (fixed.edges(first, doc.axis, height).0 as f64 * scale).round() as u32,
            bottom: (fixed.edges(last, doc.axis, height).1 as f64 * scale).round() as u32,
            original_height: height,
            scale,
            minimum,
            first,
            last,
            axis: doc.axis,
        })
    }
    fn layers(&self, frame: &Frame, fixed: &fixed::Fixed) -> [Layer; 3] {
        let (head, foot) = fixed.edges(frame.viewport, self.axis, self.original_height);
        let head = (head as f64 * self.scale).round() as u32;
        let foot = (foot as f64 * self.scale).round() as u32;
        let mut body = head..self.source_height.saturating_sub(foot);
        if let Some([start, end]) = frame.body_range {
            body.start = body
                .start
                .max(((start - frame.offset).max(0) as f64 * self.scale).round() as u32);
            body.end = body
                .end
                .min(((end - frame.offset).max(0) as f64 * self.scale).round() as u32);
        }
        [
            Layer {
                rows: body,
                offset: ((frame.offset - self.minimum) as f64 * self.scale).round() as i64,
                target: self.top..self.height.saturating_sub(self.bottom),
                fixed: false,
            },
            Layer {
                rows: 0..head,
                offset: (fixed::Fixed::physical_delta(
                    self.first,
                    frame.viewport,
                    self.axis,
                    self.original_height,
                ) * self.scale)
                    .round() as i64,
                target: 0..self.top,
                fixed: true,
            },
            Layer {
                rows: self.source_height.saturating_sub(foot)..self.source_height,
                offset: self.height as i64 - self.source_height as i64
                    + (fixed::Fixed::physical_delta(
                        self.last,
                        frame.viewport,
                        self.axis,
                        self.original_height,
                    ) * self.scale)
                        .round() as i64,
                target: self.height.saturating_sub(self.bottom)..self.height,
                fixed: true,
            },
        ]
    }
    /// Exact visible rectangles at native resolution; no pixel buffer allocation.
    pub(super) fn coverage(
        &self,
        frame: &Frame,
        fixed: &fixed::Fixed,
        body_only: bool,
    ) -> Vec<ExcludedRect> {
        debug_assert_eq!(self.scale, 1.);
        let mut result = vec![];
        for layer in self.layers(frame, fixed) {
            if body_only && layer.fixed {
                continue;
            }
            let start = (layer.target.start as i64 - layer.offset)
                .max(layer.rows.start as i64)
                .max(0) as u32;
            let end = (layer.target.end as i64 - layer.offset)
                .min(layer.rows.end as i64)
                .max(0) as u32;
            if end <= start {
                continue;
            }
            let mut cuts = vec![start, end];
            for r in &frame.excluded {
                cuts.push(r.y.clamp(start, end));
                cuts.push((r.y + r.height).clamp(start, end));
            }
            cuts.sort_unstable();
            cuts.dedup();
            for pair in cuts.windows(2) {
                for (a, b) in occlusion::spans(self.width, pair[0], &frame.excluded) {
                    result.push(ExcludedRect {
                        x: a,
                        y: (layer.offset + pair[0] as i64) as u32,
                        width: b - a,
                        height: pair[1] - pair[0],
                    });
                }
            }
        }
        result
    }
}
