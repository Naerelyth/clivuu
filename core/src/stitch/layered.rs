//! Dense 2D multi-layer fallback matcher for scenes where global RGB row means
//! fail: fixed or parallax backgrounds, frosted-glass (`backdrop-filter`) cards,
//! bare text/terminals over sharp backgrounds, whole-window compositor opacity,
//! and multi-band interior stationary overlays.

use super::{Ambiguous, MAX_ACCEPT_ERROR, Match, Profile};
use crate::i18n::t;
use crate::image_ops::Result;

const GRID_MAX_WIDTH: usize = 384;
const GRID_WORDS: usize = GRID_MAX_WIDTH / 64;
const MAX_LAYERED_OUTLIER: f64 = 0.34;
const MAX_LAYERED_SCORE: f64 = 0.37;

#[derive(Clone, Copy, Default)]
struct RowBits([u64; GRID_WORDS]);

impl RowBits {
    fn full(width: usize) -> Self {
        let mut words = [0u64; GRID_WORDS];
        let full_words = width / 64;
        for slot in words.iter_mut().take(full_words) {
            *slot = u64::MAX;
        }
        let rem = width % 64;
        if rem > 0 && full_words < GRID_WORDS {
            words[full_words] = (1u64 << rem) - 1;
        }
        Self(words)
    }

    fn set(&mut self, x: usize) {
        self.0[x / 64] |= 1u64 << (x % 64);
    }

    fn get(self, x: usize) -> bool {
        (self.0[x / 64] & (1u64 << (x % 64))) != 0
    }

    fn count_ones(self) -> u32 {
        self.0.iter().map(|w| w.count_ones()).sum()
    }

    fn is_empty(self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }

    fn bitand(self, rhs: Self) -> Self {
        let mut out = [0u64; GRID_WORDS];
        for (slot, (&a, &b)) in out.iter_mut().zip(self.0.iter().zip(rhs.0.iter())) {
            *slot = a & b;
        }
        Self(out)
    }

    fn bitor(self, rhs: Self) -> Self {
        let mut out = [0u64; GRID_WORDS];
        for (slot, (&a, &b)) in out.iter_mut().zip(self.0.iter().zip(rhs.0.iter())) {
            *slot = a | b;
        }
        Self(out)
    }

    fn bitnot(self, domain: Self) -> Self {
        let mut out = [0u64; GRID_WORDS];
        for (slot, (&a, &m)) in out.iter_mut().zip(self.0.iter().zip(domain.0.iter())) {
            *slot = !a & m;
        }
        Self(out)
    }

    fn shl1(self, domain: Self) -> Self {
        let mut out = [0u64; GRID_WORDS];
        for (w, slot) in out.iter_mut().enumerate() {
            let carry = if w > 0 { self.0[w - 1] >> 63 } else { 0 };
            *slot = ((self.0[w] << 1) | carry) & domain.0[w];
        }
        Self(out)
    }

    fn shr1(self) -> Self {
        let mut out = [0u64; GRID_WORDS];
        for (w, slot) in out.iter_mut().enumerate() {
            let carry = if w + 1 < GRID_WORDS {
                self.0[w + 1] << 63
            } else {
                0
            };
            *slot = (self.0[w] >> 1) | carry;
        }
        Self(out)
    }

    fn dilate1(self, domain: Self) -> Self {
        self.bitor(self.shl1(domain)).bitor(self.shr1())
    }
}

struct DenseGrid {
    width: usize,
    rgb: Vec<[u8; 3]>,
    luma: Vec<u8>,
    hp: Vec<i8>,
    vp: Vec<i8>,
    vy: Vec<i8>,
    detail: Vec<RowBits>,
    valid: Vec<RowBits>,
}

