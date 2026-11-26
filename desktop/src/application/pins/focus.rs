//! Pin focus policy is independent of toolbar visibility and annotation state.
use super::*;

impl Pins {
    pub(super) fn keyboard_enabled(&self) -> bool {
        !self.gnome || self.preferences.borrow().gnome_focus
    }

    pub(super) fn update_keyboard(&self) {
        let enabled = self.keyboard_enabled();
        if let Some(hosts) = self.hosts.borrow().as_ref() {
            for surface in &hosts.surfaces {
                if let Err(error) = surface.set_keyboard(enabled, false) {
                    eprintln!("{error}");
                }
            }
        }
    }

    pub(super) fn focus_pin(&self, id: u64) {
        if !self.keyboard_enabled() {
            return;
        }
        let hosts = self.hosts.borrow();
        let Some(hosts) = hosts.as_ref() else { return };
        let rect = self.board.borrow().get(id).map(Pin::rect);
        let Some(rect) = rect else { return };
        let surface = &hosts.surfaces[output_owner(hosts, rect)];
        if !surface.uses_native_focus() && !surface.window.is_active() {
            if let Err(error) = surface.set_keyboard(true, true) {
                eprintln!("{error}");
                return;
            }
            surface.root.grab_focus();
        }
    }

    fn focus_target(&self) -> Option<u64> {
        self.hosts
            .borrow()
            .as_ref()
            .filter(|h| h.surfaces.iter().any(|s| s.window.is_active()))
            .and_then(|_| self.board.borrow().selected)
    }

    pub(super) fn focus_changed(self: &Rc<Self>) {
        // Aggregate after transfers between monitor hosts and their popups.
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(this) = weak.upgrade()
                && this.keyboard_enabled()
            {
                let focused = this.focus_target();
                if focused.is_none() {
                    this.close_menu();
                }
                this.highlight(focused);
            }
        });
    }

    pub(super) fn reconcile_pointer_activation(self: &Rc<Self>) {
        if self.keyboard_enabled() {
            return;
        }
        // With no keyboard focus, activation follows pointer interaction. Keep
        // the owner active while its editor, menu or drag is in use.
        let board = self.board.borrow();
        let highlighted = self.highlighted.get().filter(|id| {
            board.get(*id).is_some_and(|p| board.visible(p))
                && (self.active.get() == Some(*id)
                    || self.menu.borrow().as_ref().is_some_and(|m| m.pin == *id)
                    || self.drag.borrow().as_ref().is_some_and(|d| d.id == *id)
                    || self.pointer.get().is_some_and(|point| {
                        board.get(*id).is_some_and(|p| contains(p.rect(), point))
                    }))
        });
        drop(board);
        self.highlight(highlighted);
    }
}
