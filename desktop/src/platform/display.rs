//! Desktop service display policy. Capture APIs are deliberately independent.
use clivuu_core::i18n::t;
use std::{ffi::OsString, os::unix::process::CommandExt, process::Command};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    Wayland,
    X11,
}
impl Backend {
    fn name(self) -> &'static str {
        match self {
            Self::Wayland => "wayland",
            Self::X11 => "x11",
        }
    }
}
pub fn is_gnome() -> bool {
    gnome_desktop(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref())
}
fn gnome_desktop(desktop: Option<&str>) -> bool {
    desktop.is_some_and(|s| s.split(':').any(|s| s.eq_ignore_ascii_case("GNOME")))
}
fn select(
    wayland_display: bool,
    x11_display: bool,
    session: Option<&str>,
    desktop: Option<&str>,
) -> Backend {
    let wayland = wayland_display || session == Some("wayland");
    let gnome = gnome_desktop(desktop);
    if wayland {
        if gnome {
            Backend::X11
        } else {
            Backend::Wayland
        }
    } else if x11_display {
        Backend::X11
    } else {
        Backend::Wayland
    }
}
/// Regular UI and a fresh service launch must not inherit layer-shell's hook.
pub fn without_layer_shell(preload: &str) -> String {
    preload
        .split([':', ' '])
        .filter(|s| {
            !s.is_empty()
                && std::path::Path::new(s)
                    .file_name()
                    .is_none_or(|n| n != "libgtk4-layer-shell.so.0")
        })
        .collect::<Vec<_>>()
        .join(":")
}
/// Re-exec before GTK initialization: avoids unsafe process-global env mutation
/// and loads layer-shell before libwayland, for CLI and direct desktop launches.
pub fn bootstrap() -> Result<(), String> {
    let backend = select(
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
    );
    if backend == Backend::X11 && std::env::var_os("DISPLAY").is_none() {
        return Err(t("The desktop service is unavailable"));
    }
    let preload =
        backend == Backend::Wayland && std::env::var_os("CLIVUU_LAYER_PRELOADED").is_none();
    if preload || std::env::var("GDK_BACKEND").as_deref() != Ok(backend.name()) {
        let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
        command
            .args(std::env::args_os().skip(1))
            .env("GDK_BACKEND", backend.name());
        if preload {
            let mut libraries = OsString::from("libgtk4-layer-shell.so.0");
            if let Some(previous) = std::env::var_os("LD_PRELOAD") {
                libraries.push(":");
                libraries.push(previous);
            }
            command
                .env("LD_PRELOAD", libraries)
                .env("CLIVUU_LAYER_PRELOADED", "1");
        }
        return Err(command.exec().to_string());
    }
    gtk::gdk::set_allowed_backends(backend.name());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wayland_desktops_do_not_fall_back_to_an_available_x_display() {
        for desktop in ["KDE", "niri", "Hyprland", "sway", "unknown"] {
            for x11 in [false, true] {
                assert_eq!(
                    select(true, x11, Some("wayland"), Some(desktop)),
                    Backend::Wayland
                );
                assert_eq!(
                    select(false, x11, Some("wayland"), Some(desktop)),
                    Backend::Wayland
                );
            }
        }
        assert_eq!(
            select(true, true, Some("wayland"), Some("ubuntu:GNOME")),
            Backend::X11
        );
        assert_eq!(select(false, true, Some("x11"), Some("KDE")), Backend::X11);
        assert_eq!(
            select(false, true, Some("x11"), Some("GNOME")),
            Backend::X11
        );
    }
}
