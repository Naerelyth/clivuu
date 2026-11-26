//! Stationary in-place selection hosts. X11 uses one override-redirect desktop
//! surface; layer-shell uses one surface per output. Both embed the same view.
use super::backend::Backend;
use crate::ui::viewport::Viewport;
use clivuu_core::{
    annotation::Bounds,
    capture::{Output, intersect},
    i18n::t,
};
use gtk::{gdk, glib, prelude::*};
use std::{cell::RefCell, rc::Rc};

pub struct Surface {
    pub window: gtk::Window,
    pub root: Viewport,
    pub rect: Bounds,
}
pub struct CaptureOverlay {
    surfaces: Vec<Surface>,
    _group: gtk::WindowGroup,
    _backend: Backend,
    signals: Rc<RefCell<Vec<(glib::Object, glib::SignalHandlerId)>>>,
}
impl Drop for CaptureOverlay {
    fn drop(&mut self) {
        for (object, id) in self.signals.borrow_mut().drain(..) {
            object.disconnect(id);
        }
        for s in &self.surfaces {
            s.window.destroy();
        }
    }
}
impl CaptureOverlay {
    pub fn new(
        display: &gdk::Display,
        outputs: &[Output],
        cancelled: impl Fn(Option<String>) + 'static,
    ) -> Result<Self, String> {
        if outputs.is_empty() {
            return Err(t("Invalid monitor layout"));
        }
        let backend = Backend::detect(display)?;
        let group = gtk::WindowGroup::new();
        let mut this = Self {
            surfaces: vec![],
            _group: group,
            _backend: backend.clone(),
            signals: Rc::new(RefCell::new(vec![])),
        };
        let cancelled: Rc<dyn Fn(Option<String>)> = Rc::new(cancelled);
        let monitors = display.monitors();
        let rectangles = backend.capture_rectangles(outputs)?;
        let notify = cancelled.clone();
        let id = monitors.connect_items_changed(move |_, _, _, _| {
            notify(Some(t("The monitor layout changed during capture")))
        });
        this.signals
            .borrow_mut()
            .push((monitors.clone().upcast(), id));
        let physical: Vec<_> = (0..monitors.n_items())
            .filter_map(|i| monitors.item(i).and_downcast::<gdk::Monitor>())
            .collect();
        for monitor in &physical {
            let notify = cancelled.clone();
            let id = monitor.connect_notify_local(None, move |_, p| {
                if matches!(p.name(), "geometry" | "scale" | "scale-factor") {
                    notify(Some(t("The monitor layout changed during capture")));
                }
            });
            this.signals
                .borrow_mut()
                .push((monitor.clone().upcast(), id));
        }
        for rect in rectangles {
            let window = gtk::Window::builder()
                .title("Clivuu capture overlay")
                .decorated(false)
                .default_width(rect.width.ceil() as i32)
                .default_height(rect.height.ceil() as i32)
                .build();
            this._group.add_window(&window);
            let root = Viewport::new();
            root.set_hexpand(true);
            root.set_vexpand(true);
            root.set_overflow(gtk::Overflow::Hidden);
            window.set_child(Some(&root));
            let notify = cancelled.clone();
            window.connect_close_request(move |_| {
                notify(None);
                glib::Propagation::Stop
            });
            this.surfaces.push(Surface {
                window: window.clone(),
                root,
                rect,
            });
            let monitor = physical.iter().find(|m| {
                let r = m.geometry();
                let overlap = intersect(
                    rect,
                    Bounds {
                        x: r.x() as f64,
                        y: r.y() as f64,
                        width: r.width() as f64,
                        height: r.height() as f64,
                    },
                );
                overlap.width > 0. && overlap.height > 0.
            });
            backend.prepare_capture(
                &window,
                rect,
                monitor,
                cancelled.clone(),
                this.signals.clone(),
            )?;
        }
        Ok(this)
    }
    pub fn surfaces(&self) -> &[Surface] {
        &self.surfaces
    }
    pub fn open(&self) {
        for s in &self.surfaces {
            s.window.set_visible(true);
        }
    }
    pub fn hide(&self) {
        for s in &self.surfaces {
            s.window.set_visible(false);
        }
    }
}

pub fn gdk_outputs(display: &gdk::Display) -> Vec<Output> {
    let monitors = display.monitors();
    (0..monitors.n_items())
        .filter_map(|i| {
            let m = monitors.item(i).and_downcast::<gdk::Monitor>()?;
            let r = m.geometry();
            Some(Output {
                rect: Bounds {
                    x: r.x() as f64,
                    y: r.y() as f64,
                    width: r.width() as f64,
                    height: r.height() as f64,
                },
                scale: m.scale(),
            })
        })
        .collect()
}

/// Own subscriptions across an asynchronous image acquisition. Drop disconnects
/// every handler; callbacks never outlive the capture that installed them.
pub struct LayoutWatch(Vec<(glib::Object, glib::SignalHandlerId)>);
impl LayoutWatch {
    pub fn new(display: &gdk::Display, changed: impl Fn() + 'static) -> Self {
        let changed: Rc<dyn Fn()> = Rc::new(changed);
        let monitors = display.monitors();
        let notify = changed.clone();
        let mut signals = vec![(
            monitors.clone().upcast(),
            monitors.connect_items_changed(move |_, _, _, _| notify()),
        )];
        for i in 0..monitors.n_items() {
            if let Some(monitor) = monitors.item(i).and_downcast::<gdk::Monitor>() {
                let notify = changed.clone();
                let id = monitor.connect_notify_local(None, move |_, p| {
                    if matches!(p.name(), "geometry" | "scale" | "scale-factor") {
                        notify();
                    }
                });
                signals.push((monitor.upcast(), id));
            }
        }
        Self(signals)
    }
}
impl Drop for LayoutWatch {
    fn drop(&mut self) {
        for (object, id) in self.0.drain(..) {
            object.disconnect(id);
        }
    }
}
