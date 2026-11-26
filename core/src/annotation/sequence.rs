//! Numbered badges and optional leaders, in document units, shared by both editors.
use super::{
    Bounds, Handle, HandleKind, Mark, Point,
    arrow::{ArrowPath, ArrowStroke},
};
use serde::{Deserialize, Serialize};
use std::f64::consts::SQRT_2;

pub const MAX_INDEX: u32 = 9999;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    #[default]
    Decimal,
    Roman,
    LowerAlpha,
    UpperAlpha,
    Chinese,
}
impl Format {
    pub const ALL: [Self; 5] = [
        Self::Decimal,
        Self::Roman,
        Self::LowerAlpha,
        Self::UpperAlpha,
        Self::Chinese,
    ];
    pub fn sample(self) -> &'static str {
        match self {
            Self::Decimal => "1, 2, 3",
            Self::Roman => "I, II, III",
            Self::LowerAlpha => "a, b, c",
            Self::UpperAlpha => "A, B, C",
            Self::Chinese => "一, 二, 三",
        }
    }
    pub fn label(self, index: u32) -> String {
        let mut n = index.clamp(1, MAX_INDEX);
        match self {
            Self::Decimal => n.to_string(),
            Self::LowerAlpha | Self::UpperAlpha => {
                let mut chars = vec![];
                while n > 0 {
                    n -= 1;
                    chars.push((b'A' + (n % 26) as u8) as char);
                    n /= 26;
                }
                let s: String = chars.into_iter().rev().collect();
                if self == Self::LowerAlpha {
                    s.to_lowercase()
                } else {
                    s
                }
            }
            Self::Roman => {
                let mut s = String::new();
                for (value, symbol) in [
                    (1000, "M"),
                    (900, "CM"),
                    (500, "D"),
                    (400, "CD"),
                    (100, "C"),
                    (90, "XC"),
                    (50, "L"),
                    (40, "XL"),
                    (10, "X"),
                    (9, "IX"),
                    (5, "V"),
                    (4, "IV"),
                    (1, "I"),
                ] {
                    while n >= value {
                        s.push_str(symbol);
                        n -= value;
                    }
                }
                s
            }
            Self::Chinese => {
                let digits = ['零', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
                let mut s = String::new();
                let mut zero = false;
                for (value, unit) in [(1000, "千"), (100, "百"), (10, "十"), (1, "")] {
                    let d = n / value;
                    n %= value;
                    if d > 0 {
                        if zero {
                            s.push('零');
                        }
                        if !(d == 1 && value == 10 && s.is_empty()) {
                            s.push(digits[d as usize]);
                        }
                        s.push_str(unit);
                        zero = false;
                    } else if !s.is_empty() && n > 0 {
                        zero = true;
                    }
                }
                s
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    #[default]
    FilledCircle,
    BorderedCircle,
    OutlineCircle,
    Text,
    RoundedSquare,
}
impl Shape {
    pub const ALL: [Self; 5] = [
        Self::FilledCircle,
        Self::BorderedCircle,
        Self::OutlineCircle,
        Self::Text,
        Self::RoundedSquare,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::FilledCircle => "Filled circle",
            Self::BorderedCircle => "Bordered circle",
            Self::OutlineCircle => "Outline circle",
            Self::Text => "Number only",
            Self::RoundedSquare => "Rounded square",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Sequence {
    pub index: u32,
    pub format: Format,
    pub shape: Shape,
    /// None selects a contrasting foreground, independent of application appearance.
    pub text_color: Option<String>,
}
impl Default for Sequence {
    fn default() -> Self {
        Self {
            index: 1,
            format: Format::default(),
            shape: Shape::default(),
            text_color: None,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Patch {
    pub index: Option<u32>,
    pub format: Option<Format>,
    pub shape: Option<Shape>,
    /// Empty text restores automatic contrast.
    pub text_color: Option<String>,
}
impl Patch {
    pub(super) fn apply(&self, s: &mut Sequence) {
        if let Some(v) = self.index {
            s.index = v.clamp(1, MAX_INDEX);
        }
        if let Some(v) = self.format {
            s.format = v;
        }
        if let Some(v) = self.shape {
            s.shape = v;
        }
        if let Some(v) = &self.text_color {
            s.text_color = (!v.is_empty()).then(|| v.clone());
        }
    }
}
impl Mark {
    pub fn sequence_radius(&self, unit: f64) -> f64 {
        SQRT_2 * self.style.width / unit
    }
    pub fn sequence_bounds(&self, unit: f64) -> Bounds {
        let radius = self.sequence_radius(unit);
        let p = self.points[0];
        Bounds {
            x: p[0] - radius,
            y: p[1] - radius,
            width: 2. * radius,
            height: 2. * radius,
        }
    }
    pub fn sequence_leader(&self, unit: f64) -> Vec<ArrowPath> {
        let a = self.points[0];
        let b = *self.points.last().unwrap();
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let length = dx.hypot(dy);
        let radius = self.sequence_radius(unit);
        let inset = if self.style.sequence.shape == Shape::RoundedSquare {
            radius * length / dx.abs().max(dy.abs()).max(0.01)
        } else {
            radius
        };
        if length <= inset {
            return vec![];
        }
        let size = self.style.width / unit;
        let [c, s] = [dx / length, dy / length];
        let map = |[x, y]: Point| [a[0] + x * c - y * s, a[1] + x * s + y * c];
        let fill = |points: Vec<Point>| ArrowPath {
            points: points.into_iter().map(map).collect(),
            closed: true,
            stroke: None,
        };
        let stroke = |points: Vec<Point>, closed| ArrowPath {
            points: points.into_iter().map(map).collect(),
            closed,
            stroke: Some(ArrowStroke {
                width: size * 0.4,
                round: true,
                dashes: vec![],
            }),
        };
        if length <= 2. * inset {
            return vec![fill(vec![[0., -size], [length, 0.], [0., size]])];
        }
        // The fixed terminal is the reference's default (type 12): a filled
        // notched arrowhead scaled to 1.1 times the standard 3 × 1.5 contour.
        let w = size * 0.4;
        vec![
            stroke(vec![[inset + 1.5 * w, 0.], [length - 2.2 * w, 0.]], false),
            fill(vec![
                [length, 0.],
                [length - 3.3 * w, 1.65 * w],
                [length - 2.2 * w, 0.],
                [length - 3.3 * w, -1.65 * w],
            ]),
        ]
    }
    pub fn sequence_has_leader(&self, unit: f64) -> bool {
        self.points.len() > 1 && !self.sequence_leader(unit).is_empty()
    }
    fn sequence_control_bounds(&self, unit: f64, scale: f64) -> Bounds {
        let mut b = self.sequence_bounds(unit);
        let t = self.transform();
        let width = b.width.max(24. / (scale * t.sx));
        let height = b.height.max(32. / (scale * t.sy));
        b.x -= (width - b.width) / 2.;
        b.y -= (height - b.height) / 2.;
        b.width = width;
        b.height = height;
        b
    }
    pub fn sequence_controls(&self, unit: f64, scale: f64) -> Vec<Handle> {
        let b = self.sequence_control_bounds(unit, scale);
        let t = self.transform();
        let mx = 9. / (scale * t.sx);
        let row = 15. / (scale * t.sy);
        let mut controls = vec![];
        for (kind, p) in [
            (HandleKind::Increase, [b.x - mx, b.y]),
            (HandleKind::Decrease, [b.x - mx, b.y + row]),
            (HandleKind::Delete, [b.x + b.width + mx, b.y]),
            (HandleKind::ToggleLeader, [b.x - mx, b.y + b.height]),
        ] {
            controls.push(Handle {
                kind,
                index: None,
                point: self.map_point(p, unit, false),
                local: None,
                endpoint: None,
            });
        }
        if self.sequence_has_leader(unit) {
            controls.push(Handle {
                kind: HandleKind::Endpoint,
                index: Some(self.points.len() - 1),
                point: self.map_point(*self.points.last().unwrap(), unit, false),
                local: None,
                endpoint: Some(1),
            });
        }
        controls
    }
    pub fn sequence_hovered(&self, p: Point, unit: f64, scale: f64) -> bool {
        let q = self.map_point(p, unit, true);
        let t = self.transform();
        let b = self.sequence_control_bounds(unit, scale);
        let mx = 18. / (scale * t.sx);
        let my = 10. / (scale * t.sy);
        q[0] >= b.x - mx
            && q[0] <= b.x + b.width + mx
            && q[1] >= b.y - my
            && q[1] <= b.y + b.height + my
    }
    pub(super) fn sequence_hit(&self, p: Point, unit: f64, margin: f64) -> bool {
        let a = self.points[0];
        let radius = self.sequence_radius(unit) + margin;
        let badge = if self.style.sequence.shape == Shape::RoundedSquare {
            (p[0] - a[0]).abs().max((p[1] - a[1]).abs()) <= radius
        } else {
            (p[0] - a[0]).hypot(p[1] - a[1]) <= radius
        };
        badge
            || self
                .sequence_leader(unit)
                .iter()
                .any(|path| path.hit(p, margin))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numbering_formats_cross_boundaries_without_wrapping_or_localizing() {
        assert_eq!(Format::UpperAlpha.label(26), "Z");
        assert_eq!(Format::UpperAlpha.label(27), "AA");
        assert_eq!(Format::LowerAlpha.label(703), "aaa");
        assert_eq!(Format::Roman.label(49), "XLIX");
        assert_eq!(Format::Chinese.label(10), "十");
        assert_eq!(Format::Chinese.label(101), "一百零一");
        assert_eq!(Format::Chinese.label(1010), "一千零一十");
        assert_eq!(Format::Decimal.label(9999), "9999");
    }
    #[test]
    fn badge_and_leader_are_both_hittable_and_click_only_marks_are_complete() {
        let mut m = Mark::new(super::super::Kind::Sequence, [100., 100.], 1.);
        assert!(m.complete());
        assert!(m.hit([100., 100.], 1., 0.));
        m.points = std::sync::Arc::new(vec![[100., 100.], [200., 100.]]);
        assert!(m.hit([170., 100.], 1., 0.));
        assert!(!m.hit([170., 110.], 1., 0.));
        let encoded = serde_json::to_value(&m).unwrap();
        let restored: Mark = serde_json::from_value(encoded).unwrap();
        assert_eq!(m, restored);
    }
    #[test]
    fn committed_numbering_tracks_history_without_consuming_cancelled_drafts() {
        use super::super::{EditorSession, Kind, StylePatch};
        let mut s = EditorSession::new(1., 1.);
        let first = s.create(Kind::Sequence, [50., 50.]);
        assert_eq!(first.style.sequence.index, 1);
        let first = s.create(Kind::Sequence, [70., 50.]);
        assert_eq!(first.style.sequence.index, 1);
        s.add(first);
        let second = s.create(Kind::Sequence, [100., 50.]);
        assert_eq!(second.style.sequence.index, 2);
        s.add(second);
        s.undo();
        assert_eq!(s.style(Kind::Sequence).style.sequence.index, 2);
        s.redo();
        assert_eq!(s.style(Kind::Sequence).style.sequence.index, 3);
        s.change(
            Kind::Sequence,
            &StylePatch {
                sequence: Some(Box::new(Patch {
                    index: Some(10),
                    ..Default::default()
                })),
                ..Default::default()
            },
            None,
        );
        assert_eq!(s.current().unwrap().style.sequence.index, 10);
        assert_eq!(s.style(Kind::Sequence).style.sequence.index, 3);
        s.undo();
        assert_eq!(s.current().unwrap().style.sequence.index, 2);
        s.remove();
        assert_eq!(s.style(Kind::Sequence).style.sequence.index, 2);
        s.clear();
        assert_eq!(s.style(Kind::Sequence).style.sequence.index, 1);
    }
    #[test]
    fn badge_controls_edit_independently_and_leader_edits_preserve_its_transform() {
        use super::super::{EditorSession, HandleKind, Kind, Transform};
        let mut s = EditorSession::new(1., 1.);
        let mut m = Mark::new(Kind::Sequence, [100., 100.], 1.);
        m.points = std::sync::Arc::new(vec![[100., 100.], [230., 170.]]);
        m.style.sequence.index = 5;
        m.transform = Some(Transform {
            sx: 2.,
            sy: 2.,
            angle: 0.3,
            ..Default::default()
        });
        s.add(m.clone());
        let end = m
            .handles(1., 1.)
            .into_iter()
            .find(|h| h.kind == HandleKind::Endpoint)
            .unwrap()
            .point;
        assert!(s.begin(end, None, 1.));
        s.preview([end[0] + 20., end[1] + 5.], false);
        s.end();
        assert_eq!(s.current().unwrap().transform, m.transform);
        s.undo();
        assert_eq!(s.current(), Some(&m));
        assert!(s.begin(end, None, 1.));
        s.preview([end[0] + 20., end[1] + 5.], true);
        let snapped = s.current().unwrap();
        let start = snapped.map_point(snapped.points[0], 1., false);
        let tip = snapped.map_point(*snapped.points.last().unwrap(), 1., false);
        let angle = (tip[1] - start[1]).atan2(tip[0] - start[0]) / std::f64::consts::FRAC_PI_4;
        assert!((angle - angle.round()).abs() < 1e-10);
        s.end();
        s.undo();
        assert_eq!(s.current(), Some(&m));
        for kind in [
            HandleKind::Increase,
            HandleKind::Decrease,
            HandleKind::ToggleLeader,
        ] {
            let h = m
                .handles(1., 1.)
                .into_iter()
                .find(|h| h.kind == kind)
                .unwrap()
                .point;
            let before = s.current().unwrap().clone();
            assert!(s.begin(h, None, 1.));
            s.end();
            let after = s.current().unwrap().clone();
            assert_ne!(after, before);
            assert_eq!(after.transform, before.transform);
            assert_eq!(s.style(Kind::Sequence).style.sequence.index, 6);
            s.undo();
            assert_eq!(s.current(), Some(&before));
            s.redo();
            assert_eq!(s.current(), Some(&after));
            s.undo();
        }
    }
    #[test]
    fn hover_controls_remain_separated_at_small_sizes_and_endpoint_inside_hides_leader() {
        use super::super::{EditorSession, Kind};
        let mut m = Mark::new(Kind::Sequence, [100., 100.], 1.);
        for scale in [0.5, 1., 2.] {
            m.style.width = 1.;
            let handles = m.handles(1., scale);
            assert!(m.visible_handles(None, 1., scale).is_empty());
            assert_eq!(m.visible_handles(Some([100., 100.]), 1., scale).len(), 4);
            for (i, a) in handles.iter().enumerate() {
                for b in &handles[i + 1..] {
                    assert!(super::super::geometry::distance(a.point, b.point) * scale >= 14.9);
                }
            }
        }
        m.style.width = 10.;
        m.points = std::sync::Arc::new(vec![[100., 100.], [250., 100.]]);
        let mut s = EditorSession::new(1., 1.);
        s.add(m.clone());
        assert!(s.begin([250., 100.], Some(Kind::Sequence), 1.));
        s.preview([100., 100.], false);
        s.end();
        assert_eq!(s.current().unwrap().points.len(), 1);
        s.undo();
        assert_eq!(s.current(), Some(&m));
    }
    #[test]
    fn leaders_keep_measured_terminal_proportions_and_source_geometry() {
        use super::super::Kind;
        let mut m = Mark::new(Kind::Sequence, [100., 100.], 1.);
        m.style.width = 10.;
        m.points = std::sync::Arc::new(vec![[100., 100.], [200., 100.]]);
        let paths = m.sequence_leader(1.);
        assert_eq!(paths[0].stroke.as_ref().unwrap().width, 4.);
        let head = paths.last().unwrap();
        assert_eq!(
            head.points,
            vec![[200., 100.], [186.8, 106.6], [191.2, 100.], [186.8, 93.4]]
        );
        let encoded = serde_json::to_value(&m).unwrap();
        assert_eq!(serde_json::from_value::<Mark>(encoded).unwrap(), m);
        m.points = std::sync::Arc::new(vec![[100., 100.], [120., 100.]]);
        let short = m.sequence_leader(1.);
        assert_eq!(short.len(), 1);
        assert!(short[0].stroke.is_none());
    }
    #[test]
    fn reset_starts_a_new_sequence_and_history_restores_its_next_index() {
        use super::super::{EditorSession, Kind, StylePatch};
        let mut s = EditorSession::new(1., 1.);
        for n in 1..=3 {
            let mark = s.create(Kind::Sequence, [n as f64 * 50., 50.]);
            assert_eq!(mark.style.sequence.index, n);
            s.add(mark);
        }
        s.select(None);
        s.change(
            Kind::Sequence,
            &StylePatch {
                sequence: Some(Box::new(Patch {
                    index: Some(1),
                    ..Default::default()
                })),
                ..Default::default()
            },
            None,
        );
        for n in 1..=2 {
            let mark = s.create(Kind::Sequence, [n as f64 * 50., 150.]);
            assert_eq!(mark.style.sequence.index, n);
            s.add(mark);
        }
        s.undo();
        assert_eq!(s.style(Kind::Sequence).style.sequence.index, 2);
        s.undo();
        assert_eq!(s.style(Kind::Sequence).style.sequence.index, 1);
        s.redo();
        assert_eq!(s.style(Kind::Sequence).style.sequence.index, 2);
        s.redo();
        assert_eq!(s.style(Kind::Sequence).style.sequence.index, 3);
        // The old group's same-numbered badge must not rewind the current group.
        s.select(Some(1));
        s.remove();
        assert_eq!(s.style(Kind::Sequence).style.sequence.index, 3);
    }
}
