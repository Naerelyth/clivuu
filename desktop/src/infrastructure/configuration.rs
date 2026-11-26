//! One live XDG configuration for CLI and GTK. Widgets publish immediately;
//! grouped, non-default file writes are coalesced and run off the GTK thread.
use super::tasks::{TaskError, TaskPool};
use clivuu_core::config;
use gtk::{gio, glib, prelude::*};
use serde_json::{Map, Value};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
pub type Values = Map<String, Value>;
type Listener = dyn Fn(&Values);
pub struct Subscription {
    _callback: Rc<dyn Fn(&Values)>,
}
pub struct Configuration {
    values: RefCell<Values>,
    pending: RefCell<Values>,
    pool: Rc<TaskPool>,
    writing: Cell<bool>,
    epoch: Cell<u64>,
    reading: Cell<bool>,
    reload_wanted: Cell<bool>,
    reload_error: RefCell<Option<String>>,
    timer: RefCell<Option<glib::SourceId>>,
    listeners: RefCell<Vec<Weak<Listener>>>,
    _monitor: gio::FileMonitor,
    system: RefCell<Option<(adw::StyleManager, glib::SignalHandlerId)>>,
    error: Rc<dyn Fn(String)>,
}
impl Drop for Configuration {
    fn drop(&mut self) {
        if let Some(id) = self.timer.take() {
            id.remove();
        }
        self._monitor.cancel();
        if let Some((manager, id)) = self.system.take() {
            manager.disconnect(id);
        }
    }
}
impl Configuration {
    pub fn new(
        values: Values,
        pool: Rc<TaskPool>,
        error: impl Fn(String) + 'static,
    ) -> Result<Rc<Self>, String> {
        let path = config::path().map_err(|e| e.to_string())?;
        let monitor = gio::File::for_path(path.parent().unwrap())
            .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
            .map_err(|e| e.to_string())?;
        let this = Rc::new(Self {
            values: RefCell::new(values),
            pending: RefCell::new(Map::new()),
            pool,
            writing: Cell::new(false),
            epoch: Cell::new(0),
            reading: Cell::new(false),
            reload_wanted: Cell::new(false),
            reload_error: RefCell::new(None),
            timer: RefCell::new(None),
            listeners: RefCell::new(vec![]),
            _monitor: monitor,
            system: RefCell::new(None),
            error: Rc::new(error),
        });
        let weak = Rc::downgrade(&this);
        this._monitor.connect_changed(move |_, file, other, _| {
            if (file
                .basename()
                .is_some_and(|s| s == std::path::Path::new("config.ini"))
                || other
                    .and_then(|f| f.basename())
                    .is_some_and(|s| s == std::path::Path::new("config.ini")))
                && let Some(this) = weak.upgrade()
            {
                this.reload();
            }
        });
        let manager = adw::StyleManager::default();
        let weak = Rc::downgrade(&this);
        let id = manager.connect_dark_notify(move |_| {
            if let Some(this) = weak.upgrade()
                && this.value("theme").as_str() == Some("system")
            {
                this.notify();
            }
        });
        this.system.replace(Some((manager, id)));
        this.appearance();
        Ok(this)
    }
    pub fn pool(&self) -> Rc<TaskPool> {
        self.pool.clone()
    }
    pub fn values(&self) -> Values {
        self.values.borrow().clone()
    }
    pub fn value(&self, key: &str) -> Value {
        self.values
            .borrow()
            .get(key)
            .cloned()
            .unwrap_or(Value::Null)
    }
    pub fn subscribe(&self, f: impl Fn(&Values) + 'static) -> Subscription {
        let callback: Rc<dyn Fn(&Values)> = Rc::new(f);
        self.listeners.borrow_mut().push(Rc::downgrade(&callback));
        Subscription {
            _callback: callback,
        }
    }

