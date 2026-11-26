//! Stable desktop identity shared by native packages and portable bundles.
//! Resolve renamed launchers using standard Desktop Entry metadata; never tie
//! window grouping to a package format or change the business D-Bus endpoint.
use gtk::{gdk, gio, glib, glib::translate::*, prelude::*};
use std::ffi::CString;

pub const APPLICATION_ID: &str = "io.github.naerelyth.clivuu";
const DESKTOP: &str = "io.github.naerelyth.clivuu.desktop";

fn is_clivuu(info: &gio::DesktopAppInfo) -> bool {
    info.startup_wm_class().as_deref() == Some(APPLICATION_ID)
        || info.id().as_deref() == Some(DESKTOP)
        // Existing integrations may predate StartupWMClass. This is standard
        // launcher metadata too, rather than a manager-specific extension.
        || info.name() == "Clivuu"
}

fn installed() -> Option<gio::DesktopAppInfo> {
    if let Some(info) = std::env::var_os("GIO_LAUNCHED_DESKTOP_FILE")
        .and_then(gio::DesktopAppInfo::from_filename)
        .filter(is_clivuu)
        // Autostart and downloaded desktop files also have an ID, but portals
        // can resolve only installed application entries. Use their metadata.
        .and_then(|info| info.id())
        .and_then(|id| gio::DesktopAppInfo::new(&id))
        .filter(is_clivuu)
    {
        return Some(info);
    }
    gio::DesktopAppInfo::new(DESKTOP).or_else(|| {
        let mut apps: Vec<_> = gio::AppInfo::all()
            .into_iter()
            .filter_map(|app| app.downcast::<gio::DesktopAppInfo>().ok())
            .filter(is_clivuu)
            .collect();
        apps.sort_by_key(|app| app.id());
        apps.into_iter().next()
    })
}

