//! Shared settings and toolbar color controls. Only acceptance commits a draft;
//! screen picking and custom-color persistence are application-provided services.
use clivuu_core::{
    annotation::{
        StylePatch,
        color::{ColorDraft, hsv_to_rgb},
    },
    i18n::t,
};
use gtk::{gdk, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
};
type PickFuture = Pin<Box<dyn Future<Output = Result<[u8; 3], String>>>>;
#[derive(Clone, Default)]
pub struct ColorServices {
    pub load: Option<Rc<dyn Fn() -> Vec<String>>>,
    pub save: Option<Rc<dyn Fn(Vec<String>)>>,
    pub pick: Option<Rc<dyn Fn(gio::Cancellable) -> PickFuture>>,
}
struct Inner {
    widget: gtk::Box,
    draft: RefCell<ColorDraft>,
    syncing: Cell<bool>,
    editing_html: Cell<bool>,
    fields: Vec<gtk::SpinButton>,
    spin_capsules: Vec<gtk::Box>,
    html: gtk::Entry,
    html_capsule: gtk::Box,
    canvases: RefCell<Vec<gtk::DrawingArea>>,
    custom: RefCell<Vec<(gtk::Button, Rc<RefCell<String>>)>>,
    slot: Cell<usize>,
    status: gtk::Label,
    status_key: Cell<Option<&'static str>>,
    basic_label: gtk::Label,
    custom_label: gtk::Label,
    pick_button: gtk::Button,
    add_button: gtk::Button,
    copy_button: gtk::Button,
    dialog_actions: Option<(gtk::Button, gtk::Button)>,
    services: ColorServices,
    colors: RefCell<Vec<String>>,
    picker: RefCell<Option<gio::Cancellable>>,
    generation: Cell<u64>,
    alpha_enabled: bool,
    accept: Box<dyn Fn(StylePatch)>,
    close: Box<dyn Fn()>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(c) = self.picker.take() {
            c.cancel();
        }
    }
}
#[derive(Clone)]
pub struct ColorPanel(Rc<Inner>);
fn row() -> gtk::Box {
    gtk::Box::builder().spacing(4).build()
}
fn column() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .build()
}
fn hide_spin_buttons(spin: &gtk::SpinButton) {
    let mut child = spin.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.is::<gtk::Button>() {
            widget.set_visible(false);
        }
    }
}
fn make_capsule(tag: &str, tooltip: Option<&str>, input: &impl IsA<gtk::Widget>) -> gtk::Box {
    let capsule = gtk::Box::builder()
        .spacing(4)
        .valign(gtk::Align::Center)
        .build();
    capsule.add_css_class("clivuu-color-field");
    let label = gtk::Label::new(Some(tag));
    label.add_css_class("clivuu-color-tag");
    label.set_xalign(0.);
    if let Some(tip) = tooltip {
        label.set_tooltip_text(Some(tip));
    }
    let click = gtk::GestureClick::builder().button(1).build();
    let target = input.as_ref().downgrade();
    click.connect_released(move |_, _, _, _| {
        if let Some(w) = target.upgrade() {
            w.grab_focus();
        }
    });
    label.add_controller(click);
    capsule.append(&label);
    capsule.append(input);
    capsule
}
fn frame(child: &impl IsA<gtk::Widget>, hexpand: bool) -> gtk::Box {
    let box_ = gtk::Box::builder().hexpand(hexpand).build();
    box_.add_css_class("clivuu-color-canvas");
    box_.set_overflow(gtk::Overflow::Hidden);
    box_.append(child);
    box_
}
fn draw_checker(c: &gtk::cairo::Context, w: f64, h: f64) {
    for y in (0..h as i32).step_by(6) {
        for x in (0..w as i32).step_by(6) {
            let v = if (x / 6 + y / 6) % 2 == 0 { 0.9 } else { 0.65 };
            c.set_source_rgb(v, v, v);
            c.rectangle(x as f64, y as f64, 6., 6.);
            let _ = c.fill();
        }
    }
}
fn draw_marker(c: &gtk::cairo::Context, x0: f64, y0: f64, x1: f64, y1: f64) {
    for (shade, alpha, width) in [(0., 0.72, 4.), (1., 0.96, 2.)] {
        c.set_source_rgba(shade, shade, shade, alpha);
        c.set_line_width(width);
        c.move_to(x0, y0);
        c.line_to(x1, y1);
        let _ = c.stroke();
    }
}
pub(super) fn style(dark: bool) -> String {
    let colors = if dark {
        [
            "#282a36", "#343746", "#f8f8f2", "#44475a", "#bd93f9", "#21222c", "#a4a7b5", "#ff5555",
        ]
    } else {
        [
            "#f8f7fc", "#ffffff", "#282a36", "#dedbe9", "#7952b3", "#ffffff", "#6b687d", "#d9383a",
        ]
    };
    let mut css = include_str!("../../resources/color-panel.css").to_owned();
    for (token, value) in [
        "background",
        "surface",
        "ink",
        "border",
        "accent",
        "field",
        "muted",
        "danger",
    ]
    .into_iter()
    .zip(colors)
    {
        css = css.replace(&format!("@color_{token}"), value);
    }
    css
}
fn swatch(color: String, action: impl Fn() + 'static) -> (gtk::Button, Rc<RefCell<String>>) {
    let b = gtk::Button::builder()
        .focusable(false)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Start)
        .build();
    b.add_css_class("clivuu-swatch");
    let a = gtk::DrawingArea::builder()
        .content_width(20)
        .content_height(20)
        .build();
    let color = Rc::new(RefCell::new(color));
    let copy = color.clone();
    a.set_draw_func(move |_, ctx, w, h| {
        let c = gdk::RGBA::parse(&*copy.borrow()).unwrap_or(gdk::RGBA::WHITE);
        ctx.set_source_rgba(
            c.red() as f64,
            c.green() as f64,
            c.blue() as f64,
            c.alpha() as f64,
        );
        ctx.rectangle(0., 0., w as f64, h as f64);
        let _ = ctx.fill();
    });
    b.set_child(Some(&a));
    b.connect_clicked(move |_| action());
    (b, color)
}
impl ColorPanel {
    pub fn new(
        initial: ColorDraft,
        services: ColorServices,
        alpha_enabled: bool,
        accept: impl Fn(StylePatch) + 'static,
        close: impl Fn() + 'static,
    ) -> Self {
        Self::build(initial, services, alpha_enabled, false, accept, close)
    }
    /// The shared content with actions provided by the dialog header.
    pub fn for_dialog(
        initial: ColorDraft,
        services: ColorServices,
        alpha_enabled: bool,
        accept: impl Fn(StylePatch) + 'static,
        close: impl Fn() + 'static,
    ) -> Self {
        Self::build(initial, services, alpha_enabled, true, accept, close)
    }
    fn build(
        initial: ColorDraft,
        services: ColorServices,
        alpha_enabled: bool,
        dialog: bool,
        accept: impl Fn(StylePatch) + 'static,
        close: impl Fn() + 'static,
    ) -> Self {
        let widget = column();
        widget.set_widget_name("clivuu-color-editor");
        widget.add_css_class("clivuu-color-editor");
        widget.set_spacing(if dialog { 12 } else { 10 });
        widget.set_width_request(if dialog { 520 } else { 460 });
        let top = row();
        let left = column();
        let right = column();
        top.set_spacing(if dialog { 20 } else { 16 });
        left.set_spacing(8);
        right.set_spacing(8);
        right.set_hexpand(true);
        top.append(&left);
        top.append(&right);
        widget.append(&top);
        let fields: Vec<_> = (0..7)
            .map(|i| {
                let s = gtk::SpinButton::with_range(0., if i == 0 { 359. } else { 255. }, 1.);
                s.set_widget_name(
                    [
                        "clivuu-color-hue",
                        "clivuu-color-saturation",
                        "clivuu-color-value",
                        "clivuu-color-red",
                        "clivuu-color-green",
                        "clivuu-color-blue",
                        "clivuu-color-alpha",
                    ][i],
                );
                s.set_numeric(true);
                s.set_alignment(1.);
                s.set_width_chars(3);
                s.set_max_width_chars(3);
                s.set_hexpand(true);
                hide_spin_buttons(&s);
                s
            })
            .collect();
        let spin_capsules: Vec<_> = fields
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let tag = ["H", "S", "V", "R", "G", "B", "A"][i];
                let tip = t([
                    "Hue",
                    "Saturation",
                    "Value",
                    "Red",
                    "Green",
                    "Blue",
                    "Alpha",
                ][i]);
                make_capsule(tag, Some(&tip), s)
            })
            .collect();
        let html = gtk::Entry::builder()
            .width_chars(8)
            .max_width_chars(9)
            .max_length(9)
            .hexpand(true)
            .build();
        html.set_widget_name("clivuu-color-html");
        let html_capsule = make_capsule("HTML", None, &html);
        let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
        copy.set_widget_name("clivuu-color-copy");
        copy.add_css_class("clivuu-color-copy");
        copy.set_tooltip_text(Some(&t("Copy color value")));
        copy.set_valign(gtk::Align::Center);
        copy.set_focus_on_click(false);
        html_capsule.append(&copy);
        let status = gtk::Label::builder().xalign(0.).visible(false).build();
        status.add_css_class("clivuu-color-status");
        let basic_label = gtk::Label::builder()
            .label(t("Basic colors"))
            .xalign(0.)
            .build();
        let custom_label = gtk::Label::builder()
            .label(t("Custom colors"))
            .xalign(0.)
            .build();
        let pick_button = gtk::Button::with_label(&t("Pick screen color"));
        pick_button.add_css_class("clivuu-color-tool");
        let add_button = gtk::Button::with_label(&t("Add to custom colors"));
        add_button.add_css_class("clivuu-color-tool");
        let cancel_btn = (!dialog).then(|| gtk::Button::with_label(&t("Cancel")));
        let ok_btn = (!dialog).then(|| gtk::Button::with_label(&t("OK")));
        let dialog_actions = cancel_btn
            .as_ref()
            .zip(ok_btn.as_ref())
            .map(|(c, o)| (c.clone(), o.clone()));
        let this = Self(Rc::new(Inner {
            widget,
            draft: RefCell::new(initial),
            syncing: Cell::new(false),
            editing_html: Cell::new(false),
            fields,
            spin_capsules,
            html,
            html_capsule,
            canvases: RefCell::new(vec![]),
            custom: RefCell::new(vec![]),
            slot: Cell::new(0),
            status,
            status_key: Cell::new(None),
            basic_label: basic_label.clone(),
            custom_label: custom_label.clone(),
            pick_button: pick_button.clone(),
            add_button: add_button.clone(),
            copy_button: copy.clone(),
            dialog_actions,
            colors: RefCell::new(services.load.as_ref().map_or_else(Vec::new, |f| f())),
            services,
            picker: RefCell::new(None),
            generation: Cell::new(0),
            alpha_enabled,
            accept: Box::new(accept),
            close: Box::new(close),
        }));
        let weak = Rc::downgrade(&this.0);
        let copy_weak = copy.downgrade();
        let copy_gen = Rc::new(Cell::new(0u64));
        copy.connect_clicked(move |btn| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let this = Self(inner);
            let d = this.0.draft.borrow();
            let text = if this.0.alpha_enabled && d.alpha < 255 {
                format!("{}{:02x}", d.color(), d.alpha)
            } else {
                d.color()
            };
            drop(d);
            btn.display().clipboard().set_text(&text);
            btn.set_icon_name("object-select-symbolic");
            btn.add_css_class("clivuu-color-copied");
            let token = copy_gen.get().wrapping_add(1);
            copy_gen.set(token);
            let copy_token = copy_gen.clone();
            let btn_weak = copy_weak.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(1200), move || {
                if copy_token.get() == token
                    && let Some(b) = btn_weak.upgrade()
                {
                    b.set_icon_name("edit-copy-symbolic");
                    b.remove_css_class("clivuu-color-copied");
                }
            });
        });
        left.append(&this.0.basic_label);
        let mut basics = vec![
            "#000000".to_string(),
            "#800000".into(),
            "#008000".into(),
            "#808000".into(),
            "#000080".into(),
            "#800080".into(),
            "#008080".into(),
            "#c0c0c0".into(),
            "#808080".into(),
            "#ff0000".into(),
            "#00ff00".into(),
            "#ffff00".into(),
            "#0000ff".into(),
            "#ff00ff".into(),
            "#00ffff".into(),
            "#ffffff".into(),
        ];
        for i in 0..32 {
            let [r, g, b] =
                hsv_to_rgb([(i % 8) as f64 * 45., [255., 170., 100., 55.][i / 8], 255.]);
            basics.push(format!("#{r:02x}{g:02x}{b:02x}"));
        }
        let basic_grid = column();
        for colors in basics.chunks(8) {
            let r = row();
            for color in colors {
                let weak = Rc::downgrade(&this.0);
                let value = color.clone();
                let (b, _) = swatch(color.clone(), move || {
                    if let Some(inner) = weak.upgrade() {
                        let this = Self(inner);
                        let alpha = this.0.draft.borrow().alpha;
                        if let Some(parsed) = ColorDraft::parse(&value, alpha) {
                            *this.0.draft.borrow_mut() = parsed;
                        }
                        this.sync();
                    }
                });
                r.append(&b);
            }
            basic_grid.append(&r);
        }
        left.append(&basic_grid);
        let pick = this.0.pick_button.clone();
        pick.set_sensitive(this.0.services.pick.is_some());
        left.append(&pick);
        let weak = Rc::downgrade(&this.0);
        pick.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).pick();
            }
        });
        left.append(&this.0.custom_label);
        let custom_grid = column();
        for y in 0..2 {
            let r = row();
            for x in 0..8 {
                let i = y * 8 + x;
                let weak = Rc::downgrade(&this.0);
                let (b, color) = swatch("#ffffff".into(), move || {
                    if let Some(inner) = weak.upgrade() {
                        let this = Self(inner);
                        this.0.slot.set(i);
                        if let Some(saved) = this.0.colors.borrow().get(i)
                            && let Some(c) = ColorDraft::parse(saved, 255)
                        {
                            *this.0.draft.borrow_mut() = c;
                        }
                        this.sync();
                    }
                });
                let slot_pick = gtk::GestureClick::builder().button(3).build();
                let weak = Rc::downgrade(&this.0);
                slot_pick.connect_pressed(move |_, _, _, _| {
                    if let Some(inner) = weak.upgrade() {
                        let this = Self(inner);
                        this.0.slot.set(i);
                        this.sync();
                    }
                });
                b.add_controller(slot_pick);
                b.set_tooltip_text(Some(&t("Right-click to choose slot")));
                r.append(&b);
                this.0.custom.borrow_mut().push((b, color));
            }
            custom_grid.append(&r);
        }
        left.append(&custom_grid);
        let add = this.0.add_button.clone();
        left.append(&add);
        let weak = Rc::downgrade(&this.0);
        add.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                let this = Self(inner);
                if !this.flush() {
                    return;
                }
                let i = this.0.slot.get();
                {
                    let mut colors = this.0.colors.borrow_mut();
                    let length = colors.len().max(i + 1);
                    colors.resize(length, "#ffffff".into());
                    let d = this.0.draft.borrow();
                    colors[i] = format!("{}{:02x}", d.color(), d.alpha);
                    if let Some(save) = &this.0.services.save {
                        save(colors.clone());
                    }
                }
                this.0.slot.set((i + 1) % 16);
                this.sync();
            }
        });
        let graphics = row();
        graphics.set_spacing(6);
        graphics.set_vexpand(true);
        right.append(&graphics);
        let plane = this.canvas(234, 150, 0);
        plane.set_hexpand(true);
        plane.set_vexpand(true);
        let value = this.canvas(16, 150, 1);
        value.set_vexpand(true);
        let plane_frame = frame(&plane, true);
        plane_frame.set_vexpand(true);
        let value_frame = frame(&value, false);
        value_frame.set_vexpand(true);
        graphics.append(&plane_frame);
        graphics.append(&value_frame);
        let bars = row();
        bars.set_spacing(6);
        right.append(&bars);
        let preview = this.canvas(if alpha_enabled { 42 } else { 256 }, 22, 2);
        preview.set_widget_name("clivuu-color-preview");
        preview.set_hexpand(!alpha_enabled);
        bars.append(&frame(&preview, !alpha_enabled));
        if alpha_enabled {
            let alpha_slider = this.canvas(196, 22, 3);
            alpha_slider.set_widget_name("clivuu-color-alpha-slider");
            alpha_slider.set_hexpand(true);
            bars.append(&frame(&alpha_slider, true));
        }
        let grid = gtk::Grid::builder()
            .column_spacing(6)
            .row_spacing(6)
            .column_homogeneous(true)
            .build();
        for i in 0..3 {
            grid.attach(&this.0.spin_capsules[i], i as i32, 0, 1, 1);
            grid.attach(&this.0.spin_capsules[i + 3], i as i32, 1, 1, 1);
        }
        grid.attach(
            &this.0.html_capsule,
            0,
            2,
            if alpha_enabled { 2 } else { 3 },
            1,
        );
        if alpha_enabled {
            grid.attach(&this.0.spin_capsules[6], 2, 2, 1, 1);
        }
        right.append(&grid);
        for (index, s) in this.0.fields.iter().enumerate() {
            let weak = Rc::downgrade(&this.0);
            s.connect_value_changed(move |s| {
                if let Some(inner) = weak.upgrade() {
                    let this = Self(inner);
                    if this.0.syncing.get() {
                        return;
                    }
                    {
                        let mut d = this.0.draft.borrow_mut();
                        if index < 3 {
                            d.set_hsv(index, s.value());
                        } else if index < 6 {
                            d.set_rgb(index - 3, s.value());
                        } else {
                            d.alpha = s.value() as u8;
                        }
                    }
                    this.sync();
                }
            });
        }
        let weak = Rc::downgrade(&this.0);
        this.0.html.connect_changed(move |entry| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let this = Self(inner);
            if this.0.syncing.get() {
                return;
            }
            this.clear_invalid();
            let text = entry.text();
            let alpha = this.0.draft.borrow().alpha;
            if let Some(parsed) = ColorDraft::parse(text.trim(), alpha) {
                *this.0.draft.borrow_mut() = parsed;
                this.0.editing_html.set(true);
                this.sync();
                this.0.editing_html.set(false);
            }
        });
        let weak = Rc::downgrade(&this.0);
        this.0.html.connect_activate(move |_| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).commit();
            }
        });
        this.0.widget.append(&this.0.status);
        let Some((cancel, ok)) = this.0.dialog_actions.clone() else {
            this.sync();
            return this;
        };
        let buttons = row();
        buttons.set_halign(gtk::Align::End);
        buttons.set_spacing(8);
        buttons.set_margin_top(4);
        for b in [&cancel, &ok] {
            b.add_css_class("clivuu-color-action");
            b.add_css_class("pill");
            buttons.append(b);
        }
        ok.add_css_class("suggested-action");
        let weak = Rc::downgrade(&this.0);
        cancel.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                let this = Self(inner);
                this.dismiss();
                (this.0.close)();
            }
        });
        let weak = Rc::downgrade(&this.0);
        ok.connect_clicked(move |_| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).commit();
            }
        });
        this.0.widget.append(&buttons);
        this.sync();
        this
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.widget
    }
    pub fn refresh_locale(&self) {
        self.0.basic_label.set_label(&t("Basic colors"));
        self.0.custom_label.set_label(&t("Custom colors"));
        self.0.pick_button.set_label(&t("Pick screen color"));
        self.0.add_button.set_label(&t("Add to custom colors"));
        self.0
            .copy_button
            .set_tooltip_text(Some(&t("Copy color value")));
        if let Some((cancel, ok)) = &self.0.dialog_actions {
            cancel.set_label(&t("Cancel"));
            ok.set_label(&t("OK"));
        }
        let tips = [
            "Hue",
            "Saturation",
            "Value",
            "Red",
            "Green",
            "Blue",
            "Alpha",
        ];
        for (capsule, tip) in self.0.spin_capsules.iter().zip(tips) {
            if let Some(label) = capsule.first_child() {
                label.set_tooltip_text(Some(&t(tip)));
            }
        }
        for (b, _) in self.0.custom.borrow().iter() {
            b.set_tooltip_text(Some(&t("Right-click to choose slot")));
        }
        if let Some(key) = self.0.status_key.get() {
            self.0.status.set_label(&t(key));
        }
    }
    pub fn reset(&self, draft: ColorDraft) {
        self.refresh_locale();
        self.dismiss();
        self.0.draft.replace(draft);
        self.0.editing_html.set(false);
        self.0.syncing.set(true);
        self.0.html.set_text(&self.0.draft.borrow().color());
        self.0.syncing.set(false);
        self.sync();
    }
    pub fn dismiss(&self) {
        self.0.generation.set(self.0.generation.get() + 1);
        if let Some(c) = self.0.picker.borrow_mut().take() {
            c.cancel();
        }
    }
    fn clear_invalid(&self) {
        for capsule in &self.0.spin_capsules {
            capsule.remove_css_class("clivuu-field-invalid");
        }
        self.0.html_capsule.remove_css_class("clivuu-field-invalid");
        self.0.status_key.set(None);
        self.0.status.set_visible(false);
    }
    fn sync(&self) {
        self.0.syncing.set(true);
        self.clear_invalid();
        if !self.0.alpha_enabled {
            self.0.draft.borrow_mut().alpha = 255;
        }
        let d = self.0.draft.borrow();
        for (i, s) in self.0.fields.iter().enumerate() {
            if !s.has_focus() {
                s.set_value(if i < 3 {
                    d.hsv[i].round()
                } else if i < 6 {
                    d.rgb[i - 3] as f64
                } else {
                    d.alpha as f64
                });
            }
        }
        if !self.0.editing_html.get() && !self.0.html.has_focus() {
            self.0.html.set_text(&d.color());
        }
        drop(d);
        for (i, (b, color)) in self.0.custom.borrow().iter().enumerate() {
            *color.borrow_mut() = self
                .0
                .colors
                .borrow()
                .get(i)
                .cloned()
                .unwrap_or("#ffffff".into());
            if self.0.slot.get() == i {
                b.add_css_class("clivuu-swatch-selected");
            } else {
                b.remove_css_class("clivuu-swatch-selected");
            }
            b.queue_draw();
        }
        for a in self.0.canvases.borrow().iter() {
            a.queue_draw();
        }
        self.0.syncing.set(false);
    }
    fn flush(&self) -> bool {
        self.clear_invalid();
        for (i, s) in self.0.fields.iter().enumerate() {
            let text = s.text();
            let valid = !text.is_empty()
                && text.bytes().all(|c| c.is_ascii_digit())
                && text
                    .parse::<u16>()
                    .is_ok_and(|v| v <= if i == 0 { 359 } else { 255 });
            if !valid {
                self.0.spin_capsules[i].add_css_class("clivuu-field-invalid");
                self.invalid();
                return false;
            }
            s.update();
        }
        let text = self.0.html.text();
        let alpha = self.0.draft.borrow().alpha;
        let Some(c) = ColorDraft::parse(text.trim(), alpha) else {
            self.0.html_capsule.add_css_class("clivuu-field-invalid");
            self.invalid();
            return false;
        };
        *self.0.draft.borrow_mut() = c;
        true
    }
    fn invalid(&self) {
        self.0.status_key.set(Some("Enter valid color values"));
        self.0.status.set_label(&t("Enter valid color values"));
        self.0.status.set_visible(true);
    }
    pub fn commit(&self) {
        if !self.flush() {
            return;
        }
        let d = self.0.draft.borrow();
        (self.0.accept)(StylePatch {
            color: Some(d.color()),
            alpha: Some(d.alpha),
            ..Default::default()
        });
        drop(d);
        self.dismiss();
        (self.0.close)();
    }
    fn pick(&self) {
        if self.0.picker.borrow().is_some() {
            return;
        }
        let Some(pick) = &self.0.services.pick else {
            return;
        };
        let c = gio::Cancellable::new();
        self.0.picker.replace(Some(c.clone()));
        let token = self.0.generation.get();
        let future = pick(c);
        let weak = Rc::downgrade(&self.0);
        glib::MainContext::default().spawn_local(async move {
            let result = future.await;
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let this = Self(inner);
            if token != this.0.generation.get() {
                return;
            }
            this.0.picker.borrow_mut().take();
            match result {
                Ok(rgb) => {
                    let [r, g, b] = rgb;
                    let alpha = this.0.draft.borrow().alpha;
                    if let Some(parsed) =
                        ColorDraft::parse(&format!("#{r:02x}{g:02x}{b:02x}"), alpha)
                    {
                        *this.0.draft.borrow_mut() = parsed;
                    }
                    this.sync();
                }
                Err(_) => {
                    this.0.status_key.set(Some("Color picking cancelled"));
                    this.0.status.set_label(&t("Color picking cancelled"));
                    this.0.status.set_visible(true);
                }
            }
        });
    }
    fn canvas(&self, w: i32, h: i32, mode: u8) -> gtk::DrawingArea {
        let a = gtk::DrawingArea::builder()
            .content_width(w)
            .content_height(h)
            .build();
        let weak = Rc::downgrade(&self.0);
        let plane_cache: Rc<RefCell<Option<(i32, i32, gtk::cairo::ImageSurface)>>> =
            Rc::new(RefCell::new(None));
        a.set_draw_func(move |_, c, w, h| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let d = inner.draft.borrow();
            let (wf, hf) = (w as f64, h as f64);
            if mode == 2 {
                draw_checker(c, wf, hf);
                let [r, g, b] = d.rgb;
                c.set_source_rgba(
                    r as f64 / 255.,
                    g as f64 / 255.,
                    b as f64 / 255.,
                    d.alpha as f64 / 255.,
                );
                c.rectangle(0., 0., wf, hf);
                let _ = c.fill();
            } else if mode == 3 {
                draw_checker(c, wf, hf);
                let [r, g, b] = d.rgb.map(|v| v as f64 / 255.);
                let grad = gtk::cairo::LinearGradient::new(0., 0., wf, 0.);
                grad.add_color_stop_rgba(0., r, g, b, 0.);
                grad.add_color_stop_rgba(1., r, g, b, 1.);
                let _ = c.set_source(&grad);
                c.rectangle(0., 0., wf, hf);
                let _ = c.fill();
                let x = (d.alpha as f64 / 255. * wf).clamp(1.5, (wf - 1.5).max(1.5));
                draw_marker(c, x, 0., x, hf);
            } else if mode == 0 {
                let mut cache = plane_cache.borrow_mut();
                if cache
                    .as_ref()
                    .is_none_or(|(cw, ch, _)| *cw != w || *ch != h)
                    && let Ok(mut surface) =
                        gtk::cairo::ImageSurface::create(gtk::cairo::Format::Rgb24, w, h)
                {
                    let stride = surface.stride() as usize;
                    if let Ok(mut data) = surface.data() {
                        let denom_x = (w - 1).max(1) as f64;
                        let denom_y = (h - 1).max(1) as f64;
                        for y in 0..h {
                            let sat = (1. - y as f64 / denom_y) * 255.;
                            let row = y as usize * stride;
                            for x in 0..w {
                                let [r, g, b] = hsv_to_rgb([x as f64 / denom_x * 359., sat, 255.]);
                                let px = 0xff00_0000u32
                                    | ((r as u32) << 16)
                                    | ((g as u32) << 8)
                                    | b as u32;
                                let off = row + x as usize * 4;
                                data[off..off + 4].copy_from_slice(&px.to_ne_bytes());
                            }
                        }
                    }
                    surface.mark_dirty();
                    *cache = Some((w, h, surface));
                }
                if let Some((_, _, surface)) = cache.as_ref() {
                    let _ = c.set_source_surface(surface, 0., 0.);
                    let _ = c.paint();
                }
                let x = d.hsv[0] / 359. * wf;
                let y = (1. - d.hsv[1] / 255.) * hf;
                for (shade, alpha, width) in [(0., 0.72, 2.5), (1., 0.96, 1.5)] {
                    c.set_source_rgba(shade, shade, shade, alpha);
                    c.set_line_width(width);
                    c.arc(x, y, 5.5, 0., std::f64::consts::TAU);
                    let _ = c.stroke();
                }
            } else {
                let [r, g, b] = hsv_to_rgb([d.hsv[0], d.hsv[1], 255.]).map(|v| v as f64 / 255.);
                let grad = gtk::cairo::LinearGradient::new(0., 0., 0., hf);
                grad.add_color_stop_rgb(0., r, g, b);
                grad.add_color_stop_rgb(1., 0., 0., 0.);
                let _ = c.set_source(&grad);
                c.rectangle(0., 0., wf, hf);
                let _ = c.fill();
                let y = ((1. - d.hsv[2] / 255.) * hf).clamp(1.5, (hf - 1.5).max(1.5));
                draw_marker(c, 0., y, wf, y);
            }
        });
        if mode != 2 {
            let drag = gtk::GestureDrag::builder().button(1).build();
            let weak = Rc::downgrade(&self.0);
            let canvas = a.downgrade();
            let update = Rc::new(move |x: f64, y: f64| {
                let (Some(inner), Some(a)) = (weak.upgrade(), canvas.upgrade()) else {
                    return;
                };
                let this = Self(inner);
                let (w, h) = (a.width().max(1) as f64, a.height().max(1) as f64);
                {
                    let mut d = this.0.draft.borrow_mut();
                    if mode == 0 {
                        d.set_hsv(0, (x / w).clamp(0., 1.) * 359.);
                        d.set_hsv(1, (1. - y / h).clamp(0., 1.) * 255.);
                    } else if mode == 1 {
                        d.set_hsv(2, (1. - y / h).clamp(0., 1.) * 255.);
                    } else {
                        d.alpha = ((x / w).clamp(0., 1.) * 255.).round() as u8;
                    }
                }
                this.sync();
            });
            let f = update.clone();
            drag.connect_drag_begin(move |_, x, y| f(x, y));
            drag.connect_drag_update(move |g, dx, dy| {
                if let Some((x, y)) = g.start_point() {
                    update(x + dx, y + dy)
                }
            });
            a.add_controller(drag);
        }
        self.0.canvases.borrow_mut().push(a.clone());
        a
    }
}
