//! Capture-specific geometry around the common editor. These widgets embed
//! into platform-owned surfaces; there is no second editor/toolbar window.
use super::{
    Action, EditorController, ImageMapping, Tool,
    capture_draw::{self, CaptureStyle},
    frozen_image::FrozenImage,
    ocr_layer::{self, OcrOverlayState, OcrPhase},
    scene::{Controller, EditorDocument, Input, Scene},
    view::EditorView,
};
use crate::{
    infrastructure::{
        configuration::Configuration,
        ocr::OcrDownloadManager,
        tasks::{TaskError, TaskPool},
    },
    ui::{
        color::ColorServices,
        ocr_bar::OcrBarAction,
        toolbar::{Output as Command, Theme},
        viewport::Viewport,
    },
};
use clivuu_core::{
    annotation::Bounds,
    capture::{Purpose, Selection, intersect},
    ocr::{OcrEngineChoice, OcrSettings},
};
use gtk::{gdk, glib, prelude::*, subclass::prelude::*};
use std::{
    cell::{Cell, Ref, RefCell},
    rc::{Rc, Weak},
};
type Paint = Box<dyn Fn(&gtk::Snapshot)>;
mod imp {
    use super::*;
    #[derive(Default)]
    pub struct Canvas {
        pub paint: RefCell<Option<Paint>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "ClivuuCaptureFeedback";
        type Type = super::Canvas;
        type ParentType = gtk::Widget;
    }
    impl ObjectImpl for Canvas {}
    impl WidgetImpl for Canvas {
        fn snapshot(&self, s: &gtk::Snapshot) {
            if let Some(paint) = self.paint.borrow().as_ref() {
                paint(s);
            }
        }
    }
}
glib::wrapper! {pub struct Canvas(ObjectSubclass<imp::Canvas>) @extends gtk::Widget,@implements gtk::Accessible,gtk::Buildable,gtk::ConstraintTarget;}
impl Canvas {
    pub(crate) fn set_painter(&self, paint: impl Fn(&gtk::Snapshot) + 'static) {
        self.imp().paint.replace(Some(Box::new(paint)));
        self.queue_draw();
    }
    pub(crate) fn new(paint: impl Fn(&gtk::Snapshot) + 'static) -> Self {
        let this: Self = glib::Object::new();
        this.set_can_target(false);
        this.imp().paint.replace(Some(Box::new(paint)));
        this
    }
}
pub struct CaptureDocument {
    pub editor: Controller,
    pub image: RefCell<FrozenImage>,
    cursor_frames: RefCell<Option<(FrozenImage, FrozenImage)>>,
    cursor_visible: Cell<bool>,
    pool: Rc<TaskPool>,
    alt: Cell<u8>,
    crosshair_toggle: Cell<bool>,
    auxiliary: Cell<bool>,
    toolbar_visible: Cell<bool>,
    selection: RefCell<Selection>,
    style: RefCell<CaptureStyle>,
    views: RefCell<Vec<Weak<View>>>,
    hex: Cell<bool>,
    history: RefCell<Vec<Bounds>>,
    history_index: Cell<isize>,
    history_loop: Cell<bool>,
    last_area: Cell<Option<Bounds>>,
    hide_single: Cell<bool>,
    ocr: RefCell<OcrOverlayState>,
    ocr_quick_copy: Cell<bool>,
    ocr_text_pin: RefCell<Option<String>>,
    ocr_confirmed_text: RefCell<Option<String>>,
    live_config: RefCell<Option<Rc<Configuration>>>,
    dark: Cell<bool>,
    scan_animating: Cell<bool>,
    command: Box<dyn Fn(Command)>,
    purpose: Cell<Purpose>,
}
impl CaptureDocument {
    pub fn pin(&self) {
        self.command(Command::Pin);
    }
    pub fn set_purpose(&self, purpose: Purpose) {
        self.purpose.set(purpose);
    }
    pub fn set_ocr_config(
        self: &Rc<Self>,
        settings: OcrSettings,
        force_quick_copy: bool,
        live: Option<Rc<Configuration>>,
    ) {
        self.ocr_quick_copy
            .set(force_quick_copy || settings.quick_copy);
        self.ocr.borrow_mut().settings = settings;
        self.live_config.replace(live);
    }
    pub fn take_pinned_text(&self) -> Option<String> {
        self.ocr_text_pin.borrow_mut().take()
    }
    pub fn take_confirmed_ocr_text(&self) -> Option<String> {
        self.ocr_confirmed_text.borrow_mut().take()
    }
    pub fn ocr_status(&self) -> serde_json::Value {
        let ocr = self.ocr.borrow();
        let phase = match &ocr.phase {
            super::ocr_layer::OcrPhase::Inactive => "inactive",
            super::ocr_layer::OcrPhase::NeedsSetup => "needs_setup",
            super::ocr_layer::OcrPhase::Scanning { .. } => "scanning",
            super::ocr_layer::OcrPhase::Ready => "ready",
            super::ocr_layer::OcrPhase::Failed(_) => "failed",
        };
        serde_json::json!({
            "active": ocr.is_active(),
            "phase": phase,
            "lines": ocr.selection.document().map_or(0, |d| d.lines.len()),
        })
    }
    pub fn deactivate_ocr(&self) {
        if self.ocr.borrow().is_active() {
            self.ocr.borrow_mut().deactivate();
            self.refresh();
        }
    }
    pub fn toggle_interactive_ocr(self: &Rc<Self>) {
        if !self.purpose.get().editable() {
            return;
        }
        if self.ocr.borrow().is_active() {
            self.deactivate_ocr();
            return;
        }
        if !self.selection.borrow().selected || self.raster().is_none() {
            return;
        }
        self.editor.dispatch(Action::CommitText);
        self.editor.dispatch(Action::Tool(Tool::Select));
        self.editor.dispatch(Action::Deselect);
        self.start_ocr_scan();
    }
    fn start_ocr_scan(self: &Rc<Self>) {
        let Some(raster) = self.raster() else {
            return;
        };
        let settings = self.ocr.borrow().settings.clone();
        let avail = clivuu_core::ocr::availability(&settings);
        if avail.needs_paddle_download(settings.engine) {
            self.ocr.borrow_mut().enter_needs_setup();
            self.refresh();
            return;
        }
        let (generation, cancel) = self.ocr.borrow_mut().begin_scan();
        self.refresh();
        self.ensure_scan_animation();

        let source = self.image.borrow().pixels.clone();
        let pool = self.pool.clone();
        let weak = Rc::downgrade(self);
        glib::MainContext::default().spawn_local(async move {
            let crop_res = async {
                let task = pool
                    .submit(move |token| {
                        token.check()?;
                        let cropped = clivuu_core::image_ops::crop_capture(&source, raster)
                            .map_err(|e| TaskError::Failed(e.to_string()))?;
                        token.check()?;
                        Ok(std::sync::Arc::new(cropped))
                    })
                    .map_err(|e| e.to_string())?;
                gtk::gio::CancellableFuture::new(task.result(), cancel.clone())
                    .await
                    .map_err(|_| clivuu_core::i18n::t("Operation cancelled"))?
                    .map_err(|e| e.to_string())
            }
            .await;

            let result = match crop_res {
                Ok(cropped) => {
                    crate::infrastructure::ocr::recognize(cropped, settings, &pool, &cancel).await
                }
                Err(err) => Err(err),
            };
            if let Some(doc) = weak.upgrade()
                && doc.ocr.borrow_mut().finish_scan(generation, result)
            {
                doc.refresh();
            }
        });
    }
    fn ensure_scan_animation(self: &Rc<Self>) {
        if self.scan_animating.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(std::time::Duration::from_millis(24), move || {
            let Some(doc) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !doc.ocr.borrow().is_scanning() {
                doc.scan_animating.set(false);
                doc.refresh();
                return glib::ControlFlow::Break;
            }
            for view in doc.views.borrow().iter().filter_map(Weak::upgrade) {
                view.ocr_canvas.queue_draw();
            }
            glib::ControlFlow::Continue
        });
    }
    fn handle_ocr_bar_action(self: &Rc<Self>, action: OcrBarAction) {
        match action {
            OcrBarAction::SelectEngine(engine, model_id) => {
                {
                    let mut ocr = self.ocr.borrow_mut();
                    ocr.settings.engine = engine;
                    ocr.settings.model_id = model_id.clone();
                }
                if let Some(cfg) = self.live_config.borrow().as_ref() {
                    let _ = cfg.set("ocr-engine", serde_json::Value::from(engine.as_str()));
                    if engine != OcrEngineChoice::Tesseract {
                        let _ = cfg.set("ocr-model", serde_json::Value::from(model_id));
                    }
                }
                self.start_ocr_scan();
            }
            OcrBarAction::ToggleParagraphs => {
                let next = {
                    let mut ocr = self.ocr.borrow_mut();
                    ocr.settings.smart_paragraphs = !ocr.settings.smart_paragraphs;
                    ocr.settings.smart_paragraphs
                };
                if let Some(cfg) = self.live_config.borrow().as_ref() {
                    let _ = cfg.set("ocr-smart-paragraphs", serde_json::Value::from(next));
                }
                self.refresh();
            }
            OcrBarAction::SelectAll => {
                if self.ocr.borrow_mut().selection.select_all() {
                    self.refresh();
                }
            }
            OcrBarAction::CopyText => {
                self.copy_ocr_text_in_place();
            }
            OcrBarAction::PinText => {
                if let Some(text) = self.ocr.borrow().text_to_copy() {
                    self.ocr_text_pin.replace(Some(text));
                    (self.command)(Command::PinText);
                }
            }
            OcrBarAction::Retry => {
                self.start_ocr_scan();
            }
            OcrBarAction::Cancel => {
                self.deactivate_ocr();
            }
            OcrBarAction::OpenSettings => {
                (self.command)(Command::OcrPreferences);
            }
        }
    }
    fn copy_ocr_text_in_place(self: &Rc<Self>) -> bool {
        let Some(text) = self.ocr.borrow().text_to_copy() else {
            return false;
        };
        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_text(&text);
            self.ocr.borrow_mut().mark_copied();
            self.refresh();
            let weak = Rc::downgrade(self);
            glib::timeout_add_local_once(std::time::Duration::from_millis(1250), move || {
                if let Some(doc) = weak.upgrade() {
                    doc.refresh();
                }
            });
            return true;
        }
        false
    }
    fn command(&self, command: Command) {
        let command = match (self.purpose.get(), command) {
            (Purpose::Screenshot | Purpose::Image, Command::Confirm | Command::Copy) => {
                Command::Copy
            }
            (Purpose::Scrolling, Command::Confirm) => Command::Scroll,
            (Purpose::Recording, Command::Confirm) => Command::Record,
            (_, Command::Close) => Command::Close,
            (Purpose::Image, Command::Scroll | Command::Record) => return,
            (purpose, command) if purpose.editable() => command,
            _ => return,
        };
        (self.command)(command);
    }
    pub fn set_history(&self, areas: Vec<Bounds>, cycle: bool) {
        self.history.replace(areas);
        self.history_loop.set(cycle);
    }
    pub fn set_last_area(&self, rect: Option<Bounds>) {
        self.last_area.set(rect);
    }
    pub fn snapshot(&self) -> crate::application::capture::Snapshot {
        let pair = self.cursor_frames.borrow();
        crate::application::capture::Snapshot {
            source: self.image.borrow().pixels.clone(),
            cursor_pair: pair
                .as_ref()
                .map(|(with_c, without_c)| (with_c.pixels.clone(), without_c.pixels.clone())),
            cursor_visible: self.cursor_visible.get(),
            selection: self.selection.borrow().clone(),
            editor: self.editor.checkpoint(),
        }
    }
    pub fn restore(&self, saved: &crate::application::capture::Snapshot) {
        self.selection.replace(saved.selection.clone());
        self.editor.restore_checkpoint(saved.editor.clone());
        if let Some((ref with_c, ref without_c)) = saved.cursor_pair
            && let (Some(with_img), Some(without_img)) = (
                FrozenImage::from_shared(with_c.clone()),
                FrozenImage::from_shared(without_c.clone()),
            )
        {
            self.set_cursor_frames(Some((with_img, without_img)), saved.cursor_visible);
        } else {
            self.replace_image(FrozenImage::from_shared(saved.source.clone()).unwrap());
        }
    }
    pub fn hide_single_pixel(&self, hide: bool) {
        self.hide_single.set(hide);
        self.refresh();
    }
    pub fn selection(&self) -> Ref<'_, Selection> {
        self.selection.borrow()
    }
    pub fn raster(&self) -> Option<clivuu_core::capture::RasterRegion> {
        let selection = self.selection.borrow();
        clivuu_core::capture::raster_region(
            if self.purpose.get() == Purpose::Image {
                &[]
            } else {
                &selection.outputs
            },
            selection.limit,
            self.image.borrow().dimensions(),
            selection.rect,
        )
    }
    fn pixel_step(&self) -> [f64; 2] {
        if let Some(raster) = self.raster() {
            let rect = self.selection.borrow().rect;
            [
                rect.width / f64::from(raster.size[0]),
                rect.height / f64::from(raster.size[1]),
            ]
        } else {
            let mapping = self.editor.borrow().mapping();
            [
                mapping.display.width / mapping.document[0],
                mapping.display.height / mapping.document[1],
            ]
        }
    }
    pub fn new(
        image: FrozenImage,
        selection: Selection,
        context: &gtk::pango::Context,
        pool: Rc<TaskPool>,
        command: impl Fn(Command) + 'static,
    ) -> Rc<Self> {
        let context = context.clone();
        let editor = EditorDocument::new(
            EditorController::new(ImageMapping {
                display: selection.limit,
                document: image.dimensions().map(f64::from),
            })
            .with_text_measurer(move |m, u| super::render::text_metrics(&context, m, u)),
        );
        editor.set_shared_source(image.pixels.clone(), pool.clone());
        let this = Rc::new(Self {
            editor,
            image: RefCell::new(image),
            cursor_frames: RefCell::new(None),
            cursor_visible: Cell::new(true),
            pool,
            alt: Cell::new(0),
            crosshair_toggle: Cell::new(false),
            auxiliary: Cell::new(false),
            toolbar_visible: Cell::new(true),
            selection: RefCell::new(selection),
            style: RefCell::new(CaptureStyle::default()),
            views: RefCell::new(vec![]),
            hex: Cell::new(false),
            history: RefCell::new(vec![]),
            history_index: Cell::new(-1),
            history_loop: Cell::new(false),
            last_area: Cell::new(None),
            hide_single: Cell::new(true),
            ocr: RefCell::new(OcrOverlayState::default()),
            ocr_quick_copy: Cell::new(false),
            ocr_text_pin: RefCell::new(None),
            ocr_confirmed_text: RefCell::new(None),
            live_config: RefCell::new(None),
            dark: Cell::new(true),
            scan_animating: Cell::new(false),
            command: Box::new(command),
            purpose: Cell::new(Purpose::Screenshot),
        });
        let weak = Rc::downgrade(&this);
        OcrDownloadManager::shared().connect_changed(move || {
            let Some(doc) = weak.upgrade() else {
                return;
            };
            if matches!(doc.ocr.borrow().phase, OcrPhase::NeedsSetup) {
                let settings = doc.ocr.borrow().settings.clone();
                if !clivuu_core::ocr::availability(&settings).needs_paddle_download(settings.engine)
                {
                    doc.start_ocr_scan();
                } else {
                    doc.refresh();
                }
            }
        });
        this
    }
    pub fn set_area(&self, rect: Bounds) {
        self.deactivate_ocr();
        let mut s = self.selection.borrow_mut();
        let r = intersect(rect, s.limit);
        if r.width > 0. && r.height > 0. {
            s.rect = r;
            s.selected = true;
            s.drag = None;
        }
    }
    fn auto_lens_visible(&self) -> bool {
        let selection = self.selection.borrow();
        if selection.show_lens() {
            return true;
        }
        if self.ocr.borrow().is_active() {
            return false;
        }
        let model = self.editor.borrow();
        if model.tool() != Tool::Select || model.has_active_gesture() {
            return false;
        }
        selection.show_lens_in_select()
            && selection.pointer.is_some_and(|p| {
                model
                    .session()
                    .target(
                        model.mapping().point(p),
                        None,
                        model.mapping().screen_scale(),
                    )
                    .is_none()
            })
    }
    pub fn lens_visible(&self) -> bool {
        self.alt.get() != 0
            || (self.auto_lens_visible()
                && !self
                    .views
                    .borrow()
                    .iter()
                    .filter_map(Weak::upgrade)
                    .any(|v| v.pointer_over_ui.get() || v.editor.options().has_popup()))
    }
    pub fn can_toggle_cursor(&self) -> bool {
        self.purpose.get() == Purpose::Screenshot && self.cursor_frames.borrow().is_some()
    }
    pub fn is_cursor_visible(&self) -> bool {
        self.cursor_visible.get()
    }
    pub fn set_cursor_frames(&self, pair: Option<(FrozenImage, FrozenImage)>, visible: bool) {
        if let Some((ref with_cursor, ref without_cursor)) = pair {
            let active = if visible { with_cursor } else { without_cursor }.clone();
            self.cursor_frames.replace(pair);
            self.cursor_visible.set(visible);
            self.apply_image(active);
        } else {
            self.cursor_frames.replace(None);
            self.cursor_visible.set(true);
            self.refresh();
        }
    }
    pub fn toggle_cursor_visible(&self) -> bool {
        let next = !self.cursor_visible.get();
        let target = {
            let pair = self.cursor_frames.borrow();
            let Some((with_cursor, without_cursor)) = pair.as_ref() else {
                return false;
            };
            if next {
                with_cursor.clone()
            } else {
                without_cursor.clone()
            }
        };
        self.cursor_visible.set(next);
        self.apply_image(target);
        true
    }
    pub fn replace_image(&self, image: FrozenImage) {
        self.cursor_frames.replace(None);
        self.cursor_visible.set(true);
        self.apply_image(image);
    }
    fn apply_image(&self, image: FrozenImage) {
        self.deactivate_ocr();
        self.editor
            .set_shared_source(image.pixels.clone(), self.pool.clone());
        let display = self.selection.borrow().limit;
        self.editor.set_mapping(ImageMapping {
            display,
            document: image.dimensions().map(f64::from),
        });
        for view in self.views.borrow().iter().filter_map(Weak::upgrade) {
            view.editor.scene().replace_texture(&image.texture);
        }
        self.image.replace(image);
        self.refresh();
    }
    /// Effective display-only feedback, after capture-phase and key policy.
    pub fn feedback_style(&self) -> CaptureStyle {
        let mut style = self.style.borrow().clone();
        let selected = self.selection.borrow().selected;
        style.crosshair = !selected && (style.crosshair ^ self.crosshair_toggle.get());
        style.guides &= selected && self.auxiliary.get();
        style
    }
    pub fn set_style(&self, style: CaptureStyle) {
        self.style.replace(style);
        self.refresh();
    }
    pub fn set_theme(&self, theme: &Theme) {
        self.dark.set(theme.is_dark());
        self.style.borrow_mut().accent = theme.decoration_style().accent;
        for view in self.views.borrow().iter().filter_map(Weak::upgrade) {
            view.editor.set_theme(theme);
        }
        self.refresh();
    }
    pub fn refresh(&self) {
        self.views
            .borrow_mut()
            .retain(|view| view.strong_count() > 0);
        for view in self.views.borrow().iter().filter_map(Weak::upgrade) {
            view.sync();
        }
    }
    fn input(self: &Rc<Self>, scene: &Scene, input: &Input) -> bool {
        match input {
            Input::Secondary { .. } => {
                if self.ocr.borrow().is_active() {
                    self.deactivate_ocr();
                    return true;
                }
                if self.selection.borrow().selected {
                    self.editor.dispatch(Action::Clear);
                    self.selection.borrow_mut().clear();
                    self.refresh();
                } else {
                    self.command(Command::Close);
                }
                true
            }
            Input::Begin { point, clicks, .. } => {
                if self.ocr.borrow().is_active() && self.alt.get() == 0 {
                    let rect = self.selection.borrow().rect;
                    if self.ocr.borrow_mut().handle_press(rect, *point, *clicks) {
                        self.refresh();
                        return true;
                    }
                    let margin = capture_draw::resize_margin(&self.style.borrow(), 8.);
                    if self.selection.borrow().cursor(*point, margin) != "move"
                        || !rect.contains(*point, 0.)
                    {
                        self.deactivate_ocr();
                    }
                }
                let model = self.editor.borrow();
                let mapping = model.mapping();
                if *clicks == 2 && model.tool() == Tool::Select && !self.ocr.borrow().is_active() {
                    let text = model
                        .session()
                        .hit_kind(
                            mapping.point(*point),
                            clivuu_core::annotation::Kind::Text,
                            6. / mapping.screen_scale(),
                        )
                        .is_some();
                    if !text {
                        drop(model);
                        self.command(Command::Confirm);
                        return true;
                    }
                }
                let selected = self.selection.borrow().selected;
                let target = selected
                    && !self.ocr.borrow().is_active()
                    && model
                        .session()
                        .target(
                            mapping.point(*point),
                            model.tool().kind(),
                            mapping.screen_scale(),
                        )
                        .is_some();
                let annotate = selected && model.tool().kind().is_some();
                drop(model);
                if target || annotate {
                    return false;
                }
                self.deactivate_ocr();
                self.editor.dispatch(Action::Deselect);
                let margin = capture_draw::resize_margin(&self.style.borrow(), 8.);
                self.selection.borrow_mut().begin(*point, margin, false);
                self.refresh();
                true
            }
            Input::Move { point, .. } => {
                if self.ocr.borrow().dragging {
                    let rect = self.selection.borrow().rect;
                    if self.ocr.borrow_mut().handle_motion(rect, *point) {
                        self.refresh();
                    }
                    return true;
                }
                if self.selection.borrow().drag.is_none() {
                    return false;
                }
                self.selection.borrow_mut().motion(*point);
                self.refresh();
                true
            }
            Input::End => {
                if self.ocr.borrow_mut().handle_release() {
                    self.refresh();
                    return true;
                }
                if self.selection.borrow().drag.is_none() {
                    return false;
                }
                self.selection.borrow_mut().end();
                if self.purpose.get() == Purpose::Recording && self.selection.borrow().selected {
                    self.command(Command::Confirm);
                } else {
                    self.refresh();
                }
                true
            }
            Input::Hover { point, pressed } => {
                if !*pressed && self.editor.borrow().is_polyline_drawing() {
                    self.selection.borrow_mut().pointer = Some(*point);
                    self.refresh();
                    return false;
                }
                if *pressed {
                    self.selection.borrow_mut().pointer = Some(*point);
                    if self.ocr.borrow().dragging {
                        scene.set_named_cursor(Some("text"));
                        return true;
                    }
                    if self.selection.borrow().drag.is_some() {
                        let cursor = self.selection.borrow().cursor(
                            *point,
                            capture_draw::resize_margin(&self.style.borrow(), 8.),
                        );
                        scene.set_named_cursor(Some(cursor));
                        // GestureDrag::drag_update dispatches Input::Move on this
                        // same motion event; avoid running a stale pre-move sync.
                        return true;
                    }
                } else {
                    self.selection.borrow_mut().motion(*point);
                }
                if self.ocr.borrow().is_active() && self.alt.get() == 0 {
                    let rect = self.selection.borrow().rect;
                    if self.ocr.borrow().line_at(rect, *point).is_some() {
                        self.refresh();
                        scene.set_named_cursor(Some("text"));
                        return true;
                    }
                }
                let selection = self.selection.borrow();
                let model = self.editor.borrow();
                let target = selection.selected
                    && !self.ocr.borrow().is_active()
                    && model
                        .session()
                        .target(
                            model.mapping().point(*point),
                            model.tool().kind(),
                            model.mapping().screen_scale(),
                        )
                        .is_some();
                let dragging = selection.drag.is_some();
                let geometry =
                    dragging || !target && (!selection.selected || model.tool() == Tool::Select);
                let cursor = selection.cursor(
                    *point,
                    capture_draw::resize_margin(&self.style.borrow(), 8.),
                );
                drop(model);
                drop(selection);
                self.refresh();
                if geometry {
                    scene.set_named_cursor(Some(cursor));
                    true
                } else {
                    false
                }
            }
            Input::Wheel(delta) => {
                if self.selection.borrow().selected {
                    return false;
                }
                if delta.is_finite() && *delta != 0. {
                    self.selection
                        .borrow_mut()
                        .scroll_target(if *delta < 0. { -1 } else { 1 });
                    self.refresh();
                }
                true
            }
            Input::KeyReleased(key) => {
                if matches!(*key, gdk::Key::Alt_L | gdk::Key::Alt_R) {
                    self.alt
                        .set(self.alt.get() & if *key == gdk::Key::Alt_L { !1 } else { !2 });
                    self.refresh();
                    return true;
                }
                false
            }
            Input::Key(key, modifiers) => {
                use super::shortcuts;
                let m = shortcuts::modifiers(*modifiers);
                if self.ocr.borrow().is_active() {
                    if m == gdk::ModifierType::CONTROL_MASK {
                        match key.to_lower() {
                            gdk::Key::c => {
                                if self.copy_ocr_text_in_place() {
                                    return true;
                                }
                            }
                            gdk::Key::a => {
                                if self.ocr.borrow_mut().selection.select_all() {
                                    self.refresh();
                                }
                                return true;
                            }
                            _ => {}
                        }
                    } else if m.is_empty() && matches!(*key, gdk::Key::Return | gdk::Key::KP_Enter)
                    {
                        if let Some(text) = self.ocr.borrow().text_to_copy() {
                            self.ocr_confirmed_text.replace(Some(text));
                            self.deactivate_ocr();
                            (self.command)(Command::ConfirmOcrText);
                        }
                        return true;
                    } else if m.is_empty() && *key == gdk::Key::Escape {
                        let cleared = self.ocr.borrow_mut().selection.deselect();
                        if cleared {
                            self.refresh();
                        } else {
                            self.deactivate_ocr();
                        }
                        return true;
                    }
                }
                if (*key == gdk::Key::grave && m.is_empty()
                    || *key == gdk::Key::exclam
                        && (m.is_empty() || m == gdk::ModifierType::SHIFT_MASK))
                    && self.can_toggle_cursor()
                {
                    self.toggle_cursor_visible();
                    return true;
                }
                if m == gdk::ModifierType::SHIFT_MASK && matches!(*key, gdk::Key::o | gdk::Key::O) {
                    self.deactivate_ocr();
                    self.command(Command::Ocr);
                    return true;
                }
                let editing = self.editor.borrow().tool().kind().is_some();
                if let Some(action) = shortcuts::output(*key, *modifiers) {
                    self.command(action);
                    return true;
                }
                if let Some(delta) = shortcuts::direction(*key) {
                    self.deactivate_ocr();
                    if m == gdk::ModifierType::CONTROL_MASK || m == gdk::ModifierType::SHIFT_MASK {
                        let pixel = self.pixel_step();
                        self.selection.borrow_mut().adjust_edge(
                            delta,
                            m == gdk::ModifierType::SHIFT_MASK,
                            pixel,
                        );
                    } else if m.is_empty() && !editing {
                        let pixel = self.pixel_step();
                        self.selection
                            .borrow_mut()
                            .nudge([delta[0] * pixel[0], delta[1] * pixel[1]], false);
                    } else {
                        return false;
                    }
                    self.refresh();
                    return true;
                }
                if m == gdk::ModifierType::CONTROL_MASK {
                    match key.to_lower() {
                        gdk::Key::t => self.pin(),
                        gdk::Key::a => {
                            let mut selection = self.selection.borrow_mut();
                            let screen = selection.outputs[selection
                                .output_at(selection.pointer.unwrap_or(selection.rect.center()))]
                            .rect;
                            selection.rect = if selection.selected && selection.rect == screen {
                                selection.limit
                            } else {
                                screen
                            };
                            selection.selected = true;
                            selection.drag = None;
                        }
                        _ => return !self.purpose.get().editable(),
                    }
                } else if m.is_empty()
                    || (m & !(gdk::ModifierType::ALT_MASK | gdk::ModifierType::SHIFT_MASK))
                        .is_empty()
                        && matches!(*key, gdk::Key::c | gdk::Key::C)
                        && self.lens_visible()
                    || matches!(
                        *key,
                        gdk::Key::Alt_L | gdk::Key::Alt_R | gdk::Key::Shift_L | gdk::Key::Shift_R
                    )
                {
                    match *key {
                        gdk::Key::Escape => self.command(Command::Close),
                        gdk::Key::Return | gdk::Key::KP_Enter => self.command(Command::Confirm),
                        gdk::Key::Alt_L | gdk::Key::Alt_R => {
                            // Toggle once per press, independently of holding Alt
                            // for the magnifier and of the other Alt key.
                            let bit = if *key == gdk::Key::Alt_L { 1 } else { 2 };
                            if self.alt.get() == 0 {
                                if self.selection.borrow().selected {
                                    self.auxiliary.set(!self.auxiliary.get());
                                } else {
                                    self.crosshair_toggle.set(!self.crosshair_toggle.get());
                                }
                            }
                            self.alt.set(self.alt.get() | bit);
                        }
                        gdk::Key::Shift_L | gdk::Key::Shift_R if self.lens_visible() => {
                            self.hex.set(!self.hex.get())
                        }
                        gdk::Key::space => self.toolbar_visible.set(!self.toolbar_visible.get()),
                        gdk::Key::o | gdk::Key::O => {
                            if self.ocr_quick_copy.get() {
                                self.command(Command::Ocr);
                            } else {
                                self.toggle_interactive_ocr();
                            }
                        }
                        gdk::Key::l | gdk::Key::L => self.command(Command::Scroll),
                        gdk::Key::v | gdk::Key::V => self.command(Command::Record),
                        gdk::Key::r | gdk::Key::R => {
                            if let Some(rect) = self.last_area.get() {
                                self.set_area(rect);
                            }
                        }
                        gdk::Key::comma => self.command(Command::PreviousCapture),
                        gdk::Key::period => self.command(Command::NextCapture),
                        gdk::Key::F5 if self.purpose.get() == Purpose::Screenshot => {
                            self.command(Command::Refresh)
                        }
                        gdk::Key::Page_Up | gdk::Key::Page_Down => {
                            let areas = self.history.borrow();
                            if let Some(index) = clivuu_core::capture::history_area(
                                areas.len(),
                                self.history_index.get(),
                                if *key == gdk::Key::Page_Up { 1 } else { -1 },
                                self.history_loop.get(),
                            ) {
                                self.set_area(areas[index]);
                                self.history_index.set(index as isize);
                            }
                        }
                        gdk::Key::c | gdk::Key::C if self.lens_visible() => {
                            let image = self.image.borrow();
                            if let Some([x, y]) =
                                self.selection.borrow().lens_pixel(image.dimensions())
                            {
                                let p = image.pixels.get_pixel(x, y);
                                scene.display().clipboard().set_text(&if self.hex.get() {
                                    format!("#{:02X}{:02X}{:02X}", p[0], p[1], p[2])
                                } else {
                                    format!("{}, {}, {}", p[0], p[1], p[2])
                                });
                            }
                        }
                        _ => return !self.purpose.get().editable(),
                    }
                } else {
                    return !self.purpose.get().editable();
                }
                self.refresh();
                true
            }
        }
    }
}
struct View {
    document: Rc<CaptureDocument>,
    root: glib::WeakRef<Viewport>,
    editor: EditorView,
    backdrop: Canvas,
    ocr_canvas: Canvas,
    lens: Canvas,
    tips: super::capture_hints::Hints,
    viewport: Bounds,
    pointer_over_ui: Cell<bool>,
    motion: RefCell<Option<gtk::EventController>>,
}
impl Drop for View {
    fn drop(&mut self) {
        if let Some(root) = self.root.upgrade() {
            if let Some(motion) = self.motion.borrow_mut().take() {
                root.remove_controller(&motion);
            }
            for w in [
                self.backdrop.upcast_ref::<gtk::Widget>(),
                self.ocr_canvas.upcast_ref(),
                self.lens.upcast_ref(),
                self.tips.widget().upcast_ref(),
            ] {
                if w.parent().is_some() {
                    root.remove(w);
                }
            }
        }
    }
}
impl View {
    fn sync(self: &Rc<Self>) {
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let selection = self.document.selection.borrow();
        let output = selection.outputs[selection.output_at(selection.rect.center())].rect;
        let owner = output.center();
        let owns_controls = self.viewport.contains(owner, 0.);
        let ocr_active = self.document.ocr.borrow().is_active();
        if selection.drag.is_none() || ocr_active {
            self.editor.toolbar().set_ocr_active(ocr_active);
            if owns_controls {
                self.editor.ocr_bar().sync(&self.document.ocr.borrow());
            } else {
                self.editor.ocr_bar().widget().set_visible(false);
            }
        }
        // The X11 host spans the desktop; its controls still belong to one
        // actual monitor. Host origin and available viewport are independent.
        self.editor.set_defer_layout(true);
        self.editor
            .set_output(output, [self.viewport.x, self.viewport.y]);
        self.editor
            .set_capture_pixels(self.document.raster().map(|r| r.size));
        self.editor.set_controls_visible(owns_controls);
        self.editor.set_toolbar_visible(
            self.document.toolbar_visible.get()
                && self.document.purpose.get() != Purpose::Recording,
        );
        self.editor.set_selection(
            selection.rect,
            selection.selected && selection.drag.is_none(),
        );
        let scale = self.document.image.borrow().dimensions()[0] as f64 / selection.limit.width;
        self.editor.set_hint_visible(
            !(self.document.hide_single.get()
                && !selection.selected
                && selection.rect.width * scale <= 1.
                && selection.rect.height * scale <= 1.),
        );
        self.editor.set_defer_layout(false);
        self.backdrop.queue_draw();
        self.editor.scene().queue_draw();
        self.ocr_canvas.set_visible(ocr_active);
        if ocr_active {
            self.ocr_canvas.queue_draw();
        }
        let pointer_output = selection.outputs
            [selection.output_at(selection.pointer.unwrap_or(selection.rect.center()))]
        .rect;
        self.tips.sync(
            &root,
            pointer_output,
            self.viewport,
            self.document.style.borrow().tips
                && self.document.purpose.get() != Purpose::Image
                && !ocr_active,
            self.document.can_toggle_cursor(),
            &selection,
            &self.document.editor.borrow(),
        );
        drop(selection);
        // Lay out hint, toolbar and magnifier synchronously in the same frame
        // as backdrop so size badges never lag the selection border by a tick.
        self.editor.layout();
        if self.lens.is_visible() {
            self.lens.queue_draw();
        }
    }
    fn position_lens(&self) {
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let can_show = self.document.alt.get() != 0
            || (self.document.auto_lens_visible()
                && !self.pointer_over_ui.get()
                && !self.editor.options().has_popup());
        let selection = self.document.selection.borrow();
        let visible = can_show
            && selection.pointer.is_some_and(|p| {
                self.viewport
                    .contains(selection.outputs[selection.output_at(p)].rect.center(), 0.)
            });
        self.lens.set_visible(visible);
        if !visible {
            return;
        }
        let [x, y] = selection.pointer.unwrap();
        let v = selection.outputs[selection.output_at([x, y])].rect;
        let size = [184., 236.];
        let avoid = self
            .editor
            .toolbar()
            .widget()
            .is_visible()
            .then(|| self.editor.panels().map(|p| p.toolbar))
            .flatten();
        let positions = [
            [x + 26., y + 26.],
            [x - size[0] - 26., y + 26.],
            [x + 26., y - size[1] - 26.],
            [x - size[0] - 26., y - size[1] - 26.],
        ]
        .map(|p| Bounds {
            x: p[0].clamp(v.x + 8., (v.x + v.width - size[0] - 8.).max(v.x + 8.)),
            y: p[1].clamp(v.y + 8., (v.y + v.height - size[1] - 8.).max(v.y + 8.)),
            width: size[0],
            height: size[1],
        });
        let r = positions
            .iter()
            .copied()
            .find(|r| {
                !r.contains([x, y], 0.)
                    && avoid.is_none_or(|a| {
                        let overlap = intersect(*r, a);
                        overlap.width == 0. || overlap.height == 0.
                    })
            })
            .unwrap_or(positions[0]);
        root.move_(&self.lens, r.x - self.viewport.x, r.y - self.viewport.y);
    }
}
#[derive(Clone)]
pub struct CaptureView(Rc<View>);
impl CaptureView {
    pub fn new(
        document: Rc<CaptureDocument>,
        root: &Viewport,
        viewport: Bounds,
        services: ColorServices,
    ) -> Self {
        let weak = Rc::downgrade(&document);
        let backdrop = Canvas::new(move |s| {
            if let Some(doc) = weak.upgrade() {
                capture_draw::backdrop_base(
                    s,
                    &doc.image.borrow(),
                    &doc.selection.borrow(),
                    &doc.feedback_style(),
                    viewport,
                );
            }
        });
        root.put(&backdrop, 0., 0.);
        root.set_child_size(
            &backdrop,
            Some((viewport.width.ceil() as i32, viewport.height.ceil() as i32)),
        );
        let weak = Rc::downgrade(&document);
        let editor = EditorView::new(
            root,
            document.editor.clone(),
            &document.image.borrow().texture,
            viewport,
            [viewport.x, viewport.y],
            false,
            services,
            move |command| {
                if let Some(doc) = weak.upgrade() {
                    if matches!(command, Command::Ocr)
                        && (!doc.ocr_quick_copy.get() || doc.ocr.borrow().is_active())
                    {
                        doc.toggle_interactive_ocr();
                    } else {
                        doc.deactivate_ocr();
                        doc.command(command);
                    }
                }
            },
        );
        {
            let weak = Rc::downgrade(&document);
            editor.toolbar().connect_tool_click(move || {
                if let Some(doc) = weak.upgrade() {
                    doc.deactivate_ocr();
                }
            });
            let weak = Rc::downgrade(&document);
            editor.ocr_bar().connect_action(move |action| {
                if let Some(doc) = weak.upgrade() {
                    doc.handle_ocr_bar_action(action);
                }
            });
        }
        editor.toolbar().configure_selection(document.purpose.get());
        editor.refresh_layout();
        editor.scene().set_source_visible(false);
        {
            let weak = Rc::downgrade(&document);
            editor.scene().set_capture_overlay(move |s| {
                if let Some(doc) = weak.upgrade() {
                    let selection = doc.selection.borrow();
                    capture_draw::overlay(
                        s,
                        &selection,
                        &doc.feedback_style(),
                        [selection.limit.x, selection.limit.y],
                        viewport,
                    );
                }
            });
        }
        let weak = Rc::downgrade(&document);
        let ocr_canvas = Canvas::new(move |s| {
            if let Some(doc) = weak.upgrade() {
                ocr_layer::paint_ocr_overlay(
                    s,
                    &doc.ocr.borrow(),
                    doc.selection.borrow().rect,
                    [viewport.x, viewport.y],
                    doc.feedback_style().accent,
                    doc.dark.get(),
                );
            }
        });
        ocr_canvas.set_visible(false);
        root.put(&ocr_canvas, 0., 0.);
        root.set_child_size(
            &ocr_canvas,
            Some((viewport.width.ceil() as i32, viewport.height.ceil() as i32)),
        );
        // Ensure toolbar/subbar/ocr_bar stay stacked above `ocr_canvas`.
        for panel in [
            editor.hint().upcast_ref::<gtk::Widget>(),
            editor.toolbar().widget().upcast_ref(),
            editor.options().widget().upcast_ref(),
            editor.ocr_bar().widget().upcast_ref(),
        ] {
            panel.insert_after(root, Some(&ocr_canvas));
        }
        let context = root.pango_context();
        let weak = Rc::downgrade(&document);
        let lens = Canvas::new(move |s| {
            if let Some(doc) = weak.upgrade() {
                capture_draw::lens(
                    s,
                    &doc.image.borrow(),
                    &doc.selection.borrow(),
                    &doc.feedback_style(),
                    &context,
                    doc.hex.get(),
                );
            }
        });
        root.put(&lens, 0., 0.);
        root.set_child_size(&lens, Some((184, 236)));
        let tips = super::capture_hints::Hints::new();
        root.put(tips.widget(), 0., 0.);
        let view = Self(Rc::new(View {
            document: document.clone(),
            root: root.downgrade(),
            editor,
            backdrop,
            ocr_canvas,
            lens,
            tips,
            viewport,
            pointer_over_ui: Cell::new(false),
            motion: RefCell::new(None),
        }));
        {
            let weak = Rc::downgrade(&view.0);
            view.0.editor.connect_layout_changed(move || {
                if let Some(view) = weak.upgrade() {
                    view.position_lens();
                }
            });
        }
        let motion = gtk::EventControllerMotion::new();
        {
            let weak = Rc::downgrade(&view.0);
            motion.connect_motion(move |_, x, y| {
                let Some(view) = weak.upgrade() else {
                    return;
                };
                if !view.document.auto_lens_visible() {
                    if view.pointer_over_ui.replace(false) {
                        view.position_lens();
                    }
                    return;
                }
                let Some(root) = view.root.upgrade() else {
                    return;
                };
                let pt = gtk::graphene::Point::new(x as f32, y as f32);
                let scene = view.editor.scene().upcast_ref::<gtk::Widget>();
                let over_ui =
                    std::iter::successors(root.first_child(), |w| w.next_sibling()).any(|w| {
                        &w != scene
                            && w.is_visible()
                            && w.can_target()
                            && w.compute_bounds(&root)
                                .is_some_and(|b| b.contains_point(&pt))
                    });
                if view.pointer_over_ui.replace(over_ui) != over_ui {
                    view.position_lens();
                }
            });
            let weak = Rc::downgrade(&view.0);
            motion.connect_leave(move |_| {
                if let Some(view) = weak.upgrade()
                    && view.pointer_over_ui.replace(false)
                {
                    view.position_lens();
                }
            });
        }
        root.add_controller(motion.clone());
        view.0.motion.replace(Some(motion.upcast()));
        document.views.borrow_mut().push(Rc::downgrade(&view.0));
        let weak = Rc::downgrade(&view.0);
        view.0.editor.scene().set_input_route(move |input| {
            weak.upgrade()
                .is_some_and(|view| view.document.input(view.editor.scene(), input))
        });
        let weak = Rc::downgrade(&view.0);
        view.0.editor.scene().connect_changed(move |_| {
            if let Some(view) = weak.upgrade() {
                view.sync();
            }
        });
        document.refresh();
        view
    }
    pub fn editor(&self) -> &EditorView {
        &self.0.editor
    }
    pub fn lens_visible(&self) -> bool {
        self.0.lens.is_visible()
    }
}
