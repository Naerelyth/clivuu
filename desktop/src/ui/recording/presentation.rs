//! Bounded visual transitions, independent of acquisition and input state.
use crate::editor::{
    capture_draw::{self, CaptureStyle},
    capture_view::Canvas,
};
use adw::prelude::*;
use clivuu_core::{annotation::Bounds, recording::Phase};
use gtk::{gdk, glib};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(super) struct Feedback {
    value: Rc<Cell<gdk::RGBA>>,
    accent: Cell<gdk::RGBA>,
    style: RefCell<CaptureStyle>,
    animation: RefCell<Option<adw::TimedAnimation>>,
}
impl Feedback {
    pub fn new(accent: gdk::RGBA, style: CaptureStyle) -> Self {
        Self {
            value: Rc::new(Cell::new(accent)),
            accent: Cell::new(accent),
            style: RefCell::new(style),
            animation: RefCell::new(None),
        }
    }
    pub fn set_appearance(
        &self,
        phase: Phase,
        accent: gdk::RGBA,
        style: CaptureStyle,
        frames: &[Canvas],
    ) {
        self.accent.set(accent);
        *self.style.borrow_mut() = style;
        if phase != Phase::Recording {
            if let Some(previous) = self.animation.take() {
                previous.pause();
            }
            self.value.set(accent);
        }
        for frame in frames {
            frame.queue_draw();
        }
    }
    pub fn resize_margin(&self, base: f64) -> f64 {
        capture_draw::resize_margin(&self.style.borrow(), base)
    }
    pub fn update(&self, widget: &impl IsA<gtk::Widget>, phase: Phase, frames: &[Canvas]) {
        if let Some(previous) = self.animation.take() {
            previous.pause();
        }
        let from = self.value.get();
        let to = if phase == Phase::Recording {
            gdk::RGBA::new(1., 0.33, 0.33, 1.)
        } else {
            self.accent.get()
        };
        if from == to {
            return;
        }
        let value = self.value.clone();
        let frames = frames.iter().map(|f| f.downgrade()).collect::<Vec<_>>();
        let target = adw::CallbackAnimationTarget::new(move |progress| {
            let mix = |a: f32, b: f32| a + (b - a) * progress as f32;
            value.set(gdk::RGBA::new(
                mix(from.red(), to.red()),
                mix(from.green(), to.green()),
                mix(from.blue(), to.blue()),
                mix(from.alpha(), to.alpha()),
            ));
            for frame in frames.iter().filter_map(glib::WeakRef::upgrade) {
                frame.queue_draw();
            }
        });
        // Libadwaita skips animation when GTK animations are disabled or the
        // owning widget is unmapped, including reduced-motion preferences.
        let animation = adw::TimedAnimation::new(widget, 0., 1., 180, target);
        animation.set_easing(adw::Easing::EaseOutCubic);
        animation.play();
        self.animation.replace(Some(animation));
    }
    pub fn paint(&self, snapshot: &gtk::Snapshot, rect: Bounds, editable: bool) {
        // Handles disappear immediately when Ready ends, before acquisition.
        // Only the outside border color transitions while recording starts.
        let mut style = self.style.borrow().clone();
        style.accent = self.value.get();
        style.anchors &= editable;
        capture_draw::frame(snapshot, rect, &style);
    }
}
impl Drop for Feedback {
    fn drop(&mut self) {
        if let Some(animation) = self.animation.take() {
            animation.pause();
        }
    }
}
pub(super) fn icons(names: &[&'static str]) -> gtk::Stack {
    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(160)
        .build();
    for name in names {
        stack.add_named(&crate::ui::icons::icon(name), Some(name));
    }
    stack
}
