//! Isolated layer-shell integration fixture; frozen source is synthetic, not a
//! substitute for the separately tested real static Screenshot acquisition.
use clivuu_core::{
    capture::{Purpose, Selection},
    image::{Rgba, RgbaImage},
};
use clivuu_desktop::{
    application::capture, infrastructure::tasks::TaskPool, platform::surfaces::capture::gdk_outputs,
};
use gtk::{gdk, glib, prelude::*};
use serde_json::json;
use std::{path::PathBuf, rc::Rc, sync::Arc, time::Duration};
#[path = "support/scroll_surface.rs"]
mod scroll_surface;
fn main() {
    adw::init().unwrap();
    let directory = PathBuf::from(std::env::var("CLIVUU_TEST_DIR").unwrap());
    let loop_ = glib::MainLoop::new(None, false);
    let stop = loop_.clone();
    glib::MainContext::default().spawn_local(async move{
        while !directory.join("layer-parent-ready").exists(){glib::timeout_future(Duration::from_millis(50)).await;}
        let fixture=gtk::Window::builder().title("Clivuu native layer fixture").default_width(1000).default_height(700).build();fixture.set_child(Some(&gtk::Label::new(Some("Native layer-shell input fixture"))));fixture.present();fixture.maximize();
        let pointer=gtk::GestureClick::new();let clicks=Rc::new(std::cell::Cell::new(0));let count=clicks.clone();let path=directory.clone();pointer.connect_pressed(move|_,_,_,_|{count.set(count.get()+1);std::fs::write(path.join("layer-clicks"),count.get().to_string()).unwrap();});fixture.add_controller(pointer);
        glib::timeout_future(Duration::from_millis(250)).await;
        let mut results=vec![];
        for step in 1..=6 {
            let display=gdk::Display::default().unwrap();let outputs=gdk_outputs(&display);let limit=Selection::new(outputs.clone(),vec![],None).unwrap().limit;
            let (width,height)=if step==5{(3457,2103)}else{(limit.width as u32,limit.height as u32)};
            let image=Arc::new(RgbaImage::from_pixel(width,height,Rgba([36,104,172,255])));let pool=Rc::new(TaskPool::new(1,2).unwrap());
            let windows=vec![];
            let options=if step==5{
                let bounds=clivuu_desktop::platform::capture::window::presentation([width,height],outputs[0]);
                capture::Options{purpose:Purpose::Image,image_bounds:Some(bounds),..Default::default()}
            }else{Default::default()};
            let mut held=None;
            if step==3 {let (tx,rx)=std::sync::mpsc::channel();let (started,ready)=async_channel::bounded(1);let task=pool.submit(move|_|{let _=started.try_send(());let _=rx.recv();Ok(())}).unwrap();ready.recv().await.unwrap();held=Some((tx,task));}
            std::fs::write(directory.join("layer-phase.json"),json!({"step":step,"width":limit.width,"height":limit.height,"socket":std::env::var("NIRI_SOCKET").unwrap()}).to_string()).unwrap();
            let selected=capture::select(&display,image,outputs,windows,None,options,pool,&gtk::gio::Cancellable::new()).await;
            if let Some((gate,task))=held{let _=gate.send(());let _=task.result().await;}
            if step>=5 {
                let details=match &selected {Ok(Some(capture::Selected::Image(r)))=>json!({"rect":r.rect,"dimensions":r.image.dimensions(),"nonBlue":r.image.pixels.pixels().filter(|p|**p!=Rgba([36,104,172,255])).count()}),Ok(None)=>json!("cancelled"),Ok(_)=>json!("region"),Err(e)=>json!(e)};
                std::fs::write(directory.join(format!("layer-window-{step}.json")),details.to_string()).unwrap();
            }
            let passed=match step {
                1=>selected.ok().flatten().is_some_and(|r|{let capture::Selected::Image(r)=r else{return false};let pixels=r.image.pixels;pixels.dimensions()==(500,300)&&pixels.pixels().all(|p|*p==Rgba([36,104,172,255]))}),
                2=>matches!(selected,Ok(None)),
                3=>selected.is_err(),
                5=>selected.ok().flatten().is_some_and(|r|{let capture::Selected::Image(r)=r else{return false};r.image.dimensions()==[3457,2103]&&r.image.pixels.pixels().all(|p|*p==Rgba([36,104,172,255]))}),
                6=>selected.ok().flatten().is_some_and(|r|{let capture::Selected::Image(r)=r else{return false};let session=r.snapshot.editor.session();let marks=session.marks();marks.len()==2&&marks.iter().all(|m|m.kind==clivuu_core::annotation::Kind::Sequence)&&marks[0].style.sequence.index==2&&marks[1].style.sequence.index==3&&marks[1].sequence_has_leader(1.)&&session.style(clivuu_core::annotation::Kind::Sequence).style.sequence.index==4&&r.image.pixels.pixels().filter(|p|**p!=Rgba([36,104,172,255])).count()>500}),
                _=>selected.ok().flatten().is_some_and(|r|{let capture::Selected::Image(r)=r else{return false};let pixels=r.image.pixels;pixels.dimensions()==(690,300)&&*pixels.get_pixel(90,50)!=Rgba([36,104,172,255])&&*pixels.get_pixel(300,100)==Rgba([36,104,172,255])}),
            };
            results.push(json!({"name":match step{1=>"native layer capture selects and exports exact pixels",2=>"native layer Escape cancels before any click",3=>"output change cancels a queued export and tears down layers",5=>"niri isolated window presentation preserves large original pixels and source bounds",6=>"niri Sequence input shares independent Z/X numbering and exports vector badges with leaders",_=>"niri native capture retains outside ink after click expansion and exports original coordinates"},"passed":passed}));
            std::fs::write(directory.join("layer-results.json"),serde_json::to_vec(&results).unwrap()).unwrap();if !passed{break;}
            // Give the observer a completed interval for checking native focus
            // and the absence of layers before opening the next session.
            std::fs::write(directory.join("layer-complete"),step.to_string()).unwrap();
            while !directory.join(format!("layer-next-{step}")).exists(){glib::timeout_future(Duration::from_millis(50)).await;}
        }
        scroll_surface::run(&directory).await;
        fixture.destroy();stop.quit();
    });
    loop_.run();
}
