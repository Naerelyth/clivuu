//! Region tiles retain the source image's sampling phase. Moving a mosaic mask
//! changes its coverage, never the pixel grid. Gaussian work includes a halo.
use super::RgbaImage;
use crate::annotation::Bounds;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Effect {
    Pixelate,
    Blur,
}
impl Effect {
    /// Unified strength range (min, max, default) on Clivuu's strength scale.
    /// Matches Snipaste's `3..=24` scale across both pixelation and blur.
    pub const STRENGTH_RANGE: [u8; 3] = [3, 24, 8];

    /// Minimum, maximum and default on Clivuu's strength scale. These are
    /// independent of the brush width and apply to both brush and region masks.
    pub const fn strength_range(self) -> [u8; 3] {
        let _ = self;
        Self::STRENGTH_RANGE
    }
    pub fn strength(self, value: f64) -> u8 {
        let [min, max, default] = self.strength_range();
        if value == 0. || value.is_nan() {
            default
        } else {
            value.round().clamp(f64::from(min), f64::from(max)) as u8
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileSpec {
    pub effect: Effect,
    pub level: u8,
    pub source: [u32; 2],
    /// Block size / blur radius in source pixels, independent of the UI level.
    pub size: u32,
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug)]
pub struct Tile {
    pub spec: TileSpec,
    pub pixels: Vec<u8>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Cancelled,
}
impl TileSpec {
    pub fn plan(source: [u32; 2], effect: Effect, level: u8, roi: Bounds) -> Option<Self> {
        Self::plan_scaled(source, effect, level, roi, 1.)
    }
    pub fn plan_scaled(
        source: [u32; 2],
        effect: Effect,
        level: u8,
        roi: Bounds,
        scale: f64,
    ) -> Option<Self> {
        if !scale.is_finite() || scale <= 0. {
            return None;
        }
        if source.contains(&0)
            || u64::from(source[0]) * u64::from(source[1]) > crate::image_ops::MAX_PIXELS
            || ![roi.x, roi.y, roi.width, roi.height]
                .iter()
                .all(|v| v.is_finite())
            || roi.width <= 0.
            || roi.height <= 0.
        {
            return None;
        }
        let level = effect.strength(f64::from(level));
        let size = (f64::from(level) * scale).round().max(1.) as u32;
        let (columns, rows, step_x, step_y) = grid(source, effect, size);
        let factor = if effect == Effect::Blur {
            (f64::from(size) / 3.).max(1.)
        } else {
            f64::from(size)
        };
        let halo = if effect == Effect::Blur {
            (f64::from(size) / factor).ceil() + 2.
        } else {
            2.
        };
        let x = roi.x.max(0.);
        let y = roi.y.max(0.);
        let right = (roi.x + roi.width).min(f64::from(source[0]));
        let bottom = (roi.y + roi.height).min(f64::from(source[1]));
        if right <= x || bottom <= y {
            return None;
        }
        let left = ((x / step_x).floor() - halo).max(0.) as u32;
        let top = ((y / step_y).floor() - halo).max(0.) as u32;
        let right = ((right / step_x).ceil() + halo).min(f64::from(columns)) as u32;
        let bottom = ((bottom / step_y).ceil() + halo).min(f64::from(rows)) as u32;
        Some(Self {
            effect,
            level,
            source,
            size,
            left,
            top,
            width: right - left,
            height: bottom - top,
        })
    }
    pub fn bounds(self) -> Bounds {
        let (sx, sy) = self.step();
        Bounds {
            x: f64::from(self.left) * sx,
            y: f64::from(self.top) * sy,
            width: f64::from(self.width) * sx,
            height: f64::from(self.height) * sy,
        }
    }
    pub(crate) fn step(self) -> (f64, f64) {
        let (_, _, sx, sy) = grid(self.source, self.effect, self.size);
        (sx, sy)
    }
    pub fn bytes(self) -> usize {
        self.width as usize * self.height as usize * 4
    }
    fn valid(self) -> bool {
        if self.source.contains(&0)
            || self.level < self.effect.strength_range()[0]
            || self.level > self.effect.strength_range()[1]
            || self.size == 0
            || self.width == 0
            || self.height == 0
            || u64::from(self.source[0]) * u64::from(self.source[1]) > crate::image_ops::MAX_PIXELS
        {
            return false;
        }
        let (columns, rows, _, _) = grid(self.source, self.effect, self.size);
        self.left
            .checked_add(self.width)
            .is_some_and(|v| v <= columns)
            && self.top.checked_add(self.height).is_some_and(|v| v <= rows)
    }
}
fn grid(source: [u32; 2], effect: Effect, size: u32) -> (u32, u32, f64, f64) {
    let factor = if effect == Effect::Blur {
        (f64::from(size) / 3.).max(1.)
    } else {
        f64::from(size)
    };
    let columns = (f64::from(source[0]) / factor).ceil().max(1.) as u32;
    let rows = (f64::from(source[1]) / factor).ceil().max(1.) as u32;
    if effect == Effect::Blur {
        (
            columns,
            rows,
            f64::from(source[0]) / f64::from(columns),
            f64::from(source[1]) / f64::from(rows),
        )
    } else {
        (columns, rows, factor, factor)
    }
}
/// Premultiplied native pixels, prepared once per frozen source.
#[derive(Clone)]
pub struct Source {
    pub(crate) dimensions: [u32; 2],
    pub(crate) pixels: std::sync::Arc<[u8]>,
}
impl Source {
    pub fn prepare(image: &RgbaImage, cancelled: impl Fn() -> bool) -> Result<Self, Error> {
        let (w, h) = image.dimensions();
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) > crate::image_ops::MAX_PIXELS {
            return Err(Error::Invalid);
        }
        let mut pixels = Vec::with_capacity(image.as_raw().len());
        for (i, p) in image.as_raw().chunks_exact(4).enumerate() {
            if i % 2048 == 0 && cancelled() {
                return Err(Error::Cancelled);
            }
            let alpha = u32::from(p[3]);
            let c = |i| (u32::from(p[i]) * alpha + 127) / 255;
            pixels.extend_from_slice(
                &((alpha << 24) | (c(0) << 16) | (c(1) << 8) | c(2)).to_ne_bytes(),
            );
        }
        if cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(Self {
            dimensions: [w, h],
            pixels: pixels.into(),
        })
    }
}
pub fn render(
    source: &RgbaImage,
    spec: TileSpec,
    cancelled: impl Fn() -> bool,
) -> Result<Tile, Error> {
    if !spec.valid() || source.dimensions() != (spec.source[0], spec.source[1]) {
        return Err(Error::Invalid);
    }
    let source = Source::prepare(source, &cancelled)?;
    render_prepared(&source, spec, cancelled)
}
pub fn render_prepared(
    source: &Source,
    spec: TileSpec,
    cancelled: impl Fn() -> bool,
) -> Result<Tile, Error> {
    if !spec.valid() || source.dimensions != spec.source {
        return Err(Error::Invalid);
    }
    super::filters::render(source, spec, cancelled)
}
impl Tile {
    /// Straight RGBA is for export/tests; GTK can use native premultiplied
    /// pixels directly without a lossy unpremultiply/re-premultiply cycle.
    pub fn rgba(&self) -> Vec<u8> {
        self.pixels
            .chunks_exact(4)
            .flat_map(|p| {
                let argb = u32::from_ne_bytes(p.try_into().unwrap());
                let a = (argb >> 24) & 255;
                let c = |shift: u32| {
                    if a == 0 {
                        0
                    } else {
                        // Match Cairo's PNG conversion, including its byte
                        // conversion of BEST-filter overshoot at alpha edges.
                        ((((argb >> shift) & 255) * 255 + a / 2) / a) as u8
                    }
                };
                [c(16), c(8), c(0), a as u8]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::Rgba;
    #[test]
    fn blur_preserves_constant_premultiplied_color_and_opaque_coverage() {
        for alpha in [128, 255] {
            let image = RgbaImage::from_pixel(65, 49, super::super::Rgba([100, 110, 120, alpha]));
            for level in [3, 8, 16, 24] {
                let spec = TileSpec::plan(
                    [65, 49],
                    Effect::Blur,
                    level,
                    Bounds {
                        x: 0.,
                        y: 0.,
                        width: 65.,
                        height: 49.,
                    },
                )
                .unwrap();
                let rendered = render(&image, spec, || false).unwrap();
                let expected = Source::prepare(&image, || false).unwrap();
                assert!(
                    rendered
                        .pixels
                        .chunks_exact(4)
                        .all(|p| p == &expected.pixels[..4]),
                    "blur level {level} alpha {alpha} changed constant color/coverage: {:?}",
                    &rendered.pixels[..4]
                );
            }
        }
    }
    #[test]
    fn region_tiles_keep_global_phase_and_blur_halo_matches_full_image() {
        let image = RgbaImage::from_fn(101, 73, |x, y| {
            Rgba([
                (x * 37 + y * 17) as u8,
                (x * 13 + y * 41) as u8,
                (x * 53 + y * 3) as u8,
                255,
            ])
        });
        let roi = Bounds {
            x: 33.,
            y: 25.,
            width: 17.,
            height: 14.,
        };
        for effect in [Effect::Pixelate, Effect::Blur] {
            for level in [3, 7, 24] {
                let full = render(
                    &image,
                    TileSpec::plan(
                        [101, 73],
                        effect,
                        level,
                        Bounds {
                            x: 0.,
                            y: 0.,
                            width: 101.,
                            height: 73.,
                        },
                    )
                    .unwrap(),
                    || false,
                )
                .unwrap();
                let tile = render(
                    &image,
                    TileSpec::plan([101, 73], effect, level, roi).unwrap(),
                    || false,
                )
                .unwrap();
                let (_, _, sx, sy) = grid([101, 73], effect, u32::from(level));
                for y in (roi.y / sy).floor() as u32..((roi.y + roi.height) / sy).ceil() as u32 {
                    for x in (roi.x / sx).floor() as u32..((roi.x + roi.width) / sx).ceil() as u32 {
                        let a = (y * full.spec.width + x) as usize * 4;
                        let b = ((y - tile.spec.top) * tile.spec.width + x - tile.spec.left)
                            as usize
                            * 4;
                        assert_eq!(
                            &full.pixels[a..a + 4],
                            &tile.pixels[b..b + 4],
                            "{effect:?}/{level} at {x},{y}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn pixelation_integrates_blocks_and_preserves_premultiplied_alpha() {
        let spec = |w, h| {
            TileSpec::plan(
                [w, h],
                Effect::Pixelate,
                3,
                Bounds {
                    x: 0.,
                    y: 0.,
                    width: f64::from(w),
                    height: f64::from(h),
                },
            )
            .unwrap()
        };
        let image = RgbaImage::from_fn(5, 3, |x, _| Rgba([if x < 3 { 0 } else { 255 }, 0, 0, 255]));
        let tile = render(&image, spec(5, 3), || false).unwrap();
        assert_eq!(tile.rgba(), [0, 0, 0, 255, 255, 0, 0, 255]);
        let image = RgbaImage::from_fn(3, 3, |x, _| {
            if x == 0 {
                Rgba([255, 0, 0, 0])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        let pixel = render(&image, spec(3, 3), || false).unwrap().rgba();
        assert_eq!(&pixel[..3], &[0, 0, 255]);
        assert!(pixel[3] > 0);
    }
    #[test]
    fn malformed_requests_and_cooperative_cancellation_never_publish_partial_tiles() {
        assert!(
            TileSpec::plan(
                [0, 1],
                Effect::Blur,
                6,
                Bounds {
                    x: 0.,
                    y: 0.,
                    width: 1.,
                    height: 1.
                }
            )
            .is_none()
        );
        assert!(
            TileSpec::plan(
                [10, 10],
                Effect::Blur,
                6,
                Bounds {
                    x: f64::NAN,
                    y: 0.,
                    width: 1.,
                    height: 1.
                }
            )
            .is_none()
        );
        let image = RgbaImage::from_pixel(50, 50, Rgba([255, 255, 255, 255]));
        let mut spec = TileSpec::plan(
            [50, 50],
            Effect::Blur,
            6,
            Bounds {
                x: 0.,
                y: 0.,
                width: 50.,
                height: 50.,
            },
        )
        .unwrap();
        let calls = std::cell::Cell::new(0);
        assert!(matches!(
            render(&image, spec, || {
                let n = calls.get() + 1;
                calls.set(n);
                n >= 3
            }),
            Err(Error::Cancelled)
        ));
        spec.width = u32::MAX;
        assert!(matches!(
            render(&image, spec, || false),
            Err(Error::Invalid)
        ));
    }
    #[test]
    fn wide_single_row_work_checks_cancellation_between_bounded_batches() {
        let image = RgbaImage::from_pixel(24_000, 1, Rgba([80, 120, 160, 255]));
        let prepared = Source::prepare(&image, || false).unwrap();
        let spec = TileSpec::plan(
            [24_000, 1],
            Effect::Pixelate,
            3,
            Bounds {
                x: 0.,
                y: 0.,
                width: 24_000.,
                height: 1.,
            },
        )
        .unwrap();
        let calls = std::cell::Cell::new(0);
        assert!(matches!(
            render_prepared(&prepared, spec, || {
                calls.set(calls.get() + 1);
                calls.get() == 4
            }),
            Err(Error::Cancelled)
        ));
        assert_eq!(calls.get(), 4);
        let calls = std::cell::Cell::new(0);
        assert!(matches!(
            Source::prepare(&image, || {
                calls.set(calls.get() + 1);
                calls.get() == 4
            }),
            Err(Error::Cancelled)
        ));
        assert_eq!(calls.get(), 4);
    }
}

#[cfg(test)]
mod strength_tests {
    use super::*;
    #[test]
    fn strong_reduction_does_not_alias_dense_content_into_visible_waves() {
        let image = RgbaImage::from_fn(1920, 1, |x, _| {
            let v = if (x / 16) % 2 == 0 { 0 } else { 255 };
            super::super::Rgba([v, v, v, 255])
        });
        let spec = TileSpec::plan_scaled(
            [1920, 1],
            Effect::Blur,
            50,
            Bounds {
                x: 0.,
                y: 0.,
                width: 1920.,
                height: 1.,
            },
            2.,
        )
        .unwrap();
        let tile = render(&image, spec, || false).unwrap();
        for p in tile
            .pixels
            .chunks_exact(4)
            .skip(6)
            .take(spec.width as usize - 12)
        {
            assert!(
                (120..=135).contains(&p[0]),
                "large-factor alias survived: {p:?}"
            );
            assert_eq!(p[3], 255);
        }
    }
    #[test]
    fn density_maps_levels_to_source_pixels_without_changing_the_ui_range() {
        let roi = Bounds {
            x: 20.,
            y: 20.,
            width: 80.,
            height: 60.,
        };
        for effect in [Effect::Pixelate, Effect::Blur] {
            for scale in [0.5, 1., 1.25, 1.5, 2., 3.] {
                let spec = TileSpec::plan_scaled([400, 300], effect, 10, roi, scale).unwrap();
                assert_eq!(spec.level, 10);
                assert_eq!(spec.size, (10. * scale).round() as u32);
                assert!(spec.valid());
            }
            for scale in [0., -1., f64::NAN, f64::INFINITY] {
                assert!(TileSpec::plan_scaled([400, 300], effect, 10, roi, scale).is_none());
            }
        }
    }
    #[test]
    fn effect_strength_ranges_defaults_and_tile_validation_share_one_definition() {
        for effect in [Effect::Pixelate, Effect::Blur] {
            let range = [3, 24, 8];
            assert_eq!(effect.strength_range(), range);
            for value in [0., f64::NAN] {
                assert_eq!(effect.strength(value), range[2]);
            }
            assert_eq!(effect.strength(-20.), range[0]);
            assert_eq!(effect.strength(100.), range[1]);
            let roi = Bounds {
                x: 2.,
                y: 3.,
                width: 40.,
                height: 30.,
            };
            for level in [range[0], range[1], range[2]] {
                assert!(
                    TileSpec::plan([100, 80], effect, level, roi)
                        .unwrap()
                        .valid()
                );
            }
            let mut invalid = TileSpec::plan([100, 80], effect, range[2], roi).unwrap();
            invalid.level = range[1] + 1;
            assert!(!invalid.valid());
        }
    }
}
