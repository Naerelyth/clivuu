//! One content-sized options strip for capture and pin editing. Controls only
//! dispatch shared editor actions; they never own marks or history snapshots.
use super::{icons, popup::PopupLayer};
use crate::editor::{Action, Effect, scene::Scene};
use clivuu_core::{
    annotation::{
        Fill, Kind, Mark, StylePatch,
        arrow::{ArrowType, LineStyle, arrow_paths},
    },
    i18n::t,
};
use gtk::{gdk, glib, pango, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::OnceLock,
};
const PALETTE: [&str; 20] = [
    "#ff6058", "#000000", "#919191", "#800000", "#ff0000", "#ff9f1c", "#ffd24a", "#4caf50",
    "#4f9ded", "#7048e8", "#ffffff", "#c7c7c7", "#b17a55", "#ffb9bf", "#ffc78a", "#f5da81",
    "#e9efad", "#b6dc8a", "#95cad0", "#cbb6df",
];
#[derive(Clone, Copy, PartialEq, Eq)]
enum SpinKind {
    StrokeWidth,
    EraserWidth,
    MarkerSize,
    FontSize,
}
fn cached_font_families(context: &pango::Context) -> &'static [String] {
    static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();
    FAMILIES.get_or_init(|| {
        let mut names: Vec<String> = context
            .list_families()
            .iter()
            .map(|f| f.name().to_string())
            .collect();
        names.sort();
        names.dedup();
        for name in ["Monospace", "Serif", "Sans"] {
            names.retain(|v| v != name);
            names.insert(0, name.into());
        }
        names
    })
}
type Update = Box<dyn Fn(&Mark, f64)>;
#[derive(Clone, PartialEq)]
struct RefreshState {
    kind: Option<Kind>,
    style: Option<clivuu_core::annotation::Style>,
    text_level_bits: u64,
    sequence_style: Option<clivuu_core::annotation::Style>,
    stroke_scale_bits: u64,
    screen_scale_bits: u64,
    language: clivuu_core::i18n::Language,
}
type BuildKey = (Option<Kind>, u64, clivuu_core::i18n::Language);
mod sequence;
struct Inner {
    row: gtk::Box,
    widget: gtk::ScrolledWindow,
    scene: glib::WeakRef<Scene>,
    panels: PopupLayer,
    built: Cell<Option<BuildKey>>,
    state: RefCell<Option<RefreshState>>,
    refreshing: Cell<bool>,
    image_scale: Cell<f64>,
    updates: RefCell<Vec<Update>>,
    color_services: super::color::ColorServices,
}
#[derive(Clone)]
pub struct Subbar(Rc<Inner>);
impl Subbar {
    pub fn new(
        scene: &Scene,
        panels: &PopupLayer,
        color_services: super::color::ColorServices,
    ) -> Self {
        let row = gtk::Box::builder()
            .spacing(3)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build();
        row.add_css_class("clivuu-annotation-options");
        row.set_valign(gtk::Align::Center);
        let widget = gtk::ScrolledWindow::builder()
            .child(&row)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(true)
            .max_content_width(1000)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build();
        widget.add_css_class("clivuu-capture-toolbar");
        widget.set_size_request(-1, 32);
        let this = Self(Rc::new(Inner {
            row,
            widget,
            scene: scene.downgrade(),
            panels: panels.clone(),
            built: Cell::new(None),
            state: RefCell::new(None),
            refreshing: Cell::new(false),
            image_scale: Cell::new(1.),
            updates: RefCell::new(vec![]),
            color_services,
        }));
        this.refresh();
        let weak = Rc::downgrade(&this.0);
        scene.connect_changed(move |effect| {
            if effect != Effect::None
                && let Some(inner) = weak.upgrade()
            {
                Self(inner).refresh();
            }
        });
        this
    }
    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.0.widget
    }
    pub fn sync_output_width(&self, max_width: i32) -> i32 {
        let max_width = max_width.max(1);
        if self.0.widget.hscrollbar_policy() != gtk::PolicyType::Never {
            self.0.widget.set_hscrollbar_policy(gtk::PolicyType::Never);
        }
        let (row_min, row_nat, _, _) = self.0.row.measure(gtk::Orientation::Horizontal, -1);
        let content_width = row_min.max(row_nat);
        let (min_u, nat_u, _, _) = self.0.widget.measure(gtk::Orientation::Horizontal, -1);
        let (min_h, nat_h, _, _) = self.0.widget.measure(gtk::Orientation::Horizontal, 32);
        let full_width = min_u.max(nat_u).max(min_h).max(nat_h);
        if full_width <= max_width {
            let adj = self.0.widget.hadjustment();
            if adj.value() != 0. {
                adj.set_value(0.);
            }
            full_width
        } else {
            let chrome = (full_width - content_width).max(0);
            let content_limit = (max_width - chrome).max(1);
            if self.0.widget.max_content_width() != content_limit {
                self.0.widget.set_max_content_width(content_limit);
            }
            self.0
                .widget
                .set_hscrollbar_policy(gtk::PolicyType::Automatic);
            max_width
        }
    }
    pub fn has_popup(&self) -> bool {
        self.0.panels.is_open()
    }
    pub fn popup_bounds(&self) -> Option<gtk::graphene::Rect> {
        self.0.panels.bounds()
    }
    pub fn close_popups(&self) {
        self.0.panels.close();
    }
    pub fn row(&self) -> &gtk::Box {
        &self.0.row
    }
    fn patch(&self, patch: StylePatch) {
        if !self.0.refreshing.get()
            && let Some(scene) = self.0.scene.upgrade()
        {
            scene.dispatch(Action::Style(patch));
        }
    }
    fn update(&self, update: impl Fn(&Mark, f64) + 'static) {
        self.0.updates.borrow_mut().push(Box::new(update));
    }
    fn separator(&self) {
        self.0.row.append(&super::floating_bar::tool_separator());
    }
    fn toggle(
        &self,
        label: &str,
        icon: &'static str,
        test: impl Fn(&Mark) -> bool + 'static,
        action: impl Fn(&Self, bool) + 'static,
    ) -> gtk::ToggleButton {
        let picture = icons::icon(icon);
        if icon.starts_with("text-") {
            picture.set_content_width(14);
            picture.set_content_height(14);
            picture.set_halign(gtk::Align::Center);
            picture.set_valign(gtk::Align::Center);
        }
        let b = gtk::ToggleButton::builder()
            .child(&picture)
            .focusable(false)
            .tooltip_text(t(label))
            .build();
        b.add_css_class("clivuu-option");
        self.0.row.append(&b);
        let weak = Rc::downgrade(&self.0);
        b.connect_clicked(move |b| {
            if let Some(inner) = weak.upgrade() {
                let this = Self(inner);
                if !this.0.refreshing.get() {
                    action(&this, b.is_active());
                }
            }
        });
        let w = b.clone();
        self.update(move |m, _| w.set_active(test(m)));
        b
    }
    fn trigger(
        &self,
        label: &str,
        child: &impl IsA<gtk::Widget>,
        caret: bool,
    ) -> gtk::ToggleButton {
        let b = gtk::ToggleButton::builder()
            .focusable(false)
            .tooltip_text(t(label))
            .build();
        if caret {
            let content = gtk::Box::builder()
                .spacing(2)
                .valign(gtk::Align::Center)
                .build();
            content.append(child);
            let arrow = gtk::Image::from_icon_name("pan-down-symbolic");
            arrow.set_pixel_size(8);
            arrow.set_opacity(0.8);
            arrow.set_valign(gtk::Align::Center);
            content.append(&arrow);
            b.set_child(Some(&content));
        } else {
            b.set_child(Some(child));
        }
        b.add_css_class("clivuu-option");
        self.0.row.append(&b);
        b
    }
    fn popup(
        &self,
        label: &str,
        child: &impl IsA<gtk::Widget>,
        caret: bool,
        content: &impl IsA<gtk::Widget>,
    ) -> gtk::ToggleButton {
        self.popup_with(label, child, caret, content, || {}, || {})
    }
    fn popup_with(
        &self,
        label: &str,
        child: &impl IsA<gtk::Widget>,
        caret: bool,
        content: &impl IsA<gtk::Widget>,
        opened: impl Fn() + 'static,
        closed: impl Fn() + 'static,
    ) -> gtk::ToggleButton {
        let b = self.trigger(label, child, caret);
        let panels = self.0.panels.clone();
        let content = content.as_ref().clone();
        let opened = Rc::new(opened);
        let closed = Rc::new(closed);
        b.connect_clicked(move |btn| {
            let weak = btn.downgrade();
            let closed_cb = closed.clone();
            panels.show(btn, &content, move || {
                if let Some(btn) = weak.upgrade() {
                    btn.set_active(false);
                }
                closed_cb();
            });
            if panels.is_open() {
                opened();
            }
        });
        b
    }
    fn spin_popup(
        &self,
        kind: SpinKind,
        label: &str,
        [min, max, step]: [f64; 3],
        value: impl Fn(&Mark, f64) -> f64 + 'static,
        patch: impl Fn(f64) -> StylePatch + 'static,
    ) {
        let spin = gtk::SpinButton::with_range(min, max, step);
        spin.set_focusable(true);
        spin.set_digits(if step < 1. { 2 } else { 0 });
        let sample = gtk::Label::builder()
            .width_chars(2)
            .valign(gtk::Align::Center)
            .halign(gtk::Align::Center)
            .yalign(0.5)
            .build();
        let width = gtk::DrawingArea::builder()
            .content_width(22)
            .content_height(22)
            .build();
        width.set_widget_name("stroke-width-preview");
        let shown = Rc::new(Cell::new(3.));
        let v = shown.clone();
        let weak = Rc::downgrade(&self.0);
        width.set_draw_func(move |a, c, w, h| {
            if let Some(inner) = weak.upgrade() {
                stroke_indicator(a, c, w, h, v.get(), inner.image_scale.get());
            }
        });
        let child: gtk::Widget = match kind {
            SpinKind::StrokeWidth | SpinKind::EraserWidth => width.clone().upcast(),
            SpinKind::MarkerSize | SpinKind::FontSize => sample.clone().upcast(),
        };
        let keys = gtk::EventControllerKey::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let panels = self.0.panels.clone();
        let weak = Rc::downgrade(&self.0);
        let spin_weak = spin.downgrade();
        let commit_patch = Rc::new(patch);
        let enter_patch = commit_patch.clone();
        keys.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if let Some(s) = spin_weak.upgrade() {
                    s.update();
                    if let Some(inner) = weak.upgrade() {
                        Self(inner).patch(enter_patch(s.value()));
                    }
                }
                panels.close();
                glib::Propagation::Stop
            }
            gdk::Key::Escape => {
                panels.close();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
        spin.add_controller(keys);
        let focus_spin = spin.downgrade();
        let button = self.popup_with(
            label,
            &child,
            true,
            &spin,
            move || {
                if let Some(s) = focus_spin.upgrade() {
                    s.grab_focus();
                    s.select_region(0, -1);
                }
            },
            || {},
        );
        if kind == SpinKind::FontSize {
            spin.set_digits(2);
            button.set_valign(gtk::Align::Center);
            button.add_css_class("clivuu-font-size");
        }
        let weak = Rc::downgrade(&self.0);
        spin.connect_value_changed(move |s| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).patch(commit_patch(s.value()));
            }
        });
        self.update(move |m, scale| {
            let v = value(m, scale);
            spin.set_value(v);
            shown.set(v);
            width.queue_draw();
            sample.set_label(&size_label(v));
        });
    }
    fn fills(&self, kind: Kind) {
        let choices: &[Fill] = match kind {
            Kind::Rect | Kind::Ellipse => &[Fill::Solid],
            Kind::Highlight => &[Fill::Stroke, Fill::Line, Fill::Rect, Fill::Ellipse],
            Kind::Mosaic => &[Fill::Stroke, Fill::Rect],
            Kind::Eraser => &[Fill::Stroke, Fill::Rect],
            _ => &[],
        };
        for &fill in choices {
            let icon = match (kind, fill) {
                (Kind::Highlight | Kind::Mosaic | Kind::Eraser, Fill::Stroke) => "highlight-free",
                (Kind::Highlight, Fill::Line) => "highlight-line",
                (_, Fill::Ellipse) => "fill-ellipse",
                _ => "fill-pattern",
            };
            let label = match (kind, fill) {
                (Kind::Rect | Kind::Ellipse, _) => "Filled",
                (Kind::Highlight, Fill::Stroke) => "Freehand highlight",
                (Kind::Highlight, Fill::Line) => "Line highlight",
                (Kind::Mosaic, Fill::Stroke) => "Freehand mosaic",
                (Kind::Eraser, Fill::Stroke) => "Freehand eraser",
                (Kind::Eraser, _) => "Area eraser",
                (_, Fill::Ellipse) => "Ellipse",
                (_, Fill::Stroke) => "Outline",
                _ => "Rectangle",
            };
            let b = self.toggle(
                label,
                icon,
                move |m| m.style.fill == fill,
                move |s, active| {
                    s.patch(StylePatch {
                        fill: Some(if matches!(kind, Kind::Rect | Kind::Ellipse) && !active {
                            Fill::Stroke
                        } else {
                            fill
                        }),
                        ..Default::default()
                    })
                },
            );
            b.add_css_class("clivuu-fill");
            if let Some(picture) = b.child() {
                self.update(move |_, _| picture.queue_draw());
            }
        }
    }
    fn arrow_menu(&self, arrow: bool) {
        let sample = gtk::DrawingArea::builder()
            .content_width(32)
            .content_height(24)
            .build();
        let choice = Rc::new(Cell::new((ArrowType::default(), LineStyle::default())));
        let value = choice.clone();
        sample.set_draw_func(move |a, c, w, h| arrow_icon(a, c, w, h, arrow, value.get()));
        let list = super::floating_bar::choice_list();
        let button = self.popup(
            if arrow {
                "Arrow type"
            } else {
                "Line segment type"
            },
            &sample,
            true,
            &list,
        );
        let choices: Vec<_> = if arrow {
            ArrowType::ALL
                .iter()
                .map(|v| (*v, LineStyle::Solid))
                .collect()
        } else {
            LineStyle::ALL
                .iter()
                .map(|v| (ArrowType::Straight, *v))
                .collect()
        };
        for option in choices {
            let icon = gtk::DrawingArea::builder()
                .content_width(72)
                .content_height(24)
                .build();
            icon.set_draw_func(move |a, c, w, h| arrow_icon(a, c, w, h, arrow, option));
            let b = gtk::Button::builder()
                .child(&icon)
                .focusable(false)
                .tooltip_text(t(if arrow {
                    option.0.label()
                } else {
                    option.1.label()
                }))
                .build();
            list.append(&b);
            let weak = Rc::downgrade(&self.0);
            b.connect_clicked(move |_| {
                if let Some(inner) = weak.upgrade() {
                    let this = Self(inner);
                    this.patch(if arrow {
                        StylePatch {
                            arrow_type: Some(option.0),
                            ..Default::default()
                        }
                    } else {
                        StylePatch {
                            line_style: Some(option.1),
                            ..Default::default()
                        }
                    });
                    this.0.panels.close();
                }
            });
            self.update(move |m, _| {
                if if arrow {
                    m.style.arrow_type == option.0
                } else {
                    m.style.line_style == option.1
                } {
                    b.add_css_class("selected");
                } else {
                    b.remove_css_class("selected");
                }
            });
        }
        self.update(move |m, _| {
            choice.set((m.style.arrow_type, m.style.line_style));
            sample.queue_draw();
            button.set_sensitive(arrow || m.style.arrow_type.supports_line_style());
        });
    }
    fn strength(&self) {
        self.separator();
        let sample = gtk::DrawingArea::builder()
            .content_width(22)
            .content_height(22)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        let choice = Rc::new(RefCell::new(String::from("pixelate")));
        let value = choice.clone();
        sample.set_draw_func(move |a, c, w, h| effect_icon(a, c, w, h, &value.borrow()));
        let button = gtk::Button::builder()
            .child(&sample)
            .focusable(false)
            .tooltip_text(t("Mosaic effect"))
            .build();
        button.add_css_class("clivuu-option");
        self.0.row.append(&button);
        let current = choice.clone();
        let weak = Rc::downgrade(&self.0);
        button.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                let this = Self(inner);
                if !this.0.refreshing.get() {
                    let next = if current.borrow().as_str() == "blur" {
                        "pixelate"
                    } else {
                        "blur"
                    };
                    this.patch(StylePatch {
                        effect: Some(next.into()),
                        ..Default::default()
                    });
                }
            }
        });
        self.update(move |m, _| {
            *choice.borrow_mut() = m.style.effect.clone();
            sample.queue_draw();
        });
        let [min, max, _] = clivuu_core::image::effects::Effect::Pixelate.strength_range();
        let strength = gtk::Scale::with_range(
            gtk::Orientation::Horizontal,
            f64::from(min),
            f64::from(max),
            1.,
        );
        strength.set_increments(1., 1.);
        strength.set_size_request(128, -1);
        strength.set_draw_value(true);
        strength.set_digits(0);
        strength.set_value_pos(gtk::PositionType::Right);
        strength.set_tooltip_text(Some(&t("Effect strength")));
        let scroll = gtk::EventControllerScroll::builder()
            .flags(gtk::EventControllerScrollFlags::BOTH_AXES)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = self.0.scene.clone();
        scroll.connect_scroll(move |_, dx, dy| {
            let delta = if dy != 0. { dy } else { -dx };
            if delta != 0.
                && let Some(scene) = weak.upgrade()
            {
                scene.dispatch(Action::WheelStrength(delta));
            }
            glib::Propagation::Stop
        });
        strength.add_controller(scroll);
        let events = gtk::EventControllerLegacy::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = self.0.scene.clone();
        events.connect_event(move |_, event| {
            if let Some(scene) = weak.upgrade() {
                match event.event_type() {
                    gdk::EventType::ButtonPress | gdk::EventType::TouchBegin => {
                        scene.dispatch(Action::BeginStyleChange);
                    }
                    gdk::EventType::ButtonRelease
                    | gdk::EventType::TouchEnd
                    | gdk::EventType::TouchCancel => {
                        scene.dispatch(Action::EndStyleChange);
                    }
                    _ => {}
                }
            }
            glib::Propagation::Proceed
        });
        strength.add_controller(events);
        let weak = Rc::downgrade(&self.0);
        strength.connect_value_changed(move |s| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).patch(StylePatch {
                    effect_level: Some(s.value()),
                    ..Default::default()
                });
            }
        });
        self.0.row.append(&strength);
        self.update(move |m, _| {
            let [min, max, _] = m.style.mosaic_effect().strength_range();
            strength.set_range(f64::from(min), f64::from(max));
            strength.set_increments(1., 1.);
            strength.set_value(f64::from(m.style.mosaic_level()));
        });
    }
    fn font(&self) {
        let name = gtk::Label::builder()
            .width_chars(10)
            .max_width_chars(10)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        let trigger = self.trigger("Font family", &name, true);
        self.update(move |m, _| name.set_label(&m.style.font_family));
        let body = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();
        let search = gtk::SearchEntry::builder()
            .placeholder_text(t("Font family"))
            .build();
        search.set_widget_name("clivuu-font-search");
        body.append(&search);
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .activate_on_single_click(true)
            .build();
        let scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_width(240)
            .max_content_height(240)
            .propagate_natural_height(true)
            .build();
        body.append(&scroll);
        for family in cached_font_families(&self.0.row.pango_context()) {
            let label = super::floating_bar::CenteredLabel::ellipsized(family, 28);
            let row = gtk::ListBoxRow::builder().child(&label).build();
            row.set_widget_name(family);
            list.append(&row);
        }
        let query = Rc::new(RefCell::new(String::new()));
        let q = query.clone();
        list.set_filter_func(move |row| row.widget_name().to_lowercase().contains(&*q.borrow()));
        let items = list.clone();
        search.connect_search_changed(move |s| {
            *query.borrow_mut() = s.text().to_lowercase();
            items.invalidate_filter();
        });
        let weak = Rc::downgrade(&self.0);
        list.connect_row_activated(move |_, row| {
            if let Some(inner) = weak.upgrade() {
                let this = Self(inner);
                this.patch(StylePatch {
                    font_family: Some(row.widget_name().to_string()),
                    ..Default::default()
                });
                this.0.panels.close();
            }
        });
        let rows = list.clone();
        self.update(move |m, _| {
            let mut child = rows.first_child();
            while let Some(widget) = child {
                if widget.widget_name() == m.style.font_family {
                    if let Ok(row) = widget.downcast::<gtk::ListBoxRow>() {
                        rows.select_row(Some(&row));
                    }
                    break;
                }
                child = widget.next_sibling();
            }
        });
        let rows = list.clone();
        search.connect_activate(move |_| {
            rows.invalidate_filter();
            let mut child = rows.first_child();
            while let Some(widget) = child {
                if widget.is_child_visible() {
                    if let Ok(row) = widget.downcast::<gtk::ListBoxRow>() {
                        rows.emit_by_name::<()>("row-activated", &[&row]);
                    }
                    break;
                }
                child = widget.next_sibling();
            }
        });
        let panels = self.0.panels.clone();
        let has_preedit = Rc::new(Cell::new(false));
        let had_preedit_on_escape = Rc::new(Cell::new(false));
        let search_text = search.delegate().and_downcast::<gtk::Text>().or_else(|| {
            std::iter::successors(search.first_child(), |w| w.next_sibling())
                .find_map(|w| w.downcast::<gtk::Text>().ok())
        });
        if let Some(text) = &search_text {
            let preedit_flag = has_preedit.clone();
            text.connect_local("preedit-changed", false, move |values| {
                let preedit = values
                    .get(1)
                    .and_then(|v| v.get::<Option<String>>().ok().flatten())
                    .unwrap_or_default();
                preedit_flag.set(!preedit.is_empty());
                None
            });
            let preedit_now = has_preedit.clone();
            let preedit_latch = had_preedit_on_escape.clone();
            let capture_keys = gtk::EventControllerKey::builder()
                .propagation_phase(gtk::PropagationPhase::Capture)
                .build();
            capture_keys.connect_key_pressed(move |_, key, _, _| {
                if key == gdk::Key::Escape {
                    preedit_latch.set(preedit_now.get());
                } else {
                    preedit_latch.set(false);
                }
                glib::Propagation::Proceed
            });
            text.add_controller(capture_keys);
            let preedit_now = has_preedit.clone();
            let preedit_latch = had_preedit_on_escape.clone();
            let text_weak = text.downgrade();
            let bubble_keys = gtk::EventControllerKey::builder()
                .propagation_phase(gtk::PropagationPhase::Bubble)
                .build();
            bubble_keys.connect_key_pressed(move |_, key, _, _| {
                if key == gdk::Key::Escape && (preedit_latch.replace(false) || preedit_now.get()) {
                    if preedit_now.replace(false)
                        && let Some(t) = text_weak.upgrade()
                    {
                        let module = t.im_module();
                        t.set_im_module(Some("gtk-im-context-simple"));
                        t.set_im_module(module.as_deref());
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            text.add_controller(bubble_keys);
        }
        let preedit_now = has_preedit.clone();
        let preedit_latch = had_preedit_on_escape.clone();
        search.connect_stop_search(move |_| {
            if !preedit_now.get() && !preedit_latch.replace(false) {
                panels.close();
            }
        });
        let panels = self.0.panels.clone();
        let search = search;
        let rows = list.clone();
        let scroller = scroll.clone();
        trigger.connect_clicked(move |b| {
            has_preedit.set(false);
            had_preedit_on_escape.set(false);
            search.set_text("");
            let weak = b.downgrade();
            panels.show(b, &body, move || {
                if let Some(b) = weak.upgrade() {
                    b.set_active(false);
                }
            });
            if panels.is_open() {
                search.grab_focus();
                if let Some(selected) = rows.selected_row() {
                    let list = rows.clone();
                    let scroll = scroller.clone();
                    glib::idle_add_local_once(move || {
                        if let Some(bounds) = selected.compute_bounds(&list) {
                            let adj = scroll.vadjustment();
                            let center = bounds.y() as f64 + bounds.height() as f64 / 2.
                                - adj.page_size() / 2.;
                            adj.set_value(
                                center.clamp(0., (adj.upper() - adj.page_size()).max(0.)),
                            );
                        }
                    });
                }
            }
        });
        self.spin_popup(
            SpinKind::FontSize,
            "Font size",
            [1., 72., 1.],
            |m, _scale| {
                if m.kind == Kind::Text {
                    m.text_level()
                } else {
                    1.
                }
            },
            |v| StylePatch {
                font_size_value: Some(v),
                ..Default::default()
            },
        );
        self.separator();
        self.toggle(
            "Bold",
            "text-bold",
            |m| m.style.bold,
            |s, v| {
                s.patch(StylePatch {
                    bold: Some(v),
                    ..Default::default()
                })
            },
        );
        self.toggle(
            "Italic",
            "text-italic",
            |m| m.style.italic,
            |s, v| {
                s.patch(StylePatch {
                    italic: Some(v),
                    ..Default::default()
                })
            },
        );
        self.separator();
        self.toggle(
            "Text outline",
            "text-outline",
            |m| m.style.outline_mode != "none",
            |s, v| {
                let mode = if v {
                    s.0.scene
                        .upgrade()
                        .and_then(|scene| {
                            scene
                                .controller()
                                .borrow()
                                .appearance()
                                .map(|m| m.template.style.outline_last_mode.clone())
                        })
                        .unwrap_or("auto".into())
                } else {
                    "none".into()
                };
                s.patch(StylePatch {
                    outline_mode: Some(mode),
                    ..Default::default()
                })
            },
        );
        let outline_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        let outline_width = gtk::SpinButton::with_range(0., 8., 0.5);
        outline_width.set_tooltip_text(Some(&t("Outline width")));
        outline_box.append(&outline_width);
        let weak = Rc::downgrade(&self.0);
        outline_width.connect_value_changed(move |s| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).patch(StylePatch {
                    outline_width: Some(s.value()),
                    ..Default::default()
                });
            }
        });
        self.update(move |m, _| outline_width.set_value(m.style.outline_width));
        let arrow = gtk::Image::builder()
            .icon_name("pan-down-symbolic")
            .pixel_size(12)
            .build();
        let weak = Rc::downgrade(&self.0);
        let panels = self.0.panels.clone();
        let scene = self.0.scene.clone();
        let radio_group: RefCell<Option<gtk::CheckButton>> = RefCell::new(None);
        for (mode, label) in [
            ("auto", "Automatic"),
            ("white", "White"),
            ("black", "Black"),
            ("custom", "Custom color"),
        ] {
            let b = gtk::CheckButton::with_label(&t(label));
            let mut group = radio_group.borrow_mut();
            if let Some(first) = group.as_ref() {
                b.set_group(Some(first));
            } else {
                *group = Some(b.clone());
            }
            outline_box.append(&b);
            let weak = Rc::downgrade(&self.0);
            b.connect_toggled(move |b| {
                if b.is_active()
                    && let Some(inner) = weak.upgrade()
                {
                    Self(inner).patch(StylePatch {
                        outline_mode: Some(mode.into()),
                        ..Default::default()
                    });
                }
            });
            self.update(move |m, _| b.set_active(m.style.outline_mode == mode));
        }
        let chooser = super::color::ColorPanel::new(
            clivuu_core::annotation::color::ColorDraft::parse("#000000", 255)
                .expect("built-in fallback color"),
            self.0.color_services.clone(),
            false,
            move |patch| {
                if let Some(inner) = weak.upgrade() {
                    Self(inner).patch(StylePatch {
                        outline_mode: Some("custom".into()),
                        outline_color: patch.color,
                        ..Default::default()
                    });
                }
            },
            move || panels.close(),
        );
        outline_box.append(chooser.widget());
        let visibility = chooser.clone();
        let panels = self.0.panels.clone();
        self.update(move |m, _| {
            let show = m.style.outline_mode == "custom";
            if visibility.widget().is_visible() != show {
                visibility.widget().set_visible(show);
                if panels.is_open() {
                    let panels = panels.clone();
                    glib::idle_add_local_once(move || panels.reposition());
                }
            }
        });
        let open_chooser = chooser.clone();
        let close_chooser = chooser;
        self.popup_with(
            "Outline options",
            &arrow,
            false,
            &outline_box,
            move || {
                if let Some(scene) = scene.upgrade()
                    && let Some(a) = scene.controller().borrow().appearance()
                    && let Some(draft) = clivuu_core::annotation::color::ColorDraft::parse(
                        &a.template.style.outline_color,
                        255,
                    )
                {
                    open_chooser.reset(draft);
                }
            },
            move || close_chooser.dismiss(),
        );
    }

    fn colors(&self) {
        self.separator();
        let sample = gtk::DrawingArea::builder()
            .content_width(22)
            .content_height(22)
            .build();
        let color = Rc::new(RefCell::new(gdk::RGBA::RED));
        let alpha = Rc::new(Cell::new(1.));
        let value = color.clone();
        let alpha_value = alpha.clone();
        sample.set_draw_func(move |a, c, w, h| {
            let color = value.borrow();
            let a_val = alpha_value.get();
            if a_val < 0.999 {
                let wf = w as f64 - 5.;
                let hf = h as f64 - 5.;
                let step = 4.;
                let cols = (wf / step).ceil() as i32;
                let rows = (hf / step).ceil() as i32;
                for r in 0..rows {
                    for col in 0..cols {
                        let v = if (r + col) % 2 == 0 { 0.88 } else { 0.64 };
                        c.set_source_rgb(v, v, v);
                        c.rectangle(
                            (2.5 + col as f64 * step).min(2.5 + wf),
                            (2.5 + r as f64 * step).min(2.5 + hf),
                            step.min(2.5 + wf - (2.5 + col as f64 * step)).max(0.),
                            step.min(2.5 + hf - (2.5 + r as f64 * step)).max(0.),
                        );
                        let _ = c.fill();
                    }
                }
            }
            c.set_source_rgba(
                color.red() as f64,
                color.green() as f64,
                color.blue() as f64,
                a_val,
            );
            c.rectangle(2.5, 2.5, w as f64 - 5., h as f64 - 5.);
            let _ = c.fill_preserve();
            let ink = base_ink(a.upcast_ref());
            c.set_source_rgb(ink.red() as f64, ink.green() as f64, ink.blue() as f64);
            c.set_line_width(1.);
            let _ = c.stroke();
            for i in 0..4 {
                let [r, g, b] = [[1., 0., 0.], [1., 1., 0.], [0., 1., 0.], [0., 0.4, 1.]][i];
                let i = i as f64;
                c.set_source_rgb(r, g, b);
                c.move_to(w as f64 - 3., h as f64 - 3.);
                c.line_to(w as f64 - 3. - i * 2., h as f64 - 11. + i * 2.);
                c.line_to(w as f64 - 5. - i * 2., h as f64 - 9. + i * 2.);
                c.close_path();
                let _ = c.fill();
            }
        });
        let preview = sample.clone();
        self.update(move |m, _| {
            if let Ok(c) = gdk::RGBA::parse(&m.style.color) {
                *color.borrow_mut() = c;
            }
            alpha.set(f64::from(m.style.alpha) / 255.);
            preview.queue_draw();
        });
        let initial = self
            .0
            .scene
            .upgrade()
            .and_then(|s| {
                s.controller()
                    .borrow()
                    .appearance()
                    .map(|a| a.template.style.clone())
            })
            .and_then(|s| clivuu_core::annotation::color::ColorDraft::parse(&s.color, s.alpha))
            .unwrap_or_else(|| {
                clivuu_core::annotation::color::ColorDraft::parse("#ff0000", 255)
                    .expect("built-in fallback color")
            });
        let weak = Rc::downgrade(&self.0);
        let panels = self.0.panels.clone();
        let chooser = super::color::ColorPanel::new(
            initial,
            self.0.color_services.clone(),
            true,
            move |patch| {
                if let Some(inner) = weak.upgrade() {
                    Self(inner).patch(patch);
                }
            },
            move || panels.close(),
        );
        let scene = self.0.scene.clone();
        let open = chooser.clone();
        let close = chooser.clone();
        let button = self.popup_with(
            "Choose color",
            &sample,
            false,
            chooser.widget(),
            move || {
                if let Some(scene) = scene.upgrade()
                    && let Some(a) = scene.controller().borrow().appearance()
                    && let Some(draft) = clivuu_core::annotation::color::ColorDraft::parse(
                        &a.template.style.color,
                        a.template.style.alpha,
                    )
                {
                    open.reset(draft);
                }
            },
            move || close.dismiss(),
        );
        button.add_css_class("clivuu-current-color");
        button.set_size_request(24, 24);
        button.set_valign(gtk::Align::Center);
        let grid = gtk::Grid::builder()
            .row_spacing(2)
            .column_spacing(2)
            .valign(gtk::Align::Center)
            .build();
        self.0.row.append(&grid);
        for (i, &color) in PALETTE.iter().enumerate() {
            let b = gtk::Button::builder()
                .focusable(false)
                .tooltip_text(color)
                .build();
            b.add_css_class("clivuu-swatch");
            let a = gtk::DrawingArea::builder()
                .content_width(8)
                .content_height(8)
                .build();
            let c = gdk::RGBA::parse(color).unwrap();
            a.set_draw_func(move |_, ctx, w, h| {
                ctx.set_source_rgb(c.red() as f64, c.green() as f64, c.blue() as f64);
                ctx.rectangle(0., 0., w as f64, h as f64);
                let _ = ctx.fill();
            });
            b.set_child(Some(&a));
            grid.attach(&b, (i % 10) as i32, (i / 10) as i32, 1, 1);
            let weak = Rc::downgrade(&self.0);
            b.connect_clicked(move |_| {
                if let Some(inner) = weak.upgrade() {
                    Self(inner).patch(StylePatch {
                        color: Some(color.into()),
                        ..Default::default()
                    });
                }
            });
            self.update(move |m, _| {
                if m.style.color == color {
                    b.add_css_class("clivuu-swatch-selected");
                } else {
                    b.remove_css_class("clivuu-swatch-selected");
                }
            });
        }
    }
    fn build(&self, kind: Kind) {
        if kind == Kind::Sequence {
            self.sequence_controls();
            self.colors();
            return;
        }
        if !matches!(kind, Kind::Text) {
            let Some(scene) = self.0.scene.upgrade() else {
                return;
            };
            let scale = scene.controller().borrow().session().stroke_scale();
            let [min, max, step] = kind.stroke_range(scale);
            let (spin_kind, label) = if kind == Kind::Eraser {
                (SpinKind::EraserWidth, "Eraser width (image pixels)")
            } else {
                (SpinKind::StrokeWidth, "Stroke width (image pixels)")
            };
            self.spin_popup(
                spin_kind,
                label,
                [min, max, step],
                |m, _| m.style.width,
                |v| StylePatch {
                    width: Some(v),
                    ..Default::default()
                },
            );
        }
        self.fills(kind);
        if matches!(kind, Kind::Rect | Kind::Ellipse) {
            self.separator();
            for (k, icon, label) in [
                (Kind::Rect, "rect", "Rectangle"),
                (Kind::Ellipse, "ellipse", "Ellipse"),
            ] {
                self.toggle(
                    label,
                    icon,
                    move |m| m.kind == k,
                    move |s, _| {
                        if let Some(scene) = s.0.scene.upgrade() {
                            scene.dispatch(Action::Shape(k));
                        }
                    },
                );
            }
        }
        if kind == Kind::Arrow {
            self.separator();
            self.arrow_menu(true);
            self.arrow_menu(false);
        }
        if kind == Kind::Mosaic {
            self.strength();
        }
        if kind == Kind::Text {
            self.font();
        }
        if !matches!(kind, Kind::Mosaic | Kind::Eraser) {
            self.colors();
        }
    }
    pub fn refresh(&self) {
        let Some(scene) = self.0.scene.upgrade() else {
            return;
        };
        let controller = scene.controller();
        let c = controller.borrow();
        let mark = c.appearance().map(|a| a.snapshot());
        let scale = c.session().stroke_scale();
        let screen_scale = c.mapping().screen_scale();
        let kind = mark.as_ref().map(|m| m.kind);
        let sequence_style =
            (kind == Some(Kind::Sequence)).then(|| c.session().style(Kind::Sequence).style.clone());
        drop(c);
        self.0.image_scale.set(screen_scale);
        let language = clivuu_core::i18n::current_language();
        let next_state = RefreshState {
            kind,
            style: mark.as_ref().map(|m| m.style.clone()),
            text_level_bits: mark
                .as_ref()
                .filter(|m| m.kind == Kind::Text)
                .map_or(0, |m| m.text_level().to_bits()),
            sequence_style,
            stroke_scale_bits: scale.to_bits(),
            screen_scale_bits: screen_scale.to_bits(),
            language,
        };
        if self.0.state.borrow().as_ref() == Some(&next_state) {
            self.0.widget.set_visible(kind.is_some());
            return;
        }
        self.0.state.replace(Some(next_state));
        self.0.widget.set_visible(kind.is_some());
        let build_key = (kind, scale.to_bits(), language);
        if self.0.built.replace(Some(build_key)) != Some(build_key) {
            self.0.panels.close();
            self.0.updates.borrow_mut().clear();
            while let Some(child) = self.0.row.first_child() {
                self.0.row.remove(&child);
            }
            if let Some(kind) = kind {
                self.build(kind);
            }
        }
        if let Some(mark) = mark {
            self.0.refreshing.set(true);
            for f in self.0.updates.borrow().iter() {
                f(&mark, scale);
            }
            self.0.refreshing.set(false);
        }
    }
}
// Menu samples retain the toolbar's ink while their trigger is checked,
// matching the GJS renderer's owner.toolbarInk rather than selectedInk.
fn base_ink(widget: &gtk::Widget) -> gdk::RGBA {
    let mut current = Some(widget.clone());
    while let Some(w) = current {
        if w.has_css_class("clivuu-capture-toolbar") {
            return w.color();
        }
        current = w.parent();
    }
    widget.color()
}
fn arrow_icon(
    a: &gtk::DrawingArea,
    c: &gtk::cairo::Context,
    w: i32,
    h: i32,
    arrow: bool,
    choice: (ArrowType, LineStyle),
) {
    let ink = base_ink(a.upcast_ref());
    c.set_source_rgba(
        ink.red() as f64,
        ink.green() as f64,
        ink.blue() as f64,
        ink.alpha() as f64,
    );
    let left = 5.;
    let right = w as f64 - 5.;
    let y = h as f64 / 2.;
    let width = 1.7;
    if !arrow {
        c.set_line_width(width);
        c.set_line_cap(gtk::cairo::LineCap::Round);
        c.set_dash(
            &choice
                .1
                .pattern()
                .iter()
                .map(|v| v * width)
                .collect::<Vec<_>>(),
            0.,
        );
        c.move_to(left, y);
        c.line_to(right, y);
        let _ = c.stroke();
        return;
    }
    for contour in arrow_paths([left, y], [right, y], width, choice.0, LineStyle::Solid) {
        c.new_path();
        for (index, p) in contour.points.iter().enumerate() {
            if index == 0 {
                c.move_to(p[0], p[1]);
            } else {
                c.line_to(p[0], p[1]);
            }
        }
        if contour.closed {
            c.close_path();
        }
        if let Some(spec) = contour.stroke {
            c.set_line_width(spec.width);
            c.set_dash(&spec.dashes, 0.);
            c.set_line_cap(if spec.round {
                gtk::cairo::LineCap::Round
            } else {
                gtk::cairo::LineCap::Butt
            });
            c.set_line_join(gtk::cairo::LineJoin::Round);
            let _ = c.stroke();
        } else {
            let _ = c.fill();
        }
    }
}
fn effect_icon(a: &gtk::DrawingArea, c: &gtk::cairo::Context, w: i32, h: i32, effect: &str) {
    let ink = base_ink(a.upcast_ref());
    let size = (w.min(h) as f64) * 0.72;
    let (x, y) = ((w as f64 - size) / 2., (h as f64 - size) / 2.);
    if effect == "blur" {
        let mut r = size / 2.;
        while r >= 1. {
            c.set_source_rgba(
                ink.red() as f64,
                ink.green() as f64,
                ink.blue() as f64,
                0.075,
            );
            c.arc(w as f64 / 2., h as f64 / 2., r, 0., std::f64::consts::TAU);
            let _ = c.fill();
            r -= 1.;
        }
    } else {
        for row in 0..3 {
            for col in 0..3 {
                c.set_source_rgba(
                    ink.red() as f64,
                    ink.green() as f64,
                    ink.blue() as f64,
                    [0.85, 0.3, 0.65, 0.35, 0.6, 0.2, 0.7, 0.15, 0.9][row * 3 + col],
                );
                c.rectangle(
                    x + col as f64 * size / 3.,
                    y + row as f64 * size / 3.,
                    size / 3.,
                    size / 3.,
                );
                let _ = c.fill();
            }
        }
    }
}
fn size_label(v: f64) -> String {
    let digits = if v >= 10. { 4 } else { 3 };
    let decimals = (digits - 1 - (v.max(0.01).log10().floor() as i32)).max(0) as usize;
    let text = format!("{v:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').into()
    } else {
        text
    }
}
fn stroke_indicator(
    a: &gtk::DrawingArea,
    c: &gtk::cairo::Context,
    w: i32,
    h: i32,
    width: f64,
    image_scale: f64,
) {
    let ink = base_ink(a.upcast_ref());
    stroke_indicator_ink(c, w, h, width, image_scale, &ink);
}
fn stroke_indicator_ink(
    c: &gtk::cairo::Context,
    w: i32,
    h: i32,
    width: f64,
    image_scale: f64,
    ink: &gdk::RGBA,
) {
    c.set_source_rgb(ink.red() as f64, ink.green() as f64, ink.blue() as f64);
    let diameter = crate::editor::cursor::stroke_diameter(width, image_scale);
    if diameter <= (w.min(h) - 4) as f64 {
        c.arc(
            w as f64 / 2.,
            h as f64 / 2.,
            diameter / 2.,
            0.,
            std::f64::consts::TAU,
        );
        let _ = c.fill();
    } else {
        let l = pangocairo::functions::create_layout(c);
        let mut f = pango::FontDescription::new();
        f.set_family("Sans");
        f.set_absolute_size(10. * pango::SCALE as f64);
        l.set_font_description(Some(&f));
        let n = (width * 100.).round() / 100.;
        l.set_text(&n.to_string());
        let (tw, th) = l.pixel_size();
        c.move_to((w - tw) as f64 / 2., (h - th) as f64 / 2.);
        pangocairo::functions::show_layout(c, &l);
    }
}
