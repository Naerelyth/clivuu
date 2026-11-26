//! Independent native drag source tests real incoming PNG and file-URI offers.
use gtk::{gdk, gio, glib, prelude::*};
fn main() {
    gtk::init().unwrap();
    let directory = std::path::PathBuf::from(std::env::var_os("CLIVUU_TEST_DIR").unwrap());
    let app = gtk::Application::builder()
        .application_id("io.github.naerelyth.Clivuu.DropProbe")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Clivuu drag source fixture")
            .default_width(300)
            .default_height(200)
            .build();
        let button = gtk::Button::with_label("Drag image");
        let source = gtk::DragSource::builder()
            .actions(gdk::DragAction::COPY)
            .build();
        let directory = directory.clone();
        source.connect_prepare(move |_, _, _| {
            let mime = std::fs::read_to_string(directory.join("pin-drop-mime")).unwrap();
            let bytes = if mime == "text/uri-list" {
                format!(
                    "{}\r\n",
                    gio::File::for_path(directory.join("pin-source.png")).uri()
                )
                .into_bytes()
            } else {
                std::fs::read(directory.join("pin-source.png")).unwrap()
            };
            Some(gdk::ContentProvider::for_bytes(
                &mime,
                &glib::Bytes::from_owned(bytes),
            ))
        });
        button.add_controller(source);
        window.set_child(Some(&button));
        window.present();
    });
    app.run_with_args(&["clivuu-drop-probe"]);
}
