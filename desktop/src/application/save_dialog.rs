//! Keep GTK's save UI native even when GNOME pins require an XWayland service.
use crate::{
    infrastructure::process,
    platform::native_ui,
    ui::save_dialog::{self, Format, Options, SavedFile},
};
use clivuu_core::{i18n::t, saving::Media};
use gtk::{gio, glib, prelude::*};
use std::{cell::RefCell, ffi::OsStr, rc::Rc};

#[derive(serde::Serialize, serde::Deserialize)]
struct Reply {
    uri: String,
    format: Format,
}

pub async fn choose(
    media: Media,
    name: &str,
    cancel: &gio::Cancellable,
) -> Result<Option<SavedFile>, String> {
    if cancel.is_cancelled() {
        return Err(t("Operation cancelled"));
    }
    let options = Options::new(media, name);
    let wayland =
        gtk::gdk::Display::default().is_some_and(|d| d.type_().name() == "GdkWaylandDisplay");
    if native_ui::backend() != "wayland" || wayland {
        return save_dialog::choose(&options, cancel).await;
    }
    let arguments = serde_json::to_string(&options).map_err(|e| e.to_string())?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let child = native_ui::launcher(gio::SubprocessFlags::STDOUT_PIPE)
        .spawn(&[
            executable.as_os_str(),
            OsStr::new("--save-dialog"),
            OsStr::new(&arguments),
        ])
        .map_err(|e| e.to_string())?;
    let output = process::read_output(child, cancel).await?;
    let reply: Result<Option<Reply>, String> =
        serde_json::from_str(&output).map_err(|e| e.to_string())?;
    Ok(reply?.map(|r| SavedFile {
        file: gio::File::for_uri(&r.uri),
        extension: r.format.extension(),
    }))
}

/// Internal process role. No service, image data, or configuration writer lives here.
pub fn run() -> glib::ExitCode {
    let options = match std::env::args()
        .nth(2)
        .and_then(|s| serde_json::from_str::<Options>(&s).ok())
    {
        Some(options) => Rc::new(options),
        None => return glib::ExitCode::FAILURE,
    };
    if let Err(e) = native_ui::initialize() {
        eprintln!("{e}");
        return glib::ExitCode::FAILURE;
    }
    let app = gtk::Application::builder()
        .application_id(crate::platform::integration::identity::APPLICATION_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let result = Rc::new(RefCell::new(None));
    let completed = result.clone();
    app.connect_activate(move |app| {
        let hold = app.hold();
        let app = app.clone();
        let options = options.clone();
        let completed = completed.clone();
        glib::MainContext::default().spawn_local(async move {
            let response = save_dialog::choose(&options, &gio::Cancellable::new()).await;
            completed.replace(Some(response.map(|file| {
                file.map(|f| Reply {
                    uri: f.file.uri().into(),
                    format: match f.extension {
                        "jpg" => Format::Jpeg,
                        "webm" => Format::Webm,
                        "mp4" => Format::Mp4,
                        "gif" => Format::Gif,
                        _ => Format::Png,
                    },
                })
            })));
            drop(hold);
            app.quit();
        });
    });
    let exit = app.run_with_args(&["clivuu-save-dialog"]);
    if let Some(response) = result.take() {
        println!(
            "{}",
            serde_json::to_string(&response).expect("save selection JSON")
        );
    }
    exit
}