impl DenseGrid {
    fn new(profile: &Profile) -> Self {
        let img_w = profile.image.width() as usize;
        let height = profile.image.height() as usize;
        let width = img_w.min(GRID_MAX_WIDTH);
        let domain = RowBits::full(width);
        let bins: Vec<_> = (0..width)
            .map(|bx| {
                let x0 = bx * img_w / width;
                let x1 = ((bx + 1) * img_w / width).max(x0 + 1);
                (x0, x1)
            })
            .collect();
        let raw = profile.image.as_raw();
        let mut rgb = vec![[0u8; 3]; height * width];
        let mut luma = vec![0u8; height * width];
        for y in 0..height {
            let row_base = y * img_w * 4;
            let dst_base = y * width;
            for (bx, &(x0, x1)) in bins.iter().enumerate() {
                let mut sr = 0u32;
                let mut sg = 0u32;
                let mut sb = 0u32;
                for x in x0..x1 {
                    let px = row_base + x * 4;
                    sr += u32::from(raw[px]);
                    sg += u32::from(raw[px + 1]);
                    sb += u32::from(raw[px + 2]);
                }
                let n = (x1 - x0) as u32;
                let r = (sr / n) as u8;
                let g = (sg / n) as u8;
                let b = (sb / n) as u8;
                rgb[dst_base + bx] = [r, g, b];
                luma[dst_base + bx] =
                    ((77 * u32::from(r) + 150 * u32::from(g) + 29 * u32::from(b)) >> 8) as u8;
            }
        }
        let rx = (width / 96).clamp(2, 4);
        let valid = if profile.excluded.is_empty() {
            vec![domain; height]
        } else {
            let mut invalid = vec![RowBits::default(); height];
            for (y, row_inv) in invalid.iter_mut().enumerate() {
                for (bx, &(x0, x1)) in bins.iter().enumerate() {
                    if profile.excluded.iter().any(|r| {
                        y as u32 >= r.y
                            && (y as u32) - r.y < r.height
                            && (x0 as u32) < r.x.saturating_add(r.width)
                            && (x1 as u32) > r.x
                    }) {
                        for nx in bx.saturating_sub(rx)..=(bx + rx).min(width - 1) {
                            row_inv.set(nx);
                        }
                    }
                }
            }
            (0..height)
                .map(|y| {
                    let mut m = RowBits::default();
                    for &inv in invalid
                        .iter()
                        .take((y + 3).min(height - 1) + 1)
                        .skip(y.saturating_sub(3))
                    {
                        m = m.bitor(inv);
                    }
                    m.bitnot(domain)
                })
                .collect()
        };

        let mut hp = vec![0i8; height * width];
        let mut vp = vec![0i8; height * width];
        let mut vy = vec![0i8; height * width];
        let mut detail = vec![RowBits::default(); height];
        let denom_x = (2 * rx + 1) as i16;
        for (y, det_row) in detail.iter_mut().enumerate() {
            let row_off = y * width;
            let y_up = y.saturating_sub(1) * width;
            let y_dn = (y + 1).min(height - 1) * width;
            let mut y_neighbors = [0usize; 7];
            for (k, slot) in y_neighbors.iter_mut().enumerate() {
                let ny = (y as isize + k as isize - 3).clamp(0, height as isize - 1) as usize;
                *slot = ny * width;
            }
            for bx in 0..width {
                let mut sx = 0i16;
                for dx in -(rx as isize)..=(rx as isize) {
                    let nx = (bx as isize + dx).clamp(0, width as isize - 1) as usize;
                    sx += i16::from(luma[row_off + nx]);
                }
                let g = i16::from(luma[row_off + bx]);
                let h_val = (g - sx / denom_x).clamp(-64, 64) as i8;
                let mut sy = 0i16;
                for &ny_off in &y_neighbors {
                    sy += i16::from(luma[ny_off + bx]);
                }
                let v_val = (g - sy / 7).clamp(-64, 64) as i8;
                let vy_val =
                    (i16::from(luma[y_dn + bx]) - i16::from(luma[y_up + bx])).clamp(-64, 64) as i8;
                hp[row_off + bx] = h_val;
                vp[row_off + bx] = v_val;
                vy[row_off + bx] = vy_val;
                let c_up = rgb[y_up + bx];
                let c_dn = rgb[y_dn + bx];
                let c_vy = c_up[0]
                    .abs_diff(c_dn[0])
                    .max(c_up[1].abs_diff(c_dn[1]))
                    .max(c_up[2].abs_diff(c_dn[2]));
                if v_val.unsigned_abs() >= 5 || vy_val.unsigned_abs() >= 10 || c_vy >= 12 {
                    det_row.set(bx);
                }
            }
        }
        Self {
            width,
            rgb,
            luma,
            hp,
            vp,
            vy,
            detail,
            valid,
        }
    }
}

