//! Normal preferences windows have their own GTK display connection. In a
//! Wayland session they never inherit the pin service's XWayland backend.
//! The same executable/UI/configuration is reused; D-Bus coordinates capture.
use crate::{
    infrastructure::{configuration::Configuration, tasks::TaskPool},
    ui::preferences::Preferences,
};
use clivuu_core::{config, i18n::t};
use gtk::{gio, glib, prelude::*};
use std::{cell::RefCell, rc::Rc};

const BUS: &str = "io.github.naerelyth.clivuu.Preferences";
const PATH: &str = "/io/github/naerelyth/clivuu/Preferences";
const INTERFACE: &str = "io.github.naerelyth.clivuu.Preferences1";

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
    pub async fn command(&self, command: &str) -> Result<(), String> {
        if self.running().await?
            && let Err(e) = self.call(command).await
            && !endpoint_absent(&e)
        {
            return Err(e.to_string());
        }
        Ok(())
    }
    async fn call(&self, command: &str) -> Result<(), glib::Error> {
        self.0
            .call_future(
                Some(BUS),
                PATH,
                INTERFACE,
                "Command",
                Some(&(command,).to_variant()),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                10000,
            )
            .await?;
        Ok(())
    }
    pub async fn open(&self) -> Result<(), String> {
        self.open_target("present").await
    }
    pub async fn open_page(&self, page: &str) -> Result<(), String> {
        self.open_target(&format!("present:{page}")).await
    }
    async fn open_target(&self, target: &str) -> Result<(), String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut exited = None;
        loop {
            // GApplication owns its name before exporting our interface, and
            // releases it after unregistering on shutdown. Wait through both
            // transitions; never send a new process to a closing instance.
            if self.running().await? {
                match self.call(target).await {
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
                std::ffi::OsStr::new("--preferences"),
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

struct State {
    app: glib::WeakRef<gtk::Application>,
    config: Rc<Configuration>,
    window: RefCell<Option<Rc<Preferences>>>,
}
impl State {
    fn new(app: &gtk::Application) -> Result<Rc<Self>, String> {
        let pool = Rc::new(TaskPool::new(1, 4).map_err(|e| e.to_string())?);
        let values = config::read().map_err(|e| e.to_string())?;
        let config = Configuration::new(values, pool, |e| eprintln!("{e}"))?;
        Ok(Rc::new(Self {
            app: app.downgrade(),
            config,
            window: RefCell::new(None),
        }))
    }
    fn register(
        self: &Rc<Self>,
        connection: &gio::DBusConnection,
    ) -> Result<gio::RegistrationId, String> {
        let info = gio::DBusNodeInfo::for_xml(&format!(
            "<node><interface name='{INTERFACE}'><method name='Command'><arg type='s' direction='in'/></method></interface></node>"
        )).map_err(|e| e.to_string())?.lookup_interface(INTERFACE).unwrap();
        let weak = Rc::downgrade(self);
        connection
            .register_object(PATH, &info)
            .method_call(move |_, _, _, _, _, parameters, invocation| {
                let Some(this) = weak.upgrade() else { return };
                let hold = this.app.upgrade().unwrap().hold();
                let command = parameters.get::<(String,)>().unwrap().0;
                glib::MainContext::default().spawn_local(async move {
                    match this.command(&command).await {
                        Ok(()) => invocation.return_value(None),
                        Err(e) => invocation.return_dbus_error(&format!("{INTERFACE}.Error"), &e),
                    }
                    drop(hold);
                });
            })
            .build()
            .map_err(|e| e.to_string())
    }
    fn present(self: &Rc<Self>) {
        if self.window.borrow().is_none() {
            let Some(app) = self.app.upgrade() else {
                return;
            };
            let preferences = Preferences::new(
                &app,
                self.config.clone(),
                super::editor_services::color_services(),
            );
            let weak = Rc::downgrade(self);
            preferences.window.connect_close_request(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.window.take();
                    // A close may precede the coalesced write timer. Keep the
                    // process alive until edits are committed to the shared file.
                    if let Some(app) = this.app.upgrade() {
                        let hold = app.hold();
                        glib::MainContext::default().spawn_local(async move {
                            if let Err(e) = this.config.flush().await {
                                eprintln!("{e}");
                            }
                            drop(hold);
                        });
                    }
                }
                glib::Propagation::Proceed
            });
            self.window.replace(Some(preferences));
        }
        if let Some(window) = self.window.borrow().as_ref() {
            window.present();
        }
    }
    async fn command(self: &Rc<Self>, command: &str) -> Result<(), String> {
        if let Some(page) = command.strip_prefix("present:") {
            self.present();
            if let Some(window) = self.window.borrow().as_ref() {
                window.select_page(page);
            }
            return Ok(());
        }
        match command {
            "present" => self.present(),
            "prepare-capture" => {
                if let Some(window) = self.window.borrow().as_ref() {
                    window.dismiss_popovers();
                }
                self.config.flush().await?;
                // Complete popup unmapping before the other process acquires
                // screenshot input. GDK supplies the native display roundtrip.
                if let Some(display) = gtk::gdk::Display::default() {
                    display.sync();
                }
            }
            "close" => {
                let window = self.window.borrow().clone();
                if let Some(window) = window {
                    window.window.close();
                }
                self.config.flush().await?;
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
        let setup = State::new(app).and_then(|this| {
            let registration = this.register(&app.dbus_connection().unwrap())?;
            exported.replace(Some(registration));
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
    let slot = state.clone();
    app.connect_activate(move |_| {
        if let Some(this) = slot.borrow().as_ref() {
            this.present();
        }
    });
    app.connect_shutdown(move |app| {
        if let Some(id) = registration.take() {
            let _ = app.dbus_connection().unwrap().unregister_object(id);
        }
        state.take();
    });
    // Role arguments are internal, not GTK command-line options.
    app.run_with_args(&["clivuu-preferences"])
}
