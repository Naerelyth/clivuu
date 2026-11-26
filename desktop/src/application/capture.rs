//! A capture session owns acquisition-independent presentation and teardown.
//! One document drives all output views; pointer updates stay local to GTK.
use crate::{
    editor::{
        capture_draw::CaptureStyle,
        capture_view::{CaptureDocument, CaptureView},
        export,
        frozen_image::FrozenImage,
    },
    infrastructure::tasks::TaskPool,
    platform::surfaces::capture::CaptureOverlay,
    ui::toolbar::{Output as Command, Toolbar},
};
use clivuu_core::{
    annotation::Bounds,
    capture::{Output, Purpose, Selection},
    i18n::t,
    image::RgbaImage,
};
use gtk::{gdk, gio, glib, prelude::*};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    sync::Arc,
};

/// Commands from CLI/tray entry points target only the interactive editor.
/// The registration expires before rendering and on every teardown path.
#[derive(Clone, Default)]
pub struct Commands(Rc<RefCell<Weak<CaptureDocument>>>);
impl Commands {
    pub fn pin(&self) -> bool {
        let Some(document) = self.0.borrow().upgrade() else {
            return false;
        };
        if document.raster().is_none() {
            return false;
        }
        self.0.replace(Weak::new());
        document.pin();
        true
    }
    pub fn ocr_status(&self) -> serde_json::Value {
        self.0
            .borrow()
            .upgrade()
            .map_or_else(|| serde_json::json!(null), |d| d.ocr_status())
    }
    pub fn cursor_status(&self) -> serde_json::Value {
        self.0.borrow().upgrade().map_or_else(
            || serde_json::json!(null),
            |d| {
                serde_json::json!({
                    "canToggle": d.can_toggle_cursor(),
                    "visible": d.is_cursor_visible(),
                })
            },
        )
    }
    fn bind(&self, document: &Rc<CaptureDocument>) -> Registration {
        self.0.replace(Rc::downgrade(document));
        Registration(self.clone())
    }
}
struct Registration(Commands);
impl Drop for Registration {
    fn drop(&mut self) {
        self.0.0.replace(Weak::new());
    }
}

