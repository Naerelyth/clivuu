//! GNOME Shell (`appindicatorsupport` + `Main.uiGroup`) tray host quirks.
//!
//! Why this adapter exists only for GNOME:
//! 1. GNOME lacks `wlr-layer-shell`, so Clivuu's capture selector runs on XWayland
//!    (`global.window_group`), stacked below GNOME Shell's `Main.uiGroup` panels
//!    and `PopupMenu`s (`Main.pushModal`).
//! 2. `appindicatorsupport` tracks clicks modulo 2 (`_clickCount = (_clickCount % 2) + 1`):
//!    Click 2 sends `Activate`, while Click 3 (or 5) starts an internal 400 ms
//!    `_waitForDoubleClick` timer that calls `PopupMenu.open()` *after* the capture
//!    overlay has already mapped, stealing modal input and resetting the cursor.
//! 3. `appindicatorsupport`'s `dbusMenu.js` ignores `LayoutUpdated` while closed,
//!    but `Gio.DBusProxy` handles `PropertiesChanged("Menu")` immediately, calling
//!    `PopupMenu.removeAll()` so `PopupMenu.isEmpty()` blocks `open()` without
//!    hiding or flickering the panel icon.
//!
//! Native Wayland compositors (`KDE`, `niri`, `Hyprland`, `sway`) and non-GNOME
//! X11 panels do not instantiate this adapter and keep immediate `Activate`
//! dispatch with a static `/MenuBar` object path.
use super::{ITEM, MENU, MENU_PATH};
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

pub(super) const EMPTY_MENU_PATH: &str = "/MenuBarEmpty";

pub struct CaptureMenuLock {
    host: Rc<GnomeHost>,
    refresh: Rc<dyn Fn()>,
}

impl Drop for CaptureMenuLock {
    fn drop(&mut self) {
        let prev = self.host.is_menu_suppressed();
        self.host
            .capture_locks
            .set(self.host.capture_locks.get().saturating_sub(1));
        if self.host.sync_menu_suppression(prev) {
            (self.refresh)();
        }
    }
}

pub(super) struct GnomeHost {
    connection: gio::DBusConnection,
    registration: Cell<Option<gio::RegistrationId>>,
    activate_suppressed: Cell<bool>,
    capture_locks: Cell<u32>,
    passive_pulse: Cell<bool>,
    passive_timer: RefCell<Option<glib::SourceId>>,
}

impl Drop for GnomeHost {
    fn drop(&mut self) {
        if let Some(id) = self.passive_timer.borrow_mut().take() {
            id.remove();
        }
        if let Some(id) = self.registration.take() {
            let _ = self.connection.unregister_object(id);
        }
    }
}

impl GnomeHost {
    pub fn new(
        connection: &gio::DBusConnection,
        revision: &Cell<u32>,
        icons: &str,
    ) -> Result<Rc<Self>, String> {
        let this = Rc::new(Self {
            connection: connection.clone(),
            registration: Cell::new(None),
            activate_suppressed: Cell::new(false),
            capture_locks: Cell::new(0),
            passive_pulse: Cell::new(false),
            passive_timer: RefCell::new(None),
        });
        let info = gio::DBusNodeInfo::for_xml(include_str!("../../../../resources/dbus-menu.xml"))
            .map_err(|e| e.to_string())?
            .lookup_interface(MENU)
            .unwrap();
        let rev = revision.get();
        let icon_dir = icons.to_owned();
        let id =
            connection
                .register_object(EMPTY_MENU_PATH, &info)
                .method_call(move |_, _, _, _, method, args, invocation| {
                    match Self::empty_menu_method(rev, method, &args) {
                        Some(v) => invocation.return_value(Some(&v)),
                        None => invocation
                            .return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", method),
                    }
                })
                .property(move |_, _, _, _, name| match name {
                    "Version" => 4u32.to_variant(),
                    "TextDirection" => "ltr".to_variant(),
                    "Status" => "normal".to_variant(),
                    "IconThemePath" => vec![icon_dir.as_str()].to_variant(),
                    _ => "".to_variant(),
                })
                .build()
                .map_err(|e| e.to_string())?;
        this.registration.set(Some(id));
        Ok(this)
    }

    pub fn has_active_capture(&self) -> bool {
        self.capture_locks.get() > 0
    }

    pub fn is_menu_suppressed(&self) -> bool {
        self.activate_suppressed.get() || self.capture_locks.get() > 0
    }

