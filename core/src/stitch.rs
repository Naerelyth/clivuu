mod layered;

use crate::i18n::t;
use crate::image_ops::{MAX_PIXELS, Result, load, save};
use image::RgbaImage;
use layered::match_layered_displacement;
use serde::Serialize;
use std::sync::Arc;
const SAMPLES: usize = 128;
const BAND: usize = SAMPLES / 4;
const FINE_BANDS: usize = 8;
const FINE_BAND_WIDTH: usize = SAMPLES / FINE_BANDS;
const MAX_ACCEPT_ERROR: f64 = 9.5;

#[derive(Debug, Serialize)]
pub struct Match {
    pub shift: u32,
    pub header: u32,
    pub footer: u32,
    pub error: f64,
}

pub struct Profile {
    pub image: Arc<RgbaImage>,
    rows: Vec<[u8; SAMPLES * 3]>,
    valid: Option<Vec<u128>>,
    texture: Vec<u128>,
    pub(super) excluded: Vec<crate::scrolling::ExcludedRect>,
}

impl Profile {
    pub fn new(image: RgbaImage) -> Self {
        Self::from_arc(Arc::new(image))
    }
    pub fn from_arc(image: Arc<RgbaImage>) -> Self {
        let rows: Vec<_> = (0..image.height())
            .map(|y| {
                let mut row = [0; SAMPLES * 3];
                for i in 0..SAMPLES {
                    let x = (f64::from(image.width() - 1) * i as f64 / (SAMPLES - 1) as f64).round()
                        as u32;
                    let pixel = image.get_pixel(x, y);
                    row[i * 3..i * 3 + 3].copy_from_slice(&pixel.0[..3]);
                }
                row
            })
            .collect();
        // Cache spatial edges once. Uniform background must never count as
        // evidence for a displacement, even when it changed at the same y.
        let texture = rows
            .iter()
            .enumerate()
            .map(|(y, row)| {
                let mut edges = 0u128;
                for i in 0..SAMPLES {
                    if (0..3).any(|c| {
                        row[i * 3 + c].abs_diff(row[i.saturating_sub(1) * 3 + c]) > 2
                            || row[i * 3 + c].abs_diff(row[(i + 1).min(SAMPLES - 1) * 3 + c]) > 2
                            || row[i * 3 + c].abs_diff(rows[y.saturating_sub(1)][i * 3 + c]) > 2
                            || row[i * 3 + c].abs_diff(rows[(y + 1).min(rows.len() - 1)][i * 3 + c])
                                > 2
                    }) {
                        edges |= 1u128 << i;
                    }
                }
                edges
            })
            .collect();
        Self {
            image,
            rows,
            valid: None,
            texture,
            excluded: Vec::new(),
        }
    }
    pub(crate) fn masked(image: RgbaImage, excluded: &[crate::scrolling::ExcludedRect]) -> Self {
        Self::masked_arc(Arc::new(image), excluded)
    }
    pub(crate) fn masked_arc(
        image: Arc<RgbaImage>,
        excluded: &[crate::scrolling::ExcludedRect],
    ) -> Self {
        let mut profile = Self::from_arc(image);
        if !excluded.is_empty() {
            profile.excluded = excluded.to_vec();
            profile.valid = Some(
                (0..profile.image.height())
                    .map(|y| {
                        let mut valid = 0;
                        for i in 0..SAMPLES {
                            let x = ((profile.image.width() - 1) as f64 * i as f64
                                / (SAMPLES - 1) as f64)
                                .round() as u32;
                            if !excluded.iter().any(|r| r.contains(x, y)) {
                                valid |= 1 << i;
                            }
                        }
                        valid
                    })
                    .collect(),
            );
        }
        profile
    }
    pub(crate) fn same_visible_view(&self, other: &Self) -> bool {
        self.same_visible_rows(other, 0, self.rows.len())
    }
    pub(super) fn same_visible_rows(&self, other: &Self, top: usize, bottom: usize) -> bool {
        if self.image.dimensions() != other.image.dimensions() {
            return false;
        }
        let mut visible = 0;
        for y in top..bottom {
            if let Some(error) = self.compare(y, other, y, &[0, 1, 2, 3]) {
                visible += 1;
                if error >= 1.3 {
                    return false;
                }
            }
        }
        visible >= 24
    }
    fn texture_valid(&self, y: usize) -> u128 {
        self.valid.as_ref().map_or(u128::MAX, |v| {
            v[y] & v[y.saturating_sub(1)] & v[(y + 1).min(v.len() - 1)]
        })
    }
    fn changes(&self, other: &Self, y: usize, threshold: u8) -> u128 {
        let valid = self.valid.as_ref().map_or(u128::MAX, |v| v[y])
            & other.valid.as_ref().map_or(u128::MAX, |v| v[y]);
        let mut changed = 0u128;
        for i in 0..SAMPLES {
            if valid & (1u128 << i) != 0
                && (0..3).any(|c| {
                    self.rows[y][i * 3 + c].abs_diff(other.rows[y][i * 3 + c]) >= threshold
                })
            {
                changed |= 1u128 << i;
            }
        }
        changed
    }
    fn changed_distance(&self, y: usize, other: &Self, z: usize, mask: u128) -> Option<f64> {
        let mut valid = mask
            & self.valid.as_ref().map_or(u128::MAX, |v| v[y])
            & other.valid.as_ref().map_or(u128::MAX, |v| v[z]);
        let count = valid.count_ones();
        let mut sum = 0u32;
        while valid != 0 {
            let i = valid.trailing_zeros() as usize;
            valid &= valid - 1;
            for c in 0..3 {
                sum += self.rows[y][i * 3 + c].abs_diff(other.rows[z][i * 3 + c]) as u32;
            }
        }
        (count > 0).then(|| sum as f64 / (count * 3) as f64)
    }
    fn compare(&self, y: usize, other: &Self, z: usize, bands: &[usize]) -> Option<f64> {
        let valid = self.valid.as_ref().map_or(u128::MAX, |v| v[y])
            & other.valid.as_ref().map_or(u128::MAX, |v| v[z]);
        if valid == u128::MAX && bands.len() == 4 {
            return Some(distance(&self.rows[y], &other.rows[z]));
        }
        let mut count = 0;
        let mut sum = 0u32;
        for &band in bands {
            for i in band * BAND..(band + 1) * BAND {
                if valid & (1 << i) != 0 {
                    count += 1;
                    for c in 0..3 {
                        sum += self.rows[y][i * 3 + c].abs_diff(other.rows[z][i * 3 + c]) as u32;
                    }
                }
            }
        }
        (count >= 8).then(|| sum as f64 / (count * 3) as f64)
    }

