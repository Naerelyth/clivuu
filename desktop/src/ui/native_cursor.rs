//! Explicit native-surface fallback; child text/tool cursors still take priority.
use gtk::{gdk, prelude::*};

pub fn set_default(widget: &impl IsA<gtk::Widget>) {
    if let Some(surface) = widget.as_ref().native().and_then(|n| n.surface()) {
        surface.set_cursor(gdk::Cursor::from_name("default", None).as_ref());
    }
}

/// Popovers own separate GdkSurfaces, including during implicit button grabs.
pub fn install(widget: &impl IsA<gtk::Widget>) {
    widget.as_ref().set_cursor_from_name(Some("default"));
    widget.as_ref().connect_realize(set_default);
}
