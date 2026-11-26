//! Inline choices, confirmation and shared selection pointer adapters.
use super::*;
impl Inner {
    pub(super) fn choices(self: &Rc<Self>, anchor: &gtk::Button, is_fps: bool) {
        if !self.phase.get().editable() {
            return;
        }
        let list = crate::ui::floating_bar::choice_list();
        let values = if is_fps {
            recording::FRAME_RATES.as_slice()
        } else {
            recording::DELAYS.as_slice()
        };
        let active = if is_fps {
            self.options.get().fps
        } else {
            self.options.get().delay
        };
        for &value in values {
            let label = if is_fps {
                format!("{value} FPS")
            } else if value == 0 {
                t("No delay")
            } else {
                tf("{0} s", &[&value.to_string()])
            };
            let b = crate::ui::floating_bar::choice_item(&label, value == active);
            let weak = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(i) = weak.upgrade() {
                    let mut o = i.options.get();
                    if is_fps {
                        o.fps = value
                    } else {
                        o.delay = value
                    }
                    i.options.set(o);
                    i.popup.close();
                    i.layout();
                }
            });
            list.append(&b);
        }
        self.popup.show(anchor, &list, || {});
        self.input();
    }
    pub(super) fn confirm(self: &Rc<Self>, anchor: &gtk::Button, command: Command) {
        let message_key = if matches!(command, Command::Restart) {
            "Discard this recording and start again?"
        } else {
            "Discard this recording?"
        };
        let confirm_label_key = if matches!(command, Command::Restart) {
            "Restart recording"
        } else {
            "Discard recording"
        };
        let weak = Rc::downgrade(self);
        crate::ui::floating_bar::confirm_popup(
            &self.popup,
            anchor,
            message_key,
            confirm_label_key,
            true,
            move || {
                if let Some(i) = weak.upgrade() {
                    (i.emit)(command);
                }
            },
        );
        self.input();
    }
    pub(super) fn install_region_drag(
        self: &Rc<Self>,
        widget: &impl IsA<gtk::Widget>,
        moving: bool,
    ) {
        let drag = gtk::GestureDrag::builder().button(1).build();
        let initial = Rc::new(Cell::new(None::<[f64; 2]>));
        let weak = Rc::downgrade(self);
        let saved = initial.clone();
        drag.connect_drag_begin(move |g, x, y| {
            if let Some(i) = weak.upgrade() {
                if !i.phase.get().editable() || i.popup.is_open() {
                    g.set_state(gtk::EventSequenceState::Denied);
                    return;
                }
                let Some(p) = g.widget().and_then(|w| {
                    w.compute_point(&i.surface().root, &graphene::Point::new(x as f32, y as f32))
                }) else {
                    return;
                };
                let origin = i.surface().rect;
                let point = [p.x() as f64 + origin.x, p.y() as f64 + origin.y];
                if !moving && i.toolbar.get().contains(point, 0.) {
                    g.set_state(gtk::EventSequenceState::Denied);
                    return;
                }
                saved.set(Some(point));
                let cursor_point = {
                    let mut s = i.selection.borrow_mut();
                    let p = if moving { s.rect.center() } else { point };
                    s.begin(p, i.feedback.resize_margin(7.), false);
                    p
                };
                i.input();
                i.update_cursor(cursor_point);
                g.set_state(gtk::EventSequenceState::Claimed);
            }
        });
        for end in [false, true] {
            let weak = Rc::downgrade(self);
            let saved = initial.clone();
            let motion = move |g: &gtk::GestureDrag, dx: f64, dy: f64| {
                if let (Some(i), Some(p)) = (weak.upgrade(), saved.get()) {
                    if !i.phase.get().editable() {
                        saved.set(None);
                        return;
                    }
                    // The button moves with the region, use the stable
                    // native surface rather than its moving local offsets.
                    let point = g
                        .current_event()
                        .and_then(|e| e.position())
                        .map_or([p[0] + dx, p[1] + dy], |(x, y)| {
                            [x + i.surface().rect.x, y + i.surface().rect.y]
                        });
                    {
                        let mut s = i.selection.borrow_mut();
                        let before = s.rect;
                        let anchor = if moving { s.drag.unwrap().point } else { p };
                        s.motion([anchor[0] + point[0] - p[0], anchor[1] + point[1] - p[1]]);
                        let after = s.rect;
                        i.position
                            .translate_custom([after.x - before.x, after.y - before.y]);
                        if end {
                            s.end();
                            saved.set(None);
                        }
                    }
                    i.layout();
                    i.update_cursor(point);
                    for f in &i.frames {
                        f.queue_draw();
                    }
                }
            };
            if end {
                drag.connect_drag_end(motion);
            } else {
                drag.connect_drag_update(motion);
            }
        }
        let weak = Rc::downgrade(self);
        drag.connect_cancel(move |_, _| {
            if let Some(i) = weak.upgrade()
                && initial.take().is_some()
            {
                let pointer = {
                    let mut selection = i.selection.borrow_mut();
                    let before = selection.rect;
                    selection.cancel_drag();
                    let after = selection.rect;
                    i.position
                        .translate_custom([after.x - before.x, after.y - before.y]);
                    selection.pointer
                };
                i.layout();
                if let Some(p) = pointer {
                    i.update_cursor(p);
                } else {
                    i.surface().root.set_cursor_from_name(Some("default"));
                    i.frames[i.owner].set_cursor_from_name(Some("default"));
                }
                for f in &i.frames {
                    f.queue_draw();
                }
            }
        });
        widget.add_controller(drag);
    }
}
