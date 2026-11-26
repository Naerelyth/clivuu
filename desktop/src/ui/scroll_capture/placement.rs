//! Long-capture HUD placement. A live preview never covers sampled pixels.
use clivuu_core::{annotation::Bounds, pins::overlaps};

/// Include the shared toolbar's 8px blur / 3px offset. Hit targets keep their
/// exact allocation; only capture occlusion and preview placement use this.
pub(super) fn paint_bounds(rect: Bounds) -> Bounds {
    Bounds {
        x: rect.x - 16.,
        y: rect.y - 16.,
        width: rect.width + 32.,
        height: rect.height + 32.,
    }
}

/// Include the inline popover's 18px shadow blur / 6px vertical offset.
pub(super) fn popup_paint_bounds(rect: Bounds) -> Bounds {
    Bounds {
        x: rect.x - 28.,
        y: rect.y - 24.,
        width: rect.width + 56.,
        height: rect.height + 56.,
    }
}

/// Only the toolbar has an outer shadow. Hint and preview are plain allocations.
pub(super) fn exclusions(toolbar: Bounds, hint: Bounds, preview: Option<Bounds>) -> Vec<Bounds> {
    let mut areas = vec![paint_bounds(toolbar), hint];
    areas.extend(preview);
    areas
}

pub(super) fn preview(
    region: Bounds,
    output: Bounds,
    size: [f64; 2],
    occupied: &[Bounds],
) -> Option<Bounds> {
    let [width, height] = size;
    if width <= 0. || height <= 0. {
        return None;
    }
    let gap = 12.;
    let fits = |r: Bounds| {
        r.x >= output.x + 1.
            && r.y >= output.y + 1.
            && r.x + r.width <= output.x + output.width - 1.
            && r.y + r.height <= output.y + output.height - 1.
            && occupied.iter().all(|o| !overlaps(r, *o))
    };
    // Preserve the natural size when possible; shrink only to use an external
    // margin. Tiny illegible previews are suppressed instead of put over the ROI.
    for scale in [1., 0.75, 0.5] {
        let (w, h) = (width * scale, height * scale);
        if scale < 1. && (w < 64. || h < 48.) {
            continue;
        }
        let x = region.x.clamp(
            output.x + 1.,
            (output.x + output.width - w - 1.).max(output.x + 1.),
        );
        let y = region.y.clamp(
            output.y + 1.,
            (output.y + output.height - h - 1.).max(output.y + 1.),
        );
        for (x, y) in [
            (region.x + region.width + gap, y),
            (region.x - w - gap, y),
            (x, region.y + region.height + gap),
            (x, region.y - h - gap),
        ] {
            let r = Bounds {
                x,
                y,
                width: w,
                height: h,
            };
            if fits(r) && !overlaps(r, region) {
                return Some(r);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_hint_and_preview_do_not_discard_capture_side_pixels() {
        let region = Bounds {
            x: 70.,
            y: 150.,
            width: 850.,
            height: 470.,
        };
        let toolbar = Bounds {
            x: 536.,
            y: 628.,
            width: 384.,
            height: 32.,
        };
        let hint = Bounds {
            x: 70.,
            y: 118.,
            width: 130.,
            height: 27.,
        };
        let preview = Bounds {
            x: 932.,
            y: 150.,
            width: 184.,
            height: 333.,
        };
        let areas = exclusions(toolbar, hint, Some(preview));
        assert!(overlaps(areas[0], region)); // actual toolbar shadow needs masking
        assert!(!overlaps(areas[1], region));
        assert!(!overlaps(areas[2], region));
        // The shared outline paints wholly outside the ROI, never a side mask.
        assert_eq!(areas.len(), 3);
    }
    #[test]
    fn live_preview_never_overlaps_capture_or_controls() {
        let output = Bounds {
            x: -1280.,
            y: -100.,
            width: 1280.,
            height: 800.,
        };
        for width in [300., 900., 1200., 1280.] {
            for height in [200., 600., 760., 800.] {
                let region = Bounds {
                    width,
                    height,
                    ..output
                };
                let controls = Bounds {
                    x: -500.,
                    y: 660.,
                    width: 490.,
                    height: 32.,
                };
                if let Some(p) = preview(region, output, [184., 300.], &[controls]) {
                    assert!(!overlaps(p, region));
                    assert!(!overlaps(p, controls));
                    assert!(p.x >= output.x && p.y >= output.y);
                    assert!(p.x + p.width <= output.x + output.width);
                    assert!(p.y + p.height <= output.y + output.height);
                }
            }
        }
        assert!(preview(output, output, [184., 300.], &[]).is_none());
    }
}
