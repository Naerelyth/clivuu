//! Single-shot binary-framed OCR worker protocol over anonymous files or pipes.
//! Each subprocess recognizes one image and exits immediately so ONNX memory and thread pools
//! never remain resident inside `clivuu-desktop`.
use crate::{Result, i18n::t, image::RgbaImage};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

use super::model::{OcrDocument, OcrSettings};

const MAGIC: &[u8; 8] = b"CLVOCR01";
const MAX_HEADER_BYTES: usize = 64 * 1024;

#[derive(Serialize, Deserialize)]
struct RequestHeader {
    width: u32,
    height: u32,
    settings: OcrSettings,
}

#[derive(Serialize, Deserialize)]
pub enum WorkerReply {
    Document(OcrDocument),
    Error(String),
}

pub fn write_request(
    writer: &mut impl Write,
    image: &RgbaImage,
    settings: &OcrSettings,
) -> Result<()> {
    let (width, height) = image.dimensions();
    if width == 0
        || height == 0
        || u64::from(width) * u64::from(height) > crate::image_ops::MAX_PIXELS
    {
        return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
    }
    let header = serde_json::to_vec(&RequestHeader {
        width,
        height,
        settings: settings.clone(),
    })?;
    writer.write_all(MAGIC)?;
    writer.write_all(&(header.len() as u32).to_le_bytes())?;
    writer.write_all(&header)?;
    writer.write_all(image.as_raw())?;
    writer.flush()?;
    Ok(())
}

pub fn read_request(reader: &mut impl Read) -> Result<(RgbaImage, OcrSettings)> {
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(t("Invalid OCR worker request header").into());
    }
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let header_len = u32::from_le_bytes(len_buf) as usize;
    if header_len == 0 || header_len > MAX_HEADER_BYTES {
        return Err(t("Invalid OCR worker request header").into());
    }
    let mut header_bytes = vec![0u8; header_len];
    reader.read_exact(&mut header_bytes)?;
    let header: RequestHeader = serde_json::from_slice(&header_bytes)?;
    if header.width == 0
        || header.height == 0
        || u64::from(header.width) * u64::from(header.height) > crate::image_ops::MAX_PIXELS
    {
        return Err(t("The image exceeds 48 megapixels or has invalid dimensions").into());
    }
    let pixel_bytes = header.width as usize * header.height as usize * 4;
    let mut raw = vec![0u8; pixel_bytes];
    reader.read_exact(&mut raw)?;
    let image = RgbaImage::from_raw(header.width, header.height, raw)
        .ok_or_else(|| t("Invalid OCR worker image payload"))?;
    Ok((image, header.settings))
}

pub fn write_reply(writer: &mut impl Write, reply: &WorkerReply) -> Result<()> {
    let payload = serde_json::to_vec(reply)?;
    writer.write_all(MAGIC)?;
    writer.write_all(&(payload.len() as u32).to_le_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

pub fn read_reply(reader: &mut impl Read) -> Result<OcrDocument> {
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(t("Invalid OCR worker response").into());
    }
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > 16 * 1024 * 1024 {
        return Err(t("Invalid OCR worker response").into());
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    match serde_json::from_slice::<WorkerReply>(&payload)? {
        WorkerReply::Document(doc) => Ok(doc),
        WorkerReply::Error(msg) => Err(msg.into()),
    }
}

/// Entry point invoked by `clivuu worker ocr-document` or `clivuu-desktop --ocr-worker`.
pub fn serve_stdio() -> Result<()> {
    let outcome = (|| -> Result<OcrDocument> {
        let mut stdin = std::io::stdin().lock();
        let (image, settings) = read_request(&mut stdin)?;
        super::recognize_document(&image, &settings)
    })();
    let reply = match outcome {
        Ok(doc) => WorkerReply::Document(doc),
        Err(err) => WorkerReply::Error(err.to_string()),
    };
    let mut stdout = std::io::stdout().lock();
    write_reply(&mut stdout, &reply)
}
