//! A GUI-independent scrolling document. Pairwise matching tracks both scroll
//! directions; immutable compressed frames retain export pixels under a bound.
use crate::{
    i18n::t,
    image::{self, RgbaImage},
    image_ops::{self, Result},
    stitch::{Profile, displacement},
};
mod composition;
mod cropping;
mod fixed;
mod occlusion;
mod repair;
mod retention;
pub use occlusion::ExcludedRect;
pub use repair::Repair;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    Vertical,
    Horizontal,
}
#[derive(Debug)]
pub enum LimitReached {
    Frames,
    Dimensions,
}
impl std::fmt::Display for LimitReached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&t(match self {
            Self::Frames=>"Frame limit reached; finish the capture",
            Self::Dimensions=>"The scrolling image exceeds 48 megapixels or the height limit; capture it in sections",
        }))
    }
}
impl std::error::Error for LimitReached {}
const RAW_BUDGET_BYTES: usize = 48 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: usize = 128 * 1024 * 1024;
pub(super) enum FramePixels {
    Raw(Arc<RgbaImage>),
    Png(Vec<u8>),
}
impl FramePixels {
    pub(super) fn byte_len(&self) -> usize {
        match self {
            Self::Raw(image) => image.as_raw().len(),
            Self::Png(bytes) => bytes.len(),
        }
    }
    pub(super) fn image(&self) -> Result<Arc<RgbaImage>> {
        match self {
            Self::Raw(image) => Ok(Arc::clone(image)),
            Self::Png(bytes) => Ok(Arc::new(image_ops::decode(bytes)?)),
        }
    }
    pub(super) fn encode_fast(image: &RgbaImage) -> Result<Vec<u8>> {
        use ::image::{
            ExtendedColorType, ImageEncoder,
            codecs::png::{CompressionType, FilterType, PngEncoder},
        };
        let mut bytes = Vec::new();
        let encoder =
            PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::Sub);
        encoder.write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgba8,
        )?;
        if bytes.len() as u64 > image_ops::MAX_BYTES {
            return Err(t("The image file exceeds 64 MiB").into());
        }
        Ok(bytes)
    }
    fn compress(&mut self) -> Result<()> {
        if let Self::Raw(image) = self {
            let bytes = Self::encode_fast(image)?;
            *self = Self::Png(bytes);
        }
        Ok(())
    }
}
pub(super) fn make_preview_source(image: &RgbaImage) -> Arc<RgbaImage> {
    let (src_w, src_h) = image.dimensions();
    let scale = (180. / f64::from(src_w))
        .min(360. / f64::from(src_h))
        .min(1.);
    let pw = (f64::from(src_w) * scale).round().max(1.) as u32;
    let ph = (f64::from(src_h) * scale).round().max(1.) as u32;
    if (pw, ph) == (src_w, src_h) {
        return Arc::new(image.clone());
    }
    let src_raw = image.as_raw();
    let src_w_us = src_w as usize;
    let src_h_us = src_h as usize;
    let pw_us = pw as usize;
    let ph_us = ph as usize;
    let mut out = vec![0u8; pw_us * ph_us * 4];
    for py in 0..ph_us {
        let y0 = py * src_h_us / ph_us;
        let y1 = ((py + 1) * src_h_us / ph_us).max(y0 + 1).min(src_h_us);
        for px in 0..pw_us {
            let x0 = px * src_w_us / pw_us;
            let x1 = ((px + 1) * src_w_us / pw_us).max(x0 + 1).min(src_w_us);
            let mut sum = [0u32; 4];
            for y in y0..y1 {
                let row = y * src_w_us * 4;
                for x in x0..x1 {
                    let idx = row + x * 4;
                    sum[0] += u32::from(src_raw[idx]);
                    sum[1] += u32::from(src_raw[idx + 1]);
                    sum[2] += u32::from(src_raw[idx + 2]);
                    sum[3] += u32::from(src_raw[idx + 3]);
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as u32;
            let half = count / 2;
            let dst = (py * pw_us + px) * 4;
            out[dst] = ((sum[0] + half) / count) as u8;
            out[dst + 1] = ((sum[1] + half) / count) as u8;
            out[dst + 2] = ((sum[2] + half) / count) as u8;
            out[dst + 3] = ((sum[3] + half) / count) as u8;
        }
    }
    Arc::new(RgbaImage::from_raw(pw, ph, out).expect("box preview dimensions valid"))
}
struct Frame {
    offset: i64,
    body_range: Option<[i64; 2]>,
    body_verified: bool,
    viewport: Option<crate::annotation::Bounds>,
    pixels: FramePixels,
    preview: Arc<RgbaImage>,
    scaled_preview: Mutex<Option<(u32, u32, Arc<RgbaImage>)>>,
    excluded: Vec<ExcludedRect>,
}
fn covers_view(frame: &Frame, offset: i64, height: u32) -> bool {
    frame.offset == offset
        && frame
            .body_range
            .is_none_or(|[low, high]| low <= offset && high >= offset + i64::from(height))
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub width: u32,
    pub height: u32,
    pub frames: usize,
    pub position: f64,
    pub viewport: f64,
    pub changed: bool,
}
pub struct Document {
    axis: Axis,
    frames: Vec<Frame>,
    previous: Option<Arc<Profile>>,
    position: i64,
    dimensions: Option<(u32, u32)>,
    minimum: i64,
    maximum: i64,
    fixed: fixed::Fixed,
    previous_viewport: Option<crate::annotation::Bounds>,
    bytes: usize,
    limit: usize,
    previous_excluded: Vec<ExcludedRect>,
    auto_crop: bool,
    auto_crop_trend: i8,
    auto_crop_anchor: i64,
}
impl Document {
    pub fn new(axis: Axis, limit: usize) -> Self {
        Self {
            axis,
            frames: vec![],
            previous: None,
            position: 0,
            dimensions: None,
            minimum: 0,
            maximum: 0,
            fixed: fixed::Fixed::default(),
            previous_viewport: None,
            bytes: 0,
            limit: limit.clamp(1, 200),
            previous_excluded: vec![],
            auto_crop: false,
            auto_crop_trend: 0,
            auto_crop_anchor: 0,
        }
    }
    pub fn axis(&self) -> Axis {
        self.axis
    }
    pub fn cursor_offset(&self) -> i64 {
        self.position
    }
    pub fn frames(&self) -> usize {
        self.frames.len()
    }
    fn extent(&self) -> u32 {
        self.dimensions
            .map_or(0, |(_, h)| h + (self.maximum - self.minimum) as u32)
    }
    fn boundary_frame<'a>(&'a self, extra: Option<&'a Frame>, offset: i64) -> Option<&'a Frame> {
        let height = self.dimensions?.1;
        self.frames
            .iter()
            .chain(extra)
            .find(|f| covers_view(f, offset, height))
            .or_else(|| self.frames.iter().chain(extra).find(|f| f.offset == offset))
    }
    pub fn progress(&self, changed: bool) -> Progress {
        let (w, h) = self.dimensions.unwrap_or((0, 0));
        let extent = self.extent();
        let length = extent;
        let (width, height) = match self.axis {
            Axis::Vertical => (w, length),
            Axis::Horizontal => (length, w),
        };
        Progress {
            width,
            height,
            frames: self.frames.len(),
            position: if length == 0 {
                0.
            } else {
                ((self.position - self.minimum) as f64 / length as f64).clamp(0., 1.)
            },
            viewport: if length == 0 {
                0.
            } else {
                let start = self.position.max(self.minimum);
                let end = (self.position + h as i64).min(self.maximum + h as i64);
                (end - start).max(0) as f64 / length as f64
            },
            changed,
        }
    }
    pub fn push(&mut self, image: RgbaImage) -> Result<Progress> {
        self.push_masked(image, vec![])
    }
    pub fn push_masked(
        &mut self,
        image: RgbaImage,
        excluded: Vec<ExcludedRect>,
    ) -> Result<Progress> {
        self.push_sample(Arc::new(image), excluded, None)
    }
    /// Desktop geometry is metadata only; matching and composition use pixels.
    pub fn push_masked_at(
        &mut self,
        image: RgbaImage,
        excluded: Vec<ExcludedRect>,
        viewport: crate::annotation::Bounds,
    ) -> Result<Progress> {
        self.push_sample(Arc::new(image), excluded, Some(viewport))
    }
    pub fn push_masked_arc_at(
        &mut self,
        image: Arc<RgbaImage>,
        excluded: Vec<ExcludedRect>,
        viewport: crate::annotation::Bounds,
    ) -> Result<Progress> {
        self.push_sample(image, excluded, Some(viewport))
    }
    pub(super) fn normalize_sample(
        &self,
        image: Arc<RgbaImage>,
        excluded: Vec<ExcludedRect>,
    ) -> Result<(Arc<RgbaImage>, Vec<ExcludedRect>)> {
        if image.width() == 0
            || image.height() == 0
            || u64::from(image.width()) * u64::from(image.height()) > image_ops::MAX_PIXELS
        {
            return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
        }
        let excluded: Vec<_> = excluded
            .into_iter()
            .filter_map(|r| r.clipped(image.width(), image.height()))
            .collect();
        let src_h = image.height();
        let image = if excluded.is_empty() && self.axis == Axis::Vertical {
            image
        } else {
            let mut owned = Arc::try_unwrap(image).unwrap_or_else(|arc| (*arc).clone());
            occlusion::clear(&mut owned, &excluded);
            match self.axis {
                Axis::Vertical => Arc::new(owned),
                Axis::Horizontal => Arc::new(image::imageops::rotate90(&owned)),
            }
        };
        let excluded: Vec<_> = excluded
            .into_iter()
            .map(|r| match self.axis {
                Axis::Vertical => r,
                Axis::Horizontal => r.rotated(src_h),
            })
            .collect();
        // Profile allocates row signatures; reject overlong axes before that
        // allocation, including narrow images within the total-pixel budget.
        if image.height() > 65535 {
            return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
        }
        if image.height() < 48 {
            return Err(t("The scrolling region must be at least 48 pixels high").into());
        }
        if self
            .dimensions
            .is_some_and(|size| size != image.dimensions())
        {
            return Err(t("Capture dimensions changed; select the region again").into());
        }
        Ok((image, excluded))
    }
    pub(super) fn compact_raw_frames(&mut self, needed: usize) -> Result<()> {
        if self.bytes + needed <= RAW_BUDGET_BYTES {
            return Ok(());
        }
        for frame in &mut self.frames {
            if self.bytes + needed <= RAW_BUDGET_BYTES {
                break;
            }
            if matches!(frame.pixels, FramePixels::Raw(_)) {
                let before = frame.pixels.byte_len();
                frame.pixels.compress()?;
                self.bytes = self.bytes - before + frame.pixels.byte_len();
            }
        }
        Ok(())
    }
    pub(super) fn frame_pixels(
        &mut self,
        image: Arc<RgbaImage>,
        replacing: usize,
    ) -> Result<FramePixels> {
        let mut pixels = FramePixels::Raw(image);
        let raw_len = pixels.byte_len();
        self.compact_raw_frames(raw_len.saturating_sub(replacing))?;
        if self.bytes.saturating_sub(replacing) + raw_len > MAX_DOCUMENT_BYTES
            || raw_len > RAW_BUDGET_BYTES
        {
            pixels.compress()?;
        }
        Ok(pixels)
    }
    pub(super) fn merge_frame_pixels(
        &mut self,
        index: usize,
        incoming: &RgbaImage,
        excluded: &[ExcludedRect],
    ) -> Result<()> {
        let old = &self.frames[index];
        let old_len = old.pixels.byte_len();
        let mut merged = (*old.pixels.image()?).clone();
        occlusion::merge(&mut merged, incoming, excluded);
        let preview = make_preview_source(&merged);
        let pixels = self.frame_pixels(Arc::new(merged), old_len)?;
        let new_len = pixels.byte_len();
        if self.bytes - old_len + new_len > MAX_DOCUMENT_BYTES {
            return Err(LimitReached::Frames.into());
        }
        self.bytes = self.bytes - old_len + new_len;
        let frame = &mut self.frames[index];
        frame.pixels = pixels;
        frame.preview = preview;
        *frame.scaled_preview.lock().unwrap() = None;
        Ok(())
    }
    fn push_sample(
        &mut self,
        image: Arc<RgbaImage>,
        excluded: Vec<ExcludedRect>,
        viewport: Option<crate::annotation::Bounds>,
    ) -> Result<Progress> {
        let (image, excluded) = self.normalize_sample(image, excluded)?;
        let previous_excluded = excluded.clone();
        let profile = Arc::new(Profile::masked_arc(image, &excluded));
        let mut position = self.position;
        let mut strips = None;
        if let Some(previous) = self.previous.as_ref() {
            // Once observed, fixed chrome stays outside displacement evidence.
            // A clock tick or activation change must not invalidate the body.
            let h = profile.image.height();
            let old = self.fixed.edges(self.previous_viewport, self.axis, h);
            let new = self.fixed.edges(viewport, self.axis, h);
            let body = (old.0.max(new.0) as usize, (h - old.1.max(new.1)) as usize);
            match displacement(previous, &profile, Some(body)) {
                Ok(matched) => {
                    position += matched.1 * i64::from(matched.0.shift);
                    if self.previous_viewport == viewport && matched.0.shift != 0 {
                        strips = Some((matched.0.header, matched.0.footer));
                    }
                }
                Err(primary_err) => {
                    if primary_err.is::<crate::stitch::Ambiguous>() {
                        return Err(primary_err);
                    }
                    let mut candidates: Vec<_> = self
                        .frames
                        .iter()
                        .filter(|f| {
                            f.body_range.is_none()
                                && !(self.previous_viewport == viewport
                                    && f.offset == self.position
                                    && f.excluded == self.previous_excluded)
                        })
                        .collect();
                    candidates.sort_by_key(|f| {
                        (
                            f.viewport != viewport,
                            (f.offset - self.position).unsigned_abs(),
                        )
                    });
                    let mut recovered = None;
                    for donor_frame in candidates.into_iter().take(6) {
                        let donor =
                            Profile::masked_arc(donor_frame.pixels.image()?, &donor_frame.excluded);
                        let donor_old = self.fixed.edges(donor_frame.viewport, self.axis, h);
                        let donor_body = (
                            donor_old.0.max(new.0) as usize,
                            (h - donor_old.1.max(new.1)) as usize,
                        );
                        match displacement(&donor, &profile, Some(donor_body)) {
                            Ok(matched) => {
                                recovered = Some((donor_frame, matched));
                                break;
                            }
                            Err(err) if err.is::<crate::stitch::Ambiguous>() => {
                                return Err(err);
                            }
                            Err(_) => {}
                        }
                    }
                    let Some((donor_frame, matched)) = recovered else {
                        return Err(primary_err);
                    };
                    position = donor_frame.offset + matched.1 * i64::from(matched.0.shift);
                    if donor_frame.viewport == viewport && matched.0.shift != 0 {
                        strips = Some((matched.0.header, matched.0.footer));
                    }
                }
            }
        }
        let mut fixed = self.fixed.clone();
        if let Some((header, footer)) = strips {
            fixed.observe(header, footer, viewport, self.axis, profile.image.height());
        }
        let body_verified = strips.is_some()
            || self
                .frames
                .iter()
                .any(|f| f.viewport == viewport && f.body_verified);
        // Revisiting already captured content updates navigation without duplicating
        // frames or consuming the frame/memory budget.
        if let Some(index) = self
            .frames
            .iter()
            .position(|frame| covers_view(frame, position, profile.image.height()))
        {
            let remaining = occlusion::intersection(&self.frames[index].excluded, &excluded);
            let changed = remaining != self.frames[index].excluded;
            if changed {
                self.merge_frame_pixels(index, &profile.image, &excluded)?;
                self.frames[index].offset = position;
                self.frames[index].body_range = None;
                self.frames[index].body_verified = body_verified;
                self.frames[index].viewport = viewport;
                self.frames[index].excluded = remaining;
            }
            let changed = changed || self.fixed != fixed;
            if body_verified {
                for f in self.frames.iter_mut().filter(|f| f.viewport == viewport) {
                    f.body_verified = true;
                }
            }
            self.fixed = fixed;
            self.previous_viewport = viewport;
            self.previous_excluded = previous_excluded;
            self.previous = Some(profile);
            self.position = position;
            let auto_cropped = self.apply_auto_crop()?;
            retention::prune_clipped(self);
            return Ok(self.progress(changed || auto_cropped));
        }
        let minimum = self.minimum.min(position);
        let maximum = self.maximum.max(position);
        let (w, h) = profile.image.dimensions();
        let extent = i64::from(h) + maximum - minimum;
        if extent > 65535 || extent * u64::from(w) as i64 > image_ops::MAX_PIXELS as i64 {
            return Err(LimitReached::Dimensions.into());
        }
        let pixels = self.frame_pixels(Arc::clone(&profile.image), 0)?;
        let mut frame = Frame {
            offset: position,
            body_range: None,
            body_verified,
            viewport,
            pixels,
            preview: Arc::new(RgbaImage::new(1, 1)),
            scaled_preview: Mutex::new(None),
            excluded,
        };
        let remove = retention::removals(self, &frame, &fixed)?;
        let old_count = self.frames.len();
        let retained = !remove.contains(&old_count);
        for &index in remove.iter().rev().filter(|&&i| i < old_count) {
            self.bytes -= self.frames[index].pixels.byte_len();
            self.frames.remove(index);
        }
        if body_verified {
            for f in self.frames.iter_mut().filter(|f| f.viewport == viewport) {
                f.body_verified = true;
            }
        }
        if retained {
            frame.preview = make_preview_source(&profile.image);
            self.bytes += frame.pixels.byte_len();
            self.frames.push(frame);
        }
        self.dimensions = Some((w, h));
        self.minimum = minimum;
        self.maximum = maximum;
        self.position = position;
        self.fixed = fixed;
        self.previous_viewport = viewport;
        self.previous_excluded = previous_excluded;
        self.previous = Some(profile);
        let _ = self.apply_auto_crop()?;
        retention::prune_clipped(self);
        Ok(self.progress(true))
    }
    pub fn image(&self) -> Result<RgbaImage> {
        self.compose(false)
    }
    pub fn preview(&self) -> Result<RgbaImage> {
        self.compose(true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn view(offset: u32) -> RgbaImage {
        RgbaImage::from_fn(128, 200, |x, y| {
            if !(20..185).contains(&y) {
                return image::Rgba([40, 50, 60, 255]);
            }
            let v = (x.wrapping_mul(1973) ^ (y + offset).wrapping_mul(9277)).wrapping_mul(26699);
            image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
        })
    }
    fn obscured(offset: u32, masks: &[ExcludedRect]) -> RgbaImage {
        let mut image = view(offset);
        for r in masks {
            for y in r.y..r.y + r.height {
                for x in r.x..r.x + r.width {
                    image.put_pixel(x, y, image::Rgba([255, 0, 255, 255]));
                }
            }
        }
        image
    }
    // Repeated icons carry little information; the thin, unique label rows
    // must disambiguate them. No window-system metadata is available to core.
    fn directory_grid(offset: u32) -> RgbaImage {
        directory_grid_height(offset, 900)
    }
    fn directory_grid_height(offset: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(320, height, |x, y| {
            if y < 180 {
                return image::Rgba([38, 40, 50, 255]);
            }
            let row = y - 180 + offset;
            let (tile, local) = (row / 276, row % 276);
            let column = x % 80;
            if (25..125).contains(&local) && (10..70).contains(&column) {
                return image::Rgba([230, 195, 120, 255]);
            }
            if (150..164).contains(&local) && (8..72).contains(&column) {
                let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277) ^ tile.wrapping_mul(7919))
                    .wrapping_mul(26699);
                return image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]);
            }
            image::Rgba([29, 29, 32, 255])
        })
    }
    #[test]
    fn repeated_directory_icons_keep_unique_labels_navigation_and_coverage() {
        for step in [40, 100] {
            let mut doc = Document::new(Axis::Vertical, 40);
            let mask = ExcludedRect {
                x: 120,
                y: 892,
                width: 200,
                height: 8,
            };
            // Include reversals and a return to the last viewport before finishing.
            let mut offsets: Vec<_> = (0..=800 / step).map(|n| n * step).collect();
            offsets.extend([800 - step, 800 - 2 * step, 800 - step, 800]);
            for offset in offsets {
                doc.push_masked(
                    directory_grid(offset),
                    vec![
                        mask,
                        ExcludedRect {
                            x: 12,
                            y: 8,
                            width: 120,
                            height: 24,
                        },
                    ],
                )
                .unwrap_or_else(|e| panic!("step={step}, offset={offset}: {e}"));
                assert_eq!(doc.position, offset as i64);
            }
            // Covered terminal pixels cannot be silently exported as white.
            assert!(doc.image().is_err());
            doc.push(directory_grid(800)).unwrap();
            let output = doc.image().unwrap();
            assert_eq!(output.dimensions(), (320, 1700));
            let expected = directory_grid_height(0, 1700);
            for (index, (actual, wanted)) in output.pixels().zip(expected.pixels()).enumerate() {
                assert_eq!(
                    actual,
                    wanted,
                    "step={step}, point=({},{})",
                    index % 320,
                    index / 320
                );
            }
        }
    }
    #[test]
    fn redundant_samples_do_not_exhaust_the_keyframe_budget_or_lose_pixels() {
        for axis in [Axis::Vertical, Axis::Horizontal] {
            for reverse in [false, true] {
                let orient = |image: RgbaImage| match axis {
                    Axis::Vertical => image,
                    Axis::Horizontal => image::imageops::rotate270(&image),
                };
                let mut bounded = Document::new(axis, 12);
                let mut reference = Document::new(axis, 200);
                for step in 0..=48 {
                    let offset = if reverse { 48 - step } else { step } * 12;
                    let image = orient(view(offset));
                    bounded.push(image.clone()).unwrap();
                    reference.push(image).unwrap();
                    assert!(bounded.frames() <= 12);
                }
                assert_eq!(bounded.image().unwrap(), reference.image().unwrap());
                assert_eq!(bounded.extent(), 776);
                assert!(bounded.frames() < 49);
            }
        }
    }
    #[test]
    fn repairing_an_offset_from_new_geometry_does_not_inherit_old_body_evidence() {
        let bounds = crate::annotation::Bounds {
            x: 0.,
            y: 100.,
            width: 128.,
            height: 200.,
        };
        let mut doc = Document::new(Axis::Vertical, 4);
        doc.push_masked_at(
            view(0),
            vec![ExcludedRect {
                x: 20,
                y: 50,
                width: 20,
                height: 20,
            }],
            bounds,
        )
        .unwrap();
        doc.push_masked_at(view(40), vec![], bounds).unwrap();
        assert!(doc.frames[0].body_verified);
        let moved = crate::annotation::Bounds { y: 60., ..bounds };
        doc.push_masked_at(view(0), vec![], moved).unwrap();
        assert_eq!(doc.frames[0].viewport, Some(moved));
        assert!(!doc.frames[0].body_verified);
        assert!(doc.frames[1].body_verified);
    }
    #[test]
    fn region_roundtrip_repairs_the_old_view_without_changing_navigation() {
        let original = crate::annotation::Bounds {
            x: 12.,
            y: 60.,
            width: 128.,
            height: 200.,
        };
        let moved = crate::annotation::Bounds {
            y: 100.,
            ..original
        };
        let mask = vec![ExcludedRect {
            x: 20,
            y: 170,
            width: 80,
            height: 30,
        }];
        for axis in [Axis::Vertical, Axis::Horizontal] {
            let rotate = |im: RgbaImage| {
                if axis == Axis::Vertical {
                    im
                } else {
                    image::imageops::rotate270(&im)
                }
            };
            let pixels = |offset: u32| {
                RgbaImage::from_fn(128, 200, |x, y| {
                    let v = (x.wrapping_mul(1973) ^ (y + offset).wrapping_mul(9277))
                        .wrapping_mul(26699);
                    image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
                })
            };
            let masks = if axis == Axis::Vertical {
                mask.clone()
            } else {
                vec![ExcludedRect {
                    x: 170,
                    y: 28,
                    width: 30,
                    height: 80,
                }]
            };
            let mut doc = Document::new(axis, 2);
            doc.push_masked_at(rotate(pixels(0)), vec![], original)
                .unwrap();
            doc.push_masked_at(rotate(pixels(40)), masks, moved)
                .unwrap();
            doc.push_masked_at(rotate(pixels(0)), vec![], original)
                .unwrap();
            assert!(doc.image().is_err());
            let before = doc.progress(false);
            let request = doc.repairs()[0];
            assert_eq!(request.viewport, moved);
            // New page content cannot silently replace a historical hole.
            assert!(doc.repair(request, rotate(pixels(80))).is_err());
            assert!(doc.image().is_err());
            doc.repair(request, rotate(pixels(40))).unwrap();
            assert_eq!(doc.progress(false).position, before.position);
            assert_eq!(doc.frames(), 2);
            assert!(doc.repairs().is_empty());
            let expected = RgbaImage::from_fn(128, 240, |x, y| {
                let v = (x.wrapping_mul(1973) ^ y.wrapping_mul(9277)).wrapping_mul(26699);
                image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
            });
            assert_eq!(doc.image().unwrap(), rotate(expected));
        }
    }
    #[test]
    fn overlay_pixels_are_never_exported_and_final_clean_still_repairs_coverage() {
        let masks = vec![ExcludedRect {
            x: 30,
            y: 100,
            width: 70,
            height: 40,
        }];
        let mut doc = Document::new(Axis::Vertical, 20);
        doc.push(view(0)).unwrap();
        doc.push_masked(obscured(40, &masks), masks.clone())
            .unwrap();
        doc.push_masked(obscured(80, &masks), masks).unwrap();
        doc.push(view(80)).unwrap();
        let output = doc.image().unwrap();
        assert_eq!(output.dimensions(), (128, 280));
        for y in 20..265 {
            let expected = view(y - 20);
            for x in 0..128 {
                assert_eq!(output.get_pixel(x, y), expected.get_pixel(x, 20));
            }
        }
    }
    // A fixed title is pixel content, not a window-system/OCR concept. Exercise
    // patterned labels rather than only uniform strips, across multiple matches.
    fn chat_view(offset: u32, title: u32) -> RgbaImage {
        RgbaImage::from_fn(192, 320, |x, y| {
            if y < title {
                let ink = (8..title.saturating_sub(8)).contains(&y)
                    && (25..150).contains(&x)
                    && (x / 5 + y / 3) % 3 == 0;
                return image::Rgba(if ink {
                    [235, 235, 240, 255]
                } else {
                    [38, 40, 50, 255]
                });
            }
            if y >= 296 {
                return image::Rgba([20 + (x % 15) as u8, 30, 45, 255]);
            }
            let row = y - title + offset;
            let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
            image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
        })
    }
    #[test]
    fn fixed_chrome_is_reused_across_many_viewports_in_both_directions() {
        for reverse in [false, true] {
            let mut doc = Document::new(Axis::Vertical, 100);
            let offsets: Vec<_> = (0..=24)
                .map(|n| if reverse { 1920 - n * 80 } else { n * 80 })
                .collect();
            let mask = vec![ExcludedRect {
                x: 25,
                y: 8,
                width: 100,
                height: 24,
            }];
            for (n, &offset) in offsets.iter().enumerate() {
                doc.push_masked(
                    chat_view(offset, 140),
                    if n == 0 { vec![] } else { mask.clone() },
                )
                .unwrap();
            }
            let output = doc.image().unwrap();
            assert_eq!(output.dimensions(), (192, 2240));
            for y in 140u32..2216 {
                let row = y - 140;
                for x in 0u32..192 {
                    let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                    assert_eq!(
                        *output.get_pixel(x, y),
                        image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]),
                        "reverse={reverse}, ({x},{y})"
                    );
                }
            }
            assert_eq!(
                output.get_pixel(50, 12),
                chat_view(0, 140).get_pixel(50, 12)
            );
        }
    }
    #[test]
    fn final_body_repair_ignores_live_fixed_chrome_but_rejects_another_page() {
        let bounds = crate::annotation::Bounds {
            x: 0.,
            y: 0.,
            width: 192.,
            height: 320.,
        };
        let mask = vec![ExcludedRect {
            x: 25,
            y: 260,
            width: 100,
            height: 35,
        }];
        let mut doc = Document::new(Axis::Vertical, 10);
        doc.push_masked_at(chat_view(0, 140), vec![], bounds)
            .unwrap();
        doc.push_masked_at(chat_view(80, 140), mask, bounds)
            .unwrap();
        assert!(doc.image().is_err());
        let request = doc.repairs()[0];
        assert!(doc.repair(request, chat_view(160, 140)).is_err());
        let mut latest = chat_view(80, 140);
        for y in 0..4 {
            for x in 25..125 {
                latest.put_pixel(x, y, image::Rgba([220, 20, 50, 255]));
            }
        }
        doc.repair(request, latest).unwrap();
        assert!(doc.image().is_ok());
    }
    #[test]
    fn small_live_title_changes_do_not_turn_the_title_into_scrolling_body() {
        for reverse in [false, true] {
            let mut doc = Document::new(Axis::Vertical, 100);
            for n in 0..=24 {
                let offset = if reverse { 1920 - n * 80 } else { n * 80 };
                let mut frame = chat_view(offset, 140);
                // A clock/hover state changes a few title rows, as can happen in
                // a GNOME shared desktop. The title itself stays physically fixed.
                for y in 0..4 {
                    for x in 25..125 {
                        frame.put_pixel(
                            x,
                            y,
                            image::Rgba([if n % 2 == 0 { 20 } else { 220 }, 40, 50, 255]),
                        );
                    }
                }
                doc.push(frame).unwrap();
            }
            let output = doc.image().unwrap();
            assert_eq!(output.dimensions(), (192, 2240));
            for y in 140u32..2216 {
                let row = y - 140;
                let x = 60u32;
                let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                assert_eq!(
                    *output.get_pixel(x, y),
                    image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]),
                    "reverse={reverse}, row={y}"
                );
            }
        }
    }
    #[test]
    fn provisional_blank_margins_can_shrink_as_new_body_texture_is_revealed() {
        let frame = |offset: u32| {
            RgbaImage::from_fn(128, 200, |x, y| {
                if y < 20 || !(100..160).contains(&(y - 20 + offset)) {
                    return image::Rgba([128, 128, 128, 255]);
                }
                let row = y - 20 + offset;
                let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
            })
        };
        let mut document = Document::new(Axis::Vertical, 10);
        for offset in [0, 40, 80, 120] {
            document.push(frame(offset)).unwrap();
            assert_eq!(document.position, offset as i64);
        }
        let output = document.image().unwrap();
        assert_eq!(output.dimensions(), (128, 320));
        let first = frame(0);
        for y in 120..180 {
            for x in 0..128 {
                assert_eq!(output.get_pixel(x, y), first.get_pixel(x, y));
            }
        }
    }
    #[test]
    fn learned_fixed_chrome_is_excluded_when_a_clock_changes_near_the_body() {
        let frame = |offset: u32, tick: bool| {
            let mut image = directory_grid(offset);
            for y in 120..136 {
                for x in 140..164 {
                    image.put_pixel(
                        x,
                        y,
                        image::Rgba(if tick {
                            [200, 210, 220, 255]
                        } else {
                            [40, 50, 60, 255]
                        }),
                    );
                }
            }
            image
        };
        let mut document = Document::new(Axis::Vertical, 40);
        document.push(frame(0, false)).unwrap();
        document.push(frame(40, false)).unwrap();
        // The clock is <height/5 away from the moving body, so a cold pair's
        // difference ROI alone cannot separate it using a large empty gap.
        document.push(frame(80, true)).unwrap();
        assert_eq!(document.position, 80);
        document.push(frame(80, false)).unwrap();
        assert_eq!(document.position, 80);
        document.push(frame(40, true)).unwrap();
        assert_eq!(document.position, 40);
    }
    #[test]
    fn fixed_patterned_chat_title_and_input_footer_are_retained_once() {
        for title in [12, 32, 64] {
            let mut doc = Document::new(Axis::Vertical, 20);
            for offset in [0, 45, 90, 135, 90, 180] {
                doc.push(chat_view(offset, title)).unwrap();
            }
            let output = doc.image().unwrap();
            assert_eq!(output.dimensions(), (192, 500));
            let start = chat_view(0, title);
            let end = chat_view(180, title);
            for y in 0..500 {
                for x in 0..192 {
                    let expected = if y < title {
                        *start.get_pixel(x, y)
                    } else if y >= 476 {
                        *end.get_pixel(x, y - 180)
                    } else {
                        let row = y - title;
                        let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                        image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
                    };
                    assert_eq!(
                        *output.get_pixel(x, y),
                        expected,
                        "title={title}, point=({x},{y})"
                    );
                }
            }
        }
    }
    #[test]
    fn moving_the_region_does_not_reclassify_the_fixed_title_as_body() {
        let base = crate::annotation::Bounds {
            x: 0.,
            y: 0.,
            width: 192.,
            height: 320.,
        };
        let title = chat_view(0, 140);
        let source = |scroll: u32, physical: u32| {
            RgbaImage::from_fn(192, 320, |x, y| {
                let world = y + physical;
                if world < 140 {
                    return *title.get_pixel(x, world);
                }
                let row = world - 140 + scroll;
                let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
            })
        };
        let mut doc = Document::new(Axis::Vertical, 20);
        for (scroll, physical) in [(0, 0), (40, 0), (80, 0), (80, 28), (80, 0), (120, 0)] {
            doc.push_masked_at(
                source(scroll, physical),
                vec![],
                crate::annotation::Bounds {
                    y: physical as f64,
                    ..base
                },
            )
            .unwrap();
        }
        let output = doc.image().unwrap();
        assert_eq!(output.dimensions(), (192, 440));
        for y in 140..440 {
            assert_eq!(
                output.get_pixel(60, y),
                source(y - 140, 0).get_pixel(60, 140),
                "row {y}"
            );
        }
    }
    #[test]
    fn tall_fixed_title_is_not_limited_to_a_third_of_the_viewport() {
        let mut doc = Document::new(Axis::Vertical, 20);
        for offset in [0, 45, 90, 135, 90, 180] {
            doc.push(chat_view(offset, 140)).unwrap();
        }
        let output = doc.image().unwrap();
        assert_eq!(output.dimensions(), (192, 500));
        let expected = chat_view(180, 140);
        // The last viewport's header must not overwrite earlier body content.
        for y in 140u32..476 {
            let row = y - 140;
            for x in 0u32..192 {
                let v = (x.wrapping_mul(1973) ^ row.wrapping_mul(9277)).wrapping_mul(26699);
                assert_eq!(
                    *output.get_pixel(x, y),
                    image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]),
                    "({x},{y})"
                );
            }
        }
        assert_eq!(output.get_pixel(0, 499), expected.get_pixel(0, 319));
    }
    #[test]
    fn fixed_strips_share_the_same_model_for_horizontal_capture() {
        let mut doc = Document::new(Axis::Horizontal, 20);
        for offset in [0, 45, 90, 135] {
            doc.push(image::imageops::rotate270(&chat_view(offset, 32)))
                .unwrap();
        }
        let mut vertical = Document::new(Axis::Vertical, 20);
        for offset in [0, 45, 90, 135] {
            vertical.push(chat_view(offset, 32)).unwrap();
        }
        assert_eq!(
            doc.image().unwrap(),
            image::imageops::rotate270(&vertical.image().unwrap())
        );
    }
    #[test]
    fn toolbar_overlap_is_recovered_over_more_than_two_viewports() {
        let masks = vec![ExcludedRect {
            x: 25,
            y: 150,
            width: 80,
            height: 50,
        }];
        let mut doc = Document::new(Axis::Vertical, 20);
        doc.push(view(0)).unwrap();
        for offset in [60, 120, 180, 240] {
            doc.push_masked(obscured(offset, &masks), masks.clone())
                .unwrap();
        }
        doc.push(view(240)).unwrap();
        let output = doc.image().unwrap();
        assert_eq!(output.dimensions(), (128, 440));
        for y in 20..425 {
            let expected = view(y - 20);
            for x in 0..128 {
                assert_eq!(output.get_pixel(x, y), expected.get_pixel(x, 20));
            }
        }
    }
    #[test]
    fn covered_content_must_be_revisited_and_same_offset_repair_is_not_a_new_frame() {
        let mask = vec![ExcludedRect {
            x: 0,
            y: 170,
            width: 128,
            height: 30,
        }];
        let mut doc = Document::new(Axis::Vertical, 2);
        doc.push_masked(obscured(0, &mask), mask).unwrap();
        assert!(doc.image().is_err());
        let repaired = doc.push(view(0)).unwrap();
        assert!(repaired.changed);
        assert_eq!(repaired.frames, 1);
        assert_eq!(doc.image().unwrap(), view(0));
        // A later masked revisit must not replace clean pixels or consume budget.
        let mask = vec![ExcludedRect {
            x: 0,
            y: 0,
            width: 128,
            height: 45,
        }];
        doc.push_masked(obscured(0, &mask), mask).unwrap();
        assert_eq!(doc.image().unwrap(), view(0));
    }
    #[test]
    fn horizontal_masks_rotate_with_source_and_partial_revisits_merge_coverage() {
        let a = ExcludedRect {
            x: 0,
            y: 70,
            width: 10,
            height: 60,
        };
        let b = ExcludedRect {
            x: 118,
            y: 70,
            width: 10,
            height: 60,
        };
        let mut doc = Document::new(Axis::Horizontal, 20);
        for mask in [a, b] {
            doc.push_masked(
                image::imageops::rotate270(&obscured(0, &[mask])),
                vec![ExcludedRect {
                    x: mask.y,
                    y: 128 - mask.x - mask.width,
                    width: mask.height,
                    height: mask.width,
                }],
            )
            .unwrap();
        }
        assert_eq!(doc.image().unwrap(), image::imageops::rotate270(&view(0)));
        assert_eq!(doc.frames(), 1);
        // Transparent source pixels remain valid; alpha is never a coverage flag.
        let mut transparent = view(40);
        transparent.get_pixel_mut(1, 1).0[3] = 0;
        let mut doc = Document::new(Axis::Vertical, 20);
        doc.push(transparent.clone()).unwrap();
        assert_eq!(doc.image().unwrap(), transparent);
    }
    #[test]
    fn tracks_forward_backward_and_revisited_content_without_duplicate_frames() {
        let mut doc = Document::new(Axis::Vertical, 20);
        doc.push(view(80)).unwrap();
        doc.push(view(143)).unwrap();
        doc.push(view(175)).unwrap();
        doc.push(view(111)).unwrap();
        let progress = doc.push(view(80)).unwrap();
        assert!(!progress.changed);
        assert_eq!(progress.height, 295);
        assert_eq!(progress.frames, 4);
        let image = doc.image().unwrap();
        assert_eq!(image.dimensions(), (128, 295));
        for y in 20..280 {
            assert_eq!(image.get_pixel(50, y), view(80 + y - 20).get_pixel(50, 20));
        }
        assert_eq!(image.get_pixel(50, 294), &image::Rgba([40, 50, 60, 255]));
    }
    #[test]
    fn extends_before_first_frame_and_supports_horizontal_output_and_trimming() {
        let mut doc = Document::new(Axis::Horizontal, 10);
        for offset in [100, 160, 60] {
            doc.push(image::imageops::rotate270(&view(offset))).unwrap();
        }
        assert_eq!(doc.image().unwrap().dimensions(), (300, 128));
        doc.crop_to_view(false).unwrap();
        assert_eq!(doc.image().unwrap().dimensions(), (200, 128));
        assert!(doc.preview().unwrap().width() <= 360);
    }
    #[test]
    fn rejects_overlong_axis_before_allocating_row_signatures() {
        let mut vertical = Document::new(Axis::Vertical, 10);
        assert!(vertical.push(RgbaImage::new(1, 65536)).is_err());
        assert_eq!(vertical.frames(), 0);
        let mut horizontal = Document::new(Axis::Horizontal, 10);
        assert!(horizontal.push(RgbaImage::new(65536, 1)).is_err());
        assert_eq!(horizontal.frames(), 0);
    }
    #[test]
    fn directional_crop_commits_pixels_and_continues_in_both_axes() {
        for axis in [Axis::Vertical, Axis::Horizontal] {
            let orient = |image: RgbaImage| {
                if axis == Axis::Horizontal {
                    image::imageops::rotate270(&image)
                } else {
                    image
                }
            };
            let mut doc = Document::new(axis, 10);
            for offset in [0, 60, 100, 60] {
                doc.push(orient(view(offset))).unwrap();
            }
            assert!(doc.crop_to_view(true).unwrap());
            assert_eq!(doc.progress(false).position, 0.);
            assert!(doc.crop_to_view(false).unwrap());
            assert_eq!(doc.extent(), 200);
            assert_eq!(doc.image().unwrap(), orient(view(60)));
            doc.push(orient(view(90))).unwrap();
            assert_eq!(doc.extent(), 230);
            doc.push(orient(view(30))).unwrap();
            assert_eq!(doc.extent(), 260);
            assert_eq!(doc.minimum, 30);
            assert_eq!(doc.maximum, 90);
        }
    }
    #[test]
    fn crop_at_either_empty_boundary_is_noop_and_preserves_document() {
        for axis in [Axis::Vertical, Axis::Horizontal] {
            let orient = |image: RgbaImage| {
                if axis == Axis::Horizontal {
                    image::imageops::rotate270(&image)
                } else {
                    image
                }
            };
            // Multi-viewport (>2x height) capture: any accidental reset or hole
            // immediately shrinks extent or fails `image()`.
            let mut doc = Document::new(axis, 20);
            for offset in [0, 100, 200, 300, 400] {
                doc.push(orient(view(offset))).unwrap();
            }
            let full = doc.image().unwrap();
            assert_eq!(doc.extent(), 600);
            // At the bottom/right boundary (no content beyond current view):
            // TrimEnd is a strict no-op and preserves position/viewport/pixels.
            assert!(!doc.crop_to_view(false).unwrap());
            assert_eq!((doc.minimum, doc.maximum, doc.position), (0, 400, 400));
            let p_end = doc.progress(false);
            assert!((p_end.position + p_end.viewport - 1.).abs() < 1e-6);
            assert_eq!(doc.image().unwrap(), full);

            // Scroll back to the top/left boundary (no content before current view):
            // TrimStart is a strict no-op (never resets to the opposite end or
            // leaves `position < minimum`).
            for offset in [300, 200, 100, 0] {
                doc.push(orient(view(offset))).unwrap();
            }
            assert!(!doc.crop_to_view(true).unwrap());
            assert_eq!((doc.minimum, doc.maximum, doc.position), (0, 400, 0));
            let p_start = doc.progress(false);
            assert_eq!(p_start.position, 0.);
            assert!((p_start.viewport - 200. / 600.).abs() < 1e-6);
            assert_eq!(doc.image().unwrap(), full);

            // Continuing to scroll after the no-op trim produces zero holes.
            doc.push(orient(view(30))).unwrap();
            assert_eq!((doc.minimum, doc.maximum, doc.position), (0, 400, 30));
            assert_eq!(doc.image().unwrap(), full);

            // Upward/leftward-first capture behaves identically at both boundaries.
            let mut upward = Document::new(axis, 20);
            for offset in [400, 300, 200, 100, 0] {
                upward.push(orient(view(offset))).unwrap();
            }
            let upward_full = upward.image().unwrap();
            assert!(!upward.crop_to_view(true).unwrap());
            assert_eq!(upward.image().unwrap(), upward_full);
            for offset in [100, 200, 300, 400] {
                upward.push(orient(view(offset))).unwrap();
            }
            assert!(!upward.crop_to_view(false).unwrap());
            assert_eq!(upward.image().unwrap(), upward_full);
        }
    }
    #[test]
    fn trim_prunes_redundant_outside_frames_and_supports_scroll_back_and_retrim() {
        // Trim below, verify discarded tail frames are reclaimed, scroll back
        // into trimmed territory and re-trim under tight keyframe retention.
        let mut doc = Document::new(Axis::Vertical, 10);
        for offset in [0, 100, 200, 300, 400] {
            doc.push(view(offset)).unwrap();
        }
        for offset in [300, 200] {
            doc.push(view(offset)).unwrap();
        }
        assert!(doc.crop_to_view(false).unwrap());
        assert_eq!((doc.minimum, doc.maximum, doc.position), (0, 200, 200));
        assert!(doc.frames.iter().all(|f| f.offset <= 200));
        // Unclipped interior frames keep `body_range == None` for overshoot recovery.
        assert!(doc.frames.iter().all(|f| f.body_range.is_none()));
        assert!(!doc.crop_to_view(false).unwrap());
        // Scroll back into previously trimmed offsets and re-trim at 300.
        for offset in [300, 400, 300] {
            doc.push(view(offset)).unwrap();
        }
        assert!(doc.crop_to_view(false).unwrap());
        assert_eq!((doc.minimum, doc.maximum, doc.position), (0, 300, 300));
        let out = doc.image().unwrap();
        assert_eq!(out.dimensions(), (128, 500));
        for y in 20..485 {
            assert_eq!(out.get_pixel(50, y), view(y - 20).get_pixel(50, 20));
        }

        // Symmetric check for TrimStart: trim above at 200, scroll back up to 0, re-trim at 100.
        let mut doc = Document::new(Axis::Vertical, 10);
        for offset in [0, 100, 200, 300, 400] {
            doc.push(view(offset)).unwrap();
        }
        for offset in [300, 200] {
            doc.push(view(offset)).unwrap();
        }
        assert!(doc.crop_to_view(true).unwrap());
        assert_eq!((doc.minimum, doc.maximum, doc.position), (200, 400, 200));
        assert!(doc.frames.iter().all(|f| f.offset >= 200));
        assert!(!doc.crop_to_view(true).unwrap());
        for offset in [100, 0, 100] {
            doc.push(view(offset)).unwrap();
        }
        assert!(doc.crop_to_view(true).unwrap());
        assert_eq!((doc.minimum, doc.maximum, doc.position), (100, 400, 100));
        let out = doc.image().unwrap();
        assert_eq!(out.dimensions(), (128, 500));
        for y in 20..485 {
            assert_eq!(out.get_pixel(50, y), view(100 + y - 20).get_pixel(50, 20));
        }
    }
    #[test]
    fn cropped_donors_are_reclaimed_at_capacity_and_boundary_revisits_are_reused() {
        let mut doc = Document::new(Axis::Vertical, 2);
        doc.push(view(0)).unwrap();
        doc.push(view(40)).unwrap();
        doc.crop_to_view(true).unwrap();
        doc.push(view(40)).unwrap();
        doc.push(view(80)).unwrap();
        assert_eq!(doc.extent(), 240);
        assert!(doc.image().is_ok());
        assert!(doc.frames() <= 2);
    }
    #[test]
    fn crop_restores_a_redundant_current_view_within_the_existing_budget() {
        let mut doc = Document::new(Axis::Vertical, 2);
        for offset in [0, 80, 40] {
            doc.push(view(offset)).unwrap();
        }
        assert_eq!(doc.frames(), 2);
        assert!(doc.crop_to_view(true).unwrap());
        assert_eq!(doc.extent(), 240);
        assert_eq!(doc.frames(), 2);
        assert!(doc.image().is_ok());
        doc.push(view(40)).unwrap();
        assert_eq!(doc.extent(), 240);
    }
    #[test]
    fn crop_preserves_masked_boundary_donors_and_rolls_back_a_failed_budget_proof() {
        let mut doc = Document::new(Axis::Vertical, 2);
        doc.push(view(0)).unwrap();
        doc.push(view(80)).unwrap();
        let mask = ExcludedRect {
            x: 0,
            y: 0,
            width: 128,
            height: 20,
        };
        doc.push_masked(obscured(40, &[mask]), vec![mask]).unwrap();
        let bytes = doc.bytes;
        assert!(doc.crop_to_view(true).unwrap_err().is::<LimitReached>());
        assert_eq!((doc.minimum, doc.maximum, doc.position), (0, 80, 40));
        assert_eq!(doc.bytes, bytes);
        assert!(doc.frames.iter().all(|f| f.body_range.is_none()));
        doc.limit = 3;
        doc.crop_to_view(true).unwrap();
        assert!(doc.frames.iter().any(|f| f.offset == 0));
        doc.fixed.observe(0, 0, None, Axis::Vertical, 200);
        assert!(doc.image().is_ok());
    }
    #[test]
    fn directional_crop_keeps_cross_boundary_pixels_that_repaired_hud_occlusion() {
        let mask = ExcludedRect {
            x: 0,
            y: 0,
            width: 128,
            height: 20,
        };
        let mut doc = Document::new(Axis::Vertical, 10);
        doc.push(view(0)).unwrap();
        for offset in [40, 80, 40] {
            doc.push_masked(obscured(offset, &[mask]), vec![mask])
                .unwrap();
        }
        assert!(doc.image().is_ok());
        doc.crop_to_view(true).unwrap();
        assert!(doc.image().is_ok());
        assert_eq!(doc.extent(), 240);
        doc.push(view(80)).unwrap();
        assert!(doc.image().is_ok());
        doc.push(view(20)).unwrap();
        assert_eq!(doc.minimum, 20);
        assert_eq!(doc.extent(), 260);
        assert!(doc.image().is_ok());
    }
    #[test]
    fn rejected_frame_does_not_change_document_or_consume_history() {
        let mut doc = Document::new(Axis::Vertical, 2);
        doc.push(view(0)).unwrap();
        let before = doc.image().unwrap();
        assert!(doc.push(RgbaImage::new(13, 100)).is_err());
        assert_eq!(doc.image().unwrap(), before);
        doc.push(view(40)).unwrap();
        doc.push(view(80)).unwrap();
        assert_eq!(doc.frames(), 2);
        let before = doc.image().unwrap();
        assert!(doc.push(view(240)).is_err());
        assert_eq!(doc.image().unwrap(), before);
        assert_eq!(doc.progress(false).height, 280);
    }
    #[test]
    fn recovers_navigation_from_retained_keyframes_after_overshoot() {
        let mut doc = Document::new(Axis::Vertical, 20);
        for offset in [0, 50, 100, 150] {
            doc.push(view(offset)).unwrap();
        }
        // Overshooting past the overlap window fails cleanly without mutating state.
        assert!(doc.push(view(360)).is_err());
        assert_eq!(doc.cursor_offset(), 150);
        // Scrolling back near an earlier retained keyframe (|30 - 150| = 120 > max_shift 110)
        // re-establishes exact document position from retained history.
        let recovered = doc.push(view(30)).unwrap();
        assert_eq!(doc.cursor_offset(), 30);
        assert_eq!(recovered.height, 350);
        // Subsequent forward scrolling continues seamlessly from the recovered offset.
        doc.push(view(90)).unwrap();
        doc.push(view(150)).unwrap();
        doc.push(view(200)).unwrap();
        let output = doc.image().unwrap();
        assert_eq!(output.dimensions(), (128, 400));
        for y in 20..385 {
            assert_eq!(output.get_pixel(50, y), view(y - 20).get_pixel(50, 20));
        }
    }

    #[test]
    fn auto_crop_trims_on_reverse_scroll_and_reextends_without_losing_start() {
        let mut doc = Document::new(Axis::Vertical, 20);
        doc.set_auto_crop(true);
        assert!(doc.auto_crop());
        for offset in [0, 60, 130, 210] {
            doc.push(view(offset)).unwrap();
        }
        assert_eq!(doc.progress(false).height, 410);
        // Rolling back across both non-keyframe (170) and retained-keyframe (130)
        // offsets trims excess tail monotonically while keeping the leading start.
        let trimmed_mid = doc.push(view(170)).unwrap();
        assert_eq!(trimmed_mid.height, 370);
        let trimmed_keyframe = doc.push(view(130)).unwrap();
        assert_eq!(trimmed_keyframe.height, 330);
        assert!(trimmed_keyframe.changed);
        // Resuming forward scroll re-extends the tail instead of cutting the top.
        let extended = doc.push(view(180)).unwrap();
        assert_eq!(extended.height, 380);
        let output = doc.image().unwrap();
        assert_eq!(output.dimensions(), (128, 380));
        assert_eq!(output.get_pixel(50, 25), view(5).get_pixel(50, 20));
        assert_eq!(output.get_pixel(50, 355), view(335).get_pixel(50, 20));

        // Upward-first capture trims overshot head when scrolling back downward.
        let mut upward = Document::new(Axis::Vertical, 20);
        upward.set_auto_crop(true);
        for offset in [200, 130, 60, 0] {
            upward.push(view(offset)).unwrap();
        }
        assert_eq!(upward.progress(false).height, 400);
        let trimmed_head = upward.push(view(80)).unwrap();
        assert_eq!(trimmed_head.height, 320);
    }

    #[test]
    fn reanchor_adopts_modified_view_while_keeping_committed_long_image() {
        let mutated_view = |offset: u32| {
            RgbaImage::from_fn(128, 200, |x, y| {
                if !(20..185).contains(&y) {
                    return image::Rgba([40, 50, 60, 255]);
                }
                let v = (x.wrapping_mul(6151) ^ (y + offset).wrapping_mul(4099)).wrapping_mul(9973);
                image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
            })
        };
        let mut doc = Document::new(Axis::Vertical, 20);
        for offset in [0, 80, 160] {
            doc.push(view(offset)).unwrap();
        }
        assert_eq!(doc.progress(false).height, 360);
        // Expanding a collapsed block or switching views fails normal overlap matching.
        assert!(doc.push(mutated_view(0)).is_err());

        let hud_mask = ExcludedRect {
            x: 16,
            y: 160,
            width: 80,
            height: 24,
        };
        let mut masked_new = mutated_view(0);
        for y in hud_mask.y..hud_mask.y + hud_mask.height {
            for x in hud_mask.x..hud_mask.x + hud_mask.width {
                masked_new.put_pixel(x, y, image::Rgba([255, 0, 255, 255]));
            }
        }
        // Re-anchoring preserves the 360px already captured, fills HUD exclusions
        // from the prior unoccluded frame, and lets new mutated frames stitch cleanly.
        let anchored = doc
            .reanchor_masked_at(masked_new, vec![hud_mask], None)
            .unwrap();
        assert_eq!(anchored.height, 360);
        doc.push(mutated_view(70)).unwrap();
        let final_progress = doc.push(mutated_view(140)).unwrap();
        assert_eq!(final_progress.height, 500);
        let output = doc.image().unwrap();
        assert_eq!(output.dimensions(), (128, 500));
        assert_eq!(output.get_pixel(50, 30), view(10).get_pixel(50, 20));
        assert_eq!(
            output.get_pixel(50, 470),
            mutated_view(140).get_pixel(50, 170)
        );
    }

    #[test]
    fn clean_boundary_before_crop_prevents_holes_when_reversing_away() {
        let mask = ExcludedRect {
            x: 16,
            y: 190,
            width: 96,
            height: 10,
        };
        let mut doc = Document::new(Axis::Vertical, 10);
        doc.push_masked(obscured(0, &[mask]), vec![mask]).unwrap();
        doc.push_masked(obscured(80, &[mask]), vec![mask]).unwrap();
        assert!(doc.can_crop_to_view(true));
        assert!(!doc.can_crop_to_view(false));
        assert!(doc.current_view_occluded());

        // Refresh the boundary anchor cleanly before committing crop_to_view(true).
        doc.push_masked(view(80), vec![]).unwrap();
        assert!(!doc.current_view_occluded());
        assert!(doc.crop_to_view(true).unwrap());
        assert_eq!(doc.progress(false).height, 200);
        assert!(!doc.can_crop_to_view(true));
        assert!(!doc.can_crop_to_view(false));

        // Scrolling UP away from the cut bottom must not reveal any transparent hole
        // or break final export.
        doc.push_masked(obscured(20, &[mask]), vec![mask]).unwrap();
        doc.push_masked(obscured(0, &[mask]), vec![mask]).unwrap();
        let preview = doc.preview().unwrap();
        assert!(preview.pixels().all(|p| p[3] == 255));
        let output = doc.image().unwrap();
        assert_eq!(output.dimensions(), (128, 280));
        assert_eq!(output.get_pixel(50, 275), view(80).get_pixel(50, 195));

        // Symmetric TrimEnd at top boundary with interior occlusion before scrolling DOWN.
        let top_mask = ExcludedRect {
            x: 16,
            y: 0,
            width: 96,
            height: 12,
        };
        let mut up = Document::new(Axis::Vertical, 10);
        up.push_masked(obscured(100, &[top_mask]), vec![top_mask])
            .unwrap();
        up.push_masked(obscured(20, &[top_mask]), vec![top_mask])
            .unwrap();
        assert!(up.can_crop_to_view(false));
        up.push_masked(view(20), vec![]).unwrap();
        assert!(up.crop_to_view(false).unwrap());
        up.push_masked(obscured(80, &[top_mask]), vec![top_mask])
            .unwrap();
        let up_preview = up.preview().unwrap();
        assert!(up_preview.pixels().all(|p| p[3] == 255));
        assert!(up.image().is_ok());
    }

    #[test]
    fn repair_clears_all_matching_frames_and_prunes_redundant_clipped_donors() {
        let vp = crate::annotation::Bounds {
            x: 0.,
            y: 0.,
            width: 128.,
            height: 200.,
        };
        let bottom_mask = ExcludedRect {
            x: 20,
            y: 188,
            width: 80,
            height: 12,
        };
        let body_mask = ExcludedRect {
            x: 20,
            y: 90,
            width: 80,
            height: 20,
        };
        let mut doc = Document::new(Axis::Vertical, 10);
        doc.push_masked_at(obscured(0, &[bottom_mask]), vec![bottom_mask], vp)
            .unwrap();
        doc.push_masked_at(obscured(80, &[bottom_mask]), vec![bottom_mask], vp)
            .unwrap();
        doc.push_masked_at(obscured(160, &[bottom_mask]), vec![bottom_mask], vp)
            .unwrap();
        doc.push_masked_at(view(160), vec![], vp).unwrap();
        // Cut to [160, 160] while frame@80 remains a clipped boundary donor for 160..280.
        assert!(doc.crop_to_view(true).unwrap());
        // Scroll back to 80 with a different mask at the same viewport: creates a
        // second frame sharing (offset=80, viewport=vp) while clipped donor@80 still exists.
        doc.push_masked_at(obscured(80, &[body_mask]), vec![body_mask], vp)
            .unwrap();
        let repairs = doc.repairs();
        assert_eq!(repairs.len(), 1);
        assert_eq!(repairs[0].viewport, vp);
        doc.repair(repairs[0], view(80)).unwrap();
        assert!(doc.repairs().is_empty());
        assert!(doc.frames.iter().all(|f| f.body_range.is_none()));
        assert!(doc.image().is_ok());
    }

    #[test]
    fn crop_needs_clean_anchor_skips_redundant_refresh_when_interior_frames_cover_mask() {
        let footer_mask = ExcludedRect {
            x: 16,
            y: 184,
            width: 96,
            height: 16,
        };
        let mut footer_doc = Document::new(Axis::Vertical, 10);
        for offset in [0, 80, 160, 240] {
            footer_doc
                .push_masked(obscured(offset, &[footer_mask]), vec![footer_mask])
                .unwrap();
        }
        footer_doc
            .push_masked(obscured(160, &[footer_mask]), vec![footer_mask])
            .unwrap();
        footer_doc
            .push_masked(obscured(80, &[footer_mask]), vec![footer_mask])
            .unwrap();
        // Fixed footer rows 185..200 were occluded in every frame, so trimming still needs a clean refresh.
        assert!(footer_doc.crop_needs_clean_anchor(true));

        let body_mask = ExcludedRect {
            x: 16,
            y: 160,
            width: 96,
            height: 16,
        };
        let mut doc = Document::new(Axis::Vertical, 10);
        for offset in [0, 80, 160] {
            doc.push_masked(obscured(offset, &[body_mask]), vec![body_mask])
                .unwrap();
        }
        doc.push_masked(obscured(240, &[body_mask]), vec![body_mask])
            .unwrap();
        // At bottom boundary (pos == max), trimming above keeps only 240 whose body mask is uncovered.
        assert!(doc.crop_needs_clean_anchor(true));
        doc.push(view(240)).unwrap();
        // Scroll back to interior offset 80: frame 160 covers body_mask rows 240..256.
        doc.push_masked(obscured(160, &[body_mask]), vec![body_mask])
            .unwrap();
        doc.push_masked(obscured(80, &[body_mask]), vec![body_mask])
            .unwrap();
        assert!(doc.current_view_occluded());
        assert!(!doc.crop_needs_clean_anchor(true));
        assert!(doc.crop_to_view(true).unwrap());
        assert_eq!((doc.minimum, doc.maximum, doc.position), (80, 240, 80));
        assert!(doc.preview().unwrap().pixels().all(|px| px[3] == 255));
        assert!(doc.image().unwrap().pixels().all(|px| px[3] == 255));
    }

    #[test]
    fn hybrid_raw_and_compressed_frames_preserve_exact_pixels_and_preview_cache() {
        let mut doc = Document::new(Axis::Vertical, 20);
        for offset in [0, 60, 120] {
            doc.push(view(offset)).unwrap();
        }
        assert!(
            doc.frames
                .iter()
                .all(|f| matches!(f.pixels, FramePixels::Raw(_)))
        );
        let p_raw = doc.preview().unwrap();
        let img_raw = doc.image().unwrap();
        assert!(p_raw.pixels().all(|px| px[3] == 255));

        // Force Raw -> fast-PNG compaction on existing frames, then revisit an already-covered offset.
        doc.compact_raw_frames(RAW_BUDGET_BYTES).unwrap();
        assert!(
            doc.frames
                .iter()
                .all(|f| matches!(f.pixels, FramePixels::Png(_)))
        );
        let revisit = doc.push(view(60)).unwrap();
        assert!(!revisit.changed);
        let p_png = doc.preview().unwrap();
        let img_png = doc.image().unwrap();
        assert_eq!(img_raw, img_png);
        assert_eq!(p_raw, p_png);
    }
}
