//! Inspect the GTK save chooser's live format/name binding.
#![allow(deprecated)]
use adw::prelude::*;
use clivuu_core::saving::Media;
use clivuu_desktop::ui::save_dialog;
use gtk::{gio, glib};
use std::time::Duration;
async fn settle() {
    glib::timeout_future(Duration::from_millis(200)).await;
}
fn dialog() -> gtk::FileChooserDialog {
    gtk::Window::list_toplevels()
        .into_iter()
        .find_map(|w| w.downcast::<gtk::FileChooserDialog>().ok())
        .unwrap()
}
async fn probe() {
    let app = gtk::Application::builder()
        .application_id("io.github.naerelyth.Clivuu.SaveProbe")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let directory = std::path::PathBuf::from(std::env::var_os("CLIVUU_TEST_DIR").unwrap());
    let completed = std::rc::Rc::new(std::cell::Cell::new(false));
    let done = completed.clone();
    let task = glib::MainContext::default().spawn_local(async move {
        let result = save_dialog::choose(
            &save_dialog::Options::new(Media::Image, "sample.png"),
            &gio::Cancellable::new(),
        )
        .await
        .unwrap();
        done.set(true);
        result
    });
    settle().await;
    let chooser = dialog();
    let filters = chooser.filters();
    let png = filters
        .item(0)
        .unwrap()
        .downcast::<gtk::FileFilter>()
        .unwrap();
    let jpeg = filters
        .item(1)
        .unwrap()
        .downcast::<gtk::FileFilter>()
        .unwrap();
    assert_eq!(png.name().as_deref(), Some("PNG"));
    assert_eq!(jpeg.name().as_deref(), Some("JPEG"));
    chooser.set_filter(&jpeg);
    assert_eq!(chooser.current_name().as_deref(), Some("sample.jpg"));
    for _ in 0..20 {
        chooser.set_filter(&png);
        assert_eq!(chooser.current_name().as_deref(), Some("sample.png"));
        chooser.set_filter(&jpeg);
        assert_eq!(chooser.current_name().as_deref(), Some("sample.jpg"));
        assert_eq!(
            dialog(),
            chooser,
            "Format changes must keep the same window"
        );
    }
    chooser.set_current_name("typed.jpeg");
    chooser.set_filter(&jpeg);
    assert_eq!(chooser.current_name().as_deref(), Some("typed.jpeg"));
    chooser.set_filter(&png);
    assert_eq!(chooser.current_name().as_deref(), Some("typed.png"));
    chooser
        .set_current_folder(Some(&gio::File::for_path(&directory)))
        .unwrap();
    settle().await;
    chooser.set_filter(&jpeg);
    chooser.set_current_name("unsupported.txt");
    settle().await;
    chooser
        .widget_for_response(gtk::ResponseType::Accept)
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    settle().await;
    assert!(
        !completed.get(),
        "Unsupported suffix must keep the chooser open"
    );
    assert_eq!(dialog(), chooser);
    chooser.set_current_name("chosen.jpeg");
    settle().await;
    chooser
        .widget_for_response(gtk::ResponseType::Accept)
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    let selected = task.await.unwrap().unwrap();
    assert_eq!(
        selected.file.basename().unwrap(),
        std::path::PathBuf::from("chosen.jpeg")
    );
    assert_eq!(selected.extension, "jpg");
    // Exercise GTK's real accept handler, including overwrite consent. Calling
    // response(Accept) directly would bypass this validation.
    let existing = directory.join("overwrite.jpg");
    std::fs::write(&existing, b"keep original").unwrap();
    let settled = std::rc::Rc::new(std::cell::Cell::new(false));
    let finished = settled.clone();
    let overwrite_task = glib::MainContext::default().spawn_local(async move {
        let result = save_dialog::choose(
            &save_dialog::Options::new(Media::Image, "overwrite.png"),
            &gio::Cancellable::new(),
        )
        .await
        .unwrap();
        finished.set(true);
        result
    });
    settle().await;
    let chooser = dialog();
    chooser
        .set_current_folder(Some(&gio::File::for_path(&directory)))
        .unwrap();
    settle().await;
    let jpeg = chooser
        .filters()
        .item(1)
        .unwrap()
        .downcast::<gtk::FileFilter>()
        .unwrap();
    chooser.set_filter(&jpeg);
    assert_eq!(chooser.current_name().as_deref(), Some("overwrite.jpg"));
    settle().await;
    chooser
        .widget_for_response(gtk::ResponseType::Accept)
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    settle().await;
    assert!(
        !settled.get(),
        "Existing file was approved without confirmation"
    );
    let prompt = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
        .find(|w| w.is_visible() && w.transient_for().as_ref() == Some(chooser.upcast_ref()))
        .expect("Overwrite confirmation must be visible");
    prompt.close();
    settle().await;
    assert!(!settled.get());
    assert_eq!(std::fs::read(&existing).unwrap(), b"keep original");
    chooser.response(gtk::ResponseType::Cancel);
    assert!(overwrite_task.await.unwrap().is_none());
    let task = glib::MainContext::default().spawn_local(async {
        save_dialog::choose(
            &save_dialog::Options::new(Media::Video, "record.webm"),
            &gio::Cancellable::new(),
        )
        .await
        .unwrap()
    });
    settle().await;
    let chooser = dialog();
    assert_eq!(chooser.filter().unwrap().name().as_deref(), Some("WebM"));
    chooser.response(gtk::ResponseType::Cancel);
    assert!(task.await.unwrap().is_none());
    let task = glib::MainContext::default().spawn_local(async {
        save_dialog::choose(
            &save_dialog::Options::new(Media::Image, "plain.png"),
            &gio::Cancellable::new(),
        )
        .await
        .unwrap()
    });
    settle().await;
    let chooser = dialog();
    chooser
        .set_current_folder(Some(&gio::File::for_path(&directory)))
        .unwrap();
    settle().await;
    let jpeg = chooser
        .filters()
        .item(1)
        .unwrap()
        .downcast::<gtk::FileFilter>()
        .unwrap();
    chooser.set_filter(&jpeg);
    chooser.set_current_name("no-suffix");
    settle().await;
    chooser
        .widget_for_response(gtk::ResponseType::Accept)
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    let selected = task.await.unwrap().unwrap();
    assert_eq!(
        selected.file.basename().unwrap(),
        std::path::PathBuf::from("no-suffix")
    );
    assert_eq!(selected.extension, "jpg");
    let cancel = gio::Cancellable::new();
    let token = cancel.clone();
    let task = glib::MainContext::default().spawn_local(async move {
        save_dialog::choose(
            &save_dialog::Options::new(Media::Image, "cancel.png"),
            &token,
        )
        .await
    });
    settle().await;
    cancel.cancel();
    assert!(task.await.unwrap().is_err());
    settle().await;
    assert!(
        !gtk::Window::list_toplevels()
            .iter()
            .any(|w| w.is::<gtk::FileChooserDialog>())
    );
    std::fs::write(directory.join("save-dialog.json"),serde_json::json!({"live_extension":true,"repeated_switch_same_window":true,"typed_jpeg":true,"invalid_suffix_same_window":true,"approved_extensionless_name":true,"format_labels":true,"video_label":true,"cancellation":true,"overwrite_confirmation":true}).to_string()).unwrap();
}
fn main() {
    glib::set_prgname(Some("clivuu-save-probe"));
    glib::set_application_name("Clivuu");
    adw::init().unwrap();
    glib::MainContext::default().block_on(probe());
}
