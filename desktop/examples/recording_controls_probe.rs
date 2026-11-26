//! Private native-Wayland fixture for recording surfaces and input, no encoder.
use clivuu_core::{
    i18n,
    recording::{Options, Phase},
};
use clivuu_desktop::{
    platform::surfaces::capture::gdk_outputs,
    ui::recording::{Appearance, Controls},
};
use gtk::{gdk, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::Duration,
};
fn toolbar_node() -> Option<serde_json::Value> {
    fn find(w: &gtk::Widget) -> Option<gtk::Widget> {
        if w.has_css_class("clivuu-capture-toolbar") {
            return Some(w.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            if let Some(found) = find(&c) {
                return Some(found);
            }
        }
        None
    }
    for window in gtk::Window::list_toplevels() {
        if let Some(panel) = find(&window).filter(|p| p.width() > 0 && p.is_visible()) {
            let snapshot = gtk::Snapshot::new();
            panel.parent().unwrap().snapshot_child(&panel, &snapshot);
            if let Some(node) = snapshot.to_node() {
                let r = node.bounds();
                return Some(
                    serde_json::json!({"x":r.x(),"y":r.y(),"width":r.width(),"height":r.height(),"widgetWidth":panel.width(),"widgetHeight":panel.height(),"allocatedWidth":panel.compute_bounds(&panel.parent().unwrap()).unwrap().width(),"allocatedHeight":panel.compute_bounds(&panel.parent().unwrap()).unwrap().height()}),
                );
            }
        }
    }
    None
}
fn main() {
    adw::init().unwrap();
    i18n::initialize(Some("en"));
    let directory = PathBuf::from(std::env::var("CLIVUU_TEST_DIR").unwrap());
    let app = gtk::Application::builder()
        .application_id("io.github.naerelyth.clivuu.recordingprobe")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app|{
        let hold=Rc::new(RefCell::new(Some(app.hold())));
        let display=gdk::Display::default().unwrap();
        let output=gdk_outputs(&display)[0].rect;
        let fixture=gtk::ApplicationWindow::builder().application(app).title("Clivuu recording native fixture").default_width(1000).default_height(650).build();
        let area=gtk::DrawingArea::new();area.set_draw_func(|_,c,_,_|{c.set_source_rgb(0.14,0.41,0.67);c.paint().unwrap();});fixture.set_child(Some(&area));fixture.present();
        let clicks=Rc::new(Cell::new(0));let clicks2=clicks.clone();let click=gtk::GestureClick::new();click.connect_pressed(move |_,_,_,_|clicks2.set(clicks2.get()+1));area.add_controller(click);
        let controls=Rc::new(RefCell::new(Some(Controls::new(&display,clivuu_core::annotation::Bounds{x:180.,y:150.,width:640.,height:320.},Appearance{dark:true,accent:"#bd93f9".into(),..Default::default()},Options::default(),Default::default(),||{},|_|{}).unwrap())));
        let app=app.clone();let directory=directory.clone();
        let maps=Rc::new(Cell::new(0));let unmaps=Rc::new(Cell::new(0));let mut tracked=std::collections::HashSet::new();
        let playback=Rc::new(RefCell::new(None::<clivuu_desktop::ui::playback::Controls>));
        glib::timeout_add_local(Duration::from_millis(50),move ||{
            let path=directory.join("native-record-command");
            if let Ok(command)=std::fs::read_to_string(&path){
                let _=std::fs::remove_file(&path);
                if command=="dispose"{controls.borrow_mut().take();playback.borrow_mut().take();}
                else if command=="playback" {
                    controls.borrow_mut().take();
                    let state=Rc::downgrade(&playback);
                    let info=clivuu_core::recording::MaterialInfo{width:640,height:320,duration_ns:5_000_000_000,tracks:vec![]};
                    let c=clivuu_desktop::ui::playback::Controls::new(&display,clivuu_core::annotation::Bounds{x:180.,y:150.,width:640.,height:320.},Appearance{dark:true,accent:"#bd93f9".into(),..Default::default()},info,move |cmd| {
                        if let Some(state)=state.upgrade()&&let Some(c)=state.borrow().as_ref()&&let clivuu_desktop::ui::playback::Command::Edit(edit)=cmd {c.edit(&edit);}
                    }).unwrap();
                    let pixels=vec![0x48u8;640*320*4];
                    c.texture(gdk::MemoryTexture::new(640,320,gdk::MemoryFormat::R8g8b8a8,&glib::Bytes::from_owned(pixels),640*4).upcast());
                    c.update(2_000_000_000,false,None);playback.replace(Some(c));
                }
                else if command=="exit"{hold.borrow_mut().take();app.quit();return glib::ControlFlow::Break;}
                else if matches!(command.as_str(),"full"|"side-left"|"side-right"){
                    let width=output.width/3.;
                    let rect=match command.as_str(){
                        "side-left"=>clivuu_core::annotation::Bounds{width,..output},
                        "side-right"=>clivuu_core::annotation::Bounds{x:output.x+output.width-width,width,..output},
                        _=>output,
                    };
                    controls.replace(Some(Controls::new(&display,rect,Appearance{dark:true,accent:"#bd93f9".into(),..Default::default()},Options::default(),Default::default(),||{},|_|{}).unwrap()));
                }
                else if command=="suspend" {if let Some(c)=playback.borrow().as_ref(){c.suspend(true);}}
                else if command=="restore" {if let Some(c)=playback.borrow().as_ref(){c.suspend(false);}}
                else if let Some(c)=controls.borrow().as_ref(){c.update(if command=="preparing"{Phase::Preparing}else if command=="starting"{Phase::Starting}else if command=="ready"{Phase::Ready}else if command=="paused"{Phase::Paused}else{Phase::Recording},Duration::from_secs(3));}
            }
            for window in gtk::Window::list_toplevels().into_iter().filter_map(|w|w.downcast::<gtk::Window>().ok()).filter(|w|w.title().as_deref()==Some("Clivuu recording")) {
                if tracked.insert(window.as_ptr() as usize) {
                    if window.is_mapped(){maps.set(maps.get()+1);}
                    let count=maps.clone();window.connect_map(move |_|count.set(count.get()+1));
                    let count=unmaps.clone();window.connect_unmap(move |_|count.set(count.get()+1));
                }
            }
            let state=serde_json::json!({"maps":maps.get(),"unmaps":unmaps.get(),"paint":toolbar_node(),"controls":controls.borrow().as_ref().map(|c|c.status()),"playback":playback.borrow().as_ref().map(|c|c.status()),"clicks":clicks.get(),"width":output.width,"height":output.height,"socket":std::env::var("NIRI_SOCKET").ok(),"backend":display.type_().name()});
            let scratch=directory.join("native-record-status.next");std::fs::write(&scratch,state.to_string()).unwrap();std::fs::rename(scratch,directory.join("native-record-status.json")).unwrap();glib::ControlFlow::Continue
        });
    });
    app.run_with_args(&["recording-controls-probe"]);
}
