//! Self-contained PP-OCRv5 Mobile inference pipeline (DBNet detector + SVTR/CTC recognizer).
use crate::{
    Result,
    i18n::t,
    image::{RgbaImage, imageops},
};
use ::image::{Rgb, RgbImage};
use std::path::Path;

use super::{
    assets::{ModelFiles, ModelSpec, verify_model_files},
    layout::{self, char_boundaries, lookalikes},
    model::{OcrDocument, OcrEngineUsed, OcrLine, RectF},
    ort_api::{OrtModelSession, OrtRuntime},
};

const DETECT_MAX_SIDE: u32 = 1920;
const DETECT_PAD: u32 = 16;
const DETECT_SCORE_THRESHOLD: f32 = 0.30;
const DETECT_BOX_THRESHOLD: f32 = 0.60;
const DETECT_UNCLIP_RATIO: f32 = 1.50;
const DETECT_MAX_CANDIDATES: usize = 1000;

const TILE_TRIGGER_HEIGHT: u32 = 2048;
const TILE_HEIGHT: u32 = 1600;
const TILE_OVERLAP: u32 = 96;

const REC_HEIGHT: u32 = 48;
const REC_MIN_WIDTH: f32 = 320.0;
const REC_MAX_WIDTH: f32 = 3200.0;

const MIN_CONFIDENCE: f32 = 0.55;
const SINGLE_CHAR_CONFIDENCE: f32 = 0.60;
const SINGLE_CHAR_HEIGHT: f32 = 0.60;
const GLYPH_HEIGHT: f32 = 1.80;
const GLYPH_ASPECT: f32 = 1.20;
const SPECK_SIZE: f32 = 0.50;
const RULE_CONTRAST: i32 = 32;
const RULE_COVERAGE: f32 = 0.90;
const RULE_REACH: f32 = 0.50;

#[derive(Clone, Copy, Debug)]
struct PointF {
    x: f32,
    y: f32,
}

#[derive(Default)]
struct ReadResult {
    text: String,
    positions: Vec<f32>,
    confidence: f32,
}

pub fn recognize_document(
    image: &RgbaImage,
    ort_lib: &Path,
    spec: &ModelSpec,
    files: &ModelFiles,
) -> Result<OcrDocument> {
    if image.width() == 0
        || image.height() == 0
        || u64::from(image.width()) * u64::from(image.height()) > crate::image_ops::MAX_PIXELS
    {
        return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
    }
    verify_model_files(spec, files)?;

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 4);
    let runtime = OrtRuntime::load(ort_lib)?;
    let detector = OrtModelSession::from_file(&runtime, &files.detector, threads)?;
    let recognizer = OrtModelSession::from_file(&runtime, &files.recognizer, threads)?;
    let dict: Vec<String> = std::fs::read_to_string(&files.dict)?
        .lines()
        .map(str::to_owned)
        .collect();

    let rgb = rgba_to_white_matted_rgb(image);
    let (width, height) = rgb.dimensions();
    let Some(area) = RectF::from_xywh(0.0, 0.0, width as f32, height as f32) else {
        return Ok(OcrDocument::default());
    };

    let raw_quads = detect_full_or_tiled(&detector, &rgb)?;
    if raw_quads.is_empty() {
        return Ok(OcrDocument {
            lines: Vec::new(),
            blocks: Vec::new(),
            width,
            height,
            engine: OcrEngineUsed::Paddle,
        });
    }

    let mut quads = Vec::with_capacity(raw_quads.len());
    let mut boxes = Vec::with_capacity(raw_quads.len());
    for q in raw_quads {
        if let Some(rect) = quad_bounds(&q) {
            quads.push(q);
            boxes.push(rect);
        }
    }
    if boxes.is_empty() {
        return Ok(OcrDocument {
            lines: Vec::new(),
            blocks: Vec::new(),
            width,
            height,
            engine: OcrEngineUsed::Paddle,
        });
    }

    let body_height = layout::median_of(boxes.iter().map(|b| b.height));
    let keep: Vec<usize> = (0..boxes.len())
        .filter(|&i| !is_speck(&boxes[i], body_height))
        .collect();
    if keep.is_empty() {
        return Ok(OcrDocument {
            lines: Vec::new(),
            blocks: Vec::new(),
            width,
            height,
            engine: OcrEngineUsed::Paddle,
        });
    }

    let kept_boxes: Vec<RectF> = keep.iter().map(|&i| boxes[i]).collect();
    let rows = layout::row_groups(&kept_boxes, |a, b| has_rule(&rgb, a, b));

    let mut rects = Vec::with_capacity(rows.len());
    let mut crops = Vec::with_capacity(rows.len());
    for row in &rows {
        let members: Vec<usize> = row.iter().map(|&r| keep[r]).collect();
        let merged = union_of(&members, &boxes);
        let crop = if let [only] = members[..] {
            rotated_quad_crop(&rgb, &quads[only]).or_else(|| axis_aligned_crop(&rgb, &merged))
        } else {
            axis_aligned_crop(&rgb, &merged)
        };
        if let Some(crop) = crop {
            rects.push(merged);
            crops.push(crop);
        }
    }

    let mut lines: Vec<OcrLine> = Vec::with_capacity(crops.len());
    for (bounds, crop) in rects.into_iter().zip(crops.iter()) {
        let read = recognize_line(&recognizer, &dict, crop)?;
        if is_noise(&bounds, &read, body_height) {
            continue;
        }
        let text = lookalikes::fix(&read.text);
        let char_x = char_boundaries(&bounds, &text, &read.positions);
        let line = OcrLine {
            text,
            bounds,
            char_x,
        };
        if let Some(clipped) = clip_line(line, area) {
            lines.push(clipped);
        }
    }
    layout::sort_lines_by_rows(&mut lines);

    Ok(OcrDocument::from_lines(
        lines,
        width,
        height,
        OcrEngineUsed::Paddle,
    ))
}

