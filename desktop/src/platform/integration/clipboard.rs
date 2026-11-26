//! Clipboard acquisition is independent of pin surfaces and compositor names.
//! Native Wayland uses ext/wlr data-control; GDK handles X11 and focused clients.
use crate::infrastructure::tasks::TaskPool;
use clivuu_core::i18n::t;
use gtk::{gdk, gio, glib, prelude::*};
use std::{os::fd::OwnedFd, rc::Rc};
use wl_clipboard_rs::paste::{self, ClipboardType, MimeType, Seat};
pub const ORIGIN: &str = "application/x-clivuu-pin";
#[derive(Clone)]
enum Access {
    Gdk,
    Native,
    Drop(gdk::Drop),
}
pub struct Offer {
    pub formats: Vec<String>,
    access: Access,
    clipboard: gdk::Clipboard,
    pool: Rc<TaskPool>,
}
impl Offer {
    pub fn from_drop(drop: &gdk::Drop, pool: Rc<TaskPool>) -> Self {
        Self {
            formats: drop
                .formats()
                .mime_types()
                .iter()
                .map(ToString::to_string)
                .collect(),
            access: Access::Drop(drop.clone()),
            clipboard: drop.display().clipboard(),
            pool,
        }
    }

    pub async fn new(
        display: &gdk::Display,
        pool: Rc<TaskPool>,
        cancel: &gio::Cancellable,
    ) -> Result<Self, String> {
        let clipboard = display.clipboard();
        if display.type_().name() == "GdkWaylandDisplay" {
            let task = pool
                .submit(|token| {
                    token.check()?;
                    Ok(paste::get_mime_types(
                        ClipboardType::Regular,
                        Seat::Unspecified,
                    ))
                })
                .map_err(|e| e.to_string())?;
            let result = gio::CancellableFuture::new(task.result(), cancel.clone())
                .await
                .map_err(|_| t("Operation cancelled"))?
                .map_err(|e| e.to_string())?;
            match result {
                Ok(formats) => {
                    return Ok(Self {
                        formats: formats.into_iter().collect(),
                        access: Access::Native,
                        clipboard,
                        pool,
                    });
                }
                Err(
                    paste::Error::ClipboardEmpty | paste::Error::NoMimeType | paste::Error::NoSeats,
                ) => {
                    return Ok(Self {
                        formats: vec![],
                        access: Access::Native,
                        clipboard,
                        pool,
                    });
                }
                Err(paste::Error::MissingProtocol { .. }) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        let formats = clipboard
            .formats()
            .mime_types()
            .iter()
            .map(|s| s.to_string())
            .collect();
        Ok(Self {
            formats,
            access: Access::Gdk,
            clipboard,
            pool,
        })
    }
    pub fn candidates(&self) -> Vec<String> {
        let mut list = Vec::new();
        for preferred in [
            ORIGIN,
            "x-special/gnome-copied-files",
            "text/uri-list",
            "image/png",
            "image/jpeg",
            "image/webp",
            "image/bmp",
            "image/tiff",
            "image/gif",
            "image/svg+xml",
            "text/html",
            "application/x-color",
            "text/plain;charset=utf-8",
            "text/plain",
            "UTF8_STRING",
        ] {
            if let Some(s) = self
                .formats
                .iter()
                .find(|s| s.eq_ignore_ascii_case(preferred))
            {
                list.push(s.clone());
            }
        }
        // Qualified text offers retain the same category priority.
        for base in ["text/html;", "text/plain;"] {
            for s in &self.formats {
                if s.to_ascii_lowercase().starts_with(base) && !list.contains(s) {
                    let before = list
                        .iter()
                        .position(|v| {
                            if base == "text/html;" {
                                v.to_ascii_lowercase().starts_with("application/x-color")
                                    || v.to_ascii_lowercase().starts_with("text/plain")
                                    || v == "UTF8_STRING"
                            } else {
                                v == "UTF8_STRING"
                            }
                        })
                        .unwrap_or(list.len());
                    list.insert(before, s.clone());
                }
            }
        }
        list
    }
    pub async fn read(&self, mime: &str, cancel: &gio::Cancellable) -> Result<Vec<u8>, String> {
        let stream: gio::InputStream = match &self.access {
            Access::Native => {
                let mime = mime.to_owned();
                let task = self
                    .pool
                    .submit(move |token| {
                        token.check()?;
                        paste::get_contents(
                            ClipboardType::Regular,
                            Seat::Unspecified,
                            MimeType::Specific(&mime),
                        )
                        .map(|(pipe, _)| OwnedFd::from(pipe))
                        .map_err(|e| crate::infrastructure::tasks::TaskError::Failed(e.to_string()))
                    })
                    .map_err(|e| e.to_string())?;
                let fd = gio::CancellableFuture::new(task.result(), cancel.clone())
                    .await
                    .map_err(|_| t("Operation cancelled"))?
                    .map_err(|e| e.to_string())?;
                gio::UnixInputStream::take_fd(fd).upcast()
            }
            Access::Drop(drop) => {
                gio::CancellableFuture::new(
                    drop.read_future(&[mime], glib::Priority::DEFAULT),
                    cancel.clone(),
                )
                .await
                .map_err(|_| t("Operation cancelled"))?
                .map_err(|e| e.to_string())?
                .0
            }
            Access::Gdk => {
                gio::CancellableFuture::new(
                    self.clipboard.read_future(&[mime], glib::Priority::DEFAULT),
                    cancel.clone(),
                )
                .await
                .map_err(|_| t("Operation cancelled"))?
                .map_err(|e| e.to_string())?
                .0
            }
        };
        let mut bytes = Vec::new();
        loop {
            let block = gio::CancellableFuture::new(
                stream.read_bytes_future(65536, glib::Priority::DEFAULT),
                cancel.clone(),
            )
            .await
            .map_err(|_| t("Operation cancelled"))?
            .map_err(|e| e.to_string())?;
            if block.is_empty() {
                break;
            }
            if bytes.len() + block.len() > clivuu_core::image_ops::MAX_BYTES as usize {
                return Err(t("The request is too large"));
            }
            bytes.extend_from_slice(&block);
        }
        Ok(bytes)
    }
}

pub fn set_image(display: &gdk::Display, png: Vec<u8>, origin: Option<&str>) -> Result<(), String> {
    let image = gdk::ContentProvider::for_bytes("image/png", &glib::Bytes::from_owned(png));
    let provider = if let Some(origin) = origin {
        gdk::ContentProvider::new_union(&[
            image,
            gdk::ContentProvider::for_bytes(
                ORIGIN,
                &glib::Bytes::from_owned(origin.as_bytes().to_vec()),
            ),
        ])
    } else {
        image
    };
    display
        .clipboard()
        .set_content(Some(&provider))
        .map_err(|e| e.to_string())
}
