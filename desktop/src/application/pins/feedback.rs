//! Temporary pin percentages live on existing hosts, outside image/export state.
use super::*;
use crate::ui::viewport::Viewport;

#[derive(Default)]
pub(super) struct Feedback {
    id: Option<u64>,
    text: String,
    widgets: Vec<(glib::WeakRef<Viewport>, [f64; 2], gtk::Label)>,
    timer: Option<glib::SourceId>,
}
impl Drop for Feedback {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }
        for (root, _, label) in self.widgets.drain(..) {
            if let Some(root) = root.upgrade()
                && label.parent().is_some()
            {
                root.remove(&label);
            }
        }
    }
}
impl Feedback {
    pub fn status(&self) -> serde_json::Value {
        use serde_json::json;
        let rects: Vec<_> = self.widgets.iter().filter_map(|(root, origin, label)| {
            let root = root.upgrade()?;
            let r = label.is_visible().then(|| label.compute_bounds(&root)).flatten()?;
            Some(json!({"x":r.x() as f64+origin[0],"y":r.y() as f64+origin[1],"width":r.width(),"height":r.height()}))
        }).collect();
        json!({"id":self.id,"text":self.text,"rects":rects})
    }
}
impl Pins {
    pub(super) fn show_feedback(self: &Rc<Self>, id: u64, text: String) {
        let mut feedback = self.feedback.borrow_mut();
        if let Some(timer) = feedback.timer.take() {
            timer.remove();
        }
        feedback.id = Some(id);
        feedback.text = text;
        let weak = Rc::downgrade(self);
        // Reference TipLabel restarts a 1000ms timer on each percentage change.
        feedback.timer = Some(glib::timeout_add_local_once(
            std::time::Duration::from_millis(1000),
            move || {
                if let Some(this) = weak.upgrade() {
                    let mut feedback = this.feedback.borrow_mut();
                    feedback.timer.take();
                    feedback.id = None;
                    for (_, _, label) in &feedback.widgets {
                        label.set_visible(false);
                    }
                }
            },
        ));
    }
    pub(super) fn update_feedback(&self) {
        let hosts = self.hosts.borrow();
        let Some(hosts) = hosts.as_ref() else { return };
        let board = self.board.borrow();
        let mut feedback = self.feedback.borrow_mut();
        let rect = feedback
            .id
            .and_then(|id| board.get(id))
            .filter(|p| board.visible(p) && !self.suspended.get() && !self.locked.get())
            .map(Pin::rect);
        if rect.is_some() && feedback.widgets.is_empty() {
            for surface in &hosts.surfaces {
                let label = gtk::Label::builder()
                    .can_target(false)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .xalign(0.)
                    .visible(false)
                    .build();
                label.add_css_class("clivuu-pin-feedback");
                surface.root.put(&label, 0., 0.);
                feedback.widgets.push((
                    surface.root.downgrade(),
                    [surface.rect.x, surface.rect.y],
                    label,
                ));
            }
        }
        let owner = rect.map(|r| {
            hosts
                .surfaces
                .iter()
                .position(|s| contains(s.rect, [r.x + 2., r.y + 2.]))
                .unwrap_or_else(|| output_owner(hosts, r))
        });
        for (i, (root, origin, label)) in feedback.widgets.iter().enumerate() {
            let visible = owner == Some(i);
            label.set_visible(visible);
            let (Some(rect), Some(root)) = (rect, root.upgrade()) else {
                continue;
            };
            if !visible {
                continue;
            }
            label.set_label(&feedback.text);
            let output = hosts.surfaces[i].rect;
            let (_, width, _, _) = label.measure(gtk::Orientation::Horizontal, -1);
            let width = width.min(output.width as i32).max(1);
            let (_, height, _, _) = label.measure(gtk::Orientation::Vertical, width);
            let x = (rect.x + 2.).clamp(
                output.x,
                (output.x + output.width - width as f64).max(output.x),
            );
            let y = (rect.y + 2.).clamp(
                output.y,
                (output.y + output.height - height as f64).max(output.y),
            );
            root.move_(label, x - origin[0], y - origin[1]);
            root.set_child_size(label, Some((width, height)));
        }
    }
}
