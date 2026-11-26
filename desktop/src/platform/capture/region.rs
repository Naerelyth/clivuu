//! A reusable region reader over one authorized continuous stream.
use super::{
    frames::Frames,
    session::{Session, Stream},
};
use crate::infrastructure::tasks::{TaskError, TaskPool};
use clivuu_core::{annotation::Bounds, i18n::t, image::RgbaImage};
use gtk::gio;
use std::{
    rc::Rc,
    sync::{Arc, Mutex},
};
type CachedCrop = (Arc<RgbaImage>, Bounds, Arc<RgbaImage>);
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerState {
    At([f64; 2]),
    LeftMonitor,
    Unchanged,
}

pub struct RegionSource {
    pub session: Session,
    frames: Arc<Frames>,
    pub stream: Stream,
    cached_crop: Mutex<Option<CachedCrop>>,
}
impl RegionSource {
    pub async fn open(session: Session, rect: Bounds, pool: &Rc<TaskPool>) -> Result<Self, String> {
        let stream = session
            .streams
            .iter()
            .find(|s| s.rect.is_some_and(|r| contains(r, rect)))
            .cloned()
            .ok_or_else(|| t("Select the monitor containing the entire scrolling region"))?;
        let frames = Frames::open(&session, &stream, pool).await?;
        Ok(Self {
            session,
            frames,
            stream,
            cached_crop: Mutex::new(None),
        })
    }
    pub async fn initial_image(
        &self,
        rect: Bounds,
        pool: &Rc<TaskPool>,
        cancel: &gio::Cancellable,
    ) -> Result<Arc<RgbaImage>, String> {
        let pixels = self.frames.settled_frame(pool, cancel).await?;
        self.crop(pixels, rect, pool, cancel).await
    }
    pub fn covers(&self, rect: Bounds) -> bool {
        self.stream.rect.is_some_and(|outer| contains(outer, rect))
    }
    pub fn has_pending_frame(&self) -> bool {
        self.frames.has_pending_frame()
    }
    pub fn notifier(&self) -> async_channel::Receiver<()> {
        self.frames.notifier()
    }
    /// Replace the terminal reader after hiding the HUD, keeping authorization
    /// and system sharing feedback stable. Reactivation requests a first frame;
    /// there is no old application cache or competing live consumer.
    pub async fn clean_image(
        &self,
        rect: Bounds,
        pool: &Rc<TaskPool>,
        cancel: &gio::Cancellable,
    ) -> Result<Arc<RgbaImage>, String> {
        *self.cached_crop.lock().unwrap() = None;
        self.frames.stop(pool).await;
        let frames = Frames::open(&self.session, &self.stream, pool).await?;
        let result = frames.frame(pool, &self.session.scope.token).await;
        frames.stop(pool).await;
        self.crop(result?, rect, pool, cancel).await
    }
    /// Rebuild only the terminal reader after moving capture controls. The
    /// authorized session (including remote input) remains unchanged.
    pub async fn refresh(&mut self, pool: &Rc<TaskPool>) -> Result<(), String> {
        *self.cached_crop.lock().unwrap() = None;
        self.frames.stop(pool).await;
        self.frames = Frames::open(&self.session, &self.stream, pool).await?;
        Ok(())
    }
    pub async fn close(&self, pool: &Rc<TaskPool>) {
        self.frames.stop(pool).await;
        self.session.close().await;
    }
    pub async fn image(
        &self,
        rect: Bounds,
        pool: &Rc<TaskPool>,
        cancel: &gio::Cancellable,
    ) -> Result<Arc<RgbaImage>, String> {
        let pixels = self.frames.frame(pool, &self.session.scope.token).await?;
        if let Some((full, cached_rect, cropped)) = self.cached_crop.lock().unwrap().as_ref()
            && Arc::ptr_eq(&pixels, full)
            && *cached_rect == rect
        {
            return Ok(cropped.clone());
        }
        let cropped = self.crop(pixels.clone(), rect, pool, cancel).await?;
        *self.cached_crop.lock().unwrap() = Some((pixels, rect, cropped.clone()));
        Ok(cropped)
    }
    pub async fn pointer_state(&self, pool: &Rc<TaskPool>) -> Result<PointerState, String> {
        let Some(pixels) = self
            .frames
            .pointer_frame(pool, &self.session.scope.token)
            .await?
        else {
            return Ok(PointerState::Unchanged);
        };
        let rect = self.stream.rect.unwrap();
        Ok(match self.frames.pointer() {
            Some(p) => PointerState::At([
                rect.x + p[0] * rect.width / pixels.width() as f64,
                rect.y + p[1] * rect.height / pixels.height() as f64,
            ]),
            None => PointerState::LeftMonitor,
        })
    }
    async fn crop(
        &self,
        pixels: Arc<RgbaImage>,
        rect: Bounds,
        pool: &Rc<TaskPool>,
        cancel: &gio::Cancellable,
    ) -> Result<Arc<RgbaImage>, String> {
        let outer = self.stream.rect.unwrap();
        let task = pool
            .submit(move |token| {
                token.check()?;
                let Some([x, y, width, height]) =
                    pixel_crop(outer, rect, pixels.width(), pixels.height())
                else {
                    return Err(TaskError::Failed(t("The system returned no image")));
                };
                let image = clivuu_core::image::imageops::crop_imm(&*pixels, x, y, width, height)
                    .to_image();
                token.check()?;
                Ok(Arc::new(image))
            })
            .map_err(|e| e.to_string())?;
        gio::CancellableFuture::new(task.result(), cancel.clone())
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|e| e.to_string())
    }
}

