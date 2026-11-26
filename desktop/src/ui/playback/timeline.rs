//! Source-time timeline with distinct trim handles and playhead hit targets.
use clivuu_core::recording::{self, MaterialInfo, PlaybackEdit};
use gtk::{gdk, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
#[derive(Clone, Copy)]
pub(super) enum Change {
    Seek(u64),
    Trim(u64, u64),
}
#[derive(Clone)]
pub(super) struct Timeline(Rc<Inner>);
struct Inner {
    area: gtk::DrawingArea,
    duration: u64,
    range: Cell<(u64, u64)>,
    position: Cell<u64>,
    drag: Cell<Option<(u8, f64)>>,
    hover: Cell<u8>,
    accent: Cell<gdk::RGBA>,
    changed: RefCell<Box<dyn Fn(Change)>>,
}
fn handle_at(x: f64, width: f64, duration: u64, (start, end): (u64, u64)) -> u8 {
    let span = duration.max(1) as f64;
    let a = 8. + start as f64 / span * width;
    let b = 8. + end as f64 / span * width;
    let da = (x - a).abs();
    let db = (x - b).abs();
    if da <= 8. && db <= 8. {
        if da <= db { 1 } else { 2 }
    } else if da <= 8. {
        1
    } else if db <= 8. {
        2
    } else {
        0
    }
}
fn rounded_rect(c: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.).min(h / 2.).max(0.);
    if w <= 0. || h <= 0. {
        return;
    }
    let pi = std::f64::consts::PI;
    c.new_sub_path();
    c.arc(x + w - r, y + r, r, -pi / 2., 0.);
    c.arc(x + w - r, y + h - r, r, 0., pi / 2.);
    c.arc(x + r, y + h - r, r, pi / 2., pi);
    c.arc(x + r, y + r, r, pi, pi * 1.5);
    c.close_path();
}
fn time_text(ns: u64) -> String {
    recording::elapsed_label(Duration::from_nanos(ns))
}
impl Timeline {
    pub fn new(info: &MaterialInfo, accent: gdk::RGBA, changed: impl Fn(Change) + 'static) -> Self {
        let area = gtk::DrawingArea::builder()
            .content_height(18)
            .margin_start(6)
            .margin_end(6)
            .hexpand(true)
            .focusable(true)
            .build();
        area.set_cursor_from_name(Some("default"));
        area.set_tooltip_text(Some(&clivuu_core::i18n::t(
            "Drag the ends to trim, click or drag inside to seek",
        )));
        let i = Rc::new(Inner {
            area,
            duration: info.duration_ns,
            range: Cell::new((0, info.duration_ns)),
            position: Cell::new(0),
            drag: Cell::new(None),
            hover: Cell::new(0),
            accent: Cell::new(accent),
            changed: RefCell::new(Box::new(changed)),
        });
        let weak = Rc::downgrade(&i);
        i.area.set_draw_func(move |area, c, w, h| {
            let Some(i) = weak.upgrade() else {
                return;
            };
            let h = h as f64;
            let width = (w as f64 - 16.).max(1.);
            let span = i.duration.max(1) as f64;
            let at = |n| 8. + n as f64 / span * width;
            let cy = h / 2.;
            let track_h = 5.;
            let track_y = cy - track_h / 2.;
            let radius = track_h / 2.;
            let ink = area.color();
            let dark = (ink.red() + ink.green() + ink.blue()) > 1.5;

            // Quiet full-length groove for trimmed-out media spans.
            rounded_rect(c, 8., track_y, width, track_h, radius);
            if dark {
                c.set_source_rgba(1., 1., 1., 0.14);
            } else {
                c.set_source_rgba(0., 0., 0., 0.12);
            }
            let _ = c.fill_preserve();
            c.set_source_rgba(
                ink.red() as f64,
                ink.green() as f64,
                ink.blue() as f64,
                0.14,
            );
            c.set_line_width(0.8);
            let _ = c.stroke();

            // Active retained span + vivid played progress up to the playhead.
            let accent = i.accent.get();
            let (start, end) = i.range.get();
            let sx = at(start);
            let ex = at(end).max(sx);
            let active_w = (ex - sx).max(2.);
            rounded_rect(c, sx, track_y, active_w, track_h, radius);
            c.set_source_rgba(
                accent.red() as f64,
                accent.green() as f64,
                accent.blue() as f64,
                if dark { 0.36 } else { 0.30 },
            );
            let _ = c.fill();

            let px = at(i.position.get()).clamp(sx, ex);
            if px > sx + 0.5 {
                let _ = c.save();
                rounded_rect(c, sx, track_y, active_w, track_h, radius);
                c.clip();
                c.rectangle(sx, track_y, px - sx, track_h);
                c.set_source_rgba(
                    accent.red() as f64,
                    accent.green() as f64,
                    accent.blue() as f64,
                    0.95,
                );
                let _ = c.fill();
                let _ = c.restore();
            }

            rounded_rect(c, sx, track_y, active_w, track_h, radius);
            c.set_source_rgba(
                accent.red() as f64,
                accent.green() as f64,
                accent.blue() as f64,
                0.6,
            );
            c.set_line_width(0.8);
            let _ = c.stroke();

            // Sculpted pill trim handles with hover/drag halo and grip notch.
            let active = i.drag.get().map(|d| d.0).unwrap_or_else(|| i.hover.get());
            let hh = (h - 5.).clamp(11., 15.);
            let hy = cy - hh / 2.;
            for (idx, n) in [(1u8, start), (2u8, end)] {
                let focused = active == idx;
                let hw = if focused { 8. } else { 7. };
                let hx = at(n) - hw / 2.;
                if focused {
                    rounded_rect(c, hx - 1., hy - 1., hw + 2., hh + 2., 3.5);
                    c.set_source_rgba(
                        accent.red() as f64,
                        accent.green() as f64,
                        accent.blue() as f64,
                        0.38,
                    );
                    c.set_line_width(2.);
                    let _ = c.stroke();
                }
                rounded_rect(c, hx, hy + 0.8, hw, hh, 2.8);
                c.set_source_rgba(0., 0., 0., if dark { 0.35 } else { 0.16 });
                let _ = c.fill();

                rounded_rect(c, hx, hy, hw, hh, 2.8);
                if focused {
                    c.set_source_rgba(1., 1., 1., 1.);
                } else if dark {
                    c.set_source_rgba(0.92, 0.93, 0.96, 1.);
                } else {
                    c.set_source_rgba(0.98, 0.98, 0.99, 1.);
                }
                let _ = c.fill();

                rounded_rect(c, hx + 0.5, hy + 0.5, hw - 1., hh - 1., 2.3);
                if focused {
                    c.set_source_rgba(
                        accent.red() as f64,
                        accent.green() as f64,
                        accent.blue() as f64,
                        1.,
                    );
                } else {
                    c.set_source_rgba(0.16, 0.17, 0.22, if dark { 0.6 } else { 0.45 });
                }
                c.set_line_width(1.);
                let _ = c.stroke();

                c.set_line_width(1.2);
                c.set_line_cap(gtk::cairo::LineCap::Round);
                if focused {
                    c.set_source_rgba(
                        accent.red() as f64,
                        accent.green() as f64,
                        accent.blue() as f64,
                        1.,
                    );
                } else {
                    c.set_source_rgba(0.22, 0.24, 0.30, 0.65);
                }
                c.move_to(hx + hw / 2., cy - hh * 0.22);
                c.line_to(hx + hw / 2., cy + hh * 0.22);
                let _ = c.stroke();
            }

            // High-contrast playhead needle with top scrubber bead.
            let play_x = at(i.position.get()).clamp(8., 8. + width);
            c.set_line_cap(gtk::cairo::LineCap::Round);
            c.set_line_width(3.);
            if dark {
                c.set_source_rgba(0.11, 0.12, 0.16, 0.45);
            } else {
                c.set_source_rgba(1., 1., 1., 0.65);
            }
            c.move_to(play_x, 3.);
            c.line_to(play_x, h - 2.);
            let _ = c.stroke();

            c.set_line_width(1.6);
            c.set_source_rgba(
                ink.red() as f64,
                ink.green() as f64,
                ink.blue() as f64,
                ink.alpha() as f64,
            );
            c.move_to(play_x, 3.);
            c.line_to(play_x, h - 2.);
            let _ = c.stroke();

            c.arc(play_x, 2.8, 2.3, 0., std::f64::consts::TAU);
            let _ = c.fill_preserve();
            if dark {
                c.set_source_rgba(0.16, 0.17, 0.21, 0.75);
            } else {
                c.set_source_rgba(1., 1., 1., 0.85);
            }
            c.set_line_width(0.9);
            let _ = c.stroke();
        });
        let drag = gtk::GestureDrag::builder().button(1).build();
        let weak = Rc::downgrade(&i);
        drag.connect_drag_begin(move |g, x, _| {
            if let Some(i) = weak.upgrade() {
                let width = (i.area.width() as f64 - 16.).max(1.);
                let handle = handle_at(x, width, i.duration, i.range.get());
                i.drag.set(Some((handle, x)));
                i.area.set_cursor_from_name(Some(if handle == 0 {
                    "grabbing"
                } else {
                    "ew-resize"
                }));
                g.set_state(gtk::EventSequenceState::Claimed);
                i.motion(x);
            }
        });
        let weak = Rc::downgrade(&i);
        drag.connect_drag_update(move |_, dx, _| {
            if let Some(i) = weak.upgrade()
                && let Some((_, x)) = i.drag.get()
            {
                i.motion(x + dx);
            }
        });
        let weak = Rc::downgrade(&i);
        drag.connect_drag_end(move |_, _, _| {
            if let Some(i) = weak.upgrade() {
                i.drag.set(None);
                i.area.set_cursor_from_name(Some(if i.hover.get() == 0 {
                    "default"
                } else {
                    "ew-resize"
                }));
                i.area.queue_draw();
            }
        });
        let weak = Rc::downgrade(&i);
        drag.connect_cancel(move |_, _| {
            if let Some(i) = weak.upgrade() {
                i.drag.set(None);
                i.area.set_cursor_from_name(Some("default"));
                i.area.queue_draw();
            }
        });
        i.area.add_controller(drag);

        let motion = gtk::EventControllerMotion::new();
        let weak = Rc::downgrade(&i);
        motion.connect_motion(move |_, x, _| {
            if let Some(i) = weak.upgrade() {
                let width = (i.area.width() as f64 - 16.).max(1.);
                let handle = if let Some((h, _)) = i.drag.get() {
                    h
                } else {
                    handle_at(x, width, i.duration, i.range.get())
                };
                if i.hover.replace(handle) != handle {
                    i.area.queue_draw();
                }
                i.area.set_cursor_from_name(Some(if handle != 0 {
                    "ew-resize"
                } else if i.drag.get().is_some() {
                    "grabbing"
                } else if x >= 8. && x <= width + 8. {
                    "pointer"
                } else {
                    "default"
                }));
                if i.drag.get().is_none() {
                    let t_ns =
                        (((x - 8.) / width).clamp(0., 1.) * i.duration as f64).round() as u64;
                    i.area.set_tooltip_text(Some(&time_text(t_ns)));
                }
            }
        });
        let weak = Rc::downgrade(&i);
        motion.connect_leave(move |_| {
            if let Some(i) = weak.upgrade() {
                if i.hover.replace(0) != 0 {
                    i.area.queue_draw();
                }
                if i.drag.get().is_none() {
                    i.area.set_cursor_from_name(Some("default"));
                    i.area.set_tooltip_text(Some(&clivuu_core::i18n::t(
                        "Drag the ends to trim, click or drag inside to seek",
                    )));
                }
            }
        });
        i.area.add_controller(motion);

        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&i);
        keys.connect_key_pressed(move |_, key, _, mods| {
            let Some(i) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let (start, end) = i.range.get();
            let max_pos = end.saturating_sub(1).max(start);
            let step = if mods.contains(gdk::ModifierType::SHIFT_MASK) {
                5_000_000_000
            } else {
                1_000_000_000
            };
            let target = match key {
                gdk::Key::Left | gdk::Key::KP_Left => {
                    Some(i.position.get().saturating_sub(step).clamp(start, max_pos))
                }
                gdk::Key::Right | gdk::Key::KP_Right => {
                    Some(i.position.get().saturating_add(step).clamp(start, max_pos))
                }
                gdk::Key::Home | gdk::Key::KP_Home => Some(start),
                gdk::Key::End | gdk::Key::KP_End => Some(max_pos),
                _ => None,
            };
            if let Some(pos) = target {
                (i.changed.borrow())(Change::Seek(pos));
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        i.area.add_controller(keys);

        Self(i)
    }
    pub fn position(&self) -> u64 {
        self.0.position.get()
    }
    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.0.area
    }
    pub fn set_accent(&self, accent: gdk::RGBA) {
        self.0.accent.set(accent);
        self.0.area.queue_draw();
    }
    pub fn update(&self, edit: &PlaybackEdit, position: u64) {
        self.0.range.set((edit.start_ns, edit.end_ns));
        self.0.position.set(position);
        self.0.area.queue_draw();
    }
}
impl Inner {
    fn motion(&self, x: f64) {
        let value = (((x - 8.) / (self.area.width() as f64 - 16.).max(1.)).clamp(0., 1.)
            * self.duration as f64)
            .round() as u64;
        let (start, end) = self.range.get();
        let gap = (self.duration / 100).clamp(1, 100_000_000);
        let (tip, change) = match self.drag.get().map(|v| v.0) {
            Some(1) => {
                let next = value.min(end.saturating_sub(gap));
                (
                    format!("{} – {}", time_text(next), time_text(end)),
                    Change::Trim(next, end),
                )
            }
            Some(2) => {
                let next = value.max(start.saturating_add(gap)).min(self.duration);
                (
                    format!("{} – {}", time_text(start), time_text(next)),
                    Change::Trim(start, next),
                )
            }
            _ => {
                let next = value.clamp(start, end.saturating_sub(1));
                (time_text(next), Change::Seek(next))
            }
        };
        self.area.set_tooltip_text(Some(&tip));
        self.area.trigger_tooltip_query();
        (self.changed.borrow())(change);
    }
}
