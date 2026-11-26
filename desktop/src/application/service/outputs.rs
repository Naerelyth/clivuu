//! Image publication and reserved output destinations, shared by all sessions.
use super::*;
use crate::infrastructure::saving::{self, Mode};
use clivuu_core::saving::Media;
impl State {
    pub(super) fn pin_capture(
        &self,
        image: crate::editor::frozen_image::FrozenImage,
        rect: Bounds,
        source: String,
    ) -> Result<u64, String> {
        let id = self
            .pins
            .add(image, Some(self.pins.capture_position(rect)))?;
        self.pins.board.borrow_mut().get_mut(id).unwrap().source = Some(source);
        self.pins.commit();
        Ok(id)
    }
    pub(super) async fn deliver_image(
        &self,
        image: crate::editor::frozen_image::FrozenImage,
        rect: Bounds,
        destination: Destination,
        settings: &serde_json::Map<String, Value>,
    ) -> Result<Value, String> {
        let records = settings["capture-history-records"].as_u64().unwrap_or(20) as usize;
        let automatic =
            settings["image-auto-save"] == true && destination != Destination::NoAutoSave;
        let duplicate_quick = destination == Destination::Quick
            && saving::directory(settings, Media::Image, Mode::Quick)
                == saving::directory(settings, Media::Image, Mode::Automatic)
            && settings["image-quick-name"] == settings["image-auto-name"];
        let automatic_target = if automatic && !duplicate_quick {
            Some(saving::target(
                settings,
                Media::Image,
                Mode::Automatic,
                &saving::now()?,
            )?)
        } else {
            None
        };
        // Capture identity belongs to the completed capture, independently of
        // clipboard contents or later edits to a pin of this capture.
        let source = glib::compute_checksum_for_string(
            glib::ChecksumType::Sha256,
            format!("capture:{}", glib::uuid_string_random()),
        )
        .expect("SHA-256 checksum")
        .to_string();
        let published = match destination {
            Destination::Pin => {
                let id = self.pin_capture(image.clone(), rect, source.clone())?;
                Some(json!({"pinned":true,"rect":rect,"id":id}))
            }
            Destination::Discard | Destination::NoAutoSave => Some(json!({"ok":true,"rect":rect})),
            _ => None,
        };
        if records == 0 {
            self.history.borrow_mut().clear();
            self.refresh_menus();
            if automatic_target.is_none()
                && let Some(result) = published.as_ref()
            {
                return Ok(result.clone());
            }
        }
        let png = crate::editor::export::encode(image.pixels, &self.pool, &self.work_cancel())
            .await
            .map_err(|e| format!("{e:?}"))?;
        let history_png = if records > 0 { png.clone() } else { vec![] };
        let automatic_bytes = automatic_target.as_ref().map(|_| png.clone());
        let mut result = if let Some(result) = published {
            result
        } else {
            self.deliver(png, rect, destination, settings).await?
        };
        if result["cancelled"] != true && records > 0 {
            {
                let mut history = self.history.borrow_mut();
                history.push(CaptureRecord {
                    png: Arc::new(history_png),
                    rect,
                    source,
                });
                while history.len() > records
                    || history.len() > 1
                        && history.iter().map(|record| record.png.len()).sum::<usize>()
                            > 128 * 1024 * 1024
                {
                    history.remove(0);
                }
            }
            self.refresh_menus();
        }
        if result["cancelled"] != true
            && let (Some(target), Some(bytes)) = (automatic_target, automatic_bytes)
            && result["path"].as_str().map(std::path::Path::new) != Some(target.as_path())
        {
            match self
                .write_image_unique(bytes, target, clivuu_core::config::image_quality(settings))
                .await
            {
                Ok(path) => {
                    if settings["image-save-notify"] == true {
                        self.notifications.send_file(
                            "saving",
                            &t("Image saved"),
                            &path.to_string_lossy(),
                            &path,
                            Media::Image,
                        );
                    }
                    result["automatic_path"] = json!(path);
                }
                Err(error) => {
                    self.saved_notification("Automatic save failed", &error);
                    result["automatic_error"] = json!(error);
                }
            }
        }
        Ok(result)
    }
    pub(super) async fn deliver(
        &self,
        png: Vec<u8>,
        rect: Bounds,
        destination: Destination,
        settings: &serde_json::Map<String, Value>,
    ) -> Result<Value, String> {
        let display =
            gdk::Display::default().ok_or_else(|| t("The desktop service is unavailable"))?;
        if matches!(destination, Destination::Printer | Destination::Barcode) {
            let directory = crate::infrastructure::temporary::Directory::new(
                &self.pool,
                "output",
                &self.work_cancel(),
            )
            .await?;
            let path = directory
                .write(&self.pool, png, &self.work_cancel())
                .await?;
            let text = if destination == Destination::Printer {
                crate::infrastructure::process::output(
                    &["lp", path.to_str().unwrap()],
                    &self.work_cancel(),
                )
                .await?
            } else {
                crate::infrastructure::process::output(
                    &["zbarimg", "--quiet", "--raw", path.to_str().unwrap()],
                    &self.work_cancel(),
                )
                .await?
            };
            if destination == Destination::Printer {
                return Ok(json!({"printed":true}));
            }
            display.clipboard().set_text(&text);
            return Ok(json!({"text":text}));
        }
        if destination == Destination::Quick {
            let target = saving::target(settings, Media::Image, Mode::Quick, &saving::now()?)?;
            let path = self
                .write_image_unique(png, target, clivuu_core::config::image_quality(settings))
                .await?;
            if settings["image-save-notify"] == true {
                self.notifications.send_file(
                    "saving",
                    &t("Image saved"),
                    &path.to_string_lossy(),
                    &path,
                    Media::Image,
                );
            }
            return Ok(json!({"path":path,"rect":rect}));
        }
        if matches!(destination, Destination::Dialog | Destination::Path(_)) {
            let manual = destination == Destination::Dialog;
            let suggested = if manual {
                Some(saving::name(
                    settings,
                    Media::Image,
                    Mode::Manual,
                    &saving::now()?,
                )?)
            } else {
                None
            };
            let (file, selected_extension) = if let Destination::Path(path) = &destination {
                (gio::File::for_path(path), "png")
            } else {
                let Some(file) = crate::application::save_dialog::choose(
                    Media::Image,
                    suggested.as_deref().unwrap(),
                    &self.operation_cancel(),
                )
                .await?
                else {
                    return Ok(json!({"cancelled":true}));
                };
                (file.file, file.extension)
            };
            let basename = file
                .basename()
                .ok_or_else(|| t("Choose a PNG or JPEG file name"))?;
            let format = if basename.extension().is_none() {
                // Choose the encoding without changing the exact path approved
                // by the chooser (appending a suffix could bypass overwrite consent).
                basename.with_extension(selected_extension)
            } else {
                basename
            };
            let extension =
                clivuu_core::saving::extension(&format, Media::Image).map_err(|e| e.to_string())?;
            let quality = clivuu_core::config::image_quality(settings);
            let bytes = if extension == "png" {
                png
            } else {
                self.pool
                    .submit(move |token| {
                        token.check()?;
                        let mut encoded = Vec::new();
                        clivuu_core::image::export::write(
                            clivuu_core::image::export::Source::Png(&png),
                            &format,
                            quality,
                            &mut encoded,
                        )
                        .map_err(|e| TaskError::Failed(e.to_string()))?;
                        Ok(encoded)
                    })
                    .map_err(|e| e.to_string())?
                    .result()
                    .await
                    .map_err(|e| e.to_string())?
            };
            gio::CancellableFuture::new(
                file.replace_contents_future(
                    bytes,
                    None,
                    false,
                    gio::FileCreateFlags::REPLACE_DESTINATION,
                ),
                self.work_cancel(),
            )
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|(_, e)| e.to_string())?;
            if manual
                && settings["image-remember-extension"] == true
                && let Some(config) = self.configuration.borrow().as_ref()
            {
                config.set("image-last-extension", json!(extension))?;
            }
            return Ok(json!({"path":file.path(),"rect":rect}));
        }
        if matches!(destination, Destination::Pin) {
            let task = self
                .pool
                .submit(move |token| {
                    token.check()?;
                    let image = clivuu_core::image_ops::decode(&png)
                        .map_err(|e| TaskError::Failed(e.to_string()))?;
                    token.check()?;
                    Ok(image)
                })
                .map_err(|e| e.to_string())?;
            let image = gio::CancellableFuture::new(task.result(), self.work_cancel())
                .await
                .map_err(|_| t("Operation cancelled"))?
                .map_err(|e| e.to_string())?;
            self.pins.add(
                crate::editor::frozen_image::FrozenImage::new(image).ok_or_else(|| {
                    t("The image exceeds 48 megapixels or has invalid dimensions")
                })?,
                Some(self.pins.capture_position(rect)),
            )?;
            return Ok(json!({"pinned":true,"rect":rect}));
        }
        if matches!(destination, Destination::Ocr) {
            let ocr_settings = clivuu_core::ocr::OcrSettings::from_values(settings);
            let smart_paragraphs = ocr_settings.smart_paragraphs;
            let task = self
                .pool
                .submit(move |token| {
                    token.check()?;
                    let image = clivuu_core::image_ops::decode(&png)
                        .map_err(|e| TaskError::Failed(e.to_string()))?;
                    let doc = crate::infrastructure::ocr::recognize_blocking(
                        &image,
                        &ocr_settings,
                        &token,
                    )?;
                    token.check()?;
                    Ok(doc.full_text(smart_paragraphs))
                })
                .map_err(|e| e.to_string())?;
            let text = gio::CancellableFuture::new(task.result(), self.work_cancel())
                .await
                .map_err(|_| t("Operation cancelled"))?
                .map_err(|e| e.to_string())?;
            display.clipboard().set_text(&text);
            return Ok(json!({"text":text,"rect":rect}));
        }
        if matches!(destination, Destination::Discard | Destination::NoAutoSave) {
            return Ok(json!({"ok":true,"rect":rect}));
        }
        if matches!(destination, Destination::Clipboard) {
            crate::platform::integration::clipboard::set_image(&display, png, None)?;
            return Ok(json!({"ok":true,"rect":rect,"action":"clipboard"}));
        }
        Err(t("Invalid desktop response"))
    }
    async fn write_image_unique(
        &self,
        png: Vec<u8>,
        target: std::path::PathBuf,
        quality: i32,
    ) -> Result<std::path::PathBuf, String> {
        let task = self
            .pool
            .submit(move |token| {
                token.check()?;
                saving::write_unique(&png, &target, quality).map_err(TaskError::Failed)
            })
            .map_err(|e| e.to_string())?;
        gio::CancellableFuture::new(task.result(), self.work_cancel())
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|e| e.to_string())
    }
    fn saved_notification(&self, title: &str, message: &str) {
        self.notifications.send("saving", &t(title), message);
    }
    pub(super) async fn pin_action(&self, id: u64, action: Output) -> Result<(), String> {
        let _activity = self.activity()?;
        match action {
            Output::Preferences => return self.preferences.open().await,
            Output::OcrPreferences => return self.preferences.open_page("ocr").await,
            Output::Ocr => {
                if self.pins.toggle_ocr(id) {
                    return Ok(());
                }
            }
            Output::CopyText => {
                let text = self
                    .pins
                    .board
                    .borrow()
                    .get(id)
                    .and_then(|p| p.plain_text.clone());
                if let Some(text) = text {
                    gdk::Display::default()
                        .ok_or_else(|| t("The desktop service is unavailable"))?
                        .clipboard()
                        .set_text(&text);
                }
                return Ok(());
            }
            Output::ClosePin | Output::DestroyPin => {
                let ids = self.pins.shortcut_targets(id);
                let destroy = matches!(action, Output::DestroyPin);
                let ask = if destroy {
                    self.pins.preferences.borrow().confirm_destroy
                } else {
                    self.pins.preferences.borrow().confirm_close
                };
                if ids.len() > 1
                    && ask
                    && !super::super::confirmation::pins(destroy, ids.len(), &self.work_cancel())
                        .await?
                {
                    return Ok(());
                }
                for id in ids {
                    if destroy {
                        self.pins.destroy(id);
                    } else {
                        self.pins.close(id);
                    }
                }
                return Ok(());
            }
            Output::ReplacePin => return self.replace_pin(id).await,
            _ => {}
        }
        if self
            .pins
            .edit_action(id, action, &self.work_cancel())
            .await?
        {
            return Ok(());
        }
        let task = self
            .pool
            .submit(|_| config::read().map_err(|e| TaskError::Failed(e.to_string())))
            .map_err(|e| e.to_string())?;
        let settings = task.result().await.map_err(|e| e.to_string())?;
        let (png, rect, origin) = self.pins.selection_png(id, &self.work_cancel()).await?;
        if matches!(action, Output::Copy) {
            let display =
                gdk::Display::default().ok_or_else(|| t("The desktop service is unavailable"))?;
            let origin = origin.map(|id| format!("{}:{id}", std::process::id()));
            crate::platform::integration::clipboard::set_image(&display, png, origin.as_deref())?;
            return Ok(());
        }
        self.deliver(png, rect, Destination::Toolbar.resolve(action)?, &settings)
            .await?;
        Ok(())
    }
}

