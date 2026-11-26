//! Private-bus integration check: no desktop, lock screen or extension is used.
use clivuu_desktop::platform::integration::session_lock::SessionLock;
use gtk::{gio, glib, prelude::*};
use std::{cell::RefCell, rc::Rc, time::Duration};
fn main() {
    glib::MainContext::default().block_on(async {
        let connection = gio::bus_get_future(gio::BusType::Session).await.unwrap();
        let held = Rc::new(RefCell::new(None::<gio::DBusMethodInvocation>));
        let mut registrations = vec![];
        for (index,(name,path,interface)) in [
            ("org.freedesktop.ScreenSaver","/ScreenSaver","org.freedesktop.ScreenSaver"),
            ("org.gnome.ScreenSaver","/org/gnome/ScreenSaver","org.gnome.ScreenSaver"),
        ].into_iter().enumerate() {
            connection.call_future(Some("org.freedesktop.DBus"),"/org/freedesktop/DBus","org.freedesktop.DBus","RequestName",Some(&(name,0u32).to_variant()),None,gio::DBusCallFlags::NONE,5000).await.unwrap();
            let info=gio::DBusNodeInfo::for_xml(&format!("<node><interface name='{interface}'><method name='GetActive'><arg type='b' direction='out'/></method><signal name='ActiveChanged'><arg type='b'/></signal></interface></node>")).unwrap().lookup_interface(interface).unwrap();
            let held=held.clone();
            registrations.push(connection.register_object(path,&info).method_call(move|_,_,_,_,_,_,invocation|{
                if index==1 { held.replace(Some(invocation)); }
                else { invocation.return_value(Some(&(false,).to_variant())); }
            }).build().unwrap());
        }
        let changes=Rc::new(RefCell::new(vec![]));let observe=changes.clone();
        let watch=SessionLock::new(&connection,move|locked|observe.borrow_mut().push(locked));
        glib::future_with_timeout(Duration::from_secs(3),async {
            while held.borrow().is_none(){glib::timeout_future(Duration::from_millis(10)).await;}
        }).await.unwrap();
        let signal=async|gnome:bool,active:bool| {
            let (path,interface)=if gnome {("/org/gnome/ScreenSaver","org.gnome.ScreenSaver")}else{("/ScreenSaver","org.freedesktop.ScreenSaver")};
            connection.emit_signal(None,path,interface,"ActiveChanged",Some(&(active,).to_variant())).unwrap();
            connection.flush_future().await.unwrap();
            glib::timeout_future(Duration::from_millis(50)).await;
        };
        signal(true,true).await;
        assert_eq!(*changes.borrow(),vec![true]);
        held.borrow_mut().take().unwrap().return_value(Some(&(false,).to_variant()));
        glib::timeout_future(Duration::from_millis(50)).await;
        assert_eq!(*changes.borrow(),vec![true],"late GetActive must not undo a newer lock signal");
        signal(false,true).await;signal(true,false).await;
        assert_eq!(*changes.borrow(),vec![true],"one provider remains locked");
        signal(false,false).await;
        assert_eq!(*changes.borrow(),vec![true,false]);
        signal(true,true).await;
        let owner=async|method:&str|{
            let parameters=if method=="RequestName" {("org.gnome.ScreenSaver",0u32).to_variant()}else{("org.gnome.ScreenSaver",).to_variant()};
            connection.call_future(Some("org.freedesktop.DBus"),"/org/freedesktop/DBus","org.freedesktop.DBus",method,Some(&parameters),None,gio::DBusCallFlags::NONE,5000).await.unwrap();
            glib::timeout_future(Duration::from_millis(50)).await;
        };
        owner("ReleaseName").await;
        assert_eq!(*changes.borrow(),vec![true,false,true],"owner absence must not prematurely unlock pins");
        owner("RequestName").await;
        glib::future_with_timeout(Duration::from_secs(2),async {
            while held.borrow().is_none(){glib::timeout_future(Duration::from_millis(10)).await;}
        }).await.expect("replacement provider must be queried");
        let stale=held.borrow_mut().take().unwrap();
        owner("ReleaseName").await;owner("RequestName").await;
        glib::future_with_timeout(Duration::from_secs(2),async {
            while held.borrow().is_none(){glib::timeout_future(Duration::from_millis(10)).await;}
        }).await.unwrap();
        stale.return_value(Some(&(false,).to_variant()));
        glib::timeout_future(Duration::from_millis(50)).await;
        assert_eq!(*changes.borrow(),vec![true,false,true],"an earlier owner's reply must not unlock pins");
        held.borrow_mut().take().unwrap().return_value(Some(&(false,).to_variant()));
        glib::timeout_future(Duration::from_millis(50)).await;
        assert_eq!(*changes.borrow(),vec![true,false,true,false],"replacement GetActive must restore unlocked state");
        drop(watch);signal(true,true).await;
        assert_eq!(*changes.borrow(),vec![true,false,true,false],"dropped subscriptions must not invoke the application");
        for id in registrations { connection.unregister_object(id).unwrap(); }
        println!("Session lock: signal delivery, initial-query race, provider overlap, unlock, owner restart, stale-owner reply and teardown passed");
    });
}
