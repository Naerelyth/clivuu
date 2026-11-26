//! Pointer adapters reuse normal capture geometry. Running capture exposes
//! only the toolbar and its inline popup; source pixels stay interactive.
use super::*;
use clivuu_core::capture::Output as Monitor;
fn selection(i: &Inner) -> Selection {
    let mut s = Selection::new(
        i.hosts
            .surfaces
            .iter()
            .map(|s| Monitor {
                rect: s.rect,
                scale: s.scale,
            })
            .collect(),
        vec![],
        None,
    )
    .unwrap();
    s.rect = i.rect.get();
    s.selected = true;
    s
}
fn event_point(g: &impl IsA<gtk::Gesture>, origin: Bounds) -> [f64; 2] {
    g.as_ref()
        .last_event(g.as_ref().last_updated_sequence().as_ref())
        .and_then(|e| e.position())
        .map_or([origin.x, origin.y], |(x, y)| [x + origin.x, y + origin.y])
}
fn begin(i: &Inner) {
    i.interacting.set(true);
    i.input_epoch.set(i.input_epoch.get().wrapping_add(1));
    i.popup.close();
}
fn end(i: &Inner) {
    i.interacting.set(false);
    i.input_epoch.set(i.input_epoch.get().wrapping_add(1));
}
fn redraw(i: &Inner) {
    i.layout();
    for frame in &i.frames {
        frame.queue_draw();
    }
}
fn apply_region(inner: &Inner, rect: Bounds) {
    let previous = inner.rect.replace(rect);
    inner
        .toolbar_position
        .translate_custom([rect.x - previous.x, rect.y - previous.y]);
}
struct RegionDrag {
    selection: Selection,
    pointer: [f64; 2],
    // Fix the allowed axes at press time, so asynchronous capture/status
    // transitions cannot unlock an axis halfway through the gesture.
    axis: Axis,
}
impl RegionDrag {
    fn motion(&mut self, p: [f64; 2]) {
        let delta = match self.axis {
            Axis::Vertical => [0., p[1] - self.pointer[1]],
            Axis::Horizontal => [p[0] - self.pointer[0], 0.],
        };
        let original = self.selection.drag.unwrap().point;
        self.selection
            .motion([original[0] + delta[0], original[1] + delta[1]]);
    }
}
fn region_drag(inner: &Rc<Inner>, button: &gtk::Button) {
    let drag = gtk::GestureDrag::builder()
        .button(1)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    let start = Rc::new(RefCell::new(None::<RegionDrag>));
    let weak = Rc::downgrade(inner);
    let saved = start.clone();
    drag.connect_drag_begin(move |g, x, y| {
        if let Some(i) = weak.upgrade() {
            // GestureDrag recognizes a drag only after its threshold. Its begin
            // coordinates describe the original press; the last event already
            // includes that first movement and would lose part of every drag.
            let Some(press) = g.widget().and_then(|widget| {
                widget.compute_point(
                    &i.surface().window,
                    &gtk::graphene::Point::new(x as f32, y as f32),
                )
            }) else {
                g.set_state(gtk::EventSequenceState::Denied);
                return;
            };
            begin(&i);
            let mut selection = selection(&i);
            selection.begin(selection.rect.center(), 0., false);
            let state = i.state.get();
            saved.replace(Some(RegionDrag {
                selection,
                pointer: [
                    press.x() as f64 + i.surface().rect.x,
                    press.y() as f64 + i.surface().rect.y,
                ],
                axis: state.axis,
            }));
            g.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    let weak = Rc::downgrade(inner);
    let saved = start.clone();
    drag.connect_drag_update(move |g, _, _| {
        if let (Some(i), Some(gesture)) = (weak.upgrade(), saved.borrow_mut().as_mut()) {
            let p = event_point(g, i.surface().rect);
            gesture.motion(p);
            apply_region(&i, gesture.selection.rect);
            redraw(&i);
        }
    });
    let weak = Rc::downgrade(inner);
    let saved = start.clone();
    drag.connect_drag_end(move |g, _, _| {
        if let (Some(i), Some(mut gesture)) = (weak.upgrade(), saved.borrow_mut().take()) {
            gesture.motion(event_point(g, i.surface().rect));
            apply_region(&i, gesture.selection.rect);
            end(&i);
            redraw(&i);
            // Even an out-and-back gesture can leave intermediate HUD buffers
            // in the reader. One region transition always starts a clean source.
            i.command(Command::Region(gesture.selection.rect));
        }
    });
    let weak = Rc::downgrade(inner);
    drag.connect_cancel(move |_, _| {
        if let (Some(i), Some(gesture)) = (weak.upgrade(), start.borrow_mut().take()) {
            apply_region(&i, gesture.selection.drag.unwrap().rect);
            end(&i);
            redraw(&i);
            i.command(Command::Region(i.rect.get()));
        }
    });
    button.add_controller(drag);
}
pub(super) fn install(inner: &Rc<Inner>, grip: &gtk::Button, move_region: &gtk::Button) {
    let weak = Rc::downgrade(inner);
    let changed = weak.clone();
    inner.toolbar_position.install(
        grip,
        move || Some(weak.upgrade()?.input.get()),
        move |active| {
            if let Some(i) = changed.upgrade() {
                if active && !i.interacting.get() {
                    begin(&i);
                }
                if !active {
                    end(&i);
                    i.command(Command::ToolbarMoved);
                }
                i.layout();
            }
        },
    );
    region_drag(inner, move_region);
    for surface in &inner.hosts.surfaces {
        let key = gtk::EventControllerKey::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = Rc::downgrade(inner);
        key.connect_key_pressed(move |_, key, _, modifiers| {
            if let Some(i) = weak.upgrade() {
                // The inline popup owns Escape, navigation and button activation.
                if i.popup.is_open() {
                    return glib::Propagation::Proceed;
                }
                if modifiers.contains(gdk::ModifierType::CONTROL_MASK)
                    && !modifiers.intersects(
                        gdk::ModifierType::ALT_MASK
                            | gdk::ModifierType::SHIFT_MASK
                            | gdk::ModifierType::SUPER_MASK,
                    )
                    && matches!(key, gdk::Key::r | gdk::Key::R)
                {
                    i.command(Command::Reset);
                    return glib::Propagation::Stop;
                }
                if let Some(action) = crate::editor::shortcuts::output(key, modifiers) {
                    i.command(Command::Output(action));
                    return glib::Propagation::Stop;
                }
                if key == gdk::Key::Escape {
                    i.command(Command::Cancel);
                    return glib::Propagation::Stop;
                }
                if key == gdk::Key::Return {
                    i.command(Command::Output(Output::Pin));
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        surface.window.add_controller(key);
    }
}
