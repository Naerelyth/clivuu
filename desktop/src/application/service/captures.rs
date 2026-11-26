//! Static screenshot acquisition, selection and output routing.
use super::*;
use crate::platform::{
    capture::{cursor, screenshot::Screenshot, window},
    surfaces::capture::{LayoutWatch, gdk_outputs},
};
use clivuu_core::capture::{Purpose, Selection};
impl State {
    pub(super) async fn acquire_screenshot(
        &self,
        cancel: &gio::Cancellable,
    ) -> Result<Arc<clivuu_core::image::RgbaImage>, String> {
        let cached = self
            .screenshot
            .borrow()
            .as_ref()
            .filter(|s| s.is_connected())
            .cloned();
        let client = if let Some(client) = cached {
            client
        } else {
            let client = Rc::new(Screenshot::connect(cancel).await?);
            self.screenshot.replace(Some(client.clone()));
            client
        };
        match client.capture(&self.pool, cancel).await {
            Ok(image) => Ok(image),
            Err(_) if !client.is_connected() && !cancel.is_cancelled() => {
                self.screenshot.replace(None);
                let client = Rc::new(Screenshot::connect(cancel).await?);
                self.screenshot.replace(Some(client.clone()));
                client.capture(&self.pool, cancel).await
            }
            Err(error) => Err(error),
        }
    }
    pub(super) async fn capture(
        &self,
        request: &Value,
        settings: &serde_json::Map<String, Value>,
        configuration: Rc<crate::infrastructure::configuration::Configuration>,
        display: &gdk::Display,
    ) -> Result<Value, String> {
        let command = request["command"].as_str().unwrap_or("");
        let capture_cursor = settings["capture-cursor"].as_bool() == Some(true);
        let mode = request["mode"].as_str().unwrap_or("");
        if ![
            "",
            "full",
            "last",
            "active-screen",
            "active-window",
            "window",
        ]
        .contains(&mode)
        {
            return Err(t(
                "The static screenshot portal cannot capture this window. Select a region instead.",
            ));
        }
        let destination = if command == "barcode-scan" {
            Destination::Barcode
        } else {
            Destination::parse(request.get("output"))?
        };
        if let Some(delay) = request["delay"].as_f64() {
            if !delay.is_finite() || !(0.0..=3600.).contains(&delay) {
                return Err(t("The delay must be between 0 and 3600 seconds"));
            }
            gio::CancellableFuture::new(
                glib::timeout_future(std::time::Duration::from_secs_f64(delay)),
                self.work_cancel(),
            )
            .await
            .map_err(|_| t("Operation cancelled"))?;
        }
        let _pins = self
            .pins
            .during_capture(settings["capture-hide-pins"].as_bool().unwrap_or(true));
        // The countdown precedes the source/layout pair. Observe even a change
        // followed by a restoration while the portal request is in flight.
        let acquisition = Scope::new(&self.operation_cancel());
        let changed = Rc::new(Cell::new(false));
        let flag = changed.clone();
        let cancel = acquisition.token.clone();
        let _watch = LayoutWatch::new(display, move || {
            flag.set(true);
            cancel.cancel();
        });
        let mut outputs = crate::platform::integration::layout::outputs(
            &self.connection,
            display,
            &acquisition.token,
        )
        .await?;
        let mut layout = Selection::new(outputs.clone(), vec![], None)
            .ok_or_else(|| t("Invalid monitor layout"))?;
        if matches!(mode, "window" | "active-window") {
            let result = window::capture(
                &self.connection,
                &self.pool,
                capture_cursor,
                &acquisition.token,
            )
            .await;
            if changed.get() {
                return Err(t("The monitor layout changed during capture"));
            }
            let Some(image) = result? else {
                return Ok(json!({"cancelled":true}));
            };
            return self
                .window_image(
                    image,
                    outputs,
                    destination,
                    settings,
                    configuration,
                    display,
                    &acquisition.token,
                )
                .await;
        }
        let windows =
            crate::platform::integration::windows::query(display, &outputs, &acquisition.token)
                .await
                .unwrap_or_default();
        let focused = windows.iter().find(|w| w.focused).map(|w| w.rect);
        let streamed_cursor = capture_cursor && !cursor::real_x11(display);
        let pointer = if capture_cursor && !streamed_cursor {
            Some(cursor::snapshot(&self.pool, &acquisition.token).await?)
        } else {
            None
        };
        let root_outputs = gdk_outputs(display);
        let root = Selection::new(root_outputs.clone(), vec![], None)
            .ok_or_else(|| t("Invalid monitor layout"))?
            .limit;
        let root_scale = root_outputs[0].scale;
        let point = if cursor::real_x11(display)
            && (mode == "active-screen" || request.get("size").is_some())
        {
            let point = if let Some(cursor) = pointer.as_ref() {
                [cursor.x as f64, cursor.y as f64]
            } else {
                cursor::position(&self.pool, &acquisition.token).await?
            };
            Some(cursor::desktop_point(point, root, root_scale, layout.limit))
        } else {
            None
        };
        let interactive_selector = mode.is_empty()
            && request.get("rect").is_none()
            && request.get("size").is_none()
            && destination != Destination::Barcode;
        let mut without_cursor = None;
        let result = if !streamed_cursor {
            self.acquire_screenshot(&acquisition.token).await
        } else {
            crate::platform::capture::still::capture(
                &self.connection,
                &self.pool,
                &outputs,
                interactive_selector,
                &acquisition.token,
            )
            .await
            .map(|captured| {
                outputs = captured.outputs;
                without_cursor = captured.without_cursor;
                // Validated by the platform assembler against actual stream pixels.
                layout = Selection::new(outputs.clone(), vec![], None).unwrap();
                captured.image
            })
        };
        if changed.get() {
            return Err(t("The monitor layout changed during capture"));
        }
        let image = result?;
        let image = if let Some(pointer) = pointer {
            if interactive_selector {
                without_cursor = Some(image.clone());
            }
            cursor::composite(
                image,
                pointer,
                root,
                root_scale,
                &self.pool,
                &acquisition.token,
            )
            .await?
        } else {
            image
        };
        if changed.get() {
            return Err(t("The monitor layout changed during capture"));
        }
        let mut initial = request
            .get("rect")
            .map(|v| {
                serde_json::from_value::<protocol::Rect>(v.clone())
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.validate().map_err(|e| e.to_string()))
            })
            .transpose()?
            .map(|r| Bounds {
                x: r.x as f64,
                y: r.y as f64,
                width: r.width as f64,
                height: r.height as f64,
            });
        if initial.is_none() {
            initial = match mode {
                "last" => Some(
                    self.last_rect
                        .get()
                        .ok_or_else(|| t("No previous selection"))?,
                ),
                "active-screen" => Some(
                    outputs[layout.output_at(point.unwrap_or_else(|| {
                        focused.map_or(outputs[0].rect.center(), |r| r.center())
                    }))]
                    .rect,
                ),
                _ => None,
            };
            if let Some(size) = request.get("size") {
                let size = size
                    .as_array()
                    .filter(|v| v.len() == 2)
                    .ok_or_else(|| t("Invalid desktop response"))?;
                let width = size[0]
                    .as_f64()
                    .filter(|v| v.is_finite() && *v > 0.)
                    .ok_or_else(|| t("Invalid desktop response"))?;
                let height = size[1]
                    .as_f64()
                    .filter(|v| v.is_finite() && *v > 0.)
                    .ok_or_else(|| t("Invalid desktop response"))?;
                let output =
                    outputs[layout.output_at(point.unwrap_or_else(|| {
                        focused.map_or(outputs[0].rect.center(), |r| r.center())
                    }))]
                    .rect;
                initial = Some(Bounds {
                    x: (output.x + (output.width - width) / 2.).round(),
                    y: (output.y + (output.height - height) / 2.).round(),
                    width,
                    height,
                });
            }
        }
        let mut snapshot = None;
        let (image, rect, action) = if initial.is_some() || mode == "full" {
            let r = initial.unwrap_or(layout.limit);
            let limit = layout.limit;
            if settings["capture-history-records"].as_u64().unwrap_or(20) > 0 {
                snapshot = Some(Box::new(capture::Snapshot {
                    source: image.clone(),
                    cursor_pair: None,
                    cursor_visible: true,
                    selection: Selection::new(outputs.clone(), vec![], Some(r)).unwrap(),
                    editor: crate::editor::EditorController::new(crate::editor::ImageMapping {
                        display: limit,
                        document: [image.width() as f64, image.height() as f64],
                    }),
                }));
            }
            const EPS: f64 = 1e-9;
            if r.x < limit.x - EPS
                || r.y < limit.y - EPS
                || r.x + r.width > limit.x + limit.width + EPS
                || r.y + r.height > limit.y + limit.height + EPS
            {
                return Err(t("The selection is outside the monitor layout"));
            }
            let raster = clivuu_core::capture::raster_region(
                &outputs,
                limit,
                [image.width(), image.height()],
                r,
            )
            .ok_or_else(|| t("The captured image does not match the monitor layout"))?;
            let task = self
                .pool
                .submit(move |token| {
                    token.check()?;
                    let cropped = clivuu_core::image_ops::crop_capture(&image, raster)
                        .map_err(|e| TaskError::Failed(e.to_string()))?;
                    token.check()?;
                    Ok(cropped)
                })
                .map_err(|error| error.to_string())?;
            let pixels = gio::CancellableFuture::new(task.result(), self.work_cancel())
                .await
                .map_err(|_| t("Operation cancelled"))?
                .map_err(|error| error.to_string())?;
            let image = crate::editor::frozen_image::FrozenImage::new(pixels)
                .ok_or_else(|| t("Invalid saved pin data"))?;
            (image, r, Output::Copy)
        } else {
            let history = self.areas.borrow().clone();
            let connection = self.connection.clone();
            let pool = self.pool.clone();
            let refresh_outputs = outputs.clone();
            let token = acquisition.token.clone();
            let refresh: capture::Refresh = Rc::new(move || {
                let (connection, pool, outputs, token) = (
                    connection.clone(),
                    pool.clone(),
                    refresh_outputs.clone(),
                    token.clone(),
                );
                Box::pin(async move {
                    super::captures::refresh_source(
                        &connection,
                        &pool,
                        &outputs,
                        capture_cursor,
                        &token,
                    )
                    .await
                })
            });
            let records = self
                .capture_records
                .borrow()
                .iter()
                .filter(|record| record.selection.limit == layout.limit)
                .cloned()
                .collect();
            let Some(selected) = capture::select(
                display,
                image,
                outputs,
                windows.iter().map(|w| w.rect).collect(),
                None,
                capture::Options {
                    without_cursor,
                    live: Some(configuration.clone()),
                    history,
                    records,
                    last_area: self.last_rect.get(),
                    refresh: Some(refresh),
                    commands: Some(self.capture_commands.clone()),
                    ocr_force_quick_copy: destination == Destination::Ocr,
                    ..options(settings)
                },
                self.pool.clone(),
                &acquisition.token,
            )
            .await?
            else {
                return Ok(json!({"cancelled":true}));
            };
            let selected = match selected {
                capture::Selected::Image(selected) => selected,
                capture::Selected::OpenPreferences(page) => {
                    self.preferences.open_page(&page).await?;
                    return Ok(json!({"cancelled":true}));
                }
                capture::Selected::RecognizedText { text, rect } => {
                    display.clipboard().set_text(&text);
                    return Ok(json!({"text":text,"rect":rect}));
                }
                capture::Selected::TextPin { text, rect } => {
                    let pos = self.pins.capture_position(rect);
                    return self
                        .paste(&json!({"source":"plain","text":text,"position":[pos.x,pos.y]}))
                        .await;
                }
                capture::Selected::Region { rect, action } => {
                    // Both continuous entry points leave the selector with only
                    // geometry. Never render an intermediate still on this path.
                    return match action {
                        Output::Scroll => self.start_scroll(rect, request, settings).await,
                        Output::Record => self.start_recording(rect, request, settings).await,
                        _ => Err(t("Invalid desktop response")),
                    };
                }
            };
            snapshot = Some(selected.snapshot);
            (selected.image, selected.rect, selected.action)
        };
        let destination = destination.resolve(action)?;
        let result = self
            .deliver_image(image, rect, destination, settings)
            .await?;
        if result["cancelled"] != true {
            self.last_rect.set(Some(rect));
            clivuu_core::capture::remember_area(
                &mut self.areas.borrow_mut(),
                rect,
                settings["capture-history-areas"].as_u64().unwrap_or(8) as usize,
            );
            let max = settings["capture-history-records"].as_u64().unwrap_or(20) as usize;
            let mut records = self.capture_records.borrow_mut();
            if let Some(mut snapshot) = snapshot {
                snapshot.cursor_pair = None;
                records.push(*snapshot);
            }
            while records.len() > max
                || records.len() > 1
                    && records
                        .iter()
                        .map(|r| r.source.as_raw().len())
                        .sum::<usize>()
                        > 128 * 1024 * 1024
            {
                records.remove(0);
            }
        }
        Ok(result)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Reuses the shared editor and explicit capture session dependencies"
    )]
    async fn window_image(
        &self,
        image: Arc<clivuu_core::image::RgbaImage>,
        outputs: Vec<clivuu_core::capture::Output>,
        destination: Destination,
        settings: &serde_json::Map<String, Value>,
        configuration: Rc<crate::infrastructure::configuration::Configuration>,
        display: &gdk::Display,
        cancel: &gio::Cancellable,
    ) -> Result<Value, String> {
        let bounds = window::presentation([image.width(), image.height()], outputs[0]);
        let (image, rect, destination) = if destination == Destination::Toolbar {
            let selected = capture::select(
                display,
                image,
                outputs,
                vec![],
                Some(bounds),
                capture::Options {
                    purpose: Purpose::Image,
                    commands: Some(self.capture_commands.clone()),
                    image_bounds: Some(bounds),
                    live: Some(configuration),
                    ocr_force_quick_copy: destination == Destination::Ocr,
                    ..options(settings)
                },
                self.pool.clone(),
                cancel,
            )
            .await?;
            let selected = match selected {
                Some(capture::Selected::Image(selected)) => selected,
                Some(capture::Selected::OpenPreferences(page)) => {
                    self.preferences.open_page(&page).await?;
                    return Ok(json!({"cancelled":true}));
                }
                Some(capture::Selected::RecognizedText { text, rect }) => {
                    display.clipboard().set_text(&text);
                    return Ok(json!({"text":text,"rect":rect}));
                }
                Some(capture::Selected::TextPin { text, rect }) => {
                    let pos = self.pins.capture_position(rect);
                    return self
                        .paste(&json!({"source":"plain","text":text,"position":[pos.x,pos.y]}))
                        .await;
                }
                _ => return Ok(json!({"cancelled":true})),
            };
            (
                selected.image,
                selected.rect,
                destination.resolve(selected.action)?,
            )
        } else {
            (
                crate::editor::frozen_image::FrozenImage::from_shared(image)
                    .ok_or_else(|| t("Invalid saved pin data"))?,
                bounds,
                destination,
            )
        };
        // Presentation coordinates are not the window's screen position, so do
        // not insert them into desktop region history or the --last selection.
        self.deliver_image(image, rect, destination, settings).await
    }
}