/// Composite straight RGBA over a pure white background into RGB so transparent pins or overlays
/// never turn dark text on transparent backgrounds into black-on-black.
pub fn rgba_to_white_matted_rgb(image: &RgbaImage) -> RgbImage {
    let (w, h) = image.dimensions();
    let mut raw = Vec::with_capacity(w as usize * h as usize * 3);
    for px in image.as_raw().chunks_exact(4) {
        let a = u16::from(px[3]);
        let inv = 255u16 - a;
        let r = ((u16::from(px[0]) * a + 255 * inv + 127) / 255) as u8;
        let g = ((u16::from(px[1]) * a + 255 * inv + 127) / 255) as u8;
        let b = ((u16::from(px[2]) * a + 255 * inv + 127) / 255) as u8;
        raw.extend_from_slice(&[r, g, b]);
    }
    RgbImage::from_raw(w, h, raw).expect("RGB dimensions match")
}

fn detect_full_or_tiled(
    detector: &OrtModelSession<'_>,
    rgb: &RgbImage,
) -> Result<Vec<[PointF; 4]>> {
    let (w, h) = rgb.dimensions();
    if h <= TILE_TRIGGER_HEIGHT || h <= w.saturating_mul(2) {
        return detect_single(detector, rgb);
    }

    // Long vertical screenshot (e.g. scrolling capture): slice with overlap so small UI font isn't
    // crushed by resizing an 8000px-tall image down to 1920px.
    let step = TILE_HEIGHT - TILE_OVERLAP;
    let mut all: Vec<[PointF; 4]> = Vec::new();
    let mut y0 = 0u32;
    while y0 < h {
        let th = TILE_HEIGHT.min(h - y0);
        let tile = imageops::crop_imm(rgb, 0, y0, w, th).to_image();
        let mut quads = detect_single(detector, &tile)?;
        for q in &mut quads {
            for p in q.iter_mut() {
                p.y += y0 as f32;
            }
        }
        if y0 > 0 {
            let seam_top = y0 as f32;
            let seam_bottom = (y0 + TILE_OVERLAP) as f32;
            for q in quads {
                let Some(rect) = quad_bounds(&q) else {
                    continue;
                };
                if rect.top() < seam_bottom && rect.bottom() > seam_top {
                    let duplicate = all.iter().any(|existing| {
                        quad_bounds(existing).is_some_and(|er| {
                            rect_iou(er, rect) > 0.45 || contains_mostly(er, rect)
                        })
                    });
                    if duplicate {
                        continue;
                    }
                }
                all.push(q);
            }
        } else {
            all.extend(quads);
        }
        if y0 + th >= h {
            break;
        }
        y0 += step;
    }
    Ok(all)
}

