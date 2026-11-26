//! Shared on-disk artwork for status notifier hosts and notification daemons.
//! File URIs work without a globally installed icon theme or desktop launcher.
use gtk::{gdk, gio, prelude::*};
use std::path::PathBuf;

/// Resolve the canonical launcher artwork at delivery time so theme changes
/// apply without restarting Clivuu. The tray uses its own transparent asset.
pub fn notification() -> String {
    let themed = gdk::Display::default().and_then(|display| {
        let theme = gtk::IconTheme::for_display(&display);
        let name = super::identity::APPLICATION_ID;
        if !theme.has_icon(name) {
            return None;
        }
        theme
            .lookup_icon(
                name,
                &[],
                48,
                1,
                gtk::TextDirection::None,
                gtk::IconLookupFlags::empty(),
            )
            .file()
    });
    themed
        .unwrap_or_else(|| gio::File::for_path(desktop()))
        .uri()
        .into()
}

pub fn desktop() -> PathBuf {
    find("clivoo.svg")
}
pub fn transparent() -> PathBuf {
    find("clivoo-transparent.svg")
}
pub fn github() -> PathBuf {
    find("github-symbolic.svg")
}
fn find(name: &str) -> PathBuf {
    let mut directories = Vec::new();
    if let Some(path) = std::env::var_os("CLIVUU_DESKTOP_DIR") {
        directories.push(PathBuf::from(path).join("icons"));
    }
    if let Some(prefix) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent()?.parent().map(PathBuf::from))
    {
        directories.push(prefix.join("icons"));
        directories.push(prefix.join("share/clivuu/desktop/icons"));
        directories.push(prefix.join("share/clivuu/icons"));
    }
    directories.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/icons"));
    directories
        .into_iter()
        .map(|path| path.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}
