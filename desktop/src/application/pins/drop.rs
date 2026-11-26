//! Incoming drops share clipboard MIME decoding and pin import; surfaces only
//! resolve the destination and forward the standard GTK offer.
use super::*;
impl Pins {
    pub fn drop_destination(&self, id: u64, group: Option<&str>) -> Option<Bounds> {
        if self.locked.get() || self.suspended.get() {
            return None;
        }
        let board = self.board.borrow();
        board
            .get(id)
            .filter(|p| board.visible(p) && !p.through && group.is_none_or(|g| g == p.group))
            .map(Pin::rect)
    }

    pub(super) fn drop_target(self: &Rc<Self>, surface: &Rc<Surface>) {
        let target = gtk::DropTargetAsync::new(
            Some(gdk::ContentFormats::new(&[
                "image/png",
                "image/jpeg",
                "image/webp",
                "image/bmp",
                "image/tiff",
                "image/gif",
                "image/svg+xml",
                "text/uri-list",
                "text/html",
                "text/plain",
                "text/plain;charset=utf-8",
                "application/x-color",
            ])),
            gdk::DragAction::COPY,
        );
        target.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        let s = Rc::downgrade(surface);
        target.connect_drag_motion(move |_, _, x, y| {
            let (Some(this), Some(s)) = (weak.upgrade(), s.upgrade()) else {
                return gdk::DragAction::empty();
            };
            if !this.locked.get()
                && !this.suspended.get()
                && this
                    .board
                    .borrow()
                    .hit([x + s.rect.x, y + s.rect.y])
                    .is_some()
            {
                gdk::DragAction::COPY
            } else {
                gdk::DragAction::empty()
            }
        });
        let weak = Rc::downgrade(self);
        let s = Rc::downgrade(surface);
        target.connect_drop(move |controller, drop, x, y| {
            let (Some(this), Some(s)) = (weak.upgrade(), s.upgrade()) else {
                return false;
            };
            if this.locked.get() || this.suspended.get() {
                return false;
            }
            let Some(id) = this.board.borrow().hit([x + s.rect.x, y + s.rect.y]) else {
                return false;
            };
            let preserve = this.preferences.borrow().preserve_drag
                || controller
                    .current_event_state()
                    .contains(gdk::ModifierType::SHIFT_MASK);
            (this.callback)(Event::Drop(id, drop.clone(), preserve));
            true
        });
        surface.root.add_controller(target);
    }
}
