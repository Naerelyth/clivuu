use crate::i18n::t;
use image::{ImageReader, RgbaImage};
use std::{fs, path::Path};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const MAX_PIXELS: u64 = 48_000_000;
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Decode immutable encoded bytes without creating an intermediate image file.
/// Desktop callers run this bounded allocation/decoder work on their task pool.
pub fn decode(bytes: &[u8]) -> Result<RgbaImage> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(t("The image file exceeds 64 MiB").into());
    }
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_PIXELS * 8);
    reader.limits(limits);
    let decoder = image::ImageDecoder::dimensions;
    let dec = reader.into_decoder()?;
    let (w, h) = decoder(&dec);
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
    }
    Ok(image::DynamicImage::from_decoder(dec)?.into_rgba8())
}

/// Read bounded image metadata without allocating the decoded pixel buffer.
pub fn dimensions(path: &str) -> Result<(u32, u32)> {
    if fs::metadata(path)?.len() > MAX_BYTES {
        return Err(t("The image file exceeds 64 MiB").into());
    }
    let size = ImageReader::open(path)?
        .with_guessed_format()?
        .into_dimensions()?;
    if size.0 == 0 || size.1 == 0 || u64::from(size.0) * u64::from(size.1) > MAX_PIXELS {
        return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
    }
    Ok(size)
}

pub fn load(path: &str) -> Result<RgbaImage> {
    if fs::metadata(path)?.len() > MAX_BYTES {
        return Err(t("The image file exceeds 64 MiB").into());
    }
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_PIXELS * 8);
    reader.limits(limits);
    let dec = reader.into_decoder()?;
    let (w, h) = image::ImageDecoder::dimensions(&dec);
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
    }
    Ok(image::DynamicImage::from_decoder(dec)?.into_rgba8())
}

pub fn save(image: &RgbaImage, path: &str) -> Result<()> {
    use crate::image::export::{self, Format, Source};
    let path = Path::new(path);
    let format = Format::from_path(path)?;
    let quality = if format == Format::Jpeg {
        crate::config::image_quality(&crate::config::read()?)
    } else {
        crate::config::default_image_quality()
    };
    use std::io::Write;
    let mut output = std::io::BufWriter::new(fs::File::create(path)?);
    export::write(Source::Pixels(image), path, quality, &mut output)?;
    output.flush()?;
    Ok(())
}

pub fn save_rgba(pixels: Vec<u8>, width: u32, height: u32, path: &str) -> Result<()> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
    }
    let image = RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| t("The image exceeds 48 megapixels or has invalid dimensions"))?;
    save(&image, path)
}

pub fn composite_cursor(
    source: &str,
    cursor: &str,
    destination: &str,
    rect: crate::protocol::Rect,
) -> Result<()> {
    let mut image = load(source)?;
    composite_cursor_image(&mut image, load(cursor)?, rect)?;
    save(&image, destination)
}

/// Shared in-memory path used by the native static capture service. The CLI
/// file adapter above uses precisely the same resampling and alpha rules.
pub fn composite_cursor_image(
    image: &mut RgbaImage,
    cursor: RgbaImage,
    rect: crate::protocol::Rect,
) -> Result<()> {
    if rect.width == 0 || rect.height == 0 || rect.width > 4096 || rect.height > 4096 {
        return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
    }
    let cursor = if cursor.dimensions() == (rect.width, rect.height) {
        cursor
    } else {
        // Interpolate premultiplied color, so transparent pixels cannot darken
        // the outline when the cursor crosses displays with different scales.
        let premultiplied =
            image::Rgba32FImage::from_fn(cursor.width(), cursor.height(), |x, y| {
                let p = cursor.get_pixel(x, y);
                let a = f32::from(p[3]) / 255.0;
                image::Rgba([
                    f32::from(p[0]) / 255.0 * a,
                    f32::from(p[1]) / 255.0 * a,
                    f32::from(p[2]) / 255.0 * a,
                    a,
                ])
            });
        let scaled = image::imageops::resize(
            &premultiplied,
            rect.width,
            rect.height,
            image::imageops::FilterType::Triangle,
        );
        RgbaImage::from_fn(rect.width, rect.height, |x, y| {
            let p = scaled.get_pixel(x, y);
            let a = p[3];
            if a <= 0.0 {
                image::Rgba([0, 0, 0, 0])
            } else {
                image::Rgba([
                    (p[0] / a * 255.0).round() as u8,
                    (p[1] / a * 255.0).round() as u8,
                    (p[2] / a * 255.0).round() as u8,
                    (a * 255.0).round() as u8,
                ])
            }
        })
    };
    // Integer source-over keeps an opaque desktop opaque (float blending can
    // round alpha 255 down to 254) and clips hotspots outside the capture.
    for (x, y, pixel) in cursor.enumerate_pixels() {
        let Some(dx) = rect.x.checked_add(i64::from(x)) else {
            continue;
        };
        let Some(dy) = rect.y.checked_add(i64::from(y)) else {
            continue;
        };
        if dx < 0 || dy < 0 || dx >= i64::from(image.width()) || dy >= i64::from(image.height()) {
            continue;
        }
        let destination = image.get_pixel_mut(dx as u32, dy as u32);
        let (a, b) = (u32::from(pixel[3]), u32::from(destination[3]));
        let alpha = a * 255 + b * (255 - a);
        if alpha == 0 {
            *destination = image::Rgba([0, 0, 0, 0]);
            continue;
        }
        for channel in 0..3 {
            destination[channel] = ((u32::from(pixel[channel]) * a * 255
                + u32::from(destination[channel]) * b * (255 - a)
                + alpha / 2)
                / alpha) as u8;
        }
        destination[3] = ((alpha + 127) / 255) as u8;
    }
    Ok(())
}

