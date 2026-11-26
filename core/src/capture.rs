//! Capture selection geometry in compositor logical coordinates. Desktop
//! adapters supply ordered windows/outputs and translate native input only.
use crate::annotation::{Bounds, Point};
use serde::{Deserialize, Serialize};
/// The operation being selected, independent of image output destinations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Purpose {
    #[default]
    Screenshot,
    /// An already captured image, with no reliable desktop source coordinates.
    Image,
    Scrolling,
    Recording,
}
impl Purpose {
    pub fn editable(self) -> bool {
        matches!(self, Self::Screenshot | Self::Image)
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Output {
    #[serde(flatten)]
    pub rect: Bounds,
    pub scale: f64,
}
/// The frozen desktop can use a uniform backing scale greater than one
/// monitor's native density. Keep source coordinates separate from output size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterRegion {
    pub source: Bounds,
    pub size: [u32; 2],
}

pub fn raster_region(
    outputs: &[Output],
    limit: Bounds,
    image: [u32; 2],
    region: Bounds,
) -> Option<RasterRegion> {
    if !valid(limit)
        || !valid(region)
        || image.contains(&0)
        || region.x < limit.x
        || region.y < limit.y
        || region.x + region.width > limit.x + limit.width
        || region.y + region.height > limit.y + limit.height
    {
        return None;
    }
    let sx = image[0] as f64 / limit.width;
    let sy = image[1] as f64 / limit.height;
    if (image[1] as f64 - limit.height * sx).abs() > 2. {
        return None;
    }
    let native = outputs
        .iter()
        .filter_map(|o| {
            let overlap = intersect(o.rect, region);
            (overlap.width > 0. && overlap.height > 0.).then_some(o.scale)
        })
        .reduce(f64::max)
        .unwrap_or(sx);
    let scale = native.min(sx).min(sy);
    if !scale.is_finite() || scale <= 0. {
        return None;
    }
    let x = ((region.x - limit.x) * sx).floor().max(0.);
    let y = ((region.y - limit.y) * sy).floor().max(0.);
    let right = ((region.x + region.width - limit.x) * sx)
        .ceil()
        .min(image[0] as f64);
    let bottom = ((region.y + region.height - limit.y) * sy)
        .ceil()
        .min(image[1] as f64);
    let source = Bounds {
        x,
        y,
        width: right - x,
        height: bottom - y,
    };
    let size = if (scale - sx).abs() < 1e-6 && (scale - sy).abs() < 1e-6 {
        [source.width as u32, source.height as u32]
    } else {
        [
            (region.width * scale).round().max(1.) as u32,
            (region.height * scale).round().max(1.) as u32,
        ]
    };
    (valid(source) && u64::from(size[0]) * u64::from(size[1]) <= crate::image_ops::MAX_PIXELS)
        .then_some(RasterRegion { source, size })
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DragKind {
    New,
    Move,
    Resize,
    Expand,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Drag {
    pub kind: DragKind,
    pub point: Point,
    pub rect: Bounds,
    pub edge: [bool; 4],
    selected: bool,
}
#[derive(Clone, Debug)]
pub struct Selection {
    pub rect: Bounds,
    pub selected: bool,
    pub pointer: Option<Point>,
    pub drag: Option<Drag>,
    pub detect_windows: bool,
    pub limit: Bounds,
    pub outputs: Vec<Output>,
    windows: Vec<Bounds>,
    level: usize,
}
fn clamp(value: f64, lo: f64, hi: f64) -> f64 {
    value.max(lo).min(hi.max(lo))
}
fn round(value: f64) -> f64 {
    (value + 0.5).floor()
}
pub fn contains(r: Bounds, p: Point) -> bool {
    p[0] >= r.x && p[0] < r.x + r.width && p[1] >= r.y && p[1] < r.y + r.height
}
fn valid(r: Bounds) -> bool {
    [r.x, r.y, r.width, r.height, r.x + r.width, r.y + r.height]
        .iter()
        .all(|v| v.is_finite())
        && r.width > 0.
        && r.height > 0.
}

pub fn intersect(a: Bounds, b: Bounds) -> Bounds {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let max_right = (a.x + a.width).min(b.x + b.width).max(x);
    let max_bottom = (a.y + a.height).min(b.y + b.height).max(y);
    let mut width = max_right - x;
    if x + width > max_right {
        width = width.next_down().max(0.);
    }
    let mut height = max_bottom - y;
    if y + height > max_bottom {
        height = height.next_down().max(0.);
    }
    Bounds {
        x,
        y,
        width,
        height,
    }
}
/// Four disjoint bands outside a crop, shared by the image and annotation mask.
pub fn outside(r: Bounds, limit: Bounds) -> [Bounds; 4] {
    let r = intersect(r, limit);
    let right = r.x + r.width;
    let bottom = r.y + r.height;
    [
        Bounds {
            x: limit.x,
            y: limit.y,
            width: limit.width,
            height: r.y - limit.y,
        },
        Bounds {
            x: limit.x,
            y: bottom,
            width: limit.width,
            height: limit.y + limit.height - bottom,
        },
        Bounds {
            x: limit.x,
            y: r.y,
            width: r.x - limit.x,
            height: r.height,
        },
        Bounds {
            x: right,
            y: r.y,
            width: limit.x + limit.width - right,
            height: r.height,
        },
    ]
}
pub fn resize_edge(r: Bounds, p: Point, margin: f64) -> [bool; 4] {
    if p[0] < r.x - margin
        || p[1] < r.y - margin
        || p[0] > r.x + r.width + margin
        || p[1] > r.y + r.height + margin
    {
        return [false; 4];
    }
    let w = (p[0] - r.x).abs();
    let e = (p[0] - r.x - r.width).abs();
    let n = (p[1] - r.y).abs();
    let s = (p[1] - r.y - r.height).abs();
    [
        w.min(e) <= margin && w < e,
        n.min(s) <= margin && n < s,
        w.min(e) <= margin && w >= e,
        n.min(s) <= margin && n >= s,
    ]
}
impl Selection {
    pub fn new(outputs: Vec<Output>, windows: Vec<Bounds>, preset: Option<Bounds>) -> Option<Self> {
        if outputs.is_empty()
            || outputs
                .iter()
                .any(|o| !valid(o.rect) || !o.scale.is_finite() || o.scale <= 0.)
        {
            return None;
        }
        let x = outputs
            .iter()
            .map(|o| o.rect.x)
            .fold(f64::INFINITY, f64::min);
        let y = outputs
            .iter()
            .map(|o| o.rect.y)
            .fold(f64::INFINITY, f64::min);
        let right = outputs
            .iter()
            .map(|o| o.rect.x + o.rect.width)
            .fold(f64::NEG_INFINITY, f64::max);
        let bottom = outputs
            .iter()
            .map(|o| o.rect.y + o.rect.height)
            .fold(f64::NEG_INFINITY, f64::max);
        let limit = Bounds {
            x,
            y,
            width: right - x,
            height: bottom - y,
        };
        let preset = preset
            .filter(|r| valid(*r))
            .map(|r| intersect(r, limit))
            .filter(|r| valid(*r));
        Some(Self {
            rect: preset.unwrap_or(limit),
            selected: preset.is_some(),
            pointer: None,
            drag: None,
            detect_windows: true,
            limit,
            outputs,
            windows: windows.into_iter().filter(|r| valid(*r)).collect(),
            level: 0,
        })
    }
    pub fn output_at(&self, p: Point) -> usize {
        if let Some(index) = self.outputs.iter().position(|o| contains(o.rect, p)) {
            return index;
        }
        let distance = |r: Bounds| {
            (clamp(p[0], r.x, r.x + r.width) - p[0]).powi(2)
                + (clamp(p[1], r.y, r.y + r.height) - p[1]).powi(2)
        };
        self.outputs.iter().enumerate().fold(0, |best, (i, o)| {
            if distance(o.rect) < distance(self.outputs[best].rect) {
                i
            } else {
                best
            }
        })
    }
    fn boundary(&self, p: Point) -> [Option<f64>; 2] {
        let index = self.output_at(p);
        let output = self.outputs[index];
        let m = output.rect;
        let step = 1. / output.scale;
        let right = m.x + m.width;
        let bottom = m.y + m.height;
        let exposed = |p| {
            !self
                .outputs
                .iter()
                .enumerate()
                .any(|(i, o)| i != index && contains(o.rect, p))
        };
        [
            if p[0] <= m.x + step && exposed([m.x - step, p[1]]) {
                Some(m.x)
            } else if p[0] >= right - step && exposed([right, p[1]]) {
                Some(right)
            } else {
                None
            },
            if p[1] <= m.y + step && exposed([p[0], m.y - step]) {
                Some(m.y)
            } else if p[1] >= bottom - step && exposed([p[0], bottom]) {
                Some(bottom)
            } else {
                None
            },
        ]
    }
    pub fn boundary_point(&self, p: Point) -> Point {
        let e = self.boundary(p);
        [e[0].unwrap_or(p[0]), e[1].unwrap_or(p[1])]
    }
    fn scale_at(&self, p: Point) -> f64 {
        let scale = self.outputs[self.output_at(p)].scale;
        if scale.is_finite() && scale > 0. {
            scale
        } else {
            1.
        }
    }
    fn snap_at(&self, p: Point) -> Point {
        let s = self.scale_at(p);
        p.map(|v| (v * s + 0.5).floor() / s)
    }
    fn bounds_from_points_scaled(&self, a: Point, b: Point, scale: f64) -> Bounds {
        let s = if scale.is_finite() && scale > 0. {
            scale
        } else {
            1.
        };
        let step = 1. / s;
        let snap = |v: f64| (v * s + 0.5).floor() / s;
        intersect(
            Bounds {
                x: snap(a[0].min(b[0])),
                y: snap(a[1].min(b[1])),
                width: snap((a[0] - b[0]).abs()).max(step),
                height: snap((a[1] - b[1]).abs()).max(step),
            },
            self.limit,
        )
    }
    fn hover(&mut self, p: Point) {
        let window = self
            .detect_windows
            .then(|| self.windows.iter().copied().find(|r| contains(*r, p)))
            .flatten();
        let monitor = self.outputs[self.output_at(p)].rect;
        let choices: Vec<_> = window.into_iter().chain([monitor, self.limit]).collect();
        self.rect = intersect(choices[self.level.min(choices.len() - 1)], self.limit);
    }
    pub fn motion(&mut self, p: Point) {
        if !p.iter().all(|v| v.is_finite()) {
            return;
        }
        self.pointer = Some(p);
        if let Some(d) = self.drag {
            let scale = self.scale_at(p);
            let q = self.snap_at(p);
            let dx = q[0] - d.point[0];
            let dy = q[1] - d.point[1];
            let r = d.rect;
            self.rect = match d.kind {
                DragKind::Move => {
                    let snap = |v: f64| (v * scale + 0.5).floor() / scale;
                    intersect(
                        Bounds {
                            x: clamp(
                                snap(r.x + dx),
                                self.limit.x,
                                self.limit.x + self.limit.width - r.width,
                            ),
                            y: clamp(
                                snap(r.y + dy),
                                self.limit.y,
                                self.limit.y + self.limit.height - r.height,
                            ),
                            ..r
                        },
                        self.limit,
                    )
                }
                DragKind::Resize => {
                    let [w, n, e, s] = d.edge;
                    let edges = self.boundary(p);
                    let dx = p[0] - d.point[0];
                    let dy = p[1] - d.point[1];
                    self.bounds_from_points_scaled(
                        [
                            if w { edges[0].unwrap_or(r.x + dx) } else { r.x },
                            if n { edges[1].unwrap_or(r.y + dy) } else { r.y },
                        ],
                        [
                            if e {
                                edges[0].unwrap_or(r.x + r.width + dx)
                            } else {
                                r.x + r.width
                            },
                            if s {
                                edges[1].unwrap_or(r.y + r.height + dy)
                            } else {
                                r.y + r.height
                            },
                        ],
                        scale,
                    )
                }
                DragKind::New if dx.hypot(dy) >= 1. => self.bounds_from_points_scaled(
                    self.boundary_point(d.point),
                    self.boundary_point(p),
                    scale,
                ),
                DragKind::New => r,
                DragKind::Expand => self.expanded(d.rect, p),
            };
        } else if !self.selected {
            self.hover(p);
        }
    }
    pub fn begin(&mut self, p: Point, margin: f64, force_new: bool) {
        if !p.iter().all(|v| v.is_finite()) {
            return;
        }
        self.pointer = Some(p);
        let edge = if self.selected {
            resize_edge(self.rect, p, margin)
        } else {
            [false; 4]
        };
        let kind = if edge.iter().any(|v| *v) {
            DragKind::Resize
        } else if self.selected && !force_new && contains(self.rect, p) {
            DragKind::Move
        } else if self.selected && !force_new {
            DragKind::Expand
        } else {
            DragKind::New
        };
        self.drag = Some(Drag {
            kind,
            point: self.snap_at(p),
            rect: self.rect,
            edge,
            selected: self.selected,
        });
        if kind == DragKind::Expand {
            self.rect = self.expanded(self.rect, p);
        }
    }
    /// Feedback for the same geometry operation that a press would perform.
    /// Outside clicks expand the facing edges, including both edges at corners.
    pub fn cursor(&self, p: Point, margin: f64) -> &'static str {
        if !self.selected || self.drag.is_some_and(|d| d.kind == DragKind::New) {
            return "crosshair";
        }
        if self.drag.is_some_and(|d| d.kind == DragKind::Move) {
            return "move";
        }
        let edge = if let Some(d) = self.drag.filter(|d| d.kind == DragKind::Resize) {
            d.edge
        } else {
            // An expand press already updates self.rect. Use its original
            // rectangle so the cursor does not turn into a move cursor mid-drag.
            let r = self.drag.map_or(self.rect, |d| d.rect);
            let edge = resize_edge(r, p, margin);
            if edge.iter().any(|v| *v) {
                edge
            } else {
                [
                    p[0] < r.x,
                    p[1] < r.y,
                    p[0] >= r.x + r.width,
                    p[1] >= r.y + r.height,
                ]
            }
        };
        match edge {
            [true, true, ..] | [false, false, true, true] => "nwse-resize",
            [true, false, false, true] | [false, true, true, false] => "nesw-resize",
            [true, false, false, false] | [false, false, true, false] => "ew-resize",
            [false, true, false, false] | [false, false, false, true] => "ns-resize",
            _ => "move",
        }
    }
    fn expanded(&self, r: Bounds, p: Point) -> Bounds {
        let p = self.boundary_point(p).map(round);
        self.bounds_from_points_scaled(
            [r.x.min(p[0]), r.y.min(p[1])],
            [
                (r.x + r.width).max(p[0] + 1.),
                (r.y + r.height).max(p[1] + 1.),
            ],
            1.,
        )
    }
    pub fn end(&mut self) {
        if self.drag.take().is_some() {
            self.selected = true;
        }
    }
    pub fn cancel_drag(&mut self) {
        if let Some(d) = self.drag.take() {
            self.rect = d.rect;
            self.selected = d.selected;
        }
    }
    pub fn clear(&mut self) {
        self.drag = None;
        self.selected = false;
        self.level = 0;
        if let Some(p) = self.pointer {
            self.hover(p);
        }
    }
    pub fn scroll_target(&mut self, delta: i32) {
        if self.selected {
            return;
        }
        self.level = if delta < 0 {
            self.level.saturating_sub(1)
        } else {
            self.level.saturating_add(usize::from(delta > 0))
        };
        if let Some(p) = self.pointer {
            self.hover(p);
        }
    }
    pub fn cycle_output(&mut self, backwards: bool) {
        let current = self.output_at(self.rect.center());
        let len = self.outputs.len();
        self.rect = self.outputs[(current + if backwards { len - 1 } else { 1 }) % len].rect;
        self.selected = true;
    }
    pub fn fullscreen(&mut self) {
        self.rect = self.limit;
        self.selected = true;
    }
    pub fn nudge(&mut self, delta: Point, resize: bool) {
        let r = self.rect;
        self.rect = intersect(
            if resize {
                Bounds {
                    width: clamp(
                        r.width + delta[0],
                        2.,
                        self.limit.x + self.limit.width - r.x,
                    ),
                    height: clamp(
                        r.height + delta[1],
                        2.,
                        self.limit.y + self.limit.height - r.y,
                    ),
                    ..r
                }
            } else {
                Bounds {
                    x: clamp(
                        r.x + delta[0],
                        self.limit.x,
                        self.limit.x + self.limit.width - r.width,
                    ),
                    y: clamp(
                        r.y + delta[1],
                        self.limit.y,
                        self.limit.y + self.limit.height - r.height,
                    ),
                    ..r
                }
            },
            self.limit,
        );
        self.selected = true;
    }
    /// Resize the edge named by the arrow, in source pixels rather than
    /// compositor logical units. Shrinking moves that same edge inward.
    pub fn adjust_edge(&mut self, direction: Point, shrink: bool, pixel: Point) {
        let mut r = self.rect;
        let amount = if shrink { -1. } else { 1. };
        if direction[0] < 0. {
            let d = (amount * pixel[0]).clamp((pixel[0] - r.width).min(0.), r.x - self.limit.x);
            r.x -= d;
            r.width += d;
        } else if direction[0] > 0. {
            r.width += (amount * pixel[0]).clamp(
                (pixel[0] - r.width).min(0.),
                self.limit.x + self.limit.width - r.x - r.width,
            );
        } else if direction[1] < 0. {
            let d = (amount * pixel[1]).clamp((pixel[1] - r.height).min(0.), r.y - self.limit.y);
            r.y -= d;
            r.height += d;
        } else if direction[1] > 0. {
            r.height += (amount * pixel[1]).clamp(
                (pixel[1] - r.height).min(0.),
                self.limit.y + self.limit.height - r.y - r.height,
            );
        }
        self.rect = intersect(r, self.limit);
        self.selected = true;
        self.drag = None;
    }
    pub fn show_lens(&self) -> bool {
        !self.selected
            || self
                .drag
                .is_some_and(|d| matches!(d.kind, DragKind::New | DragKind::Resize))
    }
    /// Selection-mode magnifier visibility, including stationary hover inside
    /// a completed selection while hiding during move or outside expansion.
    pub fn show_lens_in_select(&self) -> bool {
        self.show_lens()
            || (self.drag.is_none() && self.pointer.is_some_and(|p| contains(self.rect, p)))
    }
    pub fn lens_pixel(&self, dimensions: [u32; 2]) -> Option<[u32; 2]> {
        if dimensions.contains(&0) {
            return None;
        }
        let p = self.pointer.unwrap_or([self.rect.x, self.rect.y]);
        let r = self.limit;
        let mut pixel = [
            clamp(
                ((p[0] - r.x) * f64::from(dimensions[0]) / r.width).floor(),
                0.,
                f64::from(dimensions[0] - 1),
            ),
            clamp(
                ((p[1] - r.y) * f64::from(dimensions[1]) / r.height).floor(),
                0.,
                f64::from(dimensions[1] - 1),
            ),
        ];
        if let Some(d) = self
            .drag
            .filter(|d| matches!(d.kind, DragKind::New | DragKind::Resize))
        {
            let new = d.kind == DragKind::New;
            let sides = if new {
                [true, true]
            } else {
                [d.edge[0] || d.edge[2], d.edge[1] || d.edge[3]]
            };
            let opposite = if new {
                d.point
            } else {
                [
                    if d.edge[0] {
                        d.rect.x + d.rect.width
                    } else {
                        d.rect.x
                    },
                    if d.edge[1] {
                        d.rect.y + d.rect.height
                    } else {
                        d.rect.y
                    },
                ]
            };
            let moving = if new {
                p
            } else {
                [
                    (if d.edge[0] {
                        d.rect.x
                    } else {
                        d.rect.x + d.rect.width
                    }) + p[0]
                        - d.point[0],
                    (if d.edge[1] {
                        d.rect.y
                    } else {
                        d.rect.y + d.rect.height
                    }) + p[1]
                        - d.point[1],
                ]
            };
            for axis in 0..2 {
                if !sides[axis] {
                    continue;
                }
                let origin = if axis == 0 { r.x } else { r.y };
                let scale =
                    f64::from(dimensions[axis]) / if axis == 0 { r.width } else { r.height };
                let first = if axis == 0 { self.rect.x } else { self.rect.y };
                let length = if axis == 0 {
                    self.rect.width
                } else {
                    self.rect.height
                };
                let far = moving[axis] >= opposite[axis];
                pixel[axis] = clamp(
                    round((first + if far { length } else { 0. } - origin) * scale)
                        - f64::from(u8::from(far)),
                    0.,
                    f64::from(dimensions[axis] - 1),
                );
            }
        }
        Some(pixel.map(|v| v as u32))
    }
    pub fn lens_rect(&self, dimensions: [u32; 2], pixel: [u32; 2]) -> Bounds {
        let r = self.rect;
        let l = self.limit;
        let sx = f64::from(dimensions[0]) / l.width;
        let sy = f64::from(dimensions[1]) / l.height;
        let x = round((r.x - l.x) * sx);
        let y = round((r.y - l.y) * sy);
        Bounds {
            x: x - f64::from(pixel[0]) + 8.,
            y: y - f64::from(pixel[1]) + 6.,
            width: round((r.x + r.width - l.x) * sx) - x,
            height: round((r.y + r.height - l.y) * sy) - y,
        }
    }
}

/// Most recent first, unique by exact compositor logical rectangle.
pub fn remember_area(areas: &mut Vec<Bounds>, rect: Bounds, maximum: usize) {
    areas.retain(|r| *r != rect);
    if maximum > 0 {
        areas.insert(0, rect);
    }
    areas.truncate(maximum);
}
pub fn history_area(length: usize, index: isize, delta: isize, cycle: bool) -> Option<usize> {
    if length == 0 {
        return None;
    }
    let next = index.saturating_add(delta);
    Some(if cycle {
        next.rem_euclid(length as isize) as usize
    } else {
        next.clamp(0, length as isize - 1) as usize
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn selection() -> Selection {
        Selection::new(
            vec![
                Output {
                    rect: Bounds {
                        x: -1280.,
                        y: 0.,
                        width: 1280.,
                        height: 800.,
                    },
                    scale: 1.,
                },
                Output {
                    rect: Bounds {
                        x: 0.,
                        y: -100.,
                        width: 1920.,
                        height: 1080.,
                    },
                    scale: 2.,
                },
            ],
            vec![Bounds {
                x: -1000.,
                y: 100.,
                width: 600.,
                height: 400.,
            }],
            None,
        )
        .unwrap()
    }
    #[test]
    fn edge_keys_use_source_pixels_in_each_direction_and_allow_tiny_edge_selections() {
        let mut s = Selection::new(
            vec![Output {
                rect: Bounds {
                    x: -100.,
                    y: -100.,
                    width: 1000.,
                    height: 800.,
                },
                scale: 1.,
            }],
            vec![],
            None,
        )
        .unwrap();
        for pixel in [0.5, 0.8, 1., 2.] {
            let r = Bounds {
                x: 100.,
                y: 100.,
                width: 100.,
                height: 80.,
            };
            for d in [[-1., 0.], [1., 0.], [0., -1.], [0., 1.]] {
                s.rect = r;
                s.adjust_edge(d, false, [pixel, pixel]);
                assert!((s.rect.width - r.width - pixel * d[0].abs()).abs() < 1e-8);
                assert!((s.rect.height - r.height - pixel * d[1].abs()).abs() < 1e-8);
                s.adjust_edge(d, true, [pixel, pixel]);
                assert_eq!(s.rect, r);
            }
        }
        s.rect = Bounds {
            x: -100.,
            y: -100.,
            width: 1.,
            height: 1.,
        };
        for d in [[-1., 0.], [0., -1.]] {
            s.adjust_edge(d, false, [2., 2.]);
            s.adjust_edge(d, true, [2., 2.]);
            assert_eq!(s.rect.width, 1.);
            assert_eq!(s.rect.height, 1.);
        }
    }
    #[test]
    fn outside_cursor_matches_expanding_edges_and_stays_consistent_during_drag() {
        let mut s = selection();
        s.rect = Bounds {
            x: 100.,
            y: 200.,
            width: 300.,
            height: 200.,
        };
        s.selected = true;
        for (p, expected) in [
            ([250., 100.], "ns-resize"),
            ([250., 500.], "ns-resize"),
            ([0., 300.], "ew-resize"),
            ([500., 300.], "ew-resize"),
            ([0., 100.], "nwse-resize"),
            ([500., 500.], "nwse-resize"),
            ([500., 100.], "nesw-resize"),
            ([0., 500.], "nesw-resize"),
        ] {
            assert_eq!(s.cursor(p, 8.), expected);
            s.begin(p, 8., false);
            assert_eq!(s.drag.unwrap().kind, DragKind::Expand);
            assert_eq!(s.cursor(p, 8.), expected);
            s.cancel_drag();
        }
        assert_eq!(s.cursor([250., 300.], 8.), "move");
        s.begin([250., 200.], 8., false);
        assert_eq!(s.cursor([250., 260.], 8.), "ns-resize");
        s.cancel_drag();
        s.selected = false;
        assert_eq!(s.cursor([0., 100.], 8.), "crosshair");
    }
    #[test]
    fn outside_press_expands_only_the_facing_edges_without_a_magnifier() {
        let original = Bounds {
            x: 100.,
            y: 200.,
            width: 300.,
            height: 200.,
        };
        for (point, expected) in [
            ([50., 300.], [50., 200., 400., 400.]),
            ([500., 300.], [100., 200., 501., 400.]),
            ([200., 100.], [100., 100., 400., 400.]),
            ([200., 500.], [100., 200., 400., 501.]),
            ([50., 100.], [50., 100., 400., 400.]),
            ([500., 100.], [100., 100., 501., 400.]),
            ([50., 500.], [50., 200., 400., 501.]),
            ([500., 500.], [100., 200., 501., 501.]),
        ] {
            let mut s = selection();
            s.rect = original;
            s.selected = true;
            s.begin(point, 8., false);
            assert_eq!(s.drag.unwrap().kind, DragKind::Expand);
            assert!(!s.show_lens());
            assert_eq!(
                [
                    s.rect.x,
                    s.rect.y,
                    s.rect.x + s.rect.width,
                    s.rect.y + s.rect.height
                ],
                expected
            );
            s.motion([80., 300.]);
            assert_eq!(
                s.rect,
                Bounds {
                    x: 80.,
                    width: 320.,
                    ..original
                }
            );
            s.motion([200., 300.]);
            assert_eq!(s.rect, original);
            s.cancel_drag();
            assert_eq!(s.rect, original);
            assert!(s.selected && !s.show_lens());
        }
        let mut s = selection();
        s.rect = original;
        s.selected = true;
        s.begin([-1279., 300.], 8., false);
        s.motion([1919.5, 979.5]);
        s.end();
        assert_eq!(
            s.rect,
            Bounds {
                x: 100.,
                y: 200.,
                width: 1820.,
                height: 780.
            }
        );
        assert!(!s.show_lens());
        s.begin([-500., 300.], 8., true);
        assert_eq!(s.drag.unwrap().kind, DragKind::New);
    }
    #[test]
    fn exposed_edges_reach_exclusive_bounds_without_snapping_internal_monitor_seams() {
        let s = selection();
        assert_eq!(s.boundary_point([-1279., 200.]), [-1280., 200.]);
        assert_eq!(s.boundary_point([1919.5, 200.]), [1920., 200.]);
        assert_eq!(s.boundary_point([-0.5, 200.]), [-0.5, 200.]);
        assert_eq!(s.boundary_point([0.25, 200.]), [0.25, 200.]);
    }
    #[test]
    fn smart_window_choice_and_monitor_cycle_share_selection_and_lens_lifecycle() {
        let mut s = selection();
        s.motion([-700., 200.]);
        assert_eq!(
            s.rect,
            Bounds {
                x: -1000.,
                y: 100.,
                width: 600.,
                height: 400.
            }
        );
        assert!(s.show_lens());
        s.begin([-700., 200.], 8., false);
        s.end();
        assert!(s.selected && !s.show_lens());
        s.begin([-700., 200.], 8., false);
        s.motion([-600., 250.]);
        assert_eq!(s.drag.unwrap().kind, DragKind::Move);
        assert!(!s.show_lens());
        s.cancel_drag();
        s.cycle_output(false);
        assert_eq!(s.rect, s.outputs[1].rect);
    }
    #[test]
    fn resize_lens_tracks_the_last_included_pixel_and_preserves_the_grab_offset() {
        let mut s = Selection::new(
            vec![Output {
                rect: Bounds {
                    x: 0.,
                    y: 0.,
                    width: 1920.,
                    height: 1080.,
                },
                scale: 1.,
            }],
            vec![],
            Some(Bounds {
                x: 100.,
                y: 100.,
                width: 200.,
                height: 200.,
            }),
        )
        .unwrap();
        s.begin([296., 200.], 8., false);
        s.motion([700., 200.]);
        assert_eq!(s.rect.width, 604.);
        assert_eq!(s.lens_pixel([1920, 1080]), Some([703, 200]));
        let r = s.lens_rect([1920, 1080], [703, 200]);
        assert_eq!(r.x + r.width, 9.);
        assert!(s.show_lens());
        s.end();
        assert!(!s.show_lens());
        assert!(s.show_lens_in_select());
        s.motion([800., 200.]);
        assert!(!s.show_lens_in_select());
        s.motion([200., 200.]);
        assert!(s.show_lens_in_select());
        s.begin([200., 200.], 8., false);
        assert_eq!(s.drag.unwrap().kind, DragKind::Move);
        assert!(!s.show_lens_in_select());
        s.end();
        assert!(s.show_lens_in_select());
    }
    #[test]
    fn hidpi_selection_move_and_resize_step_in_physical_pixels() {
        let mut s = Selection::new(
            vec![Output {
                rect: Bounds {
                    x: 0.,
                    y: 0.,
                    width: 1440.,
                    height: 900.,
                },
                scale: 2.,
            }],
            vec![],
            Some(Bounds {
                x: 100.,
                y: 100.,
                width: 200.,
                height: 150.,
            }),
        )
        .unwrap();
        s.begin([300., 175.], 8., false);
        assert_eq!(s.drag.unwrap().kind, DragKind::Resize);
        s.motion([300.5, 175.]);
        assert_eq!(s.rect.width, 200.5);
        assert_eq!(s.lens_pixel([2880, 1800]), Some([600, 350]));
        s.end();

        s.begin([150., 150.], 8., false);
        assert_eq!(s.drag.unwrap().kind, DragKind::Move);
        s.motion([150.5, 151.5]);
        assert_eq!(s.rect.x, 100.5);
        assert_eq!(s.rect.y, 101.5);
        s.end();
    }
}

#[cfg(test)]
mod raster_tests {
    use super::*;

    fn layout(scale: f64) -> Selection {
        Selection::new(
            vec![
                Output {
                    rect: Bounds {
                        x: 2880. / scale,
                        y: 0.,
                        width: 1920.,
                        height: 1080.,
                    },
                    scale: 1.,
                },
                Output {
                    rect: Bounds {
                        x: 0.,
                        y: 0.,
                        width: 2880. / scale,
                        height: 1800. / scale,
                    },
                    scale,
                },
            ],
            vec![],
            None,
        )
        .unwrap()
    }

    #[test]
    fn a_short_primary_keeps_native_pixels_when_another_output_changes_scale() {
        for secondary_scale in [1., 1.25, 1.5, 2., 3.] {
            let s = layout(secondary_scale);
            let source = [
                (s.limit.width * secondary_scale).round() as u32,
                (s.limit.height * secondary_scale).round() as u32,
            ];
            let primary = raster_region(&s.outputs, s.limit, source, s.outputs[0].rect).unwrap();
            assert_eq!(primary.size, [1920, 1080]);
            let secondary = raster_region(&s.outputs, s.limit, source, s.outputs[1].rect).unwrap();
            assert_eq!(secondary.size, [2880, 1800]);
            let output = s.outputs[s.output_at(s.outputs[0].rect.center())].rect;
            assert_eq!(output.height, 1080.);
        }
    }

    #[test]
    fn density_uses_only_intersected_outputs_and_preserves_cross_output_detail() {
        let s = layout(2.);
        let region = Bounds {
            x: 1400.,
            y: 100.,
            width: 100.,
            height: 200.,
        };
        assert_eq!(
            raster_region(&s.outputs, s.limit, [6720, 2160], region)
                .unwrap()
                .size,
            [200, 400]
        );
        let primary = s.outputs[0].rect;
        let region = raster_region(&s.outputs, s.limit, [6720, 2160], primary).unwrap();
        assert_eq!(
            region.source,
            Bounds {
                x: 2880.,
                y: 0.,
                width: 3840.,
                height: 2160.
            }
        );
        assert_eq!(region.size, [1920, 1080]);
        let negative = Bounds {
            x: -1440.,
            ..s.limit
        };
        let outputs: Vec<_> = s
            .outputs
            .iter()
            .map(|o| Output {
                rect: Bounds {
                    x: o.rect.x - 1440.,
                    ..o.rect
                },
                ..*o
            })
            .collect();
        assert_eq!(
            raster_region(
                &outputs,
                negative,
                [6720, 2160],
                Bounds { x: 0., ..primary }
            )
            .unwrap()
            .size,
            [1920, 1080]
        );
    }

    #[test]
    fn lower_density_sources_are_not_upscaled_and_window_sources_keep_their_pixels() {
        let s = layout(2.);
        assert_eq!(
            raster_region(&s.outputs, s.limit, [3360, 1080], s.outputs[1].rect)
                .unwrap()
                .size,
            [1440, 900]
        );
        let window = Bounds {
            x: 12.25,
            y: 30.5,
            width: 200.,
            height: 100.,
        };
        assert_eq!(
            raster_region(&[], window, [4000, 2000], window)
                .unwrap()
                .size,
            [4000, 2000]
        );
        assert!(raster_region(&s.outputs, s.limit, [6720, 100], s.outputs[0].rect).is_none());
        assert!(
            raster_region(
                &s.outputs,
                s.limit,
                [6720, 2160],
                Bounds {
                    x: -1.,
                    ..s.outputs[0].rect
                }
            )
            .is_none()
        );
    }

    #[test]
    fn native_reduction_preserves_pixels_at_integer_and_fractional_backing_scales() {
        let original = crate::image::RgbaImage::from_fn(8, 8, |x, y| {
            let c = if (x + y) % 2 == 0 { 0 } else { 255 };
            crate::image::Rgba([c, c, c, 255])
        });
        let output = Output {
            rect: Bounds {
                x: -30.,
                y: 10.,
                width: 8.,
                height: 8.,
            },
            scale: 1.,
        };
        for scale in [1.25, 1.5, 2., 3.] {
            let backing = crate::image::imageops::resize(
                &original,
                (8. * scale) as u32,
                (8. * scale) as u32,
                crate::image::imageops::FilterType::Nearest,
            );
            let plan = raster_region(
                &[output],
                output.rect,
                [backing.width(), backing.height()],
                output.rect,
            )
            .unwrap();
            assert_eq!(
                crate::image_ops::crop_capture(&backing, plan).unwrap(),
                original,
                "{scale}x"
            );
        }
    }

    #[test]
    fn direct_export_crops_and_resamples_the_same_raster_plan() {
        let outputs = [
            Output {
                rect: Bounds {
                    x: 0.,
                    y: 0.,
                    width: 10.,
                    height: 8.,
                },
                scale: 2.,
            },
            Output {
                rect: Bounds {
                    x: 10.,
                    y: 0.,
                    width: 8.,
                    height: 5.,
                },
                scale: 1.,
            },
        ];
        let selection = Selection::new(outputs.to_vec(), vec![], None).unwrap();
        let image = crate::image::RgbaImage::from_fn(36, 16, |x, y| {
            crate::image::Rgba([
                if x < 20 { 200 } else { 10 },
                if x < 20 { 10 } else { 200 },
                y as u8,
                255,
            ])
        });
        let plan = raster_region(&outputs, selection.limit, [36, 16], outputs[1].rect).unwrap();
        let result = crate::image_ops::crop_capture(&image, plan).unwrap();
        assert_eq!(result.dimensions(), (8, 5));
        assert_eq!(&result.get_pixel(0, 0).0[..2], &[10, 200]);
        // Low-density tiles are expanded with nearest sampling by the stream
        // assembler. Restoring their native density must not blur their pixels.
        let original = crate::image::RgbaImage::from_fn(8, 5, |x, y| {
            crate::image::Rgba([if (x + y) % 2 == 0 { 0 } else { 255 }; 4])
        });
        let enlarged = crate::image::imageops::resize(
            &original,
            16,
            10,
            crate::image::imageops::FilterType::Nearest,
        );
        let mut backing = image.clone();
        crate::image::imageops::replace(&mut backing, &enlarged, 20, 0);
        assert_eq!(
            crate::image_ops::crop_capture(&backing, plan).unwrap(),
            original
        );
        let plan = raster_region(&outputs, selection.limit, [36, 16], outputs[0].rect).unwrap();
        assert_eq!(
            crate::image_ops::crop_capture(&image, plan).unwrap(),
            crate::image::imageops::crop_imm(&image, 0, 0, 20, 16).to_image()
        );
    }

    #[test]
    fn subpixel_selection_drags_and_edge_steps_stay_within_fractional_limits() {
        for scale in [1.25, 1.5, 1.75, 2.0, 2.25, 3.0] {
            let outputs = vec![Output {
                rect: Bounds {
                    x: -1200.,
                    y: 0.,
                    width: 1920.,
                    height: 1080.,
                },
                scale,
            }];
            let mut s = Selection::new(outputs, vec![], None).unwrap();
            let step = 1. / scale;
            s.begin([-1000. + step, 100. + step], 0., true);
            s.motion([720. - step, 1080. - step]);
            s.end();
            assert!((s.rect.x - (-1000. + step)).abs() < 1e-12);
            assert!((s.rect.y - (100. + step)).abs() < 1e-12);
            assert!(s.rect.x >= s.limit.x);
            assert!(s.rect.y >= s.limit.y);
            assert!(s.rect.x + s.rect.width <= s.limit.x + s.limit.width);
            assert!(s.rect.y + s.rect.height <= s.limit.y + s.limit.height);

            // Dragging flush against the right/bottom limit never spills 1 ULP past limit.
            let center = s.rect.center();
            s.begin(center, 0., false);
            s.motion([center[0] + 500., center[1] + 500.]);
            s.end();
            assert!(s.rect.x + s.rect.width <= s.limit.x + s.limit.width);
            assert!(s.rect.y + s.rect.height <= s.limit.y + s.limit.height);

            // Outside expansion and keyboard edge steps keep subpixel steps within limit.
            s.begin([-1100. + step, 50. + step], 0., false);
            s.end();
            s.adjust_edge([1., 0.], false, [step, step]);
            s.nudge([step * 4., step * 4.], false);
            assert!(s.rect.x >= s.limit.x);
            assert!(s.rect.y >= s.limit.y);
            assert!(s.rect.x + s.rect.width <= s.limit.x + s.limit.width);
            assert!(s.rect.y + s.rect.height <= s.limit.y + s.limit.height);
        }
    }
}
