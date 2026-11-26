//! Regular GTK windows use the session's native backend, independently of pins.
use gtk::{gio, glib};

pub fn backend() -> &'static str {
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland")
        || std::env::var_os("DISPLAY").is_none()
    {
        "wayland"
    } else {
        "x11"
    }
}

/// Configure only the child, before its GTK initialization.
pub fn launcher(flags: gio::SubprocessFlags) -> gio::SubprocessLauncher {
    let launcher = gio::SubprocessLauncher::new(flags);
    launcher.setenv("GDK_BACKEND", backend(), true);
    // Regular xdg_toplevels need no layer-shell interposition.
    if let Ok(preload) = std::env::var("LD_PRELOAD") {
        let preload = super::display::without_layer_shell(&preload);
        launcher.setenv("LD_PRELOAD", &preload, true);
    }
    launcher
}

pub fn initialize() -> Result<(), glib::BoolError> {
    gtk::gdk::set_allowed_backends(backend());
    glib::set_application_name("Clivuu");
    adw::init()
}
