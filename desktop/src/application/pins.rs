//! Pin presentation and input. All annotation state/undo/rendering belongs to
//! the same EditorDocument/EditorView used for capture, never to these hosts.
mod behavior;
mod drop;
mod feedback;
mod focus;
mod input;
mod magnifier;
mod shortcuts;
mod storage;
use crate::{
    editor::{
        self, Action, EditorController, ImageMapping, Tool,
        frozen_image::FrozenImage,
        scene::{Controller, EditorDocument, Input},
        view::EditorView,
    },
    infrastructure::tasks::{TaskError, TaskPool},
    platform::surfaces::{
        capture::gdk_outputs,
        pins::{Hosts, Surface},
    },
    ui::{
        pin_image::{PinImage, RADIUS},
        toolbar::{Output, Theme, Toolbar},
    },
};
use clivuu_core::{
    annotation::Bounds,
    capture::contains,
    i18n::{t, tf},
    pins::{Board, Pin, overlaps},
};
use gtk::{gdk, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::{Arc, Mutex},
};
#[derive(Clone, Debug)]
pub enum Event {
    Output(u64, Output),
    Changed,
    Paste,
    Drop(u64, gdk::Drop, bool),
    MoveToNewGroup(Vec<u64>),
    ManageGroups,
}
struct Data {
    image: FrozenImage,
    widgets: Vec<PinImage>,
    ocr: editor::ocr_layer::OcrOverlayState,
    ocr_widgets: Vec<editor::capture_view::Canvas>,
    editor: Option<Controller>,
    views: Vec<EditorView>,
    stored: Option<(u64, Arc<clivuu_core::image::RgbaImage>)>,
    version: u64,
    baseline: Option<EditorController>,
    presentation: Option<[f64; 2]>,
}
impl Data {
    fn new(
        image: FrozenImage,
        stored: Option<(u64, Arc<clivuu_core::image::RgbaImage>)>,
        version: u64,
    ) -> Self {
        Self {
            image,
            widgets: vec![],
            ocr: editor::ocr_layer::OcrOverlayState::default(),
            ocr_widgets: vec![],
            editor: None,
            views: vec![],
            stored,
            version,
            baseline: None,
            presentation: None,
        }
    }
}
struct Drag {
    id: u64,
    start: Pin,
    edge: [bool; 4],
}
struct Menu {
    surface: crate::platform::surfaces::menu::MenuSurface,
    buttons: Vec<(String, gtk::Button)>,
    pin: u64,
    opacity: [gtk::Button; 3],
}
pub struct Suspension {
    pins: Rc<Pins>,
    previous: bool,
}
impl Drop for Suspension {
    fn drop(&mut self) {
        self.pins.suspend(self.previous);
    }
}
pub struct Pins {
    pub board: RefCell<Board>,
    data: RefCell<HashMap<u64, Data>>,
    archived: RefCell<HashMap<u64, u64>>,
    storage: RefCell<Option<Arc<Mutex<crate::infrastructure::pin_store::PinStore>>>>,
    hosts: RefCell<Option<Hosts>>,
    menu: RefCell<Option<Menu>>,
    display: gdk::Display,
    gnome: bool,
    pool: Rc<TaskPool>,
    callback: Rc<dyn Fn(Event)>,
    drag: RefCell<Option<Drag>>,
    ocr_drag: Cell<Option<(u64, [f64; 2])>>,
    ocr_settings: RefCell<clivuu_core::ocr::OcrSettings>,
    ocr_animating: Cell<bool>,
    live_config: RefCell<Option<Rc<crate::infrastructure::configuration::Configuration>>>,
    active: Cell<Option<u64>>,
    highlighted: Cell<Option<u64>>,
    selected_pins: RefCell<Vec<u64>>,
    pointer: Cell<Option<[f64; 2]>>,
    lens_id: Cell<Option<u64>>,
    lens: RefCell<magnifier::Lens>,
    suspended: Cell<bool>,
    locked: Cell<bool>,
    queued: Cell<bool>,
    syncing: Cell<bool>,
    pending_input: RefCell<Option<(gdk::FrameClock, glib::SignalHandlerId)>>,
    theme: RefCell<Theme>,
    appearance: RefCell<Option<(bool, String)>>,
    language: Cell<clivuu_core::i18n::Language>,
    css: gtk::CssProvider,
    pub revision: Cell<u64>,
    pub preferences: RefCell<clivuu_core::pins::preferences::Preferences>,
    flashing: RefCell<Option<(Vec<u64>, i64)>>,
    flash_timer: RefCell<Option<glib::SourceId>>,
    feedback: RefCell<feedback::Feedback>,
}
impl Drop for Pins {
    fn drop(&mut self) {
        if let Some(timer) = self.flash_timer.take() {
            timer.remove();
        }
        if let Some((clock, id)) = self.pending_input.take() {
            clock.disconnect(id);
        }
        self.menu.take();
        self.feedback.take();
        self.data.take();
        self.hosts.take();
        gtk::style_context_remove_provider_for_display(&self.display, &self.css);
    }
}
impl Pins {
    pub fn new(
        display: &gdk::Display,
        pool: Rc<TaskPool>,
        callback: impl Fn(Event) + 'static,
    ) -> Rc<Self> {
        let dark = adw::StyleManager::default().is_dark();
        let css = gtk::CssProvider::new();
        css.load_from_string(&pin_css(dark, "#bd93f9"));
        gtk::style_context_add_provider_for_display(
            display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let this = Rc::new(Self {
            board: RefCell::new(Board::default()),
            data: RefCell::new(HashMap::new()),
            archived: RefCell::new(HashMap::new()),
            storage: RefCell::new(None),
            hosts: RefCell::new(None),
            menu: RefCell::new(None),
            display: display.clone(),
            gnome: display.type_().name() == "GdkX11Display"
                && crate::platform::display::is_gnome(),
            pool,
            callback: Rc::new(callback),
            drag: RefCell::new(None),
            ocr_drag: Cell::new(None),
            ocr_settings: RefCell::new(clivuu_core::ocr::OcrSettings::default()),
            ocr_animating: Cell::new(false),
            live_config: RefCell::new(None),
            active: Cell::new(None),
            highlighted: Cell::new(None),
            selected_pins: RefCell::new(vec![]),
            pointer: Cell::new(None),
            lens_id: Cell::new(None),
            lens: RefCell::new(Default::default()),
            suspended: Cell::new(false),
            locked: Cell::new(false),
            queued: Cell::new(false),
            syncing: Cell::new(false),
            pending_input: RefCell::new(None),
            theme: RefCell::new(Toolbar::install_theme(display, dark, "#bd93f9")),
            appearance: RefCell::new(None),
            language: Cell::new(clivuu_core::i18n::current_language()),
            css,
            revision: Cell::new(0),
            preferences: RefCell::new(Default::default()),
            flashing: RefCell::new(None),
            flash_timer: RefCell::new(None),
            feedback: RefCell::new(Default::default()),
        });
        let weak = Rc::downgrade(&this);
        crate::infrastructure::ocr::OcrDownloadManager::shared().connect_changed(move || {
            let Some(this) = weak.upgrade() else {
                return;
            };
            let ready_ids: Vec<u64> = this
                .data
                .borrow()
                .iter()
                .filter_map(|(&id, d)| {
                    if matches!(d.ocr.phase, editor::ocr_layer::OcrPhase::NeedsSetup) {
                        let s = &d.ocr.settings;
                        (!clivuu_core::ocr::availability(s).needs_paddle_download(s.engine))
                            .then_some(id)
                    } else {
                        None
                    }
                })
                .collect();
            if !ready_ids.is_empty() {
                for id in ready_ids {
                    this.start_pin_ocr_scan(id);
                }
            } else if this
                .data
                .borrow()
                .values()
                .any(|d| matches!(d.ocr.phase, editor::ocr_layer::OcrPhase::NeedsSetup))
            {
                this.sync();
            }
        });
        this
    }
    pub fn set_live_config(&self, config: Rc<crate::infrastructure::configuration::Configuration>) {
        self.live_config.replace(Some(config));
    }
    pub fn settings(self: &Rc<Self>, settings: &serde_json::Map<String, serde_json::Value>) {
        let language = clivuu_core::i18n::current_language();
        if self.language.replace(language) != language {
            self.close_menu();
            for data in self.data.borrow().values() {
                for view in &data.views {
                    view.toolbar().refresh();
                    view.options().refresh();
                }
            }
        }
        let next = clivuu_core::pins::preferences::Preferences::from_values(settings);
        let next_ocr = clivuu_core::ocr::OcrSettings::from_values(settings);
        self.ocr_settings.replace(next_ocr.clone());
        for d in self.data.borrow_mut().values_mut() {
            if d.ocr.is_active() {
                d.ocr.settings.smart_paragraphs = next_ocr.smart_paragraphs;
                d.ocr.settings.download_mirror = next_ocr.download_mirror.clone();
            } else {
                d.ocr.settings = next_ocr.clone();
            }
        }
        let previous = self.preferences.replace(next.clone());
        if previous.gnome_focus != next.gnome_focus {
            self.close_menu();
            self.reconcile_pointer_activation();
        }
        self.apply_limits();
        if previous.closed_limit != next.closed_limit {
            self.prune_closed();
        }
        let dark = match settings["theme"].as_str() {
            Some("light") => false,
            Some("dark") => true,
            _ => adw::StyleManager::default().is_dark(),
        };
        let appearance = (
            dark,
            settings["accent-color"]
                .as_str()
                .unwrap_or("#bd93f9")
                .to_string(),
        );
        if self.appearance.borrow().as_ref() == Some(&appearance) {
            self.sync();
            return;
        }
        let accent = appearance.1.clone();
        self.appearance.replace(Some(appearance));
        self.css.load_from_string(&pin_css(dark, &accent));
        self.theme.borrow_mut().update(dark, &accent);
        for d in self.data.borrow().values() {
            for view in &d.views {
                view.set_theme(&self.theme.borrow());
            }
        }
        self.sync();
    }
    pub async fn pixel_scale(
        self: &Rc<Self>,
        rect: Bounds,
        cancel: &gtk::gio::Cancellable,
    ) -> Result<f64, String> {
        self.ensure_hosts()?;
        let surface = self
            .hosts
            .borrow()
            .as_ref()
            .and_then(|hosts| hosts.surfaces.iter().find(|s| s.rect == rect).cloned())
            .ok_or_else(|| t("Invalid monitor layout"))?;
        let result = surface.pixel_scale(cancel).await;
        // Visibility commands may run while the first Wayland frame configures.
        // Restore the current board state even when probing is cancelled.
        self.sync();
        let scale = result?;
        if !self
            .hosts
            .borrow()
            .as_ref()
            .is_some_and(|hosts| hosts.surfaces.iter().any(|s| Rc::ptr_eq(s, &surface)))
        {
            return Err(t("The monitor layout changed during capture"));
        }
        Ok(scale)
    }
    pub fn add(
        self: &Rc<Self>,
        image: FrozenImage,
        position: Option<Bounds>,
    ) -> Result<u64, String> {
        self.ensure_hosts()?;
        let outputs = gdk_outputs(&self.display);
        let output = outputs
            .first()
            .copied()
            .ok_or_else(|| t("Invalid monitor layout"))?;
        let rect = position.unwrap_or_else(|| {
            let first = output.rect;
            Bounds {
                x: clivuu_core::pins::snap_coord(
                    first.x + (first.width - image.dimensions()[0] as f64) / 2.,
                    output.scale,
                ),
                y: clivuu_core::pins::snap_coord(
                    first.y + (first.height - image.dimensions()[1] as f64) / 2.,
                    output.scale,
                ),
                width: image.dimensions()[0] as f64,
                height: image.dimensions()[1] as f64,
            }
        });
        let previous = self.board.borrow().selected;
        let id = self.board.borrow_mut().add(rect)?;
        if !self.preferences.borrow().activate_new {
            self.board.borrow_mut().selected = previous;
        }
        self.preferences
            .borrow()
            .initialize(self.board.borrow_mut().get_mut(id).unwrap());
        self.data.borrow_mut().insert(id, Data::new(image, None, 0));
        self.changed();
        self.sync();
        let settings = self.preferences.borrow().clone();
        if settings.activate_new {
            self.toolbar(None);
            self.highlight(Some(id));
            self.focus_pin(id);
        }
        if settings.flash_new {
            self.flash(vec![id]);
        }
        Ok(id)
    }
    fn changed(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
        (self.callback)(Event::Changed);
    }
    fn ensure_hosts(self: &Rc<Self>) -> Result<(), String> {
        if self.hosts.borrow().is_some() {
            return Ok(());
        }
        let weak = Rc::downgrade(self);
        let hosts = Hosts::new(
            &self.display,
            crate::platform::surfaces::PinRole::Pin,
            move || {
                if let Some(this) = weak.upgrade() {
                    this.rebuild();
                }
            },
        )?;
        for s in &hosts.surfaces {
            self.input(s);
            self.drop_target(s);
        }
        self.hosts.replace(Some(hosts));
        Ok(())
    }
    fn rebuild(self: &Rc<Self>) {
        self.close_menu();
        self.toolbar(None);
        self.drag.borrow_mut().take();
        // Detach ownership before destroying native surfaces can emit input.
        for d in self.data.borrow_mut().values_mut() {
            d.views.clear();
            d.widgets.clear();
            d.ocr_widgets.clear();
        }
        self.lens.replace(Default::default());
        self.feedback.replace(Default::default());
        let old = self.hosts.borrow_mut().take();
        drop(old);
        self.board
            .borrow_mut()
            .keep_reachable(&gdk_outputs(&self.display));
        if !self.board.borrow().pins.is_empty()
            && let Err(e) = self.ensure_hosts()
        {
            eprintln!("{e}");
        }
        let ids: Vec<_> = self
            .data
            .borrow()
            .iter()
            .filter(|(_, d)| d.editor.is_some())
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Err(e) = self.ensure_editor(id) {
                eprintln!("{e}");
            }
        }
        self.changed();
        self.sync();
    }
    fn request_sync(self: &Rc<Self>, surface: &Rc<Surface>) {
        if self.queued.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        surface.window.add_tick_callback(move |_, _| {
            if let Some(this) = weak.upgrade()
                && this.queued.replace(false)
            {
                this.sync();
            }
            glib::ControlFlow::Break
        });
    }
    pub(super) fn sync_pin_translation(&self, id: u64) -> bool {
        if self.syncing.replace(true) {
            return true;
        }
        let fast_ok = {
            let board = self.board.borrow();
            let obscured = (self.suspended.get() || self.locked.get()) || board.hidden;
            let active = self.active.get();
            if let (Some(hosts), Some(pin), Some(d)) = (
                self.hosts.borrow().as_ref(),
                board.get(id),
                self.data.borrow().get(&id),
            ) && d.widgets.len() == hosts.surfaces.len()
            {
                let rect = pin.rect();
                if let Some(doc) = &d.editor {
                    doc.set_mapping(ImageMapping {
                        display: rect,
                        document: d.image.dimensions().map(|v| v as f64),
                    });
                }
                for (i, s) in hosts.surfaces.iter().enumerate() {
                    let visible = !obscured && board.visible(pin) && overlaps(rect, s.rect);
                    let widget = &d.widgets[i];
                    widget.set_visible(visible);
                    s.root.move_(
                        widget,
                        rect.x - s.rect.x - RADIUS,
                        rect.y - s.rect.y - RADIUS,
                    );
                    if let Some(ocr_w) = d.ocr_widgets.get(i) {
                        let ocr_vis = visible && d.ocr.is_active();
                        ocr_w.set_visible(ocr_vis);
                        s.root.move_(ocr_w, rect.x - s.rect.x, rect.y - s.rect.y);
                        if ocr_vis {
                            ocr_w.queue_draw();
                        }
                    }
                    if let Some(view) = d.views.get(i) {
                        let editing = active == Some(id) && !obscured && visible;
                        let owns_controls = editing && i == output_owner(hosts, rect);
                        let ocr_active = d.ocr.is_active();
                        view.scene().set_visible(visible);
                        if editing {
                            if owns_controls {
                                view.ocr_bar().sync(&d.ocr);
                            } else {
                                view.ocr_bar().widget().set_visible(false);
                            }
                            view.set_defer_layout(true);
                            view.set_controls_visible(owns_controls);
                            view.set_keyboard_enabled(self.keyboard_enabled());
                            view.inline()
                                .set_enabled(!ocr_active && self.keyboard_enabled());
                            view.scene().set_can_target(!ocr_active);
                            view.scene().set_decorations_visible(!ocr_active);
                            view.set_selection(rect, true);
                            view.set_defer_layout(false);
                            view.layout();
                        } else {
                            s.root
                                .move_(view.scene(), rect.x - s.rect.x, rect.y - s.rect.y);
                        }
                    }
                }
                true
            } else {
                false
            }
        };
        if fast_ok {
            self.update_feedback();
        }
        self.syncing.set(false);
        fast_ok
    }
    fn editor_at(&self, point: [f64; 2]) -> bool {
        let Some(id) = self.active.get() else {
            return false;
        };
        let b = self.board.borrow();
        let Some(p) = b.get(id) else {
            return false;
        };
        let data = self.data.borrow();
        let Some(d) = data.get(&id) else {
            return false;
        };
        if d.views
            .iter()
            .any(|v| v.input_rects().iter().any(|r| contains(*r, point)))
        {
            return true;
        }
        if d.ocr.is_active() {
            return false;
        }
        let Some(doc) = &d.editor else {
            return false;
        };
        let c = doc.borrow();
        c.is_polyline_drawing()
            || c.crop_contains_display(point)
            || (contains(p.rect(), point)
                && (c.tool() != Tool::Select
                    || c.session().current().is_some()
                    || c.session()
                        .target(c.mapping().point(point), None, c.mapping().screen_scale())
                        .is_some()))
    }
    /// Logical activation belongs to a pin, including its editor controls.
    fn activate_at(self: &Rc<Self>, point: [f64; 2]) -> Option<u64> {
        self.pointer.set(Some(point));
        let editing = self.editor_at(point);
        let id = if editing {
            self.active.get()
        } else {
            self.board.borrow().hit(point)
        };
        self.board.borrow_mut().selected = id;
        if !editing {
            self.selected_pins.borrow_mut().clear();
        }
        self.highlight(id);
        if let Some(id) = id {
            self.focus_pin(id);
        }
        id
    }
    /// Board::selected remains the last command target after deactivation.
    fn highlight(self: &Rc<Self>, id: Option<u64>) {
        if id.is_none() {
            self.lens.borrow_mut().alt = 0;
            for (_, w) in &self.lens.borrow().widgets {
                w.set_visible(false);
            }
        }
        if self.highlighted.replace(id) != id {
            self.sync();
        }
    }
    pub fn sync(self: &Rc<Self>) {
        self.queued.set(false);
        if self.syncing.replace(true) {
            return;
        }
        let mut feedback = None;
        let (obscured, any_visible) = {
            let board = self.board.borrow();
            if let Some(menu) = self.menu.borrow().as_ref()
                && let Some(pin) = board.get(menu.pin)
            {
                set_opacity_controls(&menu.opacity, pin.opacity);
            }
            let obscured = (self.suspended.get() || self.locked.get()) || board.hidden;
            let active = self.active.get();
            let highlighted = self.highlighted.get();
            let accent = self.theme.borrow().decoration_style().accent;
            let (translucent_flash, shadow_color) = {
                let prefs = self.preferences.borrow();
                (
                    prefs.translucent_flash,
                    gdk::RGBA::parse(&prefs.shadow_tint).expect("validated pin shadow color"),
                )
            };
            if let Some(hosts) = self.hosts.borrow().as_ref() {
                let mut data = self.data.borrow_mut();
                let selected_pins = self.selected_pins.borrow();
                for pin in &board.pins {
                    let id = &pin.id;
                    let Some(d) = data.get_mut(id) else { continue };
                    let rect = pin.rect();
                    if let Some(previous) = d.presentation.replace([pin.zoom, pin.opacity])
                        && !obscured
                        && board.visible(pin)
                    {
                        let mut text = vec![];
                        if previous[0] != pin.zoom {
                            text.push(tf("Size: {0}%", &[&(pin.zoom * 100.).round().to_string()]));
                        }
                        if previous[1] != pin.opacity {
                            text.push(tf(
                                "Opacity: {0}%",
                                &[&(pin.opacity * 100.).round().to_string()],
                            ));
                        }
                        if !text.is_empty() {
                            feedback = Some((*id, text.join("\n")));
                        }
                    }
                    while d.widgets.len() < hosts.surfaces.len() {
                        let s = &hosts.surfaces[d.widgets.len()];
                        let widget = PinImage::new(&d.image.texture);
                        s.root.put(&widget, 0., 0.);
                        d.widgets.push(widget);
                    }
                    while d.ocr_widgets.len() < hosts.surfaces.len() {
                        let idx = d.ocr_widgets.len();
                        let s = &hosts.surfaces[idx];
                        let surface_rect = s.rect;
                        let pin_id = *id;
                        let weak = Rc::downgrade(self);
                        let canvas = editor::capture_view::Canvas::new(move |snap| {
                            let Some(this) = weak.upgrade() else {
                                return;
                            };
                            let board = this.board.borrow();
                            let Some(p) = board.get(pin_id) else {
                                return;
                            };
                            let data = this.data.borrow();
                            let Some(d) = data.get(&pin_id) else {
                                return;
                            };
                            let r = p.rect();
                            let local_target = Bounds {
                                x: r.x,
                                y: r.y,
                                width: r.width,
                                height: r.height,
                            };
                            let theme = this.theme.borrow();
                            editor::ocr_layer::paint_ocr_overlay(
                                snap,
                                &d.ocr,
                                local_target,
                                [r.x, r.y],
                                theme.accent(),
                                theme.is_dark(),
                            );
                            let _ = surface_rect;
                        });
                        canvas.set_visible(false);
                        s.root.put(&canvas, 0., 0.);
                        d.ocr_widgets.push(canvas);
                    }
                    if let Some(doc) = &d.editor {
                        doc.set_mapping(ImageMapping {
                            display: rect,
                            document: d.image.dimensions().map(|v| v as f64),
                        });
                    }
                    let flashing = self.flash_phase(*id);
                    let opacity = pin.opacity
                        * if flashing && translucent_flash {
                            0.55
                        } else {
                            1.
                        };
                    let color = if highlighted == Some(*id) || selected_pins.contains(id) {
                        accent
                    } else {
                        shadow_color
                    };
                    let filter = editor::filter::Filter {
                        grayscale: pin.grayscale,
                        invert: pin.inverted,
                    };
                    for (i, s) in hosts.surfaces.iter().enumerate() {
                        let widget = &d.widgets[i];
                        let visible = !obscured && board.visible(pin) && overlaps(rect, s.rect);
                        widget.set_visible(visible);
                        s.root.move_(
                            widget,
                            rect.x - s.rect.x - RADIUS,
                            rect.y - s.rect.y - RADIUS,
                        );
                        s.root.set_child_size(
                            widget,
                            Some((
                                (rect.width + 2. * RADIUS).ceil() as i32,
                                (rect.height + 2. * RADIUS).ceil() as i32,
                            )),
                        );
                        widget.set_opacity(opacity);
                        widget.set_filter(filter);
                        widget.configure(pin.shadow, pin.smooth);
                        widget.flash(flashing.then_some(accent));
                        widget.update(rect.width, rect.height, &color, d.views.is_empty());
                        if let Some(ocr_w) = d.ocr_widgets.get(i) {
                            let ocr_vis = visible && d.ocr.is_active();
                            ocr_w.set_visible(ocr_vis);
                            s.root.move_(ocr_w, rect.x - s.rect.x, rect.y - s.rect.y);
                            s.root.set_child_size(
                                ocr_w,
                                Some((rect.width.ceil() as i32, rect.height.ceil() as i32)),
                            );
                            ocr_w.set_opacity(opacity);
                            if ocr_vis {
                                ocr_w.queue_draw();
                            }
                        }
                        if let Some(view) = d.views.get(i) {
                            let editing = active == Some(*id) && !obscured && visible;
                            let owns_controls = editing && i == output_owner(hosts, rect);
                            let ocr_active = d.ocr.is_active();
                            view.toolbar().set_ocr_active(ocr_active);
                            if owns_controls {
                                view.ocr_bar().sync(&d.ocr);
                            } else {
                                view.ocr_bar().widget().set_visible(false);
                            }
                            view.set_controls_visible(owns_controls);
                            view.set_keyboard_enabled(editing && self.keyboard_enabled());
                            view.inline()
                                .set_enabled(editing && !ocr_active && self.keyboard_enabled());
                            view.scene().set_can_target(editing && !ocr_active);
                            view.scene().set_visible(visible);
                            view.scene().set_decorations_visible(editing && !ocr_active);
                            view.scene().set_opacity(opacity);
                            view.scene().set_smooth(pin.smooth);
                            view.scene().set_filter(filter);
                            view.set_selection(rect, true);
                            view.layout();
                            if owns_controls
                                && ocr_active
                                && self.keyboard_enabled()
                                && s.window.is_active()
                                && gtk::prelude::GtkWindowExt::focus(&s.window).is_none()
                            {
                                s.root.grab_focus();
                            }
                        }
                    }
                }
                // Native sibling order and snapshot order must match Board::hit.
                for (i, s) in hosts.surfaces.iter().enumerate() {
                    let mut layers = Vec::with_capacity(board.pins.len() * 3);
                    for pin in &board.pins {
                        if let Some(d) = data.get(&pin.id) {
                            if let Some(w) = d.widgets.get(i) {
                                layers.push(w.clone().upcast());
                            }
                            if let Some(v) = d.views.get(i) {
                                layers.push(v.scene().clone().upcast());
                            }
                            if let Some(ow) = d.ocr_widgets.get(i) {
                                layers.push(ow.clone().upcast());
                            }
                        }
                    }
                    s.root.order_background(&layers);
                }
            }
            (
                obscured,
                !obscured && board.pins.iter().any(|p| board.visible(p)),
            )
        };
        if obscured || self.drag.borrow().is_none() {
            self.update_input();
        }
        if let Some(hosts) = self.hosts.borrow().as_ref() {
            for s in &hosts.surfaces {
                s.window.set_visible(any_visible);
            }
        }
        self.update_keyboard();
        if let Some((id, text)) = feedback {
            self.show_feedback(id, text);
        }
        self.update_feedback();
        self.syncing.set(false);
    }
    fn update_input(&self) {
        let board = self.board.borrow();
        let data = self.data.borrow();
        let obscured = (self.suspended.get() || self.locked.get()) || board.hidden;
        if let Some(hosts) = self.hosts.borrow().as_ref() {
            for (i, s) in hosts.surfaces.iter().enumerate() {
                let mut input = vec![];
                if !obscured {
                    input.extend(
                        board
                            .pins
                            .iter()
                            .filter(|p| board.visible(p) && !p.through)
                            .map(|p| p.rect()),
                    );
                    if let Some(id) = self
                        .active
                        .get()
                        .filter(|id| board.get(*id).is_some_and(|p| board.visible(p)))
                        && let Some(view) = data.get(&id).and_then(|d| d.views.get(i))
                    {
                        input.extend(view.input_rects());
                    }
                }
                s.set_input(input);
            }
        }
    }
    fn layout_changed(self: &Rc<Self>, surface: &Rc<Surface>) {
        if self.drag.borrow().is_some() || self.pending_input.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(self);
        if let Some(clock) = surface.window.frame_clock() {
            let id = clock.connect_after_paint(move |_| {
                if let Some(this) = weak.upgrade() {
                    if let Some((clock, id)) = this.pending_input.borrow_mut().take() {
                        clock.disconnect(id);
                    }
                    this.update_input();
                }
            });
            self.pending_input.replace(Some((clock, id)));
        } else {
            glib::idle_add_local_once(move || {
                if let Some(this) = weak.upgrade() {
                    this.update_input();
                }
            });
        }
    }
    pub fn during_capture(self: &Rc<Self>, hide: bool) -> Option<Suspension> {
        if !hide {
            return None;
        }
        let previous = self.suspended.get();
        self.suspend(true);
        Some(Suspension {
            pins: self.clone(),
            previous,
        })
    }
    pub fn lock(self: &Rc<Self>, locked: bool) {
        self.locked.set(locked);
        if locked {
            self.toolbar(None);
        }
        self.sync();
    }
    pub fn suspend(self: &Rc<Self>, suspended: bool) {
        self.suspended.set(suspended);
        if suspended {
            self.close_menu();
        }
        self.sync();
    }
    pub fn close(self: &Rc<Self>, id: u64) {
        self.close_menu();
        if self.active.get() == Some(id) {
            self.toolbar(None);
        }
        self.board
            .borrow_mut()
            .close(id, (glib::real_time() / 1000) as u64);
        if self.highlighted.get() == Some(id) {
            self.highlight(None);
        }
        self.prune_closed();
        self.changed();
        self.sync();
    }
    pub fn destroy(self: &Rc<Self>, id: u64) {
        self.close_menu();
        if self.active.get() == Some(id) {
            self.toolbar(None);
        }
        self.board.borrow_mut().destroy(id);
        if self.highlighted.get() == Some(id) {
            self.highlight(None);
        }
        self.archived.borrow_mut().remove(&id);
        self.release_data(id);
        self.changed();
        self.sync();
    }
    pub fn release_destroyed(self: &Rc<Self>, ids: &[u64]) {
        if ids.is_empty() {
            return;
        }
        self.close_menu();
        if self.active.get().is_some_and(|id| ids.contains(&id)) {
            self.toolbar(None);
        }
        if self.highlighted.get().is_some_and(|id| ids.contains(&id)) {
            self.highlight(None);
        }
        self.selected_pins
            .borrow_mut()
            .retain(|id| !ids.contains(id));
        for id in ids {
            self.archived.borrow_mut().remove(id);
            self.release_data(*id);
        }
    }
    pub fn move_to_group(self: &Rc<Self>, ids: &[u64], target: &str) -> Result<usize, String> {
        self.close_menu();
        let active_group = self.board.borrow().group.clone();
        if target != active_group && self.active.get().is_some_and(|id| ids.contains(&id)) {
            self.toolbar(None);
        }
        let moved = self.board.borrow_mut().move_pins_to_group(ids, target)?;
        if target != active_group {
            self.selected_pins
                .borrow_mut()
                .retain(|id| !ids.contains(id));
            if self.highlighted.get().is_some_and(|id| ids.contains(&id)) {
                self.highlight(None);
            }
        }
        if moved > 0 {
            self.changed();
            self.sync();
        }
        Ok(moved)
    }
    fn release_data(&self, id: u64) {
        let mut data = self.data.borrow_mut().remove(&id);
        if let Some(data) = data.as_mut() {
            data.ocr.deactivate();
            if let Some(hosts) = self.hosts.borrow().as_ref() {
                for (s, w) in hosts.surfaces.iter().zip(data.widgets.iter()) {
                    s.root.remove(w);
                }
                for (s, w) in hosts.surfaces.iter().zip(data.ocr_widgets.iter()) {
                    s.root.remove(w);
                }
            }
        }
    }
    pub fn toolbar(self: &Rc<Self>, id: Option<u64>) {
        self.close_menu();
        let old = self.active.replace(id);
        if old == id {
            return;
        }
        let previous =
            old.and_then(|id| self.data.borrow().get(&id).and_then(|d| d.editor.clone()));
        if let Some(doc) = previous {
            doc.dispatch(Action::CommitText);
            doc.dispatch(Action::FinishPolyline);
        }
        if let Some(old_id) = old
            && let Some(d) = self.data.borrow_mut().get_mut(&old_id)
        {
            d.ocr.deactivate();
        }
        if id.is_none() {
            self.reconcile_pointer_activation();
            self.sync();
            if let Some(hosts) = self.hosts.borrow().as_ref() {
                for s in &hosts.surfaces {
                    if s.window.is_active() {
                        s.root.grab_focus();
                    }
                }
            }
            return;
        }
        let id = id.unwrap();
        self.highlight(Some(id));
        let result = self.ensure_editor(id);
        if let Err(e) = result {
            eprintln!("{e}");
            self.active.set(None);
            return;
        }
        if let Some(d) = self.data.borrow_mut().get_mut(&id) {
            d.baseline = d.editor.as_ref().map(|c| c.checkpoint());
        }
        self.sync();
        self.focus_pin(id);
        if self.keyboard_enabled()
            && let Some(hosts) = self.hosts.borrow().as_ref()
            && let Some(d) = self.data.borrow().get(&id)
        {
            for (i, s) in hosts.surfaces.iter().enumerate() {
                if s.window.is_active()
                    && let Some(v) = d.views.get(i)
                {
                    v.scene().grab_focus();
                }
            }
        }
    }
    fn ensure_editor(self: &Rc<Self>, id: u64) -> Result<(), String> {
        self.ensure_hosts()?;
        let rect = self
            .board
            .borrow()
            .get(id)
            .ok_or_else(|| t("Invalid saved pin data"))?
            .rect();
        let mut data = self.data.borrow_mut();
        let d = data
            .get_mut(&id)
            .ok_or_else(|| t("Invalid saved pin data"))?;
        let hosts = self.hosts.borrow();
        let hosts = hosts.as_ref().unwrap();
        if d.editor.is_none() {
            let context = hosts.surfaces[0].root.pango_context();
            let model = EditorController::new(ImageMapping {
                display: rect,
                document: d.image.dimensions().map(|v| v as f64),
            })
            .with_text_measurer(move |m, u| editor::render::text_metrics(&context, m, u));
            let doc = EditorDocument::new(model);
            doc.set_shared_source(d.image.pixels.clone(), self.pool.clone());
            d.editor = Some(doc);
        }
        if !d.views.is_empty() {
            return Ok(());
        }
        let doc = d.editor.as_ref().unwrap();
        for s in &hosts.surfaces {
            let weak = Rc::downgrade(self);
            let view = EditorView::new(
                &s.root,
                doc.clone(),
                &d.image.texture,
                s.rect,
                [s.rect.x, s.rect.y],
                true,
                super::editor_services::color_services(),
                move |action| {
                    if let Some(this) = weak.upgrade() {
                        (this.callback)(Event::Output(id, action));
                    }
                },
            );
            let weak = Rc::downgrade(self);
            let scene = view.scene().downgrade();
            view.scene().set_input_route(move |input| {
                let (Some(this), Some(scene)) = (weak.upgrade(), scene.upgrade()) else {
                    return false;
                };
                match input {
                    Input::Key(key, state) => this.key(id, *key, *state),
                    Input::KeyReleased(key) => this.lens_key(id, *key, true),
                    Input::Secondary { point } => {
                        let _ = this.open_menu(id, *point);
                        true
                    }
                    Input::Begin { point, .. } if this.active.get() != Some(id) => {
                        let _ = scene;
                        let _ = point;
                        true
                    }
                    _ => false,
                }
            });
            let weak = Rc::downgrade(self);
            let surface = Rc::downgrade(s);
            let s = Rc::downgrade(s);
            view.scene().connect_changed(move |effect| {
                if let (Some(this), Some(s)) = (weak.upgrade(), s.upgrade()) {
                    if effect == editor::Effect::Committed {
                        this.changed();
                    }
                    this.request_sync(&s);
                }
            });
            let weak = Rc::downgrade(self);
            view.connect_layout_changed(move || {
                if let (Some(this), Some(surface)) = (weak.upgrade(), surface.upgrade()) {
                    this.layout_changed(&surface);
                }
            });
            let weak_tool = Rc::downgrade(self);
            view.toolbar().connect_tool_click(move || {
                if let Some(this) = weak_tool.upgrade() {
                    this.deactivate_pin_ocr(id);
                }
            });
            let weak_ocr = Rc::downgrade(self);
            view.ocr_bar().connect_action(move |action| {
                if let Some(this) = weak_ocr.upgrade() {
                    this.handle_pin_ocr_bar_action(id, action);
                }
            });
            view.set_theme(&self.theme.borrow());
            d.views.push(view);
        }
        Ok(())
    }
    pub async fn png(
        self: &Rc<Self>,
        id: u64,
        cancel: &gio::Cancellable,
    ) -> Result<Vec<u8>, String> {
        if self
            .board
            .borrow()
            .get(id)
            .is_some_and(|p| p.grayscale || p.inverted)
        {
            self.ensure_editor(id)?;
            self.sync();
        }
        let state = {
            let data = self.data.borrow();
            let d = data.get(&id).ok_or_else(|| t("Invalid saved pin data"))?;
            d.views.first().map(|view| view.scene().clone())
        };
        if let Some(scene) = state {
            let renderer = self
                .hosts
                .borrow()
                .as_ref()
                .and_then(|h| h.surfaces.first())
                .and_then(|s| s.window.renderer())
                .ok_or_else(|| t("The desktop service is unavailable"))?;
            return editor::export::png(&scene, &renderer, None, &self.pool, cancel)
                .await
                .map_err(|e| format!("{e:?}"));
        }
        let pixels = self.data.borrow().get(&id).unwrap().image.pixels.clone();
        let task = self
            .pool
            .submit(move |token| {
                token.check()?;
                let mut png = std::io::Cursor::new(vec![]);
                pixels
                    .write_to(&mut png, clivuu_core::image::ImageFormat::Png)
                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                token.check()?;
                Ok(png.into_inner())
            })
            .map_err(|e| e.to_string())?;
        gio::CancellableFuture::new(task.result(), cancel.clone())
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|e| e.to_string())
    }
    pub async fn edit_action(
        self: &Rc<Self>,
        id: u64,
        action: Output,
        cancel: &gio::Cancellable,
    ) -> Result<bool, String> {
        let transformed = matches!(
            action,
            Output::RotateRight
                | Output::RotateLeft
                | Output::FlipHorizontal
                | Output::FlipVertical
        );
        if !matches!(action, Output::Apply | Output::Close) && !transformed {
            return Ok(false);
        }
        if transformed {
            self.ensure_editor(id)?;
        }
        let state = {
            let data = self.data.borrow();
            data.get(&id).map(|d| {
                (
                    d.editor.clone(),
                    d.views.first().map(|v| v.scene().clone()),
                    d.image.dimensions(),
                )
            })
        };
        let Some((Some(doc), Some(scene), size)) = state else {
            self.toolbar(None);
            return Ok(true);
        };
        if matches!(action, Output::Close) {
            doc.dispatch(Action::Cancel);
            let baseline = {
                let mut data = self.data.borrow_mut();
                let d = data.get_mut(&id).unwrap();
                d.version += doc.borrow().revision() + 1;
                d.stored = None;
                d.baseline.take()
            };
            if let Some(baseline) = baseline {
                doc.restore_checkpoint(baseline);
            }
            self.toolbar(None);
            return Ok(true);
        }
        doc.dispatch(Action::CommitText);
        doc.dispatch(Action::FinishPolyline);
        let crop = doc.borrow().crop();
        if crop.is_none() && !transformed {
            self.toolbar(None);
            return Ok(true);
        }
        let revision = doc.borrow().revision();
        let renderer = self
            .hosts
            .borrow()
            .as_ref()
            .and_then(|h| h.surfaces.first())
            .and_then(|s| s.window.renderer())
            .ok_or_else(|| t("The desktop service is unavailable"))?;
        let mut image =
            editor::export::frame_unfiltered(&scene, &renderer, crop, &self.pool, cancel)
                .await
                .map_err(|e| format!("{e:?}"))?;
        if transformed {
            let pixels = image.pixels.clone();
            let task = self
                .pool
                .submit(move |token| {
                    token.check()?;
                    let result = match action {
                        Output::RotateRight => clivuu_core::image::imageops::rotate90(&*pixels),
                        Output::RotateLeft => clivuu_core::image::imageops::rotate270(&*pixels),
                        Output::FlipHorizontal => {
                            clivuu_core::image::imageops::flip_horizontal(&*pixels)
                        }
                        Output::FlipVertical => {
                            clivuu_core::image::imageops::flip_vertical(&*pixels)
                        }
                        _ => unreachable!(),
                    };
                    token.check()?;
                    Ok(result)
                })
                .map_err(|e| e.to_string())?;
            let pixels = gio::CancellableFuture::new(task.result(), cancel.clone())
                .await
                .map_err(|_| t("Operation cancelled"))?
                .map_err(|e| e.to_string())?;
            image = FrozenImage::new(pixels).ok_or_else(|| t("Invalid saved pin data"))?;
        }
        let active = self.active.get() == Some(id);
        {
            let mut data = self.data.borrow_mut();
            let d = data
                .get_mut(&id)
                .ok_or_else(|| t("Invalid saved pin data"))?;
            if !d
                .editor
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, &doc))
                || doc.borrow().revision() != revision
            {
                return Err(t("Operation cancelled"));
            }
            for widget in &d.widgets {
                widget.source(&image.texture);
            }
            d.ocr.deactivate();
            d.version += revision + 1;
            d.image = image;
            d.stored = None;
            d.views.clear();
            d.editor = None;
        }
        if let Some(pin) = self.board.borrow_mut().get_mut(id) {
            if let Some(r) = crop {
                let rect = pin.rect();
                pin.x += r.x / size[0] as f64 * rect.width;
                pin.y += r.y / size[1] as f64 * rect.height;
                pin.width *= r.width / size[0] as f64;
                pin.height *= r.height / size[1] as f64;
            }
            if matches!(action, Output::RotateLeft | Output::RotateRight) {
                std::mem::swap(&mut pin.width, &mut pin.height);
            }
        }
        if active {
            if transformed {
                self.ensure_editor(id)?;
            } else {
                self.toolbar(None);
            }
        }
        self.changed();
        self.sync();
        Ok(true)
    }
    pub fn commit(self: &Rc<Self>) {
        self.changed();
        self.sync();
    }
    pub fn finish_input(self: &Rc<Self>) {
        if self.drag.borrow_mut().take().is_some() {
            self.changed();
        }
        let documents: Vec<_> = self
            .data
            .borrow()
            .values()
            .filter_map(|d| d.editor.clone())
            .collect();
        for doc in documents {
            if doc.borrow().has_active_gesture() {
                doc.dispatch(Action::End);
            }
            doc.dispatch(Action::CommitText);
        }
        self.toolbar(None);
    }
    pub fn shutdown(self: &Rc<Self>) {
        if let Some((clock, id)) = self.pending_input.borrow_mut().take() {
            clock.disconnect(id);
        }
        self.close_menu();
        self.active.set(None);
        self.feedback.replace(Default::default());
        self.drag.borrow_mut().take();
        self.data.borrow_mut().clear();
        self.hosts.borrow_mut().take();
    }
    pub fn status(&self) -> serde_json::Value {
        use serde_json::json;
        let data = self.data.borrow();
        let toolbar = self.active.get().and_then(|id| data.get(&id))
            .and_then(|d| d.views.iter().find(|v| v.toolbar().widget().is_visible() && v.toolbar().widget().width() > 0)).map(|v| {
                let c = v.scene().controller();
                let c = c.borrow();
                let controls = |row: &gtk::Box| {
                    let mut buttons = vec![];
                    if let Some(root) = v.scene().parent() {
                        let m = c.mapping();
                        let output = self.hosts.borrow();
                        let origin = output.as_ref().and_then(|h| h.surfaces.iter()
                            .find(|s| s.root.upcast_ref::<gtk::Widget>() == &root))
                            .map_or([m.display.x,m.display.y], |s| [s.rect.x,s.rect.y]);
                        let mut child = row.first_child();
                        while let Some(w) = child {
                            child = w.next_sibling();
                            if let (Some(label), Some(r)) = (w.tooltip_text(), w.compute_bounds(&root))
                                && r.width() > 0.
                                && r.height() > 0.
                            {
                                buttons.push(json!({"label":label.as_str(),"rect":{"x":r.x() as f64+origin[0],"y":r.y() as f64+origin[1],"width":r.width(),"height":r.height()}}));
                            }
                        }
                    }
                    buttons
                };
                let popovers: Vec<_> = v
                    .options()
                    .popup_bounds()
                    .and_then(|bounds| {
                        let root = v.scene().parent()?;
                        let hosts = self.hosts.borrow();
                        let origin = hosts
                            .as_ref()?
                            .surfaces
                            .iter()
                            .find(|s| s.root.upcast_ref::<gtk::Widget>() == &root)?
                            .rect;
                        Some(json!({
                            "x": bounds.x() as f64 + origin.x,
                            "y": bounds.y() as f64 + origin.y,
                            "width": bounds.width(),
                            "height": bounds.height()
                        }))
                    })
                    .into_iter()
                    .collect();
                json!({"tool":c.tool(),"crop":c.crop(),"marks":c.session().marks(),"current":c.session().current(),"canUndo":c.session().can_undo(),"canRedo":c.session().can_redo(),"panels":v.panels(),"buttons":controls(v.toolbar().row()),"options":controls(v.options().row()),"ocrBar":controls(v.ocr_bar().row()),"ocrBarVisible":v.ocr_bar().widget().is_visible(),"popovers":popovers})
            });
        let menu = self.menu.borrow().as_ref().map(|m| {
            let buttons: Vec<_> = m
                .buttons
                .iter()
                .filter(|(_, b)| b.is_mapped())
                .filter_map(|(label, b)| {
                    m.surface
                        .button_rect(b)
                        .map(|rect| json!({"label":label,"rect":rect}))
                })
                .collect();
            let opacity: Vec<_> = m
                .opacity
                .iter()
                .filter(|b| b.is_mapped())
                .filter_map(|b| {
                    m.surface
                        .button_rect(b)
                        .map(|rect| json!({"label":b.label().as_deref(),"rect":rect}))
                })
                .collect();
            json!({"rect":m.surface.frame_rect(),"buttons":buttons,"opacity":opacity})
        });
        let ocr_selected_pin = self.highlighted.get().or(self.board.borrow().selected);
        let (ocr_active_count, ocr_lines, ocr_selected_text) = (
            data.values().filter(|d| d.ocr.is_active()).count(),
            ocr_selected_pin
                .and_then(|id| data.get(&id))
                .map_or(0, |d| d.ocr.selection.lines().len()),
            ocr_selected_pin.and_then(|id| data.get(&id)).and_then(|d| {
                d.ocr
                    .selection
                    .selected_text(d.ocr.settings.smart_paragraphs)
            }),
        );
        let frame_clock = self
            .hosts
            .borrow()
            .as_ref()
            .and_then(|h| h.surfaces.first())
            .and_then(|s| s.window.frame_clock())
            .map(|clock| {
                let (interval, _) = clock.refresh_info(0);
                json!({
                    "refreshIntervalUs": interval,
                    "frameCounter": clock.frame_counter(),
                })
            });
        json!({"keyboard":self.keyboard_enabled(),"toolbar":toolbar,"menu":menu,"feedback":self.feedback.borrow().status(),"frameClock":frame_clock,"highlighted":self.highlighted.get(),"selectedPins":*self.selected_pins.borrow(),"ocrActiveCount":ocr_active_count,"ocrLines":ocr_lines,"ocrSelectedText":ocr_selected_text,"residentImages":data.len(),"closedResidentImages":self.board.borrow().pins.iter().filter(|p|p.closed&&data.contains_key(&p.id)).count()})
    }
    pub fn close_menu(self: &Rc<Self>) {
        let old = self.menu.borrow_mut().take();
        drop(old);
        self.reconcile_pointer_activation();
    }
    fn open_menu(self: &Rc<Self>, id: u64, point: [f64; 2]) -> Result<(), String> {
        if self.menu.borrow().is_some() {
            self.close_menu();
            return Ok(());
        }
        let mut buttons = vec![];
        let opacity = [
            gtk::Button::with_label("−"),
            gtk::Button::new(),
            gtk::Button::with_label("+"),
        ];
        let (amount, thumbnailed, pin_group, summaries) = {
            let board = self.board.borrow();
            let (amount, thumbnailed, pin_group) = board.get(id).map_or_else(
                || (1., false, board.group.clone()),
                |p| (p.opacity, p.thumbnail_zoom.is_some(), p.group.clone()),
            );
            (amount, thumbnailed, pin_group, board.group_summaries())
        };
        let editing = self.active.get() == Some(id);
        let (ocr_active, ocr_has_sel, ocr_has_lines) =
            self.data
                .borrow()
                .get(&id)
                .map_or((false, false, false), |d| {
                    (
                        d.ocr.is_active(),
                        d.ocr.selection.has_selection(),
                        d.ocr.is_ready() && d.ocr.selection.is_active(),
                    )
                });
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
        box_.add_css_class("clivuu-portable-menu");
        let stack = gtk::Stack::builder()
            .vhomogeneous(true)
            .hhomogeneous(true)
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(90)
            .build();
        let main_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_page.add_css_class("clivuu-menu-page");
        let mut top_group = vec![
            ("Copy image", "edit-copy-symbolic", 0),
            ("Save image", "document-save-symbolic", 1),
            ("Recognize text", "edit-find-symbolic", 2),
        ];
        if ocr_has_sel {
            top_group.push(("Copy selected text", "edit-copy-symbolic", 11));
        }
        if ocr_has_lines {
            top_group.push(("Copy all text", "edit-copy-symbolic", 12));
        }
        top_group.push(("Paste", "edit-paste-symbolic", 9));
        for (i, group) in [
            top_group,
            vec![
                ("Show toolbar", "document-edit-symbolic", 3),
                ("Thumbnail / restore", "view-restore-symbolic", 4),
                ("Reset size", "zoom-original-symbolic", 5),
            ],
            vec![
                ("Pointer passthrough", "input-mouse-symbolic", 6),
                ("Move to group", "folder-symbolic", 10),
            ],
            vec![
                ("Close pin", "window-close-symbolic", 7),
                ("Destroy pin", "user-trash-symbolic", 8),
            ],
        ]
        .into_iter()
        .enumerate()
        {
            if i > 0 {
                main_page.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            }
            if i == 2 {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 7);
                row.add_css_class("clivuu-menu-opacity");
                let icon = gtk::Image::from_icon_name("weather-clear-symbolic");
                icon.set_pixel_size(14);
                icon.add_css_class("clivuu-menu-icon");
                row.append(&icon);
                opacity[1].set_hexpand(true);
                opacity[1].add_css_class("clivuu-menu-opacity-reset");
                set_opacity_controls(&opacity, amount);
                for (b, delta) in opacity.iter().zip([Some(-0.1), None, Some(0.1)]) {
                    b.set_focusable(false);
                    let weak = Rc::downgrade(self);
                    b.connect_clicked(move |_| {
                        if let Some(this) = weak.upgrade() {
                            if let Some(p) = this.board.borrow_mut().get_mut(id) {
                                p.opacity = delta.map_or(1., |d| (p.opacity + d).clamp(0.2, 1.));
                            }
                            this.changed();
                            this.sync();
                        }
                    });
                }
                row.append(&opacity[1]);
                let stepper = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                stepper.set_valign(gtk::Align::Center);
                stepper.add_css_class("clivuu-menu-stepper");
                stepper.append(&opacity[0]);
                stepper.append(&gtk::Separator::new(gtk::Orientation::Vertical));
                stepper.append(&opacity[2]);
                row.append(&stepper);
                main_page.append(&row);
            }
            for (label, icon, action) in group {
                let b = gtk::Button::builder()
                    .focusable(false)
                    .height_request(23)
                    .tooltip_text(t(label))
                    .build();
                b.add_css_class("clivuu-menu-item");
                if action == 8 {
                    b.add_css_class("clivuu-menu-destructive");
                }
                let toggled = (action == 2 && ocr_active)
                    || (action == 3 && editing)
                    || (action == 4 && thumbnailed);
                if toggled {
                    b.add_css_class("clivuu-menu-item-active");
                }
                let content = gtk::Box::new(gtk::Orientation::Horizontal, 7);
                content.set_valign(gtk::Align::Fill);
                let image = gtk::Image::from_icon_name(icon);
                image.set_pixel_size(14);
                image.set_valign(gtk::Align::Center);
                image.add_css_class("clivuu-menu-icon");
                content.append(&image);
                let text = crate::ui::floating_bar::CenteredLabel::ellipsized(&t(label), 28);
                content.append(&text);
                if toggled {
                    let check = gtk::Image::from_icon_name("object-select-symbolic");
                    check.set_pixel_size(12);
                    check.set_valign(gtk::Align::Center);
                    check.add_css_class("clivuu-menu-check");
                    content.append(&check);
                }
                if action == 10 {
                    let chevron = gtk::Image::from_icon_name("go-next-symbolic");
                    chevron.set_pixel_size(12);
                    chevron.set_valign(gtk::Align::Center);
                    chevron.add_css_class("clivuu-menu-chevron");
                    content.append(&chevron);
                }
                let shortcut = match action {
                    0 => Some("Ctrl+C".to_owned()),
                    1 => Some("Ctrl+S".to_owned()),
                    3 => Some(t("Space")),
                    7 => Some("Ctrl+W".to_owned()),
                    8 => Some("Shift+Esc".to_owned()),
                    _ => None,
                };
                if let Some(keys) = shortcut {
                    let badge = gtk::Label::builder()
                        .label(keys)
                        .valign(gtk::Align::Center)
                        .css_classes(["clivuu-menu-shortcut"])
                        .build();
                    content.append(&badge);
                }
                b.set_child(Some(&content));
                if action == 10 {
                    let st = stack.downgrade();
                    b.connect_clicked(move |_| {
                        if let Some(stack) = st.upgrade() {
                            stack.set_visible_child_name("groups");
                        }
                    });
                } else {
                    let weak = Rc::downgrade(self);
                    b.connect_clicked(move |_| {
                        if let Some(this) = weak.upgrade() {
                            this.close_menu();
                            match action {
                                0 => (this.callback)(Event::Output(id, Output::Copy)),
                                1 => (this.callback)(Event::Output(id, Output::Save)),
                                2 => (this.callback)(Event::Output(id, Output::Ocr)),
                                3 => this.toolbar(if this.active.get() == Some(id) {
                                    None
                                } else {
                                    Some(id)
                                }),
                                4 => {
                                    if let Some(p) = this.board.borrow_mut().get_mut(id) {
                                        this.preferences.borrow().thumbnail(p);
                                    }
                                    this.changed();
                                    this.sync();
                                }
                                5 => {
                                    if let Some(p) = this.board.borrow_mut().get_mut(id) {
                                        this.preferences.borrow().reset(p);
                                    }
                                    this.changed();
                                    this.sync();
                                }
                                6 => {
                                    if let Some(p) = this.board.borrow_mut().get_mut(id) {
                                        p.through = true;
                                    }
                                    this.changed();
                                    this.sync();
                                }
                                7 => this.close(id),
                                8 => this.destroy(id),
                                9 => (this.callback)(Event::Paste),
                                11 => {
                                    let _ = this.copy_pin_ocr_text(id, true);
                                }
                                12 => {
                                    let _ = this.copy_pin_ocr_text(id, false);
                                }
                                _ => {}
                            }
                        }
                    });
                }
                buttons.push((label.to_owned(), b.clone()));
                main_page.append(&b);
            }
        }

        let groups_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        groups_page.add_css_class("clivuu-menu-page");
        let back = gtk::Button::builder()
            .focusable(false)
            .height_request(23)
            .tooltip_text(t("Back"))
            .build();
        back.add_css_class("clivuu-menu-item");
        let back_content = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        back_content.set_valign(gtk::Align::Fill);
        let back_icon = gtk::Image::from_icon_name("go-previous-symbolic");
        back_icon.set_pixel_size(14);
        back_icon.set_valign(gtk::Align::Center);
        back_icon.add_css_class("clivuu-menu-icon");
        back_content.append(&back_icon);
        let back_label =
            crate::ui::floating_bar::CenteredLabel::ellipsized(&t("Move to group"), 28);
        back_label.add_css_class("clivuu-menu-header");
        back_content.append(&back_label);
        back.set_child(Some(&back_content));
        {
            let st = stack.downgrade();
            back.connect_clicked(move |_| {
                if let Some(stack) = st.upgrade() {
                    stack.set_visible_child_name("main");
                }
            });
        }
        buttons.push(("Back".to_owned(), back.clone()));
        groups_page.append(&back);
        groups_page.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let groups_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for summary in summaries {
            let is_pin_group = summary.name == pin_group;
            let item = gtk::Button::builder()
                .focusable(false)
                .height_request(23)
                .tooltip_text(&summary.display_name)
                .build();
            item.add_css_class("clivuu-menu-item");
            if is_pin_group {
                item.add_css_class("clivuu-menu-item-active");
            }
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 7);
            row.set_valign(gtk::Align::Fill);
            let dot = group_color_dot(summary.color.as_deref(), 8);
            row.append(&dot);
            let title =
                crate::ui::floating_bar::CenteredLabel::ellipsized(&summary.display_name, 28);
            row.append(&title);
            if is_pin_group {
                let check = gtk::Image::from_icon_name("object-select-symbolic");
                check.set_pixel_size(12);
                check.set_valign(gtk::Align::Center);
                check.add_css_class("clivuu-menu-check");
                row.append(&check);
            }
            let count = gtk::Label::builder()
                .label(summary.active_pins.to_string())
                .valign(gtk::Align::Center)
                .css_classes(["clivuu-menu-count", "numeric"])
                .build();
            row.append(&count);
            item.set_child(Some(&row));
            let target_name = summary.name.clone();
            let weak = Rc::downgrade(self);
            item.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    let ids = this.shortcut_targets(id);
                    if let Err(e) = this.move_to_group(&ids, &target_name) {
                        eprintln!("{e}");
                    }
                }
            });
            buttons.push((format!("group:{}", summary.name), item.clone()));
            groups_list.append(&item);
        }
        let groups_scroll = gtk::ScrolledWindow::builder()
            .child(&groups_list)
            .overlay_scrolling(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .build();
        groups_page.append(&groups_scroll);
        groups_page.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        for (label, icon, is_new) in [
            ("Move to new group...", "list-add-symbolic", true),
            ("Manage groups...", "emblem-system-symbolic", false),
        ] {
            let b = gtk::Button::builder()
                .focusable(false)
                .height_request(23)
                .tooltip_text(t(label))
                .build();
            b.add_css_class("clivuu-menu-item");
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 7);
            content.set_valign(gtk::Align::Fill);
            let image = gtk::Image::from_icon_name(icon);
            image.set_pixel_size(14);
            image.set_valign(gtk::Align::Center);
            image.add_css_class("clivuu-menu-icon");
            content.append(&image);
            let text = crate::ui::floating_bar::CenteredLabel::ellipsized(&t(label), 28);
            content.append(&text);
            b.set_child(Some(&content));
            let weak = Rc::downgrade(self);
            b.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    let ids = this.shortcut_targets(id);
                    this.close_menu();
                    if is_new {
                        (this.callback)(Event::MoveToNewGroup(ids));
                    } else {
                        (this.callback)(Event::ManageGroups);
                    }
                }
            });
            buttons.push((label.to_owned(), b.clone()));
            groups_page.append(&b);
        }

        stack.add_named(&main_page, Some("main"));
        stack.add_named(&groups_page, Some("groups"));
        stack.set_visible_child_name("main");
        stack.set_vexpand(true);
        box_.append(&stack);
        let parents = self.hosts.borrow();
        let hosts = parents
            .as_ref()
            .ok_or_else(|| t("The desktop service is unavailable"))?;
        let owner = hosts
            .surfaces
            .iter()
            .position(|s| contains(s.rect, point))
            .unwrap_or(0);
        let monitor = hosts.surfaces[owner].rect;
        let (min_w, _, _, _) = box_.measure(gtk::Orientation::Horizontal, -1);
        let width = ((min_w + 2).max(186) as f64).min(monitor.width - 16.);
        let (min_h, nat_h, _, _) =
            box_.measure(gtk::Orientation::Vertical, (width as i32 - 2).max(1));
        let natural_height = (min_h.max(nat_h) + 2) as f64;
        let height = natural_height.min(monitor.height - 16.);
        let rect = Bounds {
            x: point[0].round().clamp(
                monitor.x + 8.,
                (monitor.x + monitor.width - width - 8.).max(monitor.x),
            ),
            y: point[1].round().clamp(
                monitor.y + 8.,
                (monitor.y + monitor.height - height - 8.).max(monitor.y),
            ),
            width,
            height,
        };
        let needs_scroll = hosts.surfaces[owner].uses_native_focus() || natural_height > height;
        let scroll = gtk::ScrolledWindow::builder()
            .child(&box_)
            .overlay_scrolling(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(if needs_scroll {
                gtk::PolicyType::Automatic
            } else {
                gtk::PolicyType::Never
            })
            .overflow(gtk::Overflow::Hidden)
            .css_classes(["clivuu-portable-menu-scroll"])
            .build();
        let weak = Rc::downgrade(self);
        let surface = crate::platform::surfaces::menu::MenuSurface::new(
            &self.display,
            hosts,
            rect,
            point,
            &scroll,
            move |outside| {
                if let Some(this) = weak.upgrade() {
                    this.close_menu();
                    if let Some(point) = outside {
                        this.activate_at(point);
                    }
                }
            },
        )?;
        drop(parents);
        self.menu.replace(Some(Menu {
            surface,
            buttons,
            pin: id,
            opacity,
        }));
        let show = self.menu.borrow().as_ref().unwrap().surface.presenter();
        show();
        Ok(())
    }

    pub fn deactivate_pin_ocr(self: &Rc<Self>, id: u64) {
        let was_active = self.data.borrow_mut().get_mut(&id).is_some_and(|d| {
            let active = d.ocr.is_active();
            d.ocr.deactivate();
            active
        });
        if was_active {
            self.sync();
        }
    }

    pub fn pin_has_active_ocr(&self, id: u64) -> bool {
        self.data
            .borrow()
            .get(&id)
            .is_some_and(|d| d.ocr.is_active())
    }

    pub fn copy_pin_ocr_text(self: &Rc<Self>, id: u64, selected_only: bool) -> bool {
        let text = {
            let data = self.data.borrow();
            let Some(d) = data.get(&id) else {
                return false;
            };
            if !d.ocr.is_ready() || !d.ocr.selection.is_active() {
                return false;
            }
            let smart = d.ocr.settings.smart_paragraphs;
            if selected_only {
                d.ocr.selection.selected_text(smart)
            } else {
                let all = d.ocr.selection.all_text(smart);
                (!all.is_empty()).then_some(all)
            }
        };
        let Some(text) = text else {
            return false;
        };
        self.display.clipboard().set_text(&text);
        self.show_feedback(id, t("Copied"));
        true
    }

    /// Toggle interactive in-place text selection on a pin. Returns `false` when `ocr-quick-copy`
    /// is enabled so the caller runs one-shot clipboard recognition instead.
    pub fn toggle_ocr(self: &Rc<Self>, id: u64) -> bool {
        let already_active = self
            .data
            .borrow()
            .get(&id)
            .is_some_and(|d| d.ocr.is_active());
        if already_active {
            self.deactivate_pin_ocr(id);
            return true;
        }
        let settings = self.ocr_settings.borrow().clone();
        if settings.quick_copy {
            return false;
        }

        if self.active.get() != Some(id) {
            self.toolbar(Some(id));
        }
        if self.active.get() != Some(id) {
            return true;
        }

        if let Some(doc) = self.data.borrow().get(&id).and_then(|d| d.editor.clone()) {
            doc.dispatch(Action::CommitText);
            doc.dispatch(Action::Tool(Tool::Select));
            doc.dispatch(Action::Deselect);
        }
        if let Some(d) = self.data.borrow_mut().get_mut(&id) {
            d.ocr.settings = settings;
        }
        self.start_pin_ocr_scan(id);
        true
    }

    fn start_pin_ocr_scan(self: &Rc<Self>, id: u64) {
        let settings = {
            let mut data = self.data.borrow_mut();
            let Some(d) = data.get_mut(&id) else {
                return;
            };
            let settings = d.ocr.settings.clone();
            let avail = clivuu_core::ocr::availability(&settings);
            if avail.needs_paddle_download(settings.engine) {
                d.ocr.enter_needs_setup();
                drop(data);
                self.sync();
                return;
            }
            settings
        };

        let Some((pixels, generation, cancel)) = ({
            let mut data = self.data.borrow_mut();
            data.get_mut(&id).map(|d| {
                let (generation, cancel) = d.ocr.begin_scan();
                (d.image.pixels.clone(), generation, cancel)
            })
        }) else {
            return;
        };

        self.sync();
        self.ensure_ocr_animation();

        let pool = self.pool.clone();
        let weak = Rc::downgrade(self);
        glib::MainContext::default().spawn_local(async move {
            let result =
                crate::infrastructure::ocr::recognize(pixels, settings, &pool, &cancel).await;
            let Some(this) = weak.upgrade() else {
                return;
            };
            let applied = {
                let mut data = this.data.borrow_mut();
                data.get_mut(&id)
                    .is_some_and(|d| d.ocr.finish_scan(generation, result))
            };
            if applied {
                this.sync();
            }
        });
    }

    fn handle_pin_ocr_bar_action(
        self: &Rc<Self>,
        id: u64,
        action: crate::ui::ocr_bar::OcrBarAction,
    ) {
        use crate::ui::ocr_bar::OcrBarAction;
        match action {
            OcrBarAction::SelectEngine(engine, model_id) => {
                {
                    let mut settings = self.ocr_settings.borrow_mut();
                    settings.engine = engine;
                    if engine != clivuu_core::ocr::OcrEngineChoice::Tesseract {
                        settings.model_id = model_id.clone();
                    }
                }
                if let Some(d) = self.data.borrow_mut().get_mut(&id) {
                    d.ocr.settings.engine = engine;
                    d.ocr.settings.model_id = model_id.clone();
                }
                if let Some(cfg) = self.live_config.borrow().as_ref() {
                    let _ = cfg.set("ocr-engine", serde_json::Value::from(engine.as_str()));
                    if engine != clivuu_core::ocr::OcrEngineChoice::Tesseract {
                        let _ = cfg.set("ocr-model", serde_json::Value::from(model_id));
                    }
                }
                self.start_pin_ocr_scan(id);
            }
            OcrBarAction::ToggleParagraphs => {
                let next = {
                    let mut data = self.data.borrow_mut();
                    let Some(d) = data.get_mut(&id) else {
                        return;
                    };
                    d.ocr.settings.smart_paragraphs = !d.ocr.settings.smart_paragraphs;
                    d.ocr.settings.smart_paragraphs
                };
                self.ocr_settings.borrow_mut().smart_paragraphs = next;
                if let Some(cfg) = self.live_config.borrow().as_ref() {
                    let _ = cfg.set("ocr-smart-paragraphs", serde_json::Value::from(next));
                }
                self.sync();
            }
            OcrBarAction::SelectAll => {
                let changed = self
                    .data
                    .borrow_mut()
                    .get_mut(&id)
                    .is_some_and(|d| d.ocr.selection.select_all());
                if changed {
                    self.sync();
                }
            }
            OcrBarAction::CopyText => {
                let text = {
                    let mut data = self.data.borrow_mut();
                    let Some(d) = data.get_mut(&id) else {
                        return;
                    };
                    let text = d.ocr.text_to_copy();
                    if text.is_some() {
                        d.ocr.mark_copied();
                    }
                    text
                };
                if let Some(text) = text {
                    self.display.clipboard().set_text(&text);
                    self.show_feedback(id, t("Copied"));
                    self.sync();
                    let weak = Rc::downgrade(self);
                    glib::timeout_add_local_once(
                        std::time::Duration::from_millis(1250),
                        move || {
                            if let Some(this) = weak.upgrade() {
                                this.sync();
                            }
                        },
                    );
                }
            }
            OcrBarAction::PinText => {}
            OcrBarAction::Retry => {
                self.start_pin_ocr_scan(id);
            }
            OcrBarAction::Cancel => {
                self.deactivate_pin_ocr(id);
            }
            OcrBarAction::OpenSettings => {
                (self.callback)(Event::Output(id, Output::OcrPreferences));
            }
        }
    }

    fn ensure_ocr_animation(self: &Rc<Self>) {
        if self.ocr_animating.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(std::time::Duration::from_millis(24), move || {
            let Some(this) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let data = this.data.borrow();
            let mut any_scanning = false;
            for d in data.values() {
                if d.ocr.is_scanning() {
                    any_scanning = true;
                    for w in &d.ocr_widgets {
                        if w.is_visible() {
                            w.queue_draw();
                        }
                    }
                }
            }
            drop(data);
            if !any_scanning {
                this.ocr_animating.set(false);
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }
}

// Use one owner for controls and keyboard, including gaps between outputs.
fn output_owner(hosts: &Hosts, rect: Bounds) -> usize {
    let p = rect.center();
    hosts
        .surfaces
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let distance = |r: Bounds| {
                (p[0] - p[0].clamp(r.x, r.x + r.width)).powi(2)
                    + (p[1] - p[1].clamp(r.y, r.y + r.height)).powi(2)
            };
            distance(a.rect).total_cmp(&distance(b.rect))
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn set_opacity_controls(opacity: &[gtk::Button; 3], amount: f64) {
    opacity[0].set_sensitive(amount > 0.205);
    opacity[2].set_sensitive(amount < 0.995);
    opacity[1].set_label(&tf(
        "Opacity: {0}%",
        &[&(amount * 100.).round().to_string()],
    ));
    if let Some(label) = opacity[1].child().and_downcast::<gtk::Label>() {
        label.set_xalign(0.);
        label.add_css_class("numeric");
    }
}

pub(crate) fn group_color_dot(color: Option<&str>, size: i32) -> gtk::DrawingArea {
    let dot = gtk::DrawingArea::builder()
        .content_width(size)
        .content_height(size)
        .valign(gtk::Align::Center)
        .build();
    let parsed = color.and_then(|hex| gdk::RGBA::parse(hex).ok());
    dot.set_draw_func(move |_, cr, width, height| {
        let cx = width as f64 / 2.;
        let cy = height as f64 / 2.;
        let r = (width.min(height) as f64 / 2. - 1.).max(2.);
        cr.arc(cx, cy, r, 0., std::f64::consts::TAU);
        if let Some(rgba) = parsed {
            cr.set_source_rgba(
                rgba.red().into(),
                rgba.green().into(),
                rgba.blue().into(),
                rgba.alpha().into(),
            );
            let _ = cr.fill();
        } else {
            cr.set_source_rgba(0.55, 0.58, 0.66, 0.55);
            cr.set_line_width(1.4);
            let _ = cr.stroke();
        }
    });
    dot
}

fn pin_css(dark: bool, accent: &str) -> String {
    let accent = gdk::RGBA::parse(accent)
        .unwrap_or_else(|_| gdk::RGBA::new(0.741, 0.576, 0.976, 1.))
        .to_string();
    let tokens = if dark {
        [
            ("@pin_menu_bg", "#252732"),
            ("@pin_menu_fg", "#f2f3f7"),
            ("@pin_menu_muted", "#969cb0"),
            ("@pin_menu_border", "#3c4052"),
            ("@pin_menu_highlight", "alpha(#ffffff, 0.06)"),
            ("@pin_menu_sep", "#353949"),
            ("@pin_menu_hover", "#323646"),
            ("@pin_menu_press", "#3c4154"),
            ("@pin_stepper_bg", "#1d1f28"),
            ("@pin_stepper_border", "#383c4d"),
            ("@pin_danger_fg", "#ff7a7a"),
            ("@pin_danger_hover", "alpha(#ff5555, 0.15)"),
            ("@pin_active_bg", "alpha(@pin_accent, 0.16)"),
            ("@pin_active_hover", "alpha(@pin_accent, 0.24)"),
        ]
    } else {
        [
            ("@pin_menu_bg", "#fbfbfd"),
            ("@pin_menu_fg", "#1f232b"),
            ("@pin_menu_muted", "#687082"),
            ("@pin_menu_border", "#d5d8e2"),
            ("@pin_menu_highlight", "alpha(#ffffff, 0.85)"),
            ("@pin_menu_sep", "#e5e7ef"),
            ("@pin_menu_hover", "#edeff5"),
            ("@pin_menu_press", "#e1e4ee"),
            ("@pin_stepper_bg", "#f0f2f7"),
            ("@pin_stepper_border", "#d4d8e2"),
            ("@pin_danger_fg", "#cf2b2b"),
            ("@pin_danger_hover", "alpha(#d93025, 0.10)"),
            ("@pin_active_bg", "alpha(@pin_accent, 0.14)"),
            ("@pin_active_hover", "alpha(@pin_accent, 0.21)"),
        ]
    };
    let mut css = include_str!("../../resources/pins.css").to_owned();
    for (key, value) in tokens {
        css = css.replace(key, value);
    }
    css.replace("@pin_accent", &accent)
}
