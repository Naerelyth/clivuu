//! Retained GSK paths, glyphs and textures. The same nodes serve both editor
//! surfaces and export; pointer motion never rasterizes an entire image in Cairo.
use clivuu_core::annotation::{Bounds, Fill, Kind, Mark, Point};
use clivuu_core::image::effects::TileSpec;
use gtk::{gdk, graphene, gsk, pango, prelude::*};
use std::{collections::HashMap, f64::consts::PI, sync::Arc};
mod sequence;
pub(crate) fn sequence_badge(snapshot: &gtk::Snapshot, mark: &Mark, context: &pango::Context) {
    sequence::draw(snapshot, mark, context);
}

/// Keep GTK's exact-size texture path for smooth display. An explicit linear
/// scale node can blur terminal texels on fractional logical bounds at HiDPI.
pub fn source_texture(
    snapshot: &gtk::Snapshot,
    texture: &gdk::Texture,
    bounds: &graphene::Rect,
    smooth: bool,
) {
    if smooth {
        snapshot.append_texture(texture, bounds);
    } else {
        snapshot.append_scaled_texture(texture, gsk::ScalingFilter::Nearest, bounds);
    }
}
pub fn text_layout(context: &pango::Context, mark: &Mark) -> pango::Layout {
    let layout = pango::Layout::new(context);
    let mut font = pango::FontDescription::new();
    font.set_family(&mark.style.font_family);
    font.set_absolute_size(mark.text_pixels(1.) * pango::SCALE as f64);
    font.set_weight(if mark.style.bold {
        pango::Weight::Bold
    } else {
        pango::Weight::Normal
    });
    font.set_style(if mark.style.italic {
        pango::Style::Italic
    } else {
        pango::Style::Normal
    });
    layout.set_font_description(Some(&font));
    layout.set_text(mark.text.as_deref().unwrap_or(""));
    layout
}
pub fn text_metrics(context: &pango::Context, mark: &Mark, unit: f64) -> Point {
    let (w, h) = text_layout(context, mark).pixel_size();
    [w as f64 / unit, h as f64 / unit]
}
#[derive(Clone)]
pub struct MosaicTexture {
    pub texture: gdk::Texture,
    pub spec: TileSpec,
}
#[derive(Clone, Default)]
pub struct MosaicTextures {
    pub source: [u32; 2],
    /// Small, source-wide coverage while precise region tiles are pending.
    /// Never substitutes for an exact tile in export's readiness check.
    pub previews: HashMap<clivuu_core::image::effects::Effect, MosaicTexture>,
    pub entries: HashMap<TileSpec, MosaicTexture>,
}
impl MosaicTextures {
    pub fn new() -> Self {
        Self::default()
    }
    fn layers(&self, mark: &Mark) -> Vec<(&MosaicTexture, Bounds)> {
        let Some(wanted) = mosaic_spec(mark, self.source) else {
            return vec![];
        };
        if let Some(exact) = self.entries.get(&wanted) {
            return vec![(exact, exact.spec.bounds())];
        }
        let mut layers = vec![];
        if let Some(preview) = self.previews.get(&wanted.effect) {
            layers.push((preview, preview.spec.bounds()));
        }
        if let Some(spec) = self.retained(wanted) {
            let tile = &self.entries[&spec];
            let mut b = spec.bounds();
            if spec.effect == clivuu_core::image::effects::Effect::Blur {
                // Bilinear filtering must not sample beyond a retained tile's
                // edge. Keep its outer texels as interpolation support, and
                // cut coverage on whole source pixels rather than a fractional
                // sampling boundary (which could leave a partially covered seam).
                let sx = b.width / f64::from(spec.width);
                let sy = b.height / f64::from(spec.height);
                let right = (b.x + b.width - sx).floor();
                let bottom = (b.y + b.height - sy).floor();
                b.x = (b.x + sx).ceil();
                b.y = (b.y + sy).ceil();
                b.width = right - b.x;
                b.height = bottom - b.y;
            }
            if b.width > 0. && b.height > 0. {
                layers.push((tile, b));
            }
        }
        layers
    }
    pub(super) fn overlapping(&self, wanted: TileSpec) -> impl Iterator<Item = &MosaicTexture> {
        self.entries
            .values()
            .filter(move |tile| related(tile.spec, wanted))
    }
    pub(super) fn retained(&self, wanted: TileSpec) -> Option<TileSpec> {
        if self.entries.contains_key(&wanted) {
            return None;
        }
        self.overlapping(wanted)
            .max_by(|a, b| {
                let area = |spec: TileSpec| {
                    let b = spec.bounds();
                    let r = wanted.bounds();
                    ((b.x + b.width).min(r.x + r.width) - b.x.max(r.x)).max(0.)
                        * ((b.y + b.height).min(r.y + r.height) - b.y.max(r.y)).max(0.)
                };
                area(a.spec)
                    .total_cmp(&area(b.spec))
                    .then_with(|| {
                        b.spec
                            .size
                            .abs_diff(wanted.size)
                            .cmp(&a.spec.size.abs_diff(wanted.size))
                    })
                    .then_with(|| {
                        (a.spec.level, a.spec.left, a.spec.top).cmp(&(
                            b.spec.level,
                            b.spec.left,
                            b.spec.top,
                        ))
                    })
            })
            .map(|tile| tile.spec)
    }
}
/// Obsolete ROI work may still contribute to the current visible mask. Its
/// completion is useful, even if input has moved on to a larger ROI/strength.
pub(super) fn related(a: TileSpec, b: TileSpec) -> bool {
    if a.source != b.source || a.effect != b.effect {
        return false;
    }
    let a = a.bounds();
    let b = b.bounds();
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}
pub fn mosaic_spec(mark: &Mark, source: [u32; 2]) -> Option<TileSpec> {
    if mark.kind != Kind::Mosaic
        || !mark.kind.supports_fill(mark.style.fill)
        || mark.points.is_empty()
    {
        return None;
    }
    let b = mark.bounds(1.);
    let mapping = mark.point_mapping(1.);
    let corners = [
        [b.x, b.y],
        [b.x + b.width, b.y],
        [b.x, b.y + b.height],
        [b.x + b.width, b.y + b.height],
    ]
    .map(|p| mapping.map(p, false));
    let left = corners.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let top = corners.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let right = corners
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = corners
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max);
    let margin = if mark.style.fill == Fill::Stroke {
        mark.style.width / 2.
    } else {
        0.
    };
    TileSpec::plan_scaled(
        source,
        mark.style.mosaic_effect(),
        mark.style.mosaic_level(),
        Bounds {
            x: left - margin,
            y: top - margin,
            width: right - left + 2. * margin,
            height: bottom - top + 2. * margin,
        },
        mark.style.effect_scale,
    )
}

