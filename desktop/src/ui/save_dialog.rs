//! One GTK chooser for all manual image and recording saves.
#![allow(deprecated)] // Live filename binding requires GtkFileChooser, isolated here.
use clivuu_core::{i18n::t, saving::Media};
use gtk::{gio, prelude::*};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Format {
    Png,
    Jpeg,
    Webm,
    Mp4,
    Gif,
}
impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Webm => "WebM",
            Self::Mp4 => "MP4",
            Self::Gif => "GIF",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webm => "webm",
            Self::Mp4 => "mp4",
            Self::Gif => "gif",
        }
    }
    pub fn suffixes(self) -> &'static [&'static str] {
        match self {
            Self::Png => &["png"],
            Self::Jpeg => &["jpg", "jpeg"],
            Self::Webm => &["webm"],
            Self::Mp4 => &["mp4"],
            Self::Gif => &["gif"],
        }
    }
    pub fn for_name(media: Media, name: &str) -> Self {
        formats(media)
            .iter()
            .copied()
            .find(|f| {
                Path::new(name)
                    .extension()
                    .is_some_and(|s| f.suffixes().iter().any(|v| s.eq_ignore_ascii_case(v)))
            })
            .unwrap_or(if media == Media::Image {
                Self::Png
            } else {
                Self::Mp4
            })
    }
    pub fn name(self, name: &str) -> String {
        if name.is_empty()
            || Path::new(name)
                .extension()
                .is_some_and(|s| self.suffixes().iter().any(|v| s.eq_ignore_ascii_case(v)))
        {
            name.into()
        } else {
            Path::new(name)
                .with_extension(self.extension())
                .to_string_lossy()
                .into()
        }
    }
}
pub fn formats(media: Media) -> &'static [Format] {
    if media == Media::Image {
        &[Format::Png, Format::Jpeg]
    } else {
        &[Format::Webm, Format::Mp4, Format::Gif]
    }
}
pub struct SavedFile {
    pub file: gio::File,
    pub extension: &'static str,
}

/// Snapshot translated labels before crossing into an optional native UI process.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Options {
    video: bool,
    name: String,
    title: String,
    cancel: String,
    save: String,
    invalid_name: String,
}
impl Options {
    pub fn new(media: Media, name: &str) -> Self {
        Self {
            video: media == Media::Video,
            name: name.into(),
            title: t(if media == Media::Image {
                "Save screenshot"
            } else {
                "Save recording"
            }),
            cancel: t("Cancel"),
            save: t("Save"),
            invalid_name: t(if media == Media::Image {
                "Choose a PNG or JPEG file name"
            } else {
                "Choose a WebM, MP4 or GIF file name"
            }),
        }
    }
}

struct Dialog(gtk::FileChooserDialog);
impl Drop for Dialog {
    fn drop(&mut self) {
        self.0.destroy();
    }
}

#[allow(deprecated)]
pub async fn choose(
    options: &Options,
    cancel: &gio::Cancellable,
) -> Result<Option<SavedFile>, String> {
    if cancel.is_cancelled() {
        return Err(t("Operation cancelled"));
    }
    let media = if options.video {
        Media::Video
    } else {
        Media::Image
    };
    let owned = Dialog(
        gtk::FileChooserDialog::builder()
            .title(&options.title)
            .action(gtk::FileChooserAction::Save)
            .modal(true)
            .use_header_bar(1)
            .build(),
    );
    let dialog = &owned.0;
    if let Some(app) = gio::Application::default().and_downcast::<gtk::Application>() {
        dialog.set_transient_for(app.active_window().as_ref());
        dialog.set_application(Some(&app));
    }
    crate::platform::integration::identity::configure(dialog);
    dialog.add_buttons(&[
        (&options.cancel, gtk::ResponseType::Cancel),
        (&options.save, gtk::ResponseType::Accept),
    ]);
    dialog.set_default_response(gtk::ResponseType::Accept);
    let selected = Format::for_name(media, &options.name);
    for &format in formats(media) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(format.label()));
        for suffix in format.suffixes() {
            filter.add_suffix(suffix);
        }
        dialog.add_filter(&filter);
        if format == selected {
            dialog.set_filter(&filter);
        }
    }
    dialog.set_current_name(&options.name);
    dialog.connect_filter_notify(move |dialog| {
        if let Some(name) = dialog.current_name() {
            let format = selected_format(dialog, media);
            let next = format.name(&name);
            if next != name {
                dialog.set_current_name(&next);
            }
        }
    });
    let error = gtk::Label::builder().wrap(true).visible(false).build();
    error.add_css_class("error");
    dialog.content_area().append(&error);
    loop {
        let response = gio::CancellableFuture::new(dialog.run_future(), cancel.clone())
            .await
            .map_err(|_| t("Operation cancelled"))?;
        if response != gtk::ResponseType::Accept {
            return Ok(None);
        }
        let Some(file) = dialog.file() else { continue };
        let name = file
            .basename()
            .ok_or_else(|| t("Invalid desktop response"))?;
        // Preserve the exact approved target. Never change a name after GTK's
        // overwrite confirmation, including explicitly typed .jpeg or no suffix.
        if name.extension().is_some() && clivuu_core::saving::extension(&name, media).is_err() {
            error.set_label(&options.invalid_name);
            error.set_visible(true);
            continue;
        }
        let format = if name.extension().is_some() {
            Format::for_name(media, &name.to_string_lossy())
        } else {
            selected_format(dialog, media)
        };
        return Ok(Some(SavedFile {
            file,
            extension: format.extension(),
        }));
    }
}

#[allow(deprecated)]
fn selected_format(dialog: &gtk::FileChooserDialog, media: Media) -> Format {
    let label = dialog.filter().and_then(|f| f.name());
    formats(media)
        .iter()
        .copied()
        .find(|f| Some(f.label()) == label.as_deref())
        .unwrap_or(formats(media)[0])
}
