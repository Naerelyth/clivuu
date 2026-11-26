//! Shared user-image export: format detection, quality validation and encoding.
//! Callers choose configuration and publication policy; no GTK or config IO here.
use crate::{Result, i18n::t};
use image::{ImageEncoder, RgbaImage};
use std::{io::Write, path::Path};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
}
impl Format {
    pub fn from_path(path: &Path) -> Result<Self> {
        match path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => Ok(Self::Png),
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            _ => Err(t("Choose a PNG or JPEG file name").into()),
        }
    }
}
pub enum Source<'a> {
    Pixels(&'a RgbaImage),
    Png(&'a [u8]),
}
/// PNG bytes stay unchanged. JPEG is encoded with a white alpha background;
/// changing the suffix never disguises PNG bytes as another format.
pub fn write(source: Source<'_>, path: &Path, quality: i32, mut writer: impl Write) -> Result<()> {
    let format = Format::from_path(path)?;
    if !(0..=100).contains(&quality) {
        return Err(t("JPEG quality must be between 0 and 100").into());
    }
    let image = match (format, source) {
        (Format::Png, Source::Png(bytes)) => {
            writer.write_all(bytes)?;
            return Ok(());
        }
        (_, Source::Pixels(image)) => std::borrow::Cow::Borrowed(image),
        (_, Source::Png(bytes)) => std::borrow::Cow::Owned(crate::image_ops::decode(bytes)?),
    };
    if format == Format::Png {
        image::codecs::png::PngEncoder::new_with_quality(
            writer,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::Sub,
        )
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )?;
    } else {
        let mut rgb = image::RgbImage::new(image.width(), image.height());
        for (src, dst) in image.pixels().zip(rgb.pixels_mut()) {
            let a = u32::from(src[3]);
            for c in 0..3 {
                dst[c] = ((u32::from(src[c]) * a + 255 * (255 - a) + 127) / 255) as u8;
            }
        }
        // PixPin's Qt JPEG path uses libjpeg quality scaling: 0 maps to 1,
        // then 5000/q below 50 or 200-2q above it. image uses the same
        // standard tables/scaling and baseline coefficient bounds.
        image::codecs::jpeg::JpegEncoder::new_with_quality(writer, quality.max(1) as u8)
            .encode_image(&rgb)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jpeg_quality_matches_reference_quantization_and_both_suffixes() {
        use image::ImageEncoder;
        let image = image::RgbaImage::from_fn(16, 16, |x, y| {
            image::Rgba([(x * 15) as u8, (y * 15) as u8, 91, 255])
        });
        let mut png = vec![];
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(image.as_raw(), 16, 16, image::ExtendedColorType::Rgba8)
            .unwrap();
        let tables: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/data/jpeg-standard-tables.json"))
                .unwrap();
        let zigzag: [usize; 64] = [
            0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34,
            27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37,
            44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
        ];
        for quality in [0, 1, 25, 49, 50, 75, 99, 100] {
            let mut jpg = vec![];
            let mut jpeg = vec![];
            write(Source::Png(&png), Path::new("x.jpg"), quality, &mut jpg).unwrap();
            write(Source::Png(&png), Path::new("x.jpeg"), quality, &mut jpeg).unwrap();
            assert_eq!(jpg, jpeg);
            let q = quality.clamp(1, 100) as u32;
            let scale = if q < 50 { 5000 / q } else { 200 - 2 * q };
            let mut offset = 2;
            let mut found = 0;
            while offset < jpg.len() {
                assert_eq!(jpg[offset], 0xff);
                let marker = jpg[offset + 1];
                if marker == 0xda {
                    break;
                }
                let size = u16::from_be_bytes([jpg[offset + 2], jpg[offset + 3]]) as usize;
                if marker == 0xdb {
                    let mut start = offset + 4;
                    while start < offset + 2 + size {
                        let table = jpg[start];
                        assert!(table < 2);
                        start += 1;
                        let base = tables[if table == 0 { "luma" } else { "chroma" }]
                            .as_array()
                            .unwrap();
                        for (i, index) in zigzag.iter().enumerate() {
                            let expected = ((base[*index].as_u64().unwrap() as u32 * scale + 50)
                                / 100)
                                .clamp(1, 255) as u8;
                            assert_eq!(
                                jpg[start + i],
                                expected,
                                "quality={quality}, table={table}, index={index}"
                            );
                        }
                        start += 64;
                        found += 1;
                    }
                }
                offset += 2 + size;
            }
            assert_eq!(found, 2);
        }
    }
    #[test]
    fn validates_quality_and_unifies_pixel_and_encoded_sources() {
        let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([32, 64, 128, 200]));
        let mut png = vec![];
        write(Source::Pixels(&image), Path::new("x.png"), 100, &mut png).unwrap();
        for q in [-1, 101] {
            assert!(write(Source::Pixels(&image), Path::new("x.jpg"), q, Vec::new()).is_err());
        }
        for q in [0, 100] {
            let mut a = vec![];
            let mut b = vec![];
            write(Source::Pixels(&image), Path::new("x.jpg"), q, &mut a).unwrap();
            write(Source::Png(&png), Path::new("x.jpeg"), q, &mut b).unwrap();
            assert_eq!(a, b);
        }
    }
    #[test]
    fn jpeg_has_real_jpeg_bytes_and_png_keeps_alpha() {
        use image::ImageEncoder;
        let source = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 0]));
        let mut png = vec![];
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(source.as_raw(), 4, 4, image::ExtendedColorType::Rgba8)
            .unwrap();
        let mut encoded = vec![];
        write(Source::Png(&png), Path::new("x.jpg"), 100, &mut encoded).unwrap();
        assert!(encoded.starts_with(&[0xff, 0xd8]));
        let decoded = image::load_from_memory(&encoded).unwrap().to_rgb8();
        assert!(decoded.get_pixel(1, 1)[0] > 250);
        let mut same = vec![];
        write(Source::Png(&png), Path::new("x.PNG"), 100, &mut same).unwrap();
        assert_eq!(same, png);
    }
}