    fn appearance(&self) {
        let values = self.values.borrow();
        clivuu_core::i18n::initialize(values["language"].as_str());
        adw::StyleManager::default().set_color_scheme(match values["theme"].as_str() {
            Some("light") => adw::ColorScheme::ForceLight,
            Some("dark") => adw::ColorScheme::ForceDark,
            _ => adw::ColorScheme::Default,
        });
    }
    fn notify(&self) {
        self.appearance();
        let values = self.values();
        let callbacks: Vec<_> = {
            let mut ls = self.listeners.borrow_mut();
            ls.retain(|w| w.strong_count() > 0);
            ls.iter().filter_map(Weak::upgrade).collect()
        };
        for f in callbacks {
            f(&values);
        }
    }
    pub fn set(self: &Rc<Self>, key: &str, value: Value) -> Result<(), String> {
        let value = config::normalize(key, value).map_err(|e| e.to_string())?;
        if self.value(key) == value {
            return Ok(());
        }
        self.values.borrow_mut().insert(key.into(), value.clone());
        self.pending.borrow_mut().insert(key.into(), value);
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.notify();
        self.schedule();
        Ok(())
    }
    pub fn reset(self: &Rc<Self>, keys: &[String]) {
        let schema = config::schema();
        for key in keys {
            if let Some(spec) = schema.get(key) {
                let _ = self.set(key, spec["default"].clone());
            }
        }
    }
    fn schedule(self: &Rc<Self>) {
        if let Some(id) = self.timer.borrow_mut().take() {
            id.remove();
        }
        let weak = Rc::downgrade(self);
        self.timer.replace(Some(glib::timeout_add_local_once(
            std::time::Duration::from_millis(120),
            move || {
                if let Some(this) = weak.upgrade() {
                    this.timer.borrow_mut().take();
                    glib::MainContext::default().spawn_local(async move {
                        if let Err(e) = this.flush().await {
                            (this.error)(e);
                        }
                    });
                }
            },
        )));
    }
    pub async fn flush(self: &Rc<Self>) -> Result<(), String> {
        while self.writing.get() {
            glib::timeout_future(std::time::Duration::from_millis(10)).await;
        }
        if self.pending.borrow().is_empty() {
            return Ok(());
        }
        self.writing.set(true);
        while !self.pending.borrow().is_empty() {
            let changes = self.pending.take();
            let batch = changes.clone();
            let task = self.pool.submit(move |_| {
                config::path()
                    .and_then(|p| config::apply_file(&p, batch))
                    .map_err(|e| TaskError::Failed(e.to_string()))
            });
            let result = match task {
                Ok(task) => task.result().await.map_err(|e| e.to_string()),
                Err(TaskError::Busy) => {
                    let newer = self.pending.take();
                    let mut pending = changes;
                    pending.extend(newer);
                    self.pending.replace(pending);
                    glib::timeout_future(std::time::Duration::from_millis(75)).await;
                    continue;
                }
                Err(e) => Err(e.to_string()),
            };
            if let Err(e) = result {
                let newer = self.pending.take();
                let mut pending = changes;
                pending.extend(newer);
                self.pending.replace(pending);
                self.writing.set(false);
                return Err(e);
            }
        }
        self.writing.set(false);
        self.reload();
        Ok(())
    }
    pub async fn current(self: &Rc<Self>) -> Result<Values, String> {
        loop {
            self.flush().await?;
            self.reload();
            while self.reading.get() || self.reload_wanted.get() {
                glib::timeout_future(std::time::Duration::from_millis(10)).await;
            }
            if let Some(error) = self.reload_error.borrow().as_ref() {
                return Err(error.clone());
            }
            if !self.writing.get() && self.pending.borrow().is_empty() {
                return Ok(self.values());
            }
        }
    }
    fn reload(self: &Rc<Self>) {
        if self.writing.get() || !self.pending.borrow().is_empty() {
            return;
        }
        if self.reading.replace(true) {
            self.reload_wanted.set(true);
            return;
        }
        let epoch = self.epoch.get();
        let weak = Rc::downgrade(self);
        let task = match self
            .pool
            .submit(|_| config::read().map_err(|e| TaskError::Failed(e.to_string())))
        {
            Ok(task) => task,
            Err(TaskError::Busy) => {
                glib::timeout_add_local_once(std::time::Duration::from_millis(75), move || {
                    if let Some(this) = weak.upgrade() {
                        this.reading.set(false);
                        this.reload();
                    }
                });
                return;
            }
            Err(e) => {
                self.reading.set(false);
                self.reload_wanted.set(false);
                self.reload_error.replace(Some(e.to_string()));
                (self.error)(e.to_string());
                return;
            }
        };
        glib::MainContext::default().spawn_local(async move {
            let result = task.result().await;
            let Some(this) = weak.upgrade() else { return };
            this.reading.set(false);
            let newer = this.reload_wanted.replace(false);
            if !newer
                && epoch == this.epoch.get()
                && !this.writing.get()
                && this.pending.borrow().is_empty()
            {
                match result {
                    Ok(values) => {
                        this.reload_error.replace(None);
                        if values != *this.values.borrow() {
                            this.epoch.set(this.epoch.get().wrapping_add(1));
                            this.values.replace(values);
                            this.notify();
                        }
                    }
                    Err(e) => {
                        this.reload_error.replace(Some(e.to_string()));
                        (this.error)(e.to_string());
                    }
                }
            }
            // A newer file event supersedes this read; read again only after its
            // worker has finished. Completions can never run out of order.
            if newer {
                this.reload();
            }
        });
    }
}
