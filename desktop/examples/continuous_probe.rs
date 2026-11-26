//! Private-desktop integration probe, never installed in an application bundle.
use clivuu_core::annotation::Bounds;
use clivuu_desktop::{
    application::recording::{Recording, VideoOutput},
    infrastructure::tasks::TaskPool,
    platform::capture::{frames::Frames, session::Session},
};
use gst::prelude::*;
use gtk::{gio, glib};
use std::{path::PathBuf, rc::Rc, time::Duration};
fn main() {
    let path = PathBuf::from(std::env::args().nth(1).expect("private output directory"));
    let context = glib::MainContext::default();
    let result=context.block_on(async {
        let connection=gio::bus_get_future(gio::BusType::Session).await.map_err(|e|e.to_string())?;
        let cancel=gio::Cancellable::new();let pool=Rc::new(TaskPool::new(2,4).map_err(|e|e.to_string())?);
        let rect=Bounds{x:220.,y:180.,width:640.,height:320.};
        let session=Session::mutter(&connection,&cancel,rect,false).await?;
        let frames=Frames::open(&session,&session.streams[0],&pool).await?;
        let first=frames.frame(&pool,&cancel).await?;
        let next=frames.frame(&pool,&cancel).await?;
        let dimensions=first.dimensions();let pixel=first.get_pixel(50,50).0;
        drop(frames);session.close().await;
        let recording=Recording::new(pool.clone(),|_,_,_|{});
        let session=Session::mutter(&connection,&cancel,rect,false).await?;
        let destination=path.join("native-video.webm");
        recording.start(session,rect,VideoOutput{destination:destination.clone(),unique:false,replace:false,notify:false,format:clivuu_core::recording::Format::Webm},true,16).await?;
        glib::timeout_future(Duration::from_millis(1400)).await;
        let stopped=recording.stop().await?;
        let completed=recording.stop().await?;
        let size=std::fs::metadata(&destination).map_err(|e|e.to_string())?.len();
        // Decode a real frame from the completed mux, not merely a nonempty file.
        let decode=pool.submit(move |_|{
            gst::init().unwrap();
            let pipeline=gst::parse::launch("filesrc name=file ! matroskademux ! vp9dec ! videoconvert ! video/x-raw,format=RGBA ! appsink name=frame sync=false max-buffers=1 drop=true").map_err(|e|clivuu_desktop::infrastructure::tasks::TaskError::Failed(e.to_string()))?.downcast::<gst::Pipeline>().unwrap();
            pipeline.by_name("file").unwrap().set_property("location",destination.to_str().unwrap());
            pipeline.set_state(gst::State::Playing).unwrap();
            let sink=pipeline.by_name("frame").unwrap().downcast::<gst_app::AppSink>().unwrap();
            let sample=sink.try_pull_sample(gst::ClockTime::from_seconds(5));
            let info=sample.and_then(|s|gst_video::VideoInfo::from_caps(s.caps()?).ok());
            pipeline.set_state(gst::State::Null).unwrap();
            Ok(info.map(|i|[i.width(),i.height()]))
        }).map_err(|e|e.to_string())?.result().await.map_err(|e|e.to_string())?;
        Ok::<_,String>(serde_json::json!({"dimensions":dimensions,"pixel":pixel,"secondDimensions":next.dimensions(),"path":stopped,"repeatedStop":completed,"size":size,"decoded":decode,"active":recording.busy()}))
    });
    let output = match result {
        Ok(value) => value,
        Err(error) => serde_json::json!({"error":error}),
    };
    std::fs::write(path.join("continuous-result.json"), output.to_string()).unwrap();
}
