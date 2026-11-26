//! Private D-Bus fixture: no real desktop portal is called.
use clivuu_desktop::platform::portal::{self, Results};
use clivuu_desktop::{infrastructure::tasks::TaskPool, platform::capture::screenshot::Screenshot};
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
fn main() {
    glib::MainContext::default().block_on(async {
    let connection=gio::bus_get_future(gio::BusType::Session).await.unwrap();let destination=connection.unique_name().unwrap();let mode=Rc::new(Cell::new(0u8));let ids=Rc::new(RefCell::new(vec![]));let (closed_tx,closed_rx)=async_channel::bounded(4);
    let xml=gio::DBusNodeInfo::for_xml("<node><interface name='org.freedesktop.portal.Screenshot'><method name='PickColor'><arg type='s' direction='in'/><arg type='a{sv}' direction='in'/><arg type='o' direction='out'/></method></interface><interface name='org.freedesktop.portal.Request'><method name='Close'/></interface></node>").unwrap();let screenshot=xml.lookup_interface("org.freedesktop.portal.Screenshot").unwrap();let request=xml.lookup_interface("org.freedesktop.portal.Request").unwrap();
    let current=mode.clone();let requests=ids.clone();let root=connection.register_object(portal::PATH,&screenshot).method_call(move |connection,sender,_,_,_,params,invocation|{
        let (_,options)=params.get::<(String,Results)>().unwrap();let token=options["handle_token"].get::<String>().unwrap();let sender=sender.unwrap();let path=format!("{}/request/{}/{}",portal::PATH,sender.trim_start_matches(':').replace('.',"_"),token);
        let closed=closed_tx.clone();let id=connection.register_object(&path,&request).method_call(move |_,_,_,_,_,_,invocation|{let _=closed.try_send(());invocation.return_value(None);}).build().unwrap();requests.borrow_mut().push(id);
        let object=glib::variant::ObjectPath::try_from(path.as_str()).unwrap();
        if current.get()==0{
            let values=Results::from([("color".into(),(0.1f64,0.2f64,0.3f64).to_variant())]);connection.emit_signal(Some(sender),&path,"org.freedesktop.portal.Request","Response",Some(&(0u32,values).to_variant())).unwrap();
            // Deliberately delay the method reply until after the response.
            glib::timeout_add_local_once(Duration::from_millis(80),move ||invocation.return_value(Some(&(object,).to_variant())));
        }else{invocation.return_value(Some(&(object,).to_variant()));}
    }).build().unwrap();
    let c=gio::Cancellable::new();let values=portal::request(&connection,&destination,"Screenshot","PickColor",vec!["".to_variant()],Results::new(),&c).await.unwrap();assert_eq!(values["color"].get::<(f64,f64,f64)>(),Some((0.1,0.2,0.3)));assert!(closed_rx.try_recv().is_err());
    mode.set(1);let cancellation=gio::Cancellable::new();let cancel=cancellation.clone();glib::timeout_add_local_once(Duration::from_millis(20),move ||cancel.cancel());
    let cancelled=portal::request(&connection,&destination,"Screenshot","PickColor",vec!["".to_variant()],Results::new(),&cancellation).await;assert!(cancelled.is_err());closed_rx.recv().await.unwrap();assert!(closed_rx.try_recv().is_err());
    connection.unregister_object(root).unwrap();
    let directory=std::path::PathBuf::from(format!("/tmp/clivuu-static-portal-{}",std::process::id()));std::fs::create_dir_all(&directory).unwrap();
    let source=directory.join("source.png");let original=clivuu_core::image::RgbaImage::from_pixel(7,5,clivuu_core::image::Rgba([37,89,151,203]));clivuu_core::image_ops::save(&original,source.to_str().unwrap()).unwrap();
    let source_bytes=std::fs::read(&source).unwrap();
    let uri=Rc::new(RefCell::new(gio::File::for_path(&source).uri().to_string()));
    let calls=Rc::new(Cell::new(0));
    let xml=gio::DBusNodeInfo::for_xml("<node><interface name='org.freedesktop.portal.Screenshot'><method name='Screenshot'><arg type='s' direction='in'/><arg type='a{sv}' direction='in'/><arg type='o' direction='out'/></method></interface></node>").unwrap();
    let info=xml.lookup_interface("org.freedesktop.portal.Screenshot").unwrap();let served=uri.clone();let count=calls.clone();
    let root=connection.register_object(portal::PATH,&info).method_call(move|connection,sender,_,_,_,params,invocation|{
        let (_,options)=params.get::<(String,Results)>().unwrap();assert_eq!(options["interactive"].get::<bool>(),Some(false));assert_eq!(options["modal"].get::<bool>(),Some(false));
        if count.get()==0{assert!(!options.contains_key("target"));}count.set(count.get()+1);
        let sender=sender.unwrap();let token=options["handle_token"].str().unwrap();let path=format!("{}/request/{}/{}",portal::PATH,sender.trim_start_matches(':').replace('.',"_"),token);
        let values=Results::from_iter([("uri".into(),served.borrow().to_variant())]);connection.emit_signal(Some(sender),&path,"org.freedesktop.portal.Request","Response",Some(&(0u32,values).to_variant())).unwrap();
        let object=glib::variant::ObjectPath::try_from(path.as_str()).unwrap();glib::timeout_add_local_once(Duration::from_millis(50),move||invocation.return_value(Some(&(object,).to_variant())));
    }).build().unwrap();
    let screenshot=Screenshot::with_destination(&connection,&destination);let pool=TaskPool::new(1,2).unwrap();let cancel=gio::Cancellable::new();
    let pixels=screenshot.capture(&pool,&cancel).await.unwrap();assert_eq!(*pixels,original);assert_eq!(std::fs::read(&source).unwrap(),source_bytes);
    let before=calls.get();let stopped=gio::Cancellable::new();stopped.cancel();assert!(screenshot.capture(&pool,&stopped).await.is_err());assert_eq!(calls.get(),before);
    *uri.borrow_mut()="https://example.invalid/image.png".into();assert!(screenshot.capture(&pool,&cancel).await.is_err());
    let invalid=directory.join("invalid.png");std::fs::write(&invalid,b"not an image").unwrap();*uri.borrow_mut()=gio::File::for_path(&invalid).uri().to_string();assert!(screenshot.capture(&pool,&cancel).await.is_err());
    let large=directory.join("large.png");std::fs::File::create(&large).unwrap().set_len(clivuu_core::image_ops::MAX_BYTES+1).unwrap();*uri.borrow_mut()=gio::File::for_path(&large).uri().to_string();assert!(screenshot.capture(&pool,&cancel).await.is_err());assert_eq!(std::fs::metadata(&large).unwrap().len(),clivuu_core::image_ops::MAX_BYTES+1);
    assert_eq!(clivuu_core::image_ops::load(source.to_str().unwrap()).unwrap(),original);
    if std::env::var_os("CLIVUU_TEST_REGISTERED_PORTAL").is_some() {
        // A registered non-GNOME portal must work with no GTK display/focus at
        // all. An accidental permission-owner dependency would panic here.
        connection.call_future(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus",
            "org.freedesktop.DBus", "RequestName", Some(&(portal::DESTINATION, 0u32).to_variant()),
            None, gio::DBusCallFlags::NONE, 5000).await.unwrap();
        let info=gio::DBusNodeInfo::for_xml("<node><interface name='org.freedesktop.host.portal.Registry'><method name='Register'><arg type='s' direction='in'/><arg type='a{sv}' direction='in'/></method></interface></node>").unwrap().lookup_interface("org.freedesktop.host.portal.Registry").unwrap();
        let registered=Rc::new(Cell::new(false));let flag=registered.clone();
        let registry=connection.register_object(portal::PATH,&info).method_call(move|_,_,_,_,_,params,invocation|{
            let(id,_)=params.get::<(String,Results)>().unwrap();assert_eq!(id,"io.github.naerelyth.clivuu");
            flag.set(true);invocation.return_value(None);
        }).build().unwrap();
        connection.call_future(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus",
            "org.freedesktop.DBus", "RequestName", Some(&("org.freedesktop.impl.portal.PermissionStore", 0u32).to_variant()),
            None, gio::DBusCallFlags::NONE, 5000).await.unwrap();
        let permission_info=gio::DBusNodeInfo::for_xml("<node><interface name='org.freedesktop.impl.portal.PermissionStore'><method name='Lookup'><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='a{sas}' direction='out'/><arg type='v' direction='out'/></method></interface></node>").unwrap().lookup_interface("org.freedesktop.impl.portal.PermissionStore").unwrap();
        let lookups=Rc::new(Cell::new(0));let lookup_count=lookups.clone();
        let permission=connection.register_object("/org/freedesktop/impl/portal/PermissionStore",&permission_info).method_call(move|_,_,_,_,_,_,invocation|{
            lookup_count.set(lookup_count.get()+1);
            let empty=std::collections::HashMap::<String,Vec<String>>::new();
            invocation.return_value(Some(&(empty,0u8.to_variant()).to_variant()));
        }).build().unwrap();
        *uri.borrow_mut()=gio::File::for_path(&source).uri().to_string();
        let before=calls.get();let native=Screenshot::connect(&cancel).await.unwrap();
        assert!(registered.get());assert_eq!(*native.capture(&pool,&cancel).await.unwrap(),original);
        assert_eq!(calls.get(),before+1,"non-GNOME must use one static frame, without a focus/permission warmup");
        assert_eq!(lookups.get(),0,"non-GNOME consent must be left to its native portal");
        connection.unregister_object(permission).unwrap();
        connection.unregister_object(registry).unwrap();
        println!("Registered {:?} static portal works without GTK activation or an extra permission window",std::env::var("XDG_CURRENT_DESKTOP").unwrap());
    }
    connection.unregister_object(root).unwrap();for id in ids.take(){connection.unregister_object(id).unwrap();}std::fs::remove_dir_all(directory).unwrap();
    println!("Portal early response/cancel/Close + static RGBA decode, precancel, remote URI rejection, malformed image, actual byte bound and portal-file preservation passed");
});
}
