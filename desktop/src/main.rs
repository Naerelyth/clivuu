//! Native service entry point. GTK objects stay on the owning main thread.
use gtk::prelude::*;
fn main() -> gtk::glib::ExitCode {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--ocr-worker")) {
        return match clivuu_core::ocr::worker::serve_stdio() {
            Ok(()) => gtk::glib::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                gtk::glib::ExitCode::FAILURE
            }
        };
    }
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--preferences")) {
        return clivuu_desktop::application::preferences::run();
    }
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--group-manager")) {
        return clivuu_desktop::application::group_manager::run();
    }
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--save-dialog")) {
        return clivuu_desktop::application::save_dialog::run();
    }
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--confirm-pins")) {
        return clivuu_desktop::application::confirmation::run();
    }
    if let Err(error) = clivuu_desktop::platform::display::bootstrap() {
        eprintln!("{error}");
        return gtk::glib::ExitCode::FAILURE;
    }
    gtk::glib::set_application_name("Clivuu");
    adw::init().expect("GTK initialization");
    let application = gtk::Application::builder()
        .application_id(clivuu_core::protocol::DESKTOP_DESTINATION)
        .build();
    let service = std::rc::Rc::new(std::cell::RefCell::new(None));
    let state = service.clone();
    application.connect_startup(
        move |app| match clivuu_desktop::application::service::Service::new(app) {
            Ok(service) => {
                state.replace(Some(service));
            }
            Err(error) => {
                eprintln!("{error}");
                app.quit();
            }
        },
    );
    let state = service.clone();
    let relaunch = std::rc::Rc::new(std::cell::RefCell::new(None));
    let next = relaunch.clone();
    application.connect_shutdown(move |_| {
        if let Some(service) = state.take() {
            next.replace(service.take_relaunch());
        }
    });
    application.connect_activate(|_| {});
    let result = application.run();
    // Release the old application and its D-Bus ownership before a launcher
    // re-enters the single-instance client. No racing detached helper process.
    drop(application);
    if let Some(launch) = relaunch.take() {
        eprintln!("Clivuu: {}", launch.exec());
        return gtk::glib::ExitCode::FAILURE;
    }
    result
}