pub struct RefreshedSource {
    pub image: Arc<RgbaImage>,
    pub without_cursor: Option<Arc<RgbaImage>>,
}
pub type Refresh = Rc<
    dyn Fn()
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RefreshedSource, String>>>>,
>;
#[derive(Clone)]
pub struct Snapshot {
    pub source: Arc<RgbaImage>,
    pub cursor_pair: Option<(Arc<RgbaImage>, Arc<RgbaImage>)>,
    pub cursor_visible: bool,
    pub selection: Selection,
    pub editor: crate::editor::EditorController,
}
pub struct Options {
    pub purpose: Purpose,
    /// Bounds where this source image is presented, independent of output surfaces.
    pub image_bounds: Option<Bounds>,
    pub without_cursor: Option<Arc<RgbaImage>>,
    pub dark: bool,
    pub accent: String,
    pub display: CaptureStyle,
    pub detect_windows: bool,
    pub live: Option<Rc<crate::infrastructure::configuration::Configuration>>,
    pub history: Vec<Bounds>,
    pub history_loop: bool,
    pub hide_single: bool,
    pub last_area: Option<Bounds>,
    pub records: Vec<Snapshot>,
    pub refresh: Option<Refresh>,
    pub commands: Option<Commands>,
    pub ocr_settings: clivuu_core::ocr::OcrSettings,
    pub ocr_force_quick_copy: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            purpose: Purpose::Screenshot,
            image_bounds: None,
            without_cursor: None,
            dark: true,
            accent: "#bd93f9".into(),
            display: CaptureStyle::default(),
            detect_windows: true,
            live: None,
            history: vec![],
            history_loop: false,
            hide_single: true,
            last_area: None,
            records: vec![],
            refresh: None,
            commands: None,
            ocr_settings: clivuu_core::ocr::OcrSettings::default(),
            ocr_force_quick_copy: false,
        }
    }
}
pub enum Selected {
    Image(Captured),
    Region { rect: Bounds, action: Command },
    TextPin { text: String, rect: Bounds },
    RecognizedText { text: String, rect: Bounds },
    OpenPreferences(String),
}
pub struct Captured {
    pub image: FrozenImage,
    pub rect: Bounds,
    pub action: Command,
    pub snapshot: Box<Snapshot>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Capture owns explicit source, geometry, presentation and cancellation; no global session state"
)]
pub async fn select(
    display: &gdk::Display,
    image: Arc<RgbaImage>,
    outputs: Vec<Output>,
    windows: Vec<Bounds>,
    initial: Option<Bounds>,
    options: Options,
    pool: Rc<TaskPool>,
    cancel: &gio::Cancellable,
) -> Result<Option<Selected>, String> {
    let scope = crate::infrastructure::cancellation::Scope::new(cancel);
    let reason = Rc::new(RefCell::new(None::<Option<String>>));
    let result = gio::CancellableFuture::new(
        async {
            let mut selection = Selection::new(outputs, windows, initial)
                .ok_or_else(|| t("Invalid monitor layout"))?;
            if let Some(bounds) = options.image_bounds {
                let limit = selection.limit;
                if ![bounds.x, bounds.y, bounds.width, bounds.height]
                    .iter()
                    .all(|v| v.is_finite())
                    || bounds.width <= 0.
                    || bounds.height <= 0.
                    || bounds.x < limit.x
                    || bounds.y < limit.y
                    || bounds.x + bounds.width > limit.x + limit.width
                    || bounds.y + bounds.height > limit.y + limit.height
                {
                    return Err(t("The captured image does not match the monitor layout"));
                }
                selection.limit = bounds;
                selection.rect = bounds;
                selection.selected = true;
            }
            selection.detect_windows = options.detect_windows;
            let scale = image.width() as f64 / selection.limit.width;
            if (image.height() as f64 - selection.limit.height * scale).abs() > 2. {
                return Err(t("The captured image does not match the monitor layout"));
            }
            let clean_image = options
                .without_cursor
                .as_ref()
                .filter(|c| c.dimensions() == image.dimensions())
                .and_then(|c| FrozenImage::from_shared(c.clone()));
            let image = FrozenImage::from_shared(image)
                .ok_or_else(|| t("The image exceeds 48 megapixels or has invalid dimensions"))?;
            let initial_cursor_pair = clean_image.map(|clean| (image.clone(), clean));
            let (tx, rx) = async_channel::bounded(1);
            let abort = scope.token.clone();
            let failure = reason.clone();
            let overlay = CaptureOverlay::new(display, &selection.outputs, move |error| {
                failure.replace(Some(error));
                abort.cancel();
            })?;
            let context = overlay.surfaces()[0].root.pango_context();
            let document =
                CaptureDocument::new(image, selection, &context, pool.clone(), move |command| {
                    let _ = tx.try_send(command);
                });
            document.set_purpose(options.purpose);
            if initial_cursor_pair.is_some() {
                document.set_cursor_frames(initial_cursor_pair, true);
            }
            document.set_last_area(options.last_area);
            document.set_style(options.display);
            document.set_history(options.history, options.history_loop);
            document.hide_single_pixel(options.hide_single);
            document.set_ocr_config(
                options.ocr_settings.clone(),
                options.ocr_force_quick_copy,
                options.live.clone(),
            );
            let views: Vec<_> = overlay
                .surfaces()
                .iter()
                .map(|s| {
                    let view = CaptureView::new(
                        document.clone(),
                        &s.root,
                        s.rect,
                        super::editor_services::color_services(),
                    );
                    gtk::prelude::GtkWindowExt::set_focus(&s.window, Some(view.editor().scene()));
                    view
                })
                .collect();
            let theme = Rc::new(RefCell::new(Toolbar::install_theme(
                display,
                options.dark,
                &options.accent,
            )));
            document.set_theme(&theme.borrow());
            let _appearance = options.live.map(|config| {
                let document = Rc::downgrade(&document);
                let theme = theme.clone();
                config.subscribe(move |values| {
                    if let Some(document) = document.upgrade() {
                        let options = super::service::options(values);
                        document.set_style(options.display);
                        document.hide_single_pixel(options.hide_single);
                        theme.borrow_mut().update(options.dark, &options.accent);
                        document.set_theme(&theme.borrow());
                    }
                })
            });
            overlay.open();
            // GTK can clear a pre-map focus child while realizing the native
            // host. Explicit capture must accept keys before the first click.
            for view in &views {
                view.editor().scene().grab_focus();
            }
            let commands = options.commands.filter(|_| options.purpose.editable());
            let mut registration = commands.as_ref().map(|c| c.bind(&document));
            let mut current = None;
            let mut record_index = options.records.len();
            let action = loop {
                let command = rx
                    .recv()
                    .await
                    .map_err(|_| t("The desktop service is stopped"))?;
                match command {
                    Command::Close => return Ok(None),
                    Command::Refresh if options.refresh.is_some() => {
                        drop(registration.take());
                        overlay.hide();
                        display.flush();
                        glib::timeout_future(std::time::Duration::from_millis(60)).await;
                        let refreshed = options.refresh.as_ref().unwrap()().await?;
                        let clean_image = refreshed
                            .without_cursor
                            .as_ref()
                            .filter(|c| c.dimensions() == refreshed.image.dimensions())
                            .and_then(|c| FrozenImage::from_shared(c.clone()));
                        let image = FrozenImage::from_shared(refreshed.image).ok_or_else(|| {
                            t("The image exceeds 48 megapixels or has invalid dimensions")
                        })?;
                        let expected = document.image.borrow().dimensions();
                        if image.dimensions() != expected {
                            return Err(t("The captured image does not match the monitor layout"));
                        }
                        // Refresh replaces only the frozen desktop, preserving selection and edits.
                        let keep_cursor_visible = document.is_cursor_visible();
                        if let Some(clean) = clean_image {
                            document.set_cursor_frames(Some((image, clean)), keep_cursor_visible);
                        } else {
                            document.replace_image(image);
                        }
                        record_index = options.records.len();
                        current = None;
                        overlay.open();
                        for view in &views {
                            view.editor().scene().grab_focus();
                        }
                        registration = commands.as_ref().map(|c| c.bind(&document));
                    }
                    Command::PreviousCapture | Command::NextCapture => {
                        let next = if matches!(command, Command::PreviousCapture) {
                            record_index.saturating_sub(1)
                        } else {
                            (record_index + 1).min(options.records.len())
                        };
                        if next == record_index {
                            continue;
                        }
                        if current.is_none() {
                            current = Some(document.snapshot());
                        }
                        if next < options.records.len() {
                            document.restore(&options.records[next]);
                        } else if let Some(saved) = &current {
                            document.restore(saved);
                        }
                        record_index = next;
                    }
                    Command::Refresh => {}
                    other => break other,
                }
            };
            drop(registration);
            for s in overlay.surfaces() {
                s.root.set_sensitive(false);
            }
            let rect = document.selection().rect;
            if matches!(action, Command::OcrPreferences) {
                return Ok(Some(Selected::OpenPreferences("ocr".to_owned())));
            }
            if matches!(action, Command::ConfirmOcrText)
                && let Some(text) = document.take_confirmed_ocr_text()
            {
                return Ok(Some(Selected::RecognizedText { text, rect }));
            }
            if matches!(action, Command::PinText)
                && let Some(text) = document.take_pinned_text()
            {
                return Ok(Some(Selected::TextPin { text, rect }));
            }
            if matches!(action, Command::Scroll | Command::Record) {
                // This phase only chooses geometry. Do not render/export a PNG
                // (or interpret image output buttons) before continuous capture.
                return Ok(Some(Selected::Region { rect, action }));
            }
            let raster = document
                .raster()
                .ok_or_else(|| t("The captured image does not match the monitor layout"))?;
            let scene = views[0].editor().scene();
            scene.dispatch(crate::editor::Action::CommitText);
            let snapshot = Box::new(document.snapshot());
            let has_annotations = {
                let model = scene.controller();
                let borrow = model.borrow();
                borrow.session().displayed().next().is_some() || borrow.drawing().is_some()
            };
            let plain =
                !has_annotations && scene.filter() == crate::editor::filter::Filter::default();
            let image = if plain {
                overlay.hide();
                display.flush();
                let current_image = document.image.borrow().clone();
                let full = Bounds {
                    x: 0.,
                    y: 0.,
                    width: current_image.pixels.width() as f64,
                    height: current_image.pixels.height() as f64,
                };
                if raster.source == full
                    && raster.size == [current_image.pixels.width(), current_image.pixels.height()]
                {
                    current_image
                } else {
                    let source = current_image.pixels.clone();
                    let task = pool
                        .submit(move |token| {
                            token.check()?;
                            let cropped = clivuu_core::image_ops::crop_capture(&source, raster)
                                .map_err(|e| {
                                    crate::infrastructure::tasks::TaskError::Failed(e.to_string())
                                })?;
                            token.check()?;
                            Ok(cropped)
                        })
                        .map_err(|e| e.to_string())?;
                    let pixels =
                        gtk::gio::CancellableFuture::new(task.result(), scope.token.clone())
                            .await
                            .map_err(|_| t("Operation cancelled"))?
                            .map_err(|e| e.to_string())?;
                    FrozenImage::new(pixels).ok_or_else(|| {
                        t("The image exceeds 48 megapixels or has invalid dimensions")
                    })?
                }
            } else {
                let surface = &overlay.surfaces()[0];
                let renderer = surface
                    .window
                    .renderer()
                    .ok_or_else(|| t("The desktop service is unavailable"))?;
                export::frame_sized_then(
                    scene,
                    &renderer,
                    Some(raster.source),
                    raster.size,
                    &pool,
                    &scope.token,
                    || {
                        overlay.hide();
                        display.flush();
                    },
                )
                .await
                .map_err(|e| format!("{e:?}"))?
            };
            Ok(Some(Selected::Image(Captured {
                image,
                rect,
                action,
                snapshot,
            })))
        },
        scope.token.clone(),
    )
    .await;
    match result {
        Ok(result) => result,
        Err(_) => match reason.borrow_mut().take() {
            Some(Some(error)) => Err(error),
            Some(None) => Ok(None),
            None => Err(t("Operation cancelled")),
        },
    }
}
