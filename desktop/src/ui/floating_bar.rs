//! Shared chrome helpers for Clivuu's 32px floating toolbars and inline popups.
use super::popup::PopupLayer;
use clivuu_core::i18n::t;
use gtk::{glib, graphene, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};

pub(crate) const TOOLBAR_HEIGHT: i32 = 32;

fn optical_ref_text(text: &str) -> String {
    let mapped: String =
        text.chars()
            .map(|c| match c {
                'g' | 'j' | 'p' | 'q' | 'y' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | ','
                | ';' => 'x',
                other => other,
            })
            .collect();
    if mapped.trim().is_empty() {
        text.to_owned()
    } else {
        mapped
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct CenteredLabel {
        pub text: RefCell<String>,
        pub max_chars: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CenteredLabel {
        const NAME: &'static str = "ClivuuCenteredLabel";
        type Type = super::CenteredLabel;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for CenteredLabel {}

    impl WidgetImpl for CenteredLabel {
        fn measure(&self, orientation: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            let obj = self.obj();
            let text = self.text.borrow();
            let layout = obj.create_pango_layout(Some(&text));
            let (ink, log) = layout.pixel_extents();
            match orientation {
                gtk::Orientation::Horizontal => {
                    let full = log.width().max(ink.x() + ink.width()).max(0);
                    let max_chars = self.max_chars.get();
                    if max_chars > 0 {
                        let ctx = obj.pango_context();
                        let metrics = ctx.metrics(None, None);
                        let char_w = metrics.approximate_char_width().max(6 * gtk::pango::SCALE);
                        let cap = (max_chars * char_w + gtk::pango::SCALE - 1) / gtk::pango::SCALE;
                        let nat = full.min(cap.max(24));
                        (nat.min(24), nat, -1, -1)
                    } else {
                        (full, full, -1, -1)
                    }
                }
                gtk::Orientation::Vertical => {
                    let h = ink.height().max(14);
                    (h, h, -1, -1)
                }
                _ => (0, 0, -1, -1),
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let width = obj.width();
            let height = obj.height();
            if width <= 0 || height <= 0 {
                return;
            }
            let text = self.text.borrow();
            if text.is_empty() {
                return;
            }
            let layout = obj.create_pango_layout(Some(&text));
            if self.max_chars.get() > 0 {
                layout.set_ellipsize(gtk::pango::EllipsizeMode::End);
                layout.set_width(width * gtk::pango::SCALE);
            }
            let ref_layout = obj.create_pango_layout(Some(&optical_ref_text(&text)));
            let (ink, _) = ref_layout.extents();
            let ink_y = ink.y() as f32 / gtk::pango::SCALE as f32;
            let ink_h = ink.height() as f32 / gtk::pango::SCALE as f32;
            let y = ((height as f32 - ink_h) * 0.5 - ink_y).round();
            snapshot.save();
            snapshot.translate(&graphene::Point::new(0.0, y));
            snapshot.append_layout(&layout, &obj.color());
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    pub struct CenteredLabel(ObjectSubclass<imp::CenteredLabel>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl CenteredLabel {
    pub(crate) fn new(text: &str) -> Self {
        let this: Self = glib::Object::new();
        this.set_hexpand(true);
        this.set_valign(gtk::Align::Fill);
        this.set_can_target(false);
        this.imp().text.replace(text.to_owned());
        this
    }

    pub(crate) fn ellipsized(text: &str, max_chars: i32) -> Self {
        let this = Self::new(text);
        this.imp().max_chars.set(max_chars);
        this
    }

    pub(crate) fn text(&self) -> String {
        self.imp().text.borrow().clone()
    }
}

fn find_centered_label_text(widget: &gtk::Widget) -> Option<String> {
    if let Some(lbl) = widget.downcast_ref::<CenteredLabel>() {
        return Some(lbl.text());
    }
    let mut child = widget.first_child();
    while let Some(w) = child {
        if let Some(found) = find_centered_label_text(&w) {
            return Some(found);
        }
        child = w.next_sibling();
    }
    None
}

pub(crate) fn button_text(button: &gtk::Button) -> Option<String> {
    button
        .tooltip_text()
        .or_else(|| button.label())
        .map(|s| s.to_string())
        .or_else(|| find_centered_label_text(button.upcast_ref()))
}

pub(crate) fn check_mark(visible: bool) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(11)
        .content_height(14)
        .valign(gtk::Align::Fill)
        .can_target(false)
        .css_classes(["clivuu-choice-check"])
        .build();
    let active = Cell::new(visible);
    area.set_draw_func(move |w, cr, _, h| {
        if !active.get() {
            return;
        }
        let rgba = w.color();
        cr.set_source_rgba(
            rgba.red().into(),
            rgba.green().into(),
            rgba.blue().into(),
            rgba.alpha().into(),
        );
        cr.set_line_width(1.65);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        cr.set_line_join(gtk::cairo::LineJoin::Round);
        let cy = h as f64 * 0.5;
        cr.move_to(1.2, cy + 0.3);
        cr.line_to(4.2, cy + 3.2);
        cr.line_to(9.8, cy - 2.8);
        let _ = cr.stroke();
    });
    area
}

pub(crate) fn icon_button(kind: &'static str) -> gtk::Button {
    gtk::Button::builder()
        .child(&super::icons::icon(kind))
        .focusable(false)
        .valign(gtk::Align::Center)
        .build()
}

pub(crate) fn bar_label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.5)
        .yalign(0.5)
        .valign(gtk::Align::Center)
        .single_line_mode(true)
        .build()
}

pub(crate) fn metric_label(text: &str, width_chars: i32, max_width_chars: i32) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .width_chars(width_chars)
        .max_width_chars(max_width_chars)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .xalign(0.5)
        .yalign(0.5)
        .valign(gtk::Align::Fill)
        .single_line_mode(true)
        .css_classes(["numeric", "clivuu-toolbar-metric"])
        .build()
}

pub(crate) fn text_button(text: &str) -> (gtk::Button, gtk::Label) {
    let label = bar_label(text);
    let button = gtk::Button::builder()
        .child(&label)
        .focusable(false)
        .valign(gtk::Align::Center)
        .css_classes(["clivuu-toolbar-text", "clivuu-toolbar-choice"])
        .build();
    (button, label)
}

pub(crate) fn choice_button(text: &str) -> (gtk::Button, gtk::Label) {
    let label = bar_label(text);
    let arrow = super::icons::icon_sized("chevron-down", 10);
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    content.append(&label);
    content.append(&arrow);
    let button = gtk::Button::builder()
        .child(&content)
        .focusable(false)
        .valign(gtk::Align::Center)
        .css_classes(["clivuu-toolbar-text", "clivuu-toolbar-choice"])
        .build();
    (button, label)
}

pub(crate) fn choice_list() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .css_classes(["clivuu-choice-list"])
        .build()
}

pub(crate) fn choice_item(label: &str, selected: bool) -> gtk::Button {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .valign(gtk::Align::Fill)
        .build();
    row.append(&CenteredLabel::new(label));
    row.append(&check_mark(selected));
    let button = gtk::Button::builder()
        .child(&row)
        .css_classes(["clivuu-choice-item"])
        .build();
    if selected {
        button.add_css_class("selected");
    }
    button
}

pub(crate) fn action_item(label: &str) -> gtk::Button {
    gtk::Button::builder()
        .child(&CenteredLabel::new(label))
        .css_classes(["clivuu-choice-item"])
        .build()
}

pub(crate) fn tool_separator() -> gtk::Separator {
    let sep = gtk::Separator::builder()
        .orientation(gtk::Orientation::Vertical)
        .valign(gtk::Align::Fill)
        .margin_start(4)
        .margin_end(4)
        .margin_top(3)
        .margin_bottom(3)
        .can_target(false)
        .build();
    sep.add_css_class("clivuu-tool-separator");
    sep
}

pub(crate) fn scrollable_bar_row() -> (gtk::ScrolledWindow, gtk::Box) {
    let row = gtk::Box::builder()
        .spacing(2)
        .valign(gtk::Align::Center)
        .build();
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&row)
        .vexpand(true)
        .propagate_natural_width(true)
        .max_content_width(1000)
        .build();
    (scroller, row)
}

pub(crate) fn sync_scroller_width(
    scroller: &gtk::ScrolledWindow,
    row: &gtk::Box,
    max_content_width: i32,
) {
    let max_content_width = max_content_width.max(1);
    let (min_w, nat_w, _, _) = row.measure(gtk::Orientation::Horizontal, -1);
    if min_w.max(nat_w) <= max_content_width {
        if scroller.hscrollbar_policy() != gtk::PolicyType::Never {
            scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        }
        let adj = scroller.hadjustment();
        if adj.value() != 0. {
            adj.set_value(0.);
        }
    } else {
        if scroller.max_content_width() != max_content_width {
            scroller.set_max_content_width(max_content_width);
        }
        if scroller.hscrollbar_policy() != gtk::PolicyType::Automatic {
            scroller.set_hscrollbar_policy(gtk::PolicyType::Automatic);
        }
    }
}

pub(crate) fn confirm_popup(
    popup: &PopupLayer,
    anchor: &impl IsA<gtk::Widget>,
    message_key: &str,
    confirm_label_key: &str,
    destructive: bool,
    on_confirm: impl Fn() + 'static,
) {
    let box_ = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .css_classes(["clivuu-confirm-popup"])
        .build();
    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(5)
        .valign(gtk::Align::Center)
        .build();
    let icon = super::icons::icon("warning");
    icon.set_content_width(13);
    icon.set_content_height(13);
    icon.set_valign(gtk::Align::Center);
    icon.add_css_class(if destructive {
        "clivuu-confirm-icon"
    } else {
        "clivuu-confirm-icon-neutral"
    });
    let label = gtk::Label::builder()
        .label(t(message_key))
        .xalign(0.)
        .valign(gtk::Align::Center)
        .max_width_chars(32)
        .wrap(true)
        .css_classes(["clivuu-confirm-message"])
        .build();
    header.append(&icon);
    header.append(&label);
    let actions = gtk::Box::builder()
        .spacing(6)
        .halign(gtk::Align::End)
        .build();
    let cancel = gtk::Button::builder()
        .label(t("Cancel"))
        .css_classes(["clivuu-confirm-button", "clivuu-confirm-cancel"])
        .build();
    let confirm = gtk::Button::builder()
        .label(t(confirm_label_key))
        .css_classes([
            "clivuu-confirm-button",
            if destructive {
                "destructive-action"
            } else {
                "suggested-action"
            },
        ])
        .build();
    let layer = popup.clone();
    cancel.connect_clicked(move |_| layer.close());
    let layer = popup.clone();
    confirm.connect_clicked(move |_| {
        layer.close();
        on_confirm();
    });
    actions.append(&cancel);
    actions.append(&confirm);
    box_.append(&header);
    box_.append(&actions);
    let weak = anchor.as_ref().downgrade();
    popup.show(anchor, &box_, move || {
        if let Some(button) = weak.upgrade().and_downcast::<gtk::ToggleButton>() {
            button.set_active(false);
        }
    });
    cancel.grab_focus();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optical_ref_text_preserves_cjk_and_normalizes_latin_descenders() {
        let cjk =
            String::from_utf8(vec![0xe6, 0x97, 0xa0, 0xe5, 0xbb, 0xb6, 0xe8, 0xbf, 0x9f]).unwrap();
        let expected_no_delay: String = ['N', 'o', ' ', 'd', 'e', 'l', 'a', 'x']
            .into_iter()
            .collect();
        let tail: String = [' ', 'x', '1', 'x'].into_iter().collect();
        let raw_tail: String = [' ', '(', '1', ')'].into_iter().collect();
        assert_eq!(optical_ref_text(&cjk), cjk);
        assert_eq!(optical_ref_text("MP4"), "MP4");
        assert_eq!(optical_ref_text("No delay"), expected_no_delay);
        assert_eq!(
            optical_ref_text(&format!("{cjk}{raw_tail}")),
            format!("{cjk}{tail}")
        );
    }
}
