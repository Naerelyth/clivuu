//! Deferred tooltip messages retain their source keys across locale changes.
use crate::editor::shortcuts::tooltip;
use clivuu_core::i18n::{Language, current_language};
use gtk::{glib, prelude::*};
use std::cell::{Cell, RefCell};
#[derive(Default)]
pub struct Tooltips {
    language: Cell<Option<Language>>,
    bindings: RefCell<Vec<(glib::WeakRef<gtk::Widget>, String, String)>>,
}
impl Tooltips {
    pub fn bind(&self, widget: &impl IsA<gtk::Widget>, title: &str, keys: &str) {
        self.refresh();
        self.bindings
            .borrow_mut()
            .retain(|(widget, _, _)| widget.upgrade().is_some());
        let widget = widget.as_ref();
        widget.set_tooltip_text(Some(&tooltip(title, keys)));
        self.bindings
            .borrow_mut()
            .push((widget.downgrade(), title.into(), keys.into()));
    }
    pub fn refresh(&self) -> bool {
        let language = current_language();
        if self.language.replace(Some(language)) == Some(language) {
            return false;
        }
        self.bindings.borrow_mut().retain(|(widget, title, keys)| {
            if let Some(widget) = widget.upgrade() {
                let enabled = widget.has_tooltip();
                widget.set_tooltip_text(Some(&tooltip(title, keys)));
                widget.set_has_tooltip(enabled);
                true
            } else {
                false
            }
        });
        true
    }
}
