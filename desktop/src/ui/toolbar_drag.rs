//! Shared grip behavior for editor and continuous-capture toolbars.
use clivuu_core::annotation::Bounds;
use gtk::{graphene, prelude::*};
use std::{cell::Cell, rc::Rc};
#[derive(Clone, Copy)]
struct Drag {
    rect: Bounds,
    pointer: [f64; 2],
    previous: Option<[f64; 2]>,
}

#[derive(Default)]
pub struct ToolbarPosition {
    custom: Cell<Option<[f64; 2]>>,
    drag: Cell<Option<Drag>>,
}
impl ToolbarPosition {
    pub fn is_custom(&self) -> bool {
        self.custom.get().is_some()
    }
    pub fn place(&self, mut rect: Bounds, viewport: Bounds) -> Bounds {
        if let Some(p) = self.custom.get() {
            rect.x = p[0].round().clamp(
                viewport.x + 1.,
                (viewport.x + viewport.width - rect.width - 1.).max(viewport.x + 1.),
            );
            rect.y = p[1].round().clamp(
                viewport.y + 1.,
                (viewport.y + viewport.height - rect.height - 1.).max(viewport.y + 1.),
            );
        }
        rect
    }
    /// Keep the manual offset while the host moves. Clamping remains purely
    /// visual, so moving out and back restores the original toolbar position.
    pub fn translate_custom(&self, delta: [f64; 2]) {
        if let Some(origin) = self.custom.get() {
            self.custom
                .set(Some([origin[0] + delta[0], origin[1] + delta[1]]));
        }
    }
    /// Pointer coordinates stay in the native surface while the grip moves.
    pub fn install(
        self: &Rc<Self>,
        grip: &gtk::Button,
        bounds: impl Fn() -> Option<Bounds> + 'static,
        changed: impl Fn(bool) + 'static,
    ) {
        let gesture = gtk::GestureDrag::builder()
            .button(1)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let changed = Rc::new(changed);
        let state = self.clone();
        let notify = changed.clone();
        gesture.connect_drag_begin(move |g, x, y| {
            let press = g.widget().and_then(|w| {
                let native = w.native()?.dynamic_cast::<gtk::Widget>().ok()?;
                w.compute_point(&native, &graphene::Point::new(x as f32, y as f32))
            });
            if let (Some(rect), Some(press)) = (bounds(), press) {
                state.drag.set(Some(Drag {
                    rect,
                    pointer: [press.x() as f64, press.y() as f64],
                    previous: state.custom.get(),
                }));
                g.set_state(gtk::EventSequenceState::Claimed);
                notify(true);
            } else {
                g.set_state(gtk::EventSequenceState::Denied);
            }
        });
        for end in [false, true] {
            let state = self.clone();
            let notify = changed.clone();
            let motion = move |g: &gtk::GestureDrag, _: f64, _: f64| {
                if let Some(drag) = state.drag.get() {
                    if let Some((x, y)) = g.current_event().and_then(|e| e.position()) {
                        state.custom.set(Some([
                            drag.rect.x + x - drag.pointer[0],
                            drag.rect.y + y - drag.pointer[1],
                        ]));
                    }
                    if end {
                        state.drag.set(None);
                    }
                    notify(!end);
                }
            };
            if end {
                gesture.connect_drag_end(motion);
            } else {
                gesture.connect_drag_update(motion);
            }
        }
        let state = self.clone();
        gesture.connect_cancel(move |_, _| {
            if let Some(drag) = state.drag.take() {
                state.custom.set(drag.previous);
                changed(false);
            }
        });
        grip.add_controller(gesture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_the_host_back_from_output_edges_preserves_its_manual_offset() {
        let position = ToolbarPosition::default();
        let viewport = Bounds {
            x: -1000.,
            y: 400.,
            width: 1000.,
            height: 800.,
        };
        let toolbar = Bounds {
            x: -800.,
            y: 1100.,
            width: 300.,
            height: 32.,
        };
        position.custom.set(Some([toolbar.x, toolbar.y]));
        position.translate_custom([0., 200.]);
        assert_eq!(position.place(toolbar, viewport).y, 1167.);
        position.translate_custom([0., -200.]);
        assert_eq!(position.place(toolbar, viewport).y, toolbar.y);
        position.translate_custom([900., 0.]);
        assert_eq!(position.place(toolbar, viewport).x, -301.);
        position.translate_custom([-900., 0.]);
        assert_eq!(position.place(toolbar, viewport), toolbar);
        position.custom.set(Some([-799.25, 1100.75]));
        let placed = position.place(toolbar, viewport);
        assert_eq!((placed.x, placed.y), (-799., 1101.));
        position.translate_custom([100.25, -50.25]);
        let shifted = position.place(toolbar, viewport);
        assert_eq!((shifted.x, shifted.y), (-699., 1051.));
    }
}