pub(crate) fn pixel_crop(outer: Bounds, rect: Bounds, width: u32, height: u32) -> Option<[u32; 4]> {
    if !contains(outer, rect) || width == 0 || height == 0 {
        return None;
    }
    let sx = f64::from(width) / outer.width;
    let sy = f64::from(height) / outer.height;
    let x = ((rect.x - outer.x) * sx)
        .round()
        .clamp(0., f64::from(width)) as u32;
    let y = ((rect.y - outer.y) * sy)
        .round()
        .clamp(0., f64::from(height)) as u32;
    let right = ((rect.x + rect.width - outer.x) * sx)
        .round()
        .clamp(0., f64::from(width)) as u32;
    let bottom = ((rect.y + rect.height - outer.y) * sy)
        .round()
        .clamp(0., f64::from(height)) as u32;
    (right > x && bottom > y).then_some([x, y, right - x, bottom - y])
}

fn contains(a: Bounds, b: Bounds) -> bool {
    const EPS: f64 = 1e-9;
    [a.x, a.y, a.width, a.height, b.x, b.y, b.width, b.height]
        .iter()
        .all(|v| v.is_finite())
        && b.width > 0.
        && b.height > 0.
        && b.x >= a.x - EPS
        && b.y >= a.y - EPS
        && b.x + b.width <= a.x + a.width + EPS
        && b.y + b.height <= a.y + a.height + EPS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::capture::session::mutter_area;

    #[test]
    fn fractional_crops_preserve_exact_physical_pixels_without_boundary_drift() {
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0] {
            let output = clivuu_core::capture::Output {
                rect: Bounds {
                    x: -400.,
                    y: 120.,
                    width: 1920.,
                    height: 1080.,
                },
                scale,
            };
            for px in [0u32, 1, 2, 3, 5, 7, 11, 17, 199] {
                for pw in [1u32, 2, 3, 49, 81, 125, 321] {
                    let rect = Bounds {
                        x: -400. + f64::from(px) / scale,
                        y: 120. + f64::from(px) / scale,
                        width: f64::from(pw) / scale,
                        height: f64::from(pw + 48) / scale,
                    };
                    let (_, _, _, _, area) = mutter_area(rect, &[output]).expect("mutter area");
                    let stream_w = (area.width * scale).round() as u32;
                    let stream_h = (area.height * scale).round() as u32;
                    let [cx, cy, cw, ch] =
                        pixel_crop(area, rect, stream_w, stream_h).expect("pixel crop");
                    let expected_x = ((rect.x - area.x) * scale).round() as u32;
                    let expected_y = ((rect.y - area.y) * scale).round() as u32;
                    assert_eq!((cx, cy, cw, ch), (expected_x, expected_y, pw, pw + 48));
                }
            }
        }
    }
}
