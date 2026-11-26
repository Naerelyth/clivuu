//! Bulk pin actions use a regular native window, independently of pin surfaces.
use crate::{infrastructure::process, platform::native_ui};
use adw::prelude::*;
use clivuu_core::i18n::t;
use gtk::{gio, glib};
use std::{cell::Cell, ffi::OsStr, rc::Rc};

pub async fn pins(destroy: bool, count: usize, cancel: &gio::Cancellable) -> Result<bool, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let count = count.to_string();
    let child = native_ui::launcher(gio::SubprocessFlags::STDOUT_PIPE)
        .spawn(&[
            executable.as_os_str(),
            OsStr::new("--confirm-pins"),
            OsStr::new(if destroy { "destroy" } else { "close" }),
            OsStr::new(&count),
        ])
        .map_err(|e| e.to_string())?;
    let output = process::read_output(child, cancel).await?;
    serde_json::from_str(&output).map_err(|e| e.to_string())
}
pub fn run() -> glib::ExitCode {
    if native_ui::initialize().is_err() {
        return glib::ExitCode::FAILURE;
    }
    if let Ok(values) = clivuu_core::config::read() {
        clivuu_core::i18n::initialize(values["language"].as_str());
    }
    let destroy = std::env::args().nth(2).as_deref() == Some("destroy");
    let count = std::env::args()
        .nth(3)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    if count <= 1 {
        return glib::ExitCode::FAILURE;
    }
    let app = gtk::Application::builder()
        .application_id(crate::platform::integration::identity::APPLICATION_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let result = Rc::new(Cell::new(false));
    let r = result.clone();
    app.connect_activate(move |app| {
        let window = gtk::Window::builder()
            .application(app)
            .title("Clivuu")
            .build();
        crate::platform::integration::identity::configure(&window);
        let dialog = adw::MessageDialog::builder()
            .transient_for(&window)
            .heading(t(if destroy {
                "Destroy these pins?"
            } else {
                "Close these pins?"
            }))
            .body(clivuu_core::i18n::tf(
                if destroy {
                    "Permanently destroy {0} pins. This cannot be undone."
                } else {
                    "Close {0} pins. Only the configured number of closed pins can be recovered."
                },
                &[&count.to_string()],
            ))
            .build();
        dialog.set_title(Some(&t(if destroy {
            "Destroy these pins?"
        } else {
            "Close these pins?"
        })));
        dialog.add_response("cancel", &t("Cancel"));
        dialog.add_response("confirm", &t(if destroy { "Destroy" } else { "Close" }));
        dialog.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let r = r.clone();
        let app = app.clone();
        dialog.connect_response(None, move |_, response| {
            r.set(response == "confirm");
            window.destroy();
            app.quit();
        });
        dialog.present();
    });
    let exit = app.run_with_args(&["clivuu-confirm-pins"]);
    println!("{}", result.get());
    exit
}
