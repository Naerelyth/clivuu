//! Native Wayland popups keep their pin's focus; XWayland retains passive overlays.
use super::{
    MenuWatch, PinRole,
    pins::{Hosts, Surface},
};
use clivuu_core::{annotation::Bounds, capture::contains};
use gtk::{gdk, glib, prelude::*};
use std::rc::Rc;
enum Host {
    Popup {
        popover: gtk::Popover,
        parent: Rc<Surface>,
        closed: Option<glib::SignalHandlerId>,
    },
    Overlay {
        _watch: MenuWatch,
        hosts: Hosts,
    },
}
pub struct MenuSurface {
    host: Host,
    content: gtk::ScrolledWindow,
}
impl MenuSurface {
    pub fn new(
        display: &gdk::Display,
        parent: &Hosts,
        rect: Bounds,
        point: [f64; 2],
        content: &gtk::ScrolledWindow,
        dismiss: impl Fn(Option<[f64; 2]>) + 'static,
    ) -> Result<Self, String> {
        let dismiss: Rc<dyn Fn(Option<[f64; 2]>)> = Rc::new(dismiss);
        let owner = parent
            .surfaces
            .iter()
            .position(|s| contains(s.rect, point))
            .unwrap_or(0);
        let surface = &parent.surfaces[owner];
        if surface.uses_native_focus() {
            let content_w = (rect.width as i32 - 2).max(64);
            let content_h = (rect.height as i32 - 2).max(64);
            content.set_propagate_natural_height(true);
            content.set_min_content_width(content_w);
            content.set_max_content_width(content_w);
            content.set_min_content_height(content_h.min(64));
            content.set_max_content_height(content_h);
            let popover = gtk::Popover::builder()
                .autohide(true)
                .has_arrow(false)
                .child(content)
                .build();
            popover.add_css_class("clivuu-pin-popup");
            crate::ui::native_cursor::install(&popover);
            popover.set_parent(&surface.root);
            popover.set_position(gtk::PositionType::Bottom);
            popover.set_pointing_to(Some(&gdk::Rectangle::new(
                (point[0] - surface.rect.x).round() as i32,
                (point[1] - surface.rect.y).round() as i32,
                1,
                1,
            )));
            popover.set_offset((rect.width / 2.).round() as i32, 0);
            let notify = dismiss.clone();
            let closed = popover.connect_closed(move |_| notify(None));
            let click = gtk::GestureClick::builder()
                .button(3)
                .propagation_phase(gtk::PropagationPhase::Capture)
                .build();
            click.connect_pressed(move |g, _, _, _| {
                g.set_state(gtk::EventSequenceState::Claimed);
                dismiss(None);
            });
            popover.add_controller(click);
            Ok(Self {
                host: Host::Popup {
                    popover,
                    parent: surface.clone(),
                    closed: Some(closed),
                },
                content: content.clone(),
            })
        } else {
            let notify = dismiss.clone();
            let hosts = Hosts::new(display, PinRole::Menu, move || notify(None))?;
            let surface = &hosts.surfaces[owner];
            surface
                .root
                .put(content, rect.x - surface.rect.x, rect.y - surface.rect.y);
            surface
                .root
                .set_child_size(content, Some((rect.width as i32, rect.height as i32)));
            for s in &hosts.surfaces {
                let origin = [s.rect.x, s.rect.y];
                let notify = dismiss.clone();
                let click = gtk::GestureClick::builder()
                    .button(0)
                    .propagation_phase(gtk::PropagationPhase::Capture)
                    .build();
                click.connect_pressed(move |g, _, x, y| {
                    let point = [x + origin[0], y + origin[1]];
                    let outside = !contains(rect, point);
                    if g.current_button() == 3 || outside {
                        g.set_state(gtk::EventSequenceState::Claimed);
                        notify(outside.then_some(point));
                    }
                });
                s.root.add_controller(click);
                s.set_input([s.rect]);
            }
            let watch = hosts.watch_menu(move || dismiss(None));
            Ok(Self {
                host: Host::Overlay {
                    _watch: watch,
                    hosts,
                },
                content: content.clone(),
            })
        }
    }
    /// Clone presentation handles so the owner can release its state borrow
    /// before GTK emits popup/focus signals synchronously.
    pub fn presenter(&self) -> Box<dyn FnOnce()> {
        match &self.host {
            Host::Popup { popover, .. } => {
                let popup = popover.clone();
                Box::new(move || popup.popup())
            }
            Host::Overlay { hosts, .. } => {
                let windows: Vec<_> = hosts.surfaces.iter().map(|s| s.window.clone()).collect();
                Box::new(move || {
                    for window in windows {
                        window.set_visible(true);
                    }
                })
            }
        }
    }
    pub fn frame_rect(&self) -> Option<Bounds> {
        if !self.content.is_mapped() {
            return None;
        }
        self.widget_rect(&self.content)
    }
    pub fn button_rect(&self, button: &gtk::Button) -> Option<Bounds> {
        self.widget_rect(button)
    }
    fn widget_rect(&self, widget: &impl IsA<gtk::Widget>) -> Option<Bounds> {
        match &self.host {
            Host::Popup {
                popover, parent, ..
            } => {
                let r = widget.compute_bounds(popover)?;
                if r.width() <= 0. || r.height() <= 0. {
                    return None;
                }
                let popup = popover.surface().and_downcast::<gdk::Popup>()?;
                Some(Bounds {
                    x: r.x() as f64 + popup.position_x() as f64 + parent.rect.x,
                    y: r.y() as f64 + popup.position_y() as f64 + parent.rect.y,
                    width: r.width() as f64,
                    height: r.height() as f64,
                })
            }
            Host::Overlay { hosts, .. } => hosts.surfaces.iter().find_map(|s| {
                widget
                    .compute_bounds(&s.root)
                    .filter(|r| r.width() > 0. && r.height() > 0.)
                    .map(|r| Bounds {
                        x: r.x() as f64 + s.rect.x,
                        y: r.y() as f64 + s.rect.y,
                        width: r.width() as f64,
                        height: r.height() as f64,
                    })
            }),
        }
    }
}
impl Drop for MenuSurface {
    fn drop(&mut self) {
        if let Host::Popup {
            popover, closed, ..
        } = &mut self.host
        {
            if let Some(id) = closed.take() {
                popover.disconnect(id);
            }
            popover.popdown();
            popover.unparent();
        }
    }
}
