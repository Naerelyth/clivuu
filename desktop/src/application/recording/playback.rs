//! Editable material lifecycle. Acquisition has already closed before this owner
//! exists; failed/cancelled exports leave the same material and edit snapshot live.
use super::{export, player::Player, *};
use crate::{
    infrastructure::{saving, tasks::Cancellation},
    ui::playback::{Command, Controls},
};
use clivuu_core::{
    recording::{MaterialInfo, PlaybackEdit},
    saving::Media,
};
use gtk::gdk;
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone, Copy)]
pub(super) enum Event {
    Closed,
    Rerecord(Bounds),
}
#[derive(Clone)]
struct Pending {
    edit: PlaybackEdit,
    seek: Option<u64>,
    playing: bool,
}
pub(super) struct Playback {
    pool: Rc<TaskPool>,
    file: Arc<RecordingFile>,
    info: MaterialInfo,
    player: Arc<Player>,
    controls: Controls,
    rect: Bounds,
    edit: RefCell<PlaybackEdit>,
    playing: Cell<bool>,
    seeking: Cell<bool>,
    pending: RefCell<Option<Pending>>,
    export_cancel: RefCell<Option<Cancellation>>,
    progress: Arc<AtomicU64>,
    exporting: Cell<bool>,
    closing: Cell<bool>,
    closed: Cell<bool>,
    dialog_cancel: gio::Cancellable,
    timer: RefCell<Option<glib::SourceId>>,
    event: Box<dyn Fn(Event)>,
    notify: Box<dyn Fn(Option<String>, Option<String>)>,
}
impl Playback {
    #[expect(
        clippy::too_many_arguments,
        reason = "Transfer the completed material and presentation at one lifecycle boundary"
    )]
    pub fn new(
        pool: Rc<TaskPool>,
        file: Arc<RecordingFile>,
        info: MaterialInfo,
        player: Arc<Player>,
        rect: Bounds,
        appearance: crate::ui::recording::Appearance,
        event: impl Fn(Event) + 'static,
        notify: impl Fn(Option<String>, Option<String>) + 'static,
    ) -> Result<Rc<Self>, String> {
        let holder = Rc::new(RefCell::new(std::rc::Weak::<Self>::new()));
        let weak = holder.clone();
        let display =
            gdk::Display::default().ok_or_else(|| t("The desktop service is unavailable"))?;
        let controls = Controls::new(&display, rect, appearance, info.clone(), move |cmd| {
            if let Some(this) = weak.borrow().upgrade() {
                this.command(cmd);
            }
        })?;
        let this = Rc::new(Self {
            pool,
            file,
            info: info.clone(),
            player,
            controls,
            rect,
            edit: RefCell::new(PlaybackEdit::new(&info)),
            playing: Cell::new(false),
            seeking: Cell::new(false),
            pending: RefCell::new(None),
            export_cancel: RefCell::new(None),
            progress: Arc::new(AtomicU64::new(0)),
            exporting: Cell::new(false),
            closing: Cell::new(false),
            closed: Cell::new(false),
            dialog_cancel: gio::Cancellable::new(),
            timer: RefCell::new(None),
            event: Box::new(event),
            notify: Box::new(notify),
        });
        holder.replace(Rc::downgrade(&this));
        let weak = Rc::downgrade(&this);
        let timer = glib::timeout_add_local(Duration::from_millis(40), move || {
            let Some(this) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if this.closing.get() {
                return glib::ControlFlow::Continue;
            }
            this.tick();
            glib::ControlFlow::Continue
        });
        this.timer.replace(Some(timer));
        this.tick();
        Ok(this)
    }
    pub fn status(&self) -> serde_json::Value {
        self.controls.status()
    }
    pub fn set_appearance(&self, appearance: crate::ui::recording::Appearance) {
        self.controls.set_appearance(appearance);
    }
    fn tick(self: &Rc<Self>) {
        if let Some(frame) = self.player.frame() {
            let texture = gdk::MemoryTexture::new(
                frame.width as i32,
                frame.height as i32,
                gdk::MemoryFormat::R8g8b8a8,
                &glib::Bytes::from_owned(frame.bytes),
                frame.stride,
            );
            self.controls.texture(texture.upcast());
        }
        match self.player.poll() {
            Err(error) => {
                self.playing.set(false);
                self.controls.error(&error);
                self.queue(None);
            }
            Ok(true) => {
                self.playing.set(false);
                self.queue(Some(self.edit.borrow().start_ns));
            }
            _ => {}
        }
        self.controls.update(
            self.player.position(),
            self.playing.get(),
            self.exporting
                .get()
                .then(|| self.progress.load(Ordering::Relaxed) as f64 / 1000.),
        );
    }
    fn command(self: &Rc<Self>, command: Command) {
        if self.closing.get() {
            return;
        }
        match command {
            Command::CancelExport => {
                if let Some(c) = self.export_cancel.borrow().as_ref() {
                    c.cancel();
                }
            }
            Command::Unavailable => {
                let this = self.clone();
                glib::MainContext::default().spawn_local(async move {
                    this.close_preserving().await;
                });
            }
            Command::Close | Command::Restart => {
                let restart = matches!(command, Command::Restart);
                let this = self.clone();
                glib::MainContext::default().spawn_local(async move {
                    this.close(restart).await;
                });
            }
            _ if self.exporting.get() => {}
            Command::Toggle => {
                self.playing.set(!self.playing.get());
                let end = self.edit.borrow().end_ns;
                let position = self.player.position();
                self.queue(
                    (position >= end.saturating_sub(1)).then(|| self.edit.borrow().start_ns),
                );
            }
            Command::Seek(position) => self.queue(Some(position)),
            Command::Edit(edit) => {
                if edit.validate(&self.info).is_err() {
                    return;
                }
                let old = self.edit.replace(edit.clone());
                let seek = (old.start_ns != edit.start_ns
                    || old.end_ns != edit.end_ns
                    || old.rate != edit.rate)
                    .then(|| self.player.position().clamp(edit.start_ns, edit.end_ns - 1));
                self.controls.edit(&edit);
                self.queue(seek);
            }
            Command::Save(quick) => {
                let this = self.clone();
                glib::MainContext::default().spawn_local(async move {
                    this.export(quick, false).await;
                });
            }
            Command::Copy => {
                let this = self.clone();
                glib::MainContext::default().spawn_local(async move {
                    this.export(true, true).await;
                });
            }
        }
    }
    fn queue(self: &Rc<Self>, seek: Option<u64>) {
        let seek = seek.or_else(|| self.pending.borrow().as_ref().and_then(|p| p.seek));
        self.pending.replace(Some(Pending {
            edit: self.edit.borrow().clone(),
            seek,
            playing: self.playing.get(),
        }));
        if self.seeking.replace(true) {
            return;
        }
        let this = self.clone();
        glib::MainContext::default().spawn_local(async move {
            loop {
                let Some(pending) = this.pending.borrow_mut().take() else {
                    break;
                };
                let player = this.player.clone();
                let info = this.info.clone();
                let retry = pending.clone();
                let task = this.pool.submit(move |_| {
                    let result = (|| {
                        player.gains(&info, &pending.edit);
                        if let Some(at) = pending.seek {
                            player
                                .edit(&info, &pending.edit, at)
                                .map_err(TaskError::Failed)?;
                        }
                        player
                            .set_playing(pending.playing)
                            .map_err(TaskError::Failed)
                    })();
                    if result.is_err() {
                        player.stop();
                    }
                    result
                });
                let result = match task {
                    Ok(job) => job.result().await.map_err(|e| e.to_string()),
                    Err(TaskError::Busy) => {
                        {
                            let mut pending = this.pending.borrow_mut();
                            if let Some(latest) = pending.as_mut() {
                                latest.seek = latest.seek.or(retry.seek);
                            } else {
                                *pending = Some(retry);
                            }
                        }
                        glib::timeout_future(Duration::from_millis(25)).await;
                        continue;
                    }
                    Err(e) => Err(e.to_string()),
                };
                if let Err(error) = result {
                    this.controls.error(&error);
                    this.playing.set(false);
                }
                if this.closing.get() {
                    break;
                }
            }
            this.seeking.set(false);
        });
    }
    async fn export(self: &Rc<Self>, quick: bool, copy: bool) {
        if self.exporting.replace(true) {
            return;
        }
        self.progress.store(0, Ordering::Relaxed);
        self.playing.set(false);
        self.queue(None);
        self.controls.message("");
        self.tick();
        let result = self.export_inner(quick, copy).await;
        self.export_cancel.borrow_mut().take();
        self.exporting.set(false);
        if !self.closing.get() {
            self.controls.suspend(false);
            match result {
                Ok(Some((path, notify))) => {
                    self.controls.message(&if copy {
                        t("Recording file copied")
                    } else {
                        tf("Recording saved: {0}", &[&path.to_string_lossy()])
                    });
                    if notify && !copy {
                        (self.notify)(Some(path.to_string_lossy().into()), None);
                    }
                }
                Ok(None) => {}
                Err(error) => self.controls.error(&error),
            }
            self.tick();
        }
    }
    async fn export_inner(
        self: &Rc<Self>,
        quick: bool,
        copy: bool,
    ) -> Result<Option<(PathBuf, bool)>, String> {
        let task = self
            .pool
            .submit(|_| clivuu_core::config::read().map_err(|e| TaskError::Failed(e.to_string())))
            .map_err(|e| e.to_string())?;
        let settings = task.result().await.map_err(|e| e.to_string())?;
        let context = saving::now()?;
        let format = self.controls.format();
        let (destination, format, replace) = if copy {
            let root = clivuu_core::config::directory("XDG_CACHE_HOME", ".cache")
                .map_err(|e| e.to_string())?;
            (
                root.join(format!(
                    "clipboard-recording-{}.{}",
                    glib::uuid_string_random(),
                    format.extension()
                )),
                format,
                false,
            )
        } else if quick {
            let path = saving::target(&settings, Media::Video, saving::Mode::Quick, &context)?
                .with_extension(format.extension());
            let task = self
                .pool
                .submit(move |_| saving::unused(&path).map_err(TaskError::Failed))
                .map_err(|e| e.to_string())?;
            (
                task.result().await.map_err(|e| e.to_string())?,
                format,
                false,
            )
        } else {
            self.controls.suspend(true);
            glib::timeout_future(Duration::from_millis(40)).await;
            let name = saving::name(&settings, Media::Video, saving::Mode::Manual, &context)?;
            let name = std::path::Path::new(&name).with_extension(format.extension());
            let selected = crate::application::save_dialog::choose(
                Media::Video,
                &name.to_string_lossy(),
                &self.dialog_cancel,
            )
            .await;
            self.controls.suspend(false);
            let Some(selected) = selected? else {
                return Ok(None);
            };
            let path = selected
                .file
                .path()
                .ok_or_else(|| t("Recordings require a local folder"))?;
            let format = clivuu_core::recording::Format::from_path(
                &std::path::Path::new("recording").with_extension(selected.extension),
            )
            .ok_or_else(|| t("Choose a WebM, MP4 or GIF file name"))?;
            (path, format, true)
        };
        if self.closing.get() {
            return Ok(None);
        }
        self.controls.set_format(format);
        let file = self.file.clone();
        let info = self.info.clone();
        let edit = self.edit.borrow().clone();
        let progress = self.progress.clone();
        let task = self
            .pool
            .submit(move |token| {
                export::encode(
                    &file,
                    export::Export {
                        info: &info,
                        edit: &edit,
                        format,
                        destination: &destination,
                        replace,
                        progress: &progress,
                    },
                    &token,
                )
                .map_err(TaskError::Failed)
            })
            .map_err(|e| e.to_string())?;
        self.export_cancel.replace(Some(task.cancellation()));
        let path = match task.result().await {
            Ok(path) => path,
            Err(TaskError::Cancelled) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        if copy {
            let display =
                gdk::Display::default().ok_or_else(|| t("The desktop service is unavailable"))?;
            let uri = gio::File::for_path(&path).uri();
            let content = gdk::ContentProvider::new_union(&[
                gdk::ContentProvider::for_bytes(
                    "text/uri-list",
                    &glib::Bytes::from_owned(format!("{uri}\r\n").into_bytes()),
                ),
                gdk::ContentProvider::for_bytes(
                    "x-special/gnome-copied-files",
                    &glib::Bytes::from_owned(format!("copy\n{uri}").into_bytes()),
                ),
            ]);
            display
                .clipboard()
                .set_content(Some(&content))
                .map_err(|e| e.to_string())?;
        }
        Ok(Some((path, settings["video-save-notify"] != false)))
    }
    pub async fn close(self: &Rc<Self>, restart: bool) {
        self.finish_close(restart, false).await;
    }
    pub async fn close_preserving(self: &Rc<Self>) {
        self.finish_close(false, true).await;
    }
    async fn finish_close(self: &Rc<Self>, restart: bool, preserve: bool) {
        if self.closing.replace(true) {
            while !self.closed.get() {
                glib::timeout_future(Duration::from_millis(10)).await;
            }
            return;
        }
        self.dialog_cancel.cancel();
        if let Some(c) = self.export_cancel.borrow().as_ref() {
            c.cancel();
        }
        self.controls.suspend(true);
        while self.exporting.get() || self.seeking.get() {
            glib::timeout_future(Duration::from_millis(10)).await;
        }
        self.pending.borrow_mut().take();
        let player = self.player.clone();
        let file = self.file.clone();
        let manifest = serde_json::json!({"version":1,"info":self.info,"edit":*self.edit.borrow()});
        loop {
            let player = player.clone();
            let file = file.clone();
            let manifest = manifest.clone();
            match self.pool.submit(move |_| {
                player.stop();
                if preserve {
                    let path = file.path.with_file_name("material.json");
                    let staging = file.path.with_file_name("material.json.tmp");
                    std::fs::write(
                        &staging,
                        serde_json::to_vec_pretty(&manifest)
                            .map_err(|e| TaskError::Failed(e.to_string()))?,
                    )
                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                    std::fs::rename(staging, path).map_err(|e| TaskError::Failed(e.to_string()))?;
                    Ok(())
                } else {
                    file.discard().map_err(TaskError::Failed)
                }
            }) {
                Ok(task) => {
                    if let Err(e) = task.result().await {
                        (self.notify)(None, Some(e.to_string()));
                    }
                    break;
                }
                Err(TaskError::Busy) => glib::timeout_future(Duration::from_millis(25)).await,
                Err(e) => {
                    (self.notify)(None, Some(e.to_string()));
                    break;
                }
            }
        }
        if preserve {
            (self.notify)(
                None,
                Some(tf(
                    "Recording material preserved: {0}",
                    &[&self.file.path.to_string_lossy()],
                )),
            );
        }
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        self.closed.set(true);
        (self.event)(if restart {
            Event::Rerecord(self.rect)
        } else {
            Event::Closed
        });
    }
}
impl Drop for Playback {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.get_mut().take() {
            timer.remove();
        }
    }
}