    fn mostly_stationary(&self, other: &Self, top: usize, bottom: usize) -> bool {
        if self.image.dimensions() != other.image.dimensions() {
            return false;
        }
        let height = self.rows.len();
        let top = top.min(height);
        let bottom = bottom.min(height);
        let span = bottom.saturating_sub(top);
        if height < 48 || span < 24 {
            return false;
        }
        let mut visible_rows = 0usize;
        let mut textured_rows = 0usize;
        let mut band_all = [0u64; FINE_BANDS];
        let mut band_all_same = [0u64; FINE_BANDS];
        let mut band_tex = [0u64; FINE_BANDS];
        let mut band_same = [0u64; FINE_BANDS];
        let mut band_err = [0u64; FINE_BANDS];
        let mut band_changed_rows = [0usize; FINE_BANDS];
        for y in top..bottom {
            let row_valid = self.valid.as_ref().map_or(u128::MAX, |v| v[y])
                & other.valid.as_ref().map_or(u128::MAX, |v| v[y]);
            if row_valid.count_ones() >= 8 {
                visible_rows += 1;
            }
            let mut rv = row_valid;
            let mut row_changed_band = [false; FINE_BANDS];
            while rv != 0 {
                let x = rv.trailing_zeros() as usize;
                rv &= rv - 1;
                let band = x / FINE_BAND_WIDTH;
                band_all[band] += 1;
                let max_ch = (0..3)
                    .map(|c| self.rows[y][x * 3 + c].abs_diff(other.rows[y][x * 3 + c]))
                    .max()
                    .unwrap_or(0);
                if max_ch <= 3 {
                    band_all_same[band] += 1;
                } else {
                    row_changed_band[band] = true;
                }
            }
            let valid = self.texture_valid(y) & other.texture_valid(y);
            let valid = valid & ((valid << 1) | 1) & ((valid >> 1) | (1u128 << 127));
            let mut mask = (self.texture[y] | other.texture[y]) & valid;
            if mask != 0 {
                textured_rows += 1;
            }
            while mask != 0 {
                let x = mask.trailing_zeros() as usize;
                mask &= mask - 1;
                let band = x / FINE_BAND_WIDTH;
                band_tex[band] += 1;
                let mut max_ch = 0u8;
                let mut px_err = 0u64;
                for c in 0..3 {
                    let diff = self.rows[y][x * 3 + c].abs_diff(other.rows[y][x * 3 + c]);
                    max_ch = max_ch.max(diff);
                    px_err += u64::from(diff);
                }
                band_err[band] += px_err;
                if max_ch <= 3 {
                    band_same[band] += 1;
                }
            }
            for (band, &changed) in row_changed_band.iter().enumerate() {
                if changed {
                    band_changed_rows[band] += 1;
                }
            }
        }
        let total_all: u64 = band_all.iter().sum();
        let total_all_same: u64 = band_all_same.iter().sum();
        let total_tex: u64 = band_tex.iter().sum();
        if visible_rows < 24 || textured_rows < 4 || total_tex < 12 || total_all < 64 {
            return false;
        }
        let total_same: u64 = band_same.iter().sum();
        let total_err: u64 = band_err.iter().sum();
        let mut rest_all = total_all;
        let mut rest_all_same = total_all_same;
        let mut rest_tex = total_tex;
        let mut rest_same = total_same;
        let mut rest_err = total_err;
        let active_bands = band_all.iter().filter(|&&n| n >= 16).count();
        if active_bands >= 3
            && let Some(worst) = (0..FINE_BANDS)
                .filter(|&b| band_all[b] > 0 && band_all_same[b] < band_all[b])
                .max_by_key(|&b| (band_all[b] - band_all_same[b], band_err[b]))
        {
            let edge_band = worst == 0 || worst == FINE_BANDS - 1;
            let localized_rows = band_changed_rows[worst] * 4 <= span.max(16);
            if (edge_band || localized_rows)
                && total_tex - band_tex[worst] >= 8
                && total_all - band_all[worst] >= 48
            {
                rest_all = total_all - band_all[worst];
                rest_all_same = total_all_same - band_all_same[worst];
                rest_tex = total_tex - band_tex[worst];
                rest_same = total_same - band_same[worst];
                rest_err = total_err - band_err[worst];
            }
        }
        rest_all_same * 100 >= rest_all * 96
            && rest_same * 100 >= rest_tex * 94
            && (rest_err as f64) / ((rest_tex * 3) as f64) < 1.2
    }
}

fn distance(a: &[u8; SAMPLES * 3], b: &[u8; SAMPLES * 3]) -> f64 {
    let mut bands = [0.0_f64; 4];
    for (band, value) in bands.iter_mut().enumerate() {
        *value = (band * BAND * 3..(band + 1) * BAND * 3)
            .map(|i| f64::from(a[i].abs_diff(b[i])))
            .sum::<f64>()
            / (BAND * 3) as f64;
    }
    bands.iter().sum::<f64>() / 4.0
}

/// The CLI's ordered frames and the desktop's bidirectional capture share the
/// same candidate evidence and confidence rules.
pub fn find_match(previous: &Profile, current: &Profile) -> Result<Match> {
    let primary = match_displacement(previous, current, false, None);
    if let Ok((matched, _)) = primary {
        return Ok(matched);
    }
    if primary.as_ref().is_err_and(|e| e.is::<Ambiguous>()) {
        if previous.mostly_stationary(current, 0, previous.rows.len()) {
            return Ok(Match {
                shift: 0,
                header: 0,
                footer: 0,
                error: 0.,
            });
        }
        return primary.map(|(matched, _)| matched);
    }
    let layered = match_layered_displacement(previous, current, false, None);
    if let Ok((matched, _)) = layered {
        return Ok(matched);
    }
    if previous.mostly_stationary(current, 0, previous.rows.len()) {
        return Ok(Match {
            shift: 0,
            header: 0,
            footer: 0,
            error: 0.,
        });
    }
    if layered.as_ref().is_err_and(|e| e.is::<Ambiguous>()) {
        return layered.map(|(matched, _)| matched);
    }
    primary.map(|(matched, _)| matched)
}