/// Four disjoint rectangles, including fractional global sampling boundaries.
fn outside(b: Bounds, cut: Bounds) -> Vec<Bounds> {
    let x = b.x.max(cut.x);
    let y = b.y.max(cut.y);
    let right = (b.x + b.width).min(cut.x + cut.width);
    let bottom = (b.y + b.height).min(cut.y + cut.height);
    if right <= x || bottom <= y {
        return vec![b];
    }
    [
        Bounds {
            x: b.x,
            y: b.y,
            width: b.width,
            height: y - b.y,
        },
        Bounds {
            x: b.x,
            y: bottom,
            width: b.width,
            height: b.y + b.height - bottom,
        },
        Bounds {
            x: b.x,
            y,
            width: x - b.x,
            height: bottom - y,
        },
        Bounds {
            x: right,
            y,
            width: b.x + b.width - right,
            height: bottom - y,
        },
    ]
    .into_iter()
    .filter(|r| r.width > 0. && r.height > 0.)
    .collect()
}

fn color(text: &str, alpha: f32) -> gdk::RGBA {
    let mut c = gdk::RGBA::parse(text).unwrap_or(gdk::RGBA::RED);
    c.set_alpha(alpha);
    c
}
fn stroke(width: f64, dashes: &[f32]) -> gsk::Stroke {
    let s = gsk::Stroke::new(width as f32);
    s.set_line_cap(gsk::LineCap::Round);
    s.set_line_join(gsk::LineJoin::Round);
    s.set_dash(dashes);
    s
}
fn path(mark: &Mark) -> gsk::Path {
    let b = mark.bounds(1.);
    let builder = gsk::PathBuilder::new();
    let mapping = mark.point_mapping(1.);
    let map = |p: Point| mapping.map(p, false);
    let point = |p: Point, move_to: bool| {
        let q = map(p);
        if move_to {
            builder.move_to(q[0] as f32, q[1] as f32);
        } else {
            builder.line_to(q[0] as f32, q[1] as f32);
        }
    };
    if mark.kind == Kind::Rect || !mark.line() && mark.style.fill == Fill::Rect {
        point([b.x, b.y], true);
        point([b.x + b.width, b.y], false);
        point([b.x + b.width, b.y + b.height], false);
        point([b.x, b.y + b.height], false);
        builder.close();
    } else if mark.kind == Kind::Ellipse
        || !mark.line()
            && mark.style.fill == Fill::Ellipse
            && mark.kind.supports_fill(mark.style.fill)
    {
        // Transform the geometry rather than the stroke: unequal axes retain
        // the same border thickness, matching the core's world-space hit test.
        let [cx, cy] = b.center();
        let rx = b.width / 2.;
        let ry = b.height / 2.;
        let k = 0.5522847498307936;
        point([cx + rx, cy], true);
        for [a, z, end] in [
            [
                [cx + rx, cy + k * ry],
                [cx + k * rx, cy + ry],
                [cx, cy + ry],
            ],
            [
                [cx - k * rx, cy + ry],
                [cx - rx, cy + k * ry],
                [cx - rx, cy],
            ],
            [
                [cx - rx, cy - k * ry],
                [cx - k * rx, cy - ry],
                [cx, cy - ry],
            ],
            [
                [cx + k * rx, cy - ry],
                [cx + rx, cy - k * ry],
                [cx + rx, cy],
            ],
        ] {
            let a = map(a);
            let z = map(z);
            let e = map(end);
            builder.cubic_to(
                a[0] as f32,
                a[1] as f32,
                z[0] as f32,
                z[1] as f32,
                e[0] as f32,
                e[1] as f32,
            );
        }
        builder.close();
    } else {
        for (i, p) in mark.points.iter().enumerate() {
            point(*p, i == 0);
        }
    }
    builder.to_path()
}
fn arrow(snapshot: &gtk::Snapshot, mark: &Mark) {
    let ink = color(&mark.style.color, 1.);
    snapshot.push_opacity(mark.style.alpha as f64 / 255.);
    // Shared geometry also drives hit testing and the option previews.
    contours(snapshot, mark.arrow_paths(1.), &ink);
    snapshot.pop();
}
fn contours(
    snapshot: &gtk::Snapshot,
    paths: Vec<clivuu_core::annotation::arrow::ArrowPath>,
    ink: &gdk::RGBA,
) {
    for contour in paths {
        let path = gsk::PathBuilder::new();
        for (index, p) in contour.points.iter().enumerate() {
            if index == 0 {
                path.move_to(p[0] as f32, p[1] as f32);
            } else {
                path.line_to(p[0] as f32, p[1] as f32);
            }
        }
        if contour.closed {
            path.close();
        }
        if let Some(spec) = contour.stroke {
            let pen = stroke(
                spec.width,
                &spec.dashes.iter().map(|v| *v as f32).collect::<Vec<_>>(),
            );
            if !spec.round {
                pen.set_line_cap(gsk::LineCap::Butt);
            }
            snapshot.append_stroke(&path.to_path(), &pen, ink);
        } else {
            snapshot.append_fill(&path.to_path(), gsk::FillRule::Winding, ink);
        }
    }
}
fn text(snapshot: &gtk::Snapshot, mark: &Mark, context: &pango::Context) {
    let b = mark.bounds(1.);
    let [cx, cy] = b.center();
    let t = mark.transform();
    let p = mark.points[0];
    snapshot.save();
    snapshot.translate(&graphene::Point::new((cx + t.x) as f32, (cy + t.y) as f32));
    snapshot.rotate((t.angle * 180. / PI) as f32);
    snapshot.scale(t.sx as f32, t.sy as f32);
    snapshot.translate(&graphene::Point::new(
        (p[0] - cx) as f32,
        (p[1] - cy) as f32,
    ));
    let layout = text_layout(context, mark);
    snapshot.push_opacity(mark.style.alpha as f64 / 255.);
    if mark.style.outline_mode != "none" {
        let ink = match mark.style.outline_mode.as_str() {
            "white" => "#ffffff",
            "black" => "#000000",
            "custom" => &mark.style.outline_color,
            _ => {
                let c = color(&mark.style.color, 1.);
                if c.red() * 0.2126 + c.green() * 0.7152 + c.blue() * 0.0722 <= 0.76 {
                    "#ffffff"
                } else {
                    "#000000"
                }
            }
        };
        let glyphs = gsk::PathBuilder::new();
        glyphs.add_layout(&layout);
        snapshot.append_stroke(
            &glyphs.to_path(),
            &stroke(2. * mark.style.outline_width * mark.style.font_scale, &[]),
            &color(ink, 1.),
        );
    }
    snapshot.append_layout(&layout, &color(&mark.style.color, 1.));
    snapshot.pop();
    snapshot.restore();
}
fn mark_node(
    mark: &Mark,
    context: &pango::Context,
    mosaics: &MosaicTextures,
) -> Option<gsk::RenderNode> {
    let snapshot = gtk::Snapshot::new();
    match mark.kind {
        Kind::Sequence => sequence::draw(&snapshot, mark, context),
        Kind::Text => text(&snapshot, mark, context),
        Kind::Arrow => arrow(&snapshot, mark),
        Kind::Eraser if mark.style.fill == Fill::Stroke && mark.points.len() == 1 => {
            let p = mark.map_point(mark.points[0], 1., false);
            let circle = gsk::PathBuilder::new();
            circle.add_circle(
                &graphene::Point::new(p[0] as f32, p[1] as f32),
                mark.style.width as f32 / 2.,
            );
            snapshot.append_fill(&circle.to_path(), gsk::FillRule::Winding, &gdk::RGBA::BLACK);
        }
        Kind::Mosaic => {
            let layers = mosaics.layers(mark);
            if layers.is_empty() {
                return None;
            }
            let dot = mark.style.fill == Fill::Stroke && mark.points.len() == 1;
            let path = if dot {
                let builder = gsk::PathBuilder::new();
                let p = mark.map_point(mark.points[0], 1., false);
                builder.add_circle(
                    &graphene::Point::new(p[0] as f32, p[1] as f32),
                    mark.style.width as f32 / 2.,
                );
                builder.to_path()
            } else {
                path(mark)
            };
            if mark.style.fill == Fill::Stroke && !dot {
                snapshot.push_stroke(&path, &stroke(mark.style.width, &[]));
            } else {
                snapshot.push_fill(&path, gsk::FillRule::Winding);
            }
            snapshot.push_opacity(mark.style.alpha as f64 / 255.);
            for (index, (texture, coverage)) in layers.iter().enumerate() {
                let b = texture.spec.bounds();
                // Replace coverage, do not source-over it: a transparent source
                // must retain its alpha while an old ROI replaces the preview.
                let clips = if let Some(next) = layers.get(index + 1) {
                    outside(*coverage, next.1)
                } else {
                    vec![*coverage]
                };
                for clip in clips {
                    snapshot.push_clip(&graphene::Rect::new(
                        clip.x as f32,
                        clip.y as f32,
                        clip.width as f32,
                        clip.height as f32,
                    ));
                    snapshot.append_scaled_texture(
                        &texture.texture,
                        if mark.style.effect == "blur" {
                            gsk::ScalingFilter::Linear
                        } else {
                            gsk::ScalingFilter::Nearest
                        },
                        &graphene::Rect::new(
                            b.x as f32,
                            b.y as f32,
                            b.width as f32,
                            b.height as f32,
                        ),
                    );
                    snapshot.pop();
                }
            }
            snapshot.pop();
            snapshot.pop();
        }
        _ => {
            let path = path(mark);
            let ink = if mark.kind == Kind::Eraser {
                gdk::RGBA::BLACK
            } else {
                color(&mark.style.color, mark.style.alpha as f32 / 255.)
            };
            if mark.style.fill == Fill::Solid
                || matches!(mark.kind, Kind::Highlight | Kind::Eraser)
                    && matches!(mark.style.fill, Fill::Rect | Fill::Ellipse)
            {
                snapshot.append_fill(&path, gsk::FillRule::Winding, &ink);
            } else {
                snapshot.append_stroke(&path, &stroke(mark.style.width, &[]), &ink);
            }
        }
    }
    snapshot.to_node()
}
struct CachedMark {
    mark: Mark,
    node: Option<gsk::RenderNode>,
    mosaic_revision: u64,
}
fn rects_overlap(a: &graphene::Rect, b: &graphene::Rect) -> bool {
    a.intersection(b)
        .is_some_and(|r| r.width() > 0. && r.height() > 0.)
}
fn padded_rect(r: &graphene::Rect, pad: f32) -> graphene::Rect {
    graphene::Rect::new(
        r.x() - pad,
        r.y() - pad,
        r.width() + 2. * pad,
        r.height() + 2. * pad,
    )
}
fn combine_layers(layers: &[gsk::RenderNode]) -> Option<gsk::RenderNode> {
    match layers {
        [] => None,
        [only] => Some(only.clone()),
        many => Some(gsk::ContainerNode::new(many).upcast()),
    }
}
/// Scope an eraser `MaskNode` to prior layers whose bounds actually intersect
/// the eraser, preserving exact z-order and anti-aliased multi-layer overlaps
/// while keeping unrelated annotations outside any mask offscreen.
fn apply_eraser(layers: &mut Vec<gsk::RenderNode>, eraser: gsk::RenderNode) {
    let eb = eraser.bounds();
    if eb.width() <= 0. || eb.height() <= 0. {
        return;
    }
    let eb_pad = padded_rect(&eb, 1.);
    let hits: Vec<usize> = layers
        .iter()
        .enumerate()
        .filter_map(|(i, n)| rects_overlap(&n.bounds(), &eb_pad).then_some(i))
        .collect();
    if hits.is_empty() {
        return;
    }
    if hits.len() == 1 {
        let k = hits[0];
        layers[k] = gsk::MaskNode::new(&layers[k], &eraser, gsk::MaskMode::InvertedAlpha).upcast();
        return;
    }
    let first = hits[0];
    let last = *hits.last().unwrap();
    let span = &layers[first..=last];
    let span_bounds: Vec<_> = span.iter().map(|n| padded_rect(&n.bounds(), 1.)).collect();
    let mut in_group = vec![true; span.len()];
    let mut before = Vec::new();
    let mut after = Vec::new();
    for i in 0..span.len() {
        if rects_overlap(&span_bounds[i], &eb_pad) {
            continue;
        }
        let blocked =
            (0..i).any(|j| in_group[j] && rects_overlap(&span_bounds[i], &span_bounds[j]));
        if !blocked {
            in_group[i] = false;
            before.push(span[i].clone());
        }
    }
    for i in (0..span.len()).rev() {
        if !in_group[i] || rects_overlap(&span_bounds[i], &eb_pad) {
            continue;
        }
        let blocked = ((i + 1)..span.len())
            .any(|j| in_group[j] && rects_overlap(&span_bounds[i], &span_bounds[j]));
        if !blocked {
            in_group[i] = false;
            after.push(span[i].clone());
        }
    }
    after.reverse();
    let middle: Vec<_> = span
        .iter()
        .enumerate()
        .filter(|&(i, _)| in_group[i])
        .map(|(_, n)| n.clone())
        .collect();
    let Some(group) = combine_layers(&middle) else {
        return;
    };
    let masked: gsk::RenderNode =
        gsk::MaskNode::new(&group, &eraser, gsk::MaskMode::InvertedAlpha).upcast();
    let mut replacement = before;
    replacement.push(masked);
    replacement.extend(after);
    layers.splice(first..=last, replacement);
}
#[derive(Default)]
pub struct AnnotationNodes {
    cache: Vec<CachedMark>,
    layers: Vec<gsk::RenderNode>,
    composed: Option<gsk::RenderNode>,
    mosaic_revision: u64,
    has_eraser: bool,
    revision: u64,
}
impl AnnotationNodes {
    pub fn invalidate_mosaics(&mut self) {
        self.mosaic_revision = self.mosaic_revision.wrapping_add(1);
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn has_eraser(&self) -> bool {
        self.has_eraser
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn compose<'a>(
        &mut self,
        marks: impl IntoIterator<Item = &'a Mark>,
        context: &pango::Context,
        mosaics: &MosaicTextures,
    ) -> Option<gsk::RenderNode> {
        let mut count = 0;
        let mut changed = false;
        for (i, mark) in marks.into_iter().enumerate() {
            count = i + 1;
            let unchanged = self.cache.get(i).is_some_and(|c| {
                c.mark.kind == mark.kind
                    && (mark.kind != Kind::Mosaic || c.mosaic_revision == self.mosaic_revision)
                    && Arc::ptr_eq(&c.mark.points, &mark.points)
                    && c.mark.style == mark.style
                    && c.mark.transform == mark.transform
                    && c.mark.text == mark.text
                    && c.mark.text_size == mark.text_size
            });
            if !unchanged {
                changed = true;
                let entry = CachedMark {
                    mark: mark.clone(),
                    node: mark_node(mark, context, mosaics),
                    mosaic_revision: self.mosaic_revision,
                };
                if i < self.cache.len() {
                    self.cache[i] = entry;
                } else {
                    self.cache.push(entry);
                }
            }
        }
        if count != self.cache.len() {
            self.cache.truncate(count);
            changed = true;
        }
        if !changed {
            return self.composed.clone();
        }
        self.layers.clear();
        let mut has_eraser = false;
        for entry in &self.cache {
            let Some(node) = entry.node.clone() else {
                continue;
            };
            if entry.mark.kind == Kind::Eraser {
                has_eraser = true;
                apply_eraser(&mut self.layers, node);
            } else {
                self.layers.push(node);
            }
        }
        self.has_eraser = has_eraser && !self.layers.is_empty();
        self.composed = combine_layers(&self.layers);
        self.revision = self.revision.wrapping_add(1);
        self.composed.clone()
    }
    /// A growing freehand path must not enter the committed-mark cache: keeping
    /// its Arc there would force copy-on-write of every earlier sample on each
    /// new pointer event. Only its transient GSK node is retained by GTK.
    pub fn compose_preview<'a>(
        &mut self,
        marks: impl IntoIterator<Item = &'a Mark>,
        preview: Option<&Mark>,
        context: &pango::Context,
        mosaics: &MosaicTextures,
    ) -> Option<gsk::RenderNode> {
        let previous = self.compose(marks, context, mosaics);
        let Some(mark) = preview else {
            return previous;
        };
        let Some(node) = mark_node(mark, context, mosaics) else {
            return previous;
        };
        if mark.kind == Kind::Eraser {
            if self.layers.is_empty() {
                return None;
            }
            let mut layers = self.layers.clone();
            apply_eraser(&mut layers, node);
            combine_layers(&layers)
        } else {
            match previous {
                None => Some(node),
                Some(previous) => Some(gsk::ContainerNode::new(&[previous, node]).upcast()),
            }
        }
    }
}
