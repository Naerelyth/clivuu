//! Native input is translated once into shared pin bindings and editor events.
use super::*;
use clivuu_core::pins::mouse::{self, Action as MouseAction, Trigger};
impl Pins {
    pub(super) fn input(self: &Rc<Self>, surface: &Rc<Surface>) {
        let weak = Rc::downgrade(self);
        surface.window.connect_is_active_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                this.focus_changed();
            }
        });
        // Escape closes a native menu before editor/text shortcuts, without grabs.
        let key = gtk::EventControllerKey::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = Rc::downgrade(self);
        key.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if !this.keyboard_enabled() {
                return glib::Propagation::Stop;
            }
            // The shared editor routes its own keys through Pins::key. Menus
            // also own keys when an editable control bypasses editor shortcuts.
            if this.active.get().is_none() || this.menu.borrow().is_some() {
                let id = this.highlighted.get().or(this.board.borrow().selected);
                if let Some(id) = id
                    && this.key(id, key, modifiers)
                {
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(self);
        key.connect_key_released(move |_, key, _, _| {
            if let Some(this) = weak.upgrade()
                && this.keyboard_enabled()
                && let Some(id) = this.lens_id.get()
            {
                this.lens_key(id, key, true);
            }
        });
        surface.window.add_controller(key);
        surface.root.set_focusable(true);
        let weak = Rc::downgrade(self);
        let s = Rc::downgrade(surface);
        // Resolve pin gestures before the Select canvas handles its own input.
        // editor_at keeps marks, annotation tools, text and controls in charge.
        let click = gtk::GestureClick::builder()
            .button(0)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        click.connect_pressed(move |g, count, x, y| {
            let (Some(this), Some(s)) = (weak.upgrade(), s.upgrade()) else {
                return;
            };
            let point = [x + s.rect.x, y + s.rect.y];
            let editing = this.editor_at(point);
            let id = this.activate_at(point);
            if editing {
                return;
            }
            if let Some(id) = id {
                let state_mods = g.current_event_state();
                let alt_held =
                    state_mods.contains(gdk::ModifierType::ALT_MASK) || this.lens.borrow().alt != 0;
                if g.current_button() == 1
                    && !alt_held
                    && count >= 2
                    && this.pin_ocr_multi_click(id, point, count)
                {
                    g.set_state(gtk::EventSequenceState::Claimed);
                    this.drag.borrow_mut().take();
                    this.ocr_drag.set(None);
                    this.sync();
                    return;
                }
                let trigger = match (g.current_button(), count) {
                    (1, 2) => Trigger::DoubleLeft,
                    (2, 1) => Trigger::Middle,
                    (3, 1) => Trigger::Right,
                    _ => Trigger::None,
                };
                let action = this
                    .preferences
                    .borrow()
                    .mouse
                    .gesture(trigger, mouse_modifiers(g.current_event_state()));
                if let Some(action) = action {
                    g.set_state(gtk::EventSequenceState::Claimed);
                    this.drag.borrow_mut().take();
                    this.mouse_action(id, action, 0., point);
                    return;
                }
                if g.current_button() == 3 {
                    g.set_state(gtk::EventSequenceState::Claimed);
                    if let Err(e) = this.open_menu(id, point) {
                        eprintln!("{e}");
                    }
                    return;
                }
                return;
            }
            this.sync();
        });
        surface.root.add_controller(click);
        let weak = Rc::downgrade(self);
        let s = Rc::downgrade(surface);
        let drag = gtk::GestureDrag::builder().button(1).build();
        drag.connect_drag_begin(move |g, x, y| {
            let (Some(this), Some(s)) = (weak.upgrade(), s.upgrade()) else {
                return;
            };
            let point = [x + s.rect.x, y + s.rect.y];
            if this.editor_at(point) {
                g.set_state(gtk::EventSequenceState::Denied);
                return;
            }
            let start = {
                let board = this.board.borrow();
                board.hit(point).and_then(|id| board.get(id).cloned())
            };
            if let Some(start) = start {
                let id = start.id;
                let edge = start.edge(point);
                let alt_held = g
                    .current_event_state()
                    .contains(gdk::ModifierType::ALT_MASK)
                    || this.lens.borrow().alt != 0;
                if !alt_held && !edge.iter().any(|v| *v) && this.pin_ocr_begin_drag(id, point) {
                    this.drag.borrow_mut().take();
                    this.ocr_drag.set(Some((id, point)));
                    this.board.borrow_mut().selected = Some(id);
                    this.highlight(Some(id));
                    this.sync();
                    return;
                }
                this.ocr_drag.set(None);
                this.drag.replace(Some(Drag { id, start, edge }));
                this.board.borrow_mut().selected = Some(id);
                this.highlight(Some(id));
            }
        });
        let weak = Rc::downgrade(self);
        let s = Rc::downgrade(surface);
        drag.connect_drag_update(move |_, dx, dy| {
            let (Some(this), Some(s)) = (weak.upgrade(), s.upgrade()) else {
                return;
            };
            if let Some((ocr_id, start_pt)) = this.ocr_drag.get() {
                if this.pin_ocr_extend_drag(ocr_id, [start_pt[0] + dx, start_pt[1] + dy]) {
                    this.sync();
                }
                return;
            }
            let scale = s
                .window
                .surface()
                .map(|sf| sf.scale())
                .filter(|v| v.is_finite() && *v > 0.)
                .unwrap_or(1.);
            let update = {
                let drag = this.drag.borrow();
                let Some(d) = drag.as_ref() else {
                    return;
                };
                let resizing = d.edge.iter().any(|v| *v);
                let mut board = this.board.borrow_mut();
                let Some(pin) = board.get_mut(d.id) else {
                    return;
                };
                let before = (pin.x, pin.y, pin.zoom);
                if resizing {
                    pin.resize_scaled(&d.start, d.edge, [dx, dy], scale);
                } else {
                    pin.x = clivuu_core::pins::snap_coord(d.start.x + dx, scale);
                    pin.y = clivuu_core::pins::snap_coord(d.start.y + dy, scale);
                }
                ((pin.x, pin.y, pin.zoom) != before).then_some((d.id, resizing))
            };
            if let Some((id, resizing)) = update
                && (resizing || !this.sync_pin_translation(id))
            {
                this.sync();
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_end(move |_, _, _| {
            if let Some(this) = weak.upgrade() {
                if let Some((ocr_id, _)) = this.ocr_drag.take() {
                    this.pin_ocr_end_drag(ocr_id);
                    this.reconcile_pointer_activation();
                    this.sync();
                    return;
                }
                this.drag.borrow_mut().take();
                this.reconcile_pointer_activation();
                this.changed();
                this.sync();
            }
        });
        surface.root.add_controller(drag);
        let pointer = Rc::new(Cell::new(None));
        let last_cursor = Rc::new(Cell::new("default"));
        let pos = pointer.clone();
        let motion = gtk::EventControllerMotion::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = Rc::downgrade(self);
        let s = Rc::downgrade(surface);
        let cursor_state = last_cursor.clone();
        let track_pointer = move |ctrl: &gtk::EventControllerMotion, x: f64, y: f64| {
            pos.set(Some((x, y)));
            let (Some(this), Some(s)) = (weak.upgrade(), s.upgrade()) else {
                return;
            };
            let point = [x + s.rect.x, y + s.rect.y];
            this.pointer.set(Some(point));
            let apply_cursor = |name: &'static str| {
                if cursor_state.replace(name) != name {
                    s.root.set_cursor_from_name(Some(name));
                }
            };
            if this.ocr_drag.get().is_some() {
                apply_cursor("text");
                return;
            }
            if let Some(edge) = this.drag.borrow().as_ref().map(|d| d.edge) {
                if this.lens.borrow().alt != 0
                    && let Some(id) = this.active.get().or(this.board.borrow().selected)
                {
                    this.update_lens(id);
                }
                apply_cursor(clivuu_core::pins::cursor(edge));
                return;
            }
            this.reconcile_pointer_activation();
            if let Some(id) = this.active.get().or(this.board.borrow().selected) {
                this.update_lens(id);
            }

            if this.editor_at(point) {
                // Controls inherit the surface cursor. Do not leave the last
                // pin-edge resize cursor beneath an editor or toolbar widget.
                apply_cursor("default");
                return;
            }
            let alt_held = ctrl
                .current_event_state()
                .contains(gdk::ModifierType::ALT_MASK)
                || this.lens.borrow().alt != 0;
            let board = this.board.borrow();
            let hit_pin = board.hit(point).and_then(|id| board.get(id));
            let edge = hit_pin.map(|p| p.edge(point)).unwrap_or([false; 4]);
            if !alt_held
                && !edge.iter().any(|v| *v)
                && let Some(p) = hit_pin
                && this.pin_ocr_line_at(p.id, p.rect(), point).is_some()
            {
                apply_cursor("text");
                return;
            }
            apply_cursor(clivuu_core::pins::cursor(edge));
        };
        motion.connect_enter(track_pointer.clone());
        motion.connect_motion(track_pointer);
        let weak = Rc::downgrade(self);
        let pos = pointer.clone();
        motion.connect_leave(move |_| {
            pos.set(None);
            if let Some(this) = weak.upgrade() {
                this.pointer.set(None);
                this.reconcile_pointer_activation();
                for (_, w) in &this.lens.borrow().widgets {
                    w.set_visible(false);
                }
            }
        });
        surface.root.add_controller(motion);
        let scroll = gtk::EventControllerScroll::builder()
            .flags(gtk::EventControllerScrollFlags::BOTH_AXES)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = Rc::downgrade(self);
        let s = Rc::downgrade(surface);
        scroll.connect_scroll(move |controller, dx, dy| {
            let (Some(this), Some(s)) = (weak.upgrade(), s.upgrade()) else {
                return glib::Propagation::Proceed;
            };
            let Some((x, y)) = controller
                .current_event()
                .and_then(|e| e.position())
                .or(pointer.get())
            else {
                return glib::Propagation::Proceed;
            };
            let point = [x + s.rect.x, y + s.rect.y];
            if this.editor_at(point) {
                return glib::Propagation::Proceed;
            }
            let Some(id) = this.board.borrow().hit(point) else {
                return glib::Propagation::Proceed;
            };
            let action = this.preferences.borrow().mouse.gesture(
                Trigger::Wheel,
                mouse_modifiers(controller.current_event_state()),
            );
            let Some(action) = action else {
                return glib::Propagation::Proceed;
            };
            this.mouse_action(id, action, if dy == 0. { dx } else { dy }, point);
            glib::Propagation::Stop
        });
        surface.root.add_controller(scroll);
    }
    pub(super) fn mouse_action(
        self: &Rc<Self>,
        id: u64,
        action: MouseAction,
        delta: f64,
        point: [f64; 2],
    ) {
        match action {
            MouseAction::Close => {
                self.close(id);
                return;
            }
            MouseAction::Destroy => {
                self.destroy(id);
                return;
            }
            MouseAction::CopyText => {
                if self.copy_pin_ocr_text(id, true) || self.copy_pin_ocr_text(id, false) {
                    return;
                }
                if self
                    .board
                    .borrow()
                    .get(id)
                    .is_some_and(|p| p.plain_text.is_some())
                {
                    (self.callback)(Event::Output(id, Output::CopyText));
                }
                return;
            }
            _ => {}
        }
        let mut board = self.board.borrow_mut();
        let Some(pin) = board.get_mut(id) else {
            return;
        };
        let settings = self.preferences.borrow();
        match action {
            MouseAction::Zoom | MouseAction::Opacity if !delta.is_finite() || delta == 0. => return,
            MouseAction::Zoom => settings.zoom(
                pin,
                pin.zoom * if delta < 0. { 1.1 } else { 1. / 1.1 },
                point,
            ),
            MouseAction::Opacity => {
                pin.opacity = (pin.opacity - delta.signum() * 0.06).clamp(0.2, 1.)
            }
            MouseAction::Reset => {
                settings.reset(pin);
                pin.opacity = 1.;
            }
            MouseAction::Thumbnail => settings.thumbnail(pin),
            _ => unreachable!(),
        }
        drop(settings);
        drop(board);
        self.changed();
        self.sync();
    }

    pub(super) fn pin_ocr_line_at(&self, id: u64, rect: Bounds, point: [f64; 2]) -> Option<usize> {
        self.data
            .borrow()
            .get(&id)
            .and_then(|d| d.ocr.line_at(rect, point))
    }

    fn pin_ocr_multi_click(&self, id: u64, point: [f64; 2], clicks: i32) -> bool {
        let Some(rect) = self.board.borrow().get(id).map(|p| p.rect()) else {
            return false;
        };
        let mut data = self.data.borrow_mut();
        let Some(d) = data.get_mut(&id) else {
            return false;
        };
        if !d.ocr.is_ready() || d.ocr.line_at(rect, point).is_none() {
            return false;
        }
        d.ocr.handle_press(rect, point, clicks)
    }

    fn pin_ocr_begin_drag(&self, id: u64, point: [f64; 2]) -> bool {
        let Some(rect) = self.board.borrow().get(id).map(|p| p.rect()) else {
            return false;
        };
        let mut data = self.data.borrow_mut();
        let Some(d) = data.get_mut(&id) else {
            return false;
        };
        d.ocr.handle_press(rect, point, 1)
    }

    fn pin_ocr_extend_drag(&self, id: u64, point: [f64; 2]) -> bool {
        let Some(rect) = self.board.borrow().get(id).map(|p| p.rect()) else {
            return false;
        };
        let mut data = self.data.borrow_mut();
        let Some(d) = data.get_mut(&id) else {
            return false;
        };
        d.ocr.handle_motion(rect, point)
    }

    fn pin_ocr_end_drag(&self, id: u64) {
        if let Some(d) = self.data.borrow_mut().get_mut(&id) {
            d.ocr.handle_release();
        }
    }
}
pub(super) fn mouse_modifiers(state: gdk::ModifierType) -> u8 {
    let mut modifiers = 0;
    for (native, portable) in [
        (gdk::ModifierType::CONTROL_MASK, mouse::CONTROL),
        (gdk::ModifierType::SHIFT_MASK, mouse::SHIFT),
        (gdk::ModifierType::ALT_MASK, mouse::ALT),
        (gdk::ModifierType::SUPER_MASK, mouse::SUPER),
        (gdk::ModifierType::META_MASK, mouse::SUPER),
    ] {
        if state.contains(native) {
            modifiers |= portable;
        }
    }
    modifiers
}