struct LayeredContext<'a> {
    changed: &'a [RowBits],
    top: usize,
    bottom: usize,
}

struct LayeredScore {
    shift: i64,
    score: f64,
    raw_outlier: f64,
    inliers: u64,
    inlier_rows: Vec<usize>,
}

fn layered_stationary_mask(
    prev: &DenseGrid,
    curr: &DenseGrid,
    raw_ch: &[RowBits],
    top: usize,
    bottom: usize,
) -> Vec<RowBits> {
    let height = raw_ch.len();
    let domain = RowBits::full(prev.width);
    let mut quiet3 = vec![RowBits::default(); height];
    let mut static_vdet = vec![RowBits::default(); height];
    for y in 0..height {
        let valid = prev.valid[y].bitand(curr.valid[y]);
        let q = raw_ch[y].bitnot(domain).bitand(valid);
        quiet3[y] = q.bitand(q.shl1(domain)).bitand(q.shr1());
        let mut vd = RowBits::default();
        for neighbor in [y.saturating_sub(1), (y + 1).min(height - 1)] {
            if neighbor == y {
                continue;
            }
            let nq = q.bitand(raw_ch[neighbor].bitnot(domain));
            vd = vd.bitor(nq.bitand(prev.detail[y]).bitand(curr.detail[y]));
        }
        static_vdet[y] = vd.dilate1(domain);
    }
    let mut core = vec![RowBits::default(); height];
    let max_chord = (height / 2).max(4);
    for bx in 1..prev.width.saturating_sub(1) {
        let mut y = top;
        while y < bottom {
            if !quiet3[y].get(bx) {
                y += 1;
                continue;
            }
            let run_start = y;
            let mut first_vd = None;
            let mut last_vd = None;
            while y < bottom && quiet3[y].get(bx) {
                if static_vdet[y].get(bx) {
                    first_vd.get_or_insert(y);
                    last_vd = Some(y);
                }
                y += 1;
            }
            if let (Some(a), Some(b)) = (first_vd, last_vd)
                && b >= a + 3
                && b - a <= max_chord
                && !(run_start == top && y == bottom)
            {
                for row in core.iter_mut().take(b + 1).skip(a) {
                    row.set(bx - 1);
                    row.set(bx);
                    row.set(bx + 1);
                }
            }
        }
    }
    (0..height)
        .map(|y| {
            let mut m = RowBits::default();
            for &row_mask in core
                .iter()
                .take((y + 2).min(height - 1) + 1)
                .skip(y.saturating_sub(2))
            {
                m = m.bitor(row_mask);
            }
            m.dilate1(domain).bitand(raw_ch[y].bitnot(domain))
        })
        .collect()
}