fn detect_single(detector: &OrtModelSession<'_>, img: &RgbImage) -> Result<Vec<[PointF; 4]>> {
    let (padded, (pad_left, pad_top)) = pad_for_detection(img);
    let (pw, ph) = padded.dimensions();
    if pw == 0 || ph == 0 {
        return Ok(Vec::new());
    }

    let max_dim = pw.max(ph);
    let resize_ratio = if max_dim > DETECT_MAX_SIDE {
        DETECT_MAX_SIDE as f32 / max_dim as f32
    } else {
        1.0
    };
    let tw = (((pw as f32 * resize_ratio) / 32.0).round().max(1.0) as u32) * 32;
    let th = (((ph as f32 * resize_ratio) / 32.0).round().max(1.0) as u32) * 32;

    let resized = if tw == pw && th == ph {
        padded
    } else {
        imageops::resize(&padded, tw, th, imageops::FilterType::Triangle)
    };

    let scale_x = tw as f32 / pw as f32;
    let scale_y = th as f32 / ph as f32;

    // BGR ImageNet normalization into CHW `[1, 3, th, tw]`.
    let plane = (tw * th) as usize;
    let mut tensor = vec![0.0f32; plane * 3];
    let (b_plane, rest) = tensor.split_at_mut(plane);
    let (g_plane, r_plane) = rest.split_at_mut(plane);
    for (idx, px) in resized.as_raw().chunks_exact(3).enumerate() {
        let r = px[0] as f32 * (1.0 / 255.0);
        let g = px[1] as f32 * (1.0 / 255.0);
        let b = px[2] as f32 * (1.0 / 255.0);
        b_plane[idx] = (b - 0.485) / 0.229;
        g_plane[idx] = (g - 0.456) / 0.224;
        r_plane[idx] = (r - 0.406) / 0.225;
    }

    let shape = [1i64, 3, i64::from(th), i64::from(tw)];
    let (out_dims, prob) = detector.run_f32(&shape, &tensor)?;
    let (map_w, map_h) = match out_dims.as_slice() {
        [_, _, mh, mw] if *mh > 0 && *mw > 0 => (*mw as u32, *mh as u32),
        _ => (tw, th),
    };
    if prob.len() < (map_w * map_h) as usize {
        return Ok(Vec::new());
    }

    let mut quads = extract_db_boxes(
        &prob[..(map_w * map_h) as usize],
        map_w,
        map_h,
        scale_x * (map_w as f32 / tw as f32),
        scale_y * (map_h as f32 / th as f32),
        pw as f32,
        ph as f32,
    );
    for q in &mut quads {
        for p in q.iter_mut() {
            p.x -= pad_left as f32;
            p.y -= pad_top as f32;
        }
    }
    Ok(quads)
}

pub fn pad_for_detection(img: &RgbImage) -> (RgbImage, (u32, u32)) {
    let (w, h) = img.dimensions();
    let grow = |n: u32| (u64::from(n) + 2 * u64::from(DETECT_PAD)).div_ceil(32) * 32;
    let (pw, ph) = (grow(w), grow(h));
    if w == 0 || h == 0 || pw.max(ph) > u64::from(DETECT_MAX_SIDE) {
        return (img.clone(), (0, 0));
    }
    let (pw, ph) = (pw as u32, ph as u32);
    let (left, top) = ((pw - w) / 2, (ph - h) / 2);
    let mut out = RgbImage::from_pixel(pw, ph, border_median(img));
    imageops::replace(&mut out, img, i64::from(left), i64::from(top));
    (out, (left, top))
}