pub(crate) fn displacement(
    previous: &Profile,
    current: &Profile,
    body: Option<(usize, usize)>,
) -> Result<(Match, i64)> {
    let height = previous.rows.len();
    let (known_top, known_bottom) = body.unwrap_or((0, height));
    let is_stationary = || {
        previous.mostly_stationary(current, known_top, known_bottom)
            || ((known_top, known_bottom) != (0, height)
                && previous.mostly_stationary(current, 0, height))
    };
    let stationary_ok = || {
        Ok((
            Match {
                shift: 0,
                header: 0,
                footer: 0,
                error: 0.,
            },
            1,
        ))
    };
    // Fixed boundaries can temporarily include a blank paragraph. Full evidence
    // must retain the first chance to correct them. Retry the known body only
    // when unrelated chrome changes prevented a match; never override real
    // non-stationary ambiguity, but a genuinely unmoved frame (`shift == 0`
    // with fading scrollbar/caret) beats nonzero periodic aliases decisively.
    let full = match_displacement(previous, current, true, None);
    if full.is_ok() {
        return full;
    }
    if full.as_ref().is_err_and(|e| e.is::<Ambiguous>()) {
        return if is_stationary() {
            stationary_ok()
        } else {
            full
        };
    }
    if let Some(known) = body.filter(|&b| b != (0, height)) {
        let scoped = match_displacement(previous, current, true, Some(known));
        if scoped.is_ok() {
            return scoped;
        }
        if scoped.as_ref().is_err_and(|e| e.is::<Ambiguous>()) {
            return if is_stationary() {
                stationary_ok()
            } else {
                scoped
            };
        }
    }
    // Translucent sticky overlays or animated scroll-edge shadows can perturb
    // the outermost sixth of a viewport without making the interior ambiguous.
    if height >= 120 {
        let margin = height / 6;
        let inner_top = known_top.max(margin);
        let inner_bottom = known_bottom.min(height - margin);
        if inner_bottom.saturating_sub(inner_top) >= 64
            && (inner_top, inner_bottom) != body.unwrap_or((0, height))
        {
            let inner =
                match_displacement(previous, current, true, Some((inner_top, inner_bottom)));
            if inner.is_ok() {
                return inner;
            }
            if inner.as_ref().is_err_and(|e| e.is::<Ambiguous>()) {
                return if is_stationary() {
                    stationary_ok()
                } else {
                    inner
                };
            }
        }
    }
    let layered_full = match_layered_displacement(previous, current, true, None);
    if layered_full.is_ok() {
        return layered_full;
    }
    if layered_full.as_ref().is_err_and(|e| e.is::<Ambiguous>()) {
        return if is_stationary() {
            stationary_ok()
        } else {
            layered_full
        };
    }
    if let Some(known) = body.filter(|&b| b != (0, height)) {
        let layered_scoped = match_layered_displacement(previous, current, true, Some(known));
        if layered_scoped.is_ok() {
            return layered_scoped;
        }
        if layered_scoped.as_ref().is_err_and(|e| e.is::<Ambiguous>()) {
            return if is_stationary() {
                stationary_ok()
            } else {
                layered_scoped
            };
        }
    }
    if is_stationary() {
        return stationary_ok();
    }
    full
}

#[derive(Debug)]
pub(crate) struct Ambiguous;
impl std::fmt::Display for Ambiguous {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&t(
            "Repeated texture makes the offset ambiguous; scroll a smaller distance",
        ))
    }
}
impl std::error::Error for Ambiguous {}

#[derive(Clone, Copy)]
struct Candidate {
    shift: i64,
    error: f64,
    raw_error: f64,
    lower_bound: f64,
    band_mask: u128,
}
struct Score {
    error: f64,
    raw_error: f64,
    lower_bound: f64,
    band_mask: u128,
}

struct MotionFilter {
    moving_cols: u128,
    stationary: Vec<u128>,
}

impl MotionFilter {
    fn new(
        previous: &Profile,
        current: &Profile,
        changes: &[u128],
        top: usize,
        bottom: usize,
    ) -> Self {
        let height = previous.rows.len();
        let body = bottom.saturating_sub(top);
        let mut col_changed = [0usize; SAMPLES];
        let mut col_textured = [0usize; SAMPLES];
        for (y, &change_row) in changes.iter().enumerate().take(bottom).skip(top) {
            let valid = previous.texture_valid(y) & current.texture_valid(y);
            let tex = (previous.texture[y] | current.texture[y]) & valid;
            let mut ch = change_row & valid;
            while ch != 0 {
                let i = ch.trailing_zeros() as usize;
                ch &= ch - 1;
                col_changed[i] += 1;
            }
            let mut tx = tex;
            while tx != 0 {
                let i = tx.trailing_zeros() as usize;
                tx &= tx - 1;
                col_textured[i] += 1;
            }
        }
        let peak_changed = col_changed.iter().copied().max().unwrap_or(0);
        let moving_cols = if peak_changed >= 6 {
            let mut raw_moving = 0u128;
            let floor = 2.max(body / 64);
            for (i, &changed_rows) in col_changed.iter().enumerate() {
                if changed_rows * 10 >= peak_changed || changed_rows > floor {
                    raw_moving |= 1u128 << i;
                }
            }
            let dilated_moving = raw_moving | (raw_moving << 1) | (raw_moving >> 1);
            let mut stationary_cols = 0u128;
            for (i, &tex_rows) in col_textured.iter().enumerate() {
                if tex_rows > 0 && (dilated_moving & (1u128 << i)) == 0 {
                    stationary_cols |= 1u128 << i;
                }
            }
            let mask = !stationary_cols;
            if mask == 0 { u128::MAX } else { mask }
        } else {
            u128::MAX
        };

        // Detect 2D stationary interior widgets (floating buttons, watermarks,
        // sticky badges/banners) as 3-column unbroken quiet chords connecting
        // two static vertical-texture rows within half the viewport height.
        // Both rows of a vertical edge must be unchanged so blank whitespace
        // bordering a moving label is never mistaken for stationary chrome.
        let mut quiet3 = vec![0u128; height];
        let mut vtex = vec![0u128; height];
        for y in 0..height {
            let valid = previous.texture_valid(y) & current.texture_valid(y);
            let q = !changes[y] & valid;
            quiet3[y] = q & (q << 1) & (q >> 1);
            let mut vt = 0u128;
            for neighbor in [y.saturating_sub(1), (y + 1).min(height - 1)] {
                if neighbor == y {
                    continue;
                }
                let nq = q & !changes[neighbor];
                let mut mask = nq;
                while mask != 0 {
                    let i = mask.trailing_zeros() as usize;
                    mask &= mask - 1;
                    if (0..3).any(|c| {
                        previous.rows[y][i * 3 + c].abs_diff(previous.rows[neighbor][i * 3 + c]) > 2
                            && current.rows[y][i * 3 + c]
                                .abs_diff(current.rows[neighbor][i * 3 + c])
                                > 2
                    }) {
                        vt |= 1u128 << i;
                    }
                }
            }
            vtex[y] = vt | (vt << 1) | (vt >> 1);
        }
        let mut core_stationary = vec![0u128; height];
        let max_chord = (height / 2).max(4);
        for i in 1..SAMPLES - 1 {
            let bit = 1u128 << i;
            let mut y = top;
            while y < bottom {
                if quiet3[y] & bit == 0 {
                    y += 1;
                    continue;
                }
                let run_start = y;
                let mut first_vtex = None;
                let mut last_vtex = None;
                while y < bottom && (quiet3[y] & bit) != 0 {
                    if (vtex[y] & bit) != 0 {
                        first_vtex.get_or_insert(y);
                        last_vtex = Some(y);
                    }
                    y += 1;
                }
                if let (Some(a), Some(b)) = (first_vtex, last_vtex)
                    && b >= a + 3
                    && b - a <= max_chord
                    && !(run_start == top && y == bottom)
                {
                    for row in core_stationary.iter_mut().take(b + 1).skip(a) {
                        *row |= bit | (bit << 1) | (bit >> 1);
                    }
                }
            }
        }
        let mut stationary = vec![0u128; height];
        for (y, slot) in stationary.iter_mut().enumerate() {
            let mut m = 0u128;
            for &row_mask in core_stationary
                .iter()
                .take((y + 2).min(height - 1) + 1)
                .skip(y.saturating_sub(2))
            {
                m |= row_mask;
            }
            *slot = (m | (m << 1) | (m >> 1)) & !changes[y];
        }
        Self {
            moving_cols,
            stationary,
        }
    }

