//! Exercise the actual D-Bus calls on a private bus, never the user's session.
use super::*;
use std::{
    cell::RefCell,
    io::BufRead,
    process::{Child, Command, Stdio},
};

struct Bus(Child, std::path::PathBuf);
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

#[test]
fn automatic_scroll_sends_wheel_axes_without_moving_the_pointer() {
    let directory = std::path::PathBuf::from(format!(
        "/tmp/clivuu-bus-test-{}",
        glib::uuid_string_random()
    ));
    std::fs::create_dir(&directory).unwrap();
    let mut bus = Bus(
        Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .env("XDG_RUNTIME_DIR", &directory)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
        directory,
    );
    let mut address = String::new();
    std::io::BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let connection = gio::DBusConnection::for_address_sync(
        address.trim(),
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )
    .unwrap();
    connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&(portal::DESTINATION, 0u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        )
        .unwrap();
    let context = glib::MainContext::new();
    context.with_thread_default(|| {
        let interface = gio::DBusNodeInfo::for_xml(r#"<node><interface name="org.freedesktop.portal.RemoteDesktop">
            <method name="NotifyPointerAxis"><arg type="o" direction="in"/><arg type="a{sv}" direction="in"/><arg type="d" direction="in"/><arg type="d" direction="in"/></method>
            <method name="NotifyPointerAxisDiscrete"><arg type="o" direction="in"/><arg type="a{sv}" direction="in"/><arg type="u" direction="in"/><arg type="i" direction="in"/></method>
            <method name="NotifyPointerMotionAbsolute"><arg type="o" direction="in"/><arg type="a{sv}" direction="in"/><arg type="u" direction="in"/><arg type="d" direction="in"/><arg type="d" direction="in"/></method>
            </interface></node>"#).unwrap().interfaces()[0].clone();
        let calls = Rc::new(RefCell::new(vec![]));
        let recorded = calls.clone();
        let registration = connection.register_object(portal::PATH, &interface).method_call(move |_,_,_,_,method,args,reply| {
            recorded.borrow_mut().push((method.to_string(),args));
            reply.return_value(None);
        }).build().unwrap();
        let mut session = Session::new(&connection, Backend::Portal {pointer:true}, &gio::Cancellable::new());
        session.path = "/test/session".into();
        session.closing.set(true);
        context.block_on(async {
            session.scroll(0, 3).await.unwrap();
            session.scroll(1, -2).await.unwrap();
            session.scroll_smooth(0.0, 12.5).await.unwrap();
            session.backend = Backend::Portal {pointer:false};
            assert!(session.scroll(0, 3).await.is_err());
            assert!(session.scroll_smooth(0.0, 12.5).await.is_err());
        });
        let calls = calls.borrow();
        assert_eq!(calls.len(), 3, "no absolute/relative pointer motion or immediate finish-fling calls are allowed");
        for ((method,args),(axis,steps)) in calls.iter().take(2).zip([(0u32,3i32),(1,-2)]) {
            assert_eq!(method, "NotifyPointerAxisDiscrete");
            assert_eq!(args.child_value(2).get::<u32>(),Some(axis));
            assert_eq!(args.child_value(3).get::<i32>(),Some(steps));
        }
        assert_eq!(calls[2].0, "NotifyPointerAxis");
        assert!(!calls[2].1.child_value(1).get::<Results>().unwrap().contains_key("finish"));
        assert_eq!(calls[2].1.child_value(2).get::<f64>(), Some(0.0));
        assert_eq!(calls[2].1.child_value(3).get::<f64>(), Some(12.5));
        connection.unregister_object(registration).unwrap();
    }).unwrap();
    connection.close_sync(gio::Cancellable::NONE).unwrap();
}

#[test]
fn window_picker_requests_only_one_window_and_closes_on_cancel_or_frame_failure() {
    use std::cell::Cell;
    let directory = std::path::PathBuf::from(format!(
        "/tmp/clivuu-window-bus-{}",
        glib::uuid_string_random()
    ));
    std::fs::create_dir(&directory).unwrap();
    let mut bus = Bus(
        Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .env("XDG_RUNTIME_DIR", &directory)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
        directory,
    );
    let mut address = String::new();
    std::io::BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let connection = gio::DBusConnection::for_address_sync(
        address.trim(),
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )
    .unwrap();
    connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&(portal::DESTINATION, 0u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        )
        .unwrap();
    let context = glib::MainContext::new();
    context.with_thread_default(|| {
        let interface=gio::DBusNodeInfo::for_xml(r#"<node><interface name="org.freedesktop.portal.ScreenCast">
            <property name="AvailableSourceTypes" type="u" access="read"/><property name="AvailableCursorModes" type="u" access="read"/>
            <method name="CreateSession"><arg type="a{sv}" direction="in"/><arg type="o" direction="out"/></method>
            <method name="SelectSources"><arg type="o" direction="in"/><arg type="a{sv}" direction="in"/><arg type="o" direction="out"/></method>
            <method name="Start"><arg type="o" direction="in"/><arg type="s" direction="in"/><arg type="a{sv}" direction="in"/><arg type="o" direction="out"/></method>
            <method name="OpenPipeWireRemote"><arg type="o" direction="in"/><arg type="a{sv}" direction="in"/><arg type="h" direction="out"/></method>
            </interface></node>"#).unwrap().interfaces()[0].clone();
        let close_interface=gio::DBusNodeInfo::for_xml("<node><interface name='org.freedesktop.portal.Session'><method name='Close'/></interface></node>").unwrap().interfaces()[0].clone();
        let sources=Rc::new(Cell::new(3u32));let cursor_modes=Rc::new(Cell::new(3u32));
        let source_property=sources.clone();let cursor_property=cursor_modes.clone();
        let calls=Rc::new(RefCell::new(vec![]));let recorded=calls.clone();
        let closes=Rc::new(Cell::new(0));let closed=closes.clone();
        let registrations=Rc::new(RefCell::new(vec![]));let dynamic=registrations.clone();
        let cancel_stage=Rc::new(Cell::new(""));let cancelled=cancel_stage.clone();
        let source_type=Rc::new(Cell::new(2u32));let stream_type=source_type.clone();
        let registration=connection.register_object(portal::PATH,&interface)
            .property(move|_,_,_,_,name|if name=="AvailableSourceTypes"{source_property.get().to_variant()}else{cursor_property.get().to_variant()})
            .method_call(move|connection,sender,_,_,method,args,reply|{
                if method=="OpenPipeWireRemote" {reply.return_dbus_error("org.freedesktop.DBus.Error.Failed","fixture stream failure");return;}
                let options=args.child_value(args.n_children()-1).get::<Results>().unwrap();
                recorded.borrow_mut().push((method.to_string(),options.clone()));
                let sender=sender.unwrap();
                let peer=sender.trim_start_matches(':').replace('.',"_");
                let request=format!("{}/request/{}/{}",portal::PATH,peer,options["handle_token"].str().unwrap());
                let values=if method=="CreateSession" {
                    let path=format!("{}/session/{}/{}",portal::PATH,peer,options["session_handle_token"].str().unwrap());
                    let closed=closed.clone();
                    let id=connection.register_object(&path,&close_interface).method_call(move|_,_,_,_,method,_,reply|{
                        assert_eq!(method,"Close");closed.set(closed.get()+1);reply.return_value(None);
                    }).build().unwrap();dynamic.borrow_mut().push(id);
                    Results::from_iter([("session_handle".into(),path.to_variant())])
                }else if method=="Start" {
                    // No compositor position is advertised: window streams need no screen origin.
                    let properties=Results::from_iter([("size".into(),(700i32,500i32).to_variant()),("source_type".into(),stream_type.get().to_variant())]);
                    Results::from_iter([("streams".into(),vec![(7u32,properties)].to_variant())])
                }else{Results::new()};
                connection.emit_signal(Some(sender),&request,"org.freedesktop.portal.Request","Response",Some(&((if cancelled.get()==method{1u32}else{0u32}),values).to_variant())).unwrap();
                reply.return_value(Some(&(glib::variant::ObjectPath::try_from(request).unwrap(),).to_variant()));
            }).build().unwrap();
        let cancel=gio::Cancellable::new();
        context.block_on(async {
            for cursor in [false,true] {
                let session=Session::window(&connection,&cancel,cursor).await.unwrap().unwrap();
                assert_eq!(session.streams.len(),1);assert!(session.streams[0].rect.is_none());
                { let calls=calls.borrow();let (_,options)=calls.iter().rev().find(|(m,_)|m=="SelectSources").unwrap();
                assert_eq!(options["types"].get::<u32>(),Some(2));assert_eq!(options["multiple"].get::<bool>(),Some(false));
                assert_eq!(options["cursor_mode"].get::<u32>(),Some(if cursor{2}else{1}));
                assert!(!options.contains_key("persist_mode")&&!options.contains_key("restore_token"));
                } let before=closes.get();session.close().await;assert_eq!(closes.get(),before+1);
            }
            for stage in ["CreateSession","SelectSources","Start"] {
                cancel_stage.set(stage);let before=closes.get();
                assert!(Session::window(&connection,&cancel,false).await.unwrap().is_none());
                assert_eq!(closes.get(),before+1,"cancel at {stage} must close before returning");
            }
            cancel_stage.set("");let pool=Rc::new(crate::infrastructure::tasks::TaskPool::new(1,2).unwrap());
            for kind in [1u32,2u32] {
                source_type.set(kind);let before=closes.get();
                assert!(super::super::window::capture(&connection,&pool,false,&cancel).await.is_err());
                assert_eq!(closes.get(),before+1,"wrong source or reader failure must close the session");
            }
            let before=calls.borrow().len();sources.set(1);
            assert!(Session::window(&connection,&cancel,false).await.is_err());
            sources.set(3);cursor_modes.set(1);
            assert!(Session::window(&connection,&cancel,true).await.is_err());
            assert_eq!(calls.borrow().len(),before,"unsupported capabilities never open a session or fall back to desktop");
            assert!(calls.borrow().iter().all(|(name,_)|["CreateSession","SelectSources","Start"].contains(&name.as_str())));
        });
        connection.unregister_object(registration).unwrap();
        for id in registrations.borrow_mut().drain(..){connection.unregister_object(id).unwrap();}
    }).unwrap();
    connection.close_sync(gio::Cancellable::NONE).unwrap();
}

#[test]
fn mutter_encloses_fractional_selections_and_single_monitor_portal_fills_missing_origin() {
    let directory = std::path::PathBuf::from(format!(
        "/tmp/clivuu-mutter-bus-{}",
        glib::uuid_string_random()
    ));
    std::fs::create_dir(&directory).unwrap();
    let mut bus = Bus(
        Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .env("XDG_RUNTIME_DIR", &directory)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
        directory,
    );
    let mut address = String::new();
    std::io::BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let connection = gio::DBusConnection::for_address_sync(
        address.trim(),
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )
    .unwrap();
    connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&(MUTTER, 0u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        )
        .unwrap();
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            let root_iface = gio::DBusNodeInfo::for_xml(
                r#"<node><interface name="org.gnome.Mutter.ScreenCast">
                <method name="CreateSession"><arg type="a{sv}" direction="in"/><arg type="o" direction="out"/></method>
            </interface></node>"#,
            )
            .unwrap()
            .interfaces()[0]
                .clone();
            let session_iface = gio::DBusNodeInfo::for_xml(
                r#"<node><interface name="org.gnome.Mutter.ScreenCast.Session">
                <method name="RecordArea"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="a{sv}" direction="in"/><arg type="o" direction="out"/></method>
                <method name="Start"/>
                <method name="Stop"/>
            </interface></node>"#,
            )
            .unwrap()
            .interfaces()[0]
                .clone();
            let stream_iface = gio::DBusNodeInfo::for_xml(
                r#"<node><interface name="org.gnome.Mutter.ScreenCast.Stream">
                <signal name="PipeWireStreamAdded"><arg type="u"/></signal>
            </interface></node>"#,
            )
            .unwrap()
            .interfaces()[0]
                .clone();
            let recorded_areas = Rc::new(RefCell::new(Vec::<(i32, i32, i32, i32)>::new()));
            let areas_ref = recorded_areas.clone();
            let registrations = Rc::new(RefCell::new(Vec::new()));
            let dyn_regs = registrations.clone();
            let seq = Rc::new(Cell::new(0u32));
            let seq_ref = seq.clone();
            let root_reg = connection
                .register_object(ROOT, &root_iface)
                .method_call(move |conn, sender, _, _, method, _, reply| {
                    assert_eq!(method, "CreateSession");
                    let id = seq_ref.get() + 1;
                    seq_ref.set(id);
                    let session_path = format!("{ROOT}/Session/s{id}");
                    let stream_path = format!("{ROOT}/Stream/s{id}");
                    let stream_reg = conn
                        .register_object(&stream_path, &stream_iface)
                        .build()
                        .unwrap();
                    let areas = areas_ref.clone();
                    let s_path = stream_path.clone();
                    let peer = sender.map(str::to_owned);
                    let session_reg = conn
                        .register_object(&session_path, &session_iface)
                        .method_call(move |conn, _, _, _, m, args, r| match m {
                            "RecordArea" => {
                                let (x, y, w, h, _) =
                                    args.get::<(i32, i32, i32, i32, Results)>().unwrap();
                                areas.borrow_mut().push((x, y, w, h));
                                r.return_value(Some(
                                    &(glib::variant::ObjectPath::try_from(s_path.as_str())
                                        .unwrap(),)
                                        .to_variant(),
                                ));
                            }
                            "Start" => {
                                r.return_value(None);
                                conn.emit_signal(
                                    peer.as_deref(),
                                    &s_path,
                                    "org.gnome.Mutter.ScreenCast.Stream",
                                    "PipeWireStreamAdded",
                                    Some(&(42u32,).to_variant()),
                                )
                                .unwrap();
                            }
                            "Stop" => r.return_value(None),
                            _ => unreachable!(),
                        })
                        .build()
                        .unwrap();
                    dyn_regs.borrow_mut().extend([stream_reg, session_reg]);
                    reply.return_value(Some(
                        &(glib::variant::ObjectPath::try_from(session_path).unwrap(),).to_variant(),
                    ));
                })
                .build()
                .unwrap();
            let cancel = gio::Cancellable::new();
            context.block_on(async {
                for (rect, expected) in [
                    (
                        Bounds {
                            x: 100.5,
                            y: 200.5,
                            width: 320.5,
                            height: 180.5,
                        },
                        (100, 200, 321, 181),
                    ),
                    (
                        Bounds {
                            x: -1200. + 1. / 3.,
                            y: 2. / 3.,
                            width: 1920. - 1. / 3. + 1e-13,
                            height: 100. + 2. / 3.,
                        },
                        (-1200, 0, 1920, 102),
                    ),
                    (
                        Bounds {
                            x: 10.25,
                            y: 20.75,
                            width: 0.25,
                            height: 0.25,
                        },
                        (10, 20, 1, 1),
                    ),
                ] {
                    let s = Session::mutter(&connection, &cancel, rect, false)
                        .await
                        .unwrap();
                    let area = Bounds {
                        x: f64::from(expected.0),
                        y: f64::from(expected.1),
                        width: f64::from(expected.2),
                        height: f64::from(expected.3),
                    };
                    assert_eq!(s.streams[0].rect, Some(area));
                    assert_eq!(recorded_areas.borrow().last().copied(), Some(expected));
                    s.close().await;
                }
                for invalid in [
                    Bounds {
                        x: f64::NAN,
                        y: 0.,
                        width: 10.,
                        height: 10.,
                    },
                    Bounds {
                        x: 0.,
                        y: 0.,
                        width: 0.,
                        height: 10.,
                    },
                    Bounds {
                        x: f64::from(i32::MAX) + 10.,
                        y: 0.,
                        width: 10.,
                        height: 10.,
                    },
                ] {
                    assert!(Session::mutter(&connection, &cancel, invalid, false).await.is_err());
                }
            });

            // Single-monitor portal fallback fills missing coordinates only when unambiguous.
            let out = clivuu_core::capture::Output {
                rect: Bounds {
                    x: 0.,
                    y: 0.,
                    width: 1920.,
                    height: 1080.,
                },
                scale: 1.5,
            };
            let mut portal_session =
                Session::new(&connection, Backend::Portal { pointer: false }, &cancel);
            portal_session.closing.set(true);
            portal_session.streams.push(Stream {
                node: 9,
                rect: None,
                properties: Results::from_iter([("source_type".into(), 1u32.to_variant())]),
            });
            portal_session.fallback_single_monitor(&[out, out]);
            assert!(portal_session.streams[0].rect.is_none());
            portal_session.streams[0]
                .properties
                .insert("source_type".into(), 2u32.to_variant());
            portal_session.fallback_single_monitor(&[out]);
            assert!(portal_session.streams[0].rect.is_none());
            portal_session.streams[0]
                .properties
                .insert("source_type".into(), 1u32.to_variant());
            portal_session.fallback_single_monitor(&[out]);
            assert_eq!(portal_session.streams[0].rect, Some(out.rect));

            connection.unregister_object(root_reg).unwrap();
            for id in registrations.borrow_mut().drain(..) {
                connection.unregister_object(id).unwrap();
            }
        })
        .unwrap();
    connection.close_sync(gio::Cancellable::NONE).unwrap();
}