fn border_median(img: &RgbImage) -> Rgb<u8> {
    let (w, h) = img.dimensions();
    let ring = (0..w)
        .flat_map(|x| [(x, 0), (x, h - 1)])
        .chain((1..h.saturating_sub(1)).flat_map(|y| [(0, y), (w - 1, y)]));
    let mut channels: [Vec<u8>; 3] = Default::default();
    for (x, y) in ring {
        for (values, &c) in channels.iter_mut().zip(&img.get_pixel(x, y).0) {
            values.push(c);
        }
    }
    Rgb(channels.map(|mut values| {
        if values.is_empty() {
            return 255;
        }
        let mid = values.len() / 2;
        *values.select_nth_unstable(mid).1
    }))
}

fn extract_db_boxes(
    prob: &[f32],
    w: u32,
    h: u32,
    scale_x: f32,
    scale_y: f32,
    max_x: f32,
    max_y: f32,
) -> Vec<[PointF; 4]> {
    let len = (w * h) as usize;
    let mut visited = vec![0u8; len];
    let mut stack = Vec::new();
    let mut pixels = Vec::new();
    let mut boundary = Vec::new();
    let mut out = Vec::new();

    for sy in 0..h {
        for sx in 0..w {
            let start = (sy * w + sx) as usize;
            if visited[start] != 0 || prob[start] <= DETECT_SCORE_THRESHOLD {
                continue;
            }
            visited[start] = 1;
            stack.clear();
            pixels.clear();
            boundary.clear();
            stack.push((sx, sy));

            let (mut min_x, mut max_px, mut min_y, mut max_py) = (sx, sx, sy, sy);
            let mut score_sum = 0.0f32;

            while let Some((cx, cy)) = stack.pop() {
                let idx = (cy * w + cx) as usize;
                pixels.push((cx, cy));
                score_sum += prob[idx];
                min_x = min_x.min(cx);
                max_px = max_px.max(cx);
                min_y = min_y.min(cy);
                max_py = max_py.max(cy);

                let mut is_edge = cx == 0 || cy == 0 || cx + 1 == w || cy + 1 == h;
                let y0 = cy.saturating_sub(1);
                let y1 = (cy + 1).min(h - 1);
                let x0 = cx.saturating_sub(1);
                let x1 = (cx + 1).min(w - 1);
                for ny in y0..=y1 {
                    for nx in x0..=x1 {
                        if nx == cx && ny == cy {
                            continue;
                        }
                        let nidx = (ny * w + nx) as usize;
                        if prob[nidx] > DETECT_SCORE_THRESHOLD {
                            if visited[nidx] == 0 {
                                visited[nidx] = 1;
                                stack.push((nx, ny));
                            }
                        } else {
                            is_edge = true;
                        }
                    }
                }
                if is_edge {
                    boundary.push(PointF {
                        x: cx as f32,
                        y: cy as f32,
                    });
                }
            }

            if max_px - min_x < 2 || max_py - min_y < 2 || pixels.len() < 6 {
                continue;
            }
            let mean_score = score_sum / pixels.len() as f32;
            if mean_score < DETECT_BOX_THRESHOLD {
                continue;
            }

            let Some(quad) = unclipped_min_area_quad(
                &boundary,
                DETECT_UNCLIP_RATIO,
                scale_x,
                scale_y,
                max_x,
                max_y,
            ) else {
                continue;
            };
            out.push(quad);
            if out.len() >= DETECT_MAX_CANDIDATES {
                return out;
            }
        }
    }
    out
}

