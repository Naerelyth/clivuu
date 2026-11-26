//! Continuous video orchestration. GTK owns the session; a bounded worker owns
//! encoder state changes, file monitoring, final muxing and publication.
mod controls;
mod encoding;
mod export;
mod playback;
mod player;
use crate::{
    infrastructure::{
        recording_file::RecordingFile,
        tasks::{TaskError, TaskPool},
    },
    platform::capture::{
        frames::{PipelineLease, pipeline},
        session::{Session, Stream},
    },
};
use clivuu_core::{
    annotation::Bounds,
    i18n::{t, tf},
    recording::{Options, Phase, Timeline},
};
pub use controls::Setup;
use gst::prelude::*;
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct VideoOutput {
    pub destination: PathBuf,
    pub unique: bool,
    pub replace: bool,
    pub notify: bool,
    pub format: clivuu_core::recording::Format,
}
struct Completed {
    path: PathBuf,
}
struct Running {
    lease: PipelineLease,
    file: Arc<RecordingFile>,
    tracks: Vec<clivuu_core::recording::AudioRole>,
    format: clivuu_core::recording::Format,
}
struct Active {
    session: Session,
    running: Arc<Mutex<Running>>,
    timer: glib::SourceId,
    notify: bool,
    rect: Bounds,
    output: VideoOutput,
    options: Options,
    memory: bool,
    limit: u64,
    origin: Instant,
    timeline: Timeline,
    paused: bool,
}
struct Completion {
    done: Cell<bool>,
    result: RefCell<Result<Option<PathBuf>, String>>,
}
impl Default for Completion {
    fn default() -> Self {
        Self {
            done: Cell::new(false),
            result: RefCell::new(Ok(None)),
        }
    }
}
impl Completion {
    fn complete(&self, result: Result<Option<PathBuf>, String>) {
        let _ = self.result.replace(result);
        self.done.set(true);
    }
    async fn wait(&self) -> Result<Option<PathBuf>, String> {
        while !self.done.get() {
            glib::timeout_future(Duration::from_millis(10)).await;
        }
        self.result.borrow().clone()
    }
}
type Rerecord = Rc<dyn Fn(Bounds)>;
type Changed = Box<dyn Fn(bool, Option<String>, Option<String>)>;
pub struct Recording {
    self_weak: std::rc::Weak<Recording>,
    playback: RefCell<Option<Rc<playback::Playback>>>,
    appearance: RefCell<Option<crate::ui::recording::Appearance>>,
    rerecord: RefCell<Option<Rerecord>>,
    shutting_down: Cell<bool>,
    pool: Rc<TaskPool>,
    active: RefCell<Option<Active>>,
    starting: Cell<bool>,
    start_cancel: RefCell<Option<gio::Cancellable>>,
    completion: RefCell<Rc<Completion>>,
    stopping: Cell<bool>,
    transitioning: Cell<bool>,
    checking: Cell<bool>,
    pending_failure: RefCell<Option<(Arc<Mutex<Running>>, String)>>,
    changed: Changed,
    controls: RefCell<Option<crate::ui::recording::Controls>>,
    setup_cancel: RefCell<Option<gio::Cancellable>>,
    start_choice: RefCell<Option<async_channel::Sender<()>>>,
    inputs: RefCell<crate::platform::integration::audio::Devices>,
}
impl Recording {
    pub fn new(
        pool: Rc<TaskPool>,
        changed: impl Fn(bool, Option<String>, Option<String>) + 'static,
    ) -> Rc<Self> {
        Rc::new_cyclic(|weak| Self {
            self_weak: weak.clone(),
            playback: RefCell::new(None),
            appearance: RefCell::new(None),
            rerecord: RefCell::new(None),
            shutting_down: Cell::new(false),
            pool,
            active: RefCell::new(None),
            starting: Cell::new(false),
            start_cancel: RefCell::new(None),
            completion: RefCell::new(Rc::new(Completion {
                done: Cell::new(true),
                ..Default::default()
            })),
            stopping: Cell::new(false),
            transitioning: Cell::new(false),
            checking: Cell::new(false),
            pending_failure: RefCell::new(None),
            changed: Box::new(changed),
            controls: RefCell::new(None),
            setup_cancel: RefCell::new(None),
            start_choice: RefCell::new(None),
            inputs: RefCell::new(Default::default()),
        })
    }
    pub fn set_appearance(&self, appearance: crate::ui::recording::Appearance) {
        self.appearance.replace(Some(appearance.clone()));
        if let Some(controls) = self.controls.borrow().as_ref() {
            controls.set_appearance(appearance.clone());
        }
        if let Some(playback) = self.playback.borrow().as_ref() {
            playback.set_appearance(appearance);
        }
    }
    pub fn busy(&self) -> bool {
        self.starting.get()
            || self.stopping.get()
            || self.active.borrow().is_some()
            || self.controls.borrow().is_some()
            || self.setup_cancel.borrow().is_some()
            || self.playback.borrow().is_some()
    }
    pub async fn start(
        self: &Rc<Self>,
        session: Session,
        rect: Bounds,
        output: VideoOutput,
        memory: bool,
        limit: u64,
    ) -> Result<Option<PathBuf>, String> {
        if self.starting.get() || self.stopping.get() || self.active.borrow().is_some() {
            return Err(t("Recording or finalization is already in progress"));
        }
        self.starting.set(true);
        self.pending_failure.borrow_mut().take();
        self.completion.replace(Rc::new(Completion::default()));
        self.start_cancel.replace(Some(session.scope.token.clone()));
        let notify = output.notify;
        let options = self
            .controls
            .borrow()
            .as_ref()
            .map_or(Options::default(), |c| c.options());
        let result = async {
            let controls = self.controls.borrow().clone();
            if controls.is_none() {
                self.detect_audio(&session.scope.token).await?;
            }
            if let Some(controls) = controls {
                for remaining in (1..=options.delay).rev() {
                    controls.update(Phase::Countdown(remaining), Duration::ZERO);
                    gio::CancellableFuture::new(
                        glib::timeout_future(Duration::from_secs(1)),
                        session.scope.token.clone(),
                    )
                    .await
                    .map_err(|_| t("Operation cancelled"))?;
                }
                controls.update(Phase::Starting, Duration::ZERO);
                controls.settled(&session.scope.token).await?;
            }
            self.prepare(&session, rect, output.clone(), memory, limit, options)
                .await
        }
        .await;
        self.start_cancel.borrow_mut().take();
        self.starting.set(false);
        let running = match result {
            Ok(running) => running,
            Err(error) => {
                self.completion.borrow().complete(Ok(None));
                return if session.scope.token.is_cancelled() {
                    Ok(None)
                } else {
                    Err(error)
                };
            }
        };
        if session.scope.token.is_cancelled() {
            self.completion.borrow().complete(Ok(None));
            return Ok(None);
        }
        let path = running.lock().unwrap().file.destination.clone();
        let weak = Rc::downgrade(self);
        let timer = glib::timeout_add_local(Duration::from_millis(250), move || {
            let Some(recording) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !recording.checking.replace(true) {
                glib::MainContext::default().spawn_local(async move {
                    recording.check().await;
                });
            }
            glib::ControlFlow::Continue
        });
        self.active.replace(Some(Active {
            session,
            running,
            timer,
            notify,
            rect,
            output,
            options,
            memory,
            limit,
            origin: Instant::now(),
            timeline: Timeline::default(),
            paused: false,
        }));
        self.setup_cancel.borrow_mut().take();
        if let Some(active) = self.active.borrow_mut().as_mut() {
            active.timeline.resume(Duration::ZERO);
        }
        self.update_controls(Phase::Recording);
        (self.changed)(true, Some(path.to_string_lossy().into()), None);
        Ok(Some(path))
    }
    async fn prepare(
        &self,
        session: &Session,
        rect: Bounds,
        output: VideoOutput,
        memory: bool,
        limit: u64,
        options: Options,
    ) -> Result<Arc<Mutex<Running>>, String> {
        let stream = containing_stream(session, rect)?.clone();
        let fd = session.remote().await?;
        let fps = options.fps;
        let audio = self
            .inputs
            .borrow()
            .selected(options.system_audio, options.microphone);
        use clivuu_core::recording::AudioRole;
        let tracks = {
            let inputs = self.inputs.borrow();
            [
                (
                    options.system_audio && inputs.system.is_some(),
                    AudioRole::System,
                ),
                (
                    options.microphone && inputs.microphone.is_some(),
                    AudioRole::Microphone,
                ),
            ]
            .into_iter()
            .filter_map(|(enabled, role)| enabled.then_some(role))
            .collect::<Vec<_>>()
        };
        let task = self
            .pool
            .submit(move |token| {
                token.check()?;
                export::encoders().map_err(TaskError::Failed)?;
                let destination = if output.unique {
                    crate::infrastructure::saving::unused(&output.destination)
                        .map_err(TaskError::Failed)?
                } else {
                    output.destination
                };
                let file = Arc::new(
                    RecordingFile::prepare(destination, memory, limit, output.replace)
                        .map_err(TaskError::Failed)?,
                );
                std::fs::write(
                    file.path.with_file_name("material.json"),
                    serde_json::to_vec_pretty(&serde_json::json!({"version":1,"tracks":tracks}))
                        .map_err(|e| TaskError::Failed(e.to_string()))?,
                )
                .map_err(|e| TaskError::Failed(e.to_string()))?;
                let lease = pipeline(&encoding::description(fps), &stream, fd)?;
                let pipe = lease.pipeline.as_ref().unwrap();
                encoding::configure_crop(pipe, &stream, rect).map_err(TaskError::Failed)?;
                encoding::attach_audio(pipe, &audio).map_err(TaskError::Failed)?;
                let source = pipe.by_name("source").unwrap();
                // Damage-only streams otherwise turn a minute of an unchanged
                // desktop into a single-frame, millisecond-long video.
                if source.find_property("keepalive-time").is_some() {
                    source.set_property("keepalive-time", 100i32);
                }
                if source.find_property("resend-last").is_some() {
                    source.set_property("resend-last", true);
                }

                pipe.by_name("output").unwrap().set_property(
                    "location",
                    file.path.to_str().ok_or_else(|| {
                        TaskError::Failed(t("The recording output must be an absolute path"))
                    })?,
                );
                pipe.set_state(gst::State::Playing).map_err(|_| {
                    TaskError::Failed(t("Cannot start the GStreamer recording pipeline"))
                })?;
                token.check()?;
                Ok(Arc::new(Mutex::new(Running {
                    lease,
                    file,
                    tracks,
                    format: output.format,
                })))
            })
            .map_err(|e| e.to_string())?;
        gio::CancellableFuture::new(task.result(), session.scope.token.clone())
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|e| e.to_string())
    }
    async fn check(self: &Rc<Self>) {
        if self.transitioning.get() || self.stopping.get() {
            self.checking.set(false);
            return;
        }
        let pending = self.pending_failure.borrow_mut().take();
        if let Some((identity, error)) = pending
            && self
                .active
                .borrow()
                .as_ref()
                .is_some_and(|a| Arc::ptr_eq(&identity, &a.running))
        {
            let _ = self.finish(Some(error), None).await;
            self.checking.set(false);
            return;
        }
        let phase = self.active.borrow().as_ref().map(|a| {
            if a.paused {
                Phase::Paused
            } else {
                Phase::Recording
            }
        });
        if let Some(phase) = phase {
            self.update_controls(phase);
        }
        let current = self
            .active
            .borrow()
            .as_ref()
            .map(|a| (a.running.clone(), a.session.scope.token.is_cancelled()));
        let identity = current.as_ref().map(|(r, _)| r.clone());
        if current.as_ref().is_some_and(|(_, cancelled)| *cancelled) {
            // A compositor-ended session finalizes the retained video normally,
            // rather than announcing cancellation as an application error.
            self.checking.set(false);
            let _ = self.stop().await;
            return;
        }
        let outcome = if let Some((running, cancelled)) = current {
            debug_assert!(!cancelled);
            match self.pool.submit(move |_| {
                let running = running.lock().unwrap();
                if let Some(error) = pipeline_error(&running.lease) {
                    return Ok(Err(error));
                }
                Ok(running.file.space_notice())
            }) {
                Ok(task) => match task.result().await {
                    Ok(result) => Some(result),
                    Err(e) => Some(Err(e.to_string())),
                },
                Err(TaskError::Busy) => None,
                Err(e) => Some(Err(e.to_string())),
            }
        } else {
            None
        };
        self.checking.set(false);
        if !self.active.borrow().as_ref().is_some_and(|a| {
            identity
                .as_ref()
                .is_some_and(|r| Arc::ptr_eq(r, &a.running))
        }) {
            return;
        }
        if self.transitioning.get() || self.stopping.get() {
            if let Some(Err(error)) = outcome
                && let Some(identity) = identity
            {
                self.pending_failure.replace(Some((identity, error)));
            }
            return;
        }
        if let Some(result) = outcome {
            match result {
                Ok(Some(notice)) => {
                    let _ = self.finish(None, Some(notice)).await;
                }
                Err(error) => {
                    let _ = self.finish(Some(error), None).await;
                }
                _ => {}
            }
        }
    }
    pub async fn stop(&self) -> Result<Option<PathBuf>, String> {
        // Capture the completion before yielding: an old Stop must never act
        // on a newer recording that starts while finalization wakes waiters.
        let completion = self.completion.borrow().clone();
        while self.transitioning.get() {
            glib::timeout_future(Duration::from_millis(10)).await;
        }
        if completion.done.get() && self.setup_cancel.borrow().is_none() {
            return completion.wait().await;
        }
        if !self.starting.get()
            && self.active.borrow().is_none()
            && !self.stopping.get()
            && let Some(cancel) = self.setup_cancel.borrow().as_ref()
        {
            cancel.cancel();
            return Ok(None);
        }
        if self.starting.get() || self.stopping.get() {
            if let Some(cancel) = self.start_cancel.borrow().as_ref() {
                cancel.cancel();
            }
            return completion.wait().await;
        }
        if self.active.borrow().is_none() {
            return completion.wait().await;
        }
        self.finish(None, None).await
    }
    async fn finish(
        &self,
        failure: Option<String>,
        notice: Option<String>,
    ) -> Result<Option<PathBuf>, String> {
        if self.starting.get() || self.stopping.get() || self.transitioning.get() {
            return Err(t("Recording or finalization is already in progress"));
        }
        let Some(active) = self.active.borrow_mut().take() else {
            return Ok(None);
        };
        self.stopping.set(true);
        self.update_controls(Phase::Finishing);
        let completion = self.completion.borrow().clone();
        active.timer.remove();
        let notify = active.notify;
        let replay = active.options.mode == clivuu_core::recording::Mode::Normal
            && self.controls.borrow().is_some()
            && !self.shutting_down.get();
        let replay_rect = active.rect;
        let running = active.running;
        // A full pool may delay finalization, but may never discard the owned
        // pipeline. Await space asynchronously while keeping the session alive.
        let task = loop {
            let owned = running.clone();
            let failure = failure.clone();
            match self
                .pool
                .submit(move |_| finish_stream(&mut owned.lock().unwrap(), failure))
            {
                Ok(task) => break task,
                Err(TaskError::Busy) => glib::timeout_future(Duration::from_millis(25)).await,
                Err(e) => {
                    self.stopping.set(false);
                    active.session.close().await;
                    self.controls.borrow_mut().take();
                    completion.complete(Err(e.to_string()));
                    return Err(e.to_string());
                }
            }
        };
        let finished = task.result().await.map_err(|e| e.to_string());
        // No compositor stream is needed while encoding the final format.
        active.session.close().await;
        if replay && finished.is_ok() {
            self.controls.borrow_mut().take();
            let result = self.open_playback(running, replay_rect).await;
            self.stopping.set(false);
            completion.complete(result.clone().map(|_| None));
            (self.changed)(false, None, result.as_ref().err().cloned().or(notice));
            return result.map(|_| None);
        }
        self.update_controls(Phase::Exporting);
        let result = if let Err(error) = finished {
            Err(error)
        } else {
            loop {
                let owned = running.clone();
                match self
                    .pool
                    .submit(move |token| publish(&mut owned.lock().unwrap(), &token))
                {
                    Ok(task) => break task.result().await.map_err(|e| e.to_string()),
                    Err(TaskError::Busy) => glib::timeout_future(Duration::from_millis(25)).await,
                    Err(error) => break Err(error.to_string()),
                }
            }
        };
        self.stopping.set(false);
        self.controls.borrow_mut().take();
        match &result {
            Ok(saved) => (self.changed)(
                false,
                notify.then(|| saved.path.to_string_lossy().into()),
                notice,
            ),
            Err(error) => (self.changed)(false, None, Some(error.clone())),
        }
        let result = result.map(|saved| Some(saved.path));
        completion.complete(result.clone());
        result
    }
    async fn open_playback(
        &self,
        running: Arc<Mutex<Running>>,
        rect: Bounds,
    ) -> Result<(), String> {
        let task = self
            .pool
            .submit(move |_| {
                let running = running.lock().unwrap();
                let info = export::probe(&running.file.path, &running.tracks)
                    .map_err(|e| recoverable(&running, e))?;
                std::fs::write(
                    running.file.path.with_file_name("material.json"),
                    serde_json::to_vec_pretty(&info)
                        .map_err(|e| recoverable(&running, e.to_string()))?,
                )
                .map_err(|e| recoverable(&running, e.to_string()))?;
                let player = Arc::new(
                    player::Player::open(&running.file.path, &info)
                        .map_err(|e| recoverable(&running, e))?,
                );
                Ok((running.file.clone(), info, player))
            })
            .map_err(|e| e.to_string())?;
        let (file, info, player) = task.result().await.map_err(|e| e.to_string())?;
        let appearance = self
            .appearance
            .borrow()
            .clone()
            .ok_or_else(|| t("Cannot open recording playback"))?;
        let event = self.self_weak.clone();
        let notify = self.self_weak.clone();
        let replay = playback::Playback::new(
            self.pool.clone(),
            file,
            info,
            player,
            rect,
            appearance,
            move |event_kind| {
                if let Some(r) = event.upgrade() {
                    r.playback.borrow_mut().take();
                    (r.changed)(false, None, None);
                    if let playback::Event::Rerecord(rect) = event_kind
                        && let Some(start) = r.rerecord.borrow().as_ref()
                    {
                        start(rect);
                    }
                }
            },
            move |path, error| {
                if let Some(r) = notify.upgrade() {
                    (r.changed)(false, path, error);
                }
            },
        )?;
        self.playback.replace(Some(replay));
        Ok(())
    }
    pub fn on_rerecord(&self, action: impl Fn(Bounds) + 'static) {
        self.rerecord.replace(Some(Rc::new(action)));
    }
    pub async fn shutdown(&self) -> Result<(), String> {
        self.shutting_down.set(true);
        let result = self.stop().await;
        let replay = self.playback.borrow().clone();
        if let Some(replay) = replay {
            replay.close_preserving().await;
        }
        self.shutting_down.set(false);
        result.map(|_| ())
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        if let Some(active) = self.active.get_mut().take() {
            active.timer.remove();
        }
    }
}
fn recoverable(running: &Running, error: String) -> TaskError {
    TaskError::Failed(tf(
        "{0}; recoverable file: {1}",
        &[&error, &running.file.path.to_string_lossy()],
    ))
}
fn finish_stream(running: &mut Running, failure: Option<String>) -> Result<(), TaskError> {
    let result = (|| -> Result<(), String> {
        if let Some(error) = failure.or_else(|| pipeline_error(&running.lease)) {
            return Err(error);
        }
        let pipe = running
            .lease
            .pipeline
            .as_ref()
            .ok_or_else(|| t("Cannot finalize the recording stream"))?;
        let bus = pipe
            .bus()
            .ok_or_else(|| t("Cannot finalize the recording stream"))?;
        // A paused pipeline cannot drain EOS through the muxer. Resume its
        // running clock before flushing, paused wall time remains excluded.
        pipe.set_state(gst::State::Playing)
            .map_err(|e| e.to_string())?;
        if !pipe.send_event(gst::event::Eos::new()) {
            return Err(t("Cannot finalize the recording stream"));
        }
        let start = Instant::now();
        loop {
            if let Some(message) = bus.timed_pop_filtered(
                gst::ClockTime::from_mseconds(100),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            ) {
                match message.view() {
                    gst::MessageView::Eos(_) => break,
                    gst::MessageView::Error(e) => return Err(e.error().to_string()),
                    _ => {}
                }
            }
            if start.elapsed() > Duration::from_secs(15) {
                return Err(t("Recording finalization timed out"));
            }
        }
        running.lease.stop();
        Ok(())
    })();
    running.lease.stop();
    result.map_err(|e| recoverable(running, e))
}
fn publish(
    running: &mut Running,
    token: &crate::infrastructure::tasks::Cancellation,
) -> Result<Completed, TaskError> {
    let path = export::publish(&running.file, running.format, &running.tracks, token)
        .map_err(|e| recoverable(running, e))?;
    Ok(Completed { path })
}
fn pipeline_error(lease: &PipelineLease) -> Option<String> {
    lease
        .pipeline
        .as_ref()?
        .bus()?
        .pop_filtered(&[gst::MessageType::Error])
        .and_then(|message| match message.view() {
            gst::MessageView::Error(error) => Some(error.error().to_string()),
            _ => None,
        })
}
pub fn containing_stream(session: &Session, rect: Bounds) -> Result<&Stream, String> {
    session
        .streams
        .iter()
        .find(|s| s.rect.is_some_and(|r| contains(r, rect)))
        .ok_or_else(|| t("Select the monitor containing the entire recording region"))
}
fn contains(outer: Bounds, inner: Bounds) -> bool {
    const EPS: f64 = 1e-9;
    [
        outer.x,
        outer.y,
        outer.width,
        outer.height,
        inner.x,
        inner.y,
        inner.width,
        inner.height,
    ]
    .iter()
    .all(|v| v.is_finite())
        && inner.width > 0.
        && inner.height > 0.
        && inner.x >= outer.x - EPS
        && inner.y >= outer.y - EPS
        && inner.x + inner.width <= outer.x + outer.width + EPS
        && inner.y + inner.height <= outer.y + outer.height + EPS
}
