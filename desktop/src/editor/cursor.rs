//! Local vector cursor feedback, shared by capture and pin scenes. It follows
//! the document mapping without platform cursor-size limits or per-motion textures.
use super::EditorController;
use clivuu_core::annotation::{Kind, Point};
use gtk::{gdk, graphene, gsk, prelude::*};

/// One logical-pixel footprint for both the options strip and drawing feedback.
/// Width stays in image pixels; the display mapping is applied exactly once.
pub fn stroke_diameter(width: f64, image_scale: f64) -> f64 {
    (width * image_scale).max(1.)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorStyle {
    Ink,
    Mosaic,
    Eraser,
}
pub struct ToolCursor {
    pub radius: f64,
    pub color: gdk::RGBA,
    pub crosshair: bool,
    pub style: CursorStyle,
}
impl ToolCursor {
    pub fn for_editor(editor: &EditorController, point: Point) -> Option<Self> {
        // Actual move/resize/rotation targets take precedence over drawing.
        if editor.cursor(point).is_some() {
            return None;
        }
        let kind = editor.tool().kind()?;
        let mark = editor
            .drawing()
            .or_else(|| editor.session().current().filter(|m| m.kind == kind))
            .unwrap_or_else(|| editor.session().style(kind));
        if kind != Kind::Arrow && !mark.freehand() {
            return None;
        }
        let scale = editor.mapping().screen_scale();
        let color = if matches!(kind, Kind::Pen | Kind::Highlight | Kind::Arrow) {
            gdk::RGBA::parse(&mark.style.color).unwrap_or(gdk::RGBA::RED)
        } else {
            gdk::RGBA::WHITE
        };
        Some(Self {
            radius: stroke_diameter(
                editor
                    .drawing()
                    .map_or_else(|| editor.session().input_width(kind), |m| m.style.width),
                scale,
            ) / (2. * scale),
            color,
            crosshair: kind == Kind::Arrow,
            style: match kind {
                Kind::Mosaic => CursorStyle::Mosaic,
                Kind::Eraser => CursorStyle::Eraser,
                _ => CursorStyle::Ink,
            },
        })
    }
    /// Called only from the view's decoration layer; never from image export.
    pub fn snapshot(&self, s: &gtk::Snapshot, point: Point, scale: f64) {
        let center = graphene::Point::new(point[0] as f32, point[1] as f32);
        let circle = |radius: f64| {
            let b = gsk::PathBuilder::new();
            b.add_circle(&center, radius.max(0.) as f32);
            b.to_path()
        };
        match self.style {
            CursorStyle::Ink => {
                s.append_fill(&circle(self.radius), gsk::FillRule::Winding, &self.color)
            }
            CursorStyle::Mosaic => {
                // Reference: translucent white coverage, opaque #404040 rim.
                // Its 1.4px rim stays constant while the brush diameter changes.
                let rim = (1.4 / scale).min(self.radius);
                let path = circle((self.radius - 0.85 / scale).max(self.radius / 2.));
                s.append_fill(
                    &path,
                    gsk::FillRule::Winding,
                    &gdk::RGBA::new(1., 1., 1., 160. / 255.),
                );
                s.append_stroke(
                    &path,
                    &gsk::Stroke::new(rim as f32),
                    &gdk::RGBA::new(64. / 255., 64. / 255., 64. / 255., 1.),
                );
            }
            CursorStyle::Eraser => {
                // Transparent center; a 3px dark ring under a 1.5px white ring.
                let dark = (3. / scale).min(self.radius);
                let light = dark / 2.;
                let path = circle(self.radius - dark / 2.);
                // Source composition in the reference keeps both rings at
                // 192/255 alpha; overlapping translucent strokes would darken it.
                s.push_opacity(192. / 255.);
                s.append_stroke(&path, &gsk::Stroke::new(dark as f32), &gdk::RGBA::BLACK);
                s.append_stroke(&path, &gsk::Stroke::new(light as f32), &gdk::RGBA::WHITE);
                s.pop();
            }
        }
        if self.crosshair {
            let b = gsk::PathBuilder::new();
            for [dx, dy] in [[1., 0.], [-1., 0.], [0., 1.], [0., -1.]] {
                let near = self.radius + 3. / scale;
                let far = near + 4. / scale;
                b.move_to((point[0] + dx * near) as f32, (point[1] + dy * near) as f32);
                b.line_to((point[0] + dx * far) as f32, (point[1] + dy * far) as f32);
            }
            let path = b.to_path();
            s.append_stroke(&path, &gsk::Stroke::new((1. / scale) as f32), &self.color);
        }
    }
}
