//! Native Wayland layers. No Xlib, XWayland focus repair or global input polling.
use super::{backend::PinRole, runtime::Library};
use clivuu_core::i18n::t;
use gtk::{gdk, glib::translate::*};
use std::ffi::{CStr, c_char, c_int};
type Window = *mut gtk::ffi::GtkWindow;
#[derive(Clone, Copy)]
#[repr(i32)]
enum Keyboard {
    None = 0,
    Exclusive = 1,
    OnDemand = 2,
}
pub(super) struct Backend {
    _library: Library,
    version: u32,
    init: unsafe extern "C" fn(Window),
    monitor: unsafe extern "C" fn(Window, *mut gdk::ffi::GdkMonitor),
    namespace: unsafe extern "C" fn(Window, *const c_char),
    layer: unsafe extern "C" fn(Window, c_int),
    keyboard: unsafe extern "C" fn(Window, c_int),
    zone: unsafe extern "C" fn(Window, c_int),
    anchor: unsafe extern "C" fn(Window, c_int, c_int),
}
impl Backend {
    pub fn load() -> Result<Self, String> {
        let unsupported = || {
            t("This compositor has no layer-shell support; a non-focusing pin cannot be created")
        };
        let lib = Library::open(c"libgtk4-layer-shell.so.0").map_err(|_| unsupported())?;
        // SAFETY: gtk4-layer-shell.h signatures and enums; main-thread only.
        unsafe {
            let supported: unsafe extern "C" fn() -> c_int =
                lib.symbol(c"gtk_layer_is_supported")?;
            if supported() == 0 {
                return Err(unsupported());
            }
            let version: unsafe extern "C" fn() -> u32 =
                lib.symbol(c"gtk_layer_get_protocol_version")?;
            Ok(Self {
                version: version(),
                init: lib.symbol(c"gtk_layer_init_for_window")?,
                monitor: lib.symbol(c"gtk_layer_set_monitor")?,
                namespace: lib.symbol(c"gtk_layer_set_namespace")?,
                layer: lib.symbol(c"gtk_layer_set_layer")?,
                keyboard: lib.symbol(c"gtk_layer_set_keyboard_mode")?,
                zone: lib.symbol(c"gtk_layer_set_exclusive_zone")?,
                anchor: lib.symbol(c"gtk_layer_set_anchor")?,
                _library: lib,
            })
        }
    }
    fn keyboard(&self, window: &gtk::Window, keyboard: Keyboard) {
        // SAFETY: this window belongs to this Wayland backend and outlives the call.
        unsafe {
            (self.keyboard)(window.to_glib_none().0, keyboard as c_int);
        }
    }
    fn setup(
        &self,
        window: &gtk::Window,
        monitor: &gdk::Monitor,
        namespace: &CStr,
        keyboard: Keyboard,
    ) {
        // SAFETY: setup occurs before realization; all GObjects remain owned by GTK.
        unsafe {
            let w = window.to_glib_none().0;
            (self.init)(w);
            (self.monitor)(w, monitor.to_glib_none().0);
            (self.namespace)(w, namespace.as_ptr());
            (self.layer)(w, 3);
            (self.zone)(w, -1);
            for edge in 0..4 {
                (self.anchor)(w, edge, 1);
            }
        }
        self.keyboard(window, keyboard);
    }
    pub fn capture(&self, window: &gtk::Window, monitor: &gdk::Monitor) {
        self.setup(window, monitor, c"clivuu-capture", Keyboard::Exclusive);
    }
    pub fn pin(
        &self,
        window: &gtk::Window,
        monitor: &gdk::Monitor,
        role: PinRole,
    ) -> Result<(), String> {
        if role != PinRole::Controls && self.version < 4 {
            return Err(t(
                "This compositor does not support on-demand keyboard focus for pins",
            ));
        }
        let keyboard = if role != PinRole::Controls {
            Keyboard::OnDemand
        } else {
            Keyboard::None
        };
        self.setup(
            window,
            monitor,
            if role == PinRole::Menu {
                c"clivuu-pin-menu"
            } else {
                c"clivuu-pins"
            },
            keyboard,
        );
        Ok(())
    }
    pub fn edit(&self, window: &gtk::Window, role: PinRole, enabled: bool) {
        // Pins keep normal compositor focus semantics throughout their life.
        // Only a continuous-capture control surface retains its prior shortcut ownership.
        if role == PinRole::Controls {
            self.keyboard(
                window,
                if enabled {
                    Keyboard::Exclusive
                } else {
                    Keyboard::None
                },
            );
        }
    }
}
