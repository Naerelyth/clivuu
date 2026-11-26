//! Compose independently scaled monitor streams in compositor coordinates.
use crate::{Result, i18n::t, image_ops, protocol::Rect};
use image::{RgbaImage, imageops};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Frame {
    pub path: String,
    pub rect: Rect,
}

fn extent(r: Rect) -> Result<(i64, i64)> {
    r.validate()?;
    if r.x.unsigned_abs() > 10_000_000 || r.y.unsigned_abs() > 10_000_000 {
        return Err(t("Invalid monitor layout").into());
    }
    Ok((r.x + i64::from(r.width), r.y + i64::from(r.height)))
}

fn pixel(value: i64, origin: i64, scale: f64) -> i64 {
    ((value - origin) as f64 * scale).round() as i64
}

pub fn compose(
    frames: &[Frame],
    region: Option<Rect>,
    destination: &str,
) -> Result<serde_json::Value> {
    if frames.is_empty() || frames.len() > 16 {
        return Err(t("Invalid monitor layout").into());
    }
    let mut x = i64::MAX;
    let mut y = i64::MAX;
    let mut right = i64::MIN;
    let mut bottom = i64::MIN;
    let mut scale = 0.0_f64;
    for frame in frames {
        let (r, b) = extent(frame.rect)?;
        x = x.min(frame.rect.x);
        y = y.min(frame.rect.y);
        right = right.max(r);
        bottom = bottom.max(b);
        // Probe dimensions before allocating any decoded image.
        let reader = image::ImageReader::open(&frame.path)?.with_guessed_format()?;
        let (w, h) = reader.into_dimensions()?;
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) > image_ops::MAX_PIXELS {
            return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
        }
        scale = scale
            .max(f64::from(w) / f64::from(frame.rect.width))
            .max(f64::from(h) / f64::from(frame.rect.height));
    }
    let region = region.unwrap_or(Rect {
        x,
        y,
        width: u32::try_from(right - x)?,
        height: u32::try_from(bottom - y)?,
    });
    let (rr, rb) = extent(region)?;
    if region.x < x || region.y < y || rr > right || rb > bottom {
        return Err(t("The selection is outside the monitor layout").into());
    }
    let width = pixel(rr, region.x, scale);
    let height = pixel(rb, region.y, scale);
    if width < 2
        || height < 2
        || width as u64 > image_ops::MAX_PIXELS
        || height as u64 > image_ops::MAX_PIXELS
        || width as u64 * height as u64 > image_ops::MAX_PIXELS
    {
        return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
    }
    let mut output = RgbaImage::new(width as u32, height as u32);
    let mut touched = false;
    for frame in frames {
        let r = frame.rect;
        let (fr, fb) = extent(r)?;
        let (ix, iy, ir, ib) = (r.x.max(region.x), r.y.max(region.y), fr.min(rr), fb.min(rb));
        if ix >= ir || iy >= ib {
            continue;
        }
        touched = true;
        let source = image_ops::load(&frame.path)?;
        let sx = f64::from(source.width()) / f64::from(r.width);
        let sy = f64::from(source.height()) / f64::from(r.height);
        let (left, top) = (pixel(ix, r.x, sx), pixel(iy, r.y, sy));
        let (sw, sh) = (pixel(ir, r.x, sx) - left, pixel(ib, r.y, sy) - top);
        if sw < 1 || sh < 1 {
            return Err(t("Invalid monitor layout").into());
        }
        let (dx, dy) = (pixel(ix, region.x, scale), pixel(iy, region.y, scale));
        let (dw, dh) = (
            pixel(ir, region.x, scale) - dx,
            pixel(ib, region.y, scale) - dy,
        );
        let crop =
            imageops::crop_imm(&source, left as u32, top as u32, sw as u32, sh as u32).to_image();
        let tile = imageops::resize(&crop, dw as u32, dh as u32, imageops::FilterType::Triangle);
        imageops::replace(&mut output, &tile, dx, dy);
    }
    if !touched {
        return Err(t("The selection is outside the monitor layout").into());
    }
    image_ops::save(&output, destination)?;
    Ok(serde_json::json!({"rect": region, "scale": scale, "width": width, "height": height}))
}

