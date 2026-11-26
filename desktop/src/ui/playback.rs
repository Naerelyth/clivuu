//! Frameless playback presentation using the same surfaces, toolbar theme and
//! draggable positioning as capture. Media and file ownership stay in application.
mod timeline;
use crate::{
    editor::{
        capture_draw::{self, CaptureStyle},
        capture_view::Canvas,
        layout,
    },
    platform::surfaces::{PinRole, pins::Hosts},
    ui::{
        popup::PopupLayer,
        toolbar::{Theme, Toolbar},
        toolbar_drag::ToolbarPosition,
    },
};
use clivuu_core::{
    annotation::Bounds,
    capture::{Output, Selection},
    i18n::t,
    recording::{self, AudioRole, Format, MaterialInfo, PlaybackEdit},
};
use gtk::{gdk, glib, graphene, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
#[derive(Clone)]
pub enum Command {
    Toggle,
    Seek(u64),
    Edit(PlaybackEdit),
    Save(bool),
    Copy,
    Restart,
    Close,
    CancelExport,
    Unavailable,
}
struct Inner {
    hosts: Hosts,
    owner: usize,
    theme: RefCell<Theme>,
    source: Bounds,
    info: MaterialInfo,
    edit: RefCell<PlaybackEdit>,
    selection: RefCell<Selection>,
    canvas: Canvas,
    texture: RefCell<Option<gdk::Texture>>,
    bar: gtk::Box,
    upper: gtk::Stack,
    export_percent: gtk::Label,
    notice_timer: RefCell<Option<glib::SourceId>>,
    scroller: gtk::ScrolledWindow,
    controls: gtk::Box,
    time: gtk::Label,
    play: gtk::Button,
    play_icons: gtk::Stack,
    reset: gtk::Button,
    speed_label: gtk::Stack,
    format_label: gtk::Stack,
    format: Cell<Format>,
    timeline: timeline::Timeline,
    message: gtk::Label,
    notice_dismiss: gtk::Button,
    progress: gtk::ProgressBar,
    cancel: gtk::Button,
    popup: PopupLayer,
    position: Rc<ToolbarPosition>,
    toolbar: Cell<Bounds>,
    style: RefCell<CaptureStyle>,
    suspended: Cell<bool>,
    exporting: Cell<bool>,
    emit: Rc<dyn Fn(Command)>,
    clock: RefCell<Option<(gdk::FrameClock, glib::SignalHandlerId)>>,
}
#[derive(Clone)]
pub struct Controls(Rc<Inner>);
fn button(title: &str, icon: &'static str, key: &str) -> gtk::Button {
    let b = gtk::Button::builder()
        .child(&crate::ui::icons::icon(icon))
        .focusable(false)
        .build();
    b.set_tooltip_text(Some(&crate::editor::shortcuts::tooltip(title, key)));
    b
}
fn choice<I, S>(options: I, initial: &str, title: &str) -> (gtk::Button, gtk::Stack)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let stack = gtk::Stack::builder()
        .hhomogeneous(true)
        .vhomogeneous(true)
        .build();
    for text in options {
        let text = text.as_ref();
        let label = super::floating_bar::bar_label(text);
        stack.add_named(&label, Some(text));
    }
    stack.set_visible_child_name(initial);
    let arrow = super::icons::icon_sized("chevron-down", 10);
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    row.append(&stack);
    row.append(&arrow);
    let b = gtk::Button::builder()
        .child(&row)
        .focusable(false)
        .valign(gtk::Align::Center)
        .css_classes(["clivuu-toolbar-text", "clivuu-toolbar-choice"])
        .build();
    b.set_tooltip_text(Some(&t(title)));
    (b, stack)
}
fn select_choice(stack: &gtk::Stack, text: &str) {
    if stack.child_by_name(text).is_none() {
        let label = super::floating_bar::bar_label(text);
        stack.add_named(&label, Some(text));
    }
    stack.set_visible_child_name(text);
}
fn rate_label(rate: f64) -> String {
    let text = format!("{rate:.2}");
    format!("{}×", text.strip_suffix('0').unwrap_or(&text))
}
impl Controls {
    pub fn new(
        display: &gdk::Display,
        rect: Bounds,
        appearance: super::recording::Appearance,
        info: MaterialInfo,
        emit: impl Fn(Command) + 'static,
    ) -> Result<Self, String> {
        let emit: Rc<dyn Fn(Command)> = Rc::new(emit);
        let lost = emit.clone();
        let hosts = Hosts::new(display, PinRole::Controls, move || {
            lost(Command::Unavailable)
        })?;
        let owner = hosts
            .surfaces
            .iter()
            .position(|s| s.rect.contains(rect.center(), 0.))
            .unwrap_or(0);
        let surface = &hosts.surfaces[owner];
        let theme = Toolbar::install_theme(display, appearance.dark, &appearance.accent);
        let accent = theme.accent();
        let mut style = appearance.display;
        style.accent = accent;
        let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bar.add_css_class("clivuu-capture-toolbar");
        bar.add_css_class("clivuu-playback-toolbar");
        if appearance.dark {
            bar.add_css_class("clivuu-theme-dark");
        }
        let (scroller, controls) = super::floating_bar::scrollable_bar_row();
        scroller.set_vexpand(false);
        controls.set_margin_start(3);
        controls.set_margin_end(3);
        controls.set_margin_top(3);
        controls.set_margin_bottom(3);
        let grip = Toolbar::grip_button();
        grip.set_tooltip_text(Some(&t("Move toolbar")));
        controls.append(&grip);
        let play = button("Play / pause", "play", "Space");
        let play_icons = gtk::Stack::new();
        for name in ["play", "pause"] {
            play_icons.add_named(&crate::ui::icons::icon(name), Some(name));
        }
        play.set_child(Some(&play_icons));
        controls.append(&play);
        let time = super::floating_bar::metric_label("00:00 / 00:00", 16, 18);
        controls.append(&time);
        let (speed, speed_label) = choice(
            recording::PLAYBACK_RATES.iter().map(|r| rate_label(*r)),
            "1.0×",
            "Playback speed",
        );
        controls.append(&speed);
        controls.append(&super::floating_bar::tool_separator());
        let sound = button("Recorded audio", "record-speaker", "");
        sound.set_sensitive(!info.tracks.is_empty());
        if info.tracks.is_empty() {
            sound.set_tooltip_text(Some(&t("No audio tracks in this recording")));
        }
        controls.append(&sound);
        let reset = button("Reset playback edits", "undo", "");
        reset.set_sensitive(false);
        controls.append(&reset);
        controls.append(&super::floating_bar::tool_separator());
        let (format, format_label) = choice(
            [
                Format::Mp4.label(),
                Format::Webm.label(),
                Format::Gif.label(),
            ],
            Format::Mp4.label(),
            "Export format",
        );
        controls.append(&format);
        let save = button("Save recording", "quick-save", "Ctrl+S");
        controls.append(&save);
        let quick = button("Quick save", "download", "Ctrl+Shift+S");
        controls.append(&quick);
        let copy = button("Copy file", "clipboard", "Ctrl+C");
        controls.append(&copy);
        controls.append(&super::floating_bar::tool_separator());
        let restart = button("Restart recording", "restart-recording", "");
        controls.append(&restart);
        let close = button("Close playback", "close", "Esc");
        controls.append(&close);
        let weak_holder = Rc::new(RefCell::new(std::rc::Weak::<Inner>::new()));
        let weak = weak_holder.clone();
        let timeline = timeline::Timeline::new(&info, accent, move |change| {
            if let Some(i) = weak.borrow().upgrade() {
                match change {
                    timeline::Change::Seek(n) => (i.emit)(Command::Seek(n)),
                    timeline::Change::Trim(a, b) => {
                        let mut edit = i.edit.borrow().clone();
                        edit.start_ns = a;
                        edit.end_ns = b;
                        (i.emit)(Command::Edit(edit));
                    }
                }
            }
        });
        let upper = gtk::Stack::builder()
            .height_request(Toolbar::HEIGHT * 3 / 4 - 2)
            .hhomogeneous(false)
            .vhomogeneous(false)
            .hexpand(true)
            .vexpand(false)
            .build();
        upper.add_named(timeline.widget(), Some("timeline"));
        bar.append(&upper);
        let divider = gtk::Separator::new(gtk::Orientation::Horizontal);
        divider.add_css_class("clivuu-playback-divider");
        divider.set_can_target(false);
        bar.append(&divider);
        bar.append(&scroller);
        let mut child = controls.first_child();
        while let Some(widget) = child {
            if !widget.is::<gtk::Separator>() && !widget.is::<gtk::Label>() {
                widget.set_valign(gtk::Align::Center);
            }
            child = widget.next_sibling();
        }
        let message = gtk::Label::new(None);
        message.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        message.set_max_width_chars(1);
        message.set_hexpand(true);
        message.set_margin_start(8);
        message.set_margin_end(4);
        message.set_visible(false);
        let notice_dismiss = button("Dismiss", "close", "");
        notice_dismiss.add_css_class("clivuu-playback-cancel");
        notice_dismiss.set_valign(gtk::Align::Center);
        notice_dismiss.set_visible(false);
        let notice_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        notice_row.set_margin_end(3);
        notice_row.append(&message);
        notice_row.append(&notice_dismiss);
        upper.add_named(&notice_row, Some("notice"));
        let progress = gtk::ProgressBar::builder()
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        let export_percent = gtk::Label::new(Some("0%"));
        export_percent.set_width_chars(4);
        let cancel = button("Cancel export", "close", "");
        cancel.add_css_class("clivuu-playback-cancel");
        cancel.set_valign(gtk::Align::Center);
        let export_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        export_row.set_margin_start(8);
        export_row.set_margin_end(3);
        export_row.append(&progress);
        export_row.append(&export_percent);
        export_row.append(&cancel);
        upper.add_named(&export_row, Some("export"));
        upper.set_visible_child_name("timeline");
        let canvas = Canvas::new(|_| {});
        canvas.set_can_target(true);
        surface.root.put(&canvas, 0., 0.);
        surface.root.set_child_size(
            &canvas,
            Some((surface.rect.width as i32, surface.rect.height as i32)),
        );
        surface.root.put(&bar, 0., 0.);
        let popup = PopupLayer::new(&surface.root);
        let scale = info.width as f64 / rect.width;
        let scale = if scale.is_finite() && scale > 0. {
            scale
        } else {
            1.
        };
        let mut selection = Selection::new(vec![Output { rect, scale }], vec![], Some(rect))
            .ok_or_else(|| t("Invalid monitor layout"))?;
        selection.selected = true;
        let inner = Rc::new(Inner {
            hosts,
            owner,
            theme: RefCell::new(theme),
            source: rect,
            info: info.clone(),
            edit: RefCell::new(PlaybackEdit::new(&info)),
            selection: RefCell::new(selection),
            canvas,
            texture: RefCell::new(None),
            bar,
            upper,
            export_percent,
            notice_timer: RefCell::new(None),
            scroller,
            controls,
            time,
            play,
            play_icons,
            reset: reset.clone(),
            speed_label,
            format_label,
            format: Cell::new(Format::Mp4),
            timeline,
            message,
            notice_dismiss: notice_dismiss.clone(),
            progress,
            cancel,
            popup,
            position: Rc::new(ToolbarPosition::default()),
            toolbar: Cell::new(rect),
            style: RefCell::new(style),
            suspended: Cell::new(false),
            exporting: Cell::new(false),
            emit,
            clock: RefCell::new(None),
        });
        weak_holder.replace(Rc::downgrade(&inner));
        for (b, cmd) in [
            (&inner.play, Command::Toggle),
            (&save, Command::Save(false)),
            (&quick, Command::Save(true)),
            (&copy, Command::Copy),
            (&inner.cancel, Command::CancelExport),
        ] {
            let weak = Rc::downgrade(&inner);
            b.connect_clicked(move |_| {
                if let Some(i) = weak.upgrade() {
                    (i.emit)(cmd.clone());
                }
            });
        }
        for (b, cmd) in [(&restart, Command::Restart), (&close, Command::Close)] {
            let weak = Rc::downgrade(&inner);
            b.connect_clicked(move |b| {
                if let Some(i) = weak.upgrade() {
                    i.confirm(b, cmd.clone());
                }
            });
        }
        let weak = Rc::downgrade(&inner);
        reset.connect_clicked(move |_| {
            if let Some(i) = weak.upgrade() {
                (i.emit)(Command::Edit(PlaybackEdit::new(&i.info)));
            }
        });
        let weak = Rc::downgrade(&inner);
        notice_dismiss.connect_clicked(move |_| {
            if let Some(i) = weak.upgrade() {
                i.show_notice("", false);
            }
        });
        let weak = Rc::downgrade(&inner);
        speed.connect_clicked(move |b| {
            if let Some(i) = weak.upgrade() {
                i.speed_choices(b);
            }
        });
        let weak = Rc::downgrade(&inner);
        format.connect_clicked(move |b| {
            if let Some(i) = weak.upgrade() {
                i.format_choices(b);
            }
        });
        let weak = Rc::downgrade(&inner);
        sound.connect_clicked(move |b| {
            if let Some(i) = weak.upgrade() {
                i.audio_choices(b);
            }
        });
        let weak = Rc::downgrade(&inner);
        inner.canvas.set_painter(move |s| {
            if let Some(i) = weak.upgrade() {
                i.paint(s);
            }
        });
        let weak = Rc::downgrade(&inner);
        let changed = Rc::downgrade(&inner);
        inner.position.install(
            &grip,
            move || weak.upgrade().map(|i| i.toolbar.get()),
            move |_| {
                if let Some(i) = changed.upgrade() {
                    i.layout();
                }
            },
        );
        let weak = Rc::downgrade(&inner);
        inner.popup.connect_changed(move || {
            if let Some(i) = weak.upgrade() {
                i.input();
            }
        });
        inner.install_crop();
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&inner);
        keys.connect_key_pressed(move |_, key, _, mods| {
            let Some(i) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if i.popup.is_open() || i.exporting.get() {
                return glib::Propagation::Proceed;
            }
            let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
            let command = match key {
                gdk::Key::space => Some(Command::Toggle),
                gdk::Key::s | gdk::Key::S if ctrl => Some(Command::Save(shift)),
                gdk::Key::c | gdk::Key::C if ctrl => Some(Command::Copy),
                gdk::Key::Escape => {
                    i.confirm(&close, Command::Close);
                    return glib::Propagation::Stop;
                }
                _ => None,
            };
            if let Some(cmd) = command {
                (i.emit)(cmd);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        inner.surface().root.add_controller(keys);
        inner.layout();
        inner.surface().window.set_title(Some("Clivuu playback"));
        inner.surface().set_keyboard(true, true)?;
        inner.surface().root.set_focusable(true);
        inner.surface().root.grab_focus();
        if let Some(clock) = inner.surface().window.frame_clock() {
            let weak = Rc::downgrade(&inner);
            let id = clock.connect_after_paint(move |_| {
                if let Some(i) = weak.upgrade() {
                    i.input();
                }
            });
            inner.clock.replace(Some((clock, id)));
        }
        Ok(Self(inner))
    }
    pub fn texture(&self, texture: gdk::Texture) {
        self.0.texture.replace(Some(texture));
        self.0.canvas.queue_draw();
    }
    pub fn format(&self) -> Format {
        self.0.format.get()
    }
    pub fn set_format(&self, format: Format) {
        self.0.format.set(format);
        select_choice(&self.0.format_label, format.label());
    }
    pub fn edit(&self, edit: &PlaybackEdit) {
        self.0.edit.replace(edit.clone());
        let s = self.0.source;
        let crop = edit.crop;
        self.0.selection.borrow_mut().rect = Bounds {
            x: s.x + crop.x as f64 / self.0.info.width as f64 * s.width,
            y: s.y + crop.y as f64 / self.0.info.height as f64 * s.height,
            width: crop.width as f64 / self.0.info.width as f64 * s.width,
            height: crop.height as f64 / self.0.info.height as f64 * s.height,
        };
        select_choice(&self.0.speed_label, &rate_label(edit.rate));
        self.0
            .reset
            .set_sensitive(edit != &PlaybackEdit::new(&self.0.info));
        self.0.canvas.queue_draw();
    }
    pub fn update(&self, position: u64, playing: bool, progress: Option<f64>) {
        let i = &self.0;
        let edit = i.edit.borrow();
        i.time.set_label(&format!(
            "{} / {}",
            recording::elapsed_label(Duration::from_nanos(position.saturating_sub(edit.start_ns))),
            recording::elapsed_label(Duration::from_nanos(edit.end_ns - edit.start_ns))
        ));
        i.play_icons
            .set_visible_child_name(if playing { "pause" } else { "play" });
        i.timeline.update(&edit, position);
        let exporting = progress.is_some();
        let changed = i.exporting.replace(exporting) != exporting;
        i.controls.set_sensitive(!exporting);
        i.timeline.widget().set_sensitive(!exporting);
        i.upper.set_visible_child_name(if exporting {
            "export"
        } else if i.message.is_visible() {
            "notice"
        } else {
            "timeline"
        });
        if let Some(p) = progress {
            i.progress.set_fraction(p.clamp(0., 1.));
            i.export_percent
                .set_label(&format!("{:.0}%", p.clamp(0., 1.) * 100.));
        }
        if changed {
            i.canvas.queue_draw();
            i.layout();
        }
    }
    pub fn message(&self, message: &str) {
        self.0.show_notice(message, false);
    }
    pub fn error(&self, message: &str) {
        self.0.show_notice(message, true);
    }
    pub fn set_appearance(&self, appearance: super::recording::Appearance) {
        let i = &self.0;
        if appearance.dark {
            i.bar.add_css_class("clivuu-theme-dark");
        } else {
            i.bar.remove_css_class("clivuu-theme-dark");
        }
        i.theme
            .borrow_mut()
            .update(appearance.dark, &appearance.accent);
        let accent = i.theme.borrow().accent();
        let mut style = appearance.display;
        style.accent = accent;
        *i.style.borrow_mut() = style;
        i.timeline.set_accent(accent);
        i.bar.queue_draw();
        i.canvas.queue_draw();
        i.surface().root.queue_draw();
        i.input();
    }
    pub fn suspend(&self, hidden: bool) {
        let was = self.0.suspended.replace(hidden);
        self.0.popup.close();
        self.0.layout();
        if was && !hidden {
            if let Err(error) = self.0.surface().set_keyboard(true, true) {
                eprintln!("{error}");
            }
            self.0.surface().root.grab_focus();
        }
    }
    pub fn status(&self) -> serde_json::Value {
        let root = &self.0.surface().root;
        fn buttons(
            w: &gtk::Widget,
            root: &gtk::Widget,
            origin: Bounds,
            out: &mut Vec<serde_json::Value>,
        ) {
            if let Some(b) = w.downcast_ref::<gtk::Button>()
                && b.is_mapped()
                && let Some(r) = b.compute_bounds(root)
            {
                out.push(serde_json::json!({"name":b.widget_name().to_string(),"label":super::floating_bar::button_text(b),"enabled":b.is_sensitive(),"rect":Bounds{x:r.x() as f64+origin.x,y:r.y() as f64+origin.y,width:r.width() as f64,height:r.height() as f64}}));
            }
            if (w.is::<gtk::Scale>() || w.is::<gtk::CheckButton>())
                && w.is_mapped()
                && let Some(r) = w.compute_bounds(root)
            {
                out.push(serde_json::json!({"name":w.widget_name().to_string(),"label":w.downcast_ref::<gtk::CheckButton>().and_then(|b|b.label()).map(|s|s.to_string()),"value":w.downcast_ref::<gtk::Scale>().map(|s|s.value()),"enabled":w.is_sensitive(),"rect":Bounds{x:r.x() as f64+origin.x,y:r.y() as f64+origin.y,width:r.width() as f64,height:r.height() as f64}}));
            }
            let mut c = w.first_child();
            while let Some(child) = c {
                buttons(&child, root, origin, out);
                c = child.next_sibling();
            }
        }
        let mut controls = vec![];
        buttons(
            root.upcast_ref(),
            root.upcast_ref(),
            self.0.surface().rect,
            &mut controls,
        );
        let timeline = self
            .0
            .timeline
            .widget()
            .compute_bounds(root)
            .map(|r| Bounds {
                x: r.x() as f64 + self.0.surface().rect.x,
                y: r.y() as f64 + self.0.surface().rect.y,
                width: r.width() as f64,
                height: r.height() as f64,
            });
        let popup_bounds = self.0.popup.bounds().map(|p| Bounds {
            x: self.0.surface().rect.x + p.x() as f64,
            y: self.0.surface().rect.y + p.y() as f64,
            width: p.width() as f64,
            height: p.height() as f64,
        });
        serde_json::json!({"buttons":controls,"popupBounds":popup_bounds,"positionNs":self.0.timeline.position(),"message":self.0.message.label().to_string(),"playing":self.0.play_icons.visible_child_name().as_deref()==Some("pause"),"timeline":timeline,"phase":if self.0.exporting.get(){"exporting"}else{"playback"},"rect":self.0.selection.borrow().rect,"toolbar":self.0.toolbar.get(),"format":self.format().extension(),"edit":*self.0.edit.borrow(),"material":self.0.info,"visible":!self.0.suspended.get()})
    }
}
impl Inner {
    fn resize_margin(&self) -> f64 {
        capture_draw::resize_margin(&self.style.borrow(), 7.)
    }
    fn surface(&self) -> &Rc<crate::platform::surfaces::pins::Surface> {
        &self.hosts.surfaces[self.owner]
    }
    fn layout(&self) {
        if self.suspended.get() {
            for s in &self.hosts.surfaces {
                s.set_input(vec![]);
                s.window.set_visible(false);
            }
            return;
        }
        let surface = self.surface();
        self.scroller
            .set_max_content_width((surface.rect.width as i32 - 24).clamp(1, 1000));
        let width = self.bar.measure(gtk::Orientation::Horizontal, -1).1 as f64;
        let height = self.bar.measure(gtk::Orientation::Vertical, width as i32).1 as f64;
        let panels = layout::selection(
            self.source,
            surface.rect,
            layout::Size { width, height },
            layout::Size {
                width: 0.,
                height: 0.,
            },
        );
        let r = self.position.place(panels.toolbar, surface.rect);
        self.toolbar.set(r);
        surface
            .root
            .move_(&self.bar, r.x - surface.rect.x, r.y - surface.rect.y);
        surface
            .root
            .set_child_size(&self.bar, Some((r.width as i32, r.height as i32)));
        self.popup.set_viewport(Bounds {
            x: 0.,
            y: 0.,
            width: surface.rect.width,
            height: surface.rect.height,
        });
        self.popup.reposition();
        self.input();
        surface.window.set_visible(true);
    }
    fn input(&self) {
        let surface = self.surface();
        let mut areas = vec![];
        if !self.suspended.get() {
            let s = self.source;
            let m = self.resize_margin();
            areas.push(Bounds {
                x: s.x - m,
                y: s.y - m,
                width: s.width + 2. * m,
                height: s.height + 2. * m,
            });
            areas.push(self.toolbar.get());
            if let Some(p) = self.popup.bounds() {
                areas.push(Bounds {
                    x: surface.rect.x + p.x() as f64,
                    y: surface.rect.y + p.y() as f64,
                    width: p.width() as f64,
                    height: p.height() as f64,
                });
            }
        }
        surface.set_input(areas);
    }
    fn paint(&self, s: &gtk::Snapshot) {
        if self.suspended.get() {
            return;
        }
        let origin = self.surface().rect;
        let local = |r: Bounds| Bounds {
            x: r.x - origin.x,
            y: r.y - origin.y,
            ..r
        };
        let bounds = |r: Bounds| {
            graphene::Rect::new(r.x as f32, r.y as f32, r.width as f32, r.height as f32)
        };
        if let Some(texture) = self.texture.borrow().as_ref() {
            s.append_texture(texture, &bounds(local(self.source)));
        }
        let rect = self.selection.borrow().rect;
        for r in clivuu_core::capture::outside(rect, self.source) {
            s.append_color(&gdk::RGBA::new(0., 0., 0., 0.6), &bounds(local(r)));
        }
        let mut style = self.style.borrow().clone();
        style.anchors &= !self.exporting.get();
        capture_draw::frame(s, local(rect), &style);
    }
    fn show_notice(self: &Rc<Self>, message: &str, persistent: bool) {
        if let Some(timer) = self.notice_timer.borrow_mut().take() {
            timer.remove();
        }
        self.message.set_label(message);
        self.message
            .set_tooltip_text((!message.is_empty()).then_some(message));
        self.message.set_visible(!message.is_empty());
        self.notice_dismiss
            .set_visible(persistent && !message.is_empty());
        if persistent && !message.is_empty() {
            self.message.add_css_class("error");
        } else {
            self.message.remove_css_class("error");
        }
        if !self.exporting.get() {
            self.upper.set_visible_child_name(if message.is_empty() {
                "timeline"
            } else {
                "notice"
            });
        }
        if !message.is_empty() && !persistent {
            let weak = Rc::downgrade(self);
            let timer = glib::timeout_add_local_once(Duration::from_secs(4), move || {
                if let Some(i) = weak.upgrade() {
                    i.notice_timer.borrow_mut().take();
                    i.message.set_visible(false);
                    i.notice_dismiss.set_visible(false);
                    i.message.remove_css_class("error");
                    if !i.exporting.get() {
                        i.upper.set_visible_child_name("timeline");
                    }
                }
            });
            self.notice_timer.replace(Some(timer));
        }
    }
    fn confirm(self: &Rc<Self>, anchor: &gtk::Button, cmd: Command) {
        let weak = Rc::downgrade(self);
        super::floating_bar::confirm_popup(
            &self.popup,
            anchor,
            "Discard this recording and its playback edits?",
            "Discard recording",
            true,
            move || {
                if let Some(i) = weak.upgrade() {
                    (i.emit)(cmd.clone());
                }
            },
        );
        self.input();
    }
    fn speed_choices(self: &Rc<Self>, anchor: &gtk::Button) {
        let active_rate = self.edit.borrow().rate;
        let list = super::floating_bar::choice_list();
        for rate in recording::PLAYBACK_RATES {
            let b = super::floating_bar::choice_item(
                &rate_label(rate),
                (active_rate - rate).abs() < 1e-6,
            );
            let weak = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(i) = weak.upgrade() {
                    let mut edit = i.edit.borrow().clone();
                    edit.rate = rate;
                    i.popup.close();
                    (i.emit)(Command::Edit(edit));
                }
            });
            list.append(&b);
        }
        self.popup.show(anchor, &list, || {});
        self.input();
    }
    fn format_choices(self: &Rc<Self>, anchor: &gtk::Button) {
        let active_format = self.format.get();
        let list = super::floating_bar::choice_list();
        for format in [Format::Mp4, Format::Webm, Format::Gif] {
            let b = super::floating_bar::choice_item(format.label(), active_format == format);
            let weak = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(i) = weak.upgrade() {
                    i.format.set(format);
                    select_choice(&i.format_label, format.label());
                    i.popup.close();
                    i.layout();
                }
            });
            list.append(&b);
        }
        self.popup.show(anchor, &list, || {});
        self.input();
    }
    fn audio_choices(self: &Rc<Self>, anchor: &gtk::Button) {
        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .css_classes(["clivuu-audio-popup"])
            .build();
        let label_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
        let readout_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
        for role in [AudioRole::System, AudioRole::Microphone]
            .into_iter()
            .filter(|r| self.info.tracks.contains(r))
        {
            let gain = self.edit.borrow().audio[role.index()];
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(4)
                .valign(gtk::Align::Center)
                .css_classes(["clivuu-audio-row"])
                .build();
            let icons = gtk::Stack::new();
            let (active_icon, muted_icon) = if role == AudioRole::System {
                ("record-speaker", "record-speaker-muted")
            } else {
                ("record-microphone", "record-microphone-muted")
            };
            icons.add_named(&super::icons::icon_sized(active_icon, 14), Some("active"));
            icons.add_named(&super::icons::icon_sized(muted_icon, 14), Some("muted"));
            icons.set_visible_child_name(if gain.muted { "muted" } else { "active" });

            let mute = gtk::ToggleButton::builder()
                .child(&icons)
                .active(gain.muted)
                .focusable(false)
                .valign(gtk::Align::Center)
                .tooltip_text(t("Mute"))
                .css_classes(["clivuu-audio-mute"])
                .build();
            mute.set_widget_name(if role == AudioRole::System {
                "playback-system-mute"
            } else {
                "playback-microphone-mute"
            });

            let title = gtk::Label::builder()
                .label(t(role.label()))
                .xalign(0.)
                .valign(gtk::Align::Center)
                .single_line_mode(true)
                .css_classes(["clivuu-audio-title"])
                .build();
            if gain.muted {
                title.add_css_class("clivuu-audio-muted");
            }
            label_group.add_widget(&title);
            row.append(&mute);
            row.append(&title);

            let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0., 200., 1.);
            scale.set_widget_name(if role == AudioRole::System {
                "playback-system-volume"
            } else {
                "playback-microphone-volume"
            });
            scale.set_width_request(78);
            scale.set_valign(gtk::Align::Center);
            scale.set_value(gain.volume * 100.);
            scale.set_draw_value(false);
            if gain.muted {
                scale.add_css_class("clivuu-audio-muted");
            }

            let percent_label = gtk::Label::builder()
                .label(format!("{:.0}%", gain.volume * 100.))
                .xalign(1.)
                .valign(gtk::Align::Center)
                .single_line_mode(true)
                .css_classes(["numeric"])
                .build();
            let reset_gain = gtk::Button::builder()
                .child(&percent_label)
                .focusable(false)
                .valign(gtk::Align::Center)
                .tooltip_text(t("Reset volume"))
                .css_classes(["clivuu-audio-reset"])
                .build();
            reset_gain.set_sensitive(((gain.volume * 100.).round() - 100.).abs() >= 0.5);
            readout_group.add_widget(&reset_gain);
            let s_reset = scale.clone();
            reset_gain.connect_clicked(move |_| {
                s_reset.set_value(100.);
            });
            let weak = Rc::downgrade(self);
            let readout = percent_label.clone();
            let reset_btn = reset_gain.clone();
            scale.connect_value_changed(move |s| {
                let value = s.value();
                readout.set_label(&format!("{value:.0}%"));
                reset_btn.set_sensitive((value.round() - 100.).abs() >= 0.5);
                if let Some(i) = weak.upgrade() {
                    let mut edit = i.edit.borrow().clone();
                    edit.audio[role.index()].volume = value / 100.;
                    (i.emit)(Command::Edit(edit));
                }
            });
            row.append(&scale);
            row.append(&reset_gain);

            let weak = Rc::downgrade(self);
            let s_mute = scale.clone();
            let t_mute = title.clone();
            let i_mute = icons.clone();
            mute.connect_toggled(move |b| {
                let muted = b.is_active();
                i_mute.set_visible_child_name(if muted { "muted" } else { "active" });
                if muted {
                    s_mute.add_css_class("clivuu-audio-muted");
                    t_mute.add_css_class("clivuu-audio-muted");
                } else {
                    s_mute.remove_css_class("clivuu-audio-muted");
                    t_mute.remove_css_class("clivuu-audio-muted");
                }
                if let Some(i) = weak.upgrade() {
                    let mut edit = i.edit.borrow().clone();
                    edit.audio[role.index()].muted = muted;
                    (i.emit)(Command::Edit(edit));
                }
            });
            list.append(&row);
        }
        self.popup.show(anchor, &list, || {});
        self.input();
    }
    fn install_crop(self: &Rc<Self>) {
        let drag = gtk::GestureDrag::builder().button(1).build();
        let initial = Rc::new(Cell::new(None));
        let weak = Rc::downgrade(self);
        let saved = initial.clone();
        drag.connect_drag_begin(move |g, x, y| {
            if let Some(i) = weak.upgrade() {
                let origin = i.surface().rect;
                let p = [x + origin.x, y + origin.y];
                if i.exporting.get() || i.toolbar.get().contains(p, 0.) || i.popup.is_open() {
                    g.set_state(gtk::EventSequenceState::Denied);
                    return;
                }
                let cursor = i.selection.borrow().cursor(p, i.resize_margin());
                if cursor == "move" || cursor == "default" || cursor == "crosshair" {
                    (i.emit)(Command::Toggle);
                    g.set_state(gtk::EventSequenceState::Denied);
                    return;
                }
                i.selection.borrow_mut().begin(p, i.resize_margin(), false);
                saved.set(Some(p));
                g.set_state(gtk::EventSequenceState::Claimed);
            }
        });
        for end in [false, true] {
            let weak = Rc::downgrade(self);
            let saved = initial.clone();
            let change = move |_: &gtk::GestureDrag, dx: f64, dy: f64| {
                if let Some(i) = weak.upgrade()
                    && let Some(p) = saved.get()
                {
                    let r = {
                        let mut s = i.selection.borrow_mut();
                        s.motion([p[0] + dx, p[1] + dy]);
                        if end {
                            s.end();
                            saved.set(None);
                        }
                        s.rect
                    };
                    let mut edit = i.edit.borrow().clone();
                    let src = i.source;
                    let x = ((r.x - src.x) / src.width * i.info.width as f64)
                        .round()
                        .clamp(0., (i.info.width - 1) as f64) as u32;
                    let y = ((r.y - src.y) / src.height * i.info.height as f64)
                        .round()
                        .clamp(0., (i.info.height - 1) as f64) as u32;
                    let right = ((r.x + r.width - src.x) / src.width * i.info.width as f64)
                        .round()
                        .clamp((x + 1) as f64, i.info.width as f64)
                        as u32;
                    let bottom = ((r.y + r.height - src.y) / src.height * i.info.height as f64)
                        .round()
                        .clamp((y + 1) as f64, i.info.height as f64)
                        as u32;
                    edit.crop = recording::Crop {
                        x,
                        y,
                        width: right - x,
                        height: bottom - y,
                    };
                    (i.emit)(Command::Edit(edit));
                }
            };
            if end {
                drag.connect_drag_end(change);
            } else {
                drag.connect_drag_update(change);
            }
        }
        let weak = Rc::downgrade(self);
        drag.connect_cancel(move |_, _| {
            initial.set(None);
            if let Some(i) = weak.upgrade() {
                i.selection.borrow_mut().end();
            }
        });
        self.canvas.add_controller(drag);
        let motion = gtk::EventControllerMotion::new();
        let weak = Rc::downgrade(self);
        motion.connect_motion(move |_, x, y| {
            if let Some(i) = weak.upgrade() {
                let r = i.surface().rect;
                let p = [x + r.x, y + r.y];
                let cursor = i.selection.borrow().cursor(p, i.resize_margin());
                i.canvas
                    .set_cursor_from_name(Some(if cursor == "move" || i.exporting.get() {
                        "default"
                    } else {
                        cursor
                    }));
            }
        });
        self.canvas.add_controller(motion);
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(timer) = self.notice_timer.get_mut().take() {
            timer.remove();
        }
        if let Some((clock, id)) = self.clock.take() {
            clock.disconnect(id);
        }
    }
}
