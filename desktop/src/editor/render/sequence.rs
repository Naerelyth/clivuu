//! Retained vector badge and fitted Pango glyphs, used unchanged by preview/export.
use super::*;
use clivuu_core::annotation::sequence::Shape;

pub(super) fn draw(snapshot: &gtk::Snapshot, mark: &Mark, context: &pango::Context) {
    let center = mark.points[0];
    let t = mark.transform();
    let size = mark.style.width;
    let radius = mark.sequence_radius(1.);
    let ink = color(&mark.style.color, 1.);
    let contrast = if ink.red() * 0.2126 + ink.green() * 0.7152 + ink.blue() * 0.0722 <= 0.76 {
        gdk::RGBA::WHITE
    } else {
        gdk::RGBA::BLACK
    };
    let shape = mark.style.sequence.shape;
    let foreground = match shape {
        Shape::OutlineCircle | Shape::Text => ink,
        Shape::BorderedCircle => mark
            .style
            .sequence
            .text_color
            .as_deref()
            .and_then(|c| gdk::RGBA::parse(c).ok())
            .unwrap_or(contrast),
        _ => contrast,
    };
    snapshot.save();
    snapshot.translate(&graphene::Point::new(
        (center[0] + t.x) as f32,
        (center[1] + t.y) as f32,
    ));
    snapshot.rotate((t.angle * 180. / PI) as f32);
    snapshot.scale(t.sx as f32, t.sy as f32);
    snapshot.translate(&graphene::Point::new(-center[0] as f32, -center[1] as f32));
    snapshot.push_opacity(mark.style.alpha as f64 / 255.);
    let leader = mark.sequence_leader(1.);
    let callout = !matches!(shape, Shape::RoundedSquare | Shape::Text)
        && leader
            .first()
            .is_some_and(|p| p.stroke.is_none() && p.points.len() == 3);
    if !callout {
        contours(snapshot, leader, &ink);
    }
    let path = gsk::PathBuilder::new();
    if shape == Shape::RoundedSquare {
        path.add_rounded_rect(&gsk::RoundedRect::from_rect(
            graphene::Rect::new(
                (center[0] - radius) as f32,
                (center[1] - radius) as f32,
                (radius * 2.) as f32,
                (radius * 2.) as f32,
            ),
            (radius * 0.16) as f32,
        ));
    } else if callout {
        let end = *mark.points.last().unwrap();
        let dx = end[0] - center[0];
        let dy = end[1] - center[1];
        let length = dx.hypot(dy);
        let ratio = size / length;
        let x = (size * ratio
            + ((1. + ratio * ratio) * radius * radius - size * size)
                .max(0.)
                .sqrt())
            / (1. + ratio * ratio);
        let y = size * (1. - x / length);
        let map = |x: f64, y: f64| {
            [
                (center[0] + x * dx / length - y * dy / length) as f32,
                (center[1] + x * dy / length + y * dx / length) as f32,
            ]
        };
        let a = map(x, y);
        let b = map(x, -y);
        path.move_to(a[0], a[1]);
        path.svg_arc_to(radius as f32, radius as f32, 0., true, true, b[0], b[1]);
        path.line_to(end[0] as f32, end[1] as f32);
        path.close();
    } else {
        path.add_circle(
            &graphene::Point::new(center[0] as f32, center[1] as f32),
            radius as f32,
        );
    }
    let path = path.to_path();
    if !matches!(shape, Shape::OutlineCircle | Shape::Text) {
        snapshot.append_fill(&path, gsk::FillRule::Winding, &ink);
    }
    if matches!(shape, Shape::BorderedCircle | Shape::OutlineCircle) {
        snapshot.append_stroke(
            &path,
            &stroke(size * 0.2, &[]),
            if shape == Shape::OutlineCircle {
                &ink
            } else {
                &foreground
            },
        );
    }
    let layout = pango::Layout::new(context);
    layout.set_text(&mark.style.sequence.format.label(mark.style.sequence.index));
    let mut font = pango::FontDescription::new();
    font.set_family(&mark.style.font_family);
    font.set_weight(pango::Weight::Normal);
    // Fit the glyph box, so multi-digit, Roman and Chinese labels stay inside.
    let (mut lo, mut hi) = (1., (size * 4.).max(1.));
    for _ in 0..16 {
        let mid = (lo + hi) / 2.;
        font.set_absolute_size(mid * pango::SCALE as f64);
        layout.set_font_description(Some(&font));
        let (_, bounds) = layout.pixel_extents();
        if f64::from(bounds.width().max(bounds.height())) <= size * 2. {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    font.set_absolute_size(lo * pango::SCALE as f64);
    layout.set_font_description(Some(&font));
    let (b, _) = layout.pixel_extents();
    snapshot.translate(&graphene::Point::new(
        (center[0] - f64::from(b.x()) - f64::from(b.width()) / 2.) as f32,
        (center[1] - f64::from(b.y()) - f64::from(b.height()) / 2.) as f32,
    ));
    snapshot.append_layout(&layout, &foreground);
    snapshot.pop();
    snapshot.restore();
}
