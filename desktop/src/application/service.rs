//! Native Desktop1 endpoint, configuration and session lifecycle.
use super::capture;
mod captures;
mod continuous;
mod imports;
mod lifecycle;
mod outputs;
use crate::{
    infrastructure::{
        cancellation::Scope,
        tasks::{TaskError, TaskPool},
    },
    ui::toolbar::Output,
};
use clivuu_core::{
    annotation::Bounds,
    config,
    i18n::{t, tf},
    protocol,
};
use gtk::{gdk, gio, glib, prelude::*};
use outputs::Destination;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Mutex},
};
pub struct Service {
    state: Rc<State>,
    registration: Option<gio::RegistrationId>,
    _hold: gio::ApplicationHoldGuard,
    _cursor_theme: Option<crate::platform::surfaces::CursorThemeWatch>,
}
struct State {
    application: glib::WeakRef<gtk::Application>,
    connection: gio::DBusConnection,
    notifications: crate::platform::integration::notifications::Notifications,
    pool: Rc<TaskPool>,
    cancel: gio::Cancellable,
    busy: Cell<bool>,
    stopping: Cell<bool>,
    activities: Cell<usize>,
    work_cancel: RefCell<gio::Cancellable>,
    relaunch: RefCell<Option<crate::platform::integration::launch::Launch>>,
    pins: Rc<super::pins::Pins>,
    ready: Cell<bool>,
    initialization_error: RefCell<Option<String>>,
    store: RefCell<Option<Arc<Mutex<crate::infrastructure::pin_store::PinStore>>>>,
    saved: Cell<u64>,
    saving: Cell<bool>,
    memory: Cell<bool>,
    configuration: RefCell<Option<Rc<crate::infrastructure::configuration::Configuration>>>,
    configuration_watch: RefCell<Option<crate::infrastructure::configuration::Subscription>>,
    preferences: super::preferences::Client,
    group_manager: super::group_manager::Client,
    tray: RefCell<Option<Rc<crate::platform::integration::tray::Tray>>>,
    recording: Rc<super::recording::Recording>,
    record_pending: RefCell<Option<gio::Cancellable>>,
    scrolling: super::scrolling::Status,
    _session_lock: crate::platform::integration::session_lock::SessionLock,
    operation: RefCell<Option<gio::Cancellable>>,
    last_rect: Cell<Option<Bounds>>,
    capture_records: RefCell<Vec<capture::Snapshot>>,
    capture_commands: capture::Commands,
    areas: RefCell<Vec<Bounds>>,
    history: RefCell<Vec<CaptureRecord>>,
    history_directory: RefCell<Option<Arc<crate::infrastructure::temporary::Directory>>>,
    screenshot: RefCell<Option<Rc<crate::platform::capture::screenshot::Screenshot>>>,
}
#[derive(Clone)]
struct CaptureRecord {
    png: Arc<Vec<u8>>,
    rect: Bounds,
    source: String,
}
struct Operation<'a>(&'a RefCell<Option<gio::Cancellable>>);
impl Drop for Operation<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().take();
    }
}
struct Busy<'a>(&'a Cell<bool>);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.state.work_cancel().cancel();
        self.state.cancel.cancel();
        self.state.pins.shutdown();
        if let Some(id) = self.registration.take() {
            let _ = self.state.connection.unregister_object(id);
        }
    }
}
impl Service {
    pub fn take_relaunch(&self) -> Option<crate::platform::integration::launch::Launch> {
        self.state.relaunch.borrow_mut().take()
    }
    pub fn new(application: &gtk::Application) -> Result<Self, String> {
        let connection = application
            .dbus_connection()
            .ok_or_else(|| t("The desktop service is unavailable"))?;
        let pool = Rc::new(TaskPool::new(2, 4).map_err(|e| e.to_string())?);
        let display =
            gdk::Display::default().ok_or_else(|| t("The desktop service is unavailable"))?;
        let cursor_theme = crate::platform::surfaces::CursorThemeWatch::new(&display)?;
        let state = Rc::new_cyclic(|weak: &std::rc::Weak<State>| {
            let observer = weak.clone();
            let recording =
                super::recording::Recording::new(pool.clone(), move |active, path, message| {
                    if let Some(state) = observer.upgrade() {
                        state.refresh_menus();
                        if message.is_some() || !active && path.is_some() {
                            let body = message.as_deref().or(path.as_deref()).unwrap_or("");
                            if let Some(path) = path.as_deref() {
                                state.notifications.send_file(
                                    "recording",
                                    &t("Region recording"),
                                    body,
                                    std::path::Path::new(&path),
                                    clivuu_core::saving::Media::Video,
                                );
                            } else {
                                state
                                    .notifications
                                    .send("recording", &t("Region recording"), body);
                            }
                        }
                    }
                });
            let observer = weak.clone();
            recording.on_rerecord(move |rect| {
                if let Some(state) = observer.upgrade() {
                    glib::MainContext::default().spawn_local(async move {
                        let values = state
                            .configuration
                            .borrow()
                            .as_ref()
                            .map(|c| c.values())
                            .unwrap_or_else(|| {
                                clivuu_core::config::parse("").expect("default settings")
                            });
                        if let Err(error) = state.start_recording(rect, &json!({}), &values).await {
                            state.report_error(&error);
                        }
                    });
                }
            });
            let observer = weak.clone();
            let session_lock = crate::platform::integration::session_lock::SessionLock::new(
                &connection,
                move |locked| {
                    if let Some(state) = observer.upgrade() {
                        state.pins.lock(locked);
                        if locked {
                            if let Some(cancel) = state.operation.borrow().as_ref() {
                                cancel.cancel();
                            }
                            if let Some(cancel) = state.record_pending.borrow().as_ref() {
                                cancel.cancel();
                            }
                            let recording = state.recording.clone();
                            glib::MainContext::default().spawn_local(async move {
                                let _ = recording.stop().await;
                            });
                        }
                    }
                },
            );
            let weak = weak.clone();
            let pins = super::pins::Pins::new(&display, pool.clone(), move |event| {
                if let Some(state) = weak.upgrade() {
                    match event {
                        super::pins::Event::Output(id, action) => {
                            glib::MainContext::default().spawn_local(async move {
                                if let Err(e) = state.pin_action(id, action).await {
                                    state.report_error(&e);
                                }
                            });
                        }
                        super::pins::Event::Paste => {
                            glib::MainContext::default().spawn_local(async move {
                                if let Err(e) = state
                                    .execute(
                                        json!({"protocol":protocol::VERSION,"command":"paste"}),
                                    )
                                    .await
                                {
                                    state.report_error(&e);
                                }
                            });
                        }
                        super::pins::Event::Drop(id, drop, preserve) => {
                            glib::MainContext::default().spawn_local(async move {
                                state.drop_image(id, drop, preserve).await;
                            });
                        }
                        super::pins::Event::MoveToNewGroup(move_ids) => {
                            glib::MainContext::default().spawn_local(async move {
                                if let Err(e) = state
                                    .group_manager
                                    .open(super::group_manager::OpenMode::Create { move_ids })
                                    .await
                                {
                                    state.report_error(&e);
                                }
                            });
                        }
                        super::pins::Event::ManageGroups => {
                            glib::MainContext::default().spawn_local(async move {
                                if let Err(e) = state
                                    .group_manager
                                    .open(super::group_manager::OpenMode::Manage)
                                    .await
                                {
                                    state.report_error(&e);
                                }
                            });
                        }
                        super::pins::Event::Changed => state.refresh_menus(),
                    }
                }
            });
            State {
                application: application.downgrade(),
                connection: connection.clone(),
                notifications: crate::platform::integration::notifications::Notifications::new(
                    &connection,
                    pool.clone(),
                ),
                pool: pool.clone(),
                cancel: gio::Cancellable::new(),
                busy: Cell::new(false),
                stopping: Cell::new(false),
                activities: Cell::new(0),
                work_cancel: RefCell::new(gio::Cancellable::new()),
                relaunch: RefCell::new(None),
                pins,
                ready: Cell::new(false),
                initialization_error: RefCell::new(None),
                store: RefCell::new(None),
                saved: Cell::new(0),
                saving: Cell::new(false),
                memory: Cell::new(false),
                configuration: RefCell::new(None),
                configuration_watch: RefCell::new(None),
                preferences: super::preferences::Client::new(&connection),
                group_manager: super::group_manager::Client::new(&connection),
                tray: RefCell::new(None),
                recording,
                record_pending: RefCell::new(None),
                scrolling: Rc::new(RefCell::new(None)),
                _session_lock: session_lock,
                operation: RefCell::new(None),
                last_rect: Cell::new(None),
                capture_records: RefCell::new(vec![]),
                capture_commands: capture::Commands::default(),
                areas: RefCell::new(vec![]),
                history: RefCell::new(vec![]),
                history_directory: RefCell::new(None),
                screenshot: RefCell::new(None),
            }
        });
        let info=gio::DBusNodeInfo::for_xml(&format!("<node><interface name='{}'><method name='Execute'><arg type='s' direction='in'/><arg type='s' direction='out'/></method></interface></node>",protocol::INTERFACE)).map_err(|e|e.to_string())?.lookup_interface(protocol::INTERFACE).unwrap();
        let weak = Rc::downgrade(&state);
        let registration = connection
            .register_object(protocol::OBJECT_PATH, &info)
            .method_call(move |_, _, _, _, _, parameters, invocation| {
                let state = weak.upgrade();
                let request = parameters.get::<(String,)>().map(|x| x.0);
                glib::MainContext::default().spawn_local(async move {
                    let result = match (state, request) {
                        (Some(state), Some(request)) if request.len() <= 1_000_000 => {
                            match serde_json::from_str(&request) {
                                Ok(request) => state.execute(request).await,
                                Err(e) => Err(e.to_string()),
                            }
                        }
                        _ => Err(t("The request is too large")),
                    };
                    match result {
                        Ok(value) => {
                            invocation.return_value(Some(&(value.to_string(),).to_variant()))
                        }
                        Err(error) => invocation
                            .return_dbus_error(&format!("{}.Error", protocol::INTERFACE), &error),
                    }
                });
            })
            .build()
            .map_err(|e| e.to_string())?;
        let weak = Rc::downgrade(&state);
        let tray = crate::platform::integration::tray::Tray::new(&connection, move |request| {
            if let Some(state) = weak.upgrade() {
                glib::MainContext::default().spawn_local(async move {
                    if let Err(e) = state.execute(request).await {
                        state.report_error(&e);
                    }
                });
            }
        })?;
        state.tray.replace(Some(tray));
        let start = state.clone();
        glib::MainContext::default().spawn_local(async move {
            if let Err(e) = start.initialize().await {
                eprintln!("{e}");
                start.initialization_error.replace(Some(e));
            }
            start.ready.set(true);
        });
        let weak = Rc::downgrade(&state);
        glib::timeout_add_local(std::time::Duration::from_millis(600), move || {
            let Some(state) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !state.cancel.is_cancelled()
                && !state.stopping.get()
                && state.ready.get()
                && !state.saving.get()
                && state.saved.get() != state.pins.revision.get()
            {
                glib::MainContext::default().spawn_local(async move {
                    if let Err(e) = state.flush().await {
                        eprintln!("{e}");
                    }
                });
            }
            glib::ControlFlow::Continue
        });
        Ok(Self {
            state,
            registration: Some(registration),
            _hold: application.hold(),
            _cursor_theme: cursor_theme,
        })
    }
}
impl State {
    fn operation_cancel(&self) -> gio::Cancellable {
        self.operation
            .borrow()
            .clone()
            .unwrap_or_else(|| self.work_cancel())
    }
    fn report_error(&self, error: &str) {
        eprintln!("{error}");
        self.notifications
            .send("capture-error", &t("Clivuu"), error);
    }
    fn refresh_menus(&self) {
        if let Some(tray) = self.tray.borrow().as_ref() {
            let (closed_count, active_pins, has_click_through, summaries, has_empty) = {
                let board = self.pins.board.borrow();
                let current = &board.group;
                (
                    board.pins.iter().filter(|p| p.closed).count(),
                    board
                        .pins
                        .iter()
                        .filter(|p| !p.closed && &p.group == current)
                        .count(),
                    board
                        .pins
                        .iter()
                        .any(|p| !p.closed && &p.group == current && p.through),
                    board.group_summaries(),
                    board.has_empty_groups(),
                )
            };
            tray.state(crate::platform::integration::tray::TrayState {
                closed_count,
                active_pins,
                has_click_through,
                has_recent_capture: !self.history.borrow().is_empty(),
                recording: self.recording.busy() || self.record_pending.borrow().is_some(),
                pausable: self.recording.pausable(),
            });
            tray.set_groups(summaries, has_empty);
        }
        self.group_manager.notify_refresh();
    }
    async fn initialize(self: &Rc<Self>) -> Result<(), String> {
        let task = self
            .pool
            .submit(|_| {
                let config_path = config::path().map_err(|e| TaskError::Failed(e.to_string()))?;
                use std::os::unix::fs::DirBuilderExt;
                std::fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(config_path.parent().unwrap())
                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                let settings = config::read().map_err(|e| TaskError::Failed(e.to_string()))?;
                if let Ok(launch) = crate::platform::integration::launch::Launch::current()
                    && let Err(error) =
                        crate::platform::integration::autostart::Autostart::default()
                            .refresh(&launch)
                {
                    eprintln!("{error}");
                }
                let memory = settings["pin-storage"].as_str() == Some("memory");
                let (store, restored) = crate::infrastructure::pin_store::PinStore::open(memory)
                    .map_err(TaskError::Failed)?;
                Ok((settings, memory, store, restored))
            })
            .map_err(|e| e.to_string())?;
        let (settings, memory, store, restored) = task.result().await.map_err(|e| e.to_string())?;
        clivuu_core::i18n::initialize(settings["language"].as_str());
        self.pins.settings(&settings);
        self.recording
            .set_appearance(recording_appearance(&settings));
        let configuration = crate::infrastructure::configuration::Configuration::new(
            settings.clone(),
            self.pool.clone(),
            |e| eprintln!("{e}"),
        )?;
        let weak = Rc::downgrade(self);
        self.configuration_watch
            .replace(Some(configuration.subscribe(move |values| {
                if let Some(this) = weak.upgrade() {
                    this.pins.settings(values);
                    this.recording.set_appearance(recording_appearance(values));
                    if let Some(tray) = this.tray.borrow().as_ref() {
                        tray.refresh();
                    }
                    this.refresh_menus();
                    let memory = values["pin-storage"].as_str() == Some("memory");
                    if this.memory.replace(memory) != memory {
                        this.pins.commit();
                    }
                }
            })));
        self.pins.set_live_config(configuration.clone());
        self.configuration.replace(Some(configuration));
        self.memory.set(memory);
        let store = Arc::new(Mutex::new(store));
        self.store.replace(Some(store.clone()));
        self.pins.restore(restored, store)?;
        self.refresh_menus();
        self.saved.set(0);
        Ok(())
    }
    async fn flush(&self) -> Result<(), String> {
        if self.saving.replace(true) {
            return Ok(());
        }
        let _saving = Busy(&self.saving);
        let Some(store) = self.store.borrow().clone() else {
            return Ok(());
        };
        let revision = self.pins.revision.get();
        let Some(snapshot) = self.pins.snapshot(&self.cancel).await? else {
            return Ok(());
        };
        let memory = self.memory.get();
        let task = self
            .pool
            .submit(move |_| {
                store
                    .lock()
                    .unwrap()
                    .publish(snapshot, memory)
                    .map_err(TaskError::Failed)
            })
            .map_err(|e| e.to_string())?;
        let revisions = task.result().await.map_err(|e| e.to_string())?;
        self.pins.persisted(revisions);
        self.saved.set(revision);
        Ok(())
    }
    async fn execute(&self, request: Value) -> Result<Value, String> {
        let _activity = if request["command"] == "status" {
            None
        } else {
            Some(self.activity()?)
        };
        self.execute_request(request).await
    }
    async fn execute_request(&self, request: Value) -> Result<Value, String> {
        if request["protocol"].as_u64() != Some(protocol::VERSION as u64) {
            return Err(t("Invalid desktop protocol version"));
        }
        let command = request["command"]
            .as_str()
            .ok_or_else(|| t("Invalid desktop response"))?;
        if command != "status" && (self.stopping.get() || self.cancel.is_cancelled()) {
            return Err(t("The desktop service is shutting down"));
        }
        if !matches!(command, "status" | "exit" | "restart") {
            while !self.ready.get() {
                glib::timeout_future(std::time::Duration::from_millis(10)).await;
            }
            if self.stopping.get() {
                return Err(t("The desktop service is shutting down"));
            }
            if let Some(e) = self.initialization_error.borrow().as_ref() {
                return Err(e.clone());
            }
        }
        let display =
            gdk::Display::default().ok_or_else(|| t("The desktop service is unavailable"))?;
        match command {
            "status" => {
                let board = self.pins.board.borrow();
                let pins = board.pins.iter().filter(|p| !p.closed).count();
                let closed = board.pins.iter().filter(|p| p.closed).count();
                return Ok(
                    json!({"running":true,"pid":std::process::id(),"ready":self.ready.get(),"protocol":protocol::VERSION,"backend":"rust","busy":self.busy.get(),"recording":self.recording.busy(),"recordingUi":self.recording.status(),"scrolling":*self.scrolling.borrow(),"pins":pins,"closedPins":closed,"pinUi":self.pins.status(),"captureOcr":self.capture_commands.ocr_status(),"captureCursor":self.capture_commands.cursor_status(),"captureWindows":gtk::Window::list_toplevels().iter().filter_map(|w|w.downcast_ref::<gtk::Window>()).filter(|w|w.title().as_deref()==Some("Clivuu capture overlay")).map(|w|json!({"mapped":w.is_mapped(),"active":w.is_active(),"focus":gtk::prelude::GtkWindowExt::focus(w).map(|f|f.type_().name().to_string()),"controllers":w.observe_controllers().n_items()})).collect::<Vec<_>>(),"images":board.pins.iter().filter(|p|!p.closed).map(|p|json!({"id":p.id,"rect":p.rect(),"group":p.group,"zoom":p.zoom,"hidden":p.hidden,"opacity":p.opacity,"shadow":p.shadow,"smooth":p.smooth,"maxSize":p.max_size,"grayscale":p.grayscale,"inverted":p.inverted})).collect::<Vec<_>>(),"group":board.group,"groups":board.groups,"groupDetails":board.group_summaries(),"capabilities":protocol::Capabilities{protocol:protocol::VERSION,backend:display.type_().name().to_string(),capture:true,non_focusing_pins:true,smart_windows:crate::platform::integration::windows::available(),scrolling:true,recording:true,settings:true}}),
                );
            }
            "capture-history" => return self.capture_history().await,
            "clear-snip-history" => {
                self.history.borrow_mut().clear();
                self.capture_records.borrow_mut().clear();
                self.history_directory.borrow_mut().take();
                self.areas.borrow_mut().clear();
                self.last_rect.set(None);
                self.refresh_menus();
                return Ok(json!({"ok":true}));
            }
            "start" => return Ok(json!({"running":true})),
            "pause-recording" => {
                self.recording.toggle_pause().await?;
                self.refresh_menus();
                return Ok(json!({"ok":true,"recordingUi":self.recording.status()}));
            }
            "stop-recording" => {
                if let Some(cancel) = self.record_pending.borrow().as_ref() {
                    cancel.cancel();
                }
                let path = self.recording.stop().await?;
                return Ok(json!({"path":path}));
            }
            "open-preferences" => {
                self.preferences.open().await?;
                return Ok(json!({"ok":true}));
            }
            "exit" | "restart" => return self.stop_service(command == "restart").await,
            "show-images" | "hide-images" | "toggle-images" => {
                {
                    let mut b = self.pins.board.borrow_mut();
                    b.hidden = if command == "toggle-images" {
                        !b.hidden
                    } else {
                        command == "hide-images"
                    };
                    if !b.hidden {
                        for p in &mut b.pins {
                            p.hidden = false;
                        }
                    }
                }
                self.pins.commit();
                return Ok(json!({"visible":!self.pins.board.borrow().hidden}));
            }
            "no-click-through" | "toggle-click-through" => {
                for p in &mut self.pins.board.borrow_mut().pins {
                    p.through = command != "no-click-through" && !p.through;
                }
                self.pins.commit();
                return Ok(json!({"ok":true}));
            }
            "reopen-pin" | "reopen-pins" => {
                let restored = self
                    .pins
                    .reopen(command == "reopen-pins", &self.work_cancel())
                    .await?;
                return Ok(
                    json!({"restored":restored,"remaining":self.pins.board.borrow().pins.iter().filter(|p|p.closed).count()}),
                );
            }
            "close-pins" | "destroy-closed-pins" | "empty-group" => {
                if self.busy.replace(true) {
                    return Err(t("A capture or import session is already in progress"));
                }
                let _busy = Busy(&self.busy);
                let group = if command == "empty-group"
                    && let Some(arg) = request["argument"]
                        .as_str()
                        .filter(|s| !s.trim().is_empty())
                {
                    let trimmed = arg.trim();
                    if trimmed.eq_ignore_ascii_case(clivuu_core::pins::DEFAULT_GROUP) {
                        clivuu_core::pins::DEFAULT_GROUP.to_owned()
                    } else {
                        trimmed.to_owned()
                    }
                } else {
                    self.pins.board.borrow().group.clone()
                };
                if command == "empty-group" && !self.pins.board.borrow().groups.contains(&group) {
                    return Err(tf("Pin group not found: {0}", &[&group]));
                }
                let ids: Vec<_> = {
                    let b = self.pins.board.borrow();
                    b.pins
                        .iter()
                        .filter(|p| match command {
                            "close-pins" => !p.closed && p.group == group,
                            "destroy-closed-pins" => p.closed,
                            _ => p.group == group,
                        })
                        .map(|p| p.id)
                        .collect()
                };
                let destroy = command != "close-pins";
                let confirm = request["confirmed"] != true && {
                    let settings = self.pins.preferences.borrow();
                    if destroy {
                        settings.confirm_destroy
                    } else {
                        settings.confirm_close
                    }
                };
                if ids.len() > 1
                    && confirm
                    && !super::confirmation::pins(destroy, ids.len(), &self.work_cancel()).await?
                {
                    return Ok(json!({"cancelled":true}));
                }
                let mut count = 0;
                for id in ids {
                    let eligible =
                        self.pins
                            .board
                            .borrow()
                            .get(id)
                            .is_some_and(|p| match command {
                                "close-pins" => !p.closed && p.group == group,
                                "destroy-closed-pins" => p.closed,
                                _ => p.group == group,
                            });
                    if eligible {
                        if destroy {
                            self.pins.destroy(id);
                        } else {
                            self.pins.close(id);
                        }
                        count += 1;
                    }
                }
                return Ok(if destroy {
                    json!({"destroyed":count})
                } else {
                    json!({"closed":count})
                });
            }
            "groups" => {
                let b = self.pins.board.borrow();
                return Ok(json!({
                    "groups": b.groups,
                    "current": b.group,
                    "previous": b.previous,
                    "hasEmptyGroups": b.has_empty_groups(),
                    "details": b.group_summaries()
                }));
            }
            "show-group-manager" => {
                let mode = if request["mode"].as_str() == Some("create") {
                    let move_ids = request["moveIds"]
                        .as_array()
                        .map(|arr| arr.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    super::group_manager::OpenMode::Create { move_ids }
                } else {
                    super::group_manager::OpenMode::Manage
                };
                self.group_manager.open(mode).await?;
                let b = self.pins.board.borrow();
                return Ok(json!({"ok":true,"groups":b.groups,"current":b.group}));
            }
            "switch-groups" => {
                self.group_manager
                    .open(super::group_manager::OpenMode::Switcher)
                    .await?;
                let b = self.pins.board.borrow();
                return Ok(json!({"ok":true,"groups":b.groups,"current":b.group}));
            }
            "create-group" => {
                if request["prompt"] == true {
                    self.group_manager
                        .open(super::group_manager::OpenMode::Create { move_ids: vec![] })
                        .await?;
                    return Ok(json!({"prompt":true,"group":self.pins.board.borrow().group}));
                }
                let name = request["argument"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        let b = self.pins.board.borrow();
                        let mut idx = b.groups.len() + 1;
                        loop {
                            let candidate = tf("Pin group {0}", &[&idx.to_string()]);
                            if !b.groups.contains(&candidate) {
                                break candidate;
                            }
                            idx += 1;
                        }
                    });
                let activate = request["activate"].as_bool().unwrap_or(true);
                let strict = request["strict"].as_bool().unwrap_or(false);
                let color = request["color"].as_str();
                let before = self.pins.board.borrow().group.clone();
                let created = self
                    .pins
                    .board
                    .borrow_mut()
                    .create_group_with(&name, color, activate, strict)?;
                if let Some(arr) = request["moveIds"].as_array() {
                    let ids: Vec<u64> = arr.iter().filter_map(Value::as_u64).collect();
                    if !ids.is_empty() {
                        let _ = self.pins.move_to_group(&ids, &created)?;
                    }
                }
                if activate && self.pins.board.borrow().group != before {
                    self.pins.toolbar(None);
                }
                self.pins.commit();
                if activate {
                    self.pins.group_changed();
                }
                return Ok(json!({"group":created,"current":self.pins.board.borrow().group}));
            }
            "rename-group" => {
                let old = request["argument"]
                    .as_str()
                    .ok_or_else(|| t("Invalid desktop response"))?;
                let to = request["to"]
                    .as_str()
                    .ok_or_else(|| t("rename-group requires a new group name"))?;
                let renamed = self.pins.board.borrow_mut().rename_group(old, to)?;
                self.pins.commit();
                return Ok(json!({"group":renamed,"current":self.pins.board.borrow().group}));
            }
            "set-group-color" => {
                let name = request["argument"]
                    .as_str()
                    .ok_or_else(|| t("Invalid desktop response"))?;
                let color = request["color"].as_str();
                self.pins.board.borrow_mut().set_group_color(name, color)?;
                self.pins.commit();
                return Ok(json!({"ok":true}));
            }
            "reorder-groups" => {
                let order: Vec<String> = serde_json::from_value(request["groups"].clone())
                    .map_err(|_| t("Invalid pin group order"))?;
                self.pins.board.borrow_mut().reorder_groups(order)?;
                self.pins.commit();
                return Ok(json!({"groups":self.pins.board.borrow().groups}));
            }
            "move-to-group" => {
                let target = request["argument"]
                    .as_str()
                    .ok_or_else(|| t("Invalid desktop response"))?;
                let ids: Vec<u64> = serde_json::from_value(request["ids"].clone())
                    .map_err(|_| t("Invalid desktop response"))?;
                let moved = self.pins.move_to_group(&ids, target)?;
                return Ok(json!({"moved":moved}));
            }
            "remove-empty-groups" => {
                let removed = self.pins.board.borrow_mut().remove_empty_groups();
                if !removed.is_empty() {
                    self.pins.commit();
                }
                return Ok(json!({"removed":removed,"groups":self.pins.board.borrow().groups}));
            }
            "delete-group" => {
                if self.busy.replace(true) {
                    return Err(t("A capture or import session is already in progress"));
                }
                let _busy = Busy(&self.busy);
                let target = request["argument"]
                    .as_str()
                    .ok_or_else(|| t("Invalid desktop response"))?
                    .to_owned();
                let migrate_to = request["move_to"].as_str().map(str::to_owned);
                if migrate_to.is_none() && request["confirmed"] != true {
                    let count = self
                        .pins
                        .board
                        .borrow()
                        .pins
                        .iter()
                        .filter(|p| p.group == target)
                        .count();
                    if count > 1
                        && self.pins.preferences.borrow().confirm_destroy
                        && !super::confirmation::pins(true, count, &self.work_cancel()).await?
                    {
                        return Ok(json!({"cancelled":true}));
                    }
                }
                let was_current = self.pins.board.borrow().group == target;
                let (migrated, destroyed) = self
                    .pins
                    .board
                    .borrow_mut()
                    .remove_group(&target, migrate_to.as_deref())?;
                if !destroyed.is_empty() {
                    self.pins.release_destroyed(&destroyed);
                }
                if was_current {
                    self.pins.toolbar(None);
                }
                self.pins.commit();
                if was_current {
                    self.pins.group_changed();
                }
                return Ok(json!({
                    "deleted": target,
                    "migrated": migrated,
                    "destroyed": destroyed.len(),
                    "current": self.pins.board.borrow().group
                }));
            }
            "switch-group" | "switch-group-prev" => {
                let previous = self.pins.board.borrow().previous.clone();
                self.pins
                    .board
                    .borrow_mut()
                    .switch_group(if command == "switch-group-prev" {
                        Some(&previous)
                    } else {
                        request["argument"].as_str()
                    })?;
                self.pins.toolbar(None);
                self.pins.commit();
                self.pins.group_changed();
                return Ok(json!({"group":self.pins.board.borrow().group}));
            }
            "enable-hotkeys" | "disable-hotkeys" | "toggle-hotkeys" => {
                return Err(t(
                    "Bind clivuu commands in the desktop or compositor shortcut settings",
                ));
            }
            "snip" | "paste" | "reopen-capture" | "record" | "scroll" | "barcode-decode"
            | "barcode-scan" | "pick-color" => {}
            _ => {
                return Err(tf(
                    "Unknown command: {0}; run clivuu --help for usage",
                    &[command],
                ));
            }
        }
        if command == "paste"
            && self.busy.get()
            && matches!(
                clivuu_core::paste::Request::parse(&request)?.source,
                clivuu_core::paste::Source::Clipboard
            )
            && self.capture_commands.pin()
        {
            return Ok(json!({"accepted":true,"action":"pin-selection"}));
        }
        if self.busy.replace(true) {
            return Err(t("A capture or import session is already in progress"));
        }
        let _busy = Busy(&self.busy);
        let operation = Scope::new(&self.work_cancel());
        self.operation.replace(Some(operation.token.clone()));
        let _operation = Operation(&self.operation);

        let configuration = self
            .configuration
            .borrow()
            .clone()
            .ok_or_else(|| t("The desktop service is unavailable"))?;
        self.preferences.command("prepare-capture").await?;
        self.group_manager.command("prepare-capture").await?;
        let settings = configuration.current().await?;
        clivuu_core::i18n::initialize(settings["language"].as_str());
        self.pins.settings(&settings);
        self.memory
            .set(settings["pin-storage"].as_str() == Some("memory"));
        if command == "pick-color" {
            let color = crate::platform::portal::pick_color(self.work_cancel()).await?;
            let color = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
            display.clipboard().set_text(&color);
            return Ok(json!({"color":color}));
        }
        if command == "barcode-decode" {
            let path = request["argument"]
                .as_str()
                .ok_or_else(|| t("barcode-decode requires a file path"))?
                .to_owned();
            let task = self
                .pool
                .submit(move |_| {
                    let path = std::fs::canonicalize(path)
                        .map_err(|e| TaskError::Failed(e.to_string()))?;
                    clivuu_core::image_ops::load(
                        path.to_str()
                            .ok_or_else(|| TaskError::Failed(t("Invalid desktop response")))?,
                    )
                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                    Ok(path)
                })
                .map_err(|e| e.to_string())?;
            let path = task.result().await.map_err(|e| e.to_string())?;
            let text = crate::infrastructure::process::output(
                &["zbarimg", "--quiet", "--raw", path.to_str().unwrap()],
                &self.work_cancel(),
            )
            .await?;
            display.clipboard().set_text(&text);
            return Ok(json!({"text":text}));
        }
        if command == "reopen-capture" {
            let index = match request.get("argument") {
                None | Some(Value::Null) => 0,
                Some(Value::String(s)) => s
                    .parse::<usize>()
                    .map_err(|_| t("No capture at this history index"))?,
                Some(value) => value
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| t("No capture at this history index"))?,
            };
            let record = {
                let history = self.history.borrow();
                if history.is_empty() {
                    return Err(t("No capture history yet"));
                }
                history
                    .iter()
                    .rev()
                    .nth(index)
                    .cloned()
                    .ok_or_else(|| t("No capture at this history index"))?
            };
            let rect = record.rect;
            let existing = clivuu_core::paste::existing(&self.pins.board.borrow(), &record.source);
            if let Some(id) = existing {
                let restored = self.pins.reveal(id, &self.operation_cancel()).await?;
                return Ok(json!({"pinned":restored,"rect":rect,"id":id,"reused":restored}));
            }
            let png = record.png;
            let task = self
                .pool
                .submit(move |_| {
                    clivuu_core::image_ops::decode(&png)
                        .map_err(|e| TaskError::Failed(e.to_string()))
                })
                .map_err(|e| e.to_string())?;
            let image = crate::editor::frozen_image::FrozenImage::new(
                task.result().await.map_err(|e| e.to_string())?,
            )
            .ok_or_else(|| t("Invalid saved pin data"))?;
            let id = self.pin_capture(image, rect, record.source)?;
            return Ok(json!({"pinned":true,"rect":rect,"id":id,"reused":false}));
        }
        if command == "paste" {
            return self.paste(&request).await;
        }
        if command == "scroll" {
            return self.scroll(&request, &settings).await;
        }
        if command == "record" {
            return self.record(&request, &settings).await;
        }
        self.capture(&request, &settings, configuration, &display)
            .await
    }
}

