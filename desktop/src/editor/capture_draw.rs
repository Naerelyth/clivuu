//! Small retained nodes over the frozen texture. Neither the desktop image
//! nor a magnifier crop is encoded, copied or sent over IPC on pointer motion.
use super::frozen_image::FrozenImage;
use clivuu_core::{
    annotation::Bounds,
    capture::{Selection, outside},
    i18n::t,
};
use gtk::{gdk, graphene, gsk, pango, prelude::*};
#[derive(Clone)]
pub struct CaptureStyle {
    pub border_width: f64,
    pub accent: gdk::RGBA,
    pub mask: gdk::RGBA,
    pub anchor: gdk::RGBA,
    pub anchors: bool,
    pub magnifier_mask: bool,
    pub magnifier_border: bool,
    pub magnifier_anchors: bool,
    pub crosshair: bool,
    pub guides: bool,
    pub tips: bool,
}
impl Default for CaptureStyle {
    fn default() -> Self {
        static MASK: std::sync::LazyLock<gdk::RGBA> = std::sync::LazyLock::new(|| {
            gdk::RGBA::parse(
                clivuu_core::config::schema()["capture-mask-color"]["default"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap()
        });
        Self {
            border_width: 2.,
            accent: gdk::RGBA::parse("#bd93f9").unwrap(),
            mask: *MASK,
            anchor: gdk::RGBA::WHITE,
            anchors: true,
            magnifier_mask: true,
            magnifier_border: true,
            magnifier_anchors: false,
            crosshair: false,
            guides: true,
            tips: true,
        }
    }
}
fn rect(r: Bounds) -> graphene::Rect {
    graphene::Rect::new(r.x as f32, r.y as f32, r.width as f32, r.height as f32)
}
fn fill(snapshot: &gtk::Snapshot, r: Bounds, ink: &gdk::RGBA) {
    if r.width > 0. && r.height > 0. {
        snapshot.append_color(ink, &rect(r));
    }
}
fn outline(s: &gtk::Snapshot, r: Bounds, width: f64, ink: &gdk::RGBA) {
    for b in [
        Bounds {
            x: r.x - width,
            y: r.y - width,
            width: r.width + 2. * width,
            height: width,
        },
        Bounds {
            x: r.x - width,
            y: r.y + r.height,
            width: r.width + 2. * width,
            height: width,
        },
        Bounds {
            x: r.x - width,
            y: r.y,
            width,
            height: r.height,
        },
        Bounds {
            x: r.x + r.width,
            y: r.y,
            width,
            height: r.height,
        },
    ] {
        fill(s, b, ink);
    }
}
fn mask(s: &gtk::Snapshot, r: Bounds, limit: Bounds, ink: &gdk::RGBA) {
    for b in outside(r, limit) {
        fill(s, b, ink);
    }
}
/// Selection handle radius in logical px matching Snipaste (`max(4.5, 1.4 * border_width)`).
pub fn anchor_radius(border_width: f64) -> f64 {
    (border_width.max(0.) * 1.4).max(4.5)
}
/// Anchor centers lie on the outside-border centerline (`0.5 * border_width` outward),
/// matching Snipaste's `d = 0.5 * border_width` offset.
pub fn anchor_centers(r: Bounds, border_width: f64) -> [[f64; 2]; 8] {
    let d = 0.5 * border_width.max(0.);
    let b = Bounds {
        x: r.x - d,
        y: r.y - d,
        width: r.width + 2. * d,
        height: r.height + 2. * d,
    };
    [
        [0., 0.],
        [0.5, 0.],
        [1., 0.],
        [1., 0.5],
        [1., 1.],
        [0.5, 1.],
        [0., 1.],
        [0., 0.5],
    ]
    .map(|[x, y]| [b.x + b.width * x, b.y + b.height * y])
}
/// Resize hit-test margin in logical px so enlarged outward-centered anchors stay easy to grab.
pub fn resize_margin(style: &CaptureStyle, base: f64) -> f64 {
    let reach = if style.anchors {
        0.5 * style.border_width.max(0.) + anchor_radius(style.border_width) + 0.6
    } else {
        style.border_width
    };
    reach.ceil().max(base)
}
type AnchorKey = (u32, u32, [u32; 4], [u32; 4]);
thread_local! {
    static ANCHOR_NODE: std::cell::RefCell<Option<(AnchorKey, gsk::RenderNode)>> =
        const { std::cell::RefCell::new(None) };
}
fn rgba_bits(c: &gdk::RGBA) -> [u32; 4] {
    [
        c.red().to_bits(),
        c.green().to_bits(),
        c.blue().to_bits(),
        c.alpha().to_bits(),
    ]
}
fn anchor_node(radius: f32, scale: f32, style: &CaptureStyle) -> Option<gsk::RenderNode> {
    let key = (
        radius.to_bits(),
        scale.to_bits(),
        rgba_bits(&style.accent),
        rgba_bits(&style.anchor),
    );
    if let Some(found) = ANCHOR_NODE.with(|cache| {
        cache
            .borrow()
            .as_ref()
            .filter(|(k, _)| *k == key)
            .map(|(_, node)| node.clone())
    }) {
        return Some(found);
    }
    let circle = gsk::PathBuilder::new();
    circle.add_circle(&graphene::Point::new(0., 0.), radius);
    let path = circle.to_path();
    let stroke = gsk::Stroke::new(1.2 * scale);
    let snap = gtk::Snapshot::new();
    snap.append_fill(&path, gsk::FillRule::Winding, &style.accent);
    snap.append_stroke(&path, &stroke, &style.anchor);
    let node = snap.to_node()?;
    ANCHOR_NODE.with(|cache| {
        cache.replace(Some((key, node.clone())));
    });
    Some(node)
}
fn anchors(s: &gtk::Snapshot, r: Bounds, style: &CaptureStyle) {
    scaled_anchors(s, r, style.border_width, style, 1.);
}
fn scaled_anchors(
    s: &gtk::Snapshot,
    r: Bounds,
    border_width: f64,
    style: &CaptureStyle,
    scale: f32,
) {
    let radius = anchor_radius(style.border_width) as f32 * scale;
    let Some(node) = anchor_node(radius, scale, style) else {
        return;
    };
    for [cx, cy] in anchor_centers(r, border_width) {
        s.save();
        s.translate(&graphene::Point::new(cx as f32, cy as f32));
        s.append_node(&node);
        s.restore();
    }
}
/// The same selection frame over frozen captures and a live scrolling target.
pub fn frame(s: &gtk::Snapshot, selection: Bounds, style: &CaptureStyle) {
    outline(s, selection, style.border_width, &style.accent);
    if style.anchors {
        anchors(s, selection, style);
    }
}
// Difference composition preserves contrast on every source color, with the
// crossing cell left unchanged (the reference applies inversion twice there).
fn pointer_cross(s: &gtk::Snapshot, selection: &Selection) {
    let Some([x, y]) = selection.pointer else {
        return;
    };
    let x = x.floor();
    let y = y.floor();
    let v = selection.limit;
    for r in [
        Bounds {
            x: v.x,
            y,
            width: x - v.x,
            height: 1.,
        },
        Bounds {
            x: x + 1.,
            y,
            width: v.x + v.width - x - 1.,
            height: 1.,
        },
        Bounds {
            x,
            y: v.y,
            width: 1.,
            height: y - v.y,
        },
        Bounds {
            x,
            y: y + 1.,
            width: 1.,
            height: v.y + v.height - y - 1.,
        },
    ] {
        fill(s, r, &gdk::RGBA::WHITE);
    }
}
fn auxiliary(s: &gtk::Snapshot, selection: &Selection, style: &CaptureStyle, mapped: Bounds) {
    if !style.guides || !selection.selected {
        return;
    }
    let limit = selection.limit;
    let r = selection.rect;
    let sx = mapped.width / limit.width;
    let sy = mapped.height / limit.height;
    let x = |v: f64| (mapped.x + (v - limit.x) * sx) as f32;
    let y = |v: f64| (mapped.y + (v - limit.y) * sy) as f32;
    let b = gsk::PathBuilder::new();
    for v in [r.x, r.x + r.width / 2., r.x + r.width] {
        b.move_to(x(v), mapped.y as f32);
        b.line_to(x(v), (mapped.y + mapped.height) as f32);
    }
    for v in [r.y, r.y + r.height / 2., r.y + r.height] {
        b.move_to(mapped.x as f32, y(v));
        b.line_to((mapped.x + mapped.width) as f32, y(v));
    }
    let scale = sx.min(sy) as f32;
    let stroke = gsk::Stroke::new(scale);
    stroke.set_dash(&[9. * scale, 5. * scale]);
    s.append_stroke(&b.to_path(), &stroke, &style.accent);
}
/// Frozen desktop texture (plus pre-selection difference crosshair when active).
/// Selection dimming and frame render in `overlay` above the annotation layer so
/// annotations compose in a single pass without clipping or color-matrix replay.
pub fn backdrop_base(
    s: &gtk::Snapshot,
    image: &FrozenImage,
    selection: &Selection,
    style: &CaptureStyle,
    viewport: Bounds,
) {
    s.save();
    s.push_clip(&graphene::Rect::new(
        0.,
        0.,
        viewport.width as f32,
        viewport.height as f32,
    ));
    s.translate(&graphene::Point::new(
        -viewport.x as f32,
        -viewport.y as f32,
    ));
    let crosshair = style.crosshair && selection.pointer.is_some();
    if crosshair {
        s.push_blend(gsk::BlendMode::Difference);
    }
    mask(s, selection.limit, viewport, &style.mask);
    s.append_texture(&image.texture, &rect(selection.limit));
    if crosshair {
        mask(s, selection.rect, selection.limit, &style.mask);
        s.pop();
        pointer_cross(s, selection);
        s.pop();
    }
    s.pop();
    s.restore();
}
/// Exterior dim mask, alignment guides and selection border composited above
/// annotations and below active editor decorations.
pub fn overlay(
    s: &gtk::Snapshot,
    selection: &Selection,
    style: &CaptureStyle,
    origin: [f64; 2],
    viewport: Bounds,
) {
    s.save();
    s.translate(&graphene::Point::new(-origin[0] as f32, -origin[1] as f32));
    s.push_clip(&rect(viewport));
    let crosshair = style.crosshair && selection.pointer.is_some();
    if !crosshair {
        mask(s, selection.rect, selection.limit, &style.mask);
    }
    auxiliary(s, selection, style, selection.limit);
    if selection.selected {
        frame(s, selection.rect, style);
    } else {
        outline(s, selection.rect, style.border_width, &style.accent);
    }
    s.pop();
    s.restore();
}
pub fn backdrop(
    s: &gtk::Snapshot,
    image: &FrozenImage,
    selection: &Selection,
    style: &CaptureStyle,
    viewport: Bounds,
) {
    backdrop_base(s, image, selection, style, viewport);
    overlay(s, selection, style, [viewport.x, viewport.y], viewport);
}
fn label(s: &gtk::Snapshot, context: &pango::Context, text: &str, y: f32, font: &str) {
    let layout = pango::Layout::new(context);
    layout.set_font_description(Some(&pango::FontDescription::from_string(font)));
    layout.set_text(text);
    let (width, _) = layout.pixel_size();
    s.save();
    s.translate(&graphene::Point::new((184 - width) as f32 / 2., y));
    s.append_layout(&layout, &gdk::RGBA::WHITE);
    s.restore();
}
pub fn lens(
    s: &gtk::Snapshot,
    image: &FrozenImage,
    selection: &Selection,
    style: &CaptureStyle,
    context: &pango::Context,
    hex: bool,
) {
    let Some([px, py]) = selection.lens_pixel(image.dimensions()) else {
        return;
    };
    fill(
        s,
        Bounds {
            x: 0.,
            y: 0.,
            width: 184.,
            height: 236.,
        },
        &gdk::RGBA::new(0.035, 0.04, 0.05, 0.97),
    );
    // Work directly in final logical pixels. A scale around this composite
    // lets GSK rasterize the clipped group at 17×13 then linearly enlarge it,
    // which blurs the source even when its texture node requests Nearest.
    let magnified = |r: Bounds| Bounds {
        x: 7. + r.x * 10.,
        y: 7. + r.y * 10.,
        width: r.width * 10.,
        height: r.height * 10.,
    };
    let canvas = magnified(Bounds {
        x: 0.,
        y: 0.,
        width: 17.,
        height: 13.,
    });
    s.push_clip(&rect(canvas));
    let image_rect = graphene::Rect::new(
        7. + (8. - px as f32) * 10.,
        7. + (6. - py as f32) * 10.,
        image.pixels.width() as f32 * 10.,
        image.pixels.height() as f32 * 10.,
    );
    s.append_scaled_texture(&image.texture, gsk::ScalingFilter::Nearest, &image_rect);
    let pixel_rect = magnified(selection.lens_rect(image.dimensions(), [px, py]));
    if style.magnifier_mask {
        mask(s, pixel_rect, canvas, &style.mask);
    }
    let border_width =
        10. * style.border_width * f64::from(image.pixels.width()) / selection.limit.width;
    if style.magnifier_border {
        outline(s, pixel_rect, border_width, &style.accent);
    }
    if selection.selected && style.magnifier_anchors {
        scaled_anchors(s, pixel_rect, border_width, style, 10.);
    }
    let sx = f64::from(image.pixels.width()) / selection.limit.width;
    let sy = f64::from(image.pixels.height()) / selection.limit.height;
    auxiliary(
        s,
        selection,
        style,
        Bounds {
            x: 7. + (8. - f64::from(px)) * 10.,
            y: 7. + (6. - f64::from(py)) * 10.,
            width: selection.limit.width * sx * 10.,
            height: selection.limit.height * sy * 10.,
        },
    );
    // Always restore the sampled cell after mask, frame and anchors. The
    // centre swatch/crosshair identifies that exact included source pixel.
    s.push_clip(&graphene::Rect::new(87., 67., 10., 10.));
    s.append_scaled_texture(&image.texture, gsk::ScalingFilter::Nearest, &image_rect);
    s.pop();
    let mut ink = style.accent;
    ink.set_alpha(0.35);
    for r in [
        Bounds {
            x: 0.,
            y: 6.,
            width: 8.,
            height: 1.,
        },
        Bounds {
            x: 9.,
            y: 6.,
            width: 8.,
            height: 1.,
        },
        Bounds {
            x: 8.,
            y: 0.,
            width: 1.,
            height: 6.,
        },
        Bounds {
            x: 8.,
            y: 7.,
            width: 1.,
            height: 6.,
        },
    ] {
        fill(s, magnified(r), &ink);
    }
    s.pop();
    for (r, ink) in [
        (
            Bounds {
                x: 6.5,
                y: 6.5,
                width: 171.,
                height: 131.,
            },
            gdk::RGBA::WHITE,
        ),
        (
            Bounds {
                x: 86.5,
                y: 66.5,
                width: 11.,
                height: 11.,
            },
            gdk::RGBA::BLACK,
        ),
        (
            Bounds {
                x: 87.5,
                y: 67.5,
                width: 9.,
                height: 9.,
            },
            gdk::RGBA::WHITE,
        ),
    ] {
        let path = gsk::PathBuilder::new();
        path.add_rect(&rect(r));
        s.append_stroke(&path.to_path(), &gsk::Stroke::new(1.), &ink);
    }
    let x =
        selection.limit.x + f64::from(px) * selection.limit.width / f64::from(image.pixels.width());
    let y = selection.limit.y
        + f64::from(py) * selection.limit.height / f64::from(image.pixels.height());
    let coordinate = |v: f64| ((v * 100.).round() / 100.).to_string();
    label(
        s,
        context,
        &format!("({}, {})", coordinate(x), coordinate(y)),
        146.,
        "Monospace 10",
    );
    let p = image.pixels.get_pixel(px, py);
    let color = gdk::RGBA::new(
        f32::from(p[0]) / 255.,
        f32::from(p[1]) / 255.,
        f32::from(p[2]) / 255.,
        1.,
    );
    fill(
        s,
        Bounds {
            x: 13.,
            y: 171.,
            width: 13.,
            height: 13.,
        },
        &color,
    );
    let path = gsk::PathBuilder::new();
    path.add_rect(&graphene::Rect::new(13., 171., 13., 13.));
    s.append_stroke(&path.to_path(), &gsk::Stroke::new(1.), &gdk::RGBA::WHITE);
    label(
        s,
        context,
        &if hex {
            format!("#{:02X}{:02X}{:02X}", p[0], p[1], p[2])
        } else {
            format!("{}, {}, {}", p[0], p[1], p[2])
        },
        170.,
        "Monospace 10",
    );
    label(s, context, &t("C: copy color"), 195., "Sans 9");
    label(s, context, &t("Shift: RGB / HEX"), 214., "Sans 9");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_radius_and_centers_match_snipaste_border_offset() {
        assert!((anchor_radius(1.) - 4.5).abs() < 1e-6);
        assert!((anchor_radius(2.) - 4.5).abs() < 1e-6);
        assert!((anchor_radius(3.) - 4.5).abs() < 1e-6);
        assert!((anchor_radius(4.) - 5.6).abs() < 1e-6);
        assert!((anchor_radius(8.) - 11.2).abs() < 1e-6);

        let rect = Bounds {
            x: 100.,
            y: 200.,
            width: 300.,
            height: 120.,
        };
        assert_eq!(
            anchor_centers(rect, 2.),
            [
                [99., 199.],
                [250., 199.],
                [401., 199.],
                [401., 260.],
                [401., 321.],
                [250., 321.],
                [99., 321.],
                [99., 260.],
            ]
        );
        assert_eq!(
            anchor_centers(rect, 8.),
            [
                [96., 196.],
                [250., 196.],
                [404., 196.],
                [404., 260.],
                [404., 324.],
                [250., 324.],
                [96., 324.],
                [96., 260.],
            ]
        );

        let style = CaptureStyle {
            border_width: 8.,
            ..Default::default()
        };
        assert_eq!(resize_margin(&style, 7.), 16.);
        let hidden = CaptureStyle {
            anchors: false,
            border_width: 8.,
            ..Default::default()
        };
        assert_eq!(resize_margin(&hidden, 7.), 8.);
    }
}
