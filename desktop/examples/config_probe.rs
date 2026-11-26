//! Isolated integration checks: real GTK preferences and live XDG file writes.
use adw::prelude::*;
use clivuu_desktop::{
    infrastructure::{configuration::Configuration, tasks::TaskPool},
    ui::preferences::Preferences,
};
use gtk::{gio, glib};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
async fn wait(mut predicate: impl FnMut() -> bool) {
    for _ in 0..200 {
        if predicate() {
            return;
        }
        glib::timeout_future(std::time::Duration::from_millis(20)).await;
    }
    panic!("condition timed out");
}
fn main() {
    adw::init().unwrap();
    let context = glib::MainContext::default();
    context.block_on(async{
  let directory=std::path::PathBuf::from(format!("/tmp/clivuu-config-test-{}",glib::uuid_string_random()));std::fs::create_dir(&directory).unwrap();
  // The subprocess already has a private XDG environment from the GNOME harness.
  let path=clivuu_core::config::path().unwrap();std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  let pool=Rc::new(TaskPool::new(1,1).unwrap());let errors=Rc::new(RefCell::new(vec![]));let e=errors.clone();
  let settings=Configuration::new(clivuu_core::config::read().unwrap(),pool.clone(),move|s|e.borrow_mut().push(s)).unwrap();
  let(started,ready)=std::sync::mpsc::channel();let(gate,blocked)=std::sync::mpsc::channel();
  let first=pool.submit(move|_|{started.send(()).unwrap();blocked.recv().unwrap();Ok(())}).unwrap();ready.recv().unwrap();let second=pool.submit(|_|Ok(())).unwrap();
  let ticks=Rc::new(Cell::new(0));let counter=ticks.clone();let timer=glib::timeout_add_local(std::time::Duration::from_millis(25),move||{counter.set(counter.get()+1);glib::ControlFlow::Continue});
  settings.set("capture-mask-color",json!("#00000025")).unwrap();glib::timeout_future(std::time::Duration::from_millis(240)).await;assert!(ticks.get()>5);gate.send(()).unwrap();first.result().await.unwrap();second.result().await.unwrap();
  wait(||clivuu_core::config::read().unwrap()["capture-mask-color"]==json!("#00000025")).await;
  assert!(errors.borrow().is_empty());
  let app=gtk::Application::builder().application_id("io.github.naerelyth.Clivuu.ConfigurationProbe").flags(gio::ApplicationFlags::NON_UNIQUE).build();app.register(gio::Cancellable::NONE).unwrap();
  let preferences=Preferences::new(&app,settings.clone(),Default::default());preferences.present();
  settings.set("language",json!("zh-CN")).unwrap();assert_eq!(preferences.window.title().unwrap(),clivuu_core::i18n::t("Clivuu Preferences"));assert_ne!(preferences.window.title().unwrap(),"Clivuu Preferences");
  settings.set("language",json!("en")).unwrap();assert_eq!(preferences.window.title().unwrap(),"Clivuu Preferences");
  for value in 20..80{settings.set("capture-mask-color",json!(format!("#000000{value:02x}"))).unwrap();}assert_eq!(settings.value("capture-mask-color"),json!("#0000004f"));settings.flush().await.unwrap();assert_eq!(clivuu_core::config::read().unwrap()["capture-mask-color"],json!("#0000004f"));
  settings.reset(&["capture-mask-color".into(),"language".into()]);settings.flush().await.unwrap();let file=std::fs::read_to_string(&path).unwrap();assert!(!file.contains("capture-mask-color ="));assert!(!file.contains("language ="));
  let worker_path=path.clone();let thread=std::thread::spawn(move||{for value in 20..40 {clivuu_core::config::set_file(&worker_path,"capture-mask-color",json!(format!("#000000{value:02x}"))).unwrap();std::thread::sleep(std::time::Duration::from_millis(2));}});
  wait(||settings.value("capture-mask-color")==json!("#00000027")).await;thread.join().unwrap();glib::timeout_future(std::time::Duration::from_millis(100)).await;assert_eq!(settings.value("capture-mask-color"),json!("#00000027"));
  let snap=settings.current().await.unwrap();assert_eq!(snap["capture-mask-color"],json!("#00000027"));
  let values:Value=json!({"responsive_busy_retry":true,"live_language":true,"coalesced_final_value":true,"reset_removes_override":true,"external_cli_latest":true,"errors":errors.borrow().clone()});
  std::fs::write(std::env::var("CLIVUU_CONFIG_PROBE_RESULT").unwrap(),values.to_string()).unwrap();
  preferences.window.destroy();timer.remove();drop(preferences);drop(settings);drop(pool);std::fs::remove_dir(directory).unwrap();
 });
}
