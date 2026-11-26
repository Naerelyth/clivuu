use clivuu_core::annotation::{Bounds, EditorSession, Fill, Kind, Mark, Point, StylePatch};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Select,
    Crop,
    Rect,
    Ellipse,
    Arrow,
    Pen,
    Highlight,
    Sequence,
    Mosaic,
    Text,
    Eraser,
}
impl Tool {
    pub fn kind(self) -> Option<Kind> {
        match self {
            Self::Select | Self::Crop => None,
            Self::Rect => Some(Kind::Rect),
            Self::Ellipse => Some(Kind::Ellipse),
            Self::Arrow => Some(Kind::Arrow),
            Self::Pen => Some(Kind::Pen),
            Self::Highlight => Some(Kind::Highlight),
            Self::Sequence => Some(Kind::Sequence),
            Self::Mosaic => Some(Kind::Mosaic),
            Self::Text => Some(Kind::Text),
            Self::Eraser => Some(Kind::Eraser),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageMapping {
    pub display: Bounds,
    pub document: Point,
}
impl ImageMapping {
    pub fn point(self, p: Point) -> Point {
        [
            (p[0] - self.display.x) * self.document[0] / self.display.width,
            (p[1] - self.display.y) * self.document[1] / self.display.height,
        ]
    }
    pub fn screen_scale(self) -> f64 {
        self.display.width / self.document[0]
    }
    fn clamp(self, p: Point) -> Point {
        [
            p[0].clamp(0., self.document[0]),
            p[1].clamp(0., self.document[1]),
        ]
    }
}
#[derive(Clone, Debug)]
pub enum Action {
    Deselect,
    Clear,
    Tool(Tool),
    UseShapes,
    Begin { point: Point, shift: bool },
    Move { point: Point, shift: bool },
    End,
    Cancel,
    Undo,
    Redo,
    Delete,
    Style(StylePatch),
    BeginStyleChange,
    EndStyleChange,
    Shape(Kind),
    Wheel(f64),
    WheelStrength(f64),
    Text { value: String, time_ms: u64 },
    EditText { point: Point },
    CommitText,
    TextHistory { redo: bool },
    Preedit(Option<Point>),
    SequenceNext(i32),
    SequenceIndex(u32),
    FinishPolyline,
    CancelPolyline,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    None,
    Preview,
    Committed,
    BeginText,
}
/// The logical tool kind is distinct from a shared default-style template.
/// Rectangle and ellipse share defaults, while their UI selection stays exact.
#[derive(Clone, Copy)]
pub struct Appearance<'a> {
    pub kind: Kind,
    pub template: &'a Mark,
    width: f64,
}
impl Appearance<'_> {
    pub fn snapshot(self) -> Mark {
        let mut m = self.template.clone();
        m.kind = self.kind;
        m.style.width = self.width;
        m
    }
}
pub const CROP_MARGIN: f64 = 7.;

#[derive(Clone, Copy, Debug, PartialEq)]
enum CropDrag {
    New {
        start: Point,
    },
    Move {
        start: Point,
        rect: Bounds,
    },
    Resize {
        start: Point,
        rect: Bounds,
        edge: [bool; 4],
    },
}

fn crop_edge_cursor(edge: [bool; 4]) -> &'static str {
    match edge {
        [true, true, ..] | [false, false, true, true] => "nwse-resize",
        [true, false, false, true] | [false, true, true, false] => "nesw-resize",
        [true, false, false, false] | [false, false, true, false] => "ew-resize",
        [false, true, false, false] | [false, false, false, true] => "ns-resize",
        _ => "move",
    }
}

#[derive(Clone)]
struct Drawing {
    mark: Mark,
    start_display: Point,
    last_display: Point,
    polyline: bool,
}

