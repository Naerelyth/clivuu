//! Acquisition and bounded image work, with no GTK objects on worker threads.
use super::*;
use clivuu_core::scrolling::{ExcludedRect, LimitReached, Progress};

pub(super) struct SampleError {
    pub message: String,
    pub capacity: bool,
}
impl From<TaskError> for SampleError {
    fn from(error: TaskError) -> Self {
        Self {
            message: error.to_string(),
            capacity: false,
        }
    }
}
fn pixel_masks(pixels: &RgbaImage, rect: Bounds, excluded: Vec<Bounds>) -> Vec<ExcludedRect> {
    let sx = pixels.width() as f64 / rect.width;
    let sy = pixels.height() as f64 / rect.height;
    excluded
        .into_iter()
        .filter_map(|r| {
            let x = ((r.x - rect.x) * sx)
                .floor()
                .clamp(0., pixels.width() as f64) as u32;
            let y = ((r.y - rect.y) * sy)
                .floor()
                .clamp(0., pixels.height() as f64) as u32;
            let right = ((r.x + r.width - rect.x) * sx)
                .ceil()
                .clamp(0., pixels.width() as f64) as u32;
            let bottom = ((r.y + r.height - rect.y) * sy)
                .ceil()
                .clamp(0., pixels.height() as f64) as u32;
            (right > x && bottom > y).then(|| ExcludedRect {
                x,
                y,
                width: right - x,
                height: bottom - y,
            })
        })
        .collect()
}

pub(super) async fn append(
    document: &Arc<Mutex<Document>>,
    pixels: Arc<RgbaImage>,
    rect: Bounds,
    excluded: Vec<Bounds>,
    pool: &TaskPool,
) -> Result<(Progress, Option<Arc<RgbaImage>>, u32), SampleError> {
    let document = document.clone();
    let task = pool.submit(move |token| {
        token.check()?;
        let masks = pixel_masks(&pixels, rect, excluded);
        let mut document = document.lock().unwrap();
        let before = document.cursor_offset();
        let had_frames = document.frames() > 0;
        let progress = match document.push_masked_arc_at(pixels, masks, rect) {
            Ok(progress) => progress,
            Err(error) => {
                return Ok(Err(SampleError {
                    capacity: error.is::<LimitReached>(),
                    message: error.to_string(),
                }));
            }
        };
        let step_px = if had_frames {
            (document.cursor_offset() - before).unsigned_abs() as u32
        } else {
            0
        };
        let preview = if progress.changed {
            Some(Arc::new(
                document
                    .preview()
                    .map_err(|e| TaskError::Failed(e.to_string()))?,
            ))
        } else {
            None
        };
        token.check()?;
        Ok(Ok((progress, preview, step_px)))
    })?;
    task.result().await?
}

pub(super) async fn reanchor(
    document: &Arc<Mutex<Document>>,
    pixels: Arc<RgbaImage>,
    rect: Bounds,
    excluded: Vec<Bounds>,
    pool: &TaskPool,
) -> Result<(Progress, Option<Arc<RgbaImage>>), SampleError> {
    let document = document.clone();
    let task = pool.submit(move |token| {
        token.check()?;
        let masks = pixel_masks(&pixels, rect, excluded);
        let mut document = document.lock().unwrap();
        let progress = match document.reanchor_masked_at((*pixels).clone(), masks, Some(rect)) {
            Ok(progress) => progress,
            Err(error) => {
                return Ok(Err(SampleError {
                    capacity: error.is::<LimitReached>(),
                    message: error.to_string(),
                }));
            }
        };
        let preview = if progress.changed {
            Some(Arc::new(
                document
                    .preview()
                    .map_err(|e| TaskError::Failed(e.to_string()))?,
            ))
        } else {
            None
        };
        token.check()?;
        Ok(Ok((progress, preview)))
    })?;
    task.result().await?
}
pub(super) async fn image(
    document: &Arc<Mutex<Document>>,
    pool: &TaskPool,
) -> Result<Arc<RgbaImage>, String> {
    let document = document.clone();
    pool.submit(move |token| {
        token.check()?;
        let image = document
            .lock()
            .unwrap()
            .image()
            .map_err(|e| TaskError::Failed(e.to_string()))?;
        token.check()?;
        Ok(Arc::new(image))
    })
    .map_err(|e| e.to_string())?
    .result()
    .await
    .map_err(|e| e.to_string())
}

/// Return historical source geometries, independently of the current selection.
pub(super) async fn repairs(
    document: &Arc<Mutex<Document>>,
    pool: &TaskPool,
) -> Result<Vec<clivuu_core::scrolling::Repair>, String> {
    let document = document.clone();
    pool.submit(move |_| Ok(document.lock().unwrap().repairs()))
        .map_err(|e| e.to_string())?
        .result()
        .await
        .map_err(|e| e.to_string())
}
pub(super) async fn repair(
    document: &Arc<Mutex<Document>>,
    request: clivuu_core::scrolling::Repair,
    pixels: Arc<RgbaImage>,
    pool: &TaskPool,
) -> Result<(), String> {
    let document = document.clone();
    pool.submit(move |token| {
        token.check()?;
        document
            .lock()
            .unwrap()
            .repair(request, (*pixels).clone())
            .map_err(|e| TaskError::Failed(e.to_string()))
    })
    .map_err(|e| e.to_string())?
    .result()
    .await
    .map_err(|e| e.to_string())
}
