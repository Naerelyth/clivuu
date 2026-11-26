//! Real settings color picking with a private portal and compositor observer.
use adw::prelude::*;
use clivuu_desktop::{
    infrastructure::{configuration::Configuration, tasks::TaskPool},
    platform::portal,
    ui::{color::ColorServices, preferences::Preferences},
};
use gtk::{gio, glib};
use serde_json::json;
use std::{path::PathBuf, rc::Rc, time::Duration};

fn descendants(widget: &gtk::Widget, out: &mut Vec<gtk::Widget>) {
    out.push(widget.clone());
    let mut child = widget.first_child();
    while let Some(w) = child {
        child = w.next_sibling();
        descendants(&w, out);
    }
}

fn main() {
    adw::init().unwrap();
    let directory = PathBuf::from(std::env::var_os("CLIVUU_TEST_DIR").unwrap());
    let pool = Rc::new(TaskPool::new(1, 2).unwrap());
    let config = Configuration::new(clivuu_core::config::read().unwrap(), pool, |e| {
        panic!("{e}")
    })
    .unwrap();
    config.set("language", json!("en")).unwrap();
    let app = gtk::Application::builder()
        .application_id("io.github.naerelyth.clivuu")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let services = ColorServices {
        pick: Some(Rc::new(|cancel| {
            Box::pin(async move {
                let connection = gio::bus_get_future(gio::BusType::Session)
                    .await
                    .map_err(|e| e.to_string())?;
                let result = portal::request(
                    &connection,
                    "io.github.naerelyth.Clivuu.ColorTest",
                    "Screenshot",
                    "PickColor",
                    vec!["".to_variant()],
                    Default::default(),
                    &cancel,
                )
                .await?;
                let (r, g, b) = result["color"].get::<(f64, f64, f64)>().unwrap();
                Ok([r, g, b].map(|c| (c * 255.).round() as u8))
            })
        })),
        ..Default::default()
    };
    let prefs = Preferences::new(&app, config, services);
    prefs.present();
    let loop_ = glib::MainLoop::new(None, false);
    let done = loop_.clone();
    glib::timeout_add_local(Duration::from_millis(80), move || {
        if directory.join("color-probe-stop").exists() {
            prefs.window.destroy();
            done.quit();
            return glib::ControlFlow::Break;
        }
        let mut widgets = vec![];
        descendants(prefs.window.upcast_ref(), &mut widgets);
        let dialog = prefs.window.visible_dialog();
        let bounds = |w: &gtk::Widget, root: &gtk::Widget| {
            w.compute_bounds(root)
                .map(|r| json!({"x":r.x(),"y":r.y(),"width":r.width(),"height":r.height()}))
        };
        let accent = widgets
            .iter()
            .find(|w| w.widget_name() == "accent-color")
            .and_then(|w| bounds(w, prefs.window.upcast_ref()));
        let panel = dialog.as_ref().map(|w| {
            let mut children = vec![];
            descendants(w.upcast_ref(), &mut children);
            let named = |name: &str| {
                children
                    .iter()
                    .find(|w| w.widget_name() == name)
                    .and_then(|c| bounds(c, prefs.window.upcast_ref()))
            };
            let pick = children.iter().find(|w| {
                w.downcast_ref::<gtk::Button>()
                    .is_some_and(|b| b.label().as_deref() == Some("Pick screen color"))
            });
            json!({"pick":pick.and_then(|c|bounds(c,prefs.window.upcast_ref())),"cancel":named("cancel-color"),"accept":named("accept-color"),"html":children.iter().find(|w|w.widget_name()=="clivuu-color-html").and_then(|w|w.downcast_ref::<gtk::Entry>()).map(|e|e.text().to_string())})
        });
        let temp = directory.join("color-probe-state.tmp");
        std::fs::write(&temp, json!({"accent":accent,"dialog":panel}).to_string()).unwrap();
        std::fs::rename(temp, directory.join("color-probe-state.json")).unwrap();
        glib::ControlFlow::Continue
    });
    loop_.run();
}
