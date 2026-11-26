//! Native pin-group windows (Manager, Create Group, Quick Switcher) run in an
//! independent GTK process so Wayland sessions never inherit XWayland pin hosts.
use crate::{
    infrastructure::{configuration::Configuration, tasks::TaskPool},
    ui::group_manager::{GroupWindows, Mode},
};
use clivuu_core::{config, i18n::t, protocol};
use gtk::{gio, glib, prelude::*};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

const BUS: &str = "io.github.naerelyth.clivuu.Groups";
const PATH: &str = "/io/github/naerelyth/clivuu/Groups";
const INTERFACE: &str = "io.github.naerelyth.clivuu.Groups1";

#[derive(Clone, Debug)]
pub enum OpenMode {
    Manage,
    Create { move_ids: Vec<u64> },
    Switcher,
}

pub struct Client(gio::DBusConnection);

impl Client {
    pub fn new(connection: &gio::DBusConnection) -> Self {
        Self(connection.clone())
    }

    async fn running(&self) -> Result<bool, String> {
        self.0
            .call_future(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "NameHasOwner",
                Some(&(BUS,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                5000,
            )
            .await
            .map_err(|e| e.to_string())?
            .get::<(bool,)>()
            .map(|v| v.0)
            .ok_or_else(|| t("Invalid desktop response"))
    }

    pub async fn command(&self, action: &str) -> Result<(), String> {
        let payload = json!({ "action": action }).to_string();
        if self.running().await?
            && let Err(e) = self.call(&payload).await
            && !endpoint_absent(&e)
        {
            return Err(e.to_string());
        }
        Ok(())
    }

    pub fn notify_refresh(&self) {
        let client = Self(self.0.clone());
        glib::MainContext::default().spawn_local(async move {
            let _ = client.command("refresh").await;
        });
    }

    async fn call(&self, payload: &str) -> Result<(), glib::Error> {
        self.0
            .call_future(
                Some(BUS),
                PATH,
                INTERFACE,
                "Command",
                Some(&(payload,).to_variant()),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                10000,
            )
            .await?;
        Ok(())
    }

    pub async fn open(&self, mode: OpenMode) -> Result<(), String> {
        let payload = match mode {
            OpenMode::Manage => json!({ "action": "present", "mode": "manage" }),
            OpenMode::Create { move_ids } => {
                json!({ "action": "present", "mode": "create", "moveIds": move_ids })
            }
            OpenMode::Switcher => json!({ "action": "present", "mode": "switcher" }),
        }
        .to_string();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut exited = None;
        loop {
            if self.running().await? {
                match self.call(&payload).await {
                    Ok(()) => return Ok(()),
                    Err(e) if endpoint_absent(&e) => {}
                    Err(e) => return Err(e.to_string()),
                }
            } else if exited.is_none() {
                exited = Some(self.launch()?);
            } else if exited.as_ref().is_some_and(|done| done.get()) {
                return Err(t("The desktop service is unavailable"));
            }
            if std::time::Instant::now() >= deadline {
                return Err(t("The desktop service is unavailable"));
            }
            glib::timeout_future(std::time::Duration::from_millis(20)).await;
        }
    }

    fn launch(&self) -> Result<Rc<std::cell::Cell<bool>>, String> {
        let launcher = crate::platform::native_ui::launcher(gio::SubprocessFlags::NONE);
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let child = launcher
            .spawn(&[
                executable.as_os_str(),
                std::ffi::OsStr::new("--group-manager"),
            ])
            .map_err(|e| e.to_string())?;
        let exited = Rc::new(std::cell::Cell::new(false));
        let done = exited.clone();
        child.wait_async(gio::Cancellable::NONE, move |_| done.set(true));
        Ok(exited)
    }
}

fn endpoint_absent(error: &glib::Error) -> bool {
    [
        gio::DBusError::ServiceUnknown,
        gio::DBusError::NameHasNoOwner,
        gio::DBusError::UnknownMethod,
        gio::DBusError::UnknownObject,
    ]
    .into_iter()
    .any(|kind| error.matches(kind))
}

pub async fn desktop_execute(
    connection: &gio::DBusConnection,
    mut request: Value,
) -> Result<Value, String> {
    request["protocol"] = json!(protocol::VERSION);
    let reply = connection
        .call_future(
            Some(protocol::DESKTOP_DESTINATION),
            protocol::OBJECT_PATH,
            protocol::INTERFACE,
            "Execute",
            Some(&(request.to_string(),).to_variant()),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            10000,
        )
        .await
        .map_err(|mut e| {
            gio::DBusError::strip_remote_error(&mut e);
            let msg = e.message();
            msg.strip_prefix("GDBus.Error:")
                .and_then(|rest| rest.split_once(": "))
                .map_or_else(|| msg.to_string(), |(_, body)| body.to_string())
        })?;
    let raw = reply
        .get::<(String,)>()
        .map(|v| v.0)
        .ok_or_else(|| t("Invalid desktop response"))?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

struct State {
    app: glib::WeakRef<gtk::Application>,
    windows: Rc<GroupWindows>,
    startup_hold: RefCell<Option<gio::ApplicationHoldGuard>>,
}

impl State {
    fn new(app: &gtk::Application, connection: &gio::DBusConnection) -> Result<Rc<Self>, String> {
        let pool = Rc::new(TaskPool::new(1, 2).map_err(|e| e.to_string())?);
        let values = config::read().map_err(|e| e.to_string())?;
        clivuu_core::i18n::initialize(values["language"].as_str());
        let config = Configuration::new(values, pool, |e| eprintln!("{e}"))?;
        let windows = GroupWindows::new(app, connection.clone(), config);
        Ok(Rc::new(Self {
            app: app.downgrade(),
            windows,
            startup_hold: RefCell::new(Some(app.hold())),
        }))
    }

    fn register(
        self: &Rc<Self>,
        connection: &gio::DBusConnection,
    ) -> Result<gio::RegistrationId, String> {
        let info = gio::DBusNodeInfo::for_xml(&format!(
            "<node><interface name='{INTERFACE}'><method name='Command'><arg type='s' direction='in'/></method></interface></node>"
        ))
        .map_err(|e| e.to_string())?
        .lookup_interface(INTERFACE)
        .unwrap();
        let weak = Rc::downgrade(self);
        connection
            .register_object(PATH, &info)
            .method_call(move |conn, _, _, _, _, parameters, invocation| {
                let Some(this) = weak.upgrade() else { return };
                let Some(app) = this.app.upgrade() else {
                    return;
                };
                let hold = app.hold();
                let conn = conn.clone();
                let payload = parameters
                    .get::<(String,)>()
                    .map(|v| v.0)
                    .unwrap_or_default();
                glib::MainContext::default().spawn_local(async move {
                    match this.handle(&payload).await {
                        Ok(()) => invocation.return_value(None),
                        Err(e) => invocation.return_dbus_error(&format!("{INTERFACE}.Error"), &e),
                    }
                    let _ = conn.flush_future().await;
                    drop(hold);
                });
            })
            .build()
            .map_err(|e| e.to_string())
    }

    async fn handle(self: &Rc<Self>, raw: &str) -> Result<(), String> {
        let parsed: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
        match parsed["action"].as_str().unwrap_or("") {
            "present" => {
                let mode = match parsed["mode"].as_str().unwrap_or("manage") {
                    "create" => {
                        let move_ids = parsed["moveIds"]
                            .as_array()
                            .map(|arr| arr.iter().filter_map(Value::as_u64).collect())
                            .unwrap_or_default();
                        Mode::Create { move_ids }
                    }
                    "switcher" => Mode::Switcher,
                    _ => Mode::Manage,
                };
                self.windows.present(mode);
                self.startup_hold.borrow_mut().take();
            }
            "refresh" => {
                self.windows.schedule_reload();
            }
            "prepare-capture" => {
                self.windows.prepare_capture();
                if let Some(display) = gtk::gdk::Display::default() {
                    display.sync();
                }
            }
            "close" => {
                self.windows.close_all();
                self.startup_hold.borrow_mut().take();
            }
            _ => return Err(t("Invalid desktop response")),
        }
        Ok(())
    }
}

pub fn run() -> glib::ExitCode {
    if let Err(e) = crate::platform::native_ui::initialize() {
        eprintln!("{e}");
        return glib::ExitCode::FAILURE;
    }
    let app = gtk::Application::builder().application_id(BUS).build();
    let state: Rc<RefCell<Option<Rc<State>>>> = Rc::new(RefCell::new(None));
    let registration = Rc::new(RefCell::new(None));
    let slot = state.clone();
    let exported = registration.clone();
    app.connect_startup(move |app| {
        let Some(connection) = app.dbus_connection() else {
            app.quit();
            return;
        };
        let setup = State::new(app, &connection).and_then(|this| {
            let reg = this.register(&connection)?;
            exported.replace(Some(reg));
            let weak = Rc::downgrade(&this);
            glib::timeout_add_local_once(std::time::Duration::from_secs(4), move || {
                if let Some(this) = weak.upgrade() {
                    this.startup_hold.borrow_mut().take();
                }
            });
            Ok(this)
        });
        match setup {
            Ok(this) => {
                slot.replace(Some(this));
            }
            Err(e) => {
                eprintln!("{e}");
                app.quit();
            }
        }
    });
    app.connect_activate(|_| {});
    app.connect_shutdown(move |app| {
        if let Some(id) = registration.take()
            && let Some(conn) = app.dbus_connection()
        {
            let _ = conn.unregister_object(id);
        }
        state.take();
    });
    app.run_with_args(&["clivuu-groups"])
}