async fn refresh_source(
    connection: &gio::DBusConnection,
    pool: &Rc<TaskPool>,
    outputs: &[clivuu_core::capture::Output],
    capture_cursor: bool,
    cancel: &gio::Cancellable,
) -> Result<capture::RefreshedSource, String> {
    let display = gdk::Display::default().ok_or_else(|| t("The desktop service is unavailable"))?;
    if capture_cursor && !cursor::real_x11(&display) {
        return crate::platform::capture::still::capture(connection, pool, outputs, true, cancel)
            .await
            .map(|s| capture::RefreshedSource {
                image: s.image,
                without_cursor: s.without_cursor,
            });
    }
    let pointer = if capture_cursor {
        Some(cursor::snapshot(pool, cancel).await?)
    } else {
        None
    };
    let image = Screenshot::connect(cancel)
        .await?
        .capture(pool, cancel)
        .await?;
    if let Some(pointer) = pointer {
        let outputs = gdk_outputs(&display);
        let root = Selection::new(outputs.clone(), vec![], None)
            .ok_or_else(|| t("Invalid monitor layout"))?
            .limit;
        let clean = image.clone();
        let image = cursor::composite(image, pointer, root, outputs[0].scale, pool, cancel).await?;
        Ok(capture::RefreshedSource {
            image,
            without_cursor: Some(clean),
        })
    } else {
        Ok(capture::RefreshedSource {
            image,
            without_cursor: None,
        })
    }
}