    fn valid_at(&self, ay: usize, by: usize) -> u128 {
        let mask = self.moving_cols & !self.stationary[ay] & !self.stationary[by];
        if mask == 0 { u128::MAX } else { mask }
    }
}

fn match_displacement(
    previous: &Profile,
    current: &Profile,
    bidirectional: bool,
    body: Option<(usize, usize)>,
) -> Result<(Match, i64)> {
    if previous.image.dimensions() != current.image.dimensions() {
        return Err(t("Capture dimensions changed; select the region again").into());
    }
    let height = previous.rows.len();
    if height < 48 {
        return Err(t("The scrolling region must be at least 48 pixels high").into());
    }
    let (known_top, known_bottom) = body.unwrap_or((0, height));
    let changes: Vec<_> = (0..height)
        .map(|y| {
            if (known_top..known_bottom).contains(&y) {
                previous.changes(current, y, 2)
            } else {
                0
            }
        })
        .collect();
    if changes.iter().all(|&mask| mask == 0) {
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
    let (top, bottom) = changing_interval(&changes)
        .ok_or_else(|| t("Insufficient moving texture for reliable stitching"))?;
    let body = bottom - top;
    let filter = MotionFilter::new(previous, current, &changes, top, bottom);
    // Very short overlaps can contain just one repeated icon with none of its
    // unique labels. Require at least a third of the changing body in BOTH
    // directions before a candidate may participate in confidence decisions.
    let maximum = body.saturating_sub(24.max(body / 3));
    let mut ranked = Vec::new();
    let directions: &[i64] = if bidirectional { &[1, -1] } else { &[1] };
    for &direction in directions {
        for shift in 1..=maximum {
            if let Some(score) = candidate_score(
                previous, current, &filter, top, bottom, shift, direction, 64, None,
            ) {
                ranked.push(Candidate {
                    shift: shift as i64 * direction,
                    error: score.error,
                    raw_error: score.raw_error,
                    lower_bound: score.lower_bound,
                    band_mask: score.band_mask,
                });
            }
        }
    }
    ranked.sort_by(|a, b| a.error.total_cmp(&b.error));
    // Refine every plausible candidate, not just a single winner in each
    // direction. An ambiguous forward match must not legitimise a reverse alias.
    let mut refined = Vec::new();
    // A non-winning candidate above the acceptance limit may still be inside
    // the best legal winner's ambiguity interval (MAX_ACCEPT_ERROR + 15%).
    let mut bound = MAX_ACCEPT_ERROR * 1.15;
    for candidate in ranked {
        // Coarse samples are a subset of the full evidence, so their summed
        // error divided by ALL pixels is a true lower bound. Approximate means
        // only order the work; they must never discard a possible match/alias.
        if candidate.lower_bound > bound {
            continue;
        }
        if let Some(score) = candidate_score(
            previous,
            current,
            &filter,
            top,
            bottom,
            candidate.shift.unsigned_abs() as usize,
            candidate.shift.signum(),
            usize::MAX,
            Some(bound),
        ) {
            let error = score.error;
            bound = bound.min(error + 0.05_f64.max(error * 0.15));
            refined.push(Candidate {
                error,
                raw_error: score.raw_error,
                band_mask: score.band_mask,
                ..candidate
            });
        }
    }
    refined.sort_by(|a, b| a.error.total_cmp(&b.error));
    let best = refined
        .first()
        .ok_or_else(|| t("No reliable overlap found"))?;
    if best.error > MAX_ACCEPT_ERROR {
        return Err(t("No reliable overlap; scroll less and wait for animations to stop").into());
    }
    // Never let a periodic alias discard a unique ID/line-number band (`c.band_mask
    // != best.band_mask`) solely to manufacture an artificial tie against a match
    // that needed no band trimming (or trimmed a different scrollbar band).
    if refined.iter().any(|c| {
        let comparable_err = if c.band_mask == best.band_mask {
            c.error
        } else {
            c.raw_error
        };
        (c.shift - best.shift).unsigned_abs() > 2
            && comparable_err < best.error + 0.05_f64.max(best.error * 0.15)
    }) {
        return Err(Ambiguous.into());
    }
    let shift = best.shift.unsigned_abs() as usize;
    let (a, b) = if best.shift > 0 {
        (previous, current)
    } else {
        (current, previous)
    };
    let row_limit = (best.error * 1.35).clamp(4.5, 8.0);
    let min_span = ((bottom - top - shift) / 2).clamp(12, 24);
    let moving: Vec<_> = (top..bottom - shift)
        .filter(|&y| {
            let (ay, by) = if best.shift > 0 {
                (y + shift, y)
            } else {
                (y, y + shift)
            };
            let active = filter.valid_at(ay, by) & best.band_mask;
            let mask_y = changes[y] & active;
            let mask_shifted = changes[y + shift] & active;
            if mask_y == 0 || mask_shifted == 0 {
                return false;
            }
            let Some(shifted_diff) = a.changed_distance(y + shift, b, y, mask_y) else {
                return false;
            };
            let Some(in_place_diff) = previous.changed_distance(y, current, y, mask_y) else {
                return false;
            };
            shifted_diff < row_limit && shifted_diff * 1.8 < in_place_diff
        })
        .collect();
    if moving.len() < 8 || moving.last().unwrap() - moving[0] < min_span {
        return Err(t("The overlap lacks texture for reliable stitching").into());
    }
    Ok((
        Match {
            shift: shift as u32,
            header: moving[0] as u32,
            footer: (height - shift - 1 - moving.last().unwrap()) as u32,
            error: best.error,
        },
        best.shift.signum(),
    ))
}

fn changing_interval(changes: &[u128]) -> Option<(usize, usize)> {
    let active: Vec<_> = changes
        .iter()
        .enumerate()
        .filter_map(|(y, &mask)| (mask != 0).then_some(y))
        .collect();
    if active.is_empty() {
        return None;
    }
    let mut groups = vec![];
    let mut start = 0;
    for n in 1..=active.len() {
        if n == active.len() || active[n] - active[n - 1] > 48.max(changes.len() / 5) {
            groups.push(&active[start..n]);
            start = n;
        }
    }
    let largest = groups.iter().map(|g| g.len()).max().unwrap();
    // A few isolated clock/hover rows in fixed chrome do not enlarge the moving
    // body. Keep sparse label groups and all substantial changed regions intact.
    let mut retained = groups
        .into_iter()
        .filter(|g| g.len() > 8 || g.len() * 4 >= largest);
    let first = retained.next()?;
    let bottom = retained.next_back().unwrap_or(first).last()? + 1;
    Some((first[0], bottom))
}

fn band_bits(band: usize) -> u128 {
    ((1u128 << FINE_BAND_WIDTH) - 1) << (band * FINE_BAND_WIDTH)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Scores and bounds one signed candidate in the changing interval"
)]
fn candidate_score(
    a: &Profile,
    b: &Profile,
    filter: &MotionFilter,
    top: usize,
    bottom: usize,
    shift: usize,
    direction: i64,
    samples: usize,
    bound: Option<f64>,
) -> Option<Score> {
    let rows: Vec<_> = (top..bottom - shift)
        .filter_map(|y| {
            let (ay, by) = if direction > 0 {
                (y + shift, y)
            } else {
                (y, y + shift)
            };
            let valid = a.texture_valid(ay) & b.texture_valid(by);
            // A texture sample and both of its neighbours must be unoccluded.
            let valid = valid & ((valid << 1) | 1) & ((valid >> 1) | (1u128 << 127));
            let mask = (a.texture[ay] | b.texture[by]) & valid & filter.valid_at(ay, by);
            (mask != 0).then_some((ay, by, mask))
        })
        .collect();
    let min_span = ((bottom - top - shift) / 2).clamp(12, 24);
    if rows.len() < 8 || rows.last()?.0 - rows[0].0 < min_span {
        return None;
    }
    let count = rows.len().min(samples);
    let all_pixels: u64 = rows.iter().map(|r| u64::from(r.2.count_ones())).sum();
    if all_pixels < 16 {
        return None;
    }
    let mut all_band_pixels = [0u64; FINE_BANDS];
    for &(_, _, mask) in &rows {
        for (band, slot) in all_band_pixels.iter_mut().enumerate() {
            *slot += u64::from((mask & band_bits(band)).count_ones());
        }
    }
    let active_all_bands = all_band_pixels.iter().filter(|&&px| px >= 8).count();
    let can_trim_band = active_all_bands >= 4;

    let ceiling = bound.map(|limit| limit * 3. * all_pixels as f64);
    let mut band_error = [0u64; FINE_BANDS];
    let mut band_pixels = [0u64; FINE_BANDS];
    let mut total_error = 0u64;
    let mut pixels = 0u64;
    for i in 0..count {
        let (ay, by, mut mask) = rows[i * rows.len() / count];
        pixels += u64::from(mask.count_ones());
        while mask != 0 {
            let x = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            let band = x / FINE_BAND_WIDTH;
            band_pixels[band] += 1;
            let mut px_err = 0u64;
            for c in 0..3 {
                px_err += u64::from(a.rows[ay][x * 3 + c].abs_diff(b.rows[by][x * 3 + c]));
            }
            band_error[band] += px_err;
            total_error += px_err;
        }
        if let Some(limit) = ceiling {
            let lower_error = if can_trim_band {
                total_error - band_error.iter().copied().max().unwrap_or(0)
            } else {
                total_error
            };
            if lower_error as f64 > limit {
                return None;
            }
        }
    }
    if pixels < 16 {
        return None;
    }
    // Exclude at most one anomalous horizontal band when >=4 bands have strong
    // texture and one band (e.g. an opposite-moving scrollbar thumb or hover
    // badge) has >16 RGB error and >4x the mean error of the remaining bands.
    let mut final_error = total_error;
    let mut final_pixels = pixels;
    let mut band_mask = u128::MAX;
    if can_trim_band
        && let Some((worst_band, &worst_err)) = band_error
            .iter()
            .enumerate()
            .filter(|&(b, _)| band_pixels[b] > 0)
            .max_by(|&(ba, &ea), &(bb, &eb)| {
                (ea as f64 / band_pixels[ba] as f64)
                    .total_cmp(&(eb as f64 / band_pixels[bb] as f64))
            })
    {
        let worst_px = band_pixels[worst_band];
        let rest_px = pixels - worst_px;
        let rest_err = total_error - worst_err;
        if rest_px >= 32 {
            let worst_mean = worst_err as f64 / (worst_px * 3) as f64;
            let rest_mean = rest_err as f64 / (rest_px * 3) as f64;
            if worst_mean > 16.0 && worst_mean > rest_mean * 4.0 {
                final_error = rest_err;
                final_pixels = rest_px;
                band_mask = !band_bits(worst_band);
            }
        }
    }
    let lower_error = if can_trim_band {
        total_error - band_error.iter().copied().max().unwrap_or(0)
    } else {
        total_error
    };
    Some(Score {
        error: final_error as f64 / (final_pixels * 3) as f64,
        raw_error: total_error as f64 / (pixels * 3) as f64,
        lower_bound: lower_error as f64 / (all_pixels * 3) as f64,
        band_mask,
    })
}

