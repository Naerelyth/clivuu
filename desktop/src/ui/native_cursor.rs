//! Explicit native-surface fallback; child text/tool cursors still take priority.
use gtk::{gdk, prelude::*};

pub fn apply_surface(widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    let cursor = widget
        .cursor()
        .or_else(|| gdk::Cursor::from_name("default", None));
    if let Some(surface) = widget.native().and_then(|n| n.surface()) {
        surface.set_cursor(cursor.as_ref());
    }
}

pub fn set_named(widget: &impl IsA<gtk::Widget>, name: &str) {
    let widget = widget.as_ref();
    if widget.cursor().and_then(|c| c.name()).as_deref() != Some(name) {
        widget.set_cursor_from_name(Some(name));
    }
    apply_surface(widget);
}

pub fn set_default(widget: &impl IsA<gtk::Widget>) {
    set_named(widget, "default");
}

/// Popovers own separate GdkSurfaces, including during implicit button grabs.
pub fn install(widget: &impl IsA<gtk::Widget>) {
    widget.as_ref().set_cursor_from_name(Some("default"));
    widget.as_ref().connect_realize(apply_surface);
}
