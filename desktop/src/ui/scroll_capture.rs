//! Long-capture presentation. Geometry and the live frame stay in the desktop
//! surface; commands cross into application orchestration, never into I/O here.
mod interaction;
mod placement;
mod sampling;
use crate::{
    editor::{capture_draw, capture_view::Canvas, layout},
    platform::surfaces::pins::Hosts,
    ui::{
        popup::PopupLayer,
        toolbar::{Output, Theme, Toolbar},
    },
};
use clivuu_core::{
    annotation::Bounds,
    capture::Selection,
    i18n::{t, tf},
    image::RgbaImage,
    scrolling::{Axis, Progress},
};
use gtk::{gdk, glib, graphene, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
pub struct Appearance {
    pub dark: bool,
    pub accent: String,
    pub display: capture_draw::CaptureStyle,
}
pub type Status = Rc<RefCell<Option<serde_json::Value>>>;
#[derive(Clone, Copy)]
pub enum Command {
    Sample,
    Auto,
    AutoCrop(bool),
    Reanchor,
    Reset,
    Cancel,
    Axis(Axis),
    TrimStart,
    TrimEnd,
    Region(Bounds),
    ToolbarMoved,
    Output(Output),
}
struct Reporting {
    status: Status,
    clock: Option<(gdk::FrameClock, glib::SignalHandlerId)>,
}
impl Drop for Reporting {
    fn drop(&mut self) {
        if let Some((clock, id)) = self.clock.take() {
            clock.disconnect(id);
        }
        self.status.borrow_mut().take();
    }
}
#[derive(Clone, Copy)]
struct State {
    progress: Progress,
    axis: Axis,
    capturing: bool,
    automatic: bool,
    auto_crop: bool,
}
struct Inner {
    hosts: Hosts,
    theme: RefCell<Theme>,
    owner: usize,
    rect: Cell<Bounds>,
    style: RefCell<capture_draw::CaptureStyle>,
    panel: gtk::Box,
    scroller: gtk::ScrolledWindow,
    hint: gtk::Label,
    label: gtk::Label,
    toolbar_position: Rc<crate::ui::toolbar_drag::ToolbarPosition>,
    auto: gtk::Button,
    crop: gtk::Button,
    reset: gtk::Button,
    axis: gtk::Button,
    axis_label: gtk::Label,
    popup: PopupLayer,
    preview: Canvas,
    texture: RefCell<Option<gdk::Texture>>,
    frames: Vec<Canvas>,
    input: Cell<Bounds>,
    interacting: Cell<bool>,
    input_epoch: Cell<u64>,
    auto_waiting: Cell<bool>,
    tooltips: super::localization::Tooltips,
    hint_rect: Cell<Bounds>,
    preview_rect: Cell<Option<Bounds>>,
    sampling: Cell<bool>,
    message: RefCell<String>,
    state: Cell<State>,
    buttons: RefCell<Vec<(String, gtk::Button)>>,
    menu_buttons: RefCell<Vec<(String, gtk::Button)>>,
    trim_buttons: RefCell<Vec<(Command, gtk::Button)>>,
    reanchor_button: RefCell<Option<gtk::Button>>,
    open_popup_exclusion: Cell<Option<Bounds>>,
    closing_popup_exclusion: Cell<Option<Bounds>>,
    suppress_popup_close_move: Cell<bool>,
    last_inputs: RefCell<Vec<Vec<Bounds>>>,
    last_layout: Cell<Option<LayoutKey>>,
    serial: Cell<u64>,
    emit: Rc<dyn Fn(Command)>,
}
type LayoutKey = (Bounds, Bounds, Bounds, Option<Bounds>, bool);
#[derive(Clone)]
pub struct Controls {
    _reporting: Rc<Reporting>,
    inner: Rc<Inner>,
}
fn button(
    tooltips: &super::localization::Tooltips,
    title: &str,
    icon: &'static str,
) -> gtk::Button {
    let b = super::floating_bar::icon_button(icon);
    let keys = match title {
        "Reset scrolling capture" => "Ctrl+R",
        "Pin image" => "Enter",
        "Save image" => "Ctrl+S",
        "Copy image" => "Ctrl+C",
        "Cancel" => "Esc",
        _ => "",
    };
    tooltips.bind(&b, title, keys);
    b
}
impl Controls {
    pub fn new(
        display: &gdk::Display,
        rect: Bounds,
        appearance: Appearance,
        status: Status,
        lost: impl Fn() + 'static,
        emit: impl Fn(Command) + 'static,
    ) -> Result<Self, String> {
        let hosts = Hosts::new(display, crate::platform::surfaces::PinRole::Controls, lost)?;
        let owner = hosts
            .surfaces
            .iter()
            .position(|s| s.rect.contains(rect.center(), 0.))
            .unwrap_or(0);
        let theme = Toolbar::install_theme(display, appearance.dark, &appearance.accent);
        let mut style = appearance.display;
        style.accent = theme.accent();
        let panel = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build();
        panel.add_css_class("clivuu-capture-toolbar");
        if appearance.dark {
            panel.add_css_class("clivuu-theme-dark");
        }
        panel.set_size_request(-1, super::floating_bar::TOOLBAR_HEIGHT);
        let (scroller, row) = super::floating_bar::scrollable_bar_row();
        panel.append(&scroller);
        let label = super::floating_bar::metric_label(
            &format!(
                "{} × {}",
                rect.width.round() as u32,
                rect.height.round() as u32
            ),
            9,
            14,
        );
        let tooltips = super::localization::Tooltips::default();
        let grip = Toolbar::grip_button();
        tooltips.bind(&grip, "Move toolbar", "");
        row.append(&grip);
        row.append(&label);
        row.append(&super::floating_bar::tool_separator());
        let move_region = button(&tooltips, "Move capture region", "move-region");
        row.append(&move_region);
        let (axis, axis_label) = super::floating_bar::text_button(&t("Vertical"));
        tooltips.bind(&axis, "Capture direction", "");
        row.append(&axis);
        row.append(&super::floating_bar::tool_separator());
        let auto = button(&tooltips, "Auto-scroll", "scroll");
        row.append(&auto);
        let crop = button(&tooltips, "Trim scrolling capture", "scissors");
        row.append(&crop);
        let reset = button(&tooltips, "Reset scrolling capture", "rotate-left");
        row.append(&reset);
        row.append(&super::floating_bar::tool_separator());
        let mut commands = vec![("Auto-scroll".to_owned(), auto.clone(), Command::Auto)];
        for (title, icon, action) in [
            ("Edit", "pen", Output::Apply),
            ("Pin image", "pin", Output::Pin),
            ("Save image", "quick-save", Output::Save),
            ("Copy image", "clipboard", Output::Copy),
        ] {
            let b = button(&tooltips, title, icon);
            row.append(&b);
            commands.push((title.to_owned(), b, Command::Output(action)));
        }
        let cancel = button(&tooltips, "Cancel", "close");
        row.append(&cancel);
        commands.push(("Cancel".into(), cancel, Command::Cancel));
        let hint = gtk::Label::new(None);
        hint.add_css_class("clivuu-capture-hint");
        hint.set_can_target(false);
        hint.set_max_width_chars(58);
        hint.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let surface = &hosts.surfaces[owner];
        let popup = PopupLayer::new(&surface.root);
        let preview = Canvas::new(|_| {});
        let frames = hosts
            .surfaces
            .iter()
            .map(|s| {
                let c = Canvas::new(|_| {});
                s.root.put(&c, 0., 0.);
                s.root
                    .set_child_size(&c, Some((s.rect.width as i32, s.rect.height as i32)));
                c
            })
            .collect();
        surface.root.put(&preview, 0., 0.);
        surface.root.put(&panel, 0., 0.);
        surface.root.put(&hint, 0., 0.);
        let inner = Rc::new(Inner {
            hosts,
            theme: RefCell::new(theme),
            owner,
            rect: Cell::new(rect),
            style: RefCell::new(style),
            panel,
            scroller,
            hint,
            label,
            toolbar_position: Rc::new(crate::ui::toolbar_drag::ToolbarPosition::default()),
            auto,
            crop: crop.clone(),
            reset: reset.clone(),
            axis,
            axis_label,
            popup,
            preview,
            texture: RefCell::new(None),
            frames,
            input: Cell::new(rect),
            interacting: Cell::new(false),
            input_epoch: Cell::new(0),
            auto_waiting: Cell::new(false),
            tooltips,
            hint_rect: Cell::new(rect),
            preview_rect: Cell::new(None),
            sampling: Cell::new(false),
            message: RefCell::new(String::new()),
            state: Cell::new(State {
                progress: Progress {
                    width: 0,
                    height: 0,
                    frames: 0,
                    position: 0.,
                    viewport: 1.,
                    changed: false,
                },
                axis: Axis::Vertical,
                capturing: true,
                automatic: false,
                auto_crop: false,
            }),
            buttons: RefCell::new(vec![]),
            menu_buttons: RefCell::new(vec![]),
            trim_buttons: RefCell::new(vec![]),
            reanchor_button: RefCell::new(None),
            open_popup_exclusion: Cell::new(None),
            closing_popup_exclusion: Cell::new(None),
            suppress_popup_close_move: Cell::new(false),
            last_inputs: RefCell::new(vec![]),
            last_layout: Cell::new(None),
            serial: Cell::new(0),
            emit: Rc::new(emit),
        });
        for (title, b, command) in commands {
            let weak = Rc::downgrade(&inner);
            b.connect_clicked(move |_| {
                if let Some(i) = weak.upgrade() {
                    i.command(command);
                }
            });
            inner.buttons.borrow_mut().push((t(&title), b));
        }
        {
            let weak = Rc::downgrade(&inner);
            inner.axis.connect_clicked(move |_| {
                if let Some(i) = weak.upgrade() {
                    let a = if i.state.get().axis == Axis::Vertical {
                        Axis::Horizontal
                    } else {
                        Axis::Vertical
                    };
                    i.command(Command::Axis(a));
                }
            });
        }
        {
            let weak = Rc::downgrade(&inner);
            crop.connect_clicked(move |anchor| {
                if let Some(i) = weak.upgrade() {
                    i.open_trim(anchor);
                }
            });
        }
        {
            let weak = Rc::downgrade(&inner);
            reset.connect_clicked(move |anchor| {
                if let Some(i) = weak.upgrade() {
                    i.open_reset(anchor);
                }
            });
        }
        {
            let weak = Rc::downgrade(&inner);
            inner.popup.connect_changed(move || {
                if let Some(i) = weak.upgrade() {
                    i.input_epoch.set(i.input_epoch.get().wrapping_add(1));
                    if i.popup.is_open() {
                        let _ = i.current_popup_exclusion();
                    } else {
                        let prev = i.open_popup_exclusion.take();
                        if !i.suppress_popup_close_move.get()
                            && prev.is_some_and(|b| clivuu_core::pins::overlaps(b, i.rect.get()))
                        {
                            i.closing_popup_exclusion.set(prev);
                            i.command(Command::ToolbarMoved);
                        }
                    }
                    i.update_input();
                }
            });
        }
        interaction::install(&inner, &grip, &move_region);
        // Frame callbacks capture weak owners, so destroying a session releases every
        // host and input shape even when a pointer gesture was in progress.
        for (n, s) in inner.hosts.surfaces.iter().enumerate() {
            let weak = Rc::downgrade(&inner);
            let origin = s.rect;
            inner.frames[n].set_painter(move |snapshot| {
                if let Some(i) = weak.upgrade() {
                    if i.sampling.get() {
                        // An empty GTK snapshot may leave the previous native
                        // buffer untouched. An explicit full-host node clears it,
                        // including shadows outside the toolbar's allocation.
                        snapshot.append_node(gtk::gsk::ColorNode::new(
                            &gdk::RGBA::new(0., 0., 0., 0.),
                            &graphene::Rect::new(
                                0.,
                                0.,
                                (origin.width + 1.) as f32,
                                (origin.height + 1.) as f32,
                            ),
                        ));
                        return;
                    }
                    let r = i.rect.get();
                    let mut style = i.style.borrow().clone();
                    style.anchors = false;
                    capture_draw::frame(
                        snapshot,
                        Bounds {
                            x: r.x - origin.x,
                            y: r.y - origin.y,
                            ..r
                        },
                        &style,
                    );
                }
            });
            s.window.set_title(Some("Clivuu scrolling capture"));
            s.window.set_visible(true);
        }
        {
            let weak = Rc::downgrade(&inner);
            inner.preview.set_painter(move |s| {
                if let Some(i) = weak.upgrade() {
                    i.draw_preview(s);
                }
            });
        }
        inner.buttons.borrow_mut().extend([
            (t("Move toolbar"), grip),
            (t("Move capture region"), move_region),
            (t("Capture direction"), inner.axis.clone()),
            (t("Trim scrolling capture"), crop),
            (t("Reset scrolling capture"), reset),
        ]);
        inner.sync_crop();
        inner.layout();
        inner.tooltips(true);
        // Map the host before requesting focus. A retiring capture overlay may
        // still be undergoing Mutter's XWayland/Wayland focus reconciliation.
        inner.hosts.surfaces[owner].set_keyboard(true, false)?;
        let weak = Rc::downgrade(&inner);
        let clock = inner.hosts.surfaces[owner]
            .window
            .frame_clock()
            .map(|clock| {
                let status = status.clone();
                let id = clock.connect_after_paint(move |_| {
                    if let Some(i) = weak.upgrade() {
                        // Popup allocation is only final after layout/paint.
                        // Publish its real input region before exposing bounds.
                        if i.popup.is_open() {
                            let _ = i.current_popup_exclusion();
                        }
                        i.update_input();
                        i.report(&status);
                    }
                });
                (clock, id)
            });
        Ok(Self {
            inner,
            _reporting: Rc::new(Reporting { status, clock }),
        })
    }
    pub fn interacting(&self) -> bool {
        self.inner.interacting.get()
    }
    pub fn input_epoch(&self) -> u64 {
        self.inner.input_epoch.get()
    }
    pub fn accepts_scroll(&self, point: [f64; 2]) -> bool {
        let i = &self.inner;
        clivuu_core::capture::contains(i.rect.get(), point)
            && !i.interacting.get()
            && !clivuu_core::capture::contains(i.input.get(), point)
            && !clivuu_core::capture::contains(i.hint_rect.get(), point)
            && !i.popup.is_open()
    }
    pub fn auto_waiting(&self, waiting: bool) {
        let changed = self.inner.auto_waiting.replace(waiting) != waiting;
        let title = t(if waiting {
            "Move the pointer into the capture area to auto-scroll"
        } else {
            "Auto-scroll"
        });
        if !changed && self.inner.auto.tooltip_text().as_deref() == Some(title.as_str()) {
            return;
        }
        self.inner.auto.set_tooltip_text(Some(&title));
        self.inner.auto.set_has_tooltip(!self.inner.sampling.get());
        self.inner.panel.queue_draw();
    }
    pub fn sampling_allowed(&self) -> bool {
        !self.inner.interacting.get()
            && !self.inner.sampling.get()
            && (!self.inner.popup.is_open() || self.inner.open_popup_exclusion.get().is_some())
    }
    pub fn clear_closing_popup_exclusion(&self) {
        self.inner.closing_popup_exclusion.set(None);
    }
    pub fn exclusions(&self) -> Vec<Bounds> {
        let i = &self.inner;
        let mut areas =
            placement::exclusions(i.input.get(), i.hint_rect.get(), i.preview_rect.get());
        if let Some(popup) = i
            .current_popup_exclusion()
            .or_else(|| i.closing_popup_exclusion.get())
        {
            areas.push(popup);
        }
        areas
    }
    pub fn dismiss(&self) {
        self.inner.popup.close();
        self.inner.open_popup_exclusion.set(None);
        self.inner.closing_popup_exclusion.set(None);
        self.inner.last_inputs.borrow_mut().clear();
        for s in &self.inner.hosts.surfaces {
            s.set_input([]);
            s.window.set_visible(false);
        }
    }
    pub fn set_appearance(&self, appearance: Appearance) {
        let i = &self.inner;
        if appearance.dark {
            i.panel.add_css_class("clivuu-theme-dark");
        } else {
            i.panel.remove_css_class("clivuu-theme-dark");
        }
        i.theme
            .borrow_mut()
            .update(appearance.dark, &appearance.accent);
        let mut style = appearance.display;
        style.accent = i.theme.borrow().accent();
        *i.style.borrow_mut() = style;
        i.panel.queue_draw();
        i.hint.queue_draw();
        i.preview.queue_draw();
        for c in &i.frames {
            c.queue_draw();
        }
        i.surface().root.queue_draw();
    }
    pub fn sampling(&self, value: bool) {
        let i = &self.inner;
        let hide = value;
        i.sampling.set(hide);
        i.tooltips(!hide);
        if hide {
            i.popup.close();
            i.open_popup_exclusion.set(None);
            i.closing_popup_exclusion.set(None);
        }
        // Snapshot transparency must not remove hit targets. Otherwise a click
        // during a full-screen sample would pass through to the captured app.
        let opacity = if hide { 0. } else { 1. };
        i.panel.set_opacity(opacity);
        i.hint.set_opacity(opacity);
        i.preview.set_opacity(opacity);
        for c in &i.frames {
            c.queue_draw();
        }
        i.update_input();
    }
    pub fn state(
        &self,
        progress: Progress,
        axis: Axis,
        capturing: bool,
        automatic: bool,
        auto_crop: bool,
    ) {
        let i = &self.inner;
        let previous = i.state.get();
        let translated = i.tooltips.refresh();
        if translated {
            self.auto_waiting(i.auto_waiting.get());
        }
        if progress.frames == 0 {
            i.texture.take();
        }
        i.state.set(State {
            progress,
            axis,
            capturing,
            automatic,
            auto_crop,
        });
        if previous.progress.width != progress.width || previous.progress.height != progress.height
        {
            i.label
                .set_label(&format!("{} × {}", progress.width, progress.height));
        }
        if previous.axis != axis || translated {
            i.axis_label.set_label(&t(if axis == Axis::Vertical {
                "Vertical"
            } else {
                "Horizontal"
            }));
        }
        if previous.automatic != automatic {
            self.automatic(automatic);
        }
        let visual_changed = translated
            || previous.progress.frames != progress.frames
            || previous.progress.width != progress.width
            || previous.progress.height != progress.height
            || (previous.progress.position - progress.position).abs() > 1e-9
            || (previous.progress.viewport - progress.viewport).abs() > 1e-9
            || previous.axis != axis
            || previous.capturing != capturing
            || previous.automatic != automatic
            || previous.auto_crop != auto_crop;
        if visual_changed {
            i.sync_crop();
            i.layout();
            i.preview.queue_draw();
        }
    }
    pub fn preview(&self, pixels: Arc<RgbaImage>, progress: Progress) {
        let i = &self.inner;
        let had_texture = i.texture.borrow().is_some();
        let texture = gdk::MemoryTexture::new(
            pixels.width() as i32,
            pixels.height() as i32,
            gdk::MemoryFormat::R8g8b8a8,
            &glib::Bytes::from_owned(pixels.as_raw().clone()),
            pixels.width() as usize * 4,
        );
        i.texture.replace(Some(texture.upcast()));
        let mut state = i.state.get();
        let size_changed =
            state.progress.width != progress.width || state.progress.height != progress.height;
        state.progress = progress;
        i.state.set(state);
        if size_changed {
            i.label
                .set_label(&format!("{} × {}", progress.width, progress.height));
        }
        i.sync_crop();
        if !had_texture {
            i.layout();
        }
        i.preview.queue_draw();
    }
    pub fn message(&self, message: &str) {
        if self.inner.message.borrow().as_str() == message {
            return;
        }
        self.inner.message.replace(message.to_owned());
        self.inner.sync_crop();
        self.inner.layout();
        self.inner.preview.queue_draw();
        self.inner.panel.set_tooltip_text(if message.is_empty() {
            None
        } else {
            Some(message)
        });
        self.inner
            .panel
            .set_has_tooltip(!message.is_empty() && !self.inner.sampling.get());
    }
    pub fn automatic(&self, value: bool) {
        let i = &self.inner;
        if value {
            i.auto.add_css_class("suggested-action");
        } else {
            i.auto.remove_css_class("suggested-action");
        }
    }
}
impl Inner {
    fn tooltips(&self, enabled: bool) {
        self.panel
            .set_has_tooltip(enabled && self.panel.tooltip_text().is_some());
        self.label
            .set_has_tooltip(enabled && self.label.tooltip_text().is_some());
        let buttons: Vec<_> = self
            .buttons
            .borrow()
            .iter()
            .map(|(_, button)| button.clone())
            .collect();
        for button in buttons {
            button.set_has_tooltip(enabled);
        }
    }
    fn trim_sensitive(&self, command: Command) -> bool {
        let p = self.state.get().progress;
        self.message.borrow().is_empty()
            && p.viewport < 1. - 1e-6
            && match command {
                Command::TrimStart => p.position > 1e-6,
                Command::TrimEnd => p.position + p.viewport < 1. - 1e-6,
                _ => false,
            }
    }
    fn sync_crop(&self) {
        let s = self.state.get();
        let trim_targets: Vec<(gtk::Button, bool)> = self
            .trim_buttons
            .borrow()
            .iter()
            .map(|(command, button)| (button.clone(), self.trim_sensitive(*command)))
            .collect();
        let reanchor_target = self
            .reanchor_button
            .borrow()
            .as_ref()
            .map(|button| (button.clone(), s.progress.frames >= 1));
        let menu_buttons: Vec<gtk::Button> = self
            .menu_buttons
            .borrow()
            .iter()
            .map(|(_, button)| button.clone())
            .collect();

        let mut targets = trim_targets;
        if let Some(reanchor) = reanchor_target {
            targets.push(reanchor);
        }

        if self.crop.is_sensitive() != s.capturing {
            self.crop.set_sensitive(s.capturing);
        }
        if !self.reset.is_sensitive() {
            self.reset.set_sensitive(true);
        }
        for (button, sensitive) in &targets {
            if *sensitive && !button.is_sensitive() {
                button.set_sensitive(true);
            }
        }

        // Disabling a focused popup button causes GTK to clear root focus to None,
        // firing EventControllerFocus::leave and closing the popup mid-scroll.
        // Move focus to another sensitive popup control before calling set_sensitive(false).
        let focused = gtk::prelude::GtkWindowExt::focus(&self.surface().window);
        let disabling_focused = targets.iter().any(|(button, sensitive)| {
            !*sensitive
                && button.is_sensitive()
                && (button.has_focus()
                    || focused.as_ref().is_some_and(|w| {
                        w == button.upcast_ref::<gtk::Widget>() || w.is_ancestor(button)
                    }))
        });
        if disabling_focused
            && let Some(replacement) = menu_buttons.into_iter().find(|b| {
                b.is_sensitive()
                    && b.is_focusable()
                    && !targets.iter().any(|(t, sens)| !*sens && t == b)
            })
        {
            replacement.grab_focus();
        }

        for (button, sensitive) in &targets {
            if !*sensitive && button.is_sensitive() {
                button.set_sensitive(false);
            }
        }
    }
    fn open_trim(self: &Rc<Self>, anchor: &gtk::Button) {
        self.menu_buttons.borrow_mut().clear();
        self.trim_buttons.borrow_mut().clear();
        self.reanchor_button.borrow_mut().take();
        if self.popup.is_open() {
            self.popup.close();
            return;
        }
        if !self.crop.is_sensitive() {
            return;
        }
        let column = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(2)
            .css_classes(["clivuu-trim-list"])
            .build();
        let vertical = self.state.get().axis == Axis::Vertical;
        for (title, icon, command) in if vertical {
            [
                ("Trim above current view", "trim-top", Command::TrimStart),
                ("Trim below current view", "trim-bottom", Command::TrimEnd),
            ]
        } else {
            [
                ("Trim left of current view", "trim-left", Command::TrimStart),
                ("Trim right of current view", "trim-right", Command::TrimEnd),
            ]
        } {
            let b = button(&self.tooltips, title, icon);
            b.set_focusable(true);
            b.add_css_class("clivuu-trim-action");
            b.set_valign(gtk::Align::Center);
            b.set_has_tooltip(!self.sampling.get());
            b.set_sensitive(self.trim_sensitive(command));
            let weak = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(i) = weak.upgrade() {
                    i.popup.close();
                    i.command(command);
                }
            });
            column.append(&b);
            self.trim_buttons.borrow_mut().push((command, b.clone()));
            self.menu_buttons.borrow_mut().push((t(title), b));
        }
        let sep = gtk::Separator::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .height_request(14)
            .margin_start(2)
            .margin_end(2)
            .can_target(false)
            .css_classes(["clivuu-tool-separator"])
            .build();
        column.append(&sep);
        let auto_crop_title = "Auto-crop on reverse scroll";
        let auto_crop = button(&self.tooltips, auto_crop_title, "crop");
        auto_crop.set_focusable(true);
        auto_crop.add_css_class("clivuu-trim-action");
        auto_crop.set_valign(gtk::Align::Center);
        auto_crop.set_has_tooltip(!self.sampling.get());
        if self.state.get().auto_crop {
            auto_crop.add_css_class("suggested-action");
        }
        {
            let weak = Rc::downgrade(self);
            auto_crop.connect_clicked(move |btn| {
                if let Some(i) = weak.upgrade() {
                    let mut s = i.state.get();
                    s.auto_crop = !s.auto_crop;
                    i.state.set(s);
                    if s.auto_crop {
                        btn.add_css_class("suggested-action");
                    } else {
                        btn.remove_css_class("suggested-action");
                    }
                    i.command(Command::AutoCrop(s.auto_crop));
                }
            });
        }
        column.append(&auto_crop);
        self.menu_buttons
            .borrow_mut()
            .push((t(auto_crop_title), auto_crop));
        let weak = Rc::downgrade(self);
        self.popup.show(anchor, &column, move || {
            if let Some(i) = weak.upgrade() {
                i.menu_buttons.borrow_mut().clear();
                i.trim_buttons.borrow_mut().clear();
            }
        });
        let _ = self.surface().set_keyboard(true, true);
        let first = self
            .menu_buttons
            .borrow()
            .iter()
            .find(|(_, b)| b.is_sensitive())
            .map(|(_, b)| b.clone());
        if let Some(first) = first {
            first.grab_focus();
        }
    }
    fn open_reset(self: &Rc<Self>, anchor: &gtk::Button) {
        self.menu_buttons.borrow_mut().clear();
        self.trim_buttons.borrow_mut().clear();
        self.reanchor_button.borrow_mut().take();
        if self.popup.is_open() {
            self.popup.close();
            return;
        }
        let column = super::floating_bar::choice_list();
        let has_frames = self.state.get().progress.frames >= 1;
        for (title, command, sensitive) in [
            ("Continue from current view", Command::Reanchor, has_frames),
            ("Restart in current region", Command::Reset, true),
        ] {
            let label = t(title);
            let b = super::floating_bar::action_item(&label);
            b.set_focusable(true);
            b.set_sensitive(sensitive);
            let weak = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(i) = weak.upgrade() {
                    i.popup.close();
                    i.command(command);
                }
            });
            column.append(&b);
            if matches!(command, Command::Reanchor) {
                self.reanchor_button.borrow_mut().replace(b.clone());
            }
            self.menu_buttons.borrow_mut().push((label, b));
        }
        let weak = Rc::downgrade(self);
        self.popup.show(anchor, &column, move || {
            if let Some(i) = weak.upgrade() {
                i.menu_buttons.borrow_mut().clear();
                i.trim_buttons.borrow_mut().clear();
                i.reanchor_button.borrow_mut().take();
            }
        });
        let _ = self.surface().set_keyboard(true, true);
        let first = self
            .menu_buttons
            .borrow()
            .iter()
            .find(|(_, b)| b.is_sensitive())
            .map(|(_, b)| b.clone());
        if let Some(first) = first {
            first.grab_focus();
        }
    }
    fn surface(&self) -> &Rc<crate::platform::surfaces::pins::Surface> {
        &self.hosts.surfaces[self.owner]
    }
    fn command(&self, command: Command) {
        self.serial.set(self.serial.get() + 1);
        (self.emit)(command);
    }
    fn layout(&self) {
        let surface = self.surface();
        self.scroller
            .set_max_content_width((surface.rect.width as i32 - 24).clamp(1, 1000));
        let rect = self.rect.get();
        let hint_text = {
            let message = self.message.borrow();
            if !message.is_empty() {
                if *message == t("No reliable overlap found") {
                    tf(
                        "{0} · Scroll back, or use Reset to continue or restart",
                        &[message.as_str()],
                    )
                } else {
                    message.clone()
                }
            } else {
                format!("{} × {} px", rect.width as u32, rect.height as u32)
            }
        };
        if self.hint.label().as_str() != hint_text {
            self.hint.set_label(&hint_text);
        }
        let width = self.panel.measure(gtk::Orientation::Horizontal, -1).1 as f64;
        let hint = layout::Size {
            width: self.hint.measure(gtk::Orientation::Horizontal, -1).1 as f64,
            height: self.hint.measure(gtk::Orientation::Vertical, -1).1 as f64,
        };
        let placement = layout::selection(
            rect,
            surface.rect,
            layout::Size { width, height: 32. },
            hint,
        );
        let r = self.toolbar_position.place(placement.toolbar, surface.rect);
        self.input.set(r);
        let h = placement.hint;
        self.hint_rect.set(h);
        // Reserve one stable preview slot per capture geometry. Updating its
        // texture must never move the HUD or fence out a valid final page frame.
        let size = if self.texture.borrow().is_some() {
            [184., 364.]
        } else {
            [0., 0.]
        };
        let output = surface.rect;
        let preview = placement::preview(rect, output, size, &[placement::paint_bounds(r), h]);
        self.preview_rect.set(preview);
        let visible_preview = preview.is_some() && !self.sampling.get();
        let key = (rect, r, h, preview, visible_preview);
        if self.last_layout.get() == Some(key) {
            return;
        }
        self.last_layout.set(Some(key));
        surface
            .root
            .move_(&self.panel, r.x - surface.rect.x, r.y - surface.rect.y);
        surface
            .root
            .set_child_size(&self.panel, Some((r.width as i32, 32)));
        surface
            .root
            .move_(&self.hint, h.x - surface.rect.x, h.y - surface.rect.y);
        surface
            .root
            .set_child_size(&self.hint, Some((h.width as i32, h.height as i32)));
        if let Some(p) = preview {
            surface
                .root
                .move_(&self.preview, p.x - output.x, p.y - output.y);
            surface.root.set_child_size(
                &self.preview,
                Some((p.width.ceil() as i32, p.height.ceil() as i32)),
            );
        }
        self.preview.set_visible(visible_preview);
        self.popup.set_viewport(Bounds {
            x: 0.,
            y: 0.,
            width: output.width,
            height: output.height,
        });
        self.popup.reposition();
        self.update_input();
    }
    fn current_popup_exclusion(&self) -> Option<Bounds> {
        if !self.popup.is_open() {
            return None;
        }
        if let Some(p) = self.popup.bounds()
            && p.width() > 0.
            && p.height() > 0.
        {
            let origin = self.surface().rect;
            let excluded = placement::popup_paint_bounds(Bounds {
                x: p.x() as f64 + origin.x,
                y: p.y() as f64 + origin.y,
                width: p.width() as f64,
                height: p.height() as f64,
            });
            self.open_popup_exclusion.set(Some(excluded));
            return Some(excluded);
        }
        self.open_popup_exclusion.get()
    }
    fn update_input(&self) {
        let popup_bounds = self
            .popup
            .bounds()
            .filter(|p| p.width() > 0. && p.height() > 0.);
        let owner_rect = self.surface().rect;
        let owner_areas = {
            let mut areas = vec![self.input.get()];
            if let Some(p) = popup_bounds {
                areas.push(Bounds {
                    x: p.x() as f64 + owner_rect.x,
                    y: p.y() as f64 + owner_rect.y,
                    width: p.width() as f64,
                    height: p.height() as f64,
                });
            }
            areas
        };
        let updates: Vec<(usize, Vec<Bounds>)> = {
            let mut prev = self.last_inputs.borrow_mut();
            if prev.len() != self.hosts.surfaces.len() {
                prev.resize_with(self.hosts.surfaces.len(), Vec::new);
            }
            let mut updates = vec![];
            for (idx, surface) in self.hosts.surfaces.iter().enumerate() {
                let areas = if Rc::ptr_eq(surface, self.surface()) {
                    owner_areas.clone()
                } else {
                    vec![]
                };
                if prev[idx] != areas {
                    prev[idx].clone_from(&areas);
                    updates.push((idx, areas));
                }
            }
            updates
        };
        for (idx, areas) in updates {
            self.hosts.surfaces[idx].set_input(areas);
        }
    }
    fn draw_preview(&self, s: &gtk::Snapshot) {
        let Some(texture) = self.texture.borrow().clone() else {
            return;
        };
        let Some(preview) = self.preview_rect.get() else {
            return;
        };
        let (w, h) = (preview.width, preview.height);
        let scale = ((w - 4.) / texture.width() as f64).min((h - 4.) / texture.height() as f64);
        let (image_w, image_h) = (
            texture.width() as f64 * scale,
            texture.height() as f64 * scale,
        );
        let (left, top) = ((w - image_w) / 2., (h - image_h) / 2.);
        let bounds = graphene::Rect::new(
            (left - 2.) as f32,
            (top - 2.) as f32,
            (image_w + 4.) as f32,
            (image_h + 4.) as f32,
        );
        s.append_border(
            &gtk::gsk::RoundedRect::from_rect(bounds, 0.),
            &[2.; 4],
            &[gdk::RGBA::WHITE; 4],
        );
        // Transparent pixels represent uncovered source data. Do not paint
        // those holes white, which looks like captured page content.
        s.append_color(
            &gdk::RGBA::new(0.24, 0.24, 0.28, 1.),
            &graphene::Rect::new(left as f32, top as f32, image_w as f32, image_h as f32),
        );
        s.append_texture(
            &texture,
            &graphene::Rect::new(left as f32, top as f32, image_w as f32, image_h as f32),
        );
        // A rejected sample leaves the last confirmed image intact. Its old
        // viewport is not evidence of the current page position.
        if !self.message.borrow().is_empty() {
            return;
        }
        let p = self.state.get().progress;
        let horizontal = self.state.get().axis == Axis::Horizontal;
        let (x, y, width, height) = if horizontal {
            (
                left + p.position * image_w,
                top,
                p.viewport * image_w,
                image_h,
            )
        } else {
            (
                left,
                top + p.position * image_h,
                image_w,
                p.viewport * image_h,
            )
        };
        let rect = Bounds {
            x,
            y,
            width,
            height,
        };
        let mut style = self.style.borrow().clone();
        style.border_width = 2.;
        style.accent = gdk::RGBA::parse("#77dd55").unwrap();
        style.anchors = false;
        capture_draw::frame(s, rect, &style);
    }
    fn report(&self, status: &Status) {
        let surface = self.surface();
        let state = self.state.get();
        let buttons: Vec<_> = self
            .buttons
            .borrow()
            .iter()
            .chain(self.menu_buttons.borrow().iter())
            .filter(|(_, button)| button.is_mapped())
            .filter_map(|(title, button)| {
                let rect = button.compute_bounds(&surface.root)?;
                Some(serde_json::json!({
                    "label": title,
                    "sensitive":button.is_sensitive(),
                    "cursor":button.cursor().and_then(|cursor| cursor.name()).map(|name| name.to_string()),
                    "rect": {
                        "x": rect.x() as f64 + surface.rect.x,
                        "y": rect.y() as f64 + surface.rect.y,
                        "width": rect.width(),
                        "height": rect.height(),
                    },
                }))
            })
            .collect();
        let focus = gtk::prelude::GtkWindowExt::focus(&surface.window).map(|widget| {
            serde_json::json!({
                "type": widget.type_().name(),
                "label": widget.downcast_ref::<gtk::Button>()
                    .and_then(super::floating_bar::button_text),
            })
        });
        status.replace(Some(serde_json::json!({
            "frames": state.progress.frames,
            "width": state.progress.width,
            "height": state.progress.height,
            "position": state.progress.position,
            "viewport": state.progress.viewport,
            "axis": state.axis,
            "capturing": state.capturing,
            "automatic": state.automatic,
            "autoCrop": state.auto_crop,
            "autoWaiting": self.auto_waiting.get(),
            "interacting": self.interacting.get(),
            "rect": self.rect.get(),
            "toolbar": self.input.get(),
            "preview": self.preview_rect.get(),
            "controlsVisible": self.panel.is_visible(),
            "mapped": self.panel.is_mapped(),
            "sampling": self.sampling.get(),
            "message": self.message.borrow().as_str(),
            "tracking": self.message.borrow().is_empty(),
            "toolbarOpacity": self.panel.opacity(),
            "previewOpacity": self.preview.opacity(),
            "active": surface.window.is_active(),
            "buttons": buttons,
            "commandSerial": self.serial.get(),
            "popupOpen": self.popup.is_open(),
            "focus": focus,
        })));
    }
}