/// Compute minimum-area rotated bounding box via 2D monotone-chain convex hull + rotating calipers,
/// expand by DBNet's unclip offset `area * unclip_ratio / perimeter`, and return ordered corners
/// `[top_left, top_right, bottom_right, bottom_left]` in image space.
fn unclipped_min_area_quad(
    pts: &[PointF],
    unclip_ratio: f32,
    scale_x: f32,
    scale_y: f32,
    max_x: f32,
    max_y: f32,
) -> Option<[PointF; 4]> {
    if pts.len() < 3 {
        return None;
    }
    let hull = convex_hull(pts);
    if hull.len() < 2 {
        return None;
    }

    let mut best_area = f32::MAX;
    let mut best_axes = ((1.0f32, 0.0f32), (0.0f32, 1.0f32));
    let mut best_extents = (0.0f32, 0.0f32, 0.0f32, 0.0f32);

    // Always include axis-aligned candidate `(1, 0)` so horizontal UI text stays crisp.
    let mut candidate_dirs = Vec::with_capacity(hull.len() + 1);
    candidate_dirs.push((1.0f32, 0.0f32));
    for i in 0..hull.len() {
        let a = hull[i];
        let b = hull[(i + 1) % hull.len()];
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = dx.hypot(dy);
        if len > 1e-4 {
            candidate_dirs.push((dx / len, dy / len));
        }
    }

    for (ux, uy) in candidate_dirs {
        let (vx, vy) = (-uy, ux);
        let (mut min_u, mut max_u) = (f32::MAX, f32::MIN);
        let (mut min_v, mut max_v) = (f32::MAX, f32::MIN);
        for p in &hull {
            let pu = p.x * ux + p.y * uy;
            let pv = p.x * vx + p.y * vy;
            min_u = min_u.min(pu);
            max_u = max_u.max(pu);
            min_v = min_v.min(pv);
            max_v = max_v.max(pv);
        }
        // Pixel centers represent 1x1 cells, so add 1.0px extent along each axis.
        let rw = (max_u - min_u + 1.0).max(1.0);
        let rh = (max_v - min_v + 1.0).max(1.0);
        let area = rw * rh;
        if area < best_area {
            best_area = area;
            best_axes = ((ux, uy), (vx, vy));
            best_extents = (min_u - 0.5, max_u + 0.5, min_v - 0.5, max_v + 0.5);
        }
    }

    let (min_u, max_u, min_v, max_v) = best_extents;
    let rw = (max_u - min_u).max(1.0);
    let rh = (max_v - min_v).max(1.0);
    if rw.min(rh) < 3.0 {
        return None;
    }

    let perimeter = 2.0 * (rw + rh);
    let distance = (rw * rh) * unclip_ratio / perimeter.max(1e-4);
    let (u0, u1) = (min_u - distance, max_u + distance);
    let (v0, v1) = (min_v - distance, max_v + distance);
    let ((ux, uy), (vx, vy)) = best_axes;

    let to_img = |u: f32, v: f32| PointF {
        x: ((u * ux + v * vx) / scale_x).clamp(0.0, max_x),
        y: ((u * uy + v * vy) / scale_y).clamp(0.0, max_y),
    };
    let mut corners = [
        to_img(u0, v0),
        to_img(u1, v0),
        to_img(u1, v1),
        to_img(u0, v1),
    ];
    order_quad_corners(&mut corners);
    Some(corners)
}

fn convex_hull(points: &[PointF]) -> Vec<PointF> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4);
    if pts.len() <= 2 {
        return pts;
    }
    let cross =
        |o: PointF, a: PointF, b: PointF| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut lower: Vec<PointF> = Vec::with_capacity(pts.len());
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<PointF> = Vec::with_capacity(pts.len());
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Order 4 points as `[top_left, top_right, bottom_right, bottom_left]`.
fn order_quad_corners(pts: &mut [PointF; 4]) {
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let (left1, left2) = if pts[0].y <= pts[1].y {
        (pts[0], pts[1])
    } else {
        (pts[1], pts[0])
    };
    let (right1, right2) = if pts[2].y <= pts[3].y {
        (pts[2], pts[3])
    } else {
        (pts[3], pts[2])
    };
    *pts = [left1, right1, right2, left2];
}

fn quad_bounds(pts: &[PointF; 4]) -> Option<RectF> {
    let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in pts {
        l = l.min(p.x);
        t = t.min(p.y);
        r = r.max(p.x);
        b = b.max(p.y);
    }
    RectF::from_ltrb(l, t, r, b)
}

