//! Shared toolbar geometry, in desktop logical coordinates. Surface origins
//! are subtracted only when placing GTK children, never in these decisions.
use clivuu_core::annotation::Bounds;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Placement {
    Below,
    Above,
    InsideTop,
    InsideBottom,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Panels {
    pub toolbar: Bounds,
    pub hint: Bounds,
    pub placement: Placement,
}
fn clamp(value: f64, low: f64, high: f64) -> f64 {
    value.max(low).min(high.max(low))
}
fn bounds_intersect(a: Bounds, b: Bounds) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}
pub fn selection(r: Bounds, viewport: Bounds, toolbar: Size, hint: Size) -> Panels {
    selection_with_subbar(r, viewport, toolbar, hint, None)
}
pub fn selection_with_subbar(
    r: Bounds,
    viewport: Bounds,
    toolbar: Size,
    hint: Size,
    subbar: Option<Size>,
) -> Panels {
    let (edge, gap, hint_gap, sub_gap, inside) = (1., 8., 5., 4., 8.);
    let right = viewport.x + viewport.width;
    let bottom = viewport.y + viewport.height;
    let max_width = (viewport.width - 2. * edge).max(1.);
    let max_height = (viewport.height - 2. * edge).max(1.);
    let width = toolbar.width.min(max_width);
    let height = toolbar.height.min(max_height);
    let sub_reserve = subbar.map_or(0., |s| sub_gap + s.height.min(max_height));
    let mut hint = Bounds {
        x: 0.,
        y: 0.,
        width: hint.width.min(max_width),
        height: hint.height.min(max_height),
    };
    hint.x = clamp(r.x.round(), viewport.x + edge, right - hint.width - edge);
    let above_hint = r.y - hint_gap - hint.height;
    hint.y = if above_hint >= viewport.y + edge {
        above_hint.round()
    } else {
        clamp(
            (r.y + hint_gap).round(),
            viewport.y + edge,
            bottom - hint.height - edge,
        )
    };
    let x = clamp(
        (r.x + r.width - width).round(),
        viewport.x + edge,
        right - width - edge,
    );
    let below = (r.y + r.height + gap).round();
    let above = (r.y.min(hint.y) - gap - height).round();
    // Reserve the active subbar height when choosing below vs above so an open
    // subbar never overflows the edge and flips back onto the selection.
    let (y, placement) = if below + height + sub_reserve <= bottom - edge {
        (below, Placement::Below)
    } else if above - sub_reserve >= viewport.y + edge {
        (above, Placement::Above)
    } else {
        // Equal remaining space, including full screen, prefers the bottom.
        let top_space = (r.y - viewport.y).max(0.);
        let bottom_space = (bottom - r.y - r.height).max(0.);
        let (y, placement) = if top_space > bottom_space {
            ((r.y + inside).round(), Placement::InsideTop)
        } else {
            (
                (r.y + r.height - height - inside).round(),
                Placement::InsideBottom,
            )
        };
        (
            clamp(y, viewport.y + edge, bottom - height - edge),
            placement,
        )
    };
    let toolbar = Bounds {
        x,
        y,
        width,
        height,
    };
    let placed_sub = subbar.map(|s| annotation_for(placement, toolbar, viewport, s));
    let mut band_top = f64::INFINITY;
    let mut band_bottom = f64::NEG_INFINITY;
    let mut overlaps_hint = false;
    for bar in std::iter::once(toolbar).chain(placed_sub) {
        band_top = band_top.min(bar.y);
        band_bottom = band_bottom.max(bar.y + bar.height);
        overlaps_hint |= bounds_intersect(bar, hint);
    }
    if overlaps_hint {
        let after = (band_bottom + hint_gap).round();
        hint.y = if after + hint.height <= bottom - edge {
            after
        } else {
            clamp(
                (band_top - hint.height - hint_gap).round(),
                viewport.y + edge,
                bottom - hint.height - edge,
            )
        };
    }
    Panels {
        toolbar,
        hint,
        placement,
    }
}
pub fn annotation(toolbar: Bounds, viewport: Bounds, size: Size) -> Bounds {
    annotation_for(Placement::Below, toolbar, viewport, size)
}
pub fn annotation_for(
    placement: Placement,
    toolbar: Bounds,
    viewport: Bounds,
    size: Size,
) -> Bounds {
    let gap = 4.;
    let width = size.width.min((viewport.width - 2.).max(1.));
    let height = size.height.min((viewport.height - 2.).max(1.));
    let below = (toolbar.y + toolbar.height + gap).round();
    let above = (toolbar.y - gap - height).round();
    let bottom = viewport.y + viewport.height - 1.;
    let fits_below = below + height <= bottom;
    let fits_above = above >= viewport.y + 1.;
    let min_overflow = || {
        if below + height - bottom < viewport.y + 1. - above {
            below
        } else {
            above
        }
    };
    let y = if placement == Placement::Above {
        if fits_above {
            above
        } else if fits_below {
            below
        } else {
            min_overflow()
        }
    } else if fits_below {
        below
    } else if fits_above {
        above
    } else {
        min_overflow()
    };
    Bounds {
        x: clamp(
            toolbar.x.round(),
            viewport.x + 1.,
            viewport.x + viewport.width - width - 1.,
        ),
        y: clamp(y, viewport.y + 1., bottom - height),
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_and_annotation_round_panel_origins_to_integer_logical_pixels() {
        let viewport = Bounds {
            x: 0.,
            y: 0.,
            width: 1920.,
            height: 1080.,
        };
        let panels = selection(
            Bounds {
                x: 100.,
                y: 120.5,
                width: 300.5,
                height: 200.5,
            },
            viewport,
            Size {
                width: 320.,
                height: 32.,
            },
            Size {
                width: 110.,
                height: 24.,
            },
        );
        assert_eq!(panels.toolbar.x.fract(), 0.);
        assert_eq!(panels.toolbar.y.fract(), 0.);
        assert_eq!(panels.hint.x.fract(), 0.);
        assert_eq!(panels.hint.y.fract(), 0.);
        let subbar = annotation(
            Bounds {
                x: 80.5,
                y: 329.5,
                width: 320.,
                height: 32.,
            },
            viewport,
            Size {
                width: 240.,
                height: 32.,
            },
        );
        assert_eq!(subbar.x.fract(), 0.);
        assert_eq!(subbar.y.fract(), 0.);
    }
    #[test]
    fn selection_with_subbar_moves_both_bars_above_when_bottom_lacks_subbar_room() {
        let viewport = Bounds {
            x: -200.,
            y: -100.,
            width: 1920.,
            height: 1080.,
        };
        // Selection ends 55px above the viewport bottom: enough for the 32px
        // main toolbar below, but not enough for toolbar + 4px gap + 32px subbar.
        let rect = Bounds {
            x: 100.,
            y: 320.,
            width: 800.,
            height: 605.,
        };
        let toolbar_size = Size {
            width: 520.,
            height: 32.,
        };
        let hint_size = Size {
            width: 110.,
            height: 26.,
        };
        let subbar_size = Size {
            width: 340.,
            height: 32.,
        };
        let main_only = selection(rect, viewport, toolbar_size, hint_size);
        assert_eq!(main_only.placement, Placement::Below);
        let panels =
            selection_with_subbar(rect, viewport, toolbar_size, hint_size, Some(subbar_size));
        assert_eq!(panels.placement, Placement::Above);
        assert!(panels.toolbar.y + panels.toolbar.height <= panels.hint.y);
        let sub = annotation_for(panels.placement, panels.toolbar, viewport, subbar_size);
        assert!(sub.y + sub.height < panels.toolbar.y);
        assert!(sub.y >= viewport.y + 1.);
        assert!(!bounds_intersect(sub, rect));
        assert!(!bounds_intersect(sub, panels.hint));
    }
    #[test]
    fn selection_with_subbar_keeps_inside_fallback_within_tight_viewport() {
        let viewport = Bounds {
            x: 0.,
            y: 0.,
            width: 1280.,
            height: 800.,
        };
        let rect = Bounds {
            x: 100.,
            y: 45.,
            width: 600.,
            height: 705.,
        };
        let subbar_size = Size {
            width: 300.,
            height: 32.,
        };
        let panels = selection_with_subbar(
            rect,
            viewport,
            Size {
                width: 480.,
                height: 32.,
            },
            Size {
                width: 100.,
                height: 26.,
            },
            Some(subbar_size),
        );
        assert_eq!(panels.placement, Placement::InsideBottom);
        let sub = annotation_for(panels.placement, panels.toolbar, viewport, subbar_size);
        assert!(sub.y >= viewport.y + 1.);
        assert!(sub.y + sub.height <= viewport.y + viewport.height - 1.);
        assert!(!bounds_intersect(sub, panels.toolbar));
    }
}
