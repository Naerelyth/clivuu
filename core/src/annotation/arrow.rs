//! Arrow geometry recovered from PixPin 3.6.6.0. The model, hit testing,
//! canvas/export renderer and option previews consume this one description.
use super::{Mark, Point};
use serde::{Deserialize, Serialize};
use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_4};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArrowType {
    #[default]
    Straight,
    StraightBilateral,
    Line,
    LineBilateral,
    Outline,
    Filled,
    DimensionLine,
    Solid,
    SolidBilateral,
    DimensionArrow,
    PlainLine,
}
impl ArrowType {
    pub const ALL: [Self; 11] = [
        Self::Straight,
        Self::StraightBilateral,
        Self::Line,
        Self::LineBilateral,
        Self::Outline,
        Self::Filled,
        Self::DimensionLine,
        Self::Solid,
        Self::SolidBilateral,
        Self::DimensionArrow,
        Self::PlainLine,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Straight => "Straight arrow",
            Self::StraightBilateral => "Straight bilateral arrow",
            Self::Line => "Line arrow",
            Self::LineBilateral => "Line bilateral arrow",
            Self::Outline => "Arrow outline",
            Self::Filled => "Arrow fill",
            Self::DimensionLine => "Dimension line",
            Self::Solid => "Solid arrow",
            Self::SolidBilateral => "Solid bilateral arrow",
            Self::DimensionArrow => "Dimension arrow",
            Self::PlainLine => "Plain line",
        }
    }
    pub fn supports_line_style(self) -> bool {
        self != Self::Filled
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineStyle {
    #[default]
    Solid,
    Dash,
    Dot,
    DashDot,
    DashDotDot,
}
impl LineStyle {
    pub const ALL: [Self; 5] = [
        Self::Solid,
        Self::Dash,
        Self::Dot,
        Self::DashDot,
        Self::DashDotDot,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid line",
            Self::Dash => "Dashed line",
            Self::Dot => "Dotted line",
            Self::DashDot => "Dash-dot line",
            Self::DashDotDot => "Dash-dot-dot line",
        }
    }
    pub fn pattern(self) -> &'static [f64] {
        match self {
            Self::Solid => &[],
            Self::Dash => &[4., 2.],
            Self::Dot => &[1., 2.],
            Self::DashDot => &[4., 2., 1., 2.],
            Self::DashDotDot => &[4., 2., 1., 2., 1., 2.],
        }
    }
}
#[derive(Clone, Debug)]
pub struct ArrowStroke {
    pub width: f64,
    pub round: bool,
    pub dashes: Vec<f64>,
}
#[derive(Clone, Debug)]
pub struct ArrowPath {
    pub points: Vec<Point>,
    pub closed: bool,
    /// None describes a filled contour; Some describes a stroked centerline.
    pub stroke: Option<ArrowStroke>,
}
impl ArrowPath {
    fn fill(points: Vec<Point>) -> Self {
        Self {
            points,
            closed: true,
            stroke: None,
        }
    }
    fn stroke(points: Vec<Point>, width: f64, round: bool, line: LineStyle, closed: bool) -> Self {
        Self {
            points,
            closed,
            stroke: Some(ArrowStroke {
                width,
                round,
                dashes: line.pattern().iter().map(|v| v * width).collect(),
            }),
        }
    }
    pub fn hit(&self, p: Point, tolerance: f64) -> bool {
        let segments = self.points.windows(2).map(|v| (v[0], v[1])).chain(
            self.closed
                .then(|| (*self.points.last().unwrap(), self.points[0])),
        );
        let margin = tolerance + self.stroke.as_ref().map_or(0., |s| s.width / 2.);
        if self.stroke.is_none() && inside(&self.points, p) {
            return true;
        }
        segments
            .into_iter()
            .any(|(a, b)| super::geometry::segment(p, a, b) <= margin)
    }
}
fn inside(points: &[Point], p: Point) -> bool {
    let mut inside = false;
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}
/// Round to the nearest 45 degrees while retaining the pointer's radius.
/// PixPin uses qRound's half-up tie rule, including negative angles.
pub fn snap_arrow(anchor: Point, point: Point) -> Point {
    let dx = point[0] - anchor[0];
    let dy = point[1] - anchor[1];
    let radius = dx.hypot(dy);
    let direction = super::geometry::round_reference(dy.atan2(dx) / FRAC_PI_4) as i32;
    let h = FRAC_1_SQRT_2;
    let [x, y] = [
        [1., 0.],
        [h, h],
        [0., 1.],
        [-h, h],
        [-1., 0.],
        [-h, -h],
        [0., -1.],
        [h, -h],
    ][direction.rem_euclid(8) as usize];
    [anchor[0] + radius * x, anchor[1] + radius * y]
}
/// Clip along the snapped ray, so an image edge cannot turn a 45° line
/// into an arbitrary angle. The anchor is already inside the drawing limit.
pub fn snap_arrow_within(anchor: Point, point: Point, limit: super::Bounds) -> Point {
    let q = snap_arrow(anchor, point);
    let delta = [q[0] - anchor[0], q[1] - anchor[1]];
    let mut factor = 1_f64;
    for (axis, min, max) in [
        (0, limit.x, limit.x + limit.width),
        (1, limit.y, limit.y + limit.height),
    ] {
        if q[axis] < min {
            factor = factor.min((min - anchor[axis]) / delta[axis]);
        }
        if q[axis] > max {
            factor = factor.min((max - anchor[axis]) / delta[axis]);
        }
    }
    [anchor[0] + delta[0] * factor, anchor[1] + delta[1] * factor]
}
/// Construct in a canonical horizontal frame, then rotate/translate once.
/// Head dimensions are proportional to stroke width, never viewport zoom.
pub fn arrow_paths(
    a: Point,
    b: Point,
    width: f64,
    kind: ArrowType,
    line: LineStyle,
) -> Vec<ArrowPath> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length = dx.hypot(dy);
    if length < 1e-9 || !length.is_finite() || !width.is_finite() || width <= 0. {
        return vec![];
    }
    let mut paths = vec![];
    if matches!(kind, ArrowType::Outline | ArrowType::Filled) {
        let c = length - 2.2 * width;
        let mut points = vec![
            [length, 0.],
            [length - 3.3 * width, 1.65 * width],
            [c, 0.],
            [length - 3.3 * width, -1.65 * width],
        ];
        if c > 0. {
            let x = c * c / (c + 0.44 * width);
            let y = 0.66 * width * x / c;
            points = vec![
                [0., 0.],
                [x, y],
                [length - 3.3 * width, 1.65 * width],
                [length, 0.],
                [length - 3.3 * width, -1.65 * width],
                [x, -y],
            ];
        }
        paths.push(if kind == ArrowType::Outline {
            ArrowPath::stroke(points, width * 0.382, true, line, true)
        } else {
            ArrowPath::fill(points)
        });
    } else {
        let bilateral = matches!(
            kind,
            ArrowType::StraightBilateral
                | ArrowType::LineBilateral
                | ArrowType::SolidBilateral
                | ArrowType::DimensionArrow
        );
        let open = matches!(kind, ArrowType::Line | ArrowType::LineBilateral);
        let block = matches!(kind, ArrowType::Solid | ArrowType::SolidBilateral);
        let dimension = matches!(kind, ArrowType::DimensionLine | ArrowType::DimensionArrow);
        let head = !matches!(kind, ArrowType::DimensionLine | ArrowType::PlainLine);
        let inset = if !head {
            0.
        } else if open {
            2.5 * width
        } else {
            2.2 * width
        };
        let start = if bilateral { inset } else { 0. };
        let end = length - inset;
        paths.push(ArrowPath::stroke(
            vec![[start, 0.], [end, 0.]],
            width,
            kind != ArrowType::DimensionLine,
            line,
            false,
        ));
        if head {
            for (tip, sign) in if bilateral {
                vec![(length, -1.), (0., 1.)]
            } else {
                vec![(length, -1.)]
            } {
                if open {
                    paths.push(ArrowPath::stroke(
                        vec![[tip + sign * 2.5 * width, 0.], [tip, 0.]],
                        width,
                        true,
                        LineStyle::Solid,
                        false,
                    ));
                    paths.push(ArrowPath::stroke(
                        vec![
                            [tip + sign * 2.5 * width, -1.5 * width],
                            [tip, 0.],
                            [tip + sign * 2.5 * width, 1.5 * width],
                        ],
                        width,
                        true,
                        LineStyle::Solid,
                        false,
                    ));
                } else {
                    let mut p = vec![[tip, 0.], [tip + sign * 3.3 * width, 1.65 * width]];
                    if !block {
                        p.push([tip + sign * 2.2 * width, 0.]);
                    }
                    p.push([tip + sign * 3.3 * width, -1.65 * width]);
                    paths.push(ArrowPath::fill(p));
                }
            }
        }
        if dimension {
            for x in [0., length] {
                paths.push(ArrowPath::fill(vec![
                    [x - 0.625 * width, -2.5 * width],
                    [x + 0.625 * width, -2.5 * width],
                    [x + 0.625 * width, 2.5 * width],
                    [x - 0.625 * width, 2.5 * width],
                ]));
            }
        }
    }
    let (c, s) = (dx / length, dy / length);
    for path in &mut paths {
        for p in &mut path.points {
            *p = [a[0] + p[0] * c - p[1] * s, a[1] + p[0] * s + p[1] * c];
        }
    }
    paths
}
fn transform_local(origin: Point, dir: Point, p: Point) -> Point {
    [
        origin[0] + p[0] * dir[0] - p[1] * dir[1],
        origin[1] + p[0] * dir[1] + p[1] * dir[0],
    ]
}
fn push_head(
    paths: &mut Vec<ArrowPath>,
    tip: Point,
    into_tip: Point,
    width: f64,
    open: bool,
    block: bool,
) {
    let map = |p: Point| transform_local(tip, into_tip, p);
    if open {
        paths.push(ArrowPath::stroke(
            vec![map([-2.5 * width, 0.]), tip],
            width,
            true,
            LineStyle::Solid,
            false,
        ));
        paths.push(ArrowPath::stroke(
            vec![
                map([-2.5 * width, -1.5 * width]),
                tip,
                map([-2.5 * width, 1.5 * width]),
            ],
            width,
            true,
            LineStyle::Solid,
            false,
        ));
    } else {
        let mut p = vec![tip, map([-3.3 * width, 1.65 * width])];
        if !block {
            p.push(map([-2.2 * width, 0.]));
        }
        p.push(map([-3.3 * width, -1.65 * width]));
        paths.push(ArrowPath::fill(p));
    }
}
fn push_dimension_tick(paths: &mut Vec<ArrowPath>, at: Point, tangent: Point, width: f64) {
    let map = |p: Point| transform_local(at, tangent, p);
    paths.push(ArrowPath::fill(vec![
        map([-0.625 * width, -2.5 * width]),
        map([0.625 * width, -2.5 * width]),
        map([0.625 * width, 2.5 * width]),
        map([-0.625 * width, 2.5 * width]),
    ]));
}
/// Multi-segment polyline arrow geometry. Two-vertex marks delegate directly to
/// `arrow_paths` so single-segment PixPin/Qt reference contours remain exact.
pub fn polyline_arrow_paths(
    points: &[Point],
    width: f64,
    kind: ArrowType,
    line: LineStyle,
) -> Vec<ArrowPath> {
    if !width.is_finite() || width <= 0. || points.iter().flatten().any(|v| !v.is_finite()) {
        return vec![];
    }
    let mut pts: Vec<Point> = Vec::with_capacity(points.len());
    for &p in points {
        if pts
            .last()
            .is_none_or(|prev| (p[0] - prev[0]).hypot(p[1] - prev[1]) >= 1e-9)
        {
            pts.push(p);
        }
    }
    if pts.len() < 2 {
        return vec![];
    }
    if pts.len() == 2 {
        return arrow_paths(pts[0], pts[1], width, kind, line);
    }
    let n = pts.len();
    let mut seg_len = Vec::with_capacity(n - 1);
    let mut tangents = Vec::with_capacity(n - 1);
    let mut normals = Vec::with_capacity(n - 1);
    let mut arc = Vec::with_capacity(n);
    arc.push(0.);
    for pair in pts.windows(2) {
        let dx = pair[1][0] - pair[0][0];
        let dy = pair[1][1] - pair[0][1];
        let len = dx.hypot(dy);
        let t = [dx / len, dy / len];
        seg_len.push(len);
        tangents.push(t);
        normals.push([-t[1], t[0]]);
        arc.push(arc.last().unwrap() + len);
    }
    let mut paths = vec![];
    if matches!(kind, ArrowType::Outline | ArrowType::Filled) {
        let last_len = *seg_len.last().unwrap();
        let last_origin = pts[n - 2];
        let last_tan = *tangents.last().unwrap();
        let c = last_len - 2.2 * width;
        let (x_neck, neck_half_w) = if c > 0. {
            let x = c * c / (c + 0.44 * width);
            (x, 0.66 * width * x / c)
        } else {
            let total = arc[n - 2].max(1e-6);
            (0., 0.66 * width * (total / (total + 2.2 * width)).min(1.))
        };
        let neck_arc = (arc[n - 2] + x_neck).max(1e-6);
        let mut left = Vec::with_capacity(n - 2);
        let mut right = Vec::with_capacity(n - 2);
        for i in 1..n - 1 {
            let w = neck_half_w * (arc[i] / neck_arc).clamp(0., 1.);
            let n0 = normals[i - 1];
            let n1 = normals[i];
            let bx = n0[0] + n1[0];
            let by = n0[1] + n1[1];
            let b2 = bx * bx + by * by;
            let miter = if b2 > 1e-6 {
                let scale = (2. / b2).min(2.5 / b2.sqrt());
                [bx * scale, by * scale]
            } else {
                n1
            };
            left.push([pts[i][0] + w * miter[0], pts[i][1] + w * miter[1]]);
            right.push([pts[i][0] - w * miter[0], pts[i][1] - w * miter[1]]);
        }
        let mut contour = Vec::with_capacity(2 * n + 4);
        contour.push(pts[0]);
        contour.extend(left);
        let map_last = |p: Point| transform_local(last_origin, last_tan, p);
        if c > 0. {
            contour.push(map_last([x_neck, neck_half_w]));
        }
        contour.push(map_last([last_len - 3.3 * width, 1.65 * width]));
        contour.push(map_last([last_len, 0.]));
        contour.push(map_last([last_len - 3.3 * width, -1.65 * width]));
        if c > 0. {
            contour.push(map_last([x_neck, -neck_half_w]));
        }
        contour.extend(right.into_iter().rev());
        paths.push(if kind == ArrowType::Outline {
            ArrowPath::stroke(contour, width * 0.382, true, line, true)
        } else {
            ArrowPath::fill(contour)
        });
        return paths;
    }
    let bilateral = matches!(
        kind,
        ArrowType::StraightBilateral
            | ArrowType::LineBilateral
            | ArrowType::SolidBilateral
            | ArrowType::DimensionArrow
    );
    let open = matches!(kind, ArrowType::Line | ArrowType::LineBilateral);
    let block = matches!(kind, ArrowType::Solid | ArrowType::SolidBilateral);
    let dimension = matches!(kind, ArrowType::DimensionLine | ArrowType::DimensionArrow);
    let head = !matches!(kind, ArrowType::DimensionLine | ArrowType::PlainLine);
    let inset = if !head {
        0.
    } else if open {
        2.5 * width
    } else {
        2.2 * width
    };
    let start_trim = if bilateral {
        inset.min(seg_len[0] * 0.9)
    } else {
        0.
    };
    let end_trim = inset.min(seg_len[n - 2] * 0.9);
    let mut shaft = Vec::with_capacity(n);
    shaft.push([
        pts[0][0] + start_trim * tangents[0][0],
        pts[0][1] + start_trim * tangents[0][1],
    ]);
    shaft.extend_from_slice(&pts[1..n - 1]);
    shaft.push([
        pts[n - 1][0] - end_trim * tangents[n - 2][0],
        pts[n - 1][1] - end_trim * tangents[n - 2][1],
    ]);
    paths.push(ArrowPath::stroke(
        shaft,
        width,
        kind != ArrowType::DimensionLine,
        line,
        false,
    ));
    if head {
        push_head(&mut paths, pts[n - 1], tangents[n - 2], width, open, block);
        if bilateral {
            push_head(
                &mut paths,
                pts[0],
                [-tangents[0][0], -tangents[0][1]],
                width,
                open,
                block,
            );
        }
    }
    if dimension {
        push_dimension_tick(&mut paths, pts[0], tangents[0], width);
        push_dimension_tick(&mut paths, pts[n - 1], tangents[n - 2], width);
    }
    paths
}
impl Mark {
    pub fn arrow_paths(&self, unit: f64) -> Vec<ArrowPath> {
        let mapping = self.point_mapping(unit);
        let mapped: Vec<Point> = self.points.iter().map(|&p| mapping.map(p, false)).collect();
        polyline_arrow_paths(
            &mapped,
            self.style.width / unit,
            self.style.arrow_type,
            self.style.line_style,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pixpin_taper_is_not_a_constant_width_triangle() {
        let path = arrow_paths([0., 0.], [100., 0.], 3., ArrowType::Filled, LineStyle::Dash);
        let p = &path[0].points;
        assert!((p[1][0] - 92.09839527027026).abs() < 1e-10);
        assert!((p[1][1] - 1.9524070945945942).abs() < 1e-10);
        assert_eq!(p[2], [90.1, 4.949999999999999]);
        assert!(path[0].stroke.is_none());
        let outline = arrow_paths(
            [0., 0.],
            [100., 0.],
            3.,
            ArrowType::Outline,
            LineStyle::Dash,
        );
        assert_eq!(outline[0].points, *p);
        assert!((outline[0].stroke.as_ref().unwrap().width - 1.146).abs() < 1e-10);
    }
    #[test]
    fn native_menu_order_and_qt_dash_factors_are_preserved() {
        assert_eq!(ArrowType::ALL.len(), 11);
        assert_eq!(ArrowType::ALL[10], ArrowType::PlainLine);
        assert_eq!(LineStyle::Dash.pattern(), [4., 2.]);
        assert!(ArrowType::Outline.supports_line_style());
        assert!(!ArrowType::Filled.supports_line_style());
        assert!(ArrowType::Solid.supports_line_style());
    }
    #[test]
    fn all_directions_keep_shape_width_and_independent_endpoint_handles() {
        use std::sync::Arc;
        for kind in ArrowType::ALL {
            for (a, b) in [
                ([0., 0.], [100., 0.]),
                ([0., 0.], [0., 100.]),
                ([50., 50.], [-50., 50.]),
            ] {
                let paths = arrow_paths(a, b, 3., kind, LineStyle::Solid);
                assert!(!paths.is_empty());
                assert!(
                    paths
                        .iter()
                        .flat_map(|p| p.points.iter().flatten())
                        .all(|v| v.is_finite())
                );
                let mut mark = Mark::new(super::super::Kind::Arrow, a, 1.);
                mark.points = Arc::new(vec![a, b]);
                mark.style.arrow_type = kind;
                assert_eq!(mark.handles(1., 1.).len(), 2);
                assert!(mark.hit(b, 1., 2.));
                assert!(!mark.hit([1000., 1000.], 1., 1.));
            }
        }
    }
    #[test]
    fn plain_lines_have_only_a_full_length_stroke_and_preserve_every_line_style() {
        for line in LineStyle::ALL {
            let paths = arrow_paths([10., 20.], [110., 20.], 4., ArrowType::PlainLine, line);
            assert_eq!(paths.len(), 1);
            assert_eq!(paths[0].points, vec![[10., 20.], [110., 20.]]);
            assert!(!paths[0].closed);
            let stroke = paths[0].stroke.as_ref().unwrap();
            assert!(stroke.round);
            assert_eq!(stroke.width, 4.);
            assert_eq!(
                stroke.dashes,
                line.pattern().iter().map(|v| v * 4.).collect::<Vec<_>>()
            );
            assert!(!paths[0].hit([110., 27.], 0.));
        }
        let old:Mark=serde_json::from_value(serde_json::json!({"kind":"arrow","points":[[0,0],[100,0]],"startStyle":"none","endStyle":"none"})).unwrap();
        assert_eq!(old.style.arrow_type, ArrowType::PlainLine);
        let out = serde_json::to_value(&old).unwrap();
        let imported: Mark = serde_json::from_value(out).unwrap();
        assert_eq!(imported, old);
    }
    #[test]
    fn imports_normalize_legacy_endpoint_state_without_serializing_it_again() {
        let mark:Mark=serde_json::from_value(serde_json::json!({"kind":"arrow","points":[[0,0],[100,0]],"startStyle":"open","endStyle":"open"})).unwrap();
        assert_eq!(mark.style.arrow_type, ArrowType::LineBilateral);
        let out = serde_json::to_value(mark).unwrap();
        assert_eq!(out["arrowType"], "line-bilateral");
        for key in ["arrowStart", "arrowEnd", "startStyle", "endStyle"] {
            assert!(out.get(key).is_none());
        }
    }
    #[test]
    fn start_only_legacy_arrows_reverse_endpoints_without_losing_direction() {
        let mark:Mark=serde_json::from_value(serde_json::json!({"kind":"arrow","points":[[7,11],[107,41]],"startStyle":"open","endStyle":"none"})).unwrap();
        assert_eq!(mark.style.arrow_type, ArrowType::Line);
        assert_eq!(&*mark.points, &[[107., 41.], [7., 11.]]);
        let out = serde_json::to_value(mark).unwrap();
        assert_eq!(out["arrowType"], "line");
        assert!(out.get("startStyle").is_none());
    }
    #[test]
    fn snapping_retains_radius_and_matches_negative_half_up_ties() {
        for n in -16..=16 {
            let angle = (n as f64 + 0.1) * FRAC_PI_4;
            let p = snap_arrow(
                [37., -19.],
                [37. + 73. * angle.cos(), -19. + 73. * angle.sin()],
            );
            let expected = n as f64 * FRAC_PI_4;
            assert!((p[0] - (37. + 73. * expected.cos())).abs() < 1e-10);
            assert!((p[1] - (-19. + 73. * expected.sin())).abs() < 1e-10);
        }
        assert_eq!(snap_arrow([3., 4.], [3., 4.]), [3., 4.]);
    }
    #[test]
    fn shift_clips_along_the_ray_and_keeps_axis_arrows_on_each_image_edge() {
        let limit = super::super::Bounds {
            x: 0.,
            y: 0.,
            width: 100.,
            height: 100.,
        };
        let clipped = snap_arrow_within([95., 50.], [100., 60.], limit);
        assert_eq!(clipped[0], 100.);
        assert!((clipped[1] - 55.).abs() < 1e-10);
        for (a, b) in [
            ([100., 100.], [0., 100.]),
            ([0., 100.], [0., 0.]),
            ([0., 0.], [100., 0.]),
            ([100., 0.], [100., 100.]),
        ] {
            assert_eq!(snap_arrow_within(a, b, limit), b);
        }
    }
    #[test]
    fn shift_on_a_stationary_endpoint_snaps_and_undoes_once() {
        use super::super::{EditorSession, Kind};
        use std::sync::Arc;
        let mut session = EditorSession::new(1., 1.);
        let mut m = Mark::new(Kind::Arrow, [0., 0.], 1.);
        m.points = Arc::new(vec![[0., 0.], [100., 30.]]);
        session.add(m.clone());
        assert!(session.begin([100., 30.], Some(Kind::Arrow), 1.));
        session.preview([100., 30.], true);
        assert_eq!(session.current().unwrap().points[1][1], 0.);
        session.preview([100., 30.], false);
        assert_eq!(session.current(), Some(&m));
        session.preview([100., 30.], true);
        session.end();
        session.undo();
        assert_eq!(session.current(), Some(&m));
        session.redo();
        assert_eq!(session.current().unwrap().points[1][1], 0.);
    }
    #[test]
    fn polyline_arrows_preserve_all_eleven_types_handles_hit_testing_and_json_roundtrip() {
        use std::sync::Arc;
        let vertices = vec![[10., 20.], [90., 20.], [90., 100.], [180., 100.]];
        for kind in ArrowType::ALL {
            for line in LineStyle::ALL {
                let mut mark = Mark::new(super::super::Kind::Arrow, vertices[0], 1.);
                mark.points = Arc::new(vertices.clone());
                mark.style.arrow_type = kind;
                mark.style.line_style = line;
                let paths = mark.arrow_paths(1.);
                assert!(!paths.is_empty());
                assert!(
                    paths
                        .iter()
                        .flat_map(|p| p.points.iter().flatten())
                        .all(|v| v.is_finite())
                );
                match kind {
                    ArrowType::Filled => {
                        assert_eq!(paths.len(), 1);
                        assert!(paths[0].closed && paths[0].stroke.is_none());
                        assert!(paths[0].points.len() > 6);
                    }
                    ArrowType::Outline => {
                        assert_eq!(paths.len(), 1);
                        assert!(paths[0].closed && paths[0].stroke.is_some());
                        if line == LineStyle::Dash {
                            assert!(!paths[0].stroke.as_ref().unwrap().dashes.is_empty());
                        }
                    }
                    ArrowType::PlainLine => {
                        assert_eq!(paths.len(), 1);
                        assert_eq!(paths[0].points, vertices);
                    }
                    _ => {}
                }
                let handles = mark.handles(1., 1.);
                assert_eq!(handles.len(), 4);
                assert!(
                    handles
                        .iter()
                        .enumerate()
                        .all(|(i, h)| h.kind == super::super::HandleKind::Endpoint
                            && h.endpoint == Some(i)
                            && h.point == vertices[i])
                );
                assert!(mark.hit([50., 20.], 1., 2.));
                assert!(mark.hit([90., 60.], 1., 2.));
                assert!(mark.hit([135., 100.], 1., 2.));
                assert!(!mark.hit([40., 70.], 1., 1.));
                let roundtripped: Mark =
                    serde_json::from_value(serde_json::to_value(&mark).unwrap()).unwrap();
                assert_eq!(*roundtripped.points, vertices);
            }
        }
    }
}