pub fn stitch(paths: &[String], destination: &str) -> Result<serde_json::Value> {
    if paths.is_empty() || paths.len() > 200 {
        return Err(t("Scrolling capture requires 1 to 200 frames").into());
    }
    let first = load(&paths[0])?;
    let mut previous = Profile::new(first.clone());
    let mut header = 0;
    let mut footer = 0;
    let mut segments = Vec::new();
    for path in &paths[1..] {
        let current = Profile::new(load(path)?);
        let result = find_match(&previous, &current)?;
        if result.shift == 0 {
            continue;
        }
        // Retain the smallest observed stable strips. A blank paragraph that
        // happens to match at the viewport edge must not eat later content.
        if segments.is_empty() {
            header = result.header;
            footer = result.footer;
        } else {
            header = header.min(result.header);
            footer = footer.min(result.footer);
        }
        segments.push((path, result.shift));
        previous = current;
    }
    let (width, original_height) = first.dimensions();
    let height = segments
        .iter()
        .try_fold(original_height, |h, (_, s)| h.checked_add(*s))
        .ok_or(t("Scrolling image dimensions overflow"))?;
    if u64::from(width) * u64::from(height) > MAX_PIXELS || height > 65535 {
        return Err(t(
            "The scrolling image exceeds 48 megapixels or the height limit; capture it in sections",
        )
        .into());
    }
    let mut output = RgbaImage::new(width, height);
    if segments.is_empty() {
        copy_rows(&first, 0, original_height, &mut output, 0);
    } else {
        let mut offsets = Vec::with_capacity(segments.len() + 1);
        offsets.push(0u32);
        for &(_, shift) in &segments {
            offsets.push(offsets.last().unwrap() + shift);
        }
        let cuts: Vec<_> = segments
            .iter()
            .enumerate()
            .map(|(idx, &(_, shift))| {
                offsets[idx] + (shift + header + original_height - footer) / 2
            })
            .collect();
        copy_rows(&first, 0, cuts[0], &mut output, 0);
        for idx in 1..segments.len() {
            let frame = load(segments[idx - 1].0)?;
            let start = cuts[idx - 1];
            let end = cuts[idx];
            copy_rows(
                &frame,
                start - offsets[idx],
                end - start,
                &mut output,
                start,
            );
        }
        let last_idx = segments.len();
        let last_start = *cuts.last().unwrap();
        copy_rows(
            &previous.image,
            last_start - offsets[last_idx],
            height - last_start,
            &mut output,
            last_start,
        );
    }
    save(&output, destination)?;
    Ok(serde_json::json!({"width": width, "height": height, "frames": segments.len()+1}))
}

