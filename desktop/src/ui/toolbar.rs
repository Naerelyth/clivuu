use super::icons;
use crate::editor::shortcuts::tool_label;
use crate::editor::{Action, Effect, Tool, scene::Scene};
use clivuu_core::i18n::t;
use gtk::{glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

#[derive(Clone, Copy, Debug)]
pub enum Output {
    Confirm,
    Close,
    Pin,
    Apply,
    RotateRight,
    RotateLeft,
    FlipHorizontal,
    FlipVertical,
    Save,
    QuickSave,
    Copy,
    Ocr,
    Scroll,
    Record,
    ClosePin,
    DestroyPin,
    ReplacePin,
    CopyText,
    ConfirmOcrText,
    PinText,
    Preferences,
    OcrPreferences,
    Refresh,
    PreviousCapture,
    NextCapture,
}
pub fn save_shortcut(key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> Option<Output> {
    crate::editor::shortcuts::output(key, modifiers)
        .filter(|action| matches!(action, Output::Save | Output::QuickSave))
}
type ToolbarState = (Tool, bool, bool, bool, clivuu_core::i18n::Language);
struct Inner {
    widget: gtk::Box,
    row: gtk::Box,
    scroller: gtk::ScrolledWindow,
    grip: gtk::Button,
    tools: Vec<(Tool, gtk::Button)>,
    ocr_button: Option<gtk::Button>,
    ocr_active: Cell<bool>,
    on_tool_click: std::cell::RefCell<Option<Box<dyn Fn()>>>,
    undo: gtk::Button,
    redo: gtk::Button,
    scene: glib::WeakRef<Scene>,
    last_state: Cell<Option<ToolbarState>>,
    output: Rc<dyn Fn(Output)>,
    continuous_buttons: Vec<gtk::Button>,
    selection_title: std::cell::RefCell<Option<gtk::Label>>,
    tooltips: super::localization::Tooltips,
}
#[derive(Clone)]
pub struct Toolbar(Rc<Inner>);
struct DisplayTheme {
    display: gtk::gdk::Display,
    base: gtk::CssProvider,
    controls: gtk::CssProvider,
    dark: bool,
    accent_raw: String,
    accent: gtk::gdk::RGBA,
    selected_ink: gtk::gdk::RGBA,
}
impl Drop for DisplayTheme {
    fn drop(&mut self) {
        for provider in [&self.controls, &self.base] {
            gtk::style_context_remove_provider_for_display(&self.display, provider);
        }
    }
}
thread_local! {
    static DISPLAY_THEMES: RefCell<Vec<Weak<RefCell<DisplayTheme>>>> =
        const { RefCell::new(Vec::new()) };
}
#[must_use = "Keep the theme alive for as long as its editor surfaces are displayed"]
pub struct Theme {
    shared: Rc<RefCell<DisplayTheme>>,
}
impl Theme {
    pub fn is_dark(&self) -> bool {
        self.shared.borrow().dark
    }
    pub fn accent(&self) -> gtk::gdk::RGBA {
        self.shared.borrow().accent
    }
    pub fn update(&mut self, dark: bool, accent: &str) {
        let mut inner = self.shared.borrow_mut();
        if inner.dark == dark && inner.accent_raw == accent {
            return;
        }
        let (css, accent_color, selected_ink) = theme_css(dark, accent);
        inner.controls.load_from_string(&css);
        inner.dark = dark;
        inner.accent_raw.clear();
        inner.accent_raw.push_str(accent);
        inner.accent = accent_color;
        inner.selected_ink = selected_ink;
    }
    pub fn decoration_style(&self) -> crate::editor::decorations::DecorationStyle {
        let inner = self.shared.borrow();
        crate::editor::decorations::DecorationStyle {
            accent: inner.accent,
            ink: if inner.dark {
                gtk::gdk::RGBA::WHITE
            } else {
                gtk::gdk::RGBA::BLACK
            },
            selected_ink: inner.selected_ink,
        }
    }
}
impl Toolbar {
    pub const HEIGHT: i32 = super::floating_bar::TOOLBAR_HEIGHT;
    pub fn new(scene: &Scene, pin: bool, output: impl Fn(Output) + 'static) -> Self {
        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build();
        widget.add_css_class("clivuu-capture-toolbar");
        widget.set_size_request(-1, Self::HEIGHT);
        let (scroller, row) = super::floating_bar::scrollable_bar_row();
        widget.append(&scroller);
        let tooltips = super::localization::Tooltips::default();
        let grip = Self::grip_button();
        tooltips.bind(&grip, "Move toolbar", "");
        row.append(&grip);
        type ToolClickCallback = Box<dyn Fn()>;
        let mut tools = vec![];
        let on_tool_click: Rc<std::cell::RefCell<Option<ToolClickCallback>>> =
            Rc::new(std::cell::RefCell::new(None));
        for (tool, icon) in [
            (Tool::Select, "select"),
            (Tool::Rect, "shape"),
            (Tool::Arrow, "arrow"),
            (Tool::Pen, "pen"),
            (Tool::Highlight, "highlight"),
            (Tool::Sequence, "sequence"),
            (Tool::Mosaic, "mosaic"),
            (Tool::Text, "text"),
            (Tool::Eraser, "eraser"),
        ] {
            let (title, keys) = tool_label(tool);
            let button = gtk::Button::builder()
                .child(&icons::icon(icon))
                .focusable(false)
                .build();
            tooltips.bind(&button, title, keys);
            let weak = scene.downgrade();
            let notify_tool = on_tool_click.clone();
            button.connect_clicked(move |_| {
                if let Some(cb) = notify_tool.borrow().as_ref() {
                    cb();
                }
                if let Some(scene) = weak.upgrade() {
                    scene.dispatch(if tool == Tool::Rect {
                        Action::UseShapes
                    } else {
                        Action::Tool(tool)
                    });
                }
            });
            row.append(&button);
            tools.push((tool, button));
        }
        let separator = super::floating_bar::tool_separator;
        row.append(&separator());
        let button = |icon, label, keys| {
            let button = gtk::Button::builder()
                .child(&icons::icon(icon))
                .focusable(false)
                .build();
            tooltips.bind(&button, label, keys);
            button
        };
        let undo = button("undo", "Undo", "Ctrl+Z");
        let weak = scene.downgrade();
        undo.connect_clicked(move |_| {
            if let Some(scene) = weak.upgrade() {
                scene.dispatch(Action::Undo);
            }
        });
        row.append(&undo);
        let redo = button("redo", "Redo", "Ctrl+Y");
        let weak = scene.downgrade();
        redo.connect_clicked(move |_| {
            if let Some(scene) = weak.upgrade() {
                scene.dispatch(Action::Redo);
            }
        });
        row.append(&redo);
        if pin {
            row.append(&separator());
            let crop = button("crop", "Crop", "");
            let weak = scene.downgrade();
            let notify_tool = on_tool_click.clone();
            crop.connect_clicked(move |_| {
                if let Some(cb) = notify_tool.borrow().as_ref() {
                    cb();
                }
                if let Some(scene) = weak.upgrade() {
                    scene.dispatch(Action::Tool(Tool::Crop));
                }
            });
            row.append(&crop);
            tools.push((Tool::Crop, crop));
        }
        let output: Rc<dyn Fn(Output)> = Rc::new(output);
        if pin {
            for (icon, label, keys, action) in [
                ("rotate-right", "Rotate right", "1", Output::RotateRight),
                ("rotate-left", "Rotate left", "2", Output::RotateLeft),
                (
                    "flip-horizontal",
                    "Flip horizontally",
                    "3",
                    Output::FlipHorizontal,
                ),
                (
                    "flip-vertical",
                    "Flip vertically",
                    "4",
                    Output::FlipVertical,
                ),
            ] {
                let b = button(icon, label, keys);
                let output = output.clone();
                b.connect_clicked(move |_| output(action));
                row.append(&b);
            }
        }
        row.append(&separator());
        let mut continuous_buttons = vec![];
        let mut ocr_button = None;
        for (icon, label, keys, action) in [
            (
                "ocr_clipboard",
                "Recognize text",
                if pin { "" } else { "O" },
                Output::Ocr,
            ),
            ("long-capture", "Scrolling capture", "L", Output::Scroll),
            ("record", "Region recording", "V", Output::Record),
        ] {
            if pin && matches!(action, Output::Scroll | Output::Record) {
                continue;
            }
            let b = button(icon, label, keys);
            if matches!(action, Output::Ocr) {
                ocr_button = Some(b.clone());
            }
            if matches!(action, Output::Scroll | Output::Record) {
                continuous_buttons.push(b.clone());
            }
            let output = output.clone();
            b.connect_clicked(move |_| output(action));
            row.append(&b);
        }
        row.append(&separator());
        for (icon, label, keys, action) in [
            (
                "close",
                "Cancel",
                if pin { "" } else { "Esc" },
                Output::Close,
            ),
            (
                if pin { "replace" } else { "pin" },
                if pin { "Apply changes" } else { "Pin" },
                if pin { "Enter / Esc / Space" } else { "Ctrl+T" },
                if pin { Output::Apply } else { Output::Pin },
            ),
            ("quick-save", "Save image", "Ctrl+S", Output::Save),
            (
                "clipboard",
                "Copy image",
                if pin { "Ctrl+C" } else { "Enter / Ctrl+C" },
                Output::Copy,
            ),
        ] {
            let b = button(icon, label, keys);
            let output = output.clone();
            b.connect_clicked(move |_| output(action));
            row.append(&b);
        }
        let toolbar = Self(Rc::new(Inner {
            widget,
            row,
            scroller,
            grip,
            tools,
            ocr_button,
            ocr_active: Cell::new(false),
            on_tool_click: std::cell::RefCell::new(None),
            undo,
            redo,
            scene: scene.downgrade(),
            last_state: Cell::new(None),
            output,
            continuous_buttons,
            selection_title: std::cell::RefCell::new(None),
            tooltips,
        }));
        let inner_weak = Rc::downgrade(&toolbar.0);
        *on_tool_click.borrow_mut() = Some(Box::new(move || {
            if let Some(inner) = inner_weak.upgrade()
                && let Some(cb) = inner.on_tool_click.borrow().as_ref()
            {
                cb();
            }
        }));
        toolbar.refresh();
        let copy = toolbar.clone();
        scene.connect_changed(move |effect| {
            if effect != Effect::Preview {
                copy.refresh();
            }
        });
        toolbar
    }
    /// Geometry-only sessions share the same style, placement and input routing,
    /// but expose an explicit operation instead of relabelling Copy internally.
    pub fn configure_selection(&self, purpose: clivuu_core::capture::Purpose) {
        use clivuu_core::capture::Purpose;
        if purpose.editable() {
            if purpose == Purpose::Image {
                for button in &self.0.continuous_buttons {
                    button.set_visible(false);
                }
            }
            return;
        }
        if purpose == Purpose::Recording {
            self.0.widget.set_visible(false);
            return;
        }
        let mut child = self.0.row.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            widget.set_visible(widget == self.0.grip);
        }
        let (icon, label) = ("long-capture", "Start scrolling capture");
        let accept = gtk::Button::builder()
            .child(&icons::icon(icon))
            .focusable(false)
            .build();
        self.0.tooltips.bind(&accept, label, "Enter");
        let output = self.0.output.clone();
        accept.connect_clicked(move |_| output(Output::Confirm));
        let title = gtk::Label::builder()
            .label(t(label))
            .yalign(0.5)
            .valign(gtk::Align::Fill)
            .single_line_mode(true)
            .css_classes(["clivuu-toolbar-metric"])
            .build();
        self.0.selection_title.replace(Some(title.clone()));
        self.0.row.append(&title);
        self.0.row.append(&accept);
        let cancel = gtk::Button::builder()
            .child(&icons::icon("close"))
            .focusable(false)
            .build();
        self.0.tooltips.bind(&cancel, "Cancel", "Esc");
        let output = self.0.output.clone();
        cancel.connect_clicked(move |_| output(Output::Close));
        self.0.row.append(&cancel);
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.widget
    }
    pub fn row(&self) -> &gtk::Box {
        &self.0.row
    }
    pub fn grip_button() -> gtk::Button {
        let button = gtk::Button::builder()
            .child(&icons::icon("grip"))
            .focusable(false)
            .tooltip_text(t("Move toolbar"))
            .build();
        button.set_cursor_from_name(Some("move"));
        button
    }
    pub fn grip(&self) -> &gtk::Button {
        &self.0.grip
    }
    pub fn set_output_width(&self, width: i32) {
        super::floating_bar::sync_scroller_width(
            &self.0.scroller,
            &self.0.row,
            (width - 24).max(1),
        );
    }
    pub fn undo_button(&self) -> &gtk::Button {
        &self.0.undo
    }
    pub fn redo_button(&self) -> &gtk::Button {
        &self.0.redo
    }
    pub fn set_ocr_active(&self, active: bool) {
        if self.0.ocr_active.replace(active) != active {
            self.refresh();
        }
    }
    pub fn connect_tool_click(&self, callback: impl Fn() + 'static) {
        self.0.on_tool_click.replace(Some(Box::new(callback)));
    }
    pub fn refresh(&self) {
        let language = clivuu_core::i18n::current_language();
        let ocr_active = self.0.ocr_active.get();
        if let Some(scene) = self.0.scene.upgrade() {
            let controller = scene.controller();
            let c = controller.borrow();
            let tool = c.tool();
            let can_undo = c.session().can_undo();
            let can_redo = c.session().can_redo();
            drop(c);
            let next_state = (tool, ocr_active, can_undo, can_redo, language);
            if self.0.last_state.get() == Some(next_state) {
                return;
            }
            self.0.last_state.set(Some(next_state));
            if self.0.tooltips.refresh()
                && let Some(title) = self.0.selection_title.borrow().as_ref()
            {
                title.set_label(&t("Start scrolling capture"));
            }
            if let Some(btn) = &self.0.ocr_button {
                if ocr_active {
                    btn.add_css_class("suggested-action");
                } else {
                    btn.remove_css_class("suggested-action");
                }
            }
            for (item, button) in &self.0.tools {
                let active =
                    !ocr_active && (tool == *item || *item == Tool::Rect && tool == Tool::Ellipse);
                if active {
                    button.add_css_class("suggested-action");
                } else {
                    button.remove_css_class("suggested-action");
                }
            }
            self.0.undo.set_sensitive(can_undo);
            self.0.redo.set_sensitive(can_redo);
            return;
        }
        if self.0.tooltips.refresh()
            && let Some(title) = self.0.selection_title.borrow().as_ref()
        {
            title.set_label(&t("Start scrolling capture"));
        }
        if let Some(btn) = &self.0.ocr_button {
            if ocr_active {
                btn.add_css_class("suggested-action");
            } else {
                btn.remove_css_class("suggested-action");
            }
        }
    }
    pub fn install_theme(display: &gtk::gdk::Display, dark: bool, accent: &str) -> Theme {
        if let Some(shared) = DISPLAY_THEMES.with(|themes| {
            let mut themes = themes.borrow_mut();
            themes.retain(|entry| entry.strong_count() > 0);
            themes
                .iter()
                .filter_map(Weak::upgrade)
                .find(|entry| entry.borrow().display == *display)
        }) {
            let mut theme = Theme { shared };
            theme.update(dark, accent);
            return theme;
        }
        let (css, accent_color, selected_ink) = theme_css(dark, accent);
        let base = gtk::CssProvider::new();
        base.load_from_string(include_str!("../../resources/editor-base.css"));
        let controls = gtk::CssProvider::new();
        controls.load_from_string(&css);
        // Preserve provider order as well as rule order. Merging the original
        // two providers changes GTK's cascade for popover spinbutton controls.
        for provider in [&base, &controls] {
            gtk::style_context_add_provider_for_display(
                display,
                provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        let shared = Rc::new(RefCell::new(DisplayTheme {
            display: display.clone(),
            base,
            controls,
            dark,
            accent_raw: accent.to_owned(),
            accent: accent_color,
            selected_ink,
        }));
        DISPLAY_THEMES.with(|themes| {
            themes.borrow_mut().push(Rc::downgrade(&shared));
        });
        Theme { shared }
    }
}

fn theme_css(dark: bool, accent: &str) -> (String, gtk::gdk::RGBA, gtk::gdk::RGBA) {
    let accent = gtk::gdk::RGBA::parse(accent)
        .unwrap_or_else(|_| gtk::gdk::RGBA::new(0.741, 0.576, 0.976, 1.));
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let background: [f32; 3] = if dark {
        [40., 42., 54.]
    } else {
        [230., 230., 230.]
    };
    let channel = |foreground: f32, background: f32| {
        linear(foreground * accent.alpha() + background / 255. * (1. - accent.alpha()))
    };
    let selected = if channel(accent.red(), background[0]) * 0.2126
        + channel(accent.green(), background[1]) * 0.7152
        + channel(accent.blue(), background[2]) * 0.0722
        > 0.179
    {
        "#282a36"
    } else {
        "#f8f8f2"
    };
    let (colors, danger) = if dark {
        (
            (
                "#282a36", "#f8f8f2", "#625578", "#44475a", "#21222c", "#625578",
            ),
            ("#cf3a3a", "#df4848", "#ffffff", "#ff6b6b", "#e55353"),
        )
    } else {
        (
            (
                "#e6e6e6", "#404040", "#999999", "#d1d1d1", "#ffffff", "#9b9b9b",
            ),
            ("#d93025", "#c0251b", "#ffffff", "#c92a22", "#b8251b"),
        )
    };
    let accent_color = accent;
    let hex_rgba = |r: f32, g: f32, b: f32, a: f32| {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            (r.clamp(0., 1.) * 255.).round() as u8,
            (g.clamp(0., 1.) * 255.).round() as u8,
            (b.clamp(0., 1.) * 255.).round() as u8,
            (a.clamp(0., 1.) * 255.).round() as u8
        )
    };
    let check_color = if dark {
        hex_rgba(
            accent.red().max(0.35),
            accent.green().max(0.35),
            accent.blue().max(0.35),
            accent.alpha(),
        )
    } else {
        hex_rgba(
            accent.red() * 0.72,
            accent.green() * 0.72,
            accent.blue() * 0.72,
            accent.alpha(),
        )
    };
    let item_selected = hex_rgba(
        accent.red(),
        accent.green(),
        accent.blue(),
        accent.alpha() * if dark { 0.18 } else { 0.16 },
    );
    let item_selected_hover = hex_rgba(
        accent.red(),
        accent.green(),
        accent.blue(),
        accent.alpha() * if dark { 0.26 } else { 0.24 },
    );
    let (item_hover, item_active, pop_bg, pop_border) = if dark {
        ("#ffffff14", "#ffffff22", "#252734", "#3e4255")
    } else {
        ("#0000000f", "#0000001c", "#fbfbfd", "#d3d7e3")
    };
    let accent = hex_rgba(accent.red(), accent.green(), accent.blue(), accent.alpha());
    let mut css = include_str!("../../resources/editor.css").to_owned();
    for (token, value) in [
        ("itemSelectedHover", item_selected_hover.as_str()),
        ("itemSelected", item_selected.as_str()),
        ("itemActive", item_active),
        ("itemHover", item_hover),
        ("checkInk", check_color.as_str()),
        ("popBorder", pop_border),
        ("popBg", pop_bg),
        ("selectedInk", selected),
        ("selected", accent.as_str()),
        ("background", colors.0),
        ("ink", colors.1),
        ("border", colors.2),
        ("hover", colors.3),
        ("field", colors.4),
        ("divider", colors.5),
        ("dangerHover", danger.1),
        ("dangerInk", danger.2),
        ("dangerIcon", danger.3),
        ("dangerBorder", danger.4),
        ("danger", danger.0),
    ] {
        css = css.replace(&format!("@clivuu_{token}"), value);
    }
    css.push_str(&super::color::style(dark));
    (
        css,
        accent_color,
        gtk::gdk::RGBA::parse(selected).expect("built-in selected ink"),
    )
}
