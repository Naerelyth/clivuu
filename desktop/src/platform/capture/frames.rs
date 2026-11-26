//! A persistent, bounded appsink: keep one frame, reuse the PipeWire stream,
//! and return owned pixels. No PNG files or processes between scroll samples.
use super::session::{Session, Stream};
use crate::infrastructure::tasks::{Cancellation, TaskError, TaskPool};
use clivuu_core::{i18n::t, image::RgbaImage, image_ops};
use gst::prelude::*;
use gst_video::prelude::*;
use gtk::gio;
use std::{
    os::fd::{AsRawFd, OwnedFd},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
pub struct PipelineLease {
    pub pipeline: Option<gst::Pipeline>,
    pub fd: Option<OwnedFd>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newest_frames_and_static_cache_share_pixels_without_reusing_stale_pointer_positions() {
        gst::init().unwrap();
        let pipeline = gst::parse::launch("appsrc name=source format=time caps=video/x-raw,format=RGBA,width=2,height=2,framerate=0/1 ! videoconvert ! video/x-raw,format=RGBA ! appsink name=frames sync=false max-buffers=1 drop=true").unwrap().downcast::<gst::Pipeline>().unwrap();
        let source = pipeline
            .by_name("source")
            .unwrap()
            .downcast::<gst_app::AppSrc>()
            .unwrap();
        let sink = pipeline
            .by_name("frames")
            .unwrap()
            .downcast::<gst_app::AppSink>()
            .unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let cached = Arc::new(RgbaImage::from_pixel(
            2,
            2,
            clivuu_core::image::Rgba([255, 0, 0, 255]),
        ));
        let (_, notify_rx) = async_channel::bounded(1);
        let frames = Arc::new(Frames {
            lease: PipelineLease {
                pipeline: Some(pipeline),
                fd: None,
            },
            sink,
            last: Mutex::new(Some(cached.clone())),
            pointer: Mutex::new(None),
            available_seq: Arc::new(AtomicU64::new(0)),
            consumed_seq: AtomicU64::new(0),
            probe_attached: false,
            notify_rx,
        });
        let pool = TaskPool::new(1, 2).unwrap();
        let read = |mode| {
            let frames = frames.clone();
            let task = pool.submit(move |token| frames.read(&token, mode)).unwrap();
            gtk::glib::MainContext::new().block_on(task.result())
        };
        assert!(Arc::ptr_eq(&cached, &read(ReadMode::Cached).unwrap()));
        assert!(matches!(read(ReadMode::Pointer), Err(TaskError::Busy)));
        let mut buffer = gst::Buffer::from_mut_slice([0u8, 0, 255, 255].repeat(4));
        buffer
            .get_mut()
            .unwrap()
            .set_pts(gst::ClockTime::from_mseconds(1));
        gst_video::VideoRegionOfInterestMeta::add(
            buffer.get_mut().unwrap(),
            "cursor",
            (1, 0, 0, 0),
        );
        source.push_buffer(buffer).unwrap();
        let image = read(ReadMode::Cached).unwrap();
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 255, 255]);
        assert_eq!(frames.pointer(), Some([1., 0.]));
        assert!(Arc::ptr_eq(&image, &read(ReadMode::Cached).unwrap()));
        assert!(matches!(read(ReadMode::Pointer), Err(TaskError::Busy)));
        let mut buffer = gst::Buffer::from_mut_slice([0u8, 255, 0, 255].repeat(4));
        buffer
            .get_mut()
            .unwrap()
            .set_pts(gst::ClockTime::from_mseconds(2));
        source.push_buffer(buffer).unwrap();
        assert_eq!(
            read(ReadMode::Cached).unwrap().get_pixel(0, 0).0,
            [0, 255, 0, 255]
        );
        assert_eq!(frames.pointer(), None);
        let mut buffer = gst::Buffer::from_mut_slice(vec![
            10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
        ]);
        buffer
            .get_mut()
            .unwrap()
            .set_pts(gst::ClockTime::from_mseconds(3));
        gst_video::VideoCropMeta::add(buffer.get_mut().unwrap(), (1, 1, 1, 1));
        gst_video::VideoRegionOfInterestMeta::add(
            buffer.get_mut().unwrap(),
            "cursor",
            (1, 1, 0, 0),
        );
        source.push_buffer(buffer).unwrap();
        let image = read(ReadMode::Cached).unwrap();
        assert_eq!(image.dimensions(), (1, 1));
        assert_eq!(image.get_pixel(0, 0).0, [100, 110, 120, 255]);
        assert_eq!(frames.pointer(), Some([0., 0.]));
        let mut buffer = gst::Buffer::from_mut_slice(vec![0; 16]);
        buffer
            .get_mut()
            .unwrap()
            .set_pts(gst::ClockTime::from_mseconds(4));
        gst_video::VideoCropMeta::add(buffer.get_mut().unwrap(), (1, 1, 2, 2));
        source.push_buffer(buffer).unwrap();
        assert!(
            read(ReadMode::Cached).is_err(),
            "invalid crop must not expose padding or stale pixels"
        );
    }
}
impl PipelineLease {
    pub fn stop(&mut self) {
        if let Some(pipeline) = self.pipeline.take() {
            let _ = pipeline.set_state(gst::State::Null);
        }
        self.fd.take();
    }
}
impl Drop for PipelineLease {
    fn drop(&mut self) {
        if let Some(pipeline) = self.pipeline.take() {
            let fd = self.fd.take();
            let _ = std::thread::Builder::new()
                .name("clivuu-stream-stop".into())
                .spawn(move || {
                    let _ = pipeline.set_state(gst::State::Null);
                    drop(fd);
                });
        }
    }
}
pub fn pipeline(
    description: &str,
    stream: &Stream,
    fd: Option<OwnedFd>,
) -> Result<PipelineLease, TaskError> {
    gst::init().map_err(|e| TaskError::Failed(e.to_string()))?;
    let pipeline = gst::parse::launch(description)
        .map_err(|e| TaskError::Failed(e.to_string()))?
        .downcast::<gst::Pipeline>()
        .map_err(|_| TaskError::Failed(t("Cannot start the capture stream")))?;
    let source = pipeline
        .by_name("source")
        .ok_or_else(|| TaskError::Failed(t("Cannot start the capture stream")))?;
    if let Some(fd) = fd.as_ref() {
        source.set_property("fd", fd.as_raw_fd());
    }
    if let Some(serial) = stream
        .properties
        .get("pipewire-serial")
        .and_then(|v| v.get::<u64>().or_else(|| v.get::<u32>().map(u64::from)))
    {
        source.set_property("target-object", serial.to_string());
    } else {
        source.set_property("path", stream.node.to_string());
    }
    Ok(PipelineLease {
        pipeline: Some(pipeline),
        fd,
    })
}
#[derive(Clone, Copy)]
enum ReadMode {
    Cached,
    Pointer,
}
pub struct Frames {
    lease: PipelineLease,
    sink: gst_app::AppSink,
    last: Mutex<Option<Arc<RgbaImage>>>,
    pointer: Mutex<Option<[f64; 2]>>,
    available_seq: Arc<AtomicU64>,
    consumed_seq: AtomicU64,
    probe_attached: bool,
    notify_rx: async_channel::Receiver<()>,
}
impl Frames {
    pub async fn open(
        session: &Session,
        stream: &Stream,
        pool: &Rc<TaskPool>,
    ) -> Result<Arc<Self>, String> {
        let fd = session.remote().await?;
        let stream = stream.clone();
        let task = pool
            .submit(move |token| {
                token.check()?;
                let lease = pipeline(
                    "pipewiresrc name=source do-timestamp=true ! queue max-size-buffers=2 leaky=downstream ! videoconvert ! video/x-raw,format=RGBA ! appsink name=frames sync=false max-buffers=1 drop=true",
                    &stream,
                    fd,
                )?;
                let pipe = lease.pipeline.as_ref().unwrap();
                let sink = pipe
                    .by_name("frames")
                    .unwrap()
                    .downcast::<gst_app::AppSink>()
                    .unwrap();
                let available_seq = Arc::new(AtomicU64::new(0));
                let (notify_tx, notify_rx) = async_channel::bounded(1);
                let probe_attached = if let Some(pad) = sink.static_pad("sink") {
                    let seq = available_seq.clone();
                    let tx = notify_tx.clone();
                    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
                        seq.fetch_add(1, Ordering::Release);
                        let _ = tx.try_send(());
                        gst::PadProbeReturn::Ok
                    });
                    true
                } else {
                    false
                };
                pipe.set_state(gst::State::Playing)
                    .map_err(|_| TaskError::Failed(t("Cannot start the capture stream")))?;
                Ok(Arc::new(Self {
                    lease,
                    sink,
                    last: Mutex::new(None),
                    pointer: Mutex::new(None),
                    available_seq,
                    consumed_seq: AtomicU64::new(0),
                    probe_attached,
                    notify_rx,
                }))
            })
            .map_err(|e| e.to_string())?;
        gio::CancellableFuture::new(task.result(), session.scope.token.clone())
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|e| e.to_string())
    }
    pub fn has_pending_frame(&self) -> bool {
        !self.probe_attached
            || self.last.lock().unwrap().is_none()
            || self.available_seq.load(Ordering::Acquire)
                != self.consumed_seq.load(Ordering::Acquire)
    }
    pub fn notifier(&self) -> async_channel::Receiver<()> {
        self.notify_rx.clone()
    }
    pub async fn stop(self: &Arc<Self>, pool: &Rc<TaskPool>) {
        let Some(pipeline) = self.lease.pipeline.as_ref().cloned() else {
            return;
        };
        loop {
            let pipeline = pipeline.clone();
            match pool.submit(move |_| {
                let _ = pipeline.set_state(gst::State::Null);
                Ok(())
            }) {
                Ok(task) => {
                    let _ = task.result().await;
                    return;
                }
                Err(TaskError::Busy) => gtk::glib::timeout_future(Duration::from_millis(20)).await,
                Err(_) => return,
            }
        }
    }
    pub async fn frame(
        self: &Arc<Self>,
        pool: &Rc<TaskPool>,
        cancel: &gio::Cancellable,
    ) -> Result<Arc<RgbaImage>, String> {
        self.request(pool, cancel, ReadMode::Cached).await
    }
    /// A single startup barrier for client redraws caused by taking HUD focus.
    /// Keep the newest pixels throughout; do not stall on continuously animated
    /// content or add any history/timestamp fence to normal live acquisition.
    pub async fn settled_frame(
        self: &Arc<Self>,
        pool: &Rc<TaskPool>,
        cancel: &gio::Cancellable,
    ) -> Result<Arc<RgbaImage>, String> {
        let mut previous = self.frame(pool, cancel).await?;
        let started = Instant::now();
        let mut unchanged = 0;
        while started.elapsed() < Duration::from_secs(2) {
            let current = self.frame(pool, cancel).await?;
            let (a, b) = (previous.clone(), current.clone());
            let task = pool
                .submit(move |token| {
                    token.check()?;
                    Ok(Arc::ptr_eq(&a, &b) || a == b)
                })
                .map_err(|e| e.to_string())?;
            let equal = gio::CancellableFuture::new(task.result(), cancel.clone())
                .await
                .map_err(|_| t("Operation cancelled"))?
                .map_err(|e| e.to_string())?;
            unchanged = if equal { unchanged + 1 } else { 0 };
            previous = current;
            if unchanged == 2 {
                break;
            }
        }
        Ok(previous)
    }
    pub fn pointer(&self) -> Option<[f64; 2]> {
        *self.pointer.lock().unwrap()
    }
    /// Input decisions must not reuse coordinates from cached image pixels.
    pub async fn pointer_frame(
        self: &Arc<Self>,
        pool: &Rc<TaskPool>,
        cancel: &gio::Cancellable,
    ) -> Result<Option<Arc<RgbaImage>>, String> {
        self.available(pool, cancel, ReadMode::Pointer).await
    }
    async fn available(
        self: &Arc<Self>,
        pool: &Rc<TaskPool>,
        cancel: &gio::Cancellable,
        mode: ReadMode,
    ) -> Result<Option<Arc<RgbaImage>>, String> {
        let frames = self.clone();
        let task = pool
            .submit(move |token| frames.read(&token, mode))
            .map_err(|e| e.to_string())?;
        match gio::CancellableFuture::new(task.result(), cancel.clone())
            .await
            .map_err(|_| t("Operation cancelled"))?
        {
            Ok(pixels) => Ok(Some(pixels)),
            Err(TaskError::Busy) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
    async fn request(
        self: &Arc<Self>,
        pool: &Rc<TaskPool>,
        cancel: &gio::Cancellable,
        mode: ReadMode,
    ) -> Result<Arc<RgbaImage>, String> {
        let frames = self.clone();
        let task = pool
            .submit(move |token| frames.read(&token, mode))
            .map_err(|e| e.to_string())?;
        gio::CancellableFuture::new(task.result(), cancel.clone())
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|e| e.to_string())
    }
    fn read(&self, token: &Cancellation, mode: ReadMode) -> Result<Arc<RgbaImage>, TaskError> {
        let started = Instant::now();
        loop {
            token.check()?;
            if let Some(error) = self
                .lease
                .pipeline
                .as_ref()
                .and_then(|p| p.bus())
                .and_then(|bus| bus.pop_filtered(&[gst::MessageType::Error]))
                && let gst::MessageView::Error(e) = error.view()
            {
                return Err(TaskError::Failed(e.error().to_string()));
            }
            if self.sink.is_eos() {
                return Err(TaskError::Failed(t("The system returned no image")));
            }
            let seen_before = self.available_seq.load(Ordering::Acquire);
            let sample = if matches!(mode, ReadMode::Cached) && self.last.lock().unwrap().is_some()
            {
                if !self.probe_attached {
                    self.sink.try_pull_sample(gst::ClockTime::from_mseconds(10))
                } else {
                    match self.sink.try_pull_sample(gst::ClockTime::ZERO) {
                        Some(sample) => Some(sample),
                        None if seen_before != self.consumed_seq.load(Ordering::Acquire) => {
                            self.sink.try_pull_sample(gst::ClockTime::from_mseconds(10))
                        }
                        None => None,
                    }
                }
            } else {
                self.sink.try_pull_sample(gst::ClockTime::from_mseconds(50))
            };
            if let Some(sample) = sample {
                self.consumed_seq.store(
                    seen_before.max(self.consumed_seq.load(Ordering::Acquire)),
                    Ordering::Release,
                );
                let caps = sample
                    .caps()
                    .ok_or_else(|| TaskError::Failed(t("The system returned no image")))?;
                let info = gst_video::VideoInfo::from_caps(caps)
                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                if info.width() == 0
                    || info.height() == 0
                    || u64::from(info.width()) * u64::from(info.height()) > image_ops::MAX_PIXELS
                {
                    return Err(TaskError::Failed(t(
                        "The image exceeds 48 megapixels or has invalid dimensions",
                    )));
                }
                let buffer = sample
                    .buffer()
                    .ok_or_else(|| TaskError::Failed(t("The system returned no image")))?;
                let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                // PipeWire maps SPA_META_VideoCrop to this standard metadata.
                // Window buffers can be monitor-sized while their valid region
                // is much smaller. Never infer the crop from logical portal size.
                let (x, y, w, h) = buffer
                    .meta::<gst_video::VideoCropMeta>()
                    .map(|m| m.rect())
                    .unwrap_or((0, 0, info.width(), info.height()));
                if w == 0
                    || h == 0
                    || x.checked_add(w).is_none_or(|r| r > info.width())
                    || y.checked_add(h).is_none_or(|r| r > info.height())
                {
                    return Err(TaskError::Failed(t("The system returned no image")));
                }
                *self.pointer.lock().unwrap() = buffer
                    .iter_meta::<gst_video::VideoRegionOfInterestMeta>()
                    .find(|m| m.roi_type() == "cursor")
                    .map(|m| m.rect())
                    .filter(|(px, py, _, _)| *px >= x && *py >= y && *px - x < w && *py - y < h)
                    .map(|(px, py, _, _)| [(px - x) as f64, (py - y) as f64]);
                let data = frame
                    .plane_data(0)
                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                let stride = frame.plane_stride()[0];
                let width = w as usize * 4;
                let offset = x as usize * 4;
                if stride < (offset + width) as i32 {
                    return Err(TaskError::Failed(t("The system returned no image")));
                }
                let mut pixels = Vec::with_capacity(width * h as usize);
                for row in data
                    .chunks(stride as usize)
                    .skip(y as usize)
                    .take(h as usize)
                {
                    token.check()?;
                    pixels.extend_from_slice(
                        row.get(offset..offset + width)
                            .ok_or_else(|| TaskError::Failed(t("The system returned no image")))?,
                    );
                }
                let image = Arc::new(
                    RgbaImage::from_raw(w, h, pixels)
                        .ok_or_else(|| TaskError::Failed(t("The system returned no image")))?,
                );
                self.last.lock().unwrap().replace(image.clone());
                return Ok(image);
            }
            if matches!(mode, ReadMode::Pointer) {
                return Err(TaskError::Busy);
            }
            if matches!(mode, ReadMode::Cached)
                && let Some(last) = self.last.lock().unwrap().clone()
            {
                return Ok(last);
            }
            if started.elapsed() > Duration::from_secs(15) {
                return Err(TaskError::Failed(t("The capture stream timed out")));
            }
        }
    }
}