fn copy_rows(source: &RgbaImage, y: u32, count: u32, output: &mut RgbaImage, target_y: u32) {
    let stride = source.width() as usize * 4;
    output.as_mut()[target_y as usize * stride..(target_y + count) as usize * stride]
        .copy_from_slice(&source.as_raw()[y as usize * stride..(y + count) as usize * stride]);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn viewport(offset: u32, header: u32, footer: u32) -> RgbaImage {
        RgbaImage::from_fn(128, 200, |x, y| {
            if y < header || y >= 200 - footer {
                return image::Rgba([40, 50, 60, 255]);
            }
            let v = (x.wrapping_mul(1973) ^ (y + offset).wrapping_mul(9277)).wrapping_mul(26699);
            image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
        })
    }
    #[test]
    fn matches_with_fixed_header_footer() {
        let result = find_match(
            &Profile::new(viewport(0, 20, 15)),
            &Profile::new(viewport(63, 20, 15)),
        )
        .unwrap();
        assert_eq!((result.shift, result.header, result.footer), (63, 20, 15));
    }
    #[test]
    fn low_contrast_and_edge_text_are_not_false_duplicates() {
        for edge in [false, true] {
            let frame = |offset: u32| {
                RgbaImage::from_fn(1200, 320, |x, y| {
                    if y < 100 || (edge && !(60..85).contains(&x) && !(1120..1145).contains(&x)) {
                        return image::Rgba([128, 128, 128, 255]);
                    }
                    let v = (x.wrapping_mul(1973) ^ (y + offset).wrapping_mul(9277))
                        .wrapping_mul(26699);
                    if edge {
                        image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
                    } else {
                        image::Rgba([
                            128 + (v & 7) as u8,
                            128 + ((v >> 3) & 7) as u8,
                            128 + ((v >> 6) & 7) as u8,
                            255,
                        ])
                    }
                })
            };
            assert_eq!(
                find_match(&Profile::new(frame(0)), &Profile::new(frame(60)))
                    .unwrap()
                    .shift,
                60
            );
        }
    }
    #[test]
    fn coarse_matching_samples_sparse_changed_rows_instead_of_whitespace() {
        let frame = |offset: u32| {
            RgbaImage::from_fn(1200, 470, |x, y| {
                if y < 249 {
                    return image::Rgba([60, 40, 75, 255]);
                }
                let row = y - 249 + offset;
                if (60..85).contains(&x) && (30..34).contains(&(row % 60)) {
                    let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                    image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
                } else {
                    image::Rgba([255, 255, 255, 255])
                }
            })
        };
        let mut doc = crate::scrolling::Document::new(crate::scrolling::Axis::Vertical, 40);
        for offset in (0..=24).map(|n| n * 60) {
            doc.push(frame(offset)).unwrap();
        }
        let output = doc.image().unwrap();
        assert_eq!(output.dimensions(), (1200, 1910));
        for y in 249u32..1910 {
            let row = y - 249;
            let x = 70u32;
            let expected = if (30..34).contains(&(row % 60)) {
                let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
            } else {
                image::Rgba([255, 255, 255, 255])
            };
            assert_eq!(*output.get_pixel(x, y), expected, "row {y}");
        }
    }
    #[test]
    fn fully_covered_frames_are_not_verified_as_duplicates() {
        let image = viewport(0, 0, 0);
        let profile = Profile::masked(
            image,
            &[crate::scrolling::ExcludedRect {
                x: 0,
                y: 0,
                width: 128,
                height: 200,
            }],
        );
        assert!(find_match(&profile, &profile).is_err());
    }
    #[test]
    fn duplicate_frame_is_not_appended() {
        let p = Profile::new(viewport(0, 0, 0));
        assert_eq!(find_match(&p, &p).unwrap().shift, 0);
    }
    #[test]
    fn rejects_wrong_direction_and_nonoverlap() {
        let p = Profile::new(viewport(80, 0, 0));
        assert!(find_match(&p, &Profile::new(viewport(0, 0, 0))).is_err());
        assert!(find_match(&p, &Profile::new(viewport(500, 0, 0))).is_err());
    }
    #[test]
    fn thin_marks_and_unique_horizontal_color_bands_are_not_duplicates() {
        for thin in [false, true] {
            let frame = |offset: u32| {
                RgbaImage::from_fn(128, 320, |x, y| {
                    if !(20..300).contains(&y) || (thin && x != 64) {
                        return image::Rgba([128, 128, 128, 255]);
                    }
                    let v = (y + offset).wrapping_mul(9277).wrapping_mul(26699);
                    image::Rgba([(v >> 16) as u8, 128, 128, 255])
                })
            };
            let a = Profile::new(frame(0));
            let b = Profile::new(frame(60));
            assert_eq!(find_match(&a, &b).unwrap().shift, 60, "thin={thin}");
            let (matched, direction) = displacement(&b, &a, None).unwrap();
            assert_eq!((matched.shift, direction), (60, -1));
        }
    }
    #[test]
    fn above_limit_alias_can_still_make_a_legal_winner_ambiguous() {
        let frame = |offset: u32, noise: bool| {
            RgbaImage::from_fn(128, 240, |x, y| {
                let row = y + offset;
                let v = (x.wrapping_mul(1973) ^ (row % 120).wrapping_mul(9277)).wrapping_mul(26699);
                image::Rgba([
                    20 + ((v >> 16) % 80) as u8
                        + (row / 40) as u8
                        + if noise && row.is_multiple_of(2) {
                            41
                        } else {
                            0
                        },
                    20 + ((v >> 8) % 80) as u8,
                    20 + (v % 80) as u8,
                    255,
                ])
            })
        };
        let a = Profile::new(frame(0, false));
        let b = Profile::new(frame(60, true));
        assert_eq!(find_match(&a, &b).unwrap().shift, 60);
        // +60 scores 6.8333, -60 scores 7.8333: the latter cannot win but
        // remains within the legal winner's 15% confidence interval.
        assert!(displacement(&a, &b, None).is_err());
    }
    #[test]
    fn isolated_clock_changes_cannot_lower_the_fixed_title_boundary() {
        let frame = |offset: u32| {
            RgbaImage::from_fn(128, 320, |x, y| {
                if y < 100 {
                    return image::Rgba([40, 50, 60, 255]);
                }
                let v =
                    (x.wrapping_mul(1973) ^ (y + offset).wrapping_mul(9277)).wrapping_mul(26699);
                image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
            })
        };
        let first = frame(0);
        let mut second = frame(120);
        for y in 10..18 {
            second.put_pixel(64, y, *first.get_pixel(64, y + 120));
        }
        let result = find_match(&Profile::new(first), &Profile::new(second)).unwrap();
        assert_eq!((result.shift, result.header), (120, 100));
    }
    #[test]
    fn truly_periodic_texture_is_ambiguous_in_both_directions() {
        let frame = |offset: u32| {
            RgbaImage::from_fn(192, 600, |x, y| {
                let row = (y + offset) % 120;
                let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
            })
        };
        let a = Profile::new(frame(0));
        let b = Profile::new(frame(40));
        assert!(displacement(&a, &b, None).is_err());
        assert!(displacement(&b, &a, None).is_err());
    }
    #[test]
    fn rejects_resized_viewport() {
        let p = Profile::new(viewport(0, 0, 0));
        assert!(find_match(&p, &Profile::new(RgbaImage::new(129, 200))).is_err());
    }

    #[test]
    fn stationary_sidebars_and_split_panes_do_not_poison_displacement() {
        let frame = |offset: u32, split_both_sides: bool| {
            RgbaImage::from_fn(320, 260, |x, y| {
                if y < 24 {
                    return image::Rgba([35, 38, 48, 255]);
                }
                let in_sidebar = if split_both_sides {
                    !(96..240).contains(&x)
                } else {
                    x < 128
                };
                if in_sidebar {
                    let s = (x.wrapping_mul(3571) ^ y.wrapping_mul(6151)).wrapping_mul(19457);
                    return image::Rgba([(s >> 16) as u8, (s >> 8) as u8, s as u8, 255]);
                }
                let row = y - 24 + offset;
                let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
            })
        };
        for split in [false, true] {
            let a = Profile::new(frame(0, split));
            let b = Profile::new(frame(72, split));
            assert_eq!(find_match(&a, &b).unwrap().shift, 72);
            assert_eq!(displacement(&a, &b, None).unwrap().0.shift, 72);
            let (rev, dir) = displacement(&b, &a, None).unwrap();
            assert_eq!((rev.shift, rev.header, rev.footer, dir), (72, 24, 0, -1));
        }
    }

    #[test]
    fn floating_interior_buttons_watermarks_and_sticky_banners_are_ignored() {
        let frame = |offset: u32| {
            RgbaImage::from_fn(256, 320, |x, y| {
                // Interior sticky banner with scrolling content both above and below it.
                if (96..124).contains(&y) && (24..232).contains(&x) {
                    let b = (x.wrapping_mul(4099) ^ y.wrapping_mul(8191)).wrapping_mul(13107);
                    return image::Rgba([40 + (b % 180) as u8, 60, 120, 255]);
                }
                // Floating action button in the bottom-right quadrant.
                if (220..268).contains(&y) && (188..236).contains(&x) {
                    let icon =
                        (234..254).contains(&y) && (202..222).contains(&x) && (x + y) % 3 == 0;
                    return image::Rgba(if icon {
                        [250, 250, 255, 255]
                    } else {
                        [88, 101, 242, 255]
                    });
                }
                // Center watermark strokes.
                if (160..192).contains(&y) && (96..160).contains(&x) && (x / 4 + y / 4) % 4 == 0 {
                    return image::Rgba([190, 195, 205, 255]);
                }
                let row = y + offset;
                if row % 24 < 14 && (16..240).contains(&x) {
                    let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                    image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
                } else {
                    image::Rgba([248, 249, 252, 255])
                }
            })
        };
        let mut doc = crate::scrolling::Document::new(crate::scrolling::Axis::Vertical, 30);
        for offset in [0, 64, 128, 192, 128, 256] {
            doc.push(frame(offset)).unwrap();
        }
        assert_eq!(doc.image().unwrap().dimensions(), (256, 576));
    }

    #[test]
    fn opposite_moving_scrollbar_thumb_and_subpixel_jitter_preserve_match() {
        let frame = |offset: u32, thumb_y: u32, jitter: bool| {
            RgbaImage::from_fn(256, 240, |x, y| {
                // Right-edge scrollbar track + downward-moving high-contrast thumb.
                if x >= 246 {
                    return if (thumb_y..thumb_y + 44).contains(&y) {
                        image::Rgba([20, 20, 25, 255])
                    } else {
                        image::Rgba([235, 235, 240, 255])
                    };
                }
                let row = y + offset;
                let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                let delta = if jitter && (x + row).is_multiple_of(3) {
                    4u8
                } else {
                    0
                };
                image::Rgba([
                    ((v >> 16) as u8).saturating_add(delta),
                    ((v >> 8) as u8).saturating_sub(delta / 2),
                    (v as u8).saturating_add(delta / 2),
                    255,
                ])
            })
        };
        let a = Profile::new(frame(0, 20, false));
        let b = Profile::new(frame(70, 95, true));
        let (matched, dir) = displacement(&a, &b, None).unwrap();
        assert_eq!((matched.shift, dir), (70, 1));
        assert!(displacement(&a, &Profile::new(frame(320, 160, true)), None).is_err());
    }

    #[test]
    fn small_selection_height_matches_proportional_steps() {
        for height in [56u32, 72] {
            let frame = |offset: u32| {
                RgbaImage::from_fn(180, height, |x, y| {
                    let row = y + offset;
                    if row % 16 < 10 && (10..170).contains(&x) {
                        let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                        image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
                    } else {
                        image::Rgba([255, 255, 255, 255])
                    }
                })
            };
            let step = height / 3;
            let a = Profile::new(frame(0));
            let b = Profile::new(frame(step));
            let (fwd, dir) = displacement(&a, &b, None).unwrap();
            assert_eq!((fwd.shift, dir), (step, 1));
            assert_eq!(displacement(&b, &a, None).unwrap().1, -1);
        }
    }

    #[test]
    fn cli_stitch_uses_interior_seams_so_floating_bars_do_not_repeat() {
        let dir = std::env::temp_dir().join(format!("clivuu-cli-stitch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pill = [220u8, 45, 165, 255];
        let badge = [35u8, 225, 190, 255];
        let render = |offset: u32| {
            RgbaImage::from_fn(256, 240, |x, y| {
                if (6..20).contains(&y) && (224..248).contains(&x) {
                    return image::Rgba(badge);
                }
                // Floating bottom composer bar with scrolling body rows still visible below it.
                if (192..216).contains(&y) && (40..216).contains(&x) {
                    return image::Rgba(pill);
                }
                let row = y + offset;
                let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
            })
        };
        let mut paths = Vec::new();
        for (i, off) in [0u32, 60, 120, 180].into_iter().enumerate() {
            let p = dir.join(format!("f{i}.png"));
            save(&render(off), p.to_str().unwrap()).unwrap();
            paths.push(p.to_string_lossy().into_owned());
        }
        let out_path = dir.join("stitched.png");
        let meta = stitch(&paths, out_path.to_str().unwrap()).unwrap();
        assert_eq!(meta["height"], 420);
        let stitched = load(out_path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        for interior_y in [200u32, 260, 320] {
            assert_ne!(stitched.get_pixel(128, interior_y).0, pill);
        }
        assert_eq!(stitched.get_pixel(128, 380).0, pill);
        assert_eq!(stitched.get_pixel(236, 12).0, badge);
        for interior_y in [72u32, 132, 192] {
            assert_ne!(stitched.get_pixel(236, interior_y).0, badge);
        }
    }

    #[test]
    fn stationary_frame_with_fading_scrollbar_or_caret_blink_returns_zero_shift() {
        let render = |scrollbar: bool, caret: bool| {
            RgbaImage::from_fn(256, 180, |x, y| {
                if scrollbar && (246..252).contains(&x) && (16..164).contains(&y) {
                    return image::Rgba([140, 150, 165, 255]);
                }
                if caret && (96..99).contains(&x) && (64..84).contains(&y) {
                    return image::Rgba([250, 250, 255, 255]);
                }
                let v = (x.wrapping_mul(1973) ^ y.wrapping_mul(9277)).wrapping_mul(26699);
                image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
            })
        };
        let with_chrome = Profile::new(render(true, true));
        let faded = Profile::new(render(false, false));
        assert_eq!(find_match(&with_chrome, &faded).unwrap().shift, 0);
        assert_eq!(displacement(&with_chrome, &faded, None).unwrap().0.shift, 0);

        // Repeating paragraphs (`period = 48`) at `shift == 0` with a fading
        // right-edge scrollbar must also resolve to `shift = 0` rather than
        // failing with `Ambiguous` between `+48` and `-48`.
        let repeating = |scrollbar: bool| {
            RgbaImage::from_fn(256, 96, |x, y| {
                if scrollbar && (248..254).contains(&x) && (8..88).contains(&y) {
                    return image::Rgba([150, 155, 165, 255]);
                }
                let local = y % 48;
                if (10..34).contains(&local) && (16..220).contains(&x) {
                    let v = (x.wrapping_mul(1973) ^ local.wrapping_mul(9277)).wrapping_mul(26699);
                    return image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]);
                }
                image::Rgba([255, 255, 255, 255])
            })
        };
        assert_eq!(
            displacement(
                &Profile::new(repeating(true)),
                &Profile::new(repeating(false)),
                None
            )
            .unwrap()
            .0
            .shift,
            0
        );

        // A narrow interior scrolling column that overshot completely over a fixed
        // background must still fail rather than being mistaken for shift = 0.
        let narrow_col = |offset: u32| {
            RgbaImage::from_fn(256, 180, |x, y| {
                if (96..136).contains(&x) && (12..168).contains(&y) {
                    let row = y + offset;
                    let v = (x.wrapping_mul(3571) ^ row.wrapping_mul(8191)).wrapping_mul(19937);
                    return image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]);
                }
                let bg = (x.wrapping_mul(1973) ^ y.wrapping_mul(9277)).wrapping_mul(26699);
                image::Rgba([(bg >> 16) as u8, (bg >> 8) as u8, bg as u8, 255])
            })
        };
        let col_a = Profile::new(narrow_col(0));
        let col_overshot = Profile::new(narrow_col(500));
        assert!(displacement(&col_a, &col_overshot, None).is_err());
    }
}
