use super::*;
use crate::platform::capture::session::Session;
use clivuu_core::capture::{Purpose, Selection};
use std::path::PathBuf;
struct Pending<'a>(&'a State);
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.0.record_pending.borrow_mut().take();
        self.0.refresh_menus();
    }
}
impl State {
    pub(super) async fn capture_history(&self) -> Result<Value, String> {
        let images = self
            .history
            .borrow()
            .iter()
            .rev()
            .map(|record| record.png.as_ref().clone())
            .collect::<Vec<_>>();
        let directory = crate::infrastructure::temporary::Directory::new(
            &self.pool,
            "history",
            &self.work_cancel(),
        )
        .await?;
        let mut paths = vec![];
        for bytes in images {
            paths.push(
                directory
                    .write(&self.pool, bytes, &self.work_cancel())
                    .await?,
            );
        }
        self.history_directory.replace(Some(directory));
        Ok(json!({"images":paths}))
    }
    pub(super) async fn record(
        &self,
        request: &Value,
        settings: &serde_json::Map<String, Value>,
    ) -> Result<Value, String> {
        if self.recording.busy() {
            return Err(t("Recording or finalization is already in progress"));
        }
        let scope = Scope::new(&self.work_cancel());
        self.record_pending.replace(Some(scope.token.clone()));
        let _pending = Pending(self);
        self.refresh_menus();
        let selection = self
            .select_region(request, settings, Purpose::Recording, &scope.token)
            .await;
        if scope.token.is_cancelled() {
            return Ok(json!({"cancelled":true}));
        }
        let Some(rect) = selection? else {
            return Ok(json!({"cancelled":true}));
        };
        self.record_region(rect, request, settings, scope).await
    }
    async fn select_region(
        &self,
        request: &Value,
        settings: &serde_json::Map<String, Value>,
        purpose: Purpose,
        cancel: &gio::Cancellable,
    ) -> Result<Option<Bounds>, String> {
        let display =
            gdk::Display::default().ok_or_else(|| t("The desktop service is unavailable"))?;
        let outputs =
            crate::platform::integration::layout::outputs(&self.connection, &display, cancel)
                .await?;
        let layout = Selection::new(outputs.clone(), vec![], None)
            .ok_or_else(|| t("Invalid monitor layout"))?;
        let rect = if let Some(value) = request.get("rect") {
            let rect: protocol::Rect =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            let rect = rect.validate().map_err(|e| e.to_string())?;
            Bounds {
                x: rect.x as f64,
                y: rect.y as f64,
                width: rect.width as f64,
                height: rect.height as f64,
            }
        } else {
            let _pins = self
                .pins
                .during_capture(settings["capture-hide-pins"].as_bool().unwrap_or(true));
            let image = self.acquire_screenshot(cancel).await?;
            let live = self.configuration.borrow().clone();
            let selected = capture::select(
                &display,
                image,
                outputs,
                vec![],
                None,
                capture::Options {
                    purpose,
                    live,
                    ..options(settings)
                },
                self.pool.clone(),
                cancel,
            )
            .await?;
            let Some(selected) = selected else {
                return Ok(None);
            };
            let capture::Selected::Region { rect, .. } = selected else {
                return Err(t("Invalid desktop response"));
            };
            rect
        };
        const EPS: f64 = 1e-9;
        if rect.x < layout.limit.x - EPS
            || rect.y < layout.limit.y - EPS
            || rect.x + rect.width > layout.limit.x + layout.limit.width + EPS
            || rect.y + rect.height > layout.limit.y + layout.limit.height + EPS
        {
            return Err(t("The selection is outside the monitor layout"));
        }
        Ok(Some(rect))
    }
    pub(super) async fn scroll(
        &self,
        request: &Value,
        settings: &serde_json::Map<String, Value>,
    ) -> Result<Value, String> {
        let Some(rect) = self
            .select_region(
                request,
                settings,
                Purpose::Scrolling,
                &self.operation_cancel(),
            )
            .await?
        else {
            return Ok(json!({"cancelled":true}));
        };
        self.start_scroll(rect, request, settings).await
    }
    pub(super) async fn start_scroll(
        &self,
        rect: Bounds,
        request: &Value,
        settings: &serde_json::Map<String, Value>,
    ) -> Result<Value, String> {
        let display =
            gdk::Display::default().ok_or_else(|| t("The desktop service is unavailable"))?;
        let _pins = self.pins.during_capture(true);
        let live = self.configuration.borrow().clone();
        let Some(captured) = super::super::scrolling::run(
            &self.connection,
            &display,
            self.pool.clone(),
            rect,
            self.scrolling.clone(),
            super::super::capture::Options {
                live,
                ..options(settings)
            },
            settings["max-scroll-frames"].as_u64().unwrap_or(40) as usize,
            settings["scroll-interval"].as_u64().unwrap_or(700),
            settings["scroll-auto-crop"].as_bool().unwrap_or(false),
            &self.operation_cancel(),
        )
        .await?
        else {
            return Ok(json!({"cancelled":true}));
        };
        let pixels = captured.pixels;
        let rect = captured.rect;
        let scale = pixels.width() as f64 / rect.width;
        let position = Bounds {
            height: pixels.height() as f64 / scale,
            ..rect
        };
        let image = crate::editor::frozen_image::FrozenImage::from_shared(pixels)
            .ok_or_else(|| t("Invalid saved pin data"))?;
        let destination = match Destination::parse(request.get("output"))? {
            Destination::Toolbar => match captured.action {
                Output::Save => Destination::Dialog,
                Output::QuickSave => Destination::Quick,
                Output::Copy => Destination::Clipboard,
                _ => Destination::Pin,
            },
            other => other,
        };
        drop(_pins);
        if matches!(captured.action, Output::Apply) && matches!(destination, Destination::Pin) {
            let id = self.pins.add(image, Some(position))?;
            self.pins.toolbar(Some(id));
            return Ok(json!({"pinned":true,"editing":true,"rect":position}));
        }
        self.deliver_image(image, position, destination, settings)
            .await
    }
    pub(super) async fn start_recording(
        &self,
        rect: Bounds,
        request: &Value,
        settings: &serde_json::Map<String, Value>,
    ) -> Result<Value, String> {
        if self.recording.busy() {
            return Err(t("Recording or finalization is already in progress"));
        }
        let scope = Scope::new(&self.work_cancel());
        self.record_pending.replace(Some(scope.token.clone()));
        let _pending = Pending(self);
        self.refresh_menus();
        self.record_region(rect, request, settings, scope).await
    }
    async fn record_region(
        &self,
        rect: Bounds,
        request: &Value,
        settings: &serde_json::Map<String, Value>,
        scope: Scope,
    ) -> Result<Value, String> {
        use crate::infrastructure::saving::{self, Mode};
        use clivuu_core::saving::Media;
        let operation = scope.token.clone();
        let context = saving::now()?;
        let requested = request.get("output").and_then(Value::as_str);
        let unique = matches!(requested, None | Some("quick-save") | Some("file-dialog"));
        use clivuu_core::recording::{Format, Mode as RecordMode, Options as RecordOptions};
        let display =
            gdk::Display::default().ok_or_else(|| t("The desktop service is unavailable"))?;
        let live_settings = self.configuration.borrow().as_ref().map(|c| c.values());
        let appearance = recording_appearance(live_settings.as_ref().unwrap_or(settings));
        let initial = RecordOptions {
            mode: if request.get("rect").is_some() || !unique {
                RecordMode::Quick
            } else {
                RecordMode::Normal
            },
            ..Default::default()
        };
        let Some(setup) = self
            .recording
            .configure(
                &display,
                rect,
                appearance,
                initial,
                &scope.token,
                request.get("rect").is_none(),
            )
            .await?
        else {
            return Ok(json!({"cancelled":true}));
        };
        let rect = setup.rect;
        let format = requested
            .and_then(|p| Format::from_path(std::path::Path::new(p)))
            .unwrap_or(Format::Mp4);
        let destination = if unique {
            saving::target(settings, Media::Video, Mode::Quick, &context)?
                .with_extension(format.extension())
        } else {
            PathBuf::from(requested.unwrap_or_default())
        };
        if !destination.is_absolute() {
            return Err(t("The recording output must be an absolute path"));
        }
        let destination = if destination.extension().is_none() {
            destination.with_extension(format.extension())
        } else {
            destination
        };
        clivuu_core::saving::extension(&destination, Media::Video).map_err(|e| e.to_string())?;
        let output = super::super::recording::VideoOutput {
            destination,
            unique,
            replace: false,
            notify: settings["video-save-notify"] != false,
            format,
        };
        if operation.is_cancelled() {
            return Ok(json!({"cancelled": true}));
        }
        // Detect capture capability independently from the GTK display backend.
        let mutter = super::super::scrolling::has_mutter(&self.connection).await;
        let session = if mutter {
            Session::mutter(&self.connection, &scope.token, rect, setup.options.cursor)
                .await
                .map(Some)
        } else {
            Session::monitor(&self.connection, &scope.token, setup.options.cursor).await
        };
        if operation.is_cancelled() {
            return Ok(json!({"cancelled":true}));
        }
        let Some(mut session) = session? else {
            return Ok(json!({"cancelled":true}));
        };
        if !mutter {
            session.fallback_single_monitor(&crate::platform::surfaces::capture::gdk_outputs(
                &display,
            ));
        }
        session.retain_scope(scope);
        let Some(path) = self
            .recording
            .start(
                session,
                rect,
                output,
                settings["recording-storage"].as_str() != Some("direct"),
                settings["recording-temp-limit-mib"].as_u64().unwrap_or(512),
            )
            .await?
        else {
            return Ok(json!({"cancelled":true}));
        };
        Ok(if setup.options.mode == RecordMode::Normal {
            json!({"recording":true})
        } else {
            json!({"path":path,"recording":true})
        })
    }
}