/// Both capture and pin surfaces instantiate this controller. They provide
/// coordinates and execute output actions; neither has its own tool/undo logic.
#[derive(Clone)]
pub struct EditorController {
    session: EditorSession,
    tool: Tool,
    mapping: ImageMapping,
    drawing: Option<Drawing>,
    crop: Option<Bounds>,
    crop_drag: Option<CropDrag>,
    revision: u64,
    style_group: Option<u64>,
    last_shape: Tool,
}
impl EditorController {
    pub fn new(mapping: ImageMapping) -> Self {
        Self {
            session: EditorSession::new(1., 1. / mapping.screen_scale()),
            tool: Tool::Select,
            mapping,
            drawing: None,
            crop: None,
            crop_drag: None,
            revision: 0,
            style_group: None,
            last_shape: Tool::Rect,
        }
    }
    pub fn with_text_measurer(mut self, measure: impl Fn(&Mark, f64) -> Point + 'static) -> Self {
        self.session = self.session.with_text_measurer(measure);
        self
    }
    pub fn session(&self) -> &EditorSession {
        &self.session
    }
    pub fn mapping(&self) -> ImageMapping {
        self.mapping
    }
    pub fn set_mapping(&mut self, mapping: ImageMapping) {
        self.mapping = mapping;
    }
    pub fn tool(&self) -> Tool {
        self.tool
    }
    pub fn crop(&self) -> Option<Bounds> {
        self.crop
    }
    pub fn full_crop(&self) -> Bounds {
        Bounds {
            x: 0.,
            y: 0.,
            width: self.mapping.document[0],
            height: self.mapping.document[1],
        }
    }
    pub fn active_crop(&self) -> Bounds {
        self.crop.unwrap_or_else(|| self.full_crop())
    }
    pub fn display_crop(&self) -> Bounds {
        let r = self.active_crop();
        let m = self.mapping;
        let sx = m.display.width / m.document[0];
        let sy = m.display.height / m.document[1];
        Bounds {
            x: m.display.x + r.x * sx,
            y: m.display.y + r.y * sy,
            width: r.width * sx,
            height: r.height * sy,
        }
    }
    pub fn crop_contains_display(&self, point: Point) -> bool {
        self.tool == Tool::Crop
            && (self.mapping.display.contains(point, CROP_MARGIN)
                || self.display_crop().contains(point, CROP_MARGIN))
    }
    fn crop_cursor(&self, point: Point) -> &'static str {
        if let Some(drag) = self.crop_drag {
            return match drag {
                CropDrag::New { .. } => "crosshair",
                CropDrag::Move { .. } => "move",
                CropDrag::Resize { edge, .. } => crop_edge_cursor(edge),
            };
        }
        let dc = self.display_crop();
        let edge = clivuu_core::capture::resize_edge(dc, point, CROP_MARGIN);
        if edge.iter().any(|&v| v) {
            crop_edge_cursor(edge)
        } else if self.crop.is_some_and(|r| r != self.full_crop())
            && clivuu_core::capture::contains(dc, point)
        {
            "move"
        } else {
            "crosshair"
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn drawing(&self) -> Option<&Mark> {
        self.drawing.as_ref().map(|d| &d.mark)
    }
    pub fn is_polyline_drawing(&self) -> bool {
        self.drawing.as_ref().is_some_and(|d| d.polyline)
    }
    pub fn has_active_gesture(&self) -> bool {
        self.drawing.is_some() || self.crop_drag.is_some() || self.session.is_transforming()
    }
    fn finish_polyline(&mut self) -> bool {
        let Some(mut d) = self.drawing.take() else {
            return false;
        };
        if d.polyline {
            let points = Arc::make_mut(&mut d.mark.points);
            if points.len() >= 2 {
                points.pop();
            }
            let mut deduped: Vec<Point> = Vec::with_capacity(points.len());
            for &pt in points.iter() {
                if deduped
                    .last()
                    .is_none_or(|prev| clivuu_core::annotation::distance(*prev, pt) >= 1e-6)
                {
                    deduped.push(pt);
                }
            }
            *points = deduped;
        }
        if d.mark.complete() {
            self.session.add(d.mark)
        } else {
            false
        }
    }
    pub fn appearance(&self) -> Option<Appearance<'_>> {
        if let Some(mark) = self.drawing().or_else(|| self.session.current()) {
            Some(Appearance {
                kind: mark.kind,
                template: mark,
                width: self
                    .drawing()
                    .map_or_else(|| self.session.input_width(mark.kind), |m| m.style.width),
            })
        } else {
            self.tool.kind().map(|kind| Appearance {
                kind,
                template: self.session.style(kind),
                width: self.session.input_width(kind),
            })
        }
    }
    pub fn cursor(&self, p: Point) -> Option<&'static str> {
        if self.tool == Tool::Crop {
            return Some(self.crop_cursor(p));
        }
        if self.drawing.is_some() {
            return None;
        }
        self.session
            .cursor(
                self.mapping.point(p),
                self.tool.kind(),
                self.mapping.screen_scale(),
            )
            .or(match self.tool {
                Tool::Select => Some("default"),
                Tool::Text => Some("text"),
                _ => None,
            })
    }
    pub fn sequence_preview(&self, p: Point) -> Option<&Mark> {
        (self.tool == Tool::Sequence
            && self.drawing.is_none()
            && !self.session.is_transforming()
            && self.cursor(p).is_none())
        .then(|| self.session.style(Kind::Sequence))
    }
    pub fn dispatch(&mut self, action: Action) -> Effect {
        // A continuous slider interaction is one history entry. Any other
        // operation terminates it, including keyboard undo and tool switches.
        if !matches!(
            &action,
            Action::Style(_) | Action::BeginStyleChange | Action::EndStyleChange
        ) {
            self.style_group = None;
        }
        let drawing_limit = Bounds {
            x: 0.,
            y: 0.,
            width: self.mapping.document[0],
            height: self.mapping.document[1],
        };
        let effect = match action {
            Action::SequenceNext(delta) => {
                if self.tool == Tool::Sequence && !self.session.is_text_editing() {
                    self.session.adjust_next_sequence(delta);
                }
                Effect::Committed
            }
            Action::SequenceIndex(index) => {
                self.session.set_next_sequence(index);
                Effect::Committed
            }
            Action::FinishPolyline => {
                if self.is_polyline_drawing() {
                    if self.finish_polyline() {
                        Effect::Committed
                    } else {
                        Effect::Preview
                    }
                } else {
                    Effect::None
                }
            }
            Action::CancelPolyline => {
                if self.is_polyline_drawing() {
                    self.drawing = None;
                    Effect::Preview
                } else {
                    Effect::None
                }
            }
            Action::Deselect => {
                self.drawing = None;
                self.crop_drag = None;
                self.session.cancel_gesture();
                self.session.commit_text();
                self.session.select(None);
                Effect::Committed
            }
            Action::Clear => {
                self.drawing = None;
                self.crop_drag = None;
                self.session.clear();
                Effect::Committed
            }
            Action::UseShapes => return self.dispatch(Action::Tool(self.last_shape)),
            Action::BeginStyleChange => {
                self.style_group = Some(self.session.begin_change_group());
                Effect::None
            }
            Action::EndStyleChange => {
                self.style_group = None;
                Effect::None
            }
            Action::Tool(tool) => {
                if self.is_polyline_drawing() {
                    let _ = self.finish_polyline();
                } else {
                    self.drawing = None;
                }
                self.crop_drag = None;
                self.session.cancel_gesture();
                self.session.commit_text();
                // Unapplied crop belongs to Crop alone. Choosing another tool
                // changes input behavior as well as the selected button.
                if tool != Tool::Crop {
                    self.crop = None;
                }
                self.session.select(None);
                self.tool = tool;
                if matches!(tool, Tool::Rect | Tool::Ellipse) {
                    self.last_shape = tool;
                }
                Effect::Committed
            }
            Action::Begin { point, shift } => {
                let p = self.mapping.point(point);
                if self.is_polyline_drawing() {
                    let drawing_point = self.mapping.clamp(p);
                    let scale = self.mapping.screen_scale().max(0.01);
                    let d = self.drawing.as_mut().unwrap();
                    d.last_display = point;
                    let anchor_idx = d.mark.points.len().saturating_sub(2);
                    let anchor = d.mark.points[anchor_idx];
                    let snapped = if shift {
                        clivuu_core::annotation::arrow::snap_arrow_within(
                            anchor,
                            drawing_point,
                            drawing_limit,
                        )
                    } else {
                        drawing_point
                    };
                    let points = Arc::make_mut(&mut d.mark.points);
                    if points.is_empty() {
                        points.push(snapped);
                    } else {
                        *points.last_mut().unwrap() = snapped;
                    }
                    if clivuu_core::annotation::distance(anchor, snapped) * scale > 1.
                        && points.len() < 512
                    {
                        points.push(snapped);
                    }
                    Effect::Preview
                } else if self.tool == Tool::Crop {
                    let dc = self.display_crop();
                    let edge = clivuu_core::capture::resize_edge(dc, point, CROP_MARGIN);
                    if edge.iter().any(|&v| v) {
                        let rect = self.active_crop();
                        self.crop = Some(rect);
                        self.crop_drag = Some(CropDrag::Resize {
                            start: p,
                            rect,
                            edge,
                        });
                    } else if self.crop.is_some_and(|r| r != self.full_crop())
                        && clivuu_core::capture::contains(dc, point)
                    {
                        self.crop_drag = Some(CropDrag::Move {
                            start: p,
                            rect: self.crop.unwrap(),
                        });
                    } else {
                        self.crop_drag = Some(CropDrag::New {
                            start: self.mapping.clamp(p),
                        });
                    }
                    Effect::Preview
                } else if self.session.is_text_editing()
                    && self
                        .session
                        .begin(p, self.tool.kind(), self.mapping.screen_scale())
                {
                    Effect::Preview
                } else {
                    self.session.commit_text();
                    let index =
                        self.session
                            .hit_kind(p, Kind::Text, 6. / self.mapping.screen_scale());
                    let text_body = self.tool == Tool::Text
                        && index.is_some()
                        && self
                            .session
                            .target(p, None, self.mapping.screen_scale())
                            .is_none_or(|t| {
                                t.handle.kind == clivuu_core::annotation::HandleKind::Move
                            });
                    if text_body {
                        self.session.begin_text(p, index);
                        Effect::BeginText
                    } else if self
                        .session
                        .begin(p, self.tool.kind(), self.mapping.screen_scale())
                    {
                        Effect::Preview
                    } else if self.tool == Tool::Text {
                        self.session.begin_text(p, None);
                        Effect::BeginText
                    } else if let Some(kind) = self.tool.kind() {
                        let p = self.mapping.clamp(p);
                        let mark = self.session.create(kind, p);
                        self.drawing = Some(Drawing {
                            mark,
                            start_display: point,
                            last_display: point,
                            polyline: false,
                        });
                        Effect::Preview
                    } else {
                        self.session.select(None);
                        Effect::Preview
                    }
                }
            }
            Action::Move { point, shift } => {
                let p = self.mapping.point(point);
                let drawing_point = self.mapping.clamp(p);
                if let Some(drag) = self.crop_drag {
                    let doc = self.mapping.document;
                    match drag {
                        CropDrag::New { start } => {
                            let q = self.mapping.clamp(p);
                            if (q[0] - start[0]).hypot(q[1] - start[1]) >= 1. {
                                let x = start[0].min(q[0]).round().clamp(0., (doc[0] - 1.).max(0.));
                                let y = start[1].min(q[1]).round().clamp(0., (doc[1] - 1.).max(0.));
                                let right = start[0]
                                    .max(q[0])
                                    .round()
                                    .clamp((x + 1.).min(doc[0]), doc[0]);
                                let bottom = start[1]
                                    .max(q[1])
                                    .round()
                                    .clamp((y + 1.).min(doc[1]), doc[1]);
                                self.crop = Some(Bounds {
                                    x,
                                    y,
                                    width: (right - x).max(1.),
                                    height: (bottom - y).max(1.),
                                });
                            }
                        }
                        CropDrag::Move { start, rect } => {
                            let dx = p[0] - start[0];
                            let dy = p[1] - start[1];
                            let x = (rect.x + dx)
                                .round()
                                .clamp(0., (doc[0] - rect.width).max(0.));
                            let y = (rect.y + dy)
                                .round()
                                .clamp(0., (doc[1] - rect.height).max(0.));
                            self.crop = Some(Bounds { x, y, ..rect });
                        }
                        CropDrag::Resize {
                            start,
                            rect,
                            edge: [w, n, e, s],
                        } => {
                            let dx = p[0] - start[0];
                            let dy = p[1] - start[1];
                            let a0 = if w {
                                (rect.x + dx).clamp(0., doc[0])
                            } else {
                                rect.x
                            };
                            let b0 = if e {
                                (rect.x + rect.width + dx).clamp(0., doc[0])
                            } else {
                                rect.x + rect.width
                            };
                            let a1 = if n {
                                (rect.y + dy).clamp(0., doc[1])
                            } else {
                                rect.y
                            };
                            let b1 = if s {
                                (rect.y + rect.height + dy).clamp(0., doc[1])
                            } else {
                                rect.y + rect.height
                            };
                            let x = a0.min(b0).round().clamp(0., (doc[0] - 1.).max(0.));
                            let y = a1.min(b1).round().clamp(0., (doc[1] - 1.).max(0.));
                            let right = a0.max(b0).round().clamp((x + 1.).min(doc[0]), doc[0]);
                            let bottom = a1.max(b1).round().clamp((y + 1.).min(doc[1]), doc[1]);
                            self.crop = Some(Bounds {
                                x,
                                y,
                                width: (right - x).max(1.),
                                height: (bottom - y).max(1.),
                            });
                        }
                    }
                } else if let Some(d) = &mut self.drawing {
                    d.last_display = point;
                    let anchor = if d.mark.kind == Kind::Arrow {
                        d.mark.points[d.mark.points.len().saturating_sub(2)]
                    } else {
                        d.mark.points[0]
                    };
                    let p = if shift
                        && (matches!(d.mark.kind, Kind::Arrow | Kind::Sequence)
                            || (d.mark.kind == Kind::Highlight && d.mark.style.fill == Fill::Line))
                    {
                        clivuu_core::annotation::arrow::snap_arrow_within(
                            anchor,
                            drawing_point,
                            drawing_limit,
                        )
                    } else if shift
                        && (matches!(d.mark.kind, Kind::Rect | Kind::Ellipse)
                            || (d.mark.kind == Kind::Highlight
                                && matches!(d.mark.style.fill, Fill::Rect | Fill::Ellipse)))
                    {
                        drawing_limit.square_endpoint(d.mark.points[0], drawing_point)
                    } else {
                        drawing_point
                    };
                    let points = Arc::make_mut(&mut d.mark.points);
                    if d.mark.kind == Kind::Pen
                        || (matches!(d.mark.kind, Kind::Highlight | Kind::Mosaic | Kind::Eraser)
                            && d.mark.style.fill == Fill::Stroke)
                    {
                        if points.len() < 20000 && points.last() != Some(&p) {
                            points.push(p);
                        }
                    } else if points.len() == 1 {
                        points.push(p);
                    } else {
                        *points.last_mut().unwrap() = p;
                    }
                } else {
                    self.session.preview(p, shift);
                }
                Effect::Preview
            }
            Action::End => {
                self.crop_drag = None;
                if self
                    .drawing
                    .as_ref()
                    .is_some_and(|d| d.mark.kind == Kind::Arrow)
                {
                    let enter_or_keep_polyline = {
                        let d = self.drawing.as_ref().unwrap();
                        d.polyline
                            || clivuu_core::annotation::distance(d.start_display, d.last_display)
                                <= 3.
                    };
                    if enter_or_keep_polyline {
                        let d = self.drawing.as_mut().unwrap();
                        d.polyline = true;
                        let points = Arc::make_mut(&mut d.mark.points);
                        if points.len() == 1 {
                            let p0 = points[0];
                            points.push(p0);
                        }
                        return Effect::Preview;
                    }
                }
                if let Some(d) = self.drawing.take() {
                    if d.mark.complete() {
                        self.session.add(d.mark);
                    }
                } else {
                    self.session.end();
                }
                Effect::Committed
            }
            Action::Cancel => {
                self.drawing = None;
                self.crop_drag = None;
                self.crop = None;
                self.session.cancel_gesture();
                self.session.cancel_text();
                Effect::Committed
            }
            Action::Undo => {
                self.drawing = None;
                self.session.undo();
                Effect::Committed
            }
            Action::Redo => {
                self.drawing = None;
                self.session.redo();
                Effect::Committed
            }
            Action::Delete => {
                self.session.commit_text();
                self.session.remove();
                Effect::Committed
            }
            Action::Style(patch) => {
                if let Some(kind) = self
                    .appearance()
                    .map(|m| m.kind)
                    .or_else(|| self.tool.kind())
                {
                    self.session.change(kind, &patch, self.style_group);
                    if kind == Kind::Mosaic
                        && patch.fill.is_some_and(|fill| kind.supports_fill(fill))
                    {
                        self.tool = Tool::Mosaic;
                    }
                }
                Effect::Committed
            }
            Action::Shape(kind) => {
                if matches!(kind, Kind::Rect | Kind::Ellipse) {
                    if !self.session.change_shape(kind) {
                        self.session.select(None);
                    }
                    self.tool = if kind == Kind::Rect {
                        Tool::Rect
                    } else {
                        Tool::Ellipse
                    };
                    self.last_shape = self.tool;
                }
                Effect::Committed
            }
            Action::Wheel(delta) => {
                if let Some(kind) = self
                    .appearance()
                    .map(|m| m.kind)
                    .or_else(|| self.tool.kind())
                {
                    self.session.wheel(kind, delta);
                }
                Effect::Committed
            }
            Action::WheelStrength(delta) => {
                self.session.wheel_strength(delta);
                Effect::Committed
            }
            Action::Text { value, time_ms } => {
                self.session.update_text(&value, time_ms);
                Effect::Preview
            }
            Action::EditText { point } => {
                let point = self.mapping.point(point);
                if let Some(index) =
                    self.session
                        .hit_kind(point, Kind::Text, 6. / self.mapping.screen_scale())
                {
                    self.session.begin_text(point, Some(index));
                    Effect::BeginText
                } else {
                    Effect::None
                }
            }
            Action::CommitText => {
                if self.session.is_text_editing() {
                    self.session.commit_text();
                    Effect::Committed
                } else {
                    Effect::None
                }
            }
            Action::TextHistory { redo } => {
                self.session.text_history(redo);
                Effect::Preview
            }
            Action::Preedit(size) => {
                self.session.set_text_preedit(size);
                Effect::Preview
            }
        };
        if effect == Effect::Committed {
            self.revision += 1;
        }
        effect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn controller() -> EditorController {
        EditorController::new(ImageMapping {
            display: Bounds {
                x: 300.,
                y: 200.,
                width: 500.,
                height: 300.,
            },
            document: [1000., 600.],
        })
    }
    fn draw(c: &mut EditorController, tool: Tool) {
        c.dispatch(Action::Tool(tool));
        c.dispatch(Action::Begin {
            point: [330., 240.],
            shift: false,
        });
        c.dispatch(Action::Move {
            point: [430., 320.],
            shift: false,
        });
        c.dispatch(Action::End);
    }
    #[test]
    fn selection_cursor_does_not_inherit_the_drawing_crosshair() {
        let mut c = controller();
        assert_eq!(c.cursor([500., 400.]), Some("default"));
        draw(&mut c, Tool::Rect);
        assert_ne!(c.cursor([330., 240.]), Some("default"));
        c.dispatch(Action::Tool(Tool::Select));
        assert_eq!(c.cursor([500., 400.]), Some("default"));
        assert_eq!(c.cursor([330., 240.]), Some("default"));
        c.dispatch(Action::Tool(Tool::Sequence));
        c.dispatch(Action::Begin {
            point: [330., 240.],
            shift: false,
        });
        c.dispatch(Action::End);
        c.dispatch(Action::Tool(Tool::Select));
        assert_eq!(c.cursor([330., 240.]), Some("move"));
        c.dispatch(Action::Tool(Tool::Pen));
        assert_eq!(c.cursor([500., 400.]), None);
        c.dispatch(Action::Tool(Tool::Text));
        assert_eq!(c.cursor([500., 400.]), Some("text"));
        c.dispatch(Action::Tool(Tool::Select));
        assert_eq!(c.cursor([500., 400.]), Some("default"));
    }
    #[test]
    fn freehand_controls_change_future_width_without_editing_committed_ink_or_history() {
        for tool in [Tool::Pen, Tool::Highlight, Tool::Mosaic, Tool::Eraser] {
            let mut c = controller();
            draw(&mut c, tool);
            let ink = c.session().marks()[0].clone();
            assert!(ink.freehand());
            let future = ink.style.width + 4.;
            c.dispatch(Action::Style(StylePatch {
                width: Some(future),
                ..Default::default()
            }));
            for _ in 0..3 {
                c.dispatch(Action::Wheel(-1.));
            }
            let future = future + 6.; // Mapping uses two image pixels per logical pixel.
            assert_eq!(c.session().marks(), std::slice::from_ref(&ink));
            assert_eq!(c.appearance().unwrap().snapshot().style.width, future);
            c.dispatch(Action::Undo);
            assert!(c.session().marks().is_empty());
            assert!(c.session().can_redo());
            c.dispatch(Action::Style(StylePatch {
                width: Some(future),
                ..Default::default()
            }));
            assert!(c.session().can_redo());
            c.dispatch(Action::Redo);
            assert_eq!(c.session().marks(), &[ink]);
            c.dispatch(Action::Begin {
                point: [550., 250.],
                shift: false,
            });
            assert_eq!(c.drawing().unwrap().style.width, future);
            c.dispatch(Action::Move {
                point: [600., 300.],
                shift: false,
            });
            c.dispatch(Action::End);
            assert_eq!(c.session().marks()[1].style.width, future);
        }
    }
    #[test]
    fn checkpoint_preserves_prior_marks_and_independent_undo_history() {
        let mut c = controller();
        draw(&mut c, Tool::Rect);
        draw(&mut c, Tool::Ellipse);
        let saved = c.clone();
        draw(&mut c, Tool::Arrow);
        assert_eq!(c.session().marks().len(), 3);
        c = saved;
        assert_eq!(c.session().marks().len(), 2);
        c.dispatch(Action::Undo);
        assert_eq!(c.session().marks().len(), 1);
        c.dispatch(Action::Redo);
        assert_eq!(c.session().marks().len(), 2);
    }
    #[test]
    fn no_text_export_commit_does_not_dirty_the_document() {
        let mut c = EditorController::new(ImageMapping {
            display: Bounds {
                x: 0.,
                y: 0.,
                width: 100.,
                height: 100.,
            },
            document: [100., 100.],
        });
        for _ in 0..100 {
            assert_eq!(c.dispatch(Action::CommitText), Effect::None);
        }
        assert_eq!(c.revision(), 0);
    }
    #[test]
    fn capture_and_pin_use_identical_commands_coordinates_and_undo() {
        let (mut capture, mut pin) = (controller(), controller());
        for e in [&mut capture, &mut pin] {
            draw(e, Tool::Rect);
            draw(e, Tool::Ellipse);
            assert_eq!(e.session().marks().len(), 2);
            e.dispatch(Action::Undo);
        }
        assert_eq!(capture.session().marks(), pin.session().marks());
        assert_eq!(capture.session().marks().len(), 1);
        assert_eq!(capture.session().marks()[0].kind, Kind::Rect);
        assert_eq!(capture.session().marks()[0].style.width, 6.);
        assert_eq!(capture.session().marks()[0].points[0], [60., 80.]);
        assert!(capture.session().state().can_redo);
    }
    #[test]
    fn selecting_a_tool_cancels_unapplied_crop_before_its_first_gesture() {
        let mut c = controller();
        c.dispatch(Action::Tool(Tool::Crop));
        c.dispatch(Action::Begin {
            point: [330., 240.],
            shift: false,
        });
        c.dispatch(Action::Move {
            point: [430., 320.],
            shift: false,
        });
        assert!(c.crop().is_some());
        draw(&mut c, Tool::Ellipse);
        assert!(c.crop().is_none());
        assert_eq!(c.session().marks()[0].kind, Kind::Ellipse);
    }
    #[test]
    fn preview_does_not_advance_committed_revision_or_history() {
        let mut c = controller();
        c.dispatch(Action::Tool(Tool::Pen));
        let revision = c.revision();
        c.dispatch(Action::Begin {
            point: [350., 250.],
            shift: false,
        });
        for n in 0..30 {
            c.dispatch(Action::Move {
                point: [350. + n as f64, 250. + n as f64],
                shift: false,
            });
        }
        assert_eq!(c.revision(), revision);
        assert!(!c.session().state().can_undo);
        c.dispatch(Action::End);
        assert!(c.session().state().can_undo);
        c.dispatch(Action::Undo);
        assert!(c.session().marks().is_empty());
    }
    #[test]
    fn continuous_style_drag_is_one_reversible_change_and_undo_breaks_the_group() {
        let mut c = controller();
        draw(&mut c, Tool::Mosaic);
        let original = c.session().marks()[0].clone();
        c.dispatch(Action::BeginStyleChange);
        for level in 7..=24 {
            c.dispatch(Action::Style(StylePatch {
                effect_level: Some(level as f64),
                ..Default::default()
            }));
        }
        c.dispatch(Action::EndStyleChange);
        assert_eq!(c.session().marks()[0].style.effect_level, 24.);
        c.dispatch(Action::Undo);
        assert_eq!(c.session().marks()[0], original);
        c.dispatch(Action::Redo);
        assert_eq!(c.session().marks()[0].style.effect_level, 24.);
        c.dispatch(Action::Undo);
        c.dispatch(Action::Style(StylePatch {
            effect_level: Some(10.),
            ..Default::default()
        }));
        assert!(!c.session().can_redo());
        c.dispatch(Action::Undo);
        assert_eq!(c.session().marks()[0], original);
    }
    #[test]
    fn ellipse_defaults_share_style_but_keep_logical_kind_and_remember_shape_tool() {
        let mut c = controller();
        c.dispatch(Action::Tool(Tool::Ellipse));
        assert_eq!(c.appearance().unwrap().snapshot().kind, Kind::Ellipse);
        c.dispatch(Action::Style(StylePatch {
            fill: Some(Fill::Solid),
            width: Some(8.),
            ..Default::default()
        }));
        c.dispatch(Action::Tool(Tool::Pen));
        c.dispatch(Action::UseShapes);
        assert_eq!(c.tool(), Tool::Ellipse);
        assert_eq!(c.appearance().unwrap().snapshot().style.fill, Fill::Solid);
        c.dispatch(Action::Tool(Tool::Rect));
        assert_eq!(c.appearance().unwrap().template.style.width, 8.);
    }
    #[test]
    fn drawing_and_editing_arrows_snap_through_one_history_transaction() {
        let mut c = controller();
        c.dispatch(Action::Tool(Tool::Arrow));
        c.dispatch(Action::Begin {
            point: [330., 240.],
            shift: false,
        });
        c.dispatch(Action::Move {
            point: [430., 270.],
            shift: true,
        });
        let p = c.drawing().unwrap().points.clone();
        assert!((p[0][1] - p[1][1]).abs() < 1e-10);
        c.dispatch(Action::Move {
            point: [430., 270.],
            shift: false,
        });
        assert_ne!(
            c.drawing().unwrap().points[0][1],
            c.drawing().unwrap().points[1][1]
        );
        c.dispatch(Action::Move {
            point: [430., 270.],
            shift: true,
        });
        c.dispatch(Action::End);
        let before = c.session().marks()[0].clone();
        c.dispatch(Action::Begin {
            point: [430., 240.],
            shift: false,
        });
        c.dispatch(Action::Move {
            point: [460., 350.],
            shift: true,
        });
        c.dispatch(Action::End);
        let after = c.session().marks()[0].clone();
        assert_eq!(after.points[0], before.points[0]);
        let dx = after.points[1][0] - after.points[0][0];
        let dy = after.points[1][1] - after.points[0][1];
        assert!((dx.abs() - dy.abs()).abs() < 1e-10 || dx.abs() < 1e-10 || dy.abs() < 1e-10);
        c.dispatch(Action::Undo);
        assert_eq!(c.session().marks()[0], before);
        c.dispatch(Action::Redo);
        assert_eq!(c.session().marks()[0], after);
    }
    #[test]
    fn shift_shapes_keep_equal_sides_in_all_quadrants_and_one_undo() {
        for tool in [Tool::Rect, Tool::Ellipse] {
            for [dx, dy] in [
                [100., 30.],
                [-100., 30.],
                [100., -30.],
                [-100., -30.],
                [0., 100.],
                [100., 0.],
            ] {
                let mut c = controller();
                c.dispatch(Action::Tool(tool));
                let start = [550., 350.];
                let point = [start[0] + dx, start[1] + dy];
                c.dispatch(Action::Begin {
                    point: start,
                    shift: false,
                });
                c.dispatch(Action::Move { point, shift: true });
                let p = c.drawing().unwrap().points.clone();
                assert!(((p[1][0] - p[0][0]).abs() - (p[1][1] - p[0][1]).abs()).abs() < 1e-10);
                assert!(c.session().marks().is_empty());
                c.dispatch(Action::Move {
                    point,
                    shift: false,
                });
                let p = c.drawing().unwrap().points.clone();
                assert_ne!((p[1][0] - p[0][0]).abs(), (p[1][1] - p[0][1]).abs());
                c.dispatch(Action::Move { point, shift: true });
                c.dispatch(Action::End);
                assert_eq!(c.session().marks().len(), 1);
                c.dispatch(Action::Undo);
                assert!(c.session().marks().is_empty());
                c.dispatch(Action::Redo);
                assert_eq!(c.session().marks().len(), 1);
            }
        }
    }
    #[test]
    fn shift_square_at_selection_edge_limits_both_axes_together() {
        let limit = Bounds {
            x: 10.,
            y: 20.,
            width: 90.,
            height: 80.,
        };
        assert_eq!(limit.square_endpoint([95., 50.], [100., 80.]), [100., 55.]);
        assert_eq!(limit.square_endpoint([15., 50.], [10., 30.]), [10., 45.]);
    }
    #[test]
    fn highlight_shift_constrains_rect_ellipse_and_line_like_shapes_and_arrows() {
        for fill in [Fill::Rect, Fill::Ellipse] {
            for [dx, dy] in [[100., 35.], [-100., 35.], [100., -35.], [-100., -35.]] {
                let mut c = controller();
                c.dispatch(Action::Tool(Tool::Highlight));
                c.dispatch(Action::Style(StylePatch {
                    fill: Some(fill),
                    ..Default::default()
                }));
                let start = [550., 350.];
                let point = [start[0] + dx, start[1] + dy];
                c.dispatch(Action::Begin {
                    point: start,
                    shift: false,
                });
                c.dispatch(Action::Move { point, shift: true });
                let p = c.drawing().unwrap().points.clone();
                assert!(((p[1][0] - p[0][0]).abs() - (p[1][1] - p[0][1]).abs()).abs() < 1e-10);
                c.dispatch(Action::Move {
                    point,
                    shift: false,
                });
                let p = c.drawing().unwrap().points.clone();
                assert_ne!((p[1][0] - p[0][0]).abs(), (p[1][1] - p[0][1]).abs());
                c.dispatch(Action::Move { point, shift: true });
                c.dispatch(Action::End);
                assert_eq!(c.session().marks().len(), 1);
                let committed = &c.session().marks()[0].points;
                assert!(
                    ((committed[1][0] - committed[0][0]).abs()
                        - (committed[1][1] - committed[0][1]).abs())
                    .abs()
                        < 1e-10
                );
                c.dispatch(Action::Undo);
                assert!(c.session().marks().is_empty());
            }
        }
        let mut c = controller();
        c.dispatch(Action::Tool(Tool::Highlight));
        c.dispatch(Action::Style(StylePatch {
            fill: Some(Fill::Line),
            ..Default::default()
        }));
        c.dispatch(Action::Begin {
            point: [330., 240.],
            shift: false,
        });
        c.dispatch(Action::Move {
            point: [430., 265.],
            shift: true,
        });
        c.dispatch(Action::End);
        let p = &c.session().marks()[0].points;
        assert!((p[0][1] - p[1][1]).abs() < 1e-10);
    }
    #[test]
    fn arrow_click_to_draw_polyline_supports_multi_segment_shift_finish_and_cancel() {
        let mut c = controller();
        c.dispatch(Action::Tool(Tool::Arrow));
        c.dispatch(Action::Begin {
            point: [330., 240.],
            shift: false,
        });
        c.dispatch(Action::End);
        assert!(c.is_polyline_drawing());
        assert!(c.session().marks().is_empty());

        c.dispatch(Action::Move {
            point: [430., 260.],
            shift: true,
        });
        assert!((c.drawing().unwrap().points[1][1] - 80.).abs() < 1e-10);

        c.dispatch(Action::Begin {
            point: [430., 240.],
            shift: false,
        });
        c.dispatch(Action::End);
        assert!(c.is_polyline_drawing());

        c.dispatch(Action::Move {
            point: [430., 320.],
            shift: false,
        });
        c.dispatch(Action::Begin {
            point: [430., 320.],
            shift: false,
        });
        c.dispatch(Action::End);
        c.dispatch(Action::FinishPolyline);
        assert!(!c.is_polyline_drawing());
        assert_eq!(c.session().marks().len(), 1);
        assert_eq!(
            *c.session().marks()[0].points,
            vec![[60., 80.], [260., 80.], [260., 240.]]
        );
        c.dispatch(Action::Undo);
        assert!(c.session().marks().is_empty());
        c.dispatch(Action::Redo);
        assert_eq!(c.session().marks()[0].points.len(), 3);

        // Single click followed by FinishPolyline or CancelPolyline adds nothing.
        for finish in [Action::FinishPolyline, Action::CancelPolyline] {
            c.dispatch(Action::Begin {
                point: [500., 300.],
                shift: false,
            });
            c.dispatch(Action::End);
            assert!(c.is_polyline_drawing());
            c.dispatch(finish);
            assert!(!c.is_polyline_drawing());
            assert_eq!(c.session().marks().len(), 1);
        }

        // Switching tools while >= 2 vertices are placed commits the polyline.
        c.dispatch(Action::Begin {
            point: [350., 250.],
            shift: false,
        });
        c.dispatch(Action::End);
        c.dispatch(Action::Begin {
            point: [450., 310.],
            shift: false,
        });
        c.dispatch(Action::End);
        c.dispatch(Action::Tool(Tool::Select));
        assert_eq!(c.session().marks().len(), 2);
        assert!(c.session().current().is_none());
    }
    #[test]
    fn pin_crop_supports_full_frame_resize_subcrop_move_eight_way_resize_and_outer_handle_hits() {
        let mut c = controller();
        c.dispatch(Action::Tool(Tool::Crop));
        assert!(c.crop().is_none());
        assert_eq!(
            c.active_crop(),
            Bounds {
                x: 0.,
                y: 0.,
                width: 1000.,
                height: 600.,
            }
        );
        assert!(c.crop_contains_display([294., 200.]));
        assert!(!c.crop_contains_display([290., 200.]));
        assert_eq!(c.cursor([300., 200.]), Some("nwse-resize"));
        assert_eq!(c.cursor([550., 200.]), Some("ns-resize"));
        assert_eq!(c.cursor([300., 350.]), Some("ew-resize"));
        assert_eq!(c.cursor([550., 350.]), Some("crosshair"));

        // Dragging the outer left border inward immediately resizes the full-frame crop.
        c.dispatch(Action::Begin {
            point: [296., 350.],
            shift: false,
        });
        c.dispatch(Action::Move {
            point: [346., 350.],
            shift: false,
        });
        c.dispatch(Action::End);
        assert_eq!(
            c.crop(),
            Some(Bounds {
                x: 100.,
                y: 0.,
                width: 900.,
                height: 600.,
            })
        );

        // Drawing inside creates a sub-crop, then Move and Resize adjust it cleanly.
        c.dispatch(Action::Cancel);
        assert!(c.crop().is_none());
        c.dispatch(Action::Begin {
            point: [350., 250.],
            shift: false,
        });
        c.dispatch(Action::Move {
            point: [450., 350.],
            shift: false,
        });
        c.dispatch(Action::End);
        assert_eq!(
            c.crop(),
            Some(Bounds {
                x: 100.,
                y: 100.,
                width: 200.,
                height: 200.,
            })
        );
        assert_eq!(c.cursor([400., 300.]), Some("move"));
        c.dispatch(Action::Begin {
            point: [400., 300.],
            shift: false,
        });
        c.dispatch(Action::Move {
            point: [430., 320.],
            shift: false,
        });
        c.dispatch(Action::End);
        assert_eq!(
            c.crop(),
            Some(Bounds {
                x: 160.,
                y: 140.,
                width: 200.,
                height: 200.,
            })
        );
        // Display SE corner of [160, 140, 200, 200] is [300 + 360*0.5, 200 + 340*0.5] = [480, 370].
        assert_eq!(c.cursor([480., 370.]), Some("nwse-resize"));
        c.dispatch(Action::Begin {
            point: [480., 370.],
            shift: false,
        });
        c.dispatch(Action::Move {
            point: [500., 390.],
            shift: false,
        });
        c.dispatch(Action::End);
        assert_eq!(
            c.crop(),
            Some(Bounds {
                x: 160.,
                y: 140.,
                width: 240.,
                height: 240.,
            })
        );
    }
}
