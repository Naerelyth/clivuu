//! Every paste entry uses one request, content conversion and reveal/create path.
use super::*;
use crate::{application::text_image, platform::integration::clipboard};
use clivuu_core::{
    paste::{self, Request, Source},
    pins::preferences::TextOptions,
};
use std::{io::Read, path::PathBuf};
struct Prepared {
    content: Content,
    source: String,
}
enum Content {
    Raster(clivuu_core::image::RgbaImage, paste::Units),
    // The destination surface determines the raster density after MIME selection.
    Color([u8; 4]),
    Text(paste::html::Document, TextOptions),
    Ignored,
}
fn color(rgba: [u8; 4]) -> Prepared {
    Prepared {
        content: Content::Color(rgba),
        // Older cards contain a 1x raster even on high-DPI displays. Preserve
        // existing edited pins but don't restore that obsolete generated image.
        source: paste::identity("color:hidpi-v1", &rgba),
    }
}
fn text(
    text: String,
    html: bool,
    automatic: bool,
    options: &TextOptions,
) -> Result<Prepared, String> {
    paste::check_text(&text)?;
    let mut document = if html {
        paste::html::parse(&text)?
    } else {
        paste::html::Document::plain_text(&text)
    };
    let plain = document.plain();
    if automatic && let Some(rgba) = paste::color(&plain) {
        return Ok(color(rgba));
    }
    if !options.enabled {
        return Ok(Prepared {
            content: Content::Ignored,
            source: String::new(),
        });
    }
    if options.ignore_html {
        document = paste::html::Document::plain_text(&plain);
    }
    let settings = serde_json::to_vec(options).map_err(|e| e.to_string())?;
    let source = paste::identity_parts(
        if html {
            "html:hidpi-v2"
        } else {
            "text:hidpi-v2"
        },
        &[text.as_bytes(), &settings],
    );
    Ok(Prepared {
        content: Content::Text(document, options.clone()),
        source,
    })
}

