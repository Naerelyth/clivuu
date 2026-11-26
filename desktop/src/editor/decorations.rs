//! Editor feedback only. None of these nodes are included in image export.
use clivuu_core::annotation::{Fill, HandleKind, Kind, Mark, Point};
use gtk::{gdk, graphene, gsk, prelude::*};
use std::f64::consts::{FRAC_PI_2, PI};

pub struct DecorationStyle {
    pub accent: gdk::RGBA,
    pub ink: gdk::RGBA,
    pub selected_ink: gdk::RGBA,
}
impl Default for DecorationStyle {
    fn default() -> Self {
        Self {
            accent: gdk::RGBA::parse("#bd93f9").unwrap(),
            ink: gdk::RGBA::WHITE,
            selected_ink: gdk::RGBA::parse("#282a36").unwrap(),
        }
    }
}
fn polyline(points: &[Point], close: bool) -> gsk::Path {
    let b = gsk::PathBuilder::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            b.move_to(p[0] as f32, p[1] as f32);
        } else {
            b.line_to(p[0] as f32, p[1] as f32);
        }
    }
    if close {
        b.close();
    }
    b.to_path()
}
fn stroke(s: &gtk::Snapshot, path: &gsk::Path, width: f64, ink: &gdk::RGBA, dash: &[f32]) {
    let pen = gsk::Stroke::new(width as f32);
    pen.set_dash(dash);
    s.append_stroke(path, &pen, ink);
}
fn frame(s: &gtk::Snapshot, m: &Mark, scale: f64, dash: f32) {
    let b = m.control_bounds(1., scale);
    let corners = [
        [b.x, b.y],
        [b.x + b.width, b.y],
        [b.x + b.width, b.y + b.height],
        [b.x, b.y + b.height],
    ]
    .map(|p| m.map_point(p, 1., false));
    let path = polyline(&corners, true);
    stroke(s, &path, 1. / scale, &gdk::RGBA::BLACK, &[]);
    stroke(
        s,
        &path,
        1. / scale,
        &gdk::RGBA::WHITE,
        &[dash / scale as f32, dash / scale as f32],
    );
}
fn rotation(s: &gtk::Snapshot, p: Point, scale: f64, style: &DecorationStyle) {
    s.save();
    s.translate(&graphene::Point::new(p[0] as f32, p[1] as f32));
    s.scale((1. / scale) as f32, (1. / scale) as f32);
    let b = gsk::PathBuilder::new();
    b.add_circle(&graphene::Point::new(0., 0.), 6.);
    s.append_fill(&b.to_path(), gsk::FillRule::Winding, &style.accent);
    let b = gsk::PathBuilder::new();
    b.add_circle(&graphene::Point::new(0., 0.), 5.5);
    stroke(s, &b.to_path(), 0.9, &gdk::RGBA::WHITE, &[]);
    let points: Vec<_> = (0..=64)
        .map(|i| {
            let f = i as f64 / 64.;
            let a = -FRAC_PI_2 + f * PI * 3.2;
            let r = 0.25 + f * 3.3;
            [a.cos() * r, a.sin() * r]
        })
        .collect();
    let pen = gsk::Stroke::new(1.1);
    pen.set_line_cap(gsk::LineCap::Round);
    pen.set_line_join(gsk::LineJoin::Round);
    s.append_stroke(&polyline(&points, false), &pen, &gdk::RGBA::WHITE);
    s.restore();
}
pub fn hover(s: &gtk::Snapshot, m: &Mark, scale: f64) {
    frame(s, m, scale.max(0.01), 3.);
}
pub fn draft(s: &gtk::Snapshot, m: &Mark, scale: f64) {
    if matches!(m.kind, Kind::Eraser | Kind::Mosaic)
        && m.style.fill == Fill::Rect
        && m.points.len() > 1
    {
        frame(
            s,
            m,
            scale.max(0.01),
            if m.kind == Kind::Mosaic { 4. } else { 3. },
        );
    }
}
pub fn selected(
    s: &gtk::Snapshot,
    m: &Mark,
    pointer: Option<Point>,
    scale: f64,
    style: &DecorationStyle,
) {
    let scale = scale.max(0.01);
    if m.kind == Kind::Text {
        frame(s, m, scale, 3.);
        if m.preedit_frame {
            return;
        }
    }
    if matches!(m.kind, Kind::Mosaic | Kind::Eraser) && !m.freehand() {
        let b = m.bounds(1.);
        let points = [
            [b.x, b.y],
            [b.x + b.width, b.y],
            [b.x + b.width, b.y + b.height],
            [b.x, b.y + b.height],
        ]
        .map(|p| m.map_point(p, 1., false));
        stroke(
            s,
            &polyline(&points, true),
            1. / scale,
            &style.ink,
            &[4. / scale as f32, 4. / scale as f32],
        );
    }
    for handle in m.visible_handles(pointer, 1., scale) {
        if handle.kind == HandleKind::Rotate {
            rotation(s, handle.point, scale, style);
            continue;
        }
        s.save();
        s.translate(&graphene::Point::new(
            handle.point[0] as f32,
            handle.point[1] as f32,
        ));
        s.scale((1. / scale) as f32, (1. / scale) as f32);
        if m.kind == Kind::Sequence && handle.kind != HandleKind::Endpoint {
            let b = gsk::PathBuilder::new();
            b.add_rounded_rect(&gsk::RoundedRect::from_rect(
                graphene::Rect::new(-6., -6., 12., 12.),
                2.,
            ));
            s.append_fill(&b.to_path(), gsk::FillRule::Winding, &style.accent);
            let points: &[Point] = match handle.kind {
                HandleKind::Increase => &[[-3., 0.], [3., 0.]],
                HandleKind::Decrease => &[[-3., 0.], [3., 0.]],
                HandleKind::Delete => &[[-2.5, -2.5], [2.5, 2.5]],
                _ => &[
                    [-3., 2.5],
                    [2.5, -2.5],
                    [-0.5, -2.5],
                    [2.5, -2.5],
                    [2.5, 0.5],
                ],
            };
            stroke(s, &polyline(points, false), 1.2, &style.selected_ink, &[]);
            if handle.kind == HandleKind::Increase {
                stroke(
                    s,
                    &polyline(&[[0., -3.], [0., 3.]], false),
                    1.2,
                    &style.selected_ink,
                    &[],
                );
            }
            if handle.kind == HandleKind::Delete {
                stroke(
                    s,
                    &polyline(&[[-2.5, 2.5], [2.5, -2.5]], false),
                    1.2,
                    &style.selected_ink,
                    &[],
                );
            }
        } else if m.kind == Kind::Text {
            s.rotate((m.transform().angle * 180. / PI) as f32);
            s.append_color(&style.accent, &graphene::Rect::new(-6., -6., 12., 12.));
            let points: &[Point] = if handle.kind == HandleKind::Delete {
                &[[-2.5, -2.5], [2.5, 2.5]]
            } else {
                &[[-3., 0.], [-3., -3.], [0., -3.]]
            };
            if handle.index == Some(6) {
                s.rotate(90.);
            }
            stroke(s, &polyline(points, false), 1., &style.selected_ink, &[]);
            if handle.kind == HandleKind::Delete {
                stroke(
                    s,
                    &polyline(&[[2.5, -2.5], [-2.5, 2.5]], false),
                    1.,
                    &style.selected_ink,
                    &[],
                );
            } else {
                stroke(
                    s,
                    &polyline(&[[-3., -3.], [3., 3.]], false),
                    1.,
                    &style.selected_ink,
                    &[],
                );
                stroke(
                    s,
                    &polyline(&[[0., 3.], [3., 3.], [3., 0.]], false),
                    1.,
                    &style.selected_ink,
                    &[],
                );
            }
        } else {
            let b = gsk::PathBuilder::new();
            b.add_rect(&graphene::Rect::new(-3., -3., 6., 6.));
            stroke(s, &b.to_path(), 1., &style.ink, &[]);
        }
        s.restore();
    }
}

pub fn crop(
    s: &gtk::Snapshot,
    r: clivuu_core::annotation::Bounds,
    size: Point,
    style: &DecorationStyle,
) {
    let limit = clivuu_core::annotation::Bounds {
        x: 0.,
        y: 0.,
        width: size[0],
        height: size[1],
    };
    let mask = gdk::RGBA::new(0., 0., 0., 0.6);
    for b in clivuu_core::capture::outside(r, limit) {
        if b.width > 0. && b.height > 0. {
            s.append_color(
                &mask,
                &graphene::Rect::new(b.x as f32, b.y as f32, b.width as f32, b.height as f32),
            );
        }
    }
    let crop_style = super::capture_draw::CaptureStyle {
        border_width: 1.,
        accent: style.accent,
        anchor: gdk::RGBA::WHITE,
        anchors: true,
        ..super::capture_draw::CaptureStyle::default()
    };
    super::capture_draw::frame(s, r, &crop_style);
}