fn score_layered_candidate(
    a: &DenseGrid,
    b: &DenseGrid,
    ctx: &LayeredContext<'_>,
    shift: usize,
    direction: i64,
    coarse: bool,
) -> Option<LayeredScore> {
    let width = a.width;
    let y_step = if coarse { 2 } else { 1 };
    let x_subsample = if coarse {
        0x5555_5555_5555_5555u64
    } else {
        u64::MAX
    };
    let min_row_inliers = (width / 96).clamp(2, 4) as u32;
    let mut active = 0u64;
    let mut mismatch = 0u64;
    let mut fine_err = 0u64;
    let mut inlier_rows = Vec::new();
    for y in (ctx.top..ctx.bottom - shift).step_by(y_step) {
        let (ay, by) = (y + shift, y);
        let a_base = ay * width;
        let b_base = by * width;
        let mut row_active = 0u32;
        let mut row_inliers = 0u32;
        for word_idx in 0..GRID_WORDS {
            let mut mask = ctx.changed[ay].0[word_idx]
                & ctx.changed[by].0[word_idx]
                & (a.detail[ay].0[word_idx] | b.detail[by].0[word_idx])
                & a.valid[ay].0[word_idx]
                & b.valid[by].0[word_idx]
                & x_subsample;
            while mask != 0 {
                let bit = mask.trailing_zeros() as usize;
                mask &= mask - 1;
                let bx = word_idx * 64 + bit;
                let ai = a_base + bx;
                let bi = b_base + bx;
                row_active += 1;
                let ad = (a.detail[ay].0[word_idx] & (1u64 << bit)) != 0;
                let bd = (b.detail[by].0[word_idx] & (1u64 << bit)) != 0;
                let ca = a.rgb[ai];
                let cb = b.rgb[bi];
                let d_rgb = ca[0]
                    .abs_diff(cb[0])
                    .max(ca[1].abs_diff(cb[1]))
                    .max(ca[2].abs_diff(cb[2]));
                let d_hp = a.hp[ai].abs_diff(b.hp[bi]);
                let d_vp = a.vp[ai].abs_diff(b.vp[bi]);
                let d_vy = a.vy[ai].abs_diff(b.vy[bi]);
                let leave_a = i16::from(a.luma[ai]) - i16::from(b.luma[ai]);
                let arrive_b = i16::from(b.luma[bi]) - i16::from(a.luma[bi]);
                let same_pol = (a.vp[ai] >= 0) == (b.vp[bi] >= 0);
                let inlier = (d_rgb <= 11 && d_vp <= 9 && d_hp <= 10)
                    || (ad && bd && d_rgb <= 9 && same_pol && d_vy <= 18)
                    || (ad && bd && d_hp <= 8 && d_vp <= 8 && d_vy <= 12)
                    || (ad
                        && bd
                        && (leave_a - arrive_b).abs() <= 9
                        && leave_a.abs() >= 14
                        && same_pol
                        && d_hp <= 14
                        && d_vp <= 14
                        && d_vy <= 18);
                if inlier {
                    row_inliers += 1;
                } else {
                    mismatch += 1;
                }
                let residual = d_rgb
                    .min(d_hp.saturating_add(d_vp))
                    .saturating_add(d_vy / 2);
                fine_err += u64::from(residual.min(24));
            }
        }
        active += u64::from(row_active);
        if !coarse && row_inliers >= min_row_inliers && row_inliers * 100 >= row_active * 55 {
            inlier_rows.push(y);
        }
    }
    let min_active = if coarse {
        10
    } else {
        (width / 4).clamp(20, 48) as u64
    };
    if active < min_active {
        return None;
    }
    let (prior_mis, prior_act) = if coarse { (5.0, 7.0) } else { (18.0, 24.0) };
    let reg_outlier = (mismatch as f64 + prior_mis) / (active as f64 + prior_act);
    let score = reg_outlier + 0.002 * (fine_err as f64 / active as f64);
    Some(LayeredScore {
        shift: shift as i64 * direction,
        score,
        raw_outlier: mismatch as f64 / active as f64,
        inliers: active - mismatch,
        inlier_rows,
    })
}