pub(crate) fn recording_appearance(
    settings: &serde_json::Map<String, Value>,
) -> crate::ui::recording::Appearance {
    let mut opts = options(settings);
    opts.display.accent = gdk::RGBA::parse(&opts.accent).unwrap_or(opts.display.accent);
    crate::ui::recording::Appearance {
        dark: opts.dark,
        accent: opts.accent,
        display: opts.display,
    }
}

pub(crate) fn options(settings: &serde_json::Map<String, Value>) -> capture::Options {
    let mut options = capture::Options {
        dark: match settings["theme"].as_str() {
            Some("dark") => true,
            Some("light") => false,
            _ => adw::StyleManager::default().is_dark(),
        },
        accent: settings["accent-color"]
            .as_str()
            .unwrap_or("#bd93f9")
            .into(),
        ..Default::default()
    };
    options.detect_windows = settings["capture-detect-windows"].as_bool().unwrap_or(true);
    options.hide_single = settings["capture-hide-1x1"].as_bool().unwrap_or(true);
    options.history_loop = settings["capture-history-loop"].as_bool().unwrap_or(false);
    options.display.border_width = settings["capture-border-width"].as_f64().unwrap_or(2.);
    options.display.mask = settings["capture-mask-color"]
        .as_str()
        .and_then(|color| gdk::RGBA::parse(color).ok())
        .unwrap_or(options.display.mask);
    options.display.anchor = gdk::RGBA::parse(
        settings["capture-anchor-color"]
            .as_str()
            .unwrap_or("#ffffff"),
    )
    .unwrap_or(gdk::RGBA::WHITE);
    options.display.anchors = settings["capture-show-anchors"].as_bool().unwrap_or(true);
    options.display.magnifier_mask = settings["capture-magnifier-mask"].as_bool().unwrap_or(true);
    options.display.magnifier_border = settings["capture-magnifier-border"]
        .as_bool()
        .unwrap_or(true);
    options.display.magnifier_anchors = settings["capture-magnifier-anchors"]
        .as_bool()
        .unwrap_or(false);
    options.display.crosshair = settings["capture-crosshair"].as_bool().unwrap_or(false);
    options.display.guides = !settings["capture-disable-guides"]
        .as_bool()
        .unwrap_or(false);
    options.display.tips = settings["capture-show-tips"].as_bool().unwrap_or(true);
    options.ocr_settings = clivuu_core::ocr::OcrSettings::from_values(settings);
    options
}
