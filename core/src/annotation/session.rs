use super::arrow::{ArrowType, LineStyle, snap_arrow};
use super::geometry::{distance, resize_cursor, round_reference};
use super::{Fill, Handle, HandleKind, History, Kind, Mark, Point, Transform};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, f64::consts::PI, sync::Arc};

type TextMeasurer = std::rc::Rc<dyn Fn(&Mark, f64) -> Point>;

/// Property changes are discrete commands. Input adapters never own undo state.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct StylePatch {
    pub color: Option<String>,
    pub width: Option<f64>,
    pub fill: Option<Fill>,
    pub effect: Option<String>,
    pub effect_level: Option<f64>,
    pub alpha: Option<u8>,
    pub arrow_type: Option<ArrowType>,
    pub line_style: Option<LineStyle>,
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub font_size_value: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub outline_mode: Option<String>,
    pub outline_width: Option<f64>,
    pub outline_color: Option<String>,
    pub sequence: Option<Box<super::sequence::Patch>>,
}
impl StylePatch {
    fn apply(&self, m: &mut Mark, scale: f64) {
        if let Some(patch) = &self.sequence {
            patch.apply(&mut m.style.sequence);
        }
        macro_rules! copy {($($f:ident),*)=>{$(if let Some(v)=&self.$f {m.style.$f=v.clone();})*};}
        copy!(
            color,
            alpha,
            line_style,
            arrow_type,
            font_family,
            bold,
            italic,
            outline_color
        );
        if let Some(fill) = self.fill.filter(|fill| m.kind.supports_fill(*fill)) {
            m.style.fill = fill;
        }
        if let Some(w) = self.width {
            m.style.width = m.kind.stroke_width(w, scale);
        }
        if let Some(effect) = &self.effect {
            let effect = if effect == "blur" { "blur" } else { "pixelate" };
            if m.style.effect != effect {
                m.style.effect = effect.into();
                // Both effects share the unified strength range, so switching modes
                // preserves the active strength (falling back only when uninitialized).
                m.style.effect_level = f64::from(m.style.mosaic_level());
            }
        }
        if let Some(level) = self.effect_level {
            m.style.effect_level = level;
            m.style.effect_level = f64::from(m.style.mosaic_level());
        }
        if let Some(w) = self.outline_width {
            m.style.outline_width = w.clamp(0., 8.);
        }
        if let Some(mode) = &self.outline_mode {
            m.style.outline_mode = mode.clone();
            if mode != "none" {
                m.style.outline_last_mode = mode.clone();
            }
        }
        if let Some(size) = self.font_size {
            m.style.font_size = if size == 0. {
                18.
            } else {
                size.clamp(1., 300.)
            };
            m.style.font_unit = "pt".into();
        }
    }
}
#[derive(Clone, Debug)]
pub struct Target {
    pub index: Option<usize>,
    pub handle: Handle,
    pub text: bool,
}
#[derive(Clone, Debug)]
struct Gesture {
    index: Option<usize>,
    before: Mark,
    after: Mark,
    point: Point,
    handle: Handle,
    text: bool,
    draft_only: bool,
}
#[derive(Clone, Debug)]
struct TextRun {
    delete: bool,
    time_ms: u64,
    next: usize,
}
#[derive(Clone, Debug)]
struct TextEdit {
    index: Option<usize>,
    draft: Mark,
    undo: Vec<String>,
    redo: Vec<String>,
    run: Option<TextRun>,
    frame: Option<Mark>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorState {
    pub marks: Vec<Mark>,
    pub active: isize,
    pub current: Option<Mark>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub text_editing: bool,
    pub transforming: bool,
}

/// One controller model for capture and pin editing. Previews never write
/// history, render images, spawn processes or perform IPC. Completed commands
/// update history once, and UI sensitivity is derived from state().
#[derive(Clone)]
pub struct EditorSession {
    unit: f64,
    stroke_scale: f64,
    history: History,
    active: Option<usize>,
    styles: HashMap<Kind, Mark>,
    gesture: Option<Gesture>,
    text: Option<TextEdit>,
    wheel_group: Option<(Option<usize>, Kind, u64)>,
    next_group: u64,
    measure_text: Option<TextMeasurer>,
}
impl EditorSession {
    pub fn new(unit: f64, stroke_scale: f64) -> Self {
        assert!(unit.is_finite() && unit > 0. && stroke_scale.is_finite() && stroke_scale > 0.);
        let scale = stroke_scale.max(0.01);
        Self {
            unit,
            stroke_scale: scale,
            history: History::default(),
            active: None,
            styles: Kind::ALL
                .into_iter()
                .map(|k| (k, Mark::new(k, [0., 0.], scale)))
                .collect(),
            gesture: None,
            text: None,
            wheel_group: None,
            next_group: 0,
            measure_text: None,
        }
    }
    /// The platform supplies glyph metrics without introducing a toolkit
    /// dependency into core. Measurement happens before history is recorded,
    /// so changing a placed font is one atomic, origin-preserving edit.
    pub fn with_text_measurer(mut self, measure: impl Fn(&Mark, f64) -> Point + 'static) -> Self {
        self.measure_text = Some(std::rc::Rc::new(measure));
        self
    }
    pub fn unit(&self) -> f64 {
        self.unit
    }
    pub fn stroke_scale(&self) -> f64 {
        self.stroke_scale
    }
    pub fn marks(&self) -> &[Mark] {
        self.history.marks()
    }
    pub fn can_undo(&self) -> bool {
        self.history.state().can_undo
    }
    pub fn can_redo(&self) -> bool {
        self.history.state().can_redo
    }
    pub fn is_text_editing(&self) -> bool {
        self.text.is_some()
    }
    pub fn is_text_transforming(&self) -> bool {
        self.gesture.as_ref().is_some_and(|g| g.text)
    }
    pub fn is_transforming(&self) -> bool {
        self.gesture.is_some()
    }
    pub fn begin_change_group(&mut self) -> u64 {
        self.wheel_group = None;
        self.next_group += 1;
        self.next_group
    }
    pub fn active(&self) -> Option<usize> {
        self.active
    }
    pub fn current(&self) -> Option<&Mark> {
        self.gesture
            .as_ref()
            .map(|g| &g.after)
            .or_else(|| self.text.as_ref().map(|t| &t.draft))
            .or_else(|| self.active.and_then(|i| self.marks().get(i)))
    }
    pub fn control_mark(&self) -> Option<&Mark> {
        self.gesture
            .as_ref()
            .map(|g| &g.after)
            .or_else(|| self.text.as_ref().and_then(|t| t.frame.as_ref()))
            .or_else(|| self.current())
    }
    pub fn style(&self, kind: Kind) -> &Mark {
        &self.styles[&if kind == Kind::Ellipse {
            Kind::Rect
        } else {
            kind
        }]
    }
    /// Width controls describe the next freehand stroke; committed ink keeps
    /// the footprint captured at release. Other selected marks remain editable.
    pub fn input_width(&self, kind: Kind) -> f64 {
        self.current()
            .filter(|m| m.kind == kind && !m.freehand())
            .unwrap_or_else(|| self.style(kind))
            .style
            .width
    }
    pub fn state(&self) -> EditorState {
        let h = self.history.state();
        EditorState {
            marks: self.marks().to_vec(),
            active: self.active.map_or(-1, |i| i as isize),
            current: self.current().cloned(),
            can_undo: h.can_undo,
            can_redo: h.can_redo,
            text_editing: self.text.is_some(),
            transforming: self.gesture.is_some(),
        }
    }
    /// Borrow unchanged marks in the render hot path. Only the active gesture
    /// or text draft substitutes a reference; no point/style/string is cloned.
    pub fn displayed(&self) -> impl Iterator<Item = &Mark> {
        self.marks()
            .iter()
            .enumerate()
            .filter_map(move |(i, mark)| {
                if let Some(text) = &self.text {
                    if text.index == Some(i) {
                        return text.frame.is_none().then(|| self.current().unwrap());
                    }
                } else if let Some(gesture) = self.gesture.as_ref().filter(|g| g.index == Some(i)) {
                    return Some(&gesture.after);
                }
                Some(mark)
            })
            .chain(
                self.text
                    .as_ref()
                    .filter(|t| t.index.is_none() && t.frame.is_none())
                    .and_then(|_| self.current()),
            )
    }
    pub fn display_marks(&self) -> Vec<Mark> {
        self.displayed().cloned().collect()
    }
    pub fn select(&mut self, index: Option<usize>) {
        self.wheel_group = None;
        self.active = index.filter(|i| *i < self.marks().len());
    }
    pub fn hit(&self, p: Point, tolerance: f64) -> Option<usize> {
        self.marks()
            .iter()
            .rposition(|m| m.hit(p, self.unit, tolerance))
    }
    pub fn hit_kind(&self, p: Point, kind: Kind, tolerance: f64) -> Option<usize> {
        self.marks()
            .iter()
            .rposition(|m| m.kind == kind && m.hit(p, self.unit, tolerance))
    }
    fn decorate(&self, mut mark: Mark) -> Mark {
        if mark.kind == Kind::Text
            && let Some(measure) = &self.measure_text
        {
            let size = measure(&mark, self.unit);
            if size.iter().all(|v| v.is_finite() && *v >= 0.) {
                mark.text_size = Some(size);
            }
        }
        if mark.kind == Kind::Highlight && mark.style.fill == Fill::Line && mark.points.len() > 2 {
            let first = mark.map_point(mark.points[0], self.unit, false);
            mark.points = Arc::new(vec![mark.points[0], *mark.points.last().unwrap()]);
            let next = mark.map_point(mark.points[0], self.unit, false);
            if let Some(t) = &mut mark.transform {
                t.x += first[0] - next[0];
                t.y += first[1] - next[1];
            }
        } else if mark.kind == Kind::Arrow && mark.points.len() > 2 {
            let first = mark.map_point(mark.points[0], self.unit, false);
            let mut deduped: Vec<Point> = Vec::with_capacity(mark.points.len());
            for &pt in mark.points.iter() {
                if deduped
                    .last()
                    .is_none_or(|prev| distance(*prev, pt) >= 1e-9)
                {
                    deduped.push(pt);
                }
            }
            if deduped.len() != mark.points.len() && deduped.len() >= 2 {
                mark.points = Arc::new(deduped);
                let next = mark.map_point(mark.points[0], self.unit, false);
                if let Some(t) = &mut mark.transform {
                    t.x += first[0] - next[0];
                    t.y += first[1] - next[1];
                }
            }
        }
        mark
    }
    pub fn create(&mut self, kind: Kind, point: Point) -> Mark {
        self.wheel_group = None;
        self.active = None;
        let mut m = self.style(kind).clone();
        m.kind = kind;
        m.points = Arc::new(vec![point]);
        self.decorate(m)
    }
    pub fn add(&mut self, mark: Mark) -> bool {
        if !mark.valid() {
            return false;
        }
        self.wheel_group = None;
        let mark = self.decorate(mark);
        let sequence = mark.kind == Kind::Sequence;
        self.history.add(mark);
        self.active = self.history.selected();
        if sequence {
            self.refresh_sequence_index();
        }
        true
    }
    pub fn begin_text(&mut self, point: Point, index: Option<usize>) -> bool {
        self.commit_text();
        self.gesture = None;
        self.wheel_group = None;
        let draft = if let Some(i) = index {
            let Some(m) = self.marks().get(i).filter(|m| m.kind == Kind::Text) else {
                return false;
            };
            m.clone()
        } else {
            self.create(Kind::Text, point)
        };
        self.active = index;
        self.text = Some(TextEdit {
            index,
            draft,
            undo: vec![],
            redo: vec![],
            run: None,
            frame: None,
        });
        true
    }
    fn keep_origin(&self, before: &Mark, mut after: Mark) -> Mark {
        if before.kind == Kind::Text {
            let a = before.map_point(before.points[0], self.unit, false);
            let b = after.map_point(after.points[0], self.unit, false);
            if let Some(t) = &mut after.transform {
                t.x += a[0] - b[0];
                t.y += a[1] - b[1];
            }
        }
        after
    }
    pub fn set_text_metrics(&mut self, size: Point) {
        if !size.iter().all(|v| v.is_finite() && *v > 0.) {
            return;
        }
        let Some(t) = &self.text else {
            return;
        };
        let before = t.draft.clone();
        let mut next = before.clone();
        next.text_size = Some(size);
        let next = self.keep_origin(&before, next);
        self.text.as_mut().unwrap().draft = next;
    }
    pub fn set_text_preedit(&mut self, size: Option<Point>) {
        let Some(t) = &self.text else {
            return;
        };
        let frame = size.map(|s| {
            let mut m = t.draft.clone();
            m.text_size = Some(s.map(|v| v.max(1.)));
            if m.text.as_deref() == Some("") {
                m.text = Some(" ".into());
            }
            m.preedit_frame = true;
            self.keep_origin(&t.draft, m)
        });
        self.text.as_mut().unwrap().frame = frame;
    }
    fn refresh_preedit(&mut self) {
        if let Some(size) = self
            .text
            .as_ref()
            .and_then(|t| t.frame.as_ref())
            .and_then(|m| m.text_size)
        {
            self.set_text_preedit(Some(size));
        }
    }
    pub fn update_text(&mut self, value: &str, now_ms: u64) {
        if self.gesture.as_ref().is_some_and(|g| g.text) {
            self.end();
        }
        let Some(t) = &self.text else {
            return;
        };
        let before = t.draft.clone();
        let mut len = 0;
        let value: String = value
            .chars()
            .take_while(|ch| {
                len += ch.len_utf16();
                len <= 4000
            })
            .collect();
        let old = before.text.as_deref().unwrap_or("");
        if old == value {
            return;
        }
        let a: Vec<_> = old.encode_utf16().collect();
        let b: Vec<_> = value.encode_utf16().collect();
        let start = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
        let end = a[start..]
            .iter()
            .rev()
            .zip(b[start..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let removed = &a[start..a.len() - end];
        let added = &b[start..b.len() - end];
        let delete = !removed.is_empty();
        let single = removed.len() + added.len() == 1
            && !removed.iter().chain(added).any(|v| *v == 10 || *v == 13);
        let merge = single
            && t.run.as_ref().is_some_and(|r| {
                r.delete == delete
                    && now_ms.saturating_sub(r.time_ms) < 1000
                    && (start == r.next || (delete && start + 1 == r.next))
            })
            && t.redo.is_empty();
        let mut after = before.clone();
        after.text = Some(value);
        let after = self.keep_origin(&before, self.decorate(after));
        let t = self.text.as_mut().unwrap();
        if !merge {
            t.undo.push(old.into());
        }
        t.redo.clear();
        t.run = single.then_some(TextRun {
            delete,
            time_ms: now_ms,
            next: start + added.len(),
        });
        t.draft = after;
        self.refresh_preedit();
    }
    fn refresh_sequence_index(&mut self) {
        let next = self.history.sequence_index();
        self.styles
            .get_mut(&Kind::Sequence)
            .unwrap()
            .style
            .sequence
            .index = next;
    }
    pub fn set_next_sequence(&mut self, index: u32) {
        self.history.set_sequence_index(index);
        self.refresh_sequence_index();
    }
    pub fn adjust_next_sequence(&mut self, delta: i32) {
        self.set_next_sequence(self.history.sequence_index().saturating_add_signed(delta));
    }
    fn sequence_control(&mut self, index: usize, control: HandleKind) {
        let mut next = self.marks()[index].clone();
        match control {
            HandleKind::Increase => {
                next.style.sequence.index = next
                    .style
                    .sequence
                    .index
                    .saturating_add(1)
                    .min(super::sequence::MAX_INDEX)
            }
            HandleKind::Decrease => {
                next.style.sequence.index = next.style.sequence.index.saturating_sub(1).max(1)
            }
            HandleKind::ToggleLeader => {
                if next.sequence_has_leader(self.unit) {
                    next.points = Arc::new(vec![next.points[0]]);
                } else {
                    let p = next.points[0];
                    let r = next.sequence_radius(self.unit);
                    next.points = Arc::new(vec![p, [p[0] + r * 3., p[1] + r * 3.]]);
                }
            }
            _ => return,
        }
        self.history.replace(index, next, None);
        self.active = Some(index);
    }
    pub fn text_history(&mut self, redo: bool) -> bool {
        let Some(t) = &mut self.text else {
            return false;
        };
        let (source, target) = if redo {
            (&mut t.redo, &mut t.undo)
        } else {
            (&mut t.undo, &mut t.redo)
        };
        let Some(value) = source.pop() else {
            return false;
        };
        target.push(t.draft.text.clone().unwrap_or_default());
        t.run = None;
        let before = t.draft.clone();
        let mut after = before.clone();
        after.text = Some(value);
        let after = self.keep_origin(&before, self.decorate(after));
        self.text.as_mut().unwrap().draft = after;
        self.refresh_preedit();
        true
    }
    pub fn commit_text(&mut self) -> bool {
        if self.gesture.as_ref().is_some_and(|g| g.text) {
            self.end();
        }
        let Some(t) = self.text.take() else {
            return false;
        };
        self.wheel_group = None;
        if t.draft.text.as_ref().is_some_and(|s| !s.is_empty()) {
            if let Some(i) = t.index {
                self.history.replace(i, t.draft, None);
                self.active = Some(i);
            } else {
                self.history.add(t.draft);
                self.active = self.history.selected();
            }
        } else if let Some(i) = t.index {
            self.history.remove(i);
            self.active = None;
        } else {
            self.active = None;
        }
        true
    }
    pub fn cancel_text(&mut self) {
        if let Some(t) = self.text.take() {
            if self.gesture.as_ref().is_some_and(|g| g.text) {
                self.gesture = None;
            }
            self.active = t.index;
            self.wheel_group = None;
        }
    }
    pub fn change(&mut self, kind: Kind, patch: &StylePatch, group: Option<u64>) {
        // Freehand brush and geometric shape selectors choose the next mark's
        // geometry; freehand strokes and shapes must never convert into each other.
        if matches!(kind, Kind::Highlight | Kind::Mosaic | Kind::Eraser)
            && patch.fill.is_some_and(|fill| {
                kind.supports_fill(fill)
                    && self.current().is_some_and(|m| {
                        m.kind == kind && (m.style.fill == Fill::Stroke) != (fill == Fill::Stroke)
                    })
            })
        {
            self.cancel_gesture();
            self.select(None);
        }
        if let Some(level) = patch.font_size_value {
            self.set_text_size(level, group);
        }
        if group.is_none() {
            self.wheel_group = None;
        }
        let key = if kind == Kind::Ellipse {
            Kind::Rect
        } else {
            kind
        };
        let mut style = self.style(kind).clone();
        if patch.font_size.is_some() {
            style.style.font_scale = self
                .current()
                .map_or(self.stroke_scale, |m| m.style.font_scale);
        }
        style.kind = kind;
        patch.apply(&mut style, self.stroke_scale);
        style.kind = key;
        self.styles.insert(key, style);
        if let Some(before) = self.current().filter(|m| m.kind == kind).cloned() {
            let mut next = before.clone();
            patch.apply(&mut next, self.stroke_scale);
            if before.freehand() {
                next.style.width = before.style.width;
            }
            let next = self.keep_origin(&before, self.decorate(next));
            if let Some(t) = self.text.as_mut().filter(|_| kind == Kind::Text) {
                t.run = None;
                t.draft = next;
            } else if let Some(i) = self.active {
                self.history.replace(i, next, group);
            }
        }
        if kind == Kind::Sequence
            && let Some(index) = patch.sequence.as_ref().and_then(|p| p.index)
        {
            if self.current().is_none_or(|m| m.kind != Kind::Sequence) {
                self.history.set_sequence_index(index);
            }
            self.refresh_sequence_index();
        }
        self.refresh_preedit();
    }
    pub fn set_text_size(&mut self, value: f64, group: Option<u64>) {
        if self.gesture.is_some() {
            return;
        }
        if group.is_none() {
            self.wheel_group = None;
        }
        let level = value.clamp(1., 72.);
        let scale = 0.25 + (level - 1.) * 0.125;
        let mut style = self.style(Kind::Text).clone();
        style.style.font_size = 18.;
        style.style.font_unit = "pt".into();
        style.style.font_scale = self.stroke_scale;
        style.transform = Some(Transform {
            sx: scale,
            sy: scale,
            ..Transform::default()
        });
        self.styles.insert(Kind::Text, style);
        let Some(mut next) = self.current().filter(|m| m.kind == Kind::Text).cloned() else {
            return;
        };
        let base = next.text_pixels(1.) / next.style.font_scale;
        let sx = 24. * scale / base;
        let mut t = next.transform();
        t.sx = sx;
        t.sy = sx;
        next.transform = Some(t);
        if let Some(text) = &mut self.text {
            text.run = None;
            text.draft = next;
        } else if let Some(i) = self.active {
            self.history.replace(i, next, group);
        }
        self.refresh_preedit();
    }
    pub fn change_shape(&mut self, kind: Kind) -> bool {
        if !matches!(kind, Kind::Rect | Kind::Ellipse)
            || self.gesture.is_some()
            || self
                .current()
                .is_some_and(|m| !matches!(m.kind, Kind::Rect | Kind::Ellipse))
        {
            return false;
        }
        self.wheel_group = None;
        if let Some(mut mark) = self.current().cloned() {
            let s = &mut self.styles.get_mut(&Kind::Rect).unwrap().style;
            s.color = mark.style.color.clone();
            s.width = mark.style.width;
            s.fill = mark.style.fill;
            s.alpha = mark.style.alpha;
            if mark.kind != kind {
                mark.kind = kind;
                self.history.replace(self.active.unwrap(), mark, None);
            }
        }
        true
    }
    pub fn undo(&mut self) {
        self.commit_text();
        self.wheel_group = None;
        self.gesture = None;
        self.history.undo();
        self.active = self.history.selected();
        self.refresh_sequence_index();
    }
    pub fn redo(&mut self) {
        self.commit_text();
        self.wheel_group = None;
        self.gesture = None;
        self.history.redo();
        self.active = self.history.selected();
        self.refresh_sequence_index();
    }
    pub fn remove(&mut self) {
        self.wheel_group = None;
        if let Some(i) = self.active {
            self.history.remove(i);
        }
        self.active = None;
        self.refresh_sequence_index();
    }
    pub fn clear(&mut self) {
        self.cancel_text();
        self.wheel_group = None;
        self.gesture = None;
        self.active = None;
        self.history.clear();
        self.refresh_sequence_index();
    }
    pub fn target(&self, p: Point, tool: Option<Kind>, screen_scale: f64) -> Option<Target> {
        let tolerance = 7. / screen_scale.max(0.01);
        if let Some(text) = &self.text {
            if text.frame.is_some() {
                return None;
            }
            let mark = self.current()?;
            if let Some(h) = mark.nearest_handle(p, self.unit, screen_scale, tolerance) {
                return Some(Target {
                    index: text.index,
                    handle: h,
                    text: true,
                });
            }
            let q = mark.map_point(p, self.unit, true);
            let b = mark.control_bounds(self.unit, screen_scale);
            let t = mark.transform();
            let mx = 5. / (screen_scale * t.sx.abs());
            let my = 5. / (screen_scale * t.sy.abs());
            let inside = q[0] >= b.x - mx
                && q[0] <= b.x + b.width + mx
                && q[1] >= b.y - my
                && q[1] <= b.y + b.height + my;
            let edge = ((q[0] - b.x).abs() / mx)
                .min((q[0] - b.x - b.width).abs() / mx)
                .min((q[1] - b.y).abs() / my)
                .min((q[1] - b.y - b.height).abs() / my)
                <= 1.;
            return (inside && edge).then(|| Target {
                index: text.index,
                handle: Handle::moving(p),
                text: true,
            });
        }
        if tool == Some(Kind::Eraser) && self.style(Kind::Eraser).style.fill == Fill::Stroke {
            return None;
        }
        let handle = self
            .current()
            .and_then(|m| m.nearest_handle(p, self.unit, screen_scale, tolerance));
        let index = if handle.is_some() {
            self.active
        } else {
            self.marks().iter().enumerate().rposition(|(i, m)| {
                // Once deselected, only Text and Sequence can be re-selected or moved;
                // active non-freehand marks remain adjustable until finalized.
                ((Some(i) == self.active && !m.freehand()) || m.reselectable())
                    && m.hit(p, self.unit, tolerance)
            })
        }?;
        if tool.is_some()
            && Some(index) != self.active
            && handle.is_none()
            && !(tool == Some(Kind::Text) && self.marks()[index].kind == Kind::Text)
            && !(tool == Some(Kind::Sequence) && self.marks()[index].kind == Kind::Sequence)
        {
            return None;
        }
        Some(Target {
            index: Some(index),
            handle: handle.unwrap_or_else(|| Handle::moving(p)),
            text: false,
        })
    }
    pub fn hover(&self, p: Point, tool: Option<Kind>, scale: f64) -> Option<&Mark> {
        if self.gesture.is_some() || tool.is_some_and(|k| k != Kind::Text) {
            return None;
        }
        let i = self.hit_kind(p, Kind::Text, 1. / scale.max(0.01))?;
        (Some(i) != self.active).then(|| &self.marks()[i])
    }
    pub fn cursor(&self, p: Point, tool: Option<Kind>, scale: f64) -> Option<&'static str> {
        if let Some(g) = &self.gesture {
            let mark = if g.handle.kind == HandleKind::Endpoint {
                &g.after
            } else {
                &g.before
            };
            return Some(resize_cursor(mark, &g.handle, self.unit, true));
        }
        let target = self.target(p, tool, scale)?;
        let m = target
            .index
            .and_then(|i| self.marks().get(i))
            .or_else(|| self.current())?;
        Some(resize_cursor(m, &target.handle, self.unit, false))
    }
    pub fn begin(&mut self, p: Point, tool: Option<Kind>, scale: f64) -> bool {
        self.wheel_group = None;
        let Some(target) = self.target(p, tool, scale) else {
            return false;
        };
        let mut index = target.index;
        if matches!(
            target.handle.kind,
            HandleKind::Increase | HandleKind::Decrease | HandleKind::ToggleLeader
        ) {
            self.sequence_control(index.unwrap(), target.handle.kind);
            return true;
        }
        if target.handle.kind == HandleKind::Delete {
            if self.text.is_some() && self.current().and_then(|m| m.text.as_deref()) == Some("") {
                if self.text.as_ref().unwrap().index.is_none() {
                    self.cancel_text();
                } else {
                    self.commit_text();
                }
                return true;
            }
            if self.text.is_some() {
                self.commit_text();
                index = self.active;
            }
            self.active = index;
            self.remove();
            return true;
        }
        let mut draft_only = false;
        if target.text {
            if self.current().and_then(|m| m.text.as_deref()) == Some("") {
                draft_only = true;
            } else {
                let old = self.text.clone().unwrap();
                self.commit_text();
                index = self.active;
                let Some(committed) = index.and_then(|i| self.marks().get(i)).cloned() else {
                    return false;
                };
                self.text = Some(TextEdit {
                    index,
                    draft: committed,
                    run: None,
                    ..old
                });
            }
        }
        self.active = index;
        let before = if draft_only {
            self.current().unwrap().clone()
        } else {
            self.marks()[index.unwrap()].clone()
        };
        self.gesture = Some(Gesture {
            index,
            after: before.clone(),
            before,
            point: p,
            handle: target.handle,
            text: target.text,
            draft_only,
        });
        true
    }
    pub fn preview(&mut self, p: Point, shift: bool) {
        let Some(g) = &self.gesture else {
            return;
        };
        let before = &g.before;
        let b = before.bounds(self.unit);
        let t = before.transform();
        let mut next = before.clone();
        let mut n = t;
        match g.handle.kind {
            HandleKind::Endpoint if before.kind == Kind::Sequence => {
                let old = before.map_point(*before.points.last().unwrap(), self.unit, false);
                let point = [old[0] + p[0] - g.point[0], old[1] + p[1] - g.point[1]];
                let point = if shift {
                    snap_arrow(before.map_point(before.points[0], self.unit, false), point)
                } else {
                    point
                };
                let point = before.map_point(point, self.unit, true);
                let points = Arc::make_mut(&mut next.points);
                let last = points.len() - 1;
                points[last] = point;
            }
            HandleKind::Increase | HandleKind::Decrease | HandleKind::ToggleLeader => return,
            HandleKind::Endpoint => {
                if !shift && distance(p, g.point) < 1e-9 {
                    self.gesture.as_mut().unwrap().after = before.clone();
                    return;
                }
                let mut pts: Vec<Point> = before
                    .points
                    .iter()
                    .map(|&pt| before.map_point(pt, self.unit, false))
                    .collect();
                let i = g
                    .handle
                    .endpoint
                    .or(g.handle.index)
                    .unwrap_or(0)
                    .min(pts.len() - 1);
                let mut q = [pts[i][0] + p[0] - g.point[0], pts[i][1] + p[1] - g.point[1]];
                if shift && pts.len() >= 2 {
                    let other = if i == 0 { pts[1] } else { pts[i - 1] };
                    q = snap_arrow(other, q);
                }
                pts[i] = q;
                next.points = Arc::new(pts);
                next.transform = None;
            }
            HandleKind::Move => {
                n.x += p[0] - g.point[0];
                n.y += p[1] - g.point[1];
                next.transform = Some(n);
            }
            HandleKind::Rotate => {
                let c = before.map_point(b.center(), self.unit, false);
                let mut a = t.angle + (p[1] - c[1]).atan2(p[0] - c[0])
                    - (g.point[1] - c[1]).atan2(g.point[0] - c[0]);
                if shift {
                    a = round_reference(a / (PI / 12.)) * (PI / 12.);
                }
                n.angle = a;
                next.transform = Some(n);
            }
            HandleKind::Resize if before.kind == Kind::Text => {
                let c = before.map_point(b.center(), self.unit, false);
                let h = g.handle.point;
                let dx = p[0] - g.point[0] + h[0] - c[0];
                let dy = p[1] - g.point[1] + h[1] - c[1];
                let ratio = dx.hypot(dy) / distance(h, c).max(0.01);
                let base = before.text_pixels(1.) / before.style.font_scale;
                n.sx = (t.sx * ratio).clamp(6. / base, 219. / base);
                n.sy = n.sx;
                next.transform = Some(n);
            }
            HandleKind::Resize => {
                let h = g.handle.local.unwrap();
                let opposite = before.handles(self.unit, 1.)[(g.handle.index.unwrap() + 4) % 8]
                    .local
                    .unwrap();
                let q = before.map_point(p, self.unit, true);
                let q0 = before.map_point(g.point, self.unit, true);
                let mut fx = if h[0] == opposite[0] {
                    1.
                } else {
                    (h[0] + q[0] - q0[0] - opposite[0]) / (h[0] - opposite[0])
                };
                let mut fy = if h[1] == opposite[1] {
                    1.
                } else {
                    (h[1] + q[1] - q0[1] - opposite[1]) / (h[1] - opposite[1])
                };
                if shift {
                    let factor = if (fx - 1.).abs() > (fy - 1.).abs() {
                        fx
                    } else {
                        fy
                    };
                    fx = factor;
                    fy = factor;
                }
                n.sx = (t.sx * fx).clamp(0.05, 100.);
                n.sy = (t.sy * fy).clamp(0.05, 100.);
                let c = b.center();
                let dx = (opposite[0] - c[0]) * (t.sx - n.sx);
                let dy = (opposite[1] - c[1]) * (t.sy - n.sy);
                n.x = t.x + dx * t.angle.cos() - dy * t.angle.sin();
                n.y = t.y + dx * t.angle.sin() + dy * t.angle.cos();
                next.transform = Some(n);
            }
            HandleKind::Delete => return,
        }
        self.gesture.as_mut().unwrap().after = next;
    }
    pub fn cancel_gesture(&mut self) {
        self.gesture = None;
    }
    pub fn end(&mut self) {
        let Some(mut g) = self.gesture.take() else {
            return;
        };
        let a = g.before.transform();
        if g.after.kind == Kind::Sequence && !g.after.sequence_has_leader(self.unit) {
            g.after.points = Arc::new(vec![g.after.points[0]]);
        }
        let b = g.after.transform();
        if [
            a.x - b.x,
            a.y - b.y,
            a.sx - b.sx,
            a.sy - b.sy,
            a.angle - b.angle,
        ]
        .iter()
        .all(|v| v.abs() < 1e-9)
        {
            g.after.transform = g.before.transform;
        }
        if !g.draft_only {
            self.history
                .replace(g.index.unwrap(), g.after.clone(), None);
        }
        if g.text
            && let Some(t) = &mut self.text
        {
            t.draft = g.after.clone();
        }
        if g.after.kind == Kind::Text {
            let s = 0.25 + (g.after.text_level().clamp(1., 72.) - 1.) * 0.125;
            let style = self.styles.get_mut(&Kind::Text).unwrap();
            style.style.font_size = 18.;
            style.style.font_unit = "pt".into();
            style.style.font_scale = self.stroke_scale;
            style.transform = Some(Transform {
                sx: s,
                sy: s,
                ..Transform::default()
            });
        }
    }
    pub fn wheel(&mut self, kind: Kind, delta: f64) {
        if !delta.is_finite() || delta == 0. || self.gesture.is_some() {
            return;
        }
        let group = if let Some((i, k, id)) = self
            .wheel_group
            .filter(|(i, k, _)| *i == self.active && *k == kind)
        {
            self.wheel_group = Some((i, k, id));
            id
        } else {
            self.next_group += 1;
            self.wheel_group = Some((self.active, kind, self.next_group));
            self.next_group
        };
        let current = self
            .current()
            .filter(|m| m.kind == kind)
            .unwrap_or_else(|| self.style(kind));
        let patch = if kind == Kind::Mosaic && current.style.fill == Fill::Rect {
            StylePatch {
                effect_level: Some(f64::from(current.style.mosaic_level()) - delta.signum()),
                ..Default::default()
            }
        } else if kind == Kind::Text {
            StylePatch {
                font_size_value: Some(current.text_level() - delta.signum()),
                ..StylePatch::default()
            }
        } else {
            StylePatch {
                width: Some(
                    self.input_width(kind)
                        - delta.signum() * kind.stroke_range(self.stroke_scale)[2],
                ),
                ..StylePatch::default()
            }
        };
        self.change(kind, &patch, Some(group));
    }
    pub fn wheel_strength(&mut self, delta: f64) {
        if !delta.is_finite() || delta == 0. || self.gesture.is_some() {
            return;
        }
        let group = if let Some((i, k, id)) = self
            .wheel_group
            .filter(|(i, k, _)| *i == self.active && *k == Kind::Mosaic)
        {
            self.wheel_group = Some((i, k, id));
            id
        } else {
            self.next_group += 1;
            self.wheel_group = Some((self.active, Kind::Mosaic, self.next_group));
            self.next_group
        };
        let current = self
            .current()
            .filter(|m| m.kind == Kind::Mosaic)
            .unwrap_or_else(|| self.style(Kind::Mosaic));
        let patch = StylePatch {
            effect_level: Some(f64::from(current.style.mosaic_level()) - delta.signum()),
            ..Default::default()
        };
        self.change(Kind::Mosaic, &patch, Some(group));
    }
}