fn rotated_quad_crop(img: &RgbImage, pts: &[PointF; 4]) -> Option<RgbImage> {
    let w1 = (pts[1].x - pts[0].x).hypot(pts[1].y - pts[0].y);
    let w2 = (pts[2].x - pts[3].x).hypot(pts[2].y - pts[3].y);
    let h1 = (pts[3].x - pts[0].x).hypot(pts[3].y - pts[0].y);
    let h2 = (pts[2].x - pts[1].x).hypot(pts[2].y - pts[1].y);
    let dst_w = w1.max(w2).round() as u32;
    let dst_h = h1.max(h2).round() as u32;
    if dst_w == 0 || dst_h == 0 {
        return None;
    }

    // Fast path when virtually axis-aligned.
    if (pts[1].y - pts[0].y).abs() < 1.5 && (pts[3].x - pts[0].x).abs() < 1.5 {
        return quad_bounds(pts).and_then(|r| axis_aligned_crop(img, &r));
    }

    let mut out = RgbImage::new(dst_w, dst_h);
    let inv_w = 1.0 / (dst_w.saturating_sub(1).max(1) as f32);
    let inv_h = 1.0 / (dst_h.saturating_sub(1).max(1) as f32);
    for y in 0..dst_h {
        let v = if dst_h == 1 { 0.0 } else { y as f32 * inv_h };
        for x in 0..dst_w {
            let u = if dst_w == 1 { 0.0 } else { x as f32 * inv_w };
            let sx = (1.0 - u) * (1.0 - v) * pts[0].x
                + u * (1.0 - v) * pts[1].x
                + u * v * pts[2].x
                + (1.0 - u) * v * pts[3].x;
            let sy = (1.0 - u) * (1.0 - v) * pts[0].y
                + u * (1.0 - v) * pts[1].y
                + u * v * pts[2].y
                + (1.0 - u) * v * pts[3].y;
            out.put_pixel(x, y, sample_bilinear(img, sx, sy));
        }
    }
    if dst_h as f32 >= dst_w as f32 * 1.5 {
        out = imageops::rotate270(&out);
    }
    Some(out)
}

fn sample_bilinear(img: &RgbImage, x: f32, y: f32) -> Rgb<u8> {
    let (w, h) = img.dimensions();
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let p00 = img.get_pixel(x0, y0).0;
    let p10 = img.get_pixel(x1, y0).0;
    let p01 = img.get_pixel(x0, y1).0;
    let p11 = img.get_pixel(x1, y1).0;
    let mut c = [0u8; 3];
    for i in 0..3 {
        let top = p00[i] as f32 * (1.0 - fx) + p10[i] as f32 * fx;
        let bot = p01[i] as f32 * (1.0 - fx) + p11[i] as f32 * fx;
        c[i] = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
    }
    Rgb(c)
}

fn axis_aligned_crop(img: &RgbImage, rect: &RectF) -> Option<RgbImage> {
    let x = rect.left().floor().max(0.0) as u32;
    let y = rect.top().floor().max(0.0) as u32;
    let right = (rect.right().ceil() as u32).min(img.width());
    let bottom = (rect.bottom().ceil() as u32).min(img.height());
    if right <= x || bottom <= y {
        return None;
    }
    Some(imageops::crop_imm(img, x, y, right - x, bottom - y).to_image())
}