fn image(bytes: &[u8], kind: &str) -> Result<Prepared, String> {
    use gtk::gdk_pixbuf::prelude::PixbufLoaderExt;
    let loader = gtk::gdk_pixbuf::PixbufLoader::new();
    let oversized = Rc::new(Cell::new(false));
    let flag = oversized.clone();
    loader.connect_size_prepared(move |loader, w, h| {
        if w <= 0 || h <= 0 || w as u64 * h as u64 > clivuu_core::image_ops::MAX_PIXELS {
            flag.set(true);
            loader.set_size(1, 1);
        }
    });
    let written = loader.write(bytes);
    let closed = loader.close();
    if oversized.get() {
        return Err(t(
            "The image exceeds 48 megapixels or has invalid dimensions",
        ));
    }
    written.and(closed).map_err(|e| e.to_string())?;
    let p = loader
        .pixbuf()
        .ok_or_else(|| t("The clipboard contains no image or text"))?;
    let (w, h) = (p.width() as u32, p.height() as u32);
    let stride = p.rowstride() as usize;
    let channels = p.n_channels() as usize;
    if p.bits_per_sample() != 8 || ![3, 4].contains(&channels) {
        return Err(t("Invalid paste request"));
    }
    let data = p.read_pixel_bytes();
    let mut pixels = clivuu_core::image::RgbaImage::new(w, h);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let n = y * stride + x * channels;
            pixels.put_pixel(
                x as u32,
                y as u32,
                clivuu_core::image::Rgba([
                    data[n],
                    data[n + 1],
                    data[n + 2],
                    if channels == 4 { data[n + 3] } else { 255 },
                ]),
            );
        }
    }
    // A distinct identity namespace prevents reusing legacy imports whose base
    // geometry accidentally treated physical image pixels as logical pixels.
    let source = paste::identity_parts(
        &format!("{kind}:physical-pixels"),
        &[&w.to_le_bytes(), &h.to_le_bytes(), pixels.as_raw()],
    );
    Ok(Prepared {
        source,
        content: Content::Raster(pixels, paste::Units::PhysicalPixels),
    })
}
fn files(
    paths: Vec<PathBuf>,
    strict: bool,
    options: &TextOptions,
    token: &crate::infrastructure::tasks::Cancellation,
) -> Result<(Vec<Prepared>, Option<Prepared>), TaskError> {
    let mut images = Vec::new();
    let mut other = Vec::new();
    let mut budget = 0usize;
    for path in paths {
        token.check()?;
        let result = (|| {
            let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
            if !metadata.is_file() || metadata.len() > clivuu_core::image_ops::MAX_BYTES {
                return Err(t("The request is too large"));
            }
            let mut bytes = Vec::new();
            std::fs::File::open(&path)
                .map_err(|e| e.to_string())?
                .take(clivuu_core::image_ops::MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > clivuu_core::image_ops::MAX_BYTES {
                return Err(t("The request is too large"));
            }
            image(&bytes, &format!("file:{}", path.to_string_lossy()))
        })();
        match result {
            Ok(prepared) => {
                if let Content::Raster(pixels, _) = &prepared.content {
                    budget += pixels.len();
                }
                if budget > (clivuu_core::image_ops::MAX_PIXELS * 4) as usize {
                    return Err(TaskError::Failed(t("The request is too large")));
                }
                images.push(prepared);
            }
            Err(e) if strict => return Err(TaskError::Failed(e)),
            Err(_) => other.push(path.to_string_lossy().into_owned()),
        }
    }
    let fallback = if images.is_empty() && !other.is_empty() {
        Some(if options.skip_paths {
            Prepared {
                content: Content::Ignored,
                source: String::new(),
            }
        } else {
            text(other.join("\n"), false, false, options).map_err(TaskError::Failed)?
        })
    } else {
        None
    };
    token.check()?;
    Ok((images, fallback))
}
fn string(bytes: Vec<u8>) -> Result<String, String> {
    let value = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little = bytes[0] == 0xff;
        let chars: Vec<_> = bytes[2..]
            .chunks_exact(2)
            .map(|v| {
                if little {
                    u16::from_le_bytes([v[0], v[1]])
                } else {
                    u16::from_be_bytes([v[0], v[1]])
                }
            })
            .collect();
        String::from_utf16(&chars).map_err(|e| e.to_string())?
    } else {
        String::from_utf8(bytes)
            .map_err(|e| e.to_string())?
            .trim_start_matches('\u{feff}')
            .to_owned()
    };
    paste::check_text(&value)?;
    Ok(value)
}
fn paths(bytes: Vec<u8>, gnome: bool) -> Result<Vec<PathBuf>, String> {
    let value = string(bytes)?;
    let mut lines = value.lines();
    if gnome {
        let _ = lines.next();
    }
    let paths: Vec<_> = lines
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .filter_map(|uri| gio::File::for_uri(uri).path())
        .collect();
    if paths.len() > paste::MAX_FILES {
        return Err(t("--files requires 1 to 32 image files"));
    }
    Ok(paths)
}
impl State {
    pub(super) async fn drop_image(&self, id: u64, drop: gdk::Drop, preserve: bool) {
        let result = async {
            let _activity = self.activity()?;
            if !self.ready.get() || self.busy.replace(true) {
                return Err(t("A capture or import session is already in progress"));
            }
            let _busy = Busy(&self.busy);
            let operation = Scope::new(&self.work_cancel());
            self.operation.replace(Some(operation.token.clone()));
            let _operation = Operation(&self.operation);
            let rect = self
                .pins
                .drop_destination(id, None)
                .ok_or_else(|| t("Operation cancelled"))?;
            let group = self.pins.board.borrow().get(id).unwrap().group.clone();
            let before: std::collections::HashSet<_> =
                self.pins.board.borrow().pins.iter().map(|p| p.id).collect();
            let result = self
                .paste_with_offer(
                    &json!({"position":[rect.x,rect.y]}),
                    Some(clipboard::Offer::from_drop(&drop, self.pool.clone())),
                    Some((id, group)),
                )
                .await;
            if result.is_err() {
                let created: Vec<_> = self
                    .pins
                    .board
                    .borrow()
                    .pins
                    .iter()
                    .filter(|p| !before.contains(&p.id))
                    .map(|p| p.id)
                    .collect();
                for id in created {
                    self.pins.destroy(id);
                }
            }
            let result = result?;
            if !preserve && result["pinned"].as_u64().unwrap_or(0) > 0 {
                if let Some(p) = self.pins.board.borrow_mut().get_mut(id) {
                    p.hidden = true;
                }
                self.pins.commit();
            }
            Ok::<_, String>(result)
        }
        .await;
        drop.finish(if result.is_ok() {
            gdk::DragAction::COPY
        } else {
            gdk::DragAction::empty()
        });
        if let Err(error) = result {
            self.report_error(&error);
        }
    }
    pub(super) async fn replace_pin(&self, id: u64) -> Result<(), String> {
        let _activity = self.activity()?;
        if !self.ready.get() || self.busy.replace(true) {
            return Err(t("A capture or import session is already in progress"));
        }
        let _busy = Busy(&self.busy);
        let operation = Scope::new(&self.work_cancel());
        self.operation.replace(Some(operation.token.clone()));
        let _operation = Operation(&self.operation);
        let rect = self
            .pins
            .drop_destination(id, None)
            .ok_or_else(|| t("Operation cancelled"))?;
        let group = self.pins.board.borrow().get(id).unwrap().group.clone();
        self.paste_with_offer(
            &json!({"position":[rect.x,rect.y]}),
            None,
            Some((id, group)),
        )
        .await?;
        Ok(())
    }
    pub(super) async fn paste(&self, value: &Value) -> Result<Value, String> {
        self.paste_with_offer(value, None, None).await
    }
    async fn paste_with_offer(
        &self,
        value: &Value,
        supplied: Option<clipboard::Offer>,
        destination: Option<(u64, String)>,
    ) -> Result<Value, String> {
        let dropping = supplied.is_some();
        let replacing = !dropping && destination.is_some();
        let request = Request::parse(value)?;
        if !dropping && !replacing && self.pins.paste_hides_flashing() {
            return Ok(json!({"pinned":0,"ids":[],"hidden":true}));
        }
        let options = self.pins.preferences.borrow().text.clone();
        let cancel = self.operation_cancel();
        let mut origin = None;
        let mut prepared = Vec::new();
        match request.source {
            Source::Files(paths) => {
                let task = self
                    .pool
                    .submit(move |token| {
                        files(paths, true, &options, &token).map(|(images, _)| images)
                    })
                    .map_err(|e| e.to_string())?;
                prepared = gio::CancellableFuture::new(task.result(), cancel.clone())
                    .await
                    .map_err(|_| t("Operation cancelled"))?
                    .map_err(|e| e.to_string())?;
            }
            Source::Text { text: input, html } => {
                let task = self
                    .pool
                    .submit(move |token| {
                        token.check()?;
                        text(input, html, false, &options).map_err(TaskError::Failed)
                    })
                    .map_err(|e| e.to_string())?;
                prepared.push(
                    gio::CancellableFuture::new(task.result(), cancel.clone())
                        .await
                        .map_err(|_| t("Operation cancelled"))?
                        .map_err(|e| e.to_string())?,
                );
            }
            Source::Clipboard => {
                let display = gdk::Display::default()
                    .ok_or_else(|| t("The desktop service is unavailable"))?;
                let offer = match supplied {
                    Some(offer) => offer,
                    None => clipboard::Offer::new(&display, self.pool.clone(), &cancel).await?,
                };
                let mut file_paths = None;
                let mut last_error = None;
                for offered in offer.candidates() {
                    let mime = offered.to_ascii_lowercase();
                    if !mime.starts_with("image/")
                        && ![
                            clipboard::ORIGIN,
                            "text/uri-list",
                            "x-special/gnome-copied-files",
                        ]
                        .contains(&mime.as_str())
                        && let Some(paths) = file_paths.take()
                    {
                        prepared.push(paths);
                        break;
                    }
                    let transfer = Scope::new(&cancel);
                    let result = glib::future_with_timeout(
                        std::time::Duration::from_secs(10),
                        offer.read(&offered, &transfer.token),
                    )
                    .await;
                    let bytes = match result {
                        Ok(Ok(bytes)) => bytes,
                        result => {
                            if cancel.is_cancelled() {
                                return Err(t("Operation cancelled"));
                            }
                            last_error = Some(match result {
                                Ok(Err(error)) => error,
                                Err(_) => t("Clipboard transfer timed out"),
                                _ => unreachable!(),
                            });
                            continue;
                        }
                    };
                    if mime == clipboard::ORIGIN {
                        if dropping || replacing {
                            continue;
                        }
                        if let Ok(value) = string(bytes)
                            && let Some((pid, id)) = value.split_once(':')
                            && pid.parse::<u32>().ok() == Some(std::process::id())
                            && let Ok(id) = id.parse::<u64>()
                            && self.pins.board.borrow().get(id).is_some()
                        {
                            origin = Some(id);
                            break;
                        }
                        continue;
                    }
                    let is_files =
                        mime == "text/uri-list" || mime == "x-special/gnome-copied-files";
                    if is_files {
                        let paths = paths(bytes, mime.starts_with("x-special"))?;
                        if paths.is_empty() {
                            continue;
                        }
                        let options = options.clone();
                        let task = self
                            .pool
                            .submit(move |token| files(paths, false, &options, &token))
                            .map_err(|e| e.to_string())?;
                        match gio::CancellableFuture::new(task.result(), cancel.clone())
                            .await
                            .map_err(|_| t("Operation cancelled"))?
                        {
                            Ok((images, fallback)) => {
                                prepared = images;
                                file_paths = fallback;
                            }
                            Err(e) => {
                                last_error = Some(e.to_string());
                                continue;
                            }
                        }
                    } else {
                        let options = options.clone();
                        let task = self
                            .pool
                            .submit(move |token| {
                                token.check()?;
                                if mime.starts_with("image/") {
                                    image(&bytes, "image").map_err(TaskError::Failed)
                                } else if mime == "application/x-color" && bytes.len() == 8 {
                                    let rgba: Vec<_> = bytes
                                        .chunks_exact(2)
                                        .map(|v| (u16::from_ne_bytes([v[0], v[1]]) >> 8) as u8)
                                        .collect();
                                    let rgba: [u8; 4] = rgba.try_into().unwrap();
                                    Ok(color(rgba))
                                } else {
                                    let html = mime.starts_with("text/html");
                                    text(
                                        string(bytes).map_err(TaskError::Failed)?,
                                        html,
                                        true,
                                        &options,
                                    )
                                    .map_err(TaskError::Failed)
                                }
                            })
                            .map_err(|e| e.to_string())?;
                        match gio::CancellableFuture::new(task.result(), cancel.clone())
                            .await
                            .map_err(|_| t("Operation cancelled"))?
                        {
                            Ok(image) => prepared.push(image),
                            Err(e) => {
                                last_error = Some(e.to_string());
                                continue;
                            }
                        }
                    }
                    if !prepared.is_empty() {
                        break;
                    }
                }
                if prepared.is_empty() && origin.is_none() {
                    if let Some(paths) = file_paths {
                        prepared.push(paths);
                    } else if let Some(error) = last_error {
                        return Err(error);
                    }
                }
            }
        }
        if cancel.is_cancelled() {
            return Err(t("Operation cancelled"));
        }
        if let Some(id) = origin.filter(|_| !replacing) {
            return self.reveal_paste(id, &cancel).await;
        }
        prepared.retain(|p| !matches!(p.content, Content::Ignored));
        if prepared.is_empty() {
            return Ok(json!({"pinned":0,"ids":[]}));
        }
        let display =
            gdk::Display::default().ok_or_else(|| t("The desktop service is unavailable"))?;
        let outputs = crate::platform::surfaces::capture::gdk_outputs(&display);
        let focused = crate::platform::integration::windows::query(&display, &outputs, &cancel)
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|w| w.focused);
        let mut output = paste::target(&outputs, request.position, focused.map(|w| w.rect))
            .ok_or_else(|| t("Invalid monitor layout"))?;
        if prepared
            .iter()
            .any(|p| !matches!(p.content, Content::Raster(_, paste::Units::LogicalPixels)))
        {
            output.scale = self.pins.pixel_scale(output.rect, &cancel).await?;
        }
        let validate_destination = || -> Result<(), String> {
            if cancel.is_cancelled()
                || destination.as_ref().is_some_and(|(id, group)| {
                    self.pins.drop_destination(*id, Some(group)).is_none()
                })
            {
                return Err(t("Operation cancelled"));
            }
            Ok(())
        };
        validate_destination()?;
        if replacing && prepared.len() != 1 {
            return Err(t("The clipboard contains no image or text"));
        }
        let mut new = std::collections::HashSet::new();
        let mut reopening = std::collections::HashSet::new();
        {
            let b = self.pins.board.borrow();
            for p in &prepared {
                if let Some(id) = (!dropping && !replacing)
                    .then(|| paste::existing(&b, &p.source))
                    .flatten()
                {
                    if b.get(id).is_some_and(|p| p.closed) {
                        reopening.insert(id);
                    }
                } else {
                    new.insert(p.source.clone());
                }
            }
            let new_count = if replacing {
                0
            } else if dropping {
                prepared.len()
            } else {
                new.len()
            };
            if b.pins.iter().filter(|p| !p.closed).count() + new_count + reopening.len()
                > clivuu_core::pins::ACTIVE_LIMIT
            {
                return Err(t("At most 32 pins are allowed"));
            }
            if b.pins.len() + new_count > clivuu_core::pins::STORED_LIMIT {
                return Err(t(
                    "At most 128 stored pins are allowed. Destroy closed pins to free space.",
                ));
            }
        }
        let mut ids = Vec::new();
        let mut reused = 0;
        for p in prepared {
            let plain = match &p.content {
                Content::Text(doc, _) => Some(doc.plain()),
                _ => None,
            };
            if cancel.is_cancelled() {
                return Err(t("Operation cancelled"));
            }
            let existing = (!dropping && !replacing)
                .then(|| paste::existing(&self.pins.board.borrow(), &p.source))
                .flatten();
            let id = if let Some(id) = existing {
                self.pins.reveal(id, &cancel).await?;
                reused += 1;
                id
            } else {
                let (pixels, size, units) = match p.content {
                    Content::Ignored => unreachable!(),
                    Content::Text(document, options) => {
                        let scale = output.scale;
                        let task = self
                            .pool
                            .submit(move |token| {
                                token.check()?;
                                text_image::render_document_with_options(&document, &options, scale)
                                    .map_err(TaskError::Failed)
                            })
                            .map_err(|e| e.to_string())?;
                        let (pixels, size) =
                            gio::CancellableFuture::new(task.result(), cancel.clone())
                                .await
                                .map_err(|_| t("Operation cancelled"))?
                                .map_err(|e| e.to_string())?;
                        (pixels, size, paste::Units::LogicalPixels)
                    }
                    Content::Raster(pixels, units) => {
                        let size = [pixels.width(), pixels.height()];
                        (pixels, size, units)
                    }
                    Content::Color(rgba) => {
                        let scale = output.scale;
                        let task = self
                            .pool
                            .submit(move |token| {
                                token.check()?;
                                let pixels = text_image::color_card(rgba, scale)
                                    .map_err(TaskError::Failed)?;
                                token.check()?;
                                Ok(pixels)
                            })
                            .map_err(|e| e.to_string())?;
                        let pixels = gio::CancellableFuture::new(task.result(), cancel.clone())
                            .await
                            .map_err(|_| t("Operation cancelled"))?
                            .map_err(|e| e.to_string())?;
                        (
                            pixels,
                            text_image::COLOR_CARD_SIZE,
                            paste::Units::LogicalPixels,
                        )
                    }
                };
                let mut rect = paste::centered(size, output, units);
                if let Some([x, y]) = request.position {
                    rect.x = x;
                    rect.y = y;
                }
                let image =
                    crate::editor::frozen_image::FrozenImage::new(pixels).ok_or_else(|| {
                        t("The image exceeds 48 megapixels or has invalid dimensions")
                    })?;
                validate_destination()?;
                self.pins.board.borrow_mut().hidden = false;
                let id = if replacing {
                    let id = destination.as_ref().unwrap().0;
                    self.pins.replace_source(id, image, rect)?;
                    id
                } else {
                    self.pins.add(image, Some(rect))?
                };
                {
                    let mut board = self.pins.board.borrow_mut();
                    let pin = board.get_mut(id).unwrap();
                    pin.source = Some(p.source);
                    pin.plain_text = plain;
                }
                self.pins.commit();
                id
            };
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        Ok(json!({"pinned":ids.len(),"ids":ids,"reused":reused}))
    }
    async fn reveal_paste(&self, id: u64, cancel: &gio::Cancellable) -> Result<Value, String> {
        let restored = self.pins.reveal(id, cancel).await?;
        Ok(
            json!({"pinned":usize::from(restored),"ids":if restored{vec![id]}else{vec![]},"reused":usize::from(restored)}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(input: String, html: bool, automatic: bool) -> Result<Prepared, String> {
        super::text(
            input,
            html,
            automatic,
            &clivuu_core::pins::preferences::Preferences::default().text,
        )
    }
    #[test]
    fn clipboard_color_detection_runs_after_html_text_extraction() {
        let plain = text("#ffffff".into(), false, true).unwrap();
        for html in [
            "<span>#ffffff</span>",
            "<pre><code>#ffffff</code></pre>",
            "<p>&nbsp;#ffffff&nbsp;</p>",
        ] {
            let card = text(html.into(), true, true).unwrap();
            assert_eq!(card.source, plain.source);
            assert!(matches!(card.content, Content::Color([255, 255, 255, 255])));
        }
        let literal = text("<span>#ffffff</span>".into(), true, false).unwrap();
        assert_ne!(literal.source, plain.source);
        assert_ne!(
            text("<p>#ffffff and more</p>".into(), true, true)
                .unwrap()
                .source,
            plain.source
        );
    }
}