/// Compose selected pins at their displayed positions, preserving z-order,
/// alpha and opacity. Scale is the greatest source density in the selection.
pub fn pins(
    layers: &[(RgbaImage, crate::annotation::Bounds, f64)],
) -> Result<(RgbaImage, crate::annotation::Bounds)> {
    use crate::annotation::Bounds;
    if layers.is_empty() || layers.len() > crate::pins::ACTIVE_LIMIT {
        return Err(t("Invalid monitor layout").into());
    }
    let (mut x, mut y, mut right, mut bottom, mut scale) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        0f64,
    );
    for (image, r, opacity) in layers {
        if ![r.x, r.y, r.width, r.height, *opacity]
            .iter()
            .all(|n| n.is_finite())
            || r.width <= 0.
            || r.height <= 0.
            || image.width() == 0
            || image.height() == 0
        {
            return Err(t("Invalid monitor layout").into());
        }
        x = x.min(r.x);
        y = y.min(r.y);
        right = right.max(r.x + r.width);
        bottom = bottom.max(r.y + r.height);
        scale = scale
            .max(image.width() as f64 / r.width)
            .max(image.height() as f64 / r.height);
    }
    let bounds = Bounds {
        x,
        y,
        width: right - x,
        height: bottom - y,
    };
    let width = (bounds.width * scale).ceil();
    let height = (bounds.height * scale).ceil();
    if !width.is_finite()
        || !height.is_finite()
        || width < 1.
        || height < 1.
        || width * height > image_ops::MAX_PIXELS as f64
    {
        return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
    }
    let mut output = RgbaImage::new(width as u32, height as u32);
    for (image, r, opacity) in layers {
        let x = ((r.x - bounds.x) * scale).round() as i64;
        let y = ((r.y - bounds.y) * scale).round() as i64;
        let width = (r.width * scale).round().max(1.) as u32;
        let height = (r.height * scale).round().max(1.) as u32;
        let mut tile = if image.dimensions() == (width, height) {
            image.clone()
        } else {
            imageops::resize(image, width, height, imageops::FilterType::Triangle)
        };
        if *opacity < 1. {
            for p in tile.pixels_mut() {
                p[3] = (p[3] as f64 * opacity.clamp(0., 1.)).round() as u8;
            }
        }
        imageops::overlay(&mut output, &tile, x, y);
    }
    Ok((output, bounds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn selected_pins_keep_positions_alpha_opacity_and_density() {
        use crate::annotation::Bounds;
        let r = Bounds {
            x: -5.,
            y: 10.,
            width: 2.,
            height: 2.,
        };
        let a = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));
        let b = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 255, 255]));
        let (out, bounds) = pins(&[(a, r, 1.), (b, Bounds { x: -3., ..r }, 0.5)]).unwrap();
        assert_eq!(out.dimensions(), (8, 4));
        assert_eq!(bounds.width, 4.);
        assert_eq!(out.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(out.get_pixel(7, 0).0, [0, 0, 255, 128]);
        let huge = Bounds { width: 1e9, ..r };
        assert!(
            pins(&[
                (RgbaImage::new(1, 1), huge, 1.),
                (RgbaImage::new(2, 2), r, 1.)
            ])
            .is_err()
        );
    }
    #[test]
    fn rejects_dimension_overflow_before_allocating() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("clivuu-compose-limit-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let wide = dir.join("wide.png").to_string_lossy().into_owned();
        let small = dir.join("small.png").to_string_lossy().into_owned();
        let output = dir.join("out.png").to_string_lossy().into_owned();
        image_ops::save(&RgbaImage::new(65536, 2), &wide)?;
        image_ops::save(&RgbaImage::new(2, 2), &small)?;
        let frames = [
            Frame {
                path: wide,
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 2,
                },
            },
            Frame {
                path: small,
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: 131072,
                    height: 131072,
                },
            },
        ];
        assert!(compose(&frames, None, &output).is_err());
        assert!(!std::path::Path::new(&output).exists());
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn mixed_scale_negative_origin_and_cross_monitor_crop() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("clivuu-compose-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let a = dir.join("a.png").to_string_lossy().into_owned();
        let b = dir.join("b.png").to_string_lossy().into_owned();
        let out = dir.join("out.png").to_string_lossy().into_owned();
        image_ops::save(&RgbaImage::from_pixel(200, 160, Rgba([255, 0, 0, 255])), &a)?;
        image_ops::save(&RgbaImage::from_pixel(125, 100, Rgba([0, 0, 255, 255])), &b)?;
        let frames = [
            Frame {
                path: a,
                rect: Rect {
                    x: -100,
                    y: 0,
                    width: 100,
                    height: 80,
                },
            },
            Frame {
                path: b,
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 80,
                },
            },
        ];
        let rect = Rect {
            x: -25,
            y: 10,
            width: 50,
            height: 40,
        };
        let info = compose(&frames, Some(rect), &out)?;
        assert_eq!(info["scale"], 2.0);
        let image = image_ops::load(&out)?;
        assert_eq!(image.dimensions(), (100, 80));
        assert_eq!(image.get_pixel(49, 40).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(50, 40).0, [0, 0, 255, 255]);
        assert!(compose(&frames, Some(Rect { x: -101, ..rect }), &out).is_err());
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
}