fn recognize_line(
    recognizer: &OrtModelSession<'_>,
    dict: &[String],
    crop: &RgbImage,
) -> Result<ReadResult> {
    let (w, h) = crop.dimensions();
    if w == 0 || h == 0 {
        return Ok(ReadResult::default());
    }
    let ratio = w as f32 / h as f32;
    let tensor_w = ((REC_HEIGHT as f32) * ratio.max(REC_MIN_WIDTH / REC_HEIGHT as f32))
        .floor()
        .clamp(1.0, REC_MAX_WIDTH) as u32;
    let content_w = ((REC_HEIGHT as f32) * ratio)
        .ceil()
        .clamp(1.0, tensor_w as f32) as u32;

    let resized = imageops::resize(crop, content_w, REC_HEIGHT, imageops::FilterType::Triangle);

    // BGR `[-1, 1]` normalization into `[1, 3, 48, tensor_w]`, right-padded with `0.0`.
    let plane = (REC_HEIGHT * tensor_w) as usize;
    let mut tensor = vec![0.0f32; plane * 3];
    let (b_plane, rest) = tensor.split_at_mut(plane);
    let (g_plane, r_plane) = rest.split_at_mut(plane);
    let tw_usize = tensor_w as usize;
    let cw_usize = content_w as usize;
    let raw = resized.as_raw();
    for y in 0..REC_HEIGHT as usize {
        let dst_row = y * tw_usize;
        let src_row = y * cw_usize * 3;
        for x in 0..cw_usize {
            let s = src_row + x * 3;
            let r = raw[s] as f32 * (2.0 / 255.0) - 1.0;
            let g = raw[s + 1] as f32 * (2.0 / 255.0) - 1.0;
            let b = raw[s + 2] as f32 * (2.0 / 255.0) - 1.0;
            b_plane[dst_row + x] = b;
            g_plane[dst_row + x] = g;
            r_plane[dst_row + x] = r;
        }
    }

    let shape = [1i64, 3, i64::from(REC_HEIGHT), i64::from(tensor_w)];
    let (out_dims, logits) = recognizer.run_f32(&shape, &tensor)?;
    let (steps, vocab) = match out_dims.as_slice() {
        [_, t, v] if *t > 0 && *v > 1 => (*t as usize, *v as usize),
        _ => return Ok(ReadResult::default()),
    };
    if logits.len() < steps * vocab {
        return Ok(ReadResult::default());
    }

    Ok(ctc_greedy_decode(
        &logits[..steps * vocab],
        steps,
        vocab,
        dict,
        w,
        h,
    ))
}

fn ctc_greedy_decode(
    data: &[f32],
    steps: usize,
    vocab: usize,
    dict: &[String],
    crop_w: u32,
    crop_h: u32,
) -> ReadResult {
    let mut text = String::new();
    let mut raw_positions = Vec::new();
    let mut score_sum = 0.0f32;
    let mut score_count = 0usize;
    let mut prev_idx = 0usize;

    for t in 0..steps {
        let row = &data[t * vocab..(t + 1) * vocab];
        let mut best_idx = 0usize;
        let mut best_val = f32::NEG_INFINITY;
        for (i, &val) in row.iter().enumerate() {
            if val > best_val {
                best_val = val;
                best_idx = i;
            }
        }
        if best_idx != 0 && best_idx != prev_idx {
            let token = if best_idx - 1 < dict.len() {
                dict[best_idx - 1].as_str()
            } else if best_idx - 1 == dict.len() {
                " "
            } else {
                ""
            };
            if !token.is_empty() {
                let prob = if !(0.0..=1.001).contains(&best_val) {
                    let sum_exp: f32 = row.iter().map(|&v| (v - best_val).exp()).sum();
                    1.0 / sum_exp.max(1.0)
                } else {
                    best_val.clamp(0.0, 1.0)
                };
                let char_pos = t as f32 / steps as f32;
                for ch in token.chars() {
                    text.push(ch);
                    raw_positions.push(char_pos);
                }
                score_sum += prob;
                score_count += 1;
            }
        }
        prev_idx = best_idx;
    }

    let confidence = if score_count > 0 {
        score_sum / score_count as f32
    } else {
        0.0
    };
    let positions = to_crop_space(raw_positions, steps, crop_w, crop_h);
    ReadResult {
        text,
        positions,
        confidence,
    }
}