    pub fn status(&self) -> &'static str {
        if self.passive_pulse.get() {
            "Passive"
        } else {
            "Active"
        }
    }

    pub fn menu_path(&self) -> &'static str {
        if self.is_menu_suppressed() {
            EMPTY_MENU_PATH
        } else {
            MENU_PATH
        }
    }

    pub fn reset(&self) {
        self.activate_suppressed.set(false);
        self.passive_pulse.set(false);
        if let Some(id) = self.passive_timer.borrow_mut().take() {
            id.remove();
        }
    }

    pub fn on_activate(&self, menu_open: bool) -> bool {
        if menu_open || self.capture_locks.get() > 0 {
            return false;
        }
        let prev = self.is_menu_suppressed();
        self.activate_suppressed.set(true);
        self.sync_menu_suppression(prev)
    }

    pub fn on_capture_dispatched(&self) -> bool {
        let prev = self.is_menu_suppressed();
        self.activate_suppressed.set(false);
        self.sync_menu_suppression(prev)
    }

    /// Returns `(ignore_open, needs_refresh)`.
    pub fn on_menu_opened(self: &Rc<Self>, menu_open: &Cell<bool>) -> (bool, bool) {
        if self.capture_locks.get() > 0 {
            menu_open.set(false);
            self.pulse_passive();
            return (true, false);
        }
        let prev = self.is_menu_suppressed();
        self.activate_suppressed.set(false);
        (false, self.sync_menu_suppression(prev))
    }

    pub fn lock_capture(
        self: &Rc<Self>,
        menu_open: &Cell<bool>,
        refresh: Rc<dyn Fn()>,
    ) -> (CaptureMenuLock, bool) {
        let prev = self.is_menu_suppressed();
        self.capture_locks
            .set(self.capture_locks.get().saturating_add(1));
        self.activate_suppressed.set(false);
        if self.sync_menu_suppression(prev) {
            refresh();
        }
        let dismissed = if menu_open.replace(false) {
            self.pulse_passive();
            true
        } else {
            false
        };
        (
            CaptureMenuLock {
                host: self.clone(),
                refresh,
            },
            dismissed,
        )
    }

    fn sync_menu_suppression(&self, prev: bool) -> bool {
        let next = self.is_menu_suppressed();
        if prev == next {
            return false;
        }
        let menu = glib::variant::ObjectPath::try_from(self.menu_path())
            .unwrap()
            .to_variant();
        let changed = HashMap::from([("Menu".to_string(), menu)]);
        let _ = self.connection.emit_signal(
            None,
            "/StatusNotifierItem",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            Some(&(ITEM, changed, Vec::<String>::new()).to_variant()),
        );
        true
    }

    fn emit_status(&self) {
        let status = self.status();
        let _ = self.connection.emit_signal(
            None,
            "/StatusNotifierItem",
            ITEM,
            "NewStatus",
            Some(&(status,).to_variant()),
        );
        let changed = HashMap::from([("Status".to_string(), status.to_variant())]);
        let _ = self.connection.emit_signal(
            None,
            "/StatusNotifierItem",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            Some(&(ITEM, changed, Vec::<String>::new()).to_variant()),
        );
    }

    fn pulse_passive(self: &Rc<Self>) {
        if let Some(id) = self.passive_timer.borrow_mut().take() {
            id.remove();
        }
        if !self.passive_pulse.replace(true) {
            self.emit_status();
        }
        let weak = Rc::downgrade(self);
        self.passive_timer
            .replace(Some(glib::timeout_add_local_once(
                std::time::Duration::from_millis(60),
                move || {
                    if let Some(this) = weak.upgrade() {
                        this.passive_timer.borrow_mut().take();
                        if this.passive_pulse.replace(false) {
                            this.emit_status();
                        }
                    }
                },
            )));
    }

    pub fn empty_root_properties() -> HashMap<String, glib::Variant> {
        HashMap::from([("children-display".to_string(), "submenu".to_variant())])
    }

    pub fn empty_root_layout() -> glib::Variant {
        (
            0i32,
            Self::empty_root_properties(),
            Vec::<glib::Variant>::new(),
        )
            .to_variant()
    }

    fn empty_menu_method(
        revision: u32,
        method: &str,
        args: &glib::Variant,
    ) -> Option<glib::Variant> {
        Some(match method {
            "GetLayout" => {
                let _ = args.get::<(i32, i32, Vec<String>)>()?;
                glib::Variant::tuple_from_iter([revision.to_variant(), Self::empty_root_layout()])
            }
            "GetGroupProperties" => {
                let _ = args.get::<(Vec<i32>, Vec<String>)>()?;
                (vec![(0i32, Self::empty_root_properties())],).to_variant()
            }
            "GetProperty" => {
                let (id, name) = args.get::<(i32, String)>()?;
                (if id == 0 && name == "children-display" {
                    "submenu".to_variant()
                } else {
                    "".to_variant()
                },)
                    .to_variant()
            }
            "Event" => {
                let _ = args.get::<(i32, String, glib::Variant, u32)>()?;
                ().to_variant()
            }
            "EventGroup" => {
                let _ = args.get::<(Vec<(i32, String, glib::Variant, u32)>,)>()?;
                (Vec::<i32>::new(),).to_variant()
            }
            "AboutToShow" => {
                let _ = args.get::<(i32,)>()?;
                (false,).to_variant()
            }
            "AboutToShowGroup" => {
                let _ = args.get::<(Vec<i32>,)>()?;
                (Vec::<i32>::new(), Vec::<i32>::new()).to_variant()
            }
            _ => return None,
        })
    }
}