pub(super) fn match_layered_displacement(
    previous: &Profile,
    current: &Profile,
    bidirectional: bool,
    body: Option<(usize, usize)>,
) -> Result<(Match, i64)> {
    if previous.image.dimensions() != current.image.dimensions() {
        return Err(t("Capture dimensions changed; select the region again").into());
    }
    let height = previous.image.height() as usize;
    if height < 48 {
        return Err(t("The scrolling region must be at least 48 pixels high").into());
    }
    if previous.image.width() < 16 {
        return Err(t("Insufficient moving texture for reliable stitching").into());
    }
    let prev_grid = DenseGrid::new(previous);
    let curr_grid = DenseGrid::new(current);
    let width = prev_grid.width;
    let domain = RowBits::full(width);
    let (known_top, known_bottom) = body.unwrap_or((0, height));
    let mut raw_ch = vec![RowBits::default(); height];
    for (y, ch_row) in raw_ch
        .iter_mut()
        .enumerate()
        .take(known_bottom)
        .skip(known_top)
    {
        let valid = prev_grid.valid[y].bitand(curr_grid.valid[y]);
        let base = y * width;
        for word_idx in 0..GRID_WORDS {
            let mut vm = valid.0[word_idx];
            while vm != 0 {
                let bit = vm.trailing_zeros() as usize;
                vm &= vm - 1;
                let bx = word_idx * 64 + bit;
                let idx = base + bx;
                let ca = prev_grid.rgb[idx];
                let cb = curr_grid.rgb[idx];
                let d_rgb = ca[0]
                    .abs_diff(cb[0])
                    .max(ca[1].abs_diff(cb[1]))
                    .max(ca[2].abs_diff(cb[2]));
                if d_rgb > 8
                    || prev_grid.hp[idx].abs_diff(curr_grid.hp[idx]) > 5
                    || prev_grid.vp[idx].abs_diff(curr_grid.vp[idx]) > 5
                {
                    ch_row.0[word_idx] |= 1u64 << bit;
                }
            }
        }
    }
    if raw_ch.iter().all(|row| row.is_empty()) {
        if !previous.same_visible_rows(current, known_top, known_bottom) {
            return Err(t("Insufficient moving texture for reliable stitching").into());
        }
        return Ok((
            Match {
                shift: 0,
                header: 0,
                footer: 0,
                error: 0.,
            },
            1,
        ));
    }
    let min_row_ch = (width / 96).max(2) as u32;
    let active_y: Vec<_> = raw_ch
        .iter()
        .enumerate()
        .filter_map(|(y, row)| (row.count_ones() >= min_row_ch).then_some(y))
        .collect();
    if active_y.is_empty() {
        return Err(t("Insufficient moving texture for reliable stitching").into());
    }
    let mut groups = Vec::new();
    let mut start = 0;
    for n in 1..=active_y.len() {
        if n == active_y.len() || active_y[n] - active_y[n - 1] > 48.max(height / 5) {
            groups.push(&active_y[start..n]);
            start = n;
        }
    }
    let largest = groups.iter().map(|g| g.len()).max().unwrap_or(0);
    let mut retained = groups
        .into_iter()
        .filter(|g| g.len() > 8 || g.len() * 4 >= largest);
    let Some(first_group) = retained.next() else {
        return Err(t("Insufficient moving texture for reliable stitching").into());
    };
    let top = first_group[0];
    let bottom = retained.next_back().unwrap_or(first_group).last().unwrap() + 1;
    let body_len = bottom.saturating_sub(top);
    let maximum = body_len.saturating_sub(24.max(body_len / 3));
    if maximum == 0 {
        return Err(t("Insufficient moving texture for reliable stitching").into());
    }

    let stationary = layered_stationary_mask(&prev_grid, &curr_grid, &raw_ch, top, bottom);
    let changed: Vec<_> = (0..height)
        .map(|y| {
            raw_ch[y]
                .dilate1(domain)
                .bitand(stationary[y].bitnot(domain))
        })
        .collect();
    // Reject changes confined strictly to the outermost scrollbar margin (`<=6%`
    // width) so a fading overlay scrollbar at `shift == 0` is never matched as
    // a uniform self-similar foreground column.
    let edge = (prev_grid.width / 16).max(6).min(prev_grid.width / 3);
    let mut interior_mask = RowBits::default();
    for x in edge..prev_grid.width.saturating_sub(edge) {
        interior_mask.set(x);
    }
    let interior_changed: u32 = changed[top..bottom]
        .iter()
        .map(|row| row.bitand(interior_mask).count_ones())
        .sum();
    if interior_changed < 24 {
        return Err(t("Insufficient moving texture for reliable stitching").into());
    }
    let ctx = LayeredContext {
        changed: &changed,
        top,
        bottom,
    };

    let directions: &[i64] = if bidirectional { &[1, -1] } else { &[1] };
    let mut coarse_ranked = Vec::new();
    for &direction in directions {
        let (a, b) = if direction > 0 {
            (&prev_grid, &curr_grid)
        } else {
            (&curr_grid, &prev_grid)
        };
        for shift in 1..=maximum {
            if let Some(scored) = score_layered_candidate(a, b, &ctx, shift, direction, true) {
                coarse_ranked.push((scored.shift, scored.score));
            }
        }
    }
    coarse_ranked.sort_by(|a, b| a.1.total_cmp(&b.1));

    let mut to_refine = Vec::with_capacity(96);
    for &(dy, _) in coarse_ranked.iter().take(36) {
        to_refine.push(dy);
    }
    for &(dy, _) in coarse_ranked.iter().take(12) {
        let sign = dy.signum();
        let base = dy.unsigned_abs() as isize;
        for delta in -2..=2 {
            let s = base + delta;
            if (1..=maximum as isize).contains(&s) {
                to_refine.push(s as i64 * sign);
            }
        }
    }
    to_refine.sort_unstable();
    to_refine.dedup();

    let mut refined = Vec::with_capacity(to_refine.len());
    for dy in to_refine {
        let direction = dy.signum();
        let shift = dy.unsigned_abs() as usize;
        let (a, b) = if direction > 0 {
            (&prev_grid, &curr_grid)
        } else {
            (&curr_grid, &prev_grid)
        };
        if let Some(scored) = score_layered_candidate(a, b, &ctx, shift, direction, false) {
            refined.push(scored);
        }
    }
    refined.sort_by(|a, b| a.score.total_cmp(&b.score));
    let best = refined
        .first()
        .ok_or_else(|| t("No reliable overlap found"))?;
    let min_inliers = (width / 3).clamp(28, 96) as u64;
    if best.raw_outlier > MAX_LAYERED_OUTLIER
        || best.score > MAX_LAYERED_SCORE
        || best.inliers < min_inliers
    {
        return Err(t("No reliable overlap; scroll less and wait for animations to stop").into());
    }
    if refined.iter().any(|c| {
        (c.shift - best.shift).unsigned_abs() > 2
            && (c.score < best.score + 0.075_f64.max(best.score * 0.28)
                || (c.raw_outlier <= MAX_LAYERED_OUTLIER
                    && c.score <= MAX_LAYERED_SCORE
                    && c.inlier_rows.len() * 2 >= best.inlier_rows.len()))
    }) {
        return Err(Ambiguous.into());
    }
    let shift = best.shift.unsigned_abs() as usize;
    let min_span = ((bottom - top - shift) / 3).clamp(10, 24);
    if best.inlier_rows.len() < 8
        || best.inlier_rows.last().unwrap() - best.inlier_rows[0] < min_span
    {
        return Err(t("The overlap lacks texture for reliable stitching").into());
    }
    let header_row = best.inlier_rows[0];
    let footer_row = height - shift - 1 - best.inlier_rows.last().unwrap();
    Ok((
        Match {
            shift: shift as u32,
            header: header_row as u32,
            footer: footer_row as u32,
            error: (best.raw_outlier * (MAX_ACCEPT_ERROR / MAX_LAYERED_OUTLIER))
                .min(MAX_ACCEPT_ERROR),
        },
        best.shift.signum(),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::{Ambiguous, Profile, displacement, find_match};
    use crate::scrolling::{Axis, Document};
    use image::RgbaImage;

    fn sharp_background(x: u32, y: u32, height: u32) -> [u8; 3] {
        let xf = x as f64;
        let yf = y as f64;
        let wave = (xf * 0.045 + yf * 0.025).sin() * 58.0 + (yf * 0.07 - xf * 0.03).cos() * 46.0;
        let checker = if (x / 29 + y / 23).is_multiple_of(3) {
            52i32
        } else {
            0
        };
        let r = (36 + (y * 175 / height) as i32 + wave as i32 / 2 + checker).clamp(8, 248) as u8;
        let g = (88 + ((height - y) * 120 / height) as i32 - wave as i32 / 3 + checker / 2)
            .clamp(8, 248) as u8;
        let b = (128 + wave as i32 / 2 - checker / 2).clamp(8, 248) as u8;
        [r, g, b]
    }

    fn smooth_background(x: u32, y: u32, height: u32) -> [u8; 3] {
        let xf = x as f64;
        let yf = y as f64;
        let horizon = (xf * 0.015).sin() * 24.0;
        let band = ((yf + horizon) * 0.032).sin() * 68.0;
        let r = (24 + (y * 165 / height) as i32 + band as i32).clamp(10, 240) as u8;
        let g = (48 + (y * 130 / height) as i32 + (band * 0.8) as i32).clamp(12, 235) as u8;
        let b =
            (82 + ((height - y) * 110 / height) as i32 + (band * 0.4) as i32).clamp(16, 230) as u8;
        [r, g, b]
    }

    fn glyph_color(
        x: u32,
        doc_y: u32,
        left: u32,
        right: u32,
        periodic: Option<u32>,
    ) -> Option<[u8; 3]> {
        let row = periodic.map_or(doc_y, |p| doc_y % p);
        let line = row / 20;
        let ly = row % 20;
        if !(4..=15).contains(&ly) {
            return None;
        }
        let max_span = right.saturating_sub(left + 20).max(40);
        let span = if periodic.is_some() {
            max_span
        } else {
            48 + ((line.wrapping_mul(73)) % max_span)
        };
        if x < left + 10 || x >= (left + 10 + span).min(right - 8) {
            return None;
        }
        let cell = (x - left) / 8;
        if (x - left) % 8 >= 6 {
            return None;
        }
        let hash = cell.wrapping_mul(2_654_435_761)
            ^ line.wrapping_mul(2_246_822_519)
            ^ ly.wrapping_mul(3_266_489_917);
        if (hash >> 28) < 7 {
            return None;
        }
        let palette = [
            [236, 240, 246],
            [132, 202, 255],
            [168, 236, 150],
            [255, 190, 124],
        ];
        Some(palette[((line + cell) % 4) as usize])
    }

    #[test]
    fn fixed_background_with_frosted_glass_cards_gaps_and_floating_docks_stitches_cleanly() {
        let width = 360u32;
        let height = 320u32;
        let bottom_pill_color = [212u8, 42, 168];
        let top_badge_color = [40u8, 224, 196];
        let render = |scroll_y: u32| {
            RgbaImage::from_fn(width, height, |x, y| {
                // Fixed top-right corner icon badge.
                if (6..22).contains(&y) && (326..352).contains(&x) {
                    return image::Rgba([
                        top_badge_color[0],
                        top_badge_color[1],
                        top_badge_color[2],
                        255,
                    ]);
                }
                // Floating bottom composer dock with scrolling content still visible below y=294.
                if (262..294).contains(&y) && (56..304).contains(&x) {
                    return image::Rgba([
                        bottom_pill_color[0],
                        bottom_pill_color[1],
                        bottom_pill_color[2],
                        255,
                    ]);
                }
                // Floating circular scroll-to-bottom button above the composer dock.
                let dx = x as i32 - 180;
                let dy = y as i32 - 240;
                if dx * dx + dy * dy <= 11 * 11 {
                    return image::Rgba([38, 44, 56, 255]);
                }

                let doc_y = y + scroll_y;
                // Alternating narrow right-aligned cards and wider full-column cards with
                // 38 px vertical gaps exposing the fixed landscape background.
                let block = doc_y / 148;
                let local_y = doc_y % 148;
                let in_card_y = local_y < 110;
                let (card_left, card_right) = if block.is_multiple_of(2) {
                    (136u32, 326u32)
                } else {
                    (38u32, 322u32)
                };
                if in_card_y && (card_left..card_right).contains(&x) {
                    if let Some(ink) = glyph_color(x, doc_y, card_left, card_right, None) {
                        return image::Rgba([ink[0], ink[1], ink[2], 255]);
                    }
                    let bg = smooth_background(x, y, height);
                    // Frosted-glass card fill: 58% dark slate + 42% fixed background bleed.
                    return image::Rgba([
                        ((22 * 58 + u32::from(bg[0]) * 42) / 100) as u8,
                        ((34 * 58 + u32::from(bg[1]) * 42) / 100) as u8,
                        ((44 * 58 + u32::from(bg[2]) * 42) / 100) as u8,
                        255,
                    ]);
                }
                let raw_bg = sharp_background(x, y, height);
                image::Rgba([raw_bg[0], raw_bg[1], raw_bg[2], 255])
            })
        };

        let f0 = Profile::new(render(0));
        let f1 = Profile::new(render(62));
        assert_eq!(find_match(&f0, &f1).unwrap().shift, 62);
        let (fwd, fwd_dir) = displacement(&f0, &f1, None).unwrap();
        assert_eq!((fwd.shift, fwd_dir), (62, 1));
        let (rev, rev_dir) = displacement(&f1, &f0, None).unwrap();
        assert_eq!((rev.shift, rev_dir), (62, -1));
        // Non-overlapping jump over the identical fixed background must still fail.
        assert!(displacement(&f0, &Profile::new(render(520)), None).is_err());

        // Full Document stitch in BOTH forward and reverse scroll orders must
        // keep floating docks/badges out of interior seams.
        for reverse in [false, true] {
            let mut doc = Document::new(Axis::Vertical, 30);
            let offsets = if reverse {
                [180u32, 120, 60, 0]
            } else {
                [0u32, 60, 120, 180]
            };
            for off in offsets {
                doc.push(render(off))
                    .unwrap_or_else(|e| panic!("reverse={reverse}, off={off}: {e}"));
            }
            let stitched = doc.image().unwrap();
            assert_eq!(stitched.dimensions(), (width, height + 180));
            // Interior canvas rows where frame 0/1/2 had the bottom pill (262..294,
            // 322..354, 382..414) must be replaced by clean centered viewport rows,
            // while only the final bottommost pill at 442..474 remains.
            for clean_y in [275u32, 335, 395] {
                assert_ne!(
                    stitched.get_pixel(160, clean_y).0[..3],
                    bottom_pill_color,
                    "reverse={reverse}, ghost pill at y={clean_y}"
                );
            }
            assert_eq!(stitched.get_pixel(160, 456).0[..3], bottom_pill_color);
            // Likewise, the top-right corner badge appears at y=12 of the first
            // frame and must not repeat at y=72, 132, or 192.
            assert_eq!(stitched.get_pixel(336, 12).0[..3], top_badge_color);
            for interior_y in [72u32, 132, 192] {
                assert_ne!(
                    stitched.get_pixel(336, interior_y).0[..3],
                    top_badge_color,
                    "reverse={reverse}, ghost badge at y={interior_y}"
                );
            }
        }
    }

    #[test]
    fn bare_text_and_translucent_terminal_over_sharp_fixed_background_match_and_reject_periodic() {
        let width = 340u32;
        let height = 280u32;

        // A: 0%-alpha terminal/HUD text directly over a sharp unblurred background.
        let bare = |scroll_y: u32| {
            RgbaImage::from_fn(width, height, |x, y| {
                let c = glyph_color(x, y + scroll_y, 16, width - 16, None)
                    .unwrap_or_else(|| sharp_background(x, y, height));
                image::Rgba([c[0], c[1], c[2], 255])
            })
        };
        let b0 = Profile::new(bare(0));
        let b1 = Profile::new(bare(74));
        assert_eq!(find_match(&b0, &b1).unwrap().shift, 74);
        assert_eq!(displacement(&b0, &b1, None).unwrap().0.shift, 74);
        let (brev, bdir) = displacement(&b1, &b0, None).unwrap();
        assert_eq!((brev.shift, bdir), (74, -1));
        assert!(displacement(&b0, &Profile::new(bare(460)), None).is_err());

        // B: Whole-window compositor opacity (76% dark terminal + 24% sharp background).
        let alpha_win = |scroll_y: u32| {
            RgbaImage::from_fn(width, height, |x, y| {
                let fg = glyph_color(x, y + scroll_y, 16, width - 16, None).unwrap_or([24, 26, 34]);
                let bg = sharp_background(x, y, height);
                image::Rgba([
                    ((u32::from(fg[0]) * 76 + u32::from(bg[0]) * 24) / 100) as u8,
                    ((u32::from(fg[1]) * 76 + u32::from(bg[1]) * 24) / 100) as u8,
                    ((u32::from(fg[2]) * 76 + u32::from(bg[2]) * 24) / 100) as u8,
                    255,
                ])
            })
        };
        let a0 = Profile::new(alpha_win(0));
        let a1 = Profile::new(alpha_win(68));
        assert_eq!(displacement(&a0, &a1, None).unwrap().0.shift, 68);
        assert_eq!(displacement(&a1, &a0, None).unwrap().1, -1);
        assert!(displacement(&a0, &Profile::new(alpha_win(460)), None).is_err());

        // C: Periodic foreground (period = 100, 5 lines * 20 px) over sharp fixed background
        // must still be rejected as Ambiguous rather than picking an arbitrary alias.
        let periodic = |scroll_y: u32| {
            RgbaImage::from_fn(width, 320, |x, y| {
                let c = glyph_color(x, y + scroll_y, 16, width - 16, Some(100))
                    .unwrap_or_else(|| sharp_background(x, y, 320));
                image::Rgba([c[0], c[1], c[2], 255])
            })
        };
        let err = displacement(
            &Profile::new(periodic(0)),
            &Profile::new(periodic(40)),
            None,
        )
        .unwrap_err();
        assert!(err.is::<Ambiguous>(), "expected Ambiguous, got: {err}");
    }
}
