//! Ready-state presentation and serialized recording control transactions.
use super::*;
/// Setup owns the presentation until the encoder takes over. Every permission,
/// destination or cancellation error drops the same surfaces and input regions.
pub struct Setup {
    recording: Rc<Recording>,
    pub rect: Bounds,
    pub options: Options,
}
impl Drop for Setup {
    fn drop(&mut self) {
        if self.recording.active.borrow().is_none() && !self.recording.starting.get() {
            self.recording.controls.borrow_mut().take();
            self.recording.setup_cancel.borrow_mut().take();
            self.recording.start_choice.borrow_mut().take();
        }
    }
}
struct Transition<'a>(&'a Cell<bool>);
impl Drop for Transition<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl Recording {
    pub(super) async fn detect_audio(&self, cancel: &gio::Cancellable) -> Result<(), String> {
        let task = self
            .pool
            .submit(|token| {
                token.check()?;
                let devices = crate::platform::integration::audio::Devices::detect();
                token.check()?;
                Ok(devices)
            })
            .map_err(|e| e.to_string())?;
        let devices = gio::CancellableFuture::new(task.result(), cancel.clone())
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|e| e.to_string())?;
        self.inputs.replace(devices);
        Ok(())
    }
    /// Build the live UI before authorization or encoding, keeping all desktop
    /// entry points on the same recording lifecycle. Explicit CLI areas skip
    /// the ready button, but still receive live controls once recording starts.
    pub async fn configure(
        self: &Rc<Self>,
        display: &gtk::gdk::Display,
        rect: Bounds,
        appearance: crate::ui::recording::Appearance,
        options: Options,
        cancel: &gio::Cancellable,
        interactive: bool,
    ) -> Result<Option<Setup>, String> {
        let result = async {
            if self.busy() {
                return Err(t("Recording or finalization is already in progress"));
            }
            self.setup_cancel.replace(Some(cancel.clone()));
            if self.appearance.borrow().is_none() {
                self.appearance.replace(Some(appearance.clone()));
            }
            let mut setup = Setup {
                recording: self.clone(),
                rect,
                options,
            };
            self.detect_audio(cancel).await?;
            let current = self.appearance.borrow().clone().unwrap_or(appearance);
            let (tx, rx) = async_channel::bounded(1);

            let lost = Rc::downgrade(self);
            let lost_cancel = cancel.clone();
            let weak = Rc::downgrade(self);
            let controls = crate::ui::recording::Controls::new(
                display,
                rect,
                current,
                options,
                self.inputs.borrow().availability(),
                move || {
                    lost_cancel.cancel();
                    if let Some(r) = lost.upgrade() {
                        glib::MainContext::default().spawn_local(async move {
                            let _ = r.stop().await;
                        });
                    }
                },
                move |command| {
                    let Some(r) = weak.upgrade() else {
                        return;
                    };
                    use crate::ui::recording::Command;
                    if matches!(command, Command::Start) {
                        if let Some(tx) = r.start_choice.borrow_mut().take() {
                            let _ = tx.try_send(());
                        }
                        return;
                    }
                    glib::MainContext::default().spawn_local(async move {
                        let result = match command {
                            Command::Pause => r.toggle_pause().await,
                            Command::Stop => r.stop().await.map(|_| ()),
                            Command::Restart => r.restart().await,
                            Command::Discard => r.discard().await,
                            Command::Start => Ok(()),
                        };
                        if let Err(error) = result {
                            (r.changed)(true, None, Some(error));
                        }
                    });
                },
            )?;
            self.start_choice.replace(Some(tx));
            self.setup_cancel.replace(Some(cancel.clone()));
            if interactive {
                // The retiring selection and the new live controls have separate
                // native surfaces. Publish Ready only after the new one is painted
                // with its input shape and can own the ready-state shortcuts.
                controls.settled(cancel).await?;
                controls.focus_ready()?;
                controls.settled(cancel).await?;
                if let Some(current) = self.appearance.borrow().clone() {
                    controls.set_appearance(current);
                }
                self.controls.replace(Some(controls.clone()));
                gio::CancellableFuture::new(rx.recv(), cancel.clone())
                    .await
                    .map_err(|_| t("Operation cancelled"))?
                    .map_err(|_| t("Operation cancelled"))?;
            }
            if let Some(current) = self.appearance.borrow().clone() {
                controls.set_appearance(current);
            }
            self.controls.replace(Some(controls.clone()));
            self.start_choice.borrow_mut().take();
            controls.update(Phase::Preparing, Duration::ZERO);
            controls.settled(cancel).await?;
            setup.rect = controls.rect();
            setup.options = controls.options();
            Ok(setup)
        }
        .await;
        if cancel.is_cancelled() {
            Ok(None)
        } else {
            result.map(Some)
        }
    }
    pub fn pausable(&self) -> bool {
        self.active.borrow().is_some() && !self.stopping.get() && !self.transitioning.get()
    }
    pub fn status(&self) -> serde_json::Value {
        if let Some(replay) = self.playback.borrow().as_ref() {
            return replay.status();
        }
        self.controls
            .borrow()
            .as_ref()
            .map_or(serde_json::Value::Null, |c| c.status())
    }
    pub(super) fn update_controls(&self, phase: Phase) {
        let elapsed = self
            .active
            .borrow()
            .as_ref()
            .map_or(Duration::ZERO, |a| a.timeline.elapsed(a.origin.elapsed()));
        if let Some(c) = self.controls.borrow().as_ref() {
            c.update(phase, elapsed);
        }
    }
    pub async fn toggle_pause(&self) -> Result<(), String> {
        if self.starting.get() || self.stopping.get() || self.transitioning.get() {
            return Err(t("Recording or finalization is already in progress"));
        }
        let (running, paused, cancel) = self
            .active
            .borrow()
            .as_ref()
            .map(|a| (a.running.clone(), a.paused, a.session.scope.token.clone()))
            .ok_or_else(|| t("No recording is in progress"))?;
        self.transitioning.set(true);
        let _transition = Transition(&self.transitioning);
        if paused {
            self.update_controls(Phase::Starting);
            let controls = self.controls.borrow().clone();
            if let Some(c) = controls
                && let Err(error) = c.settled(&cancel).await
            {
                self.transitioning.set(false);
                return Err(error);
            }
        }
        let result = match self.pool.submit(move |_| {
            let owned = running.lock().unwrap();
            let pipe = owned
                .lease
                .pipeline
                .as_ref()
                .ok_or_else(|| TaskError::Failed(t("Cannot finalize the recording stream")))?;
            let target = if paused {
                gst::State::Playing
            } else {
                gst::State::Paused
            };
            pipe.set_state(target)
                .map_err(|e| TaskError::Failed(e.to_string()))?;
            let (result, state, pending) = pipe.state(gst::ClockTime::from_seconds(3));
            result.map_err(|e| TaskError::Failed(e.to_string()))?;
            if state != target {
                eprintln!("Recording state change: target={target:?}, current={state:?}, pending={pending:?}");
                pipe.debug_to_dot_file(gst::DebugGraphDetails::all(),"recording-state-failure");
                return Err(TaskError::Failed(t("Cannot change recording pause state")));
            }
            Ok(())
        }) {
            Ok(task) => task.result().await.map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        if result.is_ok()
            && let Some(a) = self.active.borrow_mut().as_mut()
        {
            a.paused = !paused;
            let now = a.origin.elapsed();
            if a.paused {
                a.timeline.pause(now);
            } else {
                a.timeline.resume(now);
            }
        }
        self.transitioning.set(false);
        self.update_controls(if self.active.borrow().as_ref().is_some_and(|a| a.paused) {
            Phase::Paused
        } else {
            Phase::Recording
        });
        (self.changed)(true, None, None);
        result
    }
    async fn discard_running(&self, running: Arc<Mutex<Running>>) -> Result<(), String> {
        loop {
            let owned = running.clone();
            match self.pool.submit(move |_| {
                let mut owned = owned.lock().unwrap();
                owned.lease.stop();
                owned.file.discard().map_err(TaskError::Failed)
            }) {
                Ok(task) => return task.result().await.map_err(|e| e.to_string()),
                Err(TaskError::Busy) => glib::timeout_future(Duration::from_millis(25)).await,
                Err(error) => return Err(error.to_string()),
            }
        }
    }
    pub async fn discard(&self) -> Result<(), String> {
        if self.setup_cancel.borrow().is_some() {
            if let Some(cancel) = self.setup_cancel.borrow().as_ref() {
                cancel.cancel();
            }
            return self.stop().await.map(|_| ());
        }
        if self.starting.get() || self.stopping.get() || self.transitioning.get() {
            return Err(t("Recording or finalization is already in progress"));
        }
        let Some(active) = self.active.borrow_mut().take() else {
            return Ok(());
        };
        self.stopping.set(true);
        active.timer.remove();
        self.update_controls(Phase::Finishing);
        let result = self.discard_running(active.running).await;
        active.session.close().await;
        self.controls.borrow_mut().take();
        self.stopping.set(false);
        self.completion
            .borrow()
            .complete(result.clone().map(|_| None));
        (self.changed)(false, None, result.as_ref().err().cloned());
        result
    }
    pub async fn restart(&self) -> Result<(), String> {
        if self.starting.get() || self.stopping.get() || self.transitioning.get() {
            return Err(t("Recording or finalization is already in progress"));
        }
        let Some(mut active) = self.active.borrow_mut().take() else {
            return Ok(());
        };
        self.transitioning.set(true);
        let _transition = Transition(&self.transitioning);
        self.update_controls(Phase::Starting);
        let controls = self.controls.borrow().clone();
        let result = async {
            self.discard_running(active.running.clone()).await?;
            if let Some(c) = controls {
                c.settled(&active.session.scope.token).await?;
            }
            self.prepare(
                &active.session,
                active.rect,
                active.output.clone(),
                active.memory,
                active.limit,
                active.options,
            )
            .await
        }
        .await;
        self.transitioning.set(false);
        match result {
            Ok(running) => {
                active.running = running;
                active.paused = false;
                active.origin = Instant::now();
                active.timeline = Timeline::default();
                active.timeline.resume(Duration::ZERO);
                self.active.replace(Some(active));
                self.update_controls(Phase::Recording);
                (self.changed)(true, None, None);
                Ok(())
            }
            Err(error) => {
                active.timer.remove();
                active.session.close().await;
                self.controls.borrow_mut().take();
                self.completion.borrow().complete(Err(error.clone()));
                (self.changed)(false, None, Some(error.clone()));
                Err(error)
            }
        }
    }
}
