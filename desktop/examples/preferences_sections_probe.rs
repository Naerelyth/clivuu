//! Observe actual allocations when switching short and long preference sections.
use adw::prelude::*;
use clivuu_desktop::{
    infrastructure::{configuration::Configuration, tasks::TaskPool},
    ui::preferences::Preferences,
};
use gtk::{gio, glib};
use std::{cell::RefCell, rc::Rc, time::Duration};
fn widgets(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut all = vec![root.clone()];
    let mut child = root.first_child();
    while let Some(w) = child {
        child = w.next_sibling();
        all.extend(widgets(&w));
    }
    all
}
async fn probe() {
    let pool = Rc::new(TaskPool::new(1, 2).unwrap());
    let config = Configuration::new(clivuu_core::config::read().unwrap(), pool, |e| {
        panic!("{e}")
    })
    .unwrap();
    let app = gtk::Application::builder()
        .application_id("io.github.naerelyth.Clivuu.SectionsProbe")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let prefs = Preferences::new(&app, config, Default::default());
    prefs.present();
    glib::timeout_future(Duration::from_millis(300)).await;
    let all = widgets(prefs.window.upcast_ref());
    let pages = all
        .iter()
        .find(|w| w.widget_name() == "preferences-pages")
        .unwrap()
        .clone()
        .downcast::<gtk::Stack>()
        .unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    for (page, sections) in [
        (
            "capture",
            &["behavior", "display", "behavior", "display"][..],
        ),
        ("control", &["mouse", "keys", "mouse", "keys"][..]),
        ("save", &["recording", "images", "recording", "images"][..]),
    ] {
        pages.set_visible_child_name(page);
        let name = format!("{page}-sections");
        let stack = all
            .iter()
            .find(|w| w.widget_name() == name)
            .unwrap()
            .clone()
            .downcast::<gtk::Stack>()
            .unwrap();
        glib::timeout_future(Duration::from_millis(300)).await;
        let trace = log.clone();
        let s = stack.clone();
        let tick = prefs.window.add_tick_callback(move |_, clock| {
            trace.borrow_mut().push(serde_json::json!({
                "time":clock.frame_time(), "page":page,
                "section":s.visible_child_name().map(|n|n.to_string()),
                "height":s.height(), "transition":s.is_transition_running()
            }));
            glib::ControlFlow::Continue
        });
        for name in sections {
            stack.set_visible_child_name(name);
            glib::timeout_future(Duration::from_millis(450)).await;
        }
        tick.remove();
    }
    let directory = std::path::PathBuf::from(std::env::var_os("CLIVUU_TEST_DIR").unwrap());
    std::fs::write(
        directory.join("section-frames.json"),
        serde_json::to_string_pretty(&*log.borrow()).unwrap(),
    )
    .unwrap();
    prefs.window.close();
}
fn main() {
    adw::init().unwrap();
    glib::MainContext::default().block_on(probe());
}
