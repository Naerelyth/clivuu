//! Live recording presentation on platform-owned surfaces. This module only
//! publishes commands, recording and file ownership stay in the application.
mod interaction;
mod presentation;
use crate::{
    editor::{capture_view::Canvas, layout},
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
    i18n::{t, tf},
    pins::overlaps,
    recording::{self, AudioAvailability, Options, Phase},
};
use gtk::{gdk, gio, glib, graphene, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

#[derive(Clone, Copy)]
pub enum Command {
    Start,
    Pause,
    Stop,
    Restart,
    Discard,
}
#[derive(Clone)]
pub struct Appearance {
    pub dark: bool,
    pub accent: String,
    pub display: crate::editor::capture_draw::CaptureStyle,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            dark: true,
            accent: "#bd93f9".into(),
            display: crate::editor::capture_draw::CaptureStyle::default(),
        }
    }
}
struct Inner {
    hosts: Hosts,
    theme: RefCell<Theme>,
    owner: usize,
    selection: RefCell<Selection>,
    phase: Cell<Phase>,
    tooltips: super::localization::Tooltips,
    options: Cell<Options>,
    audio: AudioAvailability,
    panel: gtk::Box,
    scroller: gtk::ScrolledWindow,
    time: gtk::Label,
    hint: gtk::Label,
    start: gtk::Button,
    start_icons: gtk::Stack,
    pause_icons: gtk::Stack,
    pause: gtk::Button,
    restart: gtk::Button,
    close: gtk::Button,
    fps: gtk::Button,
    fps_label: gtk::Label,
    mode: gtk::Button,
    mode_label: gtk::Label,
    delay: gtk::Button,
    delay_label: gtk::Label,
    cursor: gtk::Button,
    system_audio: gtk::Button,
    microphone: gtk::Button,
    move_region: gtk::Button,
    frames: Vec<Canvas>,
    popup: PopupLayer,
    position: Rc<ToolbarPosition>,
    toolbar: Cell<Bounds>,
    hint_rect: Cell<Bounds>,
    feedback: presentation::Feedback,
    emit: Rc<dyn Fn(Command)>,
    clock: RefCell<Option<(gdk::FrameClock, glib::SignalHandlerId)>>,
}
#[derive(Clone)]
pub struct Controls(Rc<Inner>);
fn button(
    tooltips: &super::localization::Tooltips,
    title: &str,
    icon: &'static str,
) -> gtk::Button {
    let b = super::floating_bar::icon_button(icon);
    tooltips.bind(&b, title, "");
    b
}
fn choice(
    tooltips: &super::localization::Tooltips,
    title: &str,
    text: &str,
) -> (gtk::Button, gtk::Label) {
    let (button, label) = super::floating_bar::choice_button(text);
    tooltips.bind(&button, title, "");
    (button, label)
}
// Measured GSK bounds of the shared CSS shadow (0 3px 8px), including
// fractional alpha pixels. Regression checks compare the actual GTK node.
const SHADOW_X: f64 = 11.;
const SHADOW_TOP: f64 = 8.;
const SHADOW_BOTTOM: f64 = 14.;
fn paint_bounds(r: Bounds) -> Bounds {
    Bounds {
        x: r.x - SHADOW_X,
        y: r.y - SHADOW_TOP,
        width: r.width + SHADOW_X * 2.,
        height: r.height + SHADOW_TOP + SHADOW_BOTTOM,
    }
}
// Lateral placement is only a fallback when the shared capture layout would
// put controls inside the crop, never a competing preference for edge regions.
fn recording_toolbar(panels: &layout::Panels, rect: Bounds, output: Bounds) -> Bounds {
    let toolbar = panels.toolbar;
    if !matches!(
        panels.placement,
        layout::Placement::InsideTop | layout::Placement::InsideBottom
    ) {
        return toolbar;
    }
    let y = (rect.y + (rect.height - toolbar.height) / 2.).clamp(
        output.y + SHADOW_TOP,
        (output.y + output.height - toolbar.height - SHADOW_BOTTOM).max(output.y + SHADOW_TOP),
    );
    for x in [
        rect.x + rect.width + SHADOW_X,
        rect.x - toolbar.width - SHADOW_X,
    ] {
        let candidate = Bounds { x, y, ..toolbar };
        let painted = paint_bounds(candidate);
        if painted.x >= output.x && painted.x + painted.width <= output.x + output.width {
            return candidate;
        }
    }
    toolbar
}
impl Controls {
    pub fn new(
        display: &gdk::Display,
        rect: Bounds,
        appearance: Appearance,
        options: Options,
        audio: AudioAvailability,
        lost: impl Fn() + 'static,
        emit: impl Fn(Command) + 'static,
    ) -> Result<Self, String> {
        let Appearance {
            dark,
            accent,
            display: style,
        } = appearance;
        let hosts = Hosts::new(display, PinRole::Controls, lost)?;
        let owner = hosts
            .surfaces
            .iter()
            .position(|s| s.rect.contains(rect.center(), 0.))
            .unwrap_or(0);
        // A region belongs to one authorized monitor, constrain ready-state
        // resize/movement to that same monitor before acquiring a stream.
        const EPS: f64 = 1e-9;
        let viewport = hosts.surfaces[owner].rect;
        if rect.x < viewport.x - EPS
            || rect.y < viewport.y - EPS
            || rect.x + rect.width > viewport.x + viewport.width + EPS
            || rect.y + rect.height > viewport.y + viewport.height + EPS
        {
            return Err(t(
                "Select the monitor containing the entire recording region",
            ));
        }
        let mut selection = Selection::new(
            vec![Output {
                rect: hosts.surfaces[owner].rect,
                scale: hosts.surfaces[owner].scale,
            }],
            vec![],
            Some(rect),
        )
        .ok_or_else(|| t("Invalid monitor layout"))?;
        selection.selected = true;
        let theme = Toolbar::install_theme(display, dark, &accent);
        let panel = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build();
        panel.add_css_class("clivuu-capture-toolbar");
        if dark {
            panel.add_css_class("clivuu-theme-dark");
        }
        panel.set_size_request(-1, super::floating_bar::TOOLBAR_HEIGHT);
        let (scroller, row) = super::floating_bar::scrollable_bar_row();
        panel.append(&scroller);
        let tooltips = super::localization::Tooltips::default();
        let grip = Toolbar::grip_button();
        tooltips.bind(&grip, "Move toolbar", "");
        row.append(&grip);
        let time = super::floating_bar::metric_label("00:00", 6, 8);
        row.append(&time);
        row.append(&super::floating_bar::tool_separator());
        let (fps, fps_label) = choice(&tooltips, "Frame rate", "24 FPS");
        row.append(&fps);
        let (mode, mode_label) = super::floating_bar::text_button(&t(options.mode.label()));
        tooltips.bind(&mode, "Recording mode", "");
        row.append(&mode);
        let (delay, delay_label) = choice(&tooltips, "Recording delay", &t("No delay"));
        row.append(&delay);
        row.append(&super::floating_bar::tool_separator());
        let cursor = button(&tooltips, "Show mouse cursor", "record-cursor");
        let system_audio = button(&tooltips, "Record system audio", "record-speaker");
        let microphone = button(&tooltips, "Record microphone audio", "record-microphone");
        for b in [&cursor, &system_audio, &microphone] {
            row.append(b);
        }
        row.append(&super::floating_bar::tool_separator());
        let start = button(&tooltips, "Start recording", "record-start");
        let start_icons = presentation::icons(&["record-start", "record-stop"]);
        start.set_child(Some(&start_icons));
        start.add_css_class("clivuu-record-button");
        row.append(&start);
        let pause = button(&tooltips, "Pause recording", "pause");
        let pause_icons = presentation::icons(&["pause", "play"]);
        pause.set_child(Some(&pause_icons));
        row.append(&pause);
        row.append(&super::floating_bar::tool_separator());
        let move_region = button(&tooltips, "Move recording region", "move-region");
        row.append(&move_region);
        let restart = button(&tooltips, "Restart recording", "restart-recording");
        row.append(&restart);
        let close = button(&tooltips, "Discard recording", "close");
        row.append(&close);
        let hint = gtk::Label::new(None);
        hint.add_css_class("clivuu-capture-hint");
        hint.set_can_target(false);
        let surface = &hosts.surfaces[owner];
        let popup = PopupLayer::new(&surface.root);
        let frames = hosts
            .surfaces
            .iter()
            .map(|s| {
                let frame = Canvas::new(|_| {});
                s.root.put(&frame, 0., 0.);
                s.root
                    .set_child_size(&frame, Some((s.rect.width as i32, s.rect.height as i32)));
                frame
            })
            .collect();
        surface.root.put(&panel, 0., 0.);
        surface.root.put(&hint, 0., 0.);
        let theme_accent = theme.accent();
        let inner = Rc::new(Inner {
            hosts,
            theme: RefCell::new(theme),
            owner,
            selection: RefCell::new(selection),
            tooltips,
            phase: Cell::new(Phase::Ready),
            options: Cell::new(options),
            audio,
            panel,
            scroller,
            time,
            hint,
            start,
            start_icons,
            pause_icons,
            pause,
            restart,
            close,
            fps,
            fps_label,
            mode,
            mode_label,
            delay,
            delay_label,
            cursor,
            system_audio,
            microphone,
            move_region,
            frames,
            popup,
            position: Rc::new(ToolbarPosition::default()),
            toolbar: Cell::new(rect),
            hint_rect: Cell::new(rect),
            feedback: presentation::Feedback::new(theme_accent, style),
            emit: Rc::new(emit),
            clock: RefCell::new(None),
        });
        for (n, b) in [&inner.cursor, &inner.system_audio, &inner.microphone]
            .into_iter()
            .enumerate()
        {
            let weak = Rc::downgrade(&inner);
            b.connect_clicked(move |_| {
                if let Some(i) = weak.upgrade().filter(|i| i.phase.get().editable()) {
                    let mut options = i.options.get();
                    match n {
                        0 => options.cursor = !options.cursor,
                        1 if i.audio.system => options.system_audio = !options.system_audio,
                        2 if i.audio.microphone => options.microphone = !options.microphone,
                        _ => return,
                    }
                    i.options.set(options);
                    i.layout();
                }
            });
        }
        for (b, cmd) in [
            (&inner.start, Command::Start),
            (&inner.pause, Command::Pause),
            (&inner.restart, Command::Restart),
            (&inner.close, Command::Discard),
        ] {
            let weak = Rc::downgrade(&inner);
            b.connect_clicked(move |anchor| {
                if let Some(i) = weak.upgrade() {
                    let cmd = if matches!(cmd, Command::Start) && i.phase.get() != Phase::Ready {
                        Command::Stop
                    } else {
                        cmd
                    };
                    if matches!(cmd, Command::Discard | Command::Restart)
                        && i.phase.get() != Phase::Ready
                    {
                        i.confirm(anchor, cmd);
                    } else {
                        (i.emit)(cmd);
                    }
                }
            });
        }
        let weak = Rc::downgrade(&inner);
        inner.mode.connect_clicked(move |_| {
            if let Some(i) = weak.upgrade().filter(|i| i.phase.get().editable()) {
                let mut o = i.options.get();
                o.mode = match o.mode {
                    recording::Mode::Normal => recording::Mode::Quick,
                    recording::Mode::Quick => recording::Mode::Normal,
                };
                i.options.set(o);
                i.layout();
            }
        });
        for (b, is_fps) in [(&inner.fps, true), (&inner.delay, false)] {
            let weak = Rc::downgrade(&inner);
            b.connect_clicked(move |anchor| {
                if let Some(i) = weak.upgrade() {
                    i.choices(anchor, is_fps)
                }
            });
        }
        let weak = Rc::downgrade(&inner);
        inner.popup.connect_changed(move || {
            if let Some(i) = weak.upgrade() {
                i.input()
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
        for (n, s) in inner.hosts.surfaces.iter().enumerate() {
            let weak = Rc::downgrade(&inner);
            let origin = s.rect;
            inner.frames[n].set_painter(move |snapshot| {
                if let Some(i) = weak.upgrade() {
                    let r = i.selection.borrow().rect;
                    i.feedback.paint(
                        snapshot,
                        Bounds {
                            x: r.x - origin.x,
                            y: r.y - origin.y,
                            ..r
                        },
                        i.phase.get().editable(),
                    );
                }
            });
            s.window.set_title(Some("Clivuu recording"));
        }
        inner.install_region_drag(&inner.move_region, true);
        // Frame resizing belongs to its own hit target. Attaching a drag to
        // the root also observes toolbar presses and competes with its grip.
        inner.frames[owner].set_can_target(true);
        inner.install_region_drag(&inner.frames[owner], false);
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&inner);
        keys.connect_key_pressed(move |_, key, _, _| {
            if let Some(i) = weak.upgrade() {
                if i.popup.is_open() {
                    return glib::Propagation::Proceed;
                }
                if key == gdk::Key::Escape {
                    if i.phase.get() == Phase::Ready {
                        (i.emit)(Command::Discard);
                    } else {
                        i.confirm(&i.close, Command::Discard);
                    }
                    return glib::Propagation::Stop;
                }
                if key == gdk::Key::Return && i.phase.get() == Phase::Ready {
                    (i.emit)(Command::Start);
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        inner.surface().root.add_controller(keys);
        let motion = gtk::EventControllerMotion::builder()
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = Rc::downgrade(&inner);
        let track = move |_: &gtk::EventControllerMotion, x: f64, y: f64| {
            if let Some(i) = weak.upgrade() {
                let r = i.surface().rect;
                i.update_cursor([x + r.x, y + r.y]);
            }
        };
        motion.connect_enter(track.clone());
        motion.connect_motion(track);
        inner.surface().root.add_controller(motion);

        // Submit the intended keyboard mode and input region on the first map,
        // rather than initially exposing an empty-input native surface.
        inner.layout();
        for surface in &inner.hosts.surfaces {
            surface.window.set_visible(true);
        }
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
    pub fn focus_ready(&self) -> Result<(), String> {
        let surface = self.0.surface();
        surface.set_keyboard(true, true)?;
        surface.root.set_focusable(true);
        surface.root.grab_focus();
        Ok(())
    }
    pub fn rect(&self) -> Bounds {
        self.0.selection.borrow().rect
    }
    pub fn options(&self) -> Options {
        self.0.effective_options()
    }
    pub fn status(&self) -> serde_json::Value {
        let i = &self.0;
        fn buttons(
            widget: &gtk::Widget,
            root: &gtk::Widget,
            origin: Bounds,
            out: &mut Vec<serde_json::Value>,
        ) {
            if let Some(button) = widget.downcast_ref::<gtk::Button>()
                && let Some(r) = button.compute_bounds(root)
            {
                out.push(serde_json::json!({"label":super::floating_bar::button_text(button),"enabled":button.is_sensitive(),"rect":Bounds{x:r.x() as f64+origin.x,y:r.y() as f64+origin.y,width:r.width() as f64,height:r.height() as f64}}));
            }
            let mut child = widget.first_child();
            while let Some(w) = child {
                buttons(&w, root, origin, out);
                child = w.next_sibling();
            }
        }
        let mut controls = vec![];
        buttons(
            i.surface().root.upcast_ref(),
            i.surface().root.upcast_ref(),
            i.surface().rect,
            &mut controls,
        );
        let options = i.effective_options();
        let pointer_cursor = i
            .frames
            .get(i.owner)
            .and_then(|f| f.cursor())
            .or_else(|| i.surface().root.cursor())
            .and_then(|c| c.name())
            .map(|s| s.to_string());
        serde_json::json!({"buttons":controls,"popup":i.popup.is_open(),"mode":if options.mode == recording::Mode::Normal {"normal"} else {"quick"},"phase":i.phase.get().name(),"rect":self.rect(),"toolbar":i.toolbar.get(),"visible":i.panel.is_visible(),"fps":options.fps,"delay":options.delay,"cursor":options.cursor,"pointerCursor":pointer_cursor,"systemAudio":options.system_audio,"microphone":options.microphone,"systemAudioAvailable":i.audio.system,"microphoneAvailable":i.audio.microphone,"elapsed":i.time.label().to_string()})
    }
    pub fn set_appearance(&self, appearance: Appearance) {
        let i = &self.0;
        if appearance.dark {
            i.panel.add_css_class("clivuu-theme-dark");
        } else {
            i.panel.remove_css_class("clivuu-theme-dark");
        }
        i.theme
            .borrow_mut()
            .update(appearance.dark, &appearance.accent);
        let accent = i.theme.borrow().accent();
        i.feedback
            .set_appearance(i.phase.get(), accent, appearance.display, &i.frames);
        i.panel.queue_draw();
        i.hint.queue_draw();
        i.surface().root.queue_draw();
        if i.phase.get().editable() {
            i.input();
        }
    }
    pub fn update(&self, phase: Phase, elapsed: Duration) {
        let changed = self.0.phase.replace(phase) != phase;
        let translated = self.0.tooltips.refresh();
        if !matches!(phase, Phase::Finishing | Phase::Exporting) {
            self.0.time.set_label(&recording::elapsed_label(elapsed));
        }
        if changed {
            self.0
                .feedback
                .update(&self.0.surface().root, phase, &self.0.frames);
        }
        if changed || translated {
            if !phase.editable() {
                self.0.selection.borrow_mut().end();
                self.0.surface().root.set_cursor_from_name(Some("default"));
                self.0.frames[self.0.owner].set_cursor_from_name(Some("default"));
            }
            self.0.popup.close();
            self.0.layout();
            for f in &self.0.frames {
                f.queue_draw();
            }
        }
    }
    /// Drain the visibility change at a session boundary, never per video frame.
    pub async fn settled(&self, cancel: &gio::Cancellable) -> Result<(), String> {
        let surface = self.0.surface();
        if let Some(clock) = surface
            .window
            .frame_clock()
            .filter(|_| surface.window.is_mapped())
        {
            let (tx, rx) = async_channel::bounded(1);
            let id = clock.connect_after_paint(move |_| {
                let _ = tx.try_send(());
            });
            surface.window.queue_draw();
            let result = glib::future_with_timeout(
                Duration::from_millis(500),
                gio::CancellableFuture::new(rx.recv(), cancel.clone()),
            )
            .await;
            clock.disconnect(id);
            result
                .map_err(|_| t("The desktop service is unavailable"))?
                .map_err(|_| t("Operation cancelled"))?
                .map_err(|_| t("Operation cancelled"))?;
        }
        gio::CancellableFuture::new(
            glib::timeout_future(Duration::from_millis(40)),
            cancel.clone(),
        )
        .await
        .map_err(|_| t("Operation cancelled"))
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        if let Some((clock, id)) = self.clock.take() {
            clock.disconnect(id);
        }
    }
}
impl Inner {
    fn effective_options(&self) -> Options {
        let mut options = self.options.get();
        options.system_audio &= self.audio.system;
        options.microphone &= self.audio.microphone;
        options
    }
    fn surface(&self) -> &Rc<crate::platform::surfaces::pins::Surface> {
        &self.hosts.surfaces[self.owner]
    }
    fn layout(&self) {
        let phase = self.phase.get();
        // Hidden GTK widgets measure as zero. Restore child visibility before
        // measuring, then apply capture-overlap rules before mapping/painting.
        self.panel.set_visible(true);
        self.hint.set_visible(true);
        for frame in &self.frames {
            frame.set_visible(true);
        }
        if !phase.editable() {
            self.surface().root.set_cursor_from_name(Some("default"));
        }
        let editable = phase.editable();
        if editable {
            self.start.add_css_class("clivuu-record-ready");
        } else {
            self.start.remove_css_class("clivuu-record-ready");
        }
        let options = self.effective_options();
        self.cursor.set_sensitive(editable);
        self.cursor.set_tooltip_text(Some(&t(if editable {
            "Show mouse cursor"
        } else {
            "Stop recording to change cursor capture"
        })));
        for (button, available, active, title, missing) in [
            (
                &self.system_audio,
                self.audio.system,
                options.system_audio,
                "Record system audio",
                "No system audio device detected",
            ),
            (
                &self.microphone,
                self.audio.microphone,
                options.microphone,
                "Record microphone audio",
                "No microphone device detected",
            ),
        ] {
            button.set_sensitive(editable && available);
            button.set_tooltip_text(Some(&t(if !available {
                missing
            } else if !editable {
                "Stop recording to change audio settings"
            } else {
                title
            })));
            if active {
                button.add_css_class("suggested-action");
            } else {
                button.remove_css_class("suggested-action");
            }
        }
        if options.cursor {
            self.cursor.add_css_class("suggested-action");
        } else {
            self.cursor.remove_css_class("suggested-action");
        }
        self.fps.set_sensitive(editable);
        self.fps.set_tooltip_text(Some(&t(if editable {
            "Frame rate"
        } else {
            "Stop recording to change frame rate"
        })));
        self.mode.set_sensitive(editable);
        self.mode.set_tooltip_text(Some(&t(if editable {
            "Recording mode"
        } else {
            "Stop recording to change recording mode"
        })));
        self.mode_label.set_label(&t(options.mode.label()));
        self.delay.set_sensitive(editable);
        self.delay.set_tooltip_text(Some(&t(if editable {
            "Recording delay"
        } else {
            "Stop recording to change delay"
        })));
        self.move_region.set_sensitive(editable);
        self.move_region.set_tooltip_text(Some(&t(if editable {
            "Move recording region"
        } else {
            "Stop recording to move region"
        })));
        let active_phase = matches!(phase, Phase::Recording | Phase::Paused);
        self.pause.set_sensitive(active_phase);
        self.restart.set_sensitive(active_phase);
        self.restart.set_tooltip_text(Some(&t(if active_phase {
            "Restart recording"
        } else {
            "Start recording before restarting"
        })));
        self.start.set_sensitive(matches!(
            phase,
            Phase::Ready | Phase::Recording | Phase::Paused
        ));
        self.close.set_sensitive(!matches!(
            phase,
            Phase::Starting | Phase::Finishing | Phase::Exporting
        ));
        self.start_icons.set_visible_child_name(if editable {
            "record-start"
        } else {
            "record-stop"
        });
        self.start
            .set_tooltip_text(Some(&crate::editor::shortcuts::tooltip(
                if editable {
                    "Start recording"
                } else {
                    "Stop recording"
                },
                if editable { "Enter" } else { "" },
            )));
        self.pause_icons
            .set_visible_child_name(if phase == Phase::Paused {
                "play"
            } else {
                "pause"
            });
        self.pause.set_tooltip_text(Some(&t(if !active_phase {
            "Start recording before pausing"
        } else if phase == Phase::Paused {
            "Resume recording"
        } else {
            "Pause recording"
        })));
        let options = self.options.get();
        self.fps_label.set_label(&format!("{} FPS", options.fps));
        self.delay_label.set_label(&if options.delay == 0 {
            t("No delay")
        } else {
            tf("{0} s", &[&options.delay.to_string()])
        });
        let rect = self.selection.borrow().rect;
        let surface = self.surface();
        self.scroller
            .set_max_content_width((surface.rect.width as i32 - 24).clamp(1, 1000));
        let description = match phase {
            Phase::Countdown(n) => tf("Recording starts in {0}", &[&n.to_string()]),
            Phase::Preparing | Phase::Starting => t("Starting recording"),
            Phase::Paused => t("Recording paused"),
            Phase::Finishing => t("Finishing recording"),
            Phase::Exporting => t("Exporting recording"),
            _ => format!(
                "{} × {} px",
                rect.width.round() as u32,
                rect.height.round() as u32
            ),
        };
        self.hint.set_label(&description);
        let panels = layout::selection(
            rect,
            surface.rect,
            layout::Size {
                width: self.panel.measure(gtk::Orientation::Horizontal, -1).1 as f64,
                height: 32.,
            },
            layout::Size {
                width: self.hint.measure(gtk::Orientation::Horizontal, -1).1 as f64,
                height: self.hint.measure(gtk::Orientation::Vertical, -1).1 as f64,
            },
        );
        let r = self
            .position
            .place(recording_toolbar(&panels, rect, surface.rect), surface.rect);
        self.toolbar.set(r);
        surface
            .root
            .move_(&self.panel, r.x - surface.rect.x, r.y - surface.rect.y);
        surface.root.set_child_size(
            &self.panel,
            Some((r.width as i32, super::floating_bar::TOOLBAR_HEIGHT)),
        );
        self.panel
            .set_visible(!phase.captures() || !overlaps(paint_bounds(r), rect));
        let hint = panels.hint;
        self.hint_rect.set(hint);
        surface
            .root
            .move_(&self.hint, hint.x - surface.rect.x, hint.y - surface.rect.y);
        surface
            .root
            .set_child_size(&self.hint, Some((hint.width as i32, hint.height as i32)));
        self.hint
            .set_visible(!phase.captures() || !overlaps(hint, rect));
        // Ready-state popups receive keyboard input, active recording leaves
        // keyboard focus with the application being demonstrated.
        if let Err(error) = surface.set_keyboard(editable, false) {
            eprintln!("{error}");
        }
        self.popup.set_viewport(Bounds {
            x: 0.,
            y: 0.,
            width: surface.rect.width,
            height: surface.rect.height,
        });
        self.popup.reposition();
        self.input();
        for surface in &self.hosts.surfaces {
            surface.window.set_visible(true);
        }
    }
    fn input(&self) {
        if let Err(error) = self
            .surface()
            .set_keyboard(self.phase.get().editable() || self.popup.is_open(), false)
        {
            eprintln!("{error}");
        }
        for surface in &self.hosts.surfaces {
            let mut areas = vec![];
            if Rc::ptr_eq(surface, self.surface()) {
                if self.panel.is_visible() {
                    areas.push(self.toolbar.get());
                }
                if let Some(p) = self.popup.bounds() {
                    areas.push(Bounds {
                        x: surface.rect.x + p.x() as f64,
                        y: surface.rect.y + p.y() as f64,
                        width: p.width() as f64,
                        height: p.height() as f64,
                    });
                }
                if self.phase.get().editable() {
                    if self.selection.borrow().drag.is_some() {
                        areas.push(surface.rect);
                    } else {
                        let r = self.selection.borrow().rect;
                        let m = self.feedback.resize_margin(7.);
                        areas.extend([
                            Bounds {
                                x: r.x - m,
                                y: r.y - m,
                                width: r.width + 2. * m,
                                height: 2. * m,
                            },
                            Bounds {
                                x: r.x - m,
                                y: r.y + r.height - m,
                                width: r.width + 2. * m,
                                height: 2. * m,
                            },
                            Bounds {
                                x: r.x - m,
                                y: r.y,
                                width: 2. * m,
                                height: r.height,
                            },
                            Bounds {
                                x: r.x + r.width - m,
                                y: r.y,
                                width: 2. * m,
                                height: r.height,
                            },
                        ]);
                    }
                }
            }
            surface.set_input(areas);
        }
    }
    pub(super) fn update_cursor(&self, point: [f64; 2]) {
        let dragging = self.selection.borrow().drag.is_some();
        let cursor = if self.phase.get().editable()
            && !self.popup.is_open()
            && (dragging || !self.toolbar.get().contains(point, 0.))
        {
            self.selection
                .borrow()
                .cursor(point, self.feedback.resize_margin(7.))
        } else {
            "default"
        };
        self.surface().root.set_cursor_from_name(Some(cursor));
        self.frames[self.owner].set_cursor_from_name(Some(cursor));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_placement_includes_shadow_and_fullscreen_requires_hiding() {
        let output = Bounds {
            x: -1920.,
            y: 100.,
            width: 1920.,
            height: 1080.,
        };
        let region = Bounds {
            x: -1800.,
            y: 200.,
            width: 900.,
            height: 500.,
        };
        assert!(!overlaps(
            paint_bounds(
                layout::selection(
                    region,
                    output,
                    layout::Size {
                        width: 520.,
                        height: 32.
                    },
                    layout::Size {
                        width: 100.,
                        height: 27.
                    }
                )
                .toolbar
            ),
            region
        ));
        assert!(overlaps(
            paint_bounds(
                layout::selection(
                    output,
                    output,
                    layout::Size {
                        width: 520.,
                        height: 32.
                    },
                    layout::Size {
                        width: 100.,
                        height: 27.
                    }
                )
                .toolbar
            ),
            output
        ));
    }
    #[test]
    fn shared_vertical_layout_wins_at_the_right_edge_and_sides_only_rescue_internal_controls() {
        let output = Bounds {
            x: -1920.,
            y: 100.,
            width: 1920.,
            height: 1080.,
        };
        let bar = |region: Bounds| {
            let panels = layout::selection(
                region,
                output,
                layout::Size {
                    width: 520.,
                    height: 32.,
                },
                layout::Size {
                    width: 100.,
                    height: 27.,
                },
            );
            recording_toolbar(&panels, region, output)
        };
        let below = Bounds {
            x: -900.,
            y: 200.,
            width: 900.,
            height: 500.,
        };
        let b = bar(below);
        assert_eq!(b.y, below.y + below.height + 8.);
        assert_eq!(b.x + b.width, -1.);
        let above = Bounds {
            y: 300.,
            height: 850.,
            ..below
        };
        let b = bar(above);
        assert!(b.y + b.height < above.y);
        assert_eq!(b.x + b.width, -1.);
        for (region, right) in [
            (
                Bounds {
                    x: -1900.,
                    y: 100.,
                    width: 500.,
                    height: 1080.,
                },
                true,
            ),
            (
                Bounds {
                    x: -500.,
                    y: 100.,
                    width: 500.,
                    height: 1080.,
                },
                false,
            ),
        ] {
            let b = bar(region);
            assert!(!overlaps(paint_bounds(b), region));
            assert_eq!(b.x > region.x, right);
            assert_eq!(b.y + b.height / 2., region.y + region.height / 2.);
            assert!(paint_bounds(b).y >= output.y);
            assert!(paint_bounds(b).y + paint_bounds(b).height <= output.y + output.height);
        }
        assert!(overlaps(paint_bounds(bar(output)), output));
    }
}
