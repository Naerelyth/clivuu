//! Restart and Quit share one drain/persist/shutdown boundary. The executable
//! handoff belongs to main, after the old GTK application releases its bus name.
use super::*;
pub(super) struct Activity<'a>(&'a Cell<usize>);
impl Drop for Activity<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
impl State {
    pub(super) fn activity(&self) -> Result<Activity<'_>, String> {
        if self.stopping.get() || self.cancel.is_cancelled() {
            return Err(t("The desktop service is shutting down"));
        }
        self.activities.set(self.activities.get() + 1);
        Ok(Activity(&self.activities))
    }
    pub(super) fn work_cancel(&self) -> gio::Cancellable {
        self.work_cancel.borrow().clone()
    }
    pub(super) async fn stop_service(&self, restart: bool) -> Result<Value, String> {
        self.stopping.set(true);
        struct Stopping<'a>(&'a State);
        impl Drop for Stopping<'_> {
            fn drop(&mut self) {
                // A failed flush must leave the current service usable. Old
                // operations have drained; a fresh token accepts future work.
                if !self.0.cancel.is_cancelled() && self.0.work_cancel().is_cancelled() {
                    self.0.work_cancel.replace(gio::Cancellable::new());
                }
                self.0.stopping.set(false);
            }
        }
        let _stopping = Stopping(self);
        let relaunch = if restart {
            let task = self
                .pool
                .submit(|_| {
                    crate::platform::integration::launch::Launch::current()
                        .map_err(TaskError::Failed)
                })
                .map_err(|e| e.to_string())?;
            Some(task.result().await.map_err(|e| e.to_string())?)
        } else {
            None
        };
        // Cancel dialogs, imports and selection before flushing. Persistence
        // uses the separate lifetime token, so cancellation cannot discard it.
        while !self.ready.get() {
            glib::timeout_future(std::time::Duration::from_millis(10)).await;
        }
        self.work_cancel().cancel();
        if let Some(cancel) = self.record_pending.borrow().as_ref() {
            cancel.cancel();
        }
        self.recording.shutdown().await?;
        while self.activities.get() > 1 {
            glib::timeout_future(std::time::Duration::from_millis(10)).await;
        }
        self.preferences.command("close").await?;
        self.group_manager.command("close").await?;
        self.pins.finish_input();
        let _pins = self.pins.during_capture(true);
        let configuration = self.configuration.borrow().clone();
        if let Some(config) = configuration {
            config.flush().await?;
        }
        while self.saving.get() {
            glib::timeout_future(std::time::Duration::from_millis(10)).await;
        }
        self.flush().await?;
        self.relaunch.replace(relaunch);
        self.cancel.cancel();
        self.pins.shutdown();
        let app = self.application.clone();
        glib::idle_add_local_once(move || {
            if let Some(app) = app.upgrade() {
                app.quit();
            }
        });
        Ok(json!({"ok":true}))
    }
}
