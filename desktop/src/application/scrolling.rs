//! Long-capture orchestration: persistent streams feed one core document.
//! Only explicit auto-scroll upgrades to a pointer-authorized portal session.
mod acquisition;
mod completion;
pub use crate::ui::scroll_capture::Status;
use crate::{
    infrastructure::{
        cancellation::Scope,
        tasks::{TaskError, TaskPool},
    },
    platform::capture::{
        region::{PointerState, RegionSource},
        session::Session,
    },
    ui::{
        scroll_capture::{Appearance, Command, Controls},
        toolbar::Output,
    },
};
use clivuu_core::{
    annotation::Bounds,
    i18n::t,
    image::RgbaImage,
    scrolling::{Axis, Document},
};
use gtk::{gdk, gio, glib, prelude::*};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
const AUTO_SAMPLE_MS: u64 = 40;
const MANUAL_MIN_SAMPLE_MS: u64 = 16;
const MANUAL_FALLBACK_POLL_MS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoStep {
    Continue,
    ReachedEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoMismatch {
    Retry,
    ReachedEnd,
    Failed,
}

#[derive(Debug, Clone)]
struct AutoScrollDriver {
    smooth_delta: f64,
    discrete_fallback: bool,
    smooth_proven: bool,
    zero_motion_ticks: u8,
    consecutive_errors: u8,
}

impl AutoScrollDriver {
    fn new(rect: Bounds, axis: Axis) -> Self {
        let span = Self::axis_span(rect, axis);
        let target = Self::target_logical(span);
        Self {
            // In Wayland/Mutter, 10.0 axis units equal one wheel detent (~120–240 px
            // in browsers on 1x–2x DPI), while 1:1 GTK surfaces use 1.0 unit/px.
            // Starting at target/16 keeps even 24x-amplified tiny selections well
            // inside overlap on the first pulse before closed-loop calibration.
            smooth_delta: (target / 16.0).clamp(0.45, 3.2),
            discrete_fallback: false,
            smooth_proven: false,
            zero_motion_ticks: 0,
            consecutive_errors: 0,
        }
    }

    fn axis_span(rect: Bounds, axis: Axis) -> f64 {
        if axis == Axis::Vertical {
            rect.height
        } else {
            rect.width
        }
        .max(48.0)
    }

    fn target_logical(span: f64) -> f64 {
        (span * 0.22).clamp(10.0, 96.0)
    }

    fn pulse_interval(rect: Bounds, axis: Axis, base_ms: u64) -> Duration {
        let base = base_ms.clamp(150, 5000);
        Duration::from_millis(if Self::axis_span(rect, axis) < 220.0 {
            (base / 2).clamp(150, 320)
        } else {
            base
        })
    }

    async fn inject(&mut self, session: &Session, axis: Axis) -> Result<(), String> {
        let code = if axis == Axis::Vertical { 0 } else { 1 };
        if !self.discrete_fallback {
            let (dx, dy) = if axis == Axis::Vertical {
                (0.0, self.smooth_delta)
            } else {
                (self.smooth_delta, 0.0)
            };
            match session.scroll_smooth(dx, dy).await {
                Ok(()) => return Ok(()),
                Err(_) => self.discrete_fallback = true,
            }
        }
        session.scroll(code, 1).await
    }

    fn observe_step(&mut self, observed_logical: f64, rect: Bounds, axis: Axis) -> AutoStep {
        self.consecutive_errors = 0;
        let span = Self::axis_span(rect, axis);
        let target = Self::target_logical(span);
        if observed_logical >= 1.0 {
            self.zero_motion_ticks = 0;
            if !self.discrete_fallback {
                self.smooth_proven = true;
                let ratio = (target / observed_logical).clamp(0.25, 3.2);
                let damped = 1.0 + (ratio - 1.0) * 0.85;
                self.smooth_delta = (self.smooth_delta * damped).clamp(0.2, 120.0);
            }
            AutoStep::Continue
        } else {
            self.zero_motion_ticks = self.zero_motion_ticks.saturating_add(1);
            if self.smooth_proven {
                if self.zero_motion_ticks >= 2 {
                    AutoStep::ReachedEnd
                } else {
                    AutoStep::Continue
                }
            } else if !self.discrete_fallback {
                if self.zero_motion_ticks <= 3 {
                    self.smooth_delta = (self.smooth_delta * 2.2).min(target);
                    AutoStep::Continue
                } else if span >= 120.0 {
                    self.discrete_fallback = true;
                    AutoStep::Continue
                } else {
                    AutoStep::ReachedEnd
                }
            } else if self.zero_motion_ticks >= 6 {
                AutoStep::ReachedEnd
            } else {
                AutoStep::Continue
            }
        }
    }

    fn observe_mismatch(&mut self) -> AutoMismatch {
        if self.zero_motion_ticks >= 1 {
            return AutoMismatch::ReachedEnd;
        }
        self.consecutive_errors = self.consecutive_errors.saturating_add(1);
        self.smooth_delta = (self.smooth_delta * 0.5).max(0.25);
        if self.consecutive_errors < 3 {
            AutoMismatch::Retry
        } else {
            AutoMismatch::Failed
        }
    }
}

pub struct Captured {
    pub pixels: Arc<RgbaImage>,
    pub rect: Bounds,
    pub action: Output,
}
async fn open(
    connection: &gio::DBusConnection,
    pool: &Rc<TaskPool>,
    rect: Bounds,
    outputs: &[clivuu_core::capture::Output],
    cancel: &gio::Cancellable,
    pointer: bool,
) -> Result<RegionSource, String> {
    let mut session = if pointer {
        Session::automatic(connection, cancel).await?
    } else if has_mutter(connection).await {
        Session::mutter(connection, cancel, rect, false).await?
    } else {
        Session::portal(connection, cancel, false, 1, false, false).await?
    };
    session.fallback_single_monitor(outputs);
    RegionSource::open(session, rect, pool).await
}
async fn close(source: &mut Option<RegionSource>, pool: &Rc<TaskPool>) {
    if let Some(source) = source.take() {
        source.close(pool).await;
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Coordinates one document with independent platform and presentation owners"
)]
pub async fn run(
    connection: &gio::DBusConnection,
    display: &gdk::Display,
    pool: Rc<TaskPool>,
    initial: Bounds,
    status: Status,
    appearance: super::capture::Options,
    max_frames: usize,
    interval: u64,
    initial_auto_crop: bool,
    cancel: &gio::Cancellable,
) -> Result<Option<Captured>, String> {
    let scope = Scope::new(cancel);
    let token = &scope.token;
    let mut rect = initial;
    let mut document_rect = initial;
    let fallback_outputs = if crate::platform::display::is_gnome() {
        Vec::new()
    } else {
        crate::platform::surfaces::capture::gdk_outputs(display)
    };
    let mut axis = Axis::Vertical;
    let mut auto_crop = initial_auto_crop;
    let mut initial_doc = Document::new(axis, max_frames);
    initial_doc.set_auto_crop(auto_crop);
    let mut document = Arc::new(Mutex::new(initial_doc));
    let (tx, rx) = async_channel::bounded(16);
    let aborted = token.clone();
    let user_cancel = token.clone();
    let live = appearance.live.clone();
    let tx_config = tx.clone();
    let controls = Controls::new(
        display,
        rect,
        Appearance {
            dark: appearance.dark,
            accent: appearance.accent,
            display: appearance.display,
        },
        status,
        move || aborted.cancel(),
        move |command| {
            if matches!(command, Command::Cancel) {
                user_cancel.cancel();
            } else {
                let _ = tx.try_send(command);
            }
        },
    )?;
    let _appearance = live.as_ref().map(|config| {
        let controls = controls.clone();
        let observed_crop = Cell::new(initial_auto_crop);
        config.subscribe(move |values| {
            let options = super::service::options(values);
            controls.set_appearance(Appearance {
                dark: options.dark,
                accent: options.accent,
                display: options.display,
            });
            let configured_crop = values
                .get("scroll-auto-crop")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if observed_crop.replace(configured_crop) != configured_crop {
                let _ = tx_config.try_send(Command::AutoCrop(configured_crop));
            }
        })
    });
    // Establish the HUD's focus before the first still, but keep all its
    // pixels transparent. The first and live frames then share activation
    // appearance without losing the complete initial viewport to HUD coverage.
    let mut source = None;
    let initial = async {
        controls.sampling(true);
        gio::CancellableFuture::new(controls.prepare_start(), token.clone())
            .await
            .map_err(|_| t("Operation cancelled"))??;
        // Session creation must receive its owned D-Bus path even when
        // cancellation races the reply, so its own cancellation cleanup
        // can Stop the server-side session. Never drop this open future.
        source = Some(open(connection, &pool, rect, &fallback_outputs, token, false).await?);
        gio::CancellableFuture::new(
            async {
                let pixels = source
                    .as_ref()
                    .unwrap()
                    .initial_image(rect, &pool, token)
                    .await?;
                let (progress, preview, _) =
                    acquisition::append(&document, pixels, rect, vec![], &pool)
                        .await
                        .map_err(|e| e.message)?;
                controls.sampling(false);
                if let Some(preview) = preview {
                    controls.preview(preview, progress);
                }
                controls.state(progress, axis, true, false, auto_crop);
                controls.present_changes().await?;
                Ok::<_, String>(progress)
            },
            token.clone(),
        )
        .await
        .map_err(|_| t("Operation cancelled"))?
    }
    .await;
    let mut progress = match initial {
        Ok(progress) => progress,
        Err(error) => {
            close(&mut source, &pool).await;
            controls.dismiss();
            return if token.is_cancelled() {
                Ok(None)
            } else {
                Err(error)
            };
        }
    };
    let result = gio::CancellableFuture::new(
        async {
            let mut capturing = true;
            let mut automatic = false;
            let mut auto_driver = AutoScrollDriver::new(rect, axis);
            let mut last_inject: Option<Instant> = None;
            let mut pulse_step_px: u32 = 0;
            let mut last_pointer: Option<[f64; 2]> = None;
            let mut last_sampled: Option<(Arc<RgbaImage>, Vec<Bounds>)> = None;
            let now = Instant::now();
            let mut next_auto = now + Duration::from_millis(AUTO_SAMPLE_MS);
            let mut next_min = now + Duration::from_millis(MANUAL_MIN_SAMPLE_MS);
            let mut next_fallback = now + Duration::from_millis(MANUAL_FALLBACK_POLL_MS);
            loop {
                if source
                    .as_ref()
                    .is_some_and(|s| s.session.scope.token.is_cancelled())
                {
                    // Source permission can end independently of the operation.
                    // Retain committed frames and leave export/cancel responsive.
                    capturing = false;
                    automatic = false;
                    last_inject = None;
                    pulse_step_px = 0;
                    last_pointer = None;
                    last_sampled = None;
                    close(&mut source, &pool).await;
                    controls.auto_waiting(false);
                    controls.message(&t("Operation cancelled"));
                    controls.state(progress, axis, capturing, automatic, auto_crop);
                }
                let command = if let Ok(command) = rx.try_recv() {
                    command
                } else if capturing && controls.sampling_allowed() && {
                    let now = Instant::now();
                    if automatic {
                        now >= next_auto
                    } else {
                        now >= next_min
                            && (source.is_none()
                                || last_sampled.is_none()
                                || source.as_ref().is_some_and(|s| s.has_pending_frame())
                                || now >= next_fallback)
                    }
                } {
                    Command::Sample
                } else {
                    let now = Instant::now();
                    let deadline = if !capturing || !controls.sampling_allowed() {
                        source
                            .as_ref()
                            .map(|_| now + Duration::from_millis(MANUAL_FALLBACK_POLL_MS))
                    } else if automatic {
                        Some(next_auto)
                    } else if now < next_min {
                        Some(next_min)
                    } else {
                        Some(next_fallback)
                    };
                    let notifier =
                        (!automatic && capturing && controls.sampling_allowed() && now >= next_min)
                            .then(|| source.as_ref().map(|s| s.notifier()))
                            .flatten();
                    let mut recv_cmd = std::pin::pin!(rx.recv());
                    let mut recv_frame = notifier.map(|n| Box::pin(async move { n.recv().await }));
                    let mut timer = deadline
                        .map(|d| Box::pin(glib::timeout_future(d.saturating_duration_since(now))));
                    let received = std::future::poll_fn(|cx| {
                        use std::task::Poll;
                        if let Poll::Ready(res) = recv_cmd.as_mut().poll(cx) {
                            return Poll::Ready(res.ok());
                        }
                        if let Some(frame_fut) = recv_frame.as_mut()
                            && let Poll::Ready(_) = frame_fut.as_mut().poll(cx)
                        {
                            return Poll::Ready(None);
                        }
                        if let Some(timer_fut) = timer.as_mut()
                            && let Poll::Ready(_) = timer_fut.as_mut().poll(cx)
                        {
                            return Poll::Ready(None);
                        }
                        Poll::Pending
                    })
                    .await;
                    if let Some(command) = received {
                        command
                    } else {
                        continue;
                    }
                };
                match command {
                    Command::Cancel => return Ok(None),
                    Command::Output(action) => {
                        automatic = false;
                        last_inject = None;
                        pulse_step_px = 0;
                        last_pointer = None;
                        capturing = false;
                        // Export the committed document first. A complete capture must
                        // not depend on a new portal/reader, paint timing or a further
                        // successful match. Repair a masked viewport only when needed.
                        let final_image = completion::image(
                            connection,
                            &pool,
                            &document,
                            &controls,
                            &fallback_outputs,
                            &mut source,
                            token,
                        )
                        .await;
                        close(&mut source, &pool).await;
                        match final_image {
                            Ok(pixels) => {
                                controls.dismiss();
                                return Ok(Some(Captured {
                                    pixels,
                                    rect: document_rect,
                                    action,
                                }));
                            }
                            Err(error) => {
                                controls.sampling(false);
                                controls.message(&error);
                            }
                        }
                    }
                    Command::AutoCrop(enabled) => {
                        auto_crop = enabled;
                        document.lock().unwrap().set_auto_crop(enabled);
                        if let Some(config) = live.as_ref()
                            && config
                                .values()
                                .get("scroll-auto-crop")
                                .and_then(|v| v.as_bool())
                                != Some(enabled)
                        {
                            let _ =
                                config.set("scroll-auto-crop", serde_json::Value::Bool(enabled));
                        }
                    }
                    Command::ToolbarMoved => {
                        // A moving HUD invalidates the reader's cached frame, not
                        // the user's image or authorization/automatic-scroll state.
                        last_sampled = None;
                        let refreshed = async {
                            controls.present_changes().await?;
                            if let Some(reader) = source.as_mut() {
                                reader.refresh(&pool).await?;
                            }
                            Ok::<_, String>(())
                        }
                        .await;
                        controls.clear_closing_popup_exclusion();
                        if let Err(error) = refreshed {
                            capturing = false;
                            automatic = false;
                            last_inject = None;
                            pulse_step_px = 0;
                            last_pointer = None;
                            close(&mut source, &pool).await;
                            controls.auto_waiting(false);
                            controls.message(&error);
                        }
                        let now = Instant::now();
                        next_auto = now + Duration::from_millis(AUTO_SAMPLE_MS);
                        next_min = now + Duration::from_millis(MANUAL_MIN_SAMPLE_MS);
                        next_fallback = now + Duration::from_millis(MANUAL_FALLBACK_POLL_MS);
                    }
                    Command::Axis(value) => {
                        if value == axis {
                            continue;
                        }
                        close(&mut source, &pool).await;
                        automatic = false;
                        last_inject = None;
                        pulse_step_px = 0;
                        last_pointer = None;
                        controls.auto_waiting(false);
                        axis = value;
                        auto_driver = AutoScrollDriver::new(rect, axis);
                        last_sampled = None;
                        capturing = true;
                        let mut next_doc = Document::new(axis, max_frames);
                        next_doc.set_auto_crop(auto_crop);
                        progress = next_doc.progress(false);
                        document = Arc::new(Mutex::new(next_doc));
                        document_rect = rect;
                        controls.message("");
                        let now = Instant::now();
                        next_auto = now + Duration::from_millis(AUTO_SAMPLE_MS);
                        next_min = now + Duration::from_millis(MANUAL_MIN_SAMPLE_MS);
                        next_fallback = now + Duration::from_millis(MANUAL_FALLBACK_POLL_MS);
                    }
                    Command::Region(region) => {
                        close(&mut source, &pool).await;
                        automatic = false;
                        last_inject = None;
                        pulse_step_px = 0;
                        last_pointer = None;
                        controls.auto_waiting(false);
                        rect = region;
                        auto_driver = AutoScrollDriver::new(rect, axis);
                        last_sampled = None;
                        capturing = true;
                        let now = Instant::now();
                        next_auto = now + Duration::from_millis(AUTO_SAMPLE_MS);
                        next_min = now + Duration::from_millis(MANUAL_MIN_SAMPLE_MS);
                        next_fallback = now + Duration::from_millis(MANUAL_FALLBACK_POLL_MS);
                    }
                    Command::Reanchor => {
                        if automatic {
                            automatic = false;
                            last_inject = None;
                            pulse_step_px = 0;
                            last_pointer = None;
                            controls.auto_waiting(false);
                            close(&mut source, &pool).await;
                        }
                        capturing = true;
                        auto_driver = AutoScrollDriver::new(rect, axis);
                        last_sampled = None;
                        let reanchored = async {
                            if source.is_none() {
                                controls.present_changes().await?;
                                source = Some(
                                    open(connection, &pool, rect, &fallback_outputs, token, false)
                                        .await?,
                                );
                                document_rect = rect;
                            }
                            let pixels = source.as_ref().unwrap().image(rect, &pool, token).await?;
                            let exclusions = controls.exclusions();
                            let (updated, preview) = acquisition::reanchor(
                                &document,
                                pixels.clone(),
                                rect,
                                exclusions.clone(),
                                &pool,
                            )
                            .await
                            .map_err(|e| e.message)?;
                            last_sampled = Some((pixels, exclusions));
                            Ok::<_, String>((updated, preview))
                        }
                        .await;
                        match reanchored {
                            Ok((updated, preview)) => {
                                progress = updated;
                                if let Some(preview) = preview {
                                    controls.preview(preview, progress);
                                }
                                controls.message("");
                            }
                            Err(error) => controls.message(&error),
                        }
                        let now = Instant::now();
                        next_auto = now + Duration::from_millis(AUTO_SAMPLE_MS);
                        next_min = now + Duration::from_millis(MANUAL_MIN_SAMPLE_MS);
                        next_fallback = now + Duration::from_millis(MANUAL_FALLBACK_POLL_MS);
                    }
                    Command::Reset => {
                        if automatic
                            || source
                                .as_ref()
                                .is_some_and(|reader| reader.session.can_scroll())
                        {
                            close(&mut source, &pool).await;
                        }
                        automatic = false;
                        last_inject = None;
                        pulse_step_px = 0;
                        last_pointer = None;
                        controls.auto_waiting(false);
                        auto_driver = AutoScrollDriver::new(rect, axis);
                        last_sampled = None;
                        capturing = true;
                        controls.message("");
                        let mut next_doc = Document::new(axis, max_frames);
                        next_doc.set_auto_crop(auto_crop);
                        let mut hid_hud = false;
                        let restarted = async {
                            hid_hud = controls.conceal_overlapping_hud(rect).await?;
                            let pixels = if let Some(reader) = source.as_mut()
                                && reader.covers(rect)
                            {
                                if hid_hud {
                                    let clean = reader.clean_image(rect, &pool, token).await?;
                                    reader.refresh(&pool).await?;
                                    clean
                                } else {
                                    let rx_n = reader.notifier();
                                    while rx_n.try_recv().is_ok() {}
                                    reader.image(rect, &pool, token).await?
                                }
                            } else {
                                close(&mut source, &pool).await;
                                let reader =
                                    open(connection, &pool, rect, &fallback_outputs, token, false)
                                        .await?;
                                let initial = reader.initial_image(rect, &pool, token).await?;
                                source = Some(reader);
                                initial
                            };
                            let masks = if hid_hud {
                                vec![]
                            } else {
                                controls.exclusions()
                            };
                            let next_arc = Arc::new(Mutex::new(next_doc));
                            let (updated, preview, _) =
                                acquisition::append(&next_arc, pixels, rect, masks, &pool)
                                    .await
                                    .map_err(|e| e.message)?;
                            Ok::<_, String>((next_arc, updated, preview))
                        }
                        .await;
                        if hid_hud {
                            controls.restore_hud();
                        }
                        match restarted {
                            Ok((next_arc, updated, preview)) => {
                                document = next_arc;
                                document_rect = rect;
                                progress = updated;
                                if let Some(preview) = preview {
                                    controls.preview(preview, progress);
                                }
                                controls.message("");
                            }
                            Err(error) => {
                                let mut empty_doc = Document::new(axis, max_frames);
                                empty_doc.set_auto_crop(auto_crop);
                                progress = empty_doc.progress(false);
                                document = Arc::new(Mutex::new(empty_doc));
                                document_rect = rect;
                                capturing = false;
                                close(&mut source, &pool).await;
                                controls.message(&error);
                            }
                        }
                        if hid_hud {
                            let _ = controls.present_changes().await;
                        }
                        let now = Instant::now();
                        next_auto = now + Duration::from_millis(AUTO_SAMPLE_MS);
                        next_min = now + Duration::from_millis(MANUAL_MIN_SAMPLE_MS);
                        next_fallback = now + Duration::from_millis(MANUAL_FALLBACK_POLL_MS);
                    }
                    Command::TrimStart | Command::TrimEnd => {
                        let is_start = matches!(command, Command::TrimStart);
                        last_sampled = None;
                        let document = document.clone();
                        let mut hid_hud = false;
                        let cropped = async {
                            // The page may have moved after the last tick (or
                            // while input interrupted an injected scroll).
                            // Crop the visible viewport, never a stale preview.
                            if let Some(reader) = source.as_ref() {
                                let epoch = controls.input_epoch();
                                if !controls.sampling_allowed() {
                                    return Err(t("Operation cancelled"));
                                }
                                let rx_n = reader.notifier();
                                while rx_n.try_recv().is_ok() {}
                                let pixels = reader.image(rect, &pool, token).await?;
                                if !controls.sampling_allowed() || controls.input_epoch() != epoch {
                                    return Err(t("Operation cancelled"));
                                }
                                let (updated, preview, _) = acquisition::append(
                                    &document,
                                    pixels,
                                    rect,
                                    controls.exclusions(),
                                    &pool,
                                )
                                .await
                                .map_err(|error| error.message)?;
                                progress = updated;
                                last_inject = None;
                                pulse_step_px = 0;
                                if let Some(preview) = preview {
                                    controls.preview(preview, progress);
                                }
                            }
                            let (can_crop, need_clean) = {
                                let doc = document.lock().unwrap();
                                (
                                    doc.can_crop_to_view(is_start),
                                    doc.crop_needs_clean_anchor(is_start),
                                )
                            };
                            if !can_crop {
                                return Ok(None);
                            }
                            if need_clean
                                && let Some(reader) = source.as_mut()
                                && reader.covers(rect)
                            {
                                hid_hud = controls.conceal_overlapping_hud(rect).await?;
                                let clean = if hid_hud {
                                    let clean = reader.clean_image(rect, &pool, token).await?;
                                    reader.refresh(&pool).await?;
                                    clean
                                } else {
                                    let rx_n = reader.notifier();
                                    while rx_n.try_recv().is_ok() {}
                                    reader.image(rect, &pool, token).await?
                                };
                                controls.restore_hud();
                                let masks = if hid_hud {
                                    vec![]
                                } else {
                                    controls.exclusions()
                                };
                                let (updated, _, _) =
                                    acquisition::append(&document, clean, rect, masks, &pool)
                                        .await
                                        .map_err(|error| error.message)?;
                                progress = updated;
                            }
                            pool.submit(move |_| {
                                let mut document = document.lock().unwrap();
                                document
                                    .crop_to_view(is_start)
                                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                                Ok((
                                    document.progress(false),
                                    Arc::new(
                                        document
                                            .preview()
                                            .map_err(|e| TaskError::Failed(e.to_string()))?,
                                    ),
                                ))
                            })
                            .map_err(|e| e.to_string())?
                            .result()
                            .await
                            .map_err(|e| e.to_string())
                            .map(Some)
                        }
                        .await;
                        if hid_hud {
                            controls.restore_hud();
                            let _ = controls.present_changes().await;
                        }
                        match cropped {
                            Ok(Some((updated, preview))) => {
                                progress = updated;
                                controls.preview(preview, progress);
                                controls.state(progress, axis, capturing, automatic, auto_crop);
                                controls.message("");
                            }
                            Ok(None) => {
                                controls.state(progress, axis, capturing, automatic, auto_crop);
                            }
                            Err(error) => controls.message(&error.to_string()),
                        }
                    }
                    Command::Auto => {
                        automatic = !automatic;
                        last_sampled = None;
                        last_inject = None;
                        pulse_step_px = 0;
                        last_pointer = None;
                        if automatic {
                            auto_driver = AutoScrollDriver::new(rect, axis);
                            // Permission UI appears only after this explicit action.
                            controls.sampling(true);
                            let authorized = Session::automatic(connection, token).await;
                            controls.sampling(false);
                            let candidate = async {
                                let mut session = authorized?;
                                session.fallback_single_monitor(&fallback_outputs);
                                controls.state(progress, axis, true, true, auto_crop);
                                controls.prepare_start().await?;
                                RegionSource::open(session, rect, &pool).await
                            }
                            .await;
                            match candidate {
                                Ok(candidate) => {
                                    close(&mut source, &pool).await;
                                    document_rect = rect;
                                    source = Some(candidate);
                                    capturing = true;
                                    controls.message("");
                                }
                                Err(error) => {
                                    automatic = false;
                                    controls.message(&error);
                                }
                            }
                        } else {
                            controls.auto_waiting(false);
                            // Harvest any already-rendered frame from the remote-input
                            // stream before closing it so manual stop never loses an
                            // intermediate scroll frame across the stream handoff.
                            if let Some(reader) = source.as_ref()
                                && controls.sampling_allowed()
                                && let Ok(pixels) = reader.image(rect, &pool, token).await
                                && let Ok((updated, preview, _)) = acquisition::append(
                                    &document,
                                    pixels,
                                    rect,
                                    controls.exclusions(),
                                    &pool,
                                )
                                .await
                            {
                                progress = updated;
                                if let Some(preview) = preview {
                                    controls.preview(preview, progress);
                                }
                            }
                            close(&mut source, &pool).await;
                        }
                        let now = Instant::now();
                        next_auto = now;
                        next_min = now;
                        next_fallback = now;
                    }
                    Command::Sample => {
                        let now = Instant::now();
                        next_auto = now + Duration::from_millis(AUTO_SAMPLE_MS);
                        next_min = now + Duration::from_millis(MANUAL_MIN_SAMPLE_MS);
                        next_fallback = now + Duration::from_millis(MANUAL_FALLBACK_POLL_MS);
                        if !capturing || !controls.sampling_allowed() {
                            continue;
                        }
                        let epoch = controls.input_epoch();
                        if source.is_none() {
                            let candidate = async {
                                controls.present_changes().await?;
                                open(connection, &pool, rect, &fallback_outputs, token, false).await
                            }
                            .await;
                            match candidate {
                                Ok(candidate) => {
                                    document_rect = rect;
                                    last_sampled = None;
                                    source = Some(candidate);
                                }
                                Err(error) => {
                                    capturing = false;
                                    controls.message(&error);
                                    controls.state(progress, axis, capturing, automatic, auto_crop);
                                    continue;
                                }
                            }
                        }
                        if !controls.sampling_allowed() || controls.input_epoch() != epoch {
                            continue;
                        }
                        let mut auto_allowed = false;
                        if automatic {
                            match source.as_ref().unwrap().pointer_state(&pool).await {
                                Ok(PointerState::At(p)) => last_pointer = Some(p),
                                Ok(PointerState::LeftMonitor) => last_pointer = None,
                                Ok(PointerState::Unchanged) => {}
                                Err(error) => {
                                    capturing = false;
                                    automatic = false;
                                    last_inject = None;
                                    pulse_step_px = 0;
                                    last_pointer = None;
                                    last_sampled = None;
                                    close(&mut source, &pool).await;
                                    controls.auto_waiting(false);
                                    controls.message(&error);
                                    controls.state(progress, axis, capturing, automatic, auto_crop);
                                    continue;
                                }
                            }
                            auto_allowed = last_pointer.is_some_and(|p| controls.accepts_scroll(p));
                            controls.auto_waiting(!auto_allowed);
                            if !auto_allowed && last_inject.is_none() {
                                continue;
                            }
                        } else {
                            controls.auto_waiting(false);
                        }
                        if !controls.sampling_allowed() || controls.input_epoch() != epoch {
                            continue;
                        }
                        // Permission/readiness failures belong to the source, not
                        // the document. This stage never drops committed frames.
                        let should_read_frame = !automatic || last_inject.is_some();
                        if should_read_frame {
                            let rx_n = source.as_ref().unwrap().notifier();
                            while rx_n.try_recv().is_ok() {}
                            let pixels = source.as_ref().unwrap().image(rect, &pool, token).await;
                            if !controls.sampling_allowed() || controls.input_epoch() != epoch {
                                continue;
                            }
                            let pixels = match pixels {
                                Ok(pixels) => pixels,
                                Err(error) => {
                                    capturing = false;
                                    automatic = false;
                                    last_inject = None;
                                    pulse_step_px = 0;
                                    last_pointer = None;
                                    last_sampled = None;
                                    close(&mut source, &pool).await;
                                    controls.auto_waiting(false);
                                    controls.message(&error);
                                    controls.state(progress, axis, capturing, automatic, auto_crop);
                                    continue;
                                }
                            };
                            let exclusions = controls.exclusions();
                            let unchanged_buffer = last_sampled.as_ref().is_some_and(
                                |(prev_pixels, prev_exclusions)| {
                                    Arc::ptr_eq(&pixels, prev_pixels)
                                        && *prev_exclusions == exclusions
                                },
                            );
                            let pixel_span = if axis == Axis::Vertical {
                                pixels.height()
                            } else {
                                pixels.width()
                            }
                            .max(1) as f64;
                            if !unchanged_buffer {
                                let sampled = acquisition::append(
                                    &document,
                                    pixels.clone(),
                                    rect,
                                    exclusions.clone(),
                                    &pool,
                                )
                                .await;
                                match sampled {
                                    Ok((updated, preview, step_px)) => {
                                        last_sampled = Some((pixels, exclusions));
                                        if automatic && last_inject.is_some() {
                                            pulse_step_px = pulse_step_px.saturating_add(step_px);
                                            auto_driver.consecutive_errors = 0;
                                        }
                                        progress = updated;
                                        if let Some(preview) = preview {
                                            controls.preview(preview, progress);
                                        }
                                        controls.message("");
                                    }
                                    Err(error) => {
                                        last_sampled = None;
                                        if error.capacity {
                                            automatic = false;
                                            capturing = false;
                                            last_inject = None;
                                            pulse_step_px = 0;
                                            last_pointer = None;
                                            close(&mut source, &pool).await;
                                            controls.auto_waiting(false);
                                            controls.message(&error.message);
                                        } else if automatic {
                                            match auto_driver.observe_mismatch() {
                                                AutoMismatch::Retry => {
                                                    // Sample passively without injecting further scroll
                                                    // while an animation/compositor transition settles.
                                                    next_auto =
                                                        Instant::now() + Duration::from_millis(90);
                                                    controls.state(
                                                        progress, axis, capturing, automatic,
                                                        auto_crop,
                                                    );
                                                    continue;
                                                }
                                                AutoMismatch::ReachedEnd => {
                                                    automatic = false;
                                                    last_inject = None;
                                                    pulse_step_px = 0;
                                                    last_pointer = None;
                                                    close(&mut source, &pool).await;
                                                    controls.auto_waiting(false);
                                                    controls.message("");
                                                }
                                                AutoMismatch::Failed => {
                                                    automatic = false;
                                                    last_inject = None;
                                                    pulse_step_px = 0;
                                                    last_pointer = None;
                                                    close(&mut source, &pool).await;
                                                    controls.auto_waiting(false);
                                                    controls.message(&error.message);
                                                }
                                            }
                                        } else {
                                            controls.message(&error.message);
                                        }
                                    }
                                }
                            }
                            if automatic
                                && let Some(t_inject) = last_inject
                                && t_inject.elapsed()
                                    >= AutoScrollDriver::pulse_interval(rect, axis, interval)
                            {
                                let logical_step = pulse_step_px as f64
                                    * AutoScrollDriver::axis_span(rect, axis)
                                    / pixel_span;
                                last_inject = None;
                                pulse_step_px = 0;
                                if (auto_allowed || logical_step >= 1.0)
                                    && auto_driver.observe_step(logical_step, rect, axis)
                                        == AutoStep::ReachedEnd
                                {
                                    automatic = false;
                                    last_pointer = None;
                                    close(&mut source, &pool).await;
                                    controls.auto_waiting(false);
                                    controls.message("");
                                }
                            }
                        }
                        if automatic
                            && auto_allowed
                            && last_inject.is_none()
                            && auto_driver.consecutive_errors == 0
                            && let Some(reader) = source.as_ref()
                        {
                            if let Err(error) = auto_driver.inject(&reader.session, axis).await {
                                capturing = false;
                                automatic = false;
                                last_pointer = None;
                                last_sampled = None;
                                close(&mut source, &pool).await;
                                controls.auto_waiting(false);
                                controls.message(&error);
                            } else {
                                last_inject = Some(Instant::now());
                                pulse_step_px = 0;
                            }
                        }
                        let now = Instant::now();
                        next_auto = now + Duration::from_millis(AUTO_SAMPLE_MS);
                        next_min = now + Duration::from_millis(MANUAL_MIN_SAMPLE_MS);
                        next_fallback = now + Duration::from_millis(MANUAL_FALLBACK_POLL_MS);
                    }
                }
                controls.state(progress, axis, capturing, automatic, auto_crop);
            }
        },
        token.clone(),
    )
    .await;
    drop(controls);
    close(&mut source, &pool).await;
    result.unwrap_or(Ok(None))
}
pub async fn has_mutter(connection: &gio::DBusConnection) -> bool {
    if !crate::platform::display::is_gnome() {
        return false;
    }
    connection
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&("org.gnome.Mutter.ScreenCast",).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            3000,
        )
        .await
        .ok()
        .and_then(|v| v.get::<(bool,)>())
        .is_some_and(|v| v.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_scroll_scales_proportionally_for_small_and_amplified_selections() {
        for span in [48.0, 72.0, 140.0, 470.0] {
            let rect = Bounds {
                x: 0.0,
                y: 0.0,
                width: 300.0,
                height: span,
            };
            let target = AutoScrollDriver::target_logical(span);
            // 1:1 Wayland surface-pixel client converges smoothly to ~22% of selection height.
            let mut wayland = AutoScrollDriver::new(rect, Axis::Vertical);
            assert!(wayland.smooth_delta < span * 0.25);
            let mut subpixel = 0.0;
            for _ in 0..8 {
                subpixel += wayland.smooth_delta;
                let observed = if subpixel >= 1.0 {
                    let px = subpixel.floor();
                    subpixel -= px;
                    px
                } else {
                    0.0
                };
                assert!(observed < span * 0.5, "span={span} overshot: {observed}");
                assert_eq!(
                    wayland.observe_step(observed, rect, Axis::Vertical),
                    AutoStep::Continue
                );
            }
            assert!((wayland.smooth_delta - target).abs() < 3.0);

            // Both 12x (100% browser) and 24x (HiDPI browser) clients stay well inside
            // overlap on the very first pulse even for a tiny 48px selection.
            for gain in [12.0, 24.0] {
                let mut amplified = AutoScrollDriver::new(rect, Axis::Vertical);
                let first_jump_logical = amplified.smooth_delta * 12.0;
                assert!(
                    first_jump_logical < span * 0.35,
                    "{gain}x client overshot on first tick: {first_jump_logical} vs {span}"
                );
                for _ in 0..5 {
                    let observed = amplified.smooth_delta * gain / (gain / 12.0);
                    assert_eq!(
                        amplified.observe_step(observed, rect, Axis::Vertical),
                        AutoStep::Continue
                    );
                }
                assert!((amplified.smooth_delta * 12.0 - target).abs() < 2.5);
            }
        }
    }

    #[test]
    fn auto_scroll_stops_cleanly_at_bottom_and_avoids_discrete_bursts_on_small_boxes() {
        let tall = Bounds {
            x: 0.0,
            y: 0.0,
            width: 240.0,
            height: 200.0,
        };
        let mut proven = AutoScrollDriver::new(tall, Axis::Vertical);
        assert_eq!(
            proven.observe_step(36.0, tall, Axis::Vertical),
            AutoStep::Continue
        );
        assert!(proven.smooth_proven);
        // Reaching the bottom after smooth scrolling succeeded must never switch to
        // discrete notches and must signal ReachedEnd cleanly without mismatch errors.
        assert_eq!(
            proven.observe_step(0.0, tall, Axis::Vertical),
            AutoStep::Continue
        );
        assert!(!proven.discrete_fallback);
        assert_eq!(proven.observe_mismatch(), AutoMismatch::ReachedEnd);
        assert_eq!(
            proven.observe_step(0.0, tall, Axis::Vertical),
            AutoStep::ReachedEnd
        );

        // Small selections (<120px) never fall back to a 120px discrete wheel notch.
        let tiny = Bounds {
            x: 0.0,
            y: 0.0,
            width: 240.0,
            height: 72.0,
        };
        let mut tiny_driver = AutoScrollDriver::new(tiny, Axis::Vertical);
        for _ in 0..3 {
            assert_eq!(
                tiny_driver.observe_step(0.0, tiny, Axis::Vertical),
                AutoStep::Continue
            );
            assert!(!tiny_driver.discrete_fallback);
        }
        assert_eq!(
            tiny_driver.observe_step(0.0, tiny, Axis::Vertical),
            AutoStep::ReachedEnd
        );
        assert!(!tiny_driver.discrete_fallback);

        // Large discrete-only surfaces fall back after smooth ramp and stop if still immobile.
        let mut fallback = AutoScrollDriver::new(tall, Axis::Vertical);
        for _ in 0..4 {
            assert_eq!(
                fallback.observe_step(0.0, tall, Axis::Vertical),
                AutoStep::Continue
            );
        }
        assert!(fallback.discrete_fallback);
        assert_eq!(
            fallback.observe_step(0.0, tall, Axis::Vertical),
            AutoStep::Continue
        );
        assert_eq!(
            fallback.observe_step(0.0, tall, Axis::Vertical),
            AutoStep::ReachedEnd
        );

        let mut retry = AutoScrollDriver::new(tall, Axis::Vertical);
        retry.smooth_delta = 4.0;
        assert_eq!(retry.observe_mismatch(), AutoMismatch::Retry);
        assert_eq!(retry.smooth_delta, 2.0);
        assert_eq!(retry.observe_mismatch(), AutoMismatch::Retry);
        assert_eq!(retry.smooth_delta, 1.0);
        assert_eq!(retry.observe_mismatch(), AutoMismatch::Failed);
    }
}
