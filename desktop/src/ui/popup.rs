//! In-surface panels keep IME coordinates in the editor's native surface.
//! The surface adapter adds their bounds to its input shape; no popup window,
//! keyboard grab or full-screen persistent input region is introduced here.
use clivuu_core::annotation::Bounds;
use gtk::{gdk, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
struct Open {
    frame: gtk::Frame,
    scroll: gtk::ScrolledWindow,
    anchor: glib::WeakRef<gtk::Widget>,
    previous_focus: Option<glib::WeakRef<gtk::Widget>>,
    closed: Box<dyn Fn()>,
}
struct Inner {
    root: glib::WeakRef<super::viewport::Viewport>,
    open: RefCell<Option<Open>>,
    viewport: Cell<Option<Bounds>>,
    changed: RefCell<Vec<Box<dyn Fn()>>>,
    controllers: RefCell<Vec<gtk::EventController>>,
    unmap: RefCell<Option<glib::SignalHandlerId>>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(root) = self.root.upgrade() {
            for controller in self.controllers.take() {
                root.remove_controller(&controller);
            }
            if let Some(signal) = self.unmap.take() {
                root.disconnect(signal);
            }
            if let Some(open) = self.open.take() {
                root.remove(&open.frame);
                open.scroll.set_child(None::<&gtk::Widget>);
                open.frame.set_child(None::<&gtk::Widget>);
            }
        }
    }
}
#[derive(Clone)]
pub struct PopupLayer(Rc<Inner>);
impl PopupLayer {
    pub fn new(root: &super::viewport::Viewport) -> Self {
        let layer = Self(Rc::new(Inner {
            root: root.downgrade(),
            open: RefCell::new(None),
            viewport: Cell::new(None),
            changed: RefCell::new(vec![]),
            controllers: RefCell::new(vec![]),
            unmap: RefCell::new(None),
        }));
        let click = gtk::GestureClick::builder()
            .button(0)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = Rc::downgrade(&layer.0);
        click.connect_pressed(move |_, _, x, y| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let this = Self(inner);
            let Some(root) = this.0.root.upgrade() else {
                return;
            };
            let outside = this.0.open.borrow().as_ref().is_some_and(|p| {
                let inside = |w: &gtk::Widget| {
                    w.compute_bounds(&root).is_some_and(|b| {
                        b.contains_point(&gtk::graphene::Point::new(x as f32, y as f32))
                    })
                };
                !inside(p.frame.upcast_ref()) && !p.anchor.upgrade().is_some_and(|a| inside(&a))
            });
            if outside {
                this.close();
            }
        });
        root.add_controller(click.clone());
        layer.0.controllers.borrow_mut().push(click.upcast());
        let keys = gtk::EventControllerKey::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = Rc::downgrade(&layer.0);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(inner) = weak.upgrade()
            {
                let layer = Self(inner);
                if layer.is_open() {
                    let text_focused_in_popup = layer
                        .0
                        .root
                        .upgrade()
                        .and_then(|r| r.root())
                        .and_then(|r| r.focus())
                        .is_some_and(|f| {
                            (f.is::<gtk::Text>() || f.is::<gtk::TextView>())
                                && layer.0.open.borrow().as_ref().is_some_and(|o| {
                                    f.is_ancestor(o.frame.upcast_ref::<gtk::Widget>())
                                })
                        });
                    if text_focused_in_popup {
                        return glib::Propagation::Proceed;
                    }
                    layer.close();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        root.add_controller(keys.clone());
        layer.0.controllers.borrow_mut().push(keys.upcast());
        let bubble_keys = gtk::EventControllerKey::builder()
            .propagation_phase(gtk::PropagationPhase::Bubble)
            .build();
        let weak = Rc::downgrade(&layer.0);
        bubble_keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape
                && let Some(inner) = weak.upgrade()
            {
                let layer = Self(inner);
                if layer.is_open() {
                    layer.close();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        root.add_controller(bubble_keys.clone());
        layer.0.controllers.borrow_mut().push(bubble_keys.upcast());
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(&layer.0);
        focus.connect_leave(move |_| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).close();
            }
        });
        root.add_controller(focus.clone());
        layer.0.controllers.borrow_mut().push(focus.upcast());
        let weak = Rc::downgrade(&layer.0);
        let unmap = root.connect_unmap(move |_| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).close();
            }
        });
        layer.0.unmap.replace(Some(unmap));
        layer
    }
    /// Available monitor bounds, expressed in the host's local coordinates.
    pub fn set_viewport(&self, viewport: Bounds) {
        self.0.viewport.set(Some(viewport));
    }
    fn viewport(&self, root: &super::viewport::Viewport) -> Bounds {
        self.0.viewport.get().unwrap_or(Bounds {
            x: 0.,
            y: 0.,
            width: root.width() as f64,
            height: root.height() as f64,
        })
    }
    pub fn is_open(&self) -> bool {
        self.0.open.borrow().is_some()
    }
    pub fn connect_changed(&self, callback: impl Fn() + 'static) {
        self.0.changed.borrow_mut().push(Box::new(callback));
    }
    fn changed(&self) {
        for f in self.0.changed.borrow().iter() {
            f();
        }
    }
    pub fn close(&self) {
        let old = self.0.open.borrow_mut().take();
        if let Some(old) = old {
            let root_focus = self
                .0
                .root
                .upgrade()
                .and_then(|r| r.root())
                .and_then(|r| r.focus());
            let focus_inside_popup = root_focus.as_ref().is_some_and(|w| {
                w == old.frame.upcast_ref::<gtk::Widget>()
                    || w.is_ancestor(old.frame.upcast_ref::<gtk::Widget>())
                    || old
                        .anchor
                        .upgrade()
                        .is_some_and(|a| w == &a || w.is_ancestor(&a))
            });
            if root_focus.is_none() || focus_inside_popup {
                if let Some(focus) = old
                    .previous_focus
                    .as_ref()
                    .and_then(|w| w.upgrade())
                    .filter(|w| w.is_mapped())
                {
                    focus.grab_focus();
                } else if let Some(root) = self
                    .0
                    .root
                    .upgrade()
                    .filter(|r| r.is_mapped() && r.is_focusable())
                {
                    root.grab_focus();
                } else if let Some(window) = self
                    .0
                    .root
                    .upgrade()
                    .and_then(|r| r.root())
                    .and_downcast::<gtk::Window>()
                {
                    gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
                }
            }
            if let Some(root) = self.0.root.upgrade() {
                root.remove(&old.frame);
            }
            old.scroll.set_child(None::<&gtk::Widget>);
            old.frame.set_child(None::<&gtk::Widget>);
            (old.closed)();
            self.changed();
        }
    }
    pub fn show(
        &self,
        anchor: &impl IsA<gtk::Widget>,
        child: &impl IsA<gtk::Widget>,
        closed: impl Fn() + 'static,
    ) {
        let same = self
            .0
            .open
            .borrow()
            .as_ref()
            .and_then(|o| o.anchor.upgrade())
            .as_ref()
            == Some(anchor.as_ref());
        self.close();
        if same {
            return;
        }
        let Some(root) = self.0.root.upgrade() else {
            return;
        };
        // A size request is a minimum, not an allocation limit. Let the
        // scroller cap natural size while preserving the normal content size.
        let scroll = gtk::ScrolledWindow::builder()
            .child(child)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_width(true)
            .propagate_natural_height(true)
            .build();
        let frame = gtk::Frame::builder()
            .child(&scroll)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build();
        frame.add_css_class("clivuu-inline-popover");
        // Parent `frame` into `root` BEFORE any `measure()` call so descendant
        // `.clivuu-inline-popover ...` CSS selectors resolve instead of caching
        // unparented Adwaita button heights inside `GtkScrolledWindow`.
        root.put(&frame, 0., 0.);
        let viewport = self.viewport(&root);
        let available_width = (viewport.width as i32 - 16).max(1);
        let (min_cw, nat_cw, _, _) = child.as_ref().measure(gtk::Orientation::Horizontal, -1);
        let width = min_cw.max(nat_cw);
        let (min_ch, nat_ch, _, _) = child
            .as_ref()
            .measure(gtk::Orientation::Vertical, width.min(available_width));
        let height = min_ch.max(nat_ch);
        if width <= available_width {
            scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        }
        if height <= viewport.height as i32 - 16 {
            scroll.set_vscrollbar_policy(gtk::PolicyType::Never);
        }
        let tick_frame = frame.clone();
        self.0.open.replace(Some(Open {
            frame,
            scroll,
            anchor: anchor.as_ref().downgrade(),
            previous_focus: root.root().and_then(|r| r.focus()).map(|w| w.downgrade()),
            closed: Box::new(closed),
        }));
        self.reposition();
        self.changed();
        let weak = Rc::downgrade(&self.0);
        tick_frame.add_tick_callback(move |w, _| {
            if let Some(inner) = weak.upgrade() {
                let still_current = inner
                    .open
                    .borrow()
                    .as_ref()
                    .is_some_and(|o| o.frame.upcast_ref::<gtk::Widget>() == w);
                if still_current {
                    let layer = Self(inner);
                    layer.reposition();
                    layer.changed();
                }
            }
            glib::ControlFlow::Break
        });
    }
    pub fn reposition(&self) {
        let Some(root) = self.0.root.upgrade() else {
            return;
        };
        let (frame, scroll, anchor) = {
            let open = self.0.open.borrow();
            let Some(p) = open.as_ref() else {
                return;
            };
            let Some(anchor) = p.anchor.upgrade() else {
                return;
            };
            (p.frame.clone(), p.scroll.clone(), anchor)
        };
        let Some(a) = anchor.compute_bounds(&root) else {
            return;
        };
        let viewport = self.viewport(&root);
        let (rw, rh) = (viewport.width, viewport.height);
        let left = viewport.x + 8.;
        let top = viewport.y + 8.;
        let right = viewport.x + rw;
        let bottom = viewport.y + rh;
        let margin = 8.;
        // Account for the frame's actual CSS border/padding; do not duplicate
        // theme constants in the allocation code.
        let (min_fw, nat_fw, _, _) = frame.measure(gtk::Orientation::Horizontal, -1);
        let frame_width = min_fw.max(nat_fw);
        let (min_sw, nat_sw, _, _) = scroll.measure(gtk::Orientation::Horizontal, -1);
        let content_width = min_sw.max(nat_sw);
        let horizontal = (frame_width - content_width).max(0);
        scroll.set_max_content_width(((rw - 2. * margin) as i32 - horizontal).max(1));
        let (min_w, nat_w, _, _) = frame.measure(gtk::Orientation::Horizontal, -1);
        let w = min_w.max(nat_w);
        let (min_fh, nat_fh, _, _) = frame.measure(gtk::Orientation::Vertical, w);
        let frame_height = min_fh.max(nat_fh);
        let (min_sh, nat_sh, _, _) =
            scroll.measure(gtk::Orientation::Vertical, (w - horizontal).max(1));
        let content_height = min_sh.max(nat_sh);
        let vertical = (frame_height - content_height).max(0);
        scroll.set_max_content_height(((rh - 2. * margin) as i32 - vertical).max(1));
        let (min_h, nat_h, _, _) = frame.measure(gtk::Orientation::Vertical, w);
        let h = min_h.max(nat_h);
        let (w, h) = (w as f64, h as f64);
        let x = (a.x() as f64)
            .round()
            .clamp(left, (right - w - margin).max(left));
        let gap = 4.;
        let below = (a.y() as f64 + a.height() as f64 + gap).max(top);
        let above = (a.y() as f64 - gap).min(bottom - margin);
        let space_below = (bottom - margin - below).max(0.);
        let space_above = (above - top).max(0.);
        let max_scroll_h = ((rh - 2. * margin) as i32 - vertical).max(1);
        let y = if below + h <= bottom - margin {
            below
        } else if above - h >= top {
            above - h
        } else if space_below >= space_above {
            let max_h = (space_below as i32 - vertical).max(24);
            scroll.set_max_content_height(max_h.min(max_scroll_h));
            let (min_ch, nat_ch, _, _) = frame.measure(gtk::Orientation::Vertical, w as i32);
            let clamped_h = min_ch.max(nat_ch);
            below.clamp(top, (bottom - margin - clamped_h as f64).max(top))
        } else {
            let max_h = (space_above as i32 - vertical).max(24);
            scroll.set_max_content_height(max_h.min(max_scroll_h));
            let (min_ch, nat_ch, _, _) = frame.measure(gtk::Orientation::Vertical, w as i32);
            let clamped_h = min_ch.max(nat_ch);
            (above - clamped_h as f64).clamp(top, (bottom - margin - clamped_h as f64).max(top))
        };
        let y = y.round().clamp(top, (bottom - margin).max(top));
        root.move_(&frame, x, y);
    }
    pub fn bounds(&self) -> Option<gtk::graphene::Rect> {
        let root = self.0.root.upgrade()?;
        self.0.open.borrow().as_ref()?.frame.compute_bounds(&root)
    }
}