pub fn crop(source: &str, destination: &str, rect: crate::protocol::Rect) -> Result<()> {
    let rect = rect.validate()?;
    let image = load(source)?;
    if rect.x < 0
        || rect.y < 0
        || rect.x + i64::from(rect.width) > i64::from(image.width())
        || rect.y + i64::from(rect.height) > i64::from(image.height())
    {
        return Err(t("The crop is outside the image").into());
    }
    save(
        &image::imageops::crop_imm(
            &image,
            rect.x as u32,
            rect.y as u32,
            rect.width,
            rect.height,
        )
        .to_image(),
        destination,
    )
}

/// Region export shared by direct desktop capture and its native pixel mapping.
pub fn crop_capture(image: &RgbaImage, region: crate::capture::RasterRegion) -> Result<RgbaImage> {
    let r = region.source;
    if ![r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite())
        || r.x < 0.
        || r.y < 0.
        || r.width < 1.
        || r.height < 1.
        || r.x + r.width > image.width() as f64
        || r.y + r.height > image.height() as f64
        || region.size.contains(&0)
        || u64::from(region.size[0]) * u64::from(region.size[1]) > MAX_PIXELS
    {
        return Err(t("The crop is outside the image").into());
    }
    let crop = image::imageops::crop_imm(
        image,
        r.x as u32,
        r.y as u32,
        r.width as u32,
        r.height as u32,
    );
    Ok(
        if (r.width as u32, r.height as u32) == (region.size[0], region.size[1]) {
            crop.to_image()
        } else {
            image::imageops::resize(
                &*crop,
                region.size[0],
                region.size[1],
                image::imageops::FilterType::Nearest,
            )
        },
    )
}

pub fn prepare_pin(source: &str, destination: &str) -> Result<serde_json::Value> {
    let image = load(source)?;
    let (w, h) = image.dimensions();
    let ratio = 1.0_f64
        .min(4096.0 / f64::from(w.max(h)))
        .min((8_000_000.0 / (f64::from(w) * f64::from(h))).sqrt());
    let image = if ratio < 1.0 {
        image::imageops::resize(
            &image,
            (f64::from(w) * ratio).round().max(1.0) as u32,
            (f64::from(h) * ratio).round().max(1.0) as u32,
            image::imageops::FilterType::Triangle,
        )
    } else {
        image
    };
    save(&image, destination)?;
    Ok(
        serde_json::json!({"width": image.width(), "height": image.height(),
        "originalWidth": w, "originalHeight": h}),
    )
}

pub fn transform(source: &str, destination: &str, operation: &str) -> Result<()> {
    let image = load(source)?;
    let result = match operation {
        "rotate-right" => image::imageops::rotate90(&image),
        "rotate-left" => image::imageops::rotate270(&image),
        "flip-horizontal" => image::imageops::flip_horizontal(&image),
        "flip-vertical" => image::imageops::flip_vertical(&image),
        _ => return Err(t("Unknown image transformation").into()),
    };
    save(&result, destination)
}

#[cfg(test)]
mod cursor_tests {
    use super::*;
    #[test]
    fn transparent_cursor_scales_clips_and_preserves_source() {
        let directory = format!("/tmp/clivuu-cursor-core-{}", std::process::id());
        fs::create_dir_all(&directory).unwrap();
        let source = format!("{directory}/source.png");
        let cursor = format!("{directory}/cursor.png");
        let output = format!("{directory}/output.png");
        save(
            &RgbaImage::from_pixel(8, 8, image::Rgba([255, 255, 255, 255])),
            &source,
        )
        .unwrap();
        save_rgba([255, 0, 0, 128].repeat(4), 2, 2, &cursor).unwrap();
        composite_cursor(
            &source,
            &cursor,
            &output,
            crate::protocol::Rect {
                x: -1,
                y: -2,
                width: 4,
                height: 4,
            },
        )
        .unwrap();
        let image = load(&output).unwrap();
        assert_eq!(image.get_pixel(0, 0).0, [255, 127, 127, 255]);
        assert_eq!(image.get_pixel(3, 0).0, [255, 255, 255, 255]);
        assert_eq!(image.get_pixel(0, 2).0, [255, 255, 255, 255]);
        assert_eq!(
            load(&source).unwrap().get_pixel(0, 0).0,
            [255, 255, 255, 255]
        );
        assert!(save_rgba(vec![], 0, 2, &cursor).is_err());
        save(
            &RgbaImage::from_pixel(8, 8, image::Rgba([0, 0, 0, 255])),
            &source,
        )
        .unwrap();
        save_rgba(vec![0, 0, 0, 0, 255, 255, 255, 255], 2, 1, &cursor).unwrap();
        composite_cursor(
            &source,
            &cursor,
            &output,
            crate::protocol::Rect {
                x: 0,
                y: 0,
                width: 4,
                height: 1,
            },
        )
        .unwrap();
        let edge = load(&output).unwrap();
        assert_eq!(edge.get_pixel(1, 0).0, [64, 64, 64, 255]);
        assert_eq!(edge.get_pixel(2, 0).0, [191, 191, 191, 255]);
        fs::remove_dir_all(directory).unwrap();
    }
}
