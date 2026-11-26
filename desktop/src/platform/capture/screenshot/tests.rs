//! Real request/response messages, isolated from the user's portal and permissions.
use super::*;
use std::{
    cell::{Cell, RefCell},
    io::BufRead,
    process::{Child, Command, Stdio},
    rc::Rc,
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
fn ordinary_screenshot_remains_noninteractive_and_preserves_original_pixels() {
    let directory =
        std::path::PathBuf::from(format!("/tmp/clivuu-window-{}", glib::uuid_string_random()));
    std::fs::create_dir(&directory).unwrap();
    let image = RgbaImage::from_fn(73, 41, |x, y| {
        clivuu_core::image::Rgba([x as u8, y as u8, 99, 255])
    });
    let path = directory.join("window.png");
    clivuu_core::image::export::write(
        clivuu_core::image::export::Source::Pixels(&image),
        &path,
        100,
        std::fs::File::create(&path).unwrap(),
    )
    .unwrap();
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
    let context = glib::MainContext::new();
    context.with_thread_default(|| {
        let interface = gio::DBusNodeInfo::for_xml(r#"<node><interface name="org.freedesktop.portal.Screenshot">
            <method name="Screenshot"><arg type="s" direction="in"/><arg type="a{sv}" direction="in"/><arg type="o" direction="out"/></method>
            </interface></node>"#).unwrap().interfaces()[0].clone();
        let response = Rc::new(Cell::new(0u32));
        let response_code = response.clone();
        let calls = Rc::new(RefCell::new(vec![]));
        let recorded = calls.clone();
        let uri = gio::File::for_path(&path).uri().to_string();
        let registration = connection.register_object(portal::PATH, &interface)
            .method_call(move |connection,sender,_,_,method,args,reply| {
                assert_eq!(method,"Screenshot"); // No ScreenCast API is exported by this fixture.
                assert_eq!(args.child_value(0).str(),Some(""));
                let options = args.child_value(1).get::<Results>().unwrap();
                let token = options["handle_token"].str().unwrap();
                let sender = sender.unwrap();
                let handle = format!("{}/request/{}/{}",portal::PATH,sender.trim_start_matches(':').replace('.',"_"),token);
                recorded.borrow_mut().push(options);
                let values = Results::from_iter([("uri".into(),uri.to_variant())]);
                // Also covers an immediate response arriving before the method reply.
                connection.emit_signal(Some(sender), &handle, "org.freedesktop.portal.Request", "Response",
                    Some(&(response_code.get(),values).to_variant())).unwrap();
                reply.return_value(Some(&(glib::variant::ObjectPath::try_from(handle).unwrap(),).to_variant()));
            }).build().unwrap();
        let screenshot = Screenshot::with_destination(&connection, &connection.unique_name().unwrap());
        let cancel = gio::Cancellable::new();
        let pool = TaskPool::new(1,2).unwrap();
        context.block_on(async {
            let actual = screenshot.capture(&pool,&cancel).await.unwrap();
            assert_eq!(&*actual,&image);
            let options=calls.borrow()[0].clone();
            assert_eq!(options["interactive"].get::<bool>(),Some(false));
            assert!(!options.contains_key("target"));
            response.set(1);
            assert!(screenshot.capture(&pool,&cancel).await.unwrap_err().contains("cancelled"));
            response.set(2);
            assert!(screenshot.capture(&pool,&cancel).await.is_err());
            cancel.cancel();
            assert!(screenshot.capture(&pool,&cancel).await.is_err());
        });
        connection.unregister_object(registration).unwrap();
    }).unwrap();
    connection.close_sync(gio::Cancellable::NONE).unwrap();
}
