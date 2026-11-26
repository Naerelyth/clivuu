//! Transient pin feedback and history policy shared by all surface backends.
use super::*;
impl Pins {
    pub(super) fn apply_limits(self: &Rc<Self>) {
        let changed = {
            let settings = self.preferences.borrow();
            let mut board = self.board.borrow_mut();
            let mut changed = false;
            for pin in &mut board.pins {
                changed |= settings.constrain(pin);
            }
            changed
        };
        if changed {
            self.changed();
        }
    }

    pub fn prune_closed(self: &Rc<Self>) {
        let ids = self
            .board
            .borrow()
            .obsolete_closed(self.preferences.borrow().closed_limit);
        for id in ids {
            self.destroy(id);
        }
    }
    pub fn flash(self: &Rc<Self>, ids: Vec<u64>) {
        if ids.is_empty() {
            return;
        }
        self.flashing.replace(Some((ids, glib::monotonic_time())));
        self.sync();
        if self.flash_timer.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(self);
        let timer = glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            let Some(this) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let finished = this
                .flashing
                .borrow()
                .as_ref()
                .is_none_or(|(_, start)| glib::monotonic_time() - start >= 800_000);
            if finished {
                this.flashing.borrow_mut().take();
                this.flash_timer.borrow_mut().take();
            }
            this.sync();
            if finished {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        self.flash_timer.replace(Some(timer));
    }
    pub(super) fn flash_phase(&self, id: u64) -> bool {
        self.flashing.borrow().as_ref().is_some_and(|(ids, start)| {
            ids.contains(&id) && (glib::monotonic_time() - start) / 100_000 % 2 == 0
        })
    }
    pub fn paste_hides_flashing(self: &Rc<Self>) -> bool {
        let flashing = self
            .flashing
            .borrow()
            .as_ref()
            .is_some_and(|(_, start)| glib::monotonic_time() - start < 800_000);
        if !self.board.borrow().hidden && self.preferences.borrow().hide_on_flash && flashing {
            self.flashing.borrow_mut().take();
            self.board.borrow_mut().hidden = true;
            self.close_menu();
            self.sync();
            true
        } else {
            false
        }
    }
    pub fn group_changed(self: &Rc<Self>) {
        self.sync();
        if self.preferences.borrow().flash_group {
            let ids = {
                let board = self.board.borrow();
                board
                    .pins
                    .iter()
                    .filter(|p| board.visible(p))
                    .map(|p| p.id)
                    .collect()
            };
            self.flash(ids);
        }
    }
    pub fn capture_position(&self, rect: Bounds) -> Bounds {
        if self.preferences.borrow().use_snip_position {
            return rect;
        }
        let Some(output) = gdk_outputs(&self.display).first().copied() else {
            return rect;
        };
        Bounds {
            x: output.rect.x + (output.rect.width - rect.width) / 2.,
            y: output.rect.y + (output.rect.height - rect.height) / 2.,
            ..rect
        }
    }
}