/// Resolved launcher ID, shared by windows, system consent and notifications.
pub fn desktop_id() -> Option<String> {
    installed()
        .and_then(|info| info.id())
        .map(|id| id.trim_end_matches(".desktop").to_owned())
}
pub fn configure(window: &impl IsA<gtk::Window>) {
    let window = window.as_ref();
    let id = CString::new(desktop_id().unwrap_or_else(|| APPLICATION_ID.into()))
        .expect("desktop file ID");
    let display = gtk::prelude::WidgetExt::display(window);
    if display.type_().name() == "GdkX11Display" {
        // SAFETY: loaded GTK's X11 API, exact header signature, concrete
        // display verified. Set before this window's surface is realized.
        unsafe {
            let symbol = libc::dlsym(
                libc::RTLD_DEFAULT,
                c"gdk_x11_display_set_program_class".as_ptr(),
            );
            if !symbol.is_null() {
                let set: unsafe extern "C" fn(*mut gdk::ffi::GdkDisplay, *const libc::c_char) =
                    std::mem::transmute(symbol);
                set(display.to_glib_none().0, id.as_ptr());
            }
        }
    }
    // Wayland has no xdg_toplevel during GTK's realize signal yet. Apply
    // the ID after mapping, when GDK's public setter can send set_app_id.
    window.connect_map(move |window| {
        let Some(surface) = window.surface() else {
            return;
        };
        if surface.type_().name() == "GdkWaylandToplevel" {
            // SAFETY: exact gdkwaylandtoplevel.h signature; never call a
            // Wayland function on an X11 surface or a layer-shell window.
            unsafe {
                let symbol = libc::dlsym(
                    libc::RTLD_DEFAULT,
                    c"gdk_wayland_toplevel_set_application_id".as_ptr(),
                );
                if !symbol.is_null() {
                    let set: unsafe extern "C" fn(*mut gdk::ffi::GdkSurface, *const libc::c_char) =
                        std::mem::transmute(symbol);
                    set(surface.to_glib_none().0, id.as_ptr());
                }
            }
        }
        // X11 taskbars can also use the actual texture when no launcher has
        // been installed. Keep the icon independent of the transient mount.
        if let Ok(toplevel) = surface.downcast::<gdk::Toplevel>() {
            let bytes =
                glib::Bytes::from_static(include_bytes!("../../../../assets/icons/clivoo.svg"));
            let stream = gio::MemoryInputStream::from_bytes(&bytes);
            if let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_stream_at_scale(
                &stream,
                128,
                128,
                true,
                None::<&gio::Cancellable>,
            ) {
                toplevel.set_icon_list(&[gdk::Texture::for_pixbuf(&pixbuf)]);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command};

    #[test]
    fn launch_sources_resolve_to_installed_application_ids() {
        // Each child gets fresh GIO directory caches, without mutating this
        // multithreaded test process's environment or reading user launchers.
        if let Ok(expected) = std::env::var("CLIVUU_TEST_IDENTITY_EXPECTED") {
            let actual = desktop_id();
            assert_eq!(
                actual.as_deref(),
                (!expected.is_empty()).then_some(expected.as_str())
            );
            if let Some(id) = actual {
                assert!(gio::DesktopAppInfo::new(&format!("{id}.desktop")).is_some());
            }
            return;
        }
        let root =
            std::env::temp_dir().join(format!("clivuu-identity-{}", glib::uuid_string_random()));
        let canonical = "data/applications/io.github.naerelyth.clivuu.desktop";
        let renamed = "data/applications/clivuu.desktop";
        let autostart = "config/autostart/io.github.naerelyth.clivuu.desktop";
        let cases: &[(&str, &[&str], Option<&str>, &str)] = &[
            ("canonical", &[canonical], None, APPLICATION_ID),
            ("renamed-autostart", &[renamed], Some(autostart), "clivuu"),
            (
                "preferred-launcher",
                &[canonical, renamed],
                Some(renamed),
                "clivuu",
            ),
            (
                "external-launcher",
                &[renamed],
                Some("downloads/external.desktop"),
                "clivuu",
            ),
            (
                "system-install",
                &["system/applications/clivuu.desktop"],
                Some(autostart),
                "clivuu",
            ),
            (
                "nested-install",
                &["data/applications/vendor/clivuu.desktop"],
                Some(autostart),
                "vendor-clivuu",
            ),
            ("autostart-without-install", &[], Some(autostart), ""),
            ("uninstalled", &[], None, ""),
        ];
        for (name, applications, launched, expected) in cases {
            let directory = root.join(name);
            for relative in applications.iter().copied().chain(*launched) {
                let path = directory.join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, format!("[Desktop Entry]\nType=Application\nName=Clivuu\nExec=/usr/bin/true\nStartupWMClass={APPLICATION_ID}\n")).unwrap();
            }
            let mut child = Command::new(std::env::current_exe().unwrap());
            child.args(["--exact", "platform::integration::identity::tests::launch_sources_resolve_to_installed_application_ids", "--nocapture"])
                    .env("HOME", directory.join("home"))
                    .env("XDG_DATA_HOME", directory.join("data"))
                    .env("XDG_DATA_DIRS", directory.join("system"))
                    .env("XDG_CONFIG_HOME", directory.join("config"))
                    .env("XDG_CACHE_HOME", directory.join("cache"))
                    .env("XDG_STATE_HOME", directory.join("state"))
                    .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/tmp/no-clivuu-identity-test-bus")
                    .env_remove("XDG_CURRENT_DESKTOP")
                    .env("CLIVUU_TEST_IDENTITY_EXPECTED", expected)
                    .env_remove("GIO_LAUNCHED_DESKTOP_FILE");
            if let Some(path) = launched {
                child.env("GIO_LAUNCHED_DESKTOP_FILE", directory.join(path));
            }
            let result = child.output().unwrap();
            assert!(
                result.status.success(),
                "{name}: {}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
