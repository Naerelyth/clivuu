//! Private notification daemon: verify delivery, replacement and owner restart.
use clivuu_desktop::platform::integration::notifications::Notifications;
use gtk::{gio, glib, prelude::*};
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, time::Duration};

const BUNDLED: &[u8] = include_bytes!("../../assets/icons/clivoo.svg");
struct ThemeFixture {
    root: PathBuf,
    theme: gtk::IconTheme,
}
impl ThemeFixture {
    fn new() -> Self {
        gtk::init().unwrap();
        let root = PathBuf::from(format!(
            "/tmp/clivuu-notification-theme-{}",
            glib::uuid_string_random()
        ));
        for (prefix, name, icon, color) in [
            ("", "Canonical", "io.github.naerelyth.clivuu", "red"),
            ("", "Alias", "clivuu", "green"),
            ("", "hicolor", "io.github.naerelyth.clivuu", "blue"),
            ("alias-only", "Alias", "clivuu", "green"),
        ] {
            let directory = root.join(prefix).join(name);
            std::fs::create_dir_all(directory.join("48x48/apps")).unwrap();
            std::fs::write(directory.join("index.theme"), format!("[Icon Theme]\nName={name}\nDirectories=48x48/apps\n[48x48/apps]\nSize=48\nType=Fixed\nContext=Applications\n")).unwrap();
            std::fs::write(
                directory.join(format!("48x48/apps/{icon}.svg")),
                Self::svg(color),
            )
            .unwrap();
        }
        std::fs::create_dir_all(root.join("empty")).unwrap();
        let theme = gtk::IconTheme::for_display(&gtk::gdk::Display::default().unwrap());
        Self { root, theme }
    }
    fn svg(color: &str) -> Vec<u8> {
        format!("<svg xmlns='http://www.w3.org/2000/svg' width='48' height='48'><rect width='48' height='48' fill='{color}'/></svg>").into_bytes()
    }
    fn select(&self, name: &str) {
        let path = match name {
            "Missing" => self.root.join("empty"),
            "AliasOnly" => self.root.join("alias-only"),
            _ => self.root.clone(),
        };
        self.theme.set_search_path(&[&path]);
        gtk::Settings::default()
            .unwrap()
            .set_gtk_icon_theme_name(Some(if name == "AliasOnly" { "Alias" } else { name }));
    }
}
impl Drop for ThemeFixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
async fn peer() -> gio::DBusConnection {
    let address =
        gio::dbus_address_get_for_bus_sync(gio::BusType::Session, None::<&gio::Cancellable>)
            .unwrap();
    gio::DBusConnection::for_address_future(
        &address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
    )
    .await
    .unwrap()
}
async fn name(connection: &gio::DBusConnection, method: &str) {
    let params = if method == "RequestName" {
        ("org.freedesktop.Notifications", 0u32).to_variant()
    } else {
        ("org.freedesktop.Notifications",).to_variant()
    };
    connection
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            method,
            Some(&params),
            None,
            gio::DBusCallFlags::NONE,
            5000,
        )
        .await
        .unwrap();
}
fn daemon(
    connection: &gio::DBusConnection,
    id: u32,
    tx: async_channel::Sender<u32>,
    expected: Rc<RefCell<Vec<u8>>>,
    attachment: Rc<RefCell<Option<(PathBuf, clivuu_core::saving::Media)>>>,
) -> gio::RegistrationId {
    let info = gio::DBusNodeInfo::for_xml("<node><interface name='org.freedesktop.Notifications'><method name='Notify'><arg type='s' direction='in'/><arg type='u' direction='in'/><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='as' direction='in'/><arg type='a{sv}' direction='in'/><arg type='i' direction='in'/><arg type='u' direction='out'/></method></interface></node>").unwrap().lookup_interface("org.freedesktop.Notifications").unwrap();
    connection
        .register_object("/org/freedesktop/Notifications", &info)
        .method_call(move |_, _, _, _, _, args, call| {
            let (app, previous, icon, title, body, actions, hints, timeout) = args
                .get::<(
                    String,
                    u32,
                    String,
                    String,
                    String,
                    Vec<String>,
                    HashMap<String, glib::Variant>,
                    i32,
                )>()
                .unwrap();
            assert_eq!(app, "Clivuu");
            assert!(!icon.is_empty());
            let file = gio::File::for_uri(&icon);
            let bytes = std::fs::read(file.path().expect("notification icon file URI")).unwrap();
            assert_eq!(bytes, *expected.borrow());
            assert!(!hints.contains_key("image-path"));
            if let Some(entry) = hints.get("desktop-entry") {
                assert_ne!(entry.str(), Some("io.github.naerelyth.Clivuu.Desktop"));
            }
            assert_eq!(title, "Saved");
            assert_eq!(body, "A&amp;B");
            assert_eq!(timeout, -1);
            if let Some((_, media)) = attachment.borrow().as_ref() {
                assert_eq!(
                    actions,
                    vec![
                        "show-in-files",
                        &clivuu_desktop::platform::integration::files::show_label()
                    ]
                );
                if *media == clivuu_core::saving::Media::Image {
                    let (w, h, stride, alpha, bits, channels, pixels) = hints["image-data"]
                        .get::<(i32, i32, i32, bool, i32, i32, Vec<u8>)>()
                        .unwrap();
                    assert_eq!(
                        (w, h, stride, alpha, bits, channels),
                        (160, 80, 640, true, 8, 4)
                    );
                    assert!(pixels.chunks_exact(4).all(|p| p == [36, 104, 172, 255]));
                } else {
                    assert!(!hints.contains_key("image-data"));
                }
            } else {
                assert!(actions.is_empty());
                assert!(!hints.contains_key("image-data"));
            }
            call.return_value(Some(&(id,).to_variant()));
            tx.try_send(previous).unwrap();
        })
        .build()
        .unwrap()
}
fn main() {
    if let Some(expected) = std::env::args().nth(1) {
        clivuu_core::i18n::initialize(Some("system"));
        assert_eq!(
            clivuu_desktop::platform::integration::files::show_label(),
            expected
        );
        // Changing the configured locale also must not alter system actions.
        clivuu_core::i18n::initialize(Some("en"));
        assert_eq!(
            clivuu_desktop::platform::integration::files::show_label(),
            expected
        );
        clivuu_core::i18n::initialize(Some("zh-CN"));
        assert_eq!(
            clivuu_desktop::platform::integration::files::show_label(),
            expected
        );
        println!("System action locale is independent of application overrides");
        return;
    }
    let fixture = std::env::var_os("CLIVUU_TEST_NOTIFICATION_THEME").map(|_| ThemeFixture::new());
    glib::MainContext::default().block_on(async {
        let (tx, rx) = async_channel::bounded(4);
        let expected = Rc::new(RefCell::new(BUNDLED.to_vec()));
        if let Some(fixture) = &fixture {
            fixture.select("Canonical");
            *expected.borrow_mut() = ThemeFixture::svg("red");
        }
        let attachment = Rc::new(RefCell::new(None));
        let root=PathBuf::from(format!("/tmp/clivuu-notification-files-{}",glib::uuid_string_random()));
        std::fs::create_dir(&root).unwrap();
        let image=root.join("saved & image.png");
        clivuu_core::image::RgbaImage::from_pixel(360,180,clivuu_core::image::Rgba([36,104,172,255])).save(&image).unwrap();
        let video=root.join("recording.webm");std::fs::write(&video,b"video fixture").unwrap();
        let manager=peer().await;
        manager.call_future(Some("org.freedesktop.DBus"),"/org/freedesktop/DBus","org.freedesktop.DBus","RequestName",Some(&("org.freedesktop.FileManager1",0u32).to_variant()),None,gio::DBusCallFlags::NONE,5000).await.unwrap();
        let info=gio::DBusNodeInfo::for_xml("<node><interface name='org.freedesktop.FileManager1'><method name='ShowItems'><arg type='as' direction='in'/><arg type='s' direction='in'/></method></interface></node>").unwrap().lookup_interface("org.freedesktop.FileManager1").unwrap();
        let (shown_tx,shown_rx)=async_channel::bounded(4);
        let manager_registration=manager.register_object("/org/freedesktop/FileManager1",&info).method_call(move|_,_,_,_,_,args,call|{
            let(uris,startup)=args.get::<(Vec<String>,String)>().unwrap();assert!(startup.is_empty());call.return_value(None);shown_tx.try_send(uris).unwrap();
        }).build().unwrap();
        let first = peer().await;
        let registration = daemon(&first, 41, tx.clone(), expected.clone(), attachment.clone());
        name(&first, "RequestName").await;
        let client = peer().await;
        let notifications = Notifications::new(&client, Rc::new(clivuu_desktop::infrastructure::tasks::TaskPool::new(1, 2).unwrap()));
        notifications.send("saving", "Saved", "A&B");
        assert_eq!(rx.recv().await.unwrap(), 0);
        glib::timeout_future(Duration::from_millis(30)).await;
        if let Some(fixture) = &fixture {
            fixture.select("Alias");
            *expected.borrow_mut() = ThemeFixture::svg("blue");
        }
        *attachment.borrow_mut()=Some((image.clone(),clivuu_core::saving::Media::Image));
        notifications.send_file("saving", "Saved", "A&B", &image, clivuu_core::saving::Media::Image);
        assert_eq!(rx.recv().await.unwrap(), 41);
        glib::timeout_future(Duration::from_millis(30)).await;
        first.emit_signal(None,"/org/freedesktop/Notifications","org.freedesktop.Notifications","ActionInvoked",Some(&(41u32,"show-in-files").to_variant())).unwrap();
        let shown=glib::future_with_timeout(Duration::from_secs(2),shown_rx.recv()).await.unwrap().unwrap();
        assert_eq!(shown,vec![gio::File::for_path(&image).uri().to_string()]);
        name(&first, "ReleaseName").await;
        first.unregister_object(registration).unwrap();
        let second = peer().await;
        let registration = daemon(&second, 1, tx, expected.clone(), attachment.clone());
        name(&second, "RequestName").await;
        if let Some(fixture) = &fixture {
            fixture.select("AliasOnly");
            *expected.borrow_mut() = BUNDLED.to_vec();
        }
        *attachment.borrow_mut()=Some((video.clone(),clivuu_core::saving::Media::Video));
        notifications.send_file("saving", "Saved", "A&B", &video, clivuu_core::saving::Media::Video);
        assert_eq!(rx.recv().await.unwrap(), 0);
        glib::timeout_future(Duration::from_millis(30)).await;
        if let Some(fixture) = &fixture {
            fixture.select("Missing");
            *expected.borrow_mut() = BUNDLED.to_vec();
        }
        *attachment.borrow_mut()=None;
        notifications.send("saving", "Saved", "A&B");
        assert_eq!(rx.recv().await.unwrap(), 1);
        glib::timeout_future(Duration::from_millis(30)).await;
        second.emit_signal(None,"/org/freedesktop/Notifications","org.freedesktop.Notifications","ActionInvoked",Some(&(1u32,"show-in-files").to_variant())).unwrap();
        glib::timeout_future(Duration::from_millis(50)).await;
        assert!(shown_rx.try_recv().is_err(),"replacing a file notification must remove its old action");
        manager.unregister_object(manager_registration).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        name(&second, "ReleaseName").await;
        second.unregister_object(registration).unwrap();
        println!("Notification artwork, live theme switching, desktop identity, replacement and daemon restart passed");
    });
}
