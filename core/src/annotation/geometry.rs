use super::{Fill, Kind, Mark, Point};
use serde::{Deserialize, Serialize};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Bounds {
    pub fn center(self) -> Point {
        [self.x + self.width / 2., self.y + self.height / 2.]
    }
    /// Opposite corner of a square, anchored at the initial press. Reduce both
    /// axes together at an image edge, so the constrained shape stays square.
    pub fn square_endpoint(self, anchor: Point, pointer: Point) -> Point {
        let dx = pointer[0] - anchor[0];
        let dy = pointer[1] - anchor[1];
        let room_x = if dx < 0. {
            anchor[0] - self.x
        } else {
            self.x + self.width - anchor[0]
        };
        let room_y = if dy < 0. {
            anchor[1] - self.y
        } else {
            self.y + self.height - anchor[1]
        };
        let side = dx.abs().max(dy.abs()).min(room_x).min(room_y).max(0.);
        [
            anchor[0] + dx.signum() * side,
            anchor[1] + dy.signum() * side,
        ]
    }
    pub fn contains(self, p: Point, margin: f64) -> bool {
        p[0] >= self.x - margin
            && p[0] <= self.x + self.width + margin
            && p[1] >= self.y - margin
            && p[1] <= self.y + self.height + margin
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandleKind {
    Endpoint,
    Resize,
    Delete,
    Rotate,
    Move,
    Increase,
    Decrease,
    ToggleLeader,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Handle {
    pub kind: HandleKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    pub point: Point,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<Point>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<usize>,
}
impl Handle {
    pub fn moving(point: Point) -> Self {
        Self {
            kind: HandleKind::Move,
            index: None,
            point,
            local: None,
            endpoint: None,
        }
    }
}
pub fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
// Preserve the reference editor's tie direction, including negative angles.
pub(super) fn round_reference(value: f64) -> f64 {
    (value + 0.5).floor()
}
pub(super) fn segment(p: Point, a: Point, b: Point) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let den = dx * dx + dy * dy;
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / if den == 0. { 1. } else { den })
        .clamp(0., 1.);
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}
impl Mark {
    pub fn bounds(&self, unit: f64) -> Bounds {
        if self.kind == Kind::Sequence {
            return self.sequence_bounds(unit);
        }
        if self.kind == Kind::Text {
            let p = self.points[0];
            let size = self.text_size.unwrap_or([
                self.text.as_deref().unwrap_or("").encode_utf16().count() as f64
                    * self.text_pixels(unit)
                    * 0.6,
                self.text_pixels(unit) * 1.3,
            ]);
            return Bounds {
                x: p[0],
                y: p[1],
                width: size[0].max(1.),
                height: size[1].max(1.),
            };
        }
        let (mut x, mut y, mut right, mut bottom) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for p in self.points.iter() {
            x = x.min(p[0]);
            y = y.min(p[1]);
            right = right.max(p[0]);
            bottom = bottom.max(p[1]);
        }
        Bounds {
            x,
            y,
            width: (right - x).max(1.),
            height: (bottom - y).max(1.),
        }
    }
    pub fn map_point(&self, p: Point, unit: f64, inverse: bool) -> Point {
        self.point_mapping(unit).map(p, inverse)
    }
    /// Compute the center and trigonometry once for an entire path, rather
    /// than walking every point to rediscover its bounds for each sample.
    pub fn point_mapping(&self, unit: f64) -> PointMapping {
        let center = self.bounds(unit).center();
        let transform = self.transform();
        let (s, c) = transform.angle.sin_cos();
        PointMapping {
            center,
            transform,
            s,
            c,
        }
    }
}
#[derive(Clone, Copy)]
pub struct PointMapping {
    center: Point,
    transform: super::Transform,
    s: f64,
    c: f64,
}
impl PointMapping {
    pub fn map(self, p: Point, inverse: bool) -> Point {
        let [cx, cy] = self.center;
        let t = self.transform;
        let (s, c) = (self.s, self.c);
        if inverse {
            let x = p[0] - cx - t.x;
            let y = p[1] - cy - t.y;
            [cx + (x * c + y * s) / t.sx, cy + (-x * s + y * c) / t.sy]
        } else {
            let x = (p[0] - cx) * t.sx;
            let y = (p[1] - cy) * t.sy;
            [cx + t.x + x * c - y * s, cy + t.y + x * s + y * c]
        }
    }
}
impl Mark {
    pub fn control_bounds(&self, unit: f64, screen_scale: f64) -> Bounds {
        let b = self.bounds(unit);
        if self.kind != Kind::Text {
            return b;
        }
        let t = self.transform();
        let scale = screen_scale.max(0.01);
        let w = b.width.max(28. / (scale * t.sx.abs().max(0.01)));
        let h = b.height.max(28. / (scale * t.sy.abs().max(0.01)));
        Bounds {
            x: b.x + (b.width - w) / 2.,
            y: b.y + (b.height - h) / 2.,
            width: w,
            height: h,
        }
    }
    pub fn handles(&self, unit: f64, screen_scale: f64) -> Vec<Handle> {
        if self.kind == Kind::Sequence {
            return self.sequence_controls(unit, screen_scale);
        }
        if self.freehand() {
            return vec![];
        }
        if self.line() {
            return (0..self.points.len())
                .map(|index| Handle {
                    kind: HandleKind::Endpoint,
                    index: Some(index),
                    point: self.map_point(self.points[index], unit, false),
                    local: None,
                    endpoint: Some(index),
                })
                .collect();
        }
        let b = self.control_bounds(unit, screen_scale);
        let (x, y, w, h) = (b.x, b.y, b.width, b.height);
        let mut handles = vec![];
        for (index, p) in [
            [x, y],
            [x + w / 2., y],
            [x + w, y],
            [x + w, y + h / 2.],
            [x + w, y + h],
            [x + w / 2., y + h],
            [x, y + h],
            [x, y + h / 2.],
        ]
        .into_iter()
        .enumerate()
        {
            if self.kind == Kind::Text && index % 2 != 0 {
                continue;
            }
            handles.push(Handle {
                kind: if self.kind == Kind::Text && index == 2 {
                    HandleKind::Delete
                } else {
                    HandleKind::Resize
                },
                index: Some(index),
                point: self.map_point(p, unit, false),
                local: Some(p),
                endpoint: None,
            });
        }
        handles.push(Handle {
            kind: HandleKind::Rotate,
            index: None,
            point: self.map_point(
                [
                    x + w / 2.,
                    y - 22. / (screen_scale * self.transform().sy).max(0.01),
                ],
                unit,
                false,
            ),
            local: None,
            endpoint: None,
        });
        handles
    }
    pub fn visible_handles(&self, p: Option<Point>, unit: f64, scale: f64) -> Vec<Handle> {
        let handles = self.handles(unit, scale);
        if self.kind == Kind::Sequence {
            return if p.is_some_and(|p| self.sequence_hovered(p, unit, scale)) {
                handles
            } else {
                handles
                    .into_iter()
                    .filter(|h| h.kind == HandleKind::Endpoint)
                    .collect()
            };
        }
        if self.kind != Kind::Text {
            return handles;
        }
        let b = self.control_bounds(unit, scale);
        let t = self.transform();
        let hovered = p.is_some_and(|p| {
            let p = self.map_point(p, unit, true);
            let mx = 8. / (scale * t.sx.abs());
            let my = 8. / (scale * t.sy.abs());
            p[0] >= b.x - mx
                && p[0] <= b.x + b.width + mx
                && p[1] >= b.y - my
                && p[1] <= b.y + b.height + my
        });
        if hovered {
            handles
        } else {
            handles
                .into_iter()
                .filter(|h| h.kind == HandleKind::Rotate)
                .collect()
        }
    }
    pub fn nearest_handle(
        &self,
        p: Point,
        unit: f64,
        scale: f64,
        tolerance: f64,
    ) -> Option<Handle> {
        self.handles(unit, scale)
            .into_iter()
            .filter(|h| distance(p, h.point) <= tolerance)
            .min_by(|a, b| {
                distance(p, a.point)
                    .total_cmp(&distance(p, b.point))
                    .then_with(|| match (a.kind, b.kind) {
                        (HandleKind::Delete, _) => std::cmp::Ordering::Less,
                        (_, HandleKind::Delete) => std::cmp::Ordering::Greater,
                        _ => std::cmp::Ordering::Equal,
                    })
            })
    }
    pub fn hit(&self, point: Point, unit: f64, tolerance: f64) -> bool {
        if matches!(self.kind, Kind::Rect | Kind::Ellipse) {
            return self.hit_shape(point, unit, tolerance);
        }
        let p = self.map_point(point, unit, true);
        let b = self.bounds(unit);
        let t = self.transform();
        if self.kind == Kind::Sequence {
            return self.sequence_hit(p, unit, tolerance / t.sx.min(t.sy).max(0.05));
        }
        let m = tolerance / t.sx.min(t.sy).max(0.05)
            + self.style.width / (2. * unit * (t.sx * t.sy).sqrt());
        let a = self.points[0];
        let z = *self.points.last().unwrap();
        if self.kind == Kind::Arrow {
            return self
                .arrow_paths(unit)
                .iter()
                .any(|path| path.hit(point, tolerance));
        }
        if self.line() {
            return segment(p, a, z) <= m;
        }
        if !b.contains(p, m) {
            return false;
        }
        if self.kind == Kind::Text {
            return true;
        }
        if self.style.fill == Fill::Rect {
            return true;
        }
        if self.style.fill == Fill::Ellipse && self.kind.supports_fill(self.style.fill) {
            return ((p[0] - b.x - b.width / 2.) / (b.width / 2.))
                .hypot((p[1] - b.y - b.height / 2.) / (b.height / 2.))
                <= 1. + m / b.width.min(b.height);
        }
        if self.points.len() == 1 {
            return distance(p, a) <= m;
        }
        self.points.windows(2).any(|v| segment(p, v[0], v[1]) <= m)
    }
    fn hit_shape(&self, point: Point, unit: f64, tolerance: f64) -> bool {
        let b = self.bounds(unit);
        let t = self.transform();
        let [cx, cy] = b.center();
        let dx = point[0] - cx - t.x;
        let dy = point[1] - cy - t.y;
        let (s, c) = t.angle.sin_cos();
        let x = (dx * c + dy * s).abs();
        let y = (-dx * s + dy * c).abs();
        let rx = b.width * t.sx.abs() / 2.;
        let ry = b.height * t.sy.abs() / 2.;
        let filled = self.style.fill == Fill::Solid;
        let margin = tolerance
            + if filled {
                0.
            } else {
                self.style.width / (2. * unit)
            };
        if x > rx + margin || y > ry + margin {
            return false;
        }
        if self.kind == Kind::Rect {
            if x <= rx && y <= ry {
                return filled || (rx - x).min(ry - y) <= margin;
            }
            return (x - rx).max(0.).hypot((y - ry).max(0.)) <= margin;
        }
        let norm = (x / rx).powi(2) + (y / ry).powi(2);
        if filled && norm <= 1. {
            return true;
        }
        // Nearest distance in world units preserves a constant stroke width on
        // highly eccentric, non-uniformly resized ellipses.
        let d = |a: f64| (rx * a.cos() - x).powi(2) + (ry * a.sin() - y).powi(2);
        let k = (5_f64.sqrt() - 1.) / 2.;
        let (mut lo, mut hi) = (0., FRAC_PI_2);
        let mut a = hi - k * (hi - lo);
        let mut z = lo + k * (hi - lo);
        let (mut fa, mut fz) = (d(a), d(z));
        for _ in 0..32 {
            if fa < fz {
                hi = z;
                z = a;
                fz = fa;
                a = hi - k * (hi - lo);
                fa = d(a);
            } else {
                lo = a;
                a = z;
                fa = fz;
                z = lo + k * (hi - lo);
                fz = d(z);
            }
        }
        fa.min(fz).min(d(0.)).min(d(FRAC_PI_2)) <= margin * margin
    }
}
pub fn resize_cursor(mark: &Mark, handle: &Handle, unit: f64, dragging: bool) -> &'static str {
    match handle.kind {
        HandleKind::Delete
        | HandleKind::Increase
        | HandleKind::Decrease
        | HandleKind::ToggleLeader => "pointer",
        HandleKind::Move => "move",
        HandleKind::Rotate => {
            if dragging {
                "grabbing"
            } else {
                "grab"
            }
        }
        _ => {
            let direction = if handle.kind == HandleKind::Endpoint {
                let n = mark.points.len();
                let idx = handle.endpoint.or(handle.index).unwrap_or(0).min(n - 1);
                let other = if idx == 0 { 1.min(n - 1) } else { idx - 1 };
                let p = mark.map_point(mark.points[idx], unit, false);
                let q = mark.map_point(mark.points[other], unit, false);
                round_reference((p[1] - q[1]).atan2(p[0] - q[0]) / FRAC_PI_4) as i32
            } else {
                round_reference(
                    (handle.index.unwrap() as f64 - 3.) + mark.transform().angle / FRAC_PI_4,
                ) as i32
            };
            [
                "e-resize",
                "se-resize",
                "s-resize",
                "sw-resize",
                "w-resize",
                "nw-resize",
                "n-resize",
                "ne-resize",
            ][direction.rem_euclid(8) as usize]
        }
    }
}
