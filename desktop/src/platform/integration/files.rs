//! Standard file-manager integration; action labels follow the system locale.
use gtk::{gio, prelude::*};

pub fn show_label() -> String {
    clivuu_core::i18n::translate_system("Show in Files")
}

pub async fn show(connection: &gio::DBusConnection, file: &gio::File) -> Result<(), String> {
    match connection
        .call_future(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1",
            "ShowItems",
            Some(&(vec![file.uri().to_string()], "").to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            5000,
        )
        .await
    {
        Ok(_) => Ok(()),
        Err(error)
            if error.matches(gio::DBusError::ServiceUnknown)
                || error.matches(gio::DBusError::NameHasNoOwner)
                || error.matches(gio::DBusError::UnknownMethod)
                || error.matches(gio::DBusError::UnknownInterface) =>
        {
            let directory = file.parent().unwrap_or_else(|| file.clone());
            gio::AppInfo::launch_default_for_uri_future(
                &directory.uri(),
                None::<&gio::AppLaunchContext>,
            )
            .await
            .map_err(|e| e.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}