fn to_crop_space(mut positions: Vec<f32>, steps: usize, w: u32, h: u32) -> Vec<f32> {
    let ratio = w as f32 / h.max(1) as f32;
    let tensor_w = ((REC_HEIGHT as f32) * ratio.max(REC_MIN_WIDTH / REC_HEIGHT as f32))
        .floor()
        .min(REC_MAX_WIDTH);
    let content_w = ((REC_HEIGHT as f32) * ratio).ceil().min(tensor_w).max(1.0);
    let half_step = if steps > 0 { 0.5 / steps as f32 } else { 0.0 };
    for p in &mut positions {
        *p = ((*p + half_step) * tensor_w / content_w).clamp(0.0, 1.0);
    }
    positions
}

fn is_speck(bounds: &RectF, body_height: f32) -> bool {
    if bounds.height < SPECK_SIZE * body_height && bounds.width < SPECK_SIZE * body_height {
        return true;
    }
    bounds.height > GLYPH_HEIGHT * body_height && bounds.width < GLYPH_ASPECT * bounds.height
}

fn is_noise(bounds: &RectF, read: &ReadResult, body_height: f32) -> bool {
    if read.text.trim().is_empty() || read.confidence < MIN_CONFIDENCE {
        return true;
    }
    read.text.chars().count() <= 1
        && (bounds.height < SINGLE_CHAR_HEIGHT * body_height
            || read.confidence < SINGLE_CHAR_CONFIDENCE)
}

fn has_rule(img: &RgbImage, a: &RectF, b: &RectF) -> bool {
    let (top, bottom) = (a.top().min(b.top()), a.bottom().max(b.bottom()));
    let reach = (bottom - top) * RULE_REACH;
    let clamp_y = |y: f32| (y.max(0.0) as u32).min(img.height());
    let x0 = a.right().ceil().max(0.0) as u32;
    let x1 = (b.left().floor().max(0.0) as u32).min(img.width());
    let (above, y0, y1, below) = (
        clamp_y(top - reach),
        clamp_y(top),
        clamp_y(bottom),
        clamp_y(bottom + reach),
    );
    if x1 <= x0 || y1 <= y0 {
        return false;
    }
    let luma = |x: u32, y: u32| {
        let [r, g, b] = img.get_pixel(x, y).0;
        (i32::from(r) * 299 + i32::from(g) * 587 + i32::from(b) * 114) / 1000
    };
    let mut values: Vec<i32> = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| luma(x, y)))
        .collect();
    let mid = values.len() / 2;
    let background = *values.select_nth_unstable(mid).1;
    let covered = |x: u32, from: u32, to: u32| {
        let needed = ((to - from) as f32 * RULE_COVERAGE).ceil() as usize;
        to > from
            && (from..to)
                .filter(|&y| (luma(x, y) - background).abs() > RULE_CONTRAST)
                .count()
                >= needed
    };
    (x0..x1).any(|x| covered(x, y0, y1) && (covered(x, above, y0) || covered(x, y1, below)))
}

fn union_of(members: &[usize], boxes: &[RectF]) -> RectF {
    let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &i in members {
        l = l.min(boxes[i].left());
        t = t.min(boxes[i].top());
        r = r.max(boxes[i].right());
        b = b.max(boxes[i].bottom());
    }
    RectF::from_ltrb(l, t, r, b).unwrap_or(boxes[members[0]])
}

pub fn clip_line(mut line: OcrLine, area: RectF) -> Option<OcrLine> {
    line.bounds = line.bounds.intersect(area)?;
    for x in &mut line.char_x {
        *x = x.clamp(line.bounds.left(), line.bounds.right());
    }
    Some(line)
}

fn rect_iou(a: RectF, b: RectF) -> f32 {
    let Some(inter) = a.intersect(b) else {
        return 0.0;
    };
    let ia = inter.width * inter.height;
    let ua = a.width * a.height + b.width * b.height - ia;
    if ua <= 0.0 { 0.0 } else { ia / ua }
}

fn contains_mostly(outer: RectF, inner: RectF) -> bool {
    let Some(inter) = outer.intersect(inner) else {
        return false;
    };
    let inner_area = inner.width * inner.height;
    inner_area > 0.0 && (inter.width * inter.height) / inner_area > 0.80
}