/// Reserved protocol actions must never fall through to filesystem paths.
#[derive(Debug, PartialEq)]
pub(super) enum Destination {
    Toolbar,
    Clipboard,
    Printer,
    Barcode,
    Pin,
    Ocr,
    Dialog,
    Quick,
    NoAutoSave,
    Discard,
    Path(String),
}
impl Destination {
    pub(super) fn resolve(self, action: Output) -> Result<Self, String> {
        if self != Self::Toolbar {
            return Ok(self);
        }
        match action {
            Output::Copy => Ok(Self::Clipboard),
            Output::Save => Ok(Self::Dialog),
            Output::QuickSave => Ok(Self::Quick),
            Output::Ocr => Ok(Self::Ocr),
            Output::Pin => Ok(Self::Pin),
            _ => Err(t("Invalid desktop response")),
        }
    }
    pub(super) fn parse(value: Option<&Value>) -> Result<Self, String> {
        let value = match value {
            None | Some(Value::Null) => return Ok(Self::Toolbar),
            Some(Value::String(value)) => value.as_str(),
            _ => return Err(t("Invalid desktop response")),
        };
        match value {
            "clipboard" => Ok(Self::Clipboard),
            "file-dialog" => Ok(Self::Dialog),
            "quick-save" => Ok(Self::Quick),
            "no-auto-save" => Ok(Self::NoAutoSave),
            "success" | "silent" => Ok(Self::Discard),
            "ocr_clipboard" => Ok(Self::Ocr),
            "pin" => Ok(Self::Pin),
            "printer" => Ok(Self::Printer),
            "barcode" => Ok(Self::Barcode),
            "" => Err(t("Invalid desktop response")),
            path => Ok(Self::Path(path.into())),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_output_overrides_toolbar_action() {
        for action in [Output::Save, Output::Pin, Output::Ocr] {
            assert_eq!(
                Destination::Clipboard.resolve(action).unwrap(),
                Destination::Clipboard
            );
            assert_eq!(
                Destination::Discard.resolve(action).unwrap(),
                Destination::Discard
            );
            assert_eq!(
                Destination::Path("out.png".into()).resolve(action).unwrap(),
                Destination::Path("out.png".into())
            );
        }
        assert_eq!(
            Destination::Toolbar.resolve(Output::Save).unwrap(),
            Destination::Dialog
        );
        assert_eq!(
            Destination::Toolbar.resolve(Output::Pin).unwrap(),
            Destination::Pin
        );
    }
    #[test]
    fn output_actions_are_not_paths() {
        assert_eq!(
            Destination::parse(Some(&json!("file-dialog"))).unwrap(),
            Destination::Dialog
        );
        assert_eq!(
            Destination::parse(Some(&json!("quick-save"))).unwrap(),
            Destination::Quick
        );
        assert_eq!(
            Destination::parse(Some(&json!("no-auto-save"))).unwrap(),
            Destination::NoAutoSave
        );
        for token in ["silent", "success"] {
            assert_eq!(
                Destination::parse(Some(&json!(token))).unwrap(),
                Destination::Discard
            );
        }
        assert_eq!(
            Destination::parse(Some(&json!("pin"))).unwrap(),
            Destination::Pin
        );
        assert_eq!(
            Destination::parse(Some(&json!("printer"))).unwrap(),
            Destination::Printer
        );
        assert_eq!(
            Destination::parse(Some(&json!("barcode"))).unwrap(),
            Destination::Barcode
        );
        assert!(Destination::parse(Some(&json!(42))).is_err());
        assert_eq!(
            Destination::parse(Some(&json!("./silent"))).unwrap(),
            Destination::Path("./silent".into())
        );
    }
}
