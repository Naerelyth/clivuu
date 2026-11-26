//! XDG Autostart is operating-system state, not a second INI boolean.
use super::{identity::APPLICATION_ID, launch::Launch};
use gtk::{gio, glib, prelude::*};
use std::path::{Path, PathBuf};
const GROUP: &str = "Desktop Entry";
const OWNED: &str = "X-Clivuu-Autostart";
#[derive(Clone)]
pub struct Autostart {
    user: PathBuf,
    system: Vec<PathBuf>,
}
impl Default for Autostart {
    fn default() -> Self {
        let filename = format!("{APPLICATION_ID}.desktop");
        Self {
            user: glib::user_config_dir().join("autostart").join(&filename),
            system: glib::system_config_dirs()
                .iter()
                .map(|d| d.join("autostart").join(&filename))
                .collect(),
        }
    }
}
impl Autostart {
    fn effective(&self) -> Option<&Path> {
        std::iter::once(&self.user)
            .chain(self.system.iter())
            .find(|p| p.exists())
            .map(PathBuf::as_path)
    }
    fn read(path: &Path) -> Result<glib::KeyFile, String> {
        let file = glib::KeyFile::new();
        file.load_from_file(path, glib::KeyFileFlags::NONE)
            .map_err(|e| e.to_string())?;
        Ok(file)
    }
    pub fn enabled(&self) -> Result<bool, String> {
        let Some(path) = self.effective() else {
            return Ok(false);
        };
        let file = Self::read(path)?;
        Ok(!file.boolean(GROUP, "Hidden").unwrap_or(false)
            && file
                .boolean(GROUP, "X-GNOME-Autostart-enabled")
                .unwrap_or(true))
    }
    pub fn set(&self, enabled: bool, launch: Option<&Launch>) -> Result<bool, String> {
        if enabled {
            let launch = launch
                .ok_or_else(|| clivuu_core::i18n::t("The application launcher is unavailable"))?;
            self.write(&Self::entry(launch)?)?;
        } else if self.system.iter().any(|p| p.exists()) {
            // The user override must suppress a system-wide entry of this name.
            let file = glib::KeyFile::new();
            file.set_string(GROUP, "Type", "Application");
            file.set_string(GROUP, "Name", "Clivuu");
            file.set_boolean(GROUP, "Hidden", true);
            self.write(&file)?;
        } else {
            match std::fs::remove_file(&self.user) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        self.enabled()
    }
    fn entry(launch: &Launch) -> Result<glib::KeyFile, String> {
        let file = glib::KeyFile::new();
        for (key, value) in [
            ("Type", "Application"),
            ("Name", "Clivuu"),
            ("Icon", APPLICATION_ID),
        ] {
            file.set_string(GROUP, key, value);
        }
        file.set_string(GROUP, "Exec", &launch.desktop_exec()?);
        file.set_boolean(GROUP, "Terminal", false);
        file.set_boolean(GROUP, "StartupNotify", false);
        file.set_string(GROUP, "StartupWMClass", APPLICATION_ID);
        file.set_boolean(GROUP, OWNED, true);
        Ok(file)
    }
    fn write(&self, file: &glib::KeyFile) -> Result<(), String> {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(self.user.parent().unwrap())
            .map_err(|e| e.to_string())?;
        gio::File::for_path(&self.user)
            .replace_contents(
                file.to_data().as_bytes(),
                None,
                false,
                gio::FileCreateFlags::PRIVATE | gio::FileCreateFlags::REPLACE_DESTINATION,
                None::<&gio::Cancellable>,
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    /// Only repair an already enabled entry created by Clivuu. Never enable one
    /// on startup or overwrite a foreign/system launcher as a side effect.
    pub fn refresh(&self, launch: &Launch) -> Result<(), String> {
        if !self.user.is_file() || !self.enabled()? {
            return Ok(());
        }
        let file = Self::read(&self.user)?;
        if file.boolean(GROUP, OWNED).unwrap_or(false)
            && file.string(GROUP, "Exec").ok().as_deref() != Some(launch.desktop_exec()?.as_str())
        {
            self.write(&Self::entry(launch)?)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    struct Fixture {
        root: PathBuf,
        autostart: Autostart,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("clivuu-startup-{}", glib::uuid_string_random()));
            std::fs::create_dir_all(&root).unwrap();
            let autostart = Autostart {
                user: root.join("user/autostart/clivuu.desktop"),
                system: vec![root.join("system/autostart/clivuu.desktop")],
            };
            Self { root, autostart }
        }
        fn launch(&self, name: &str) -> Launch {
            let path = self.root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                r#"#!/bin/sh
printf '%s' "$0" > "$0.result"
"#,
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            Launch::at(path, name.ends_with("AppImage")).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn startup_defaults_off_and_native_or_portable_launchers_round_trip() {
        let f = Fixture::new();
        assert!(!f.autostart.enabled().unwrap());
        f.autostart.set(false, None).unwrap();
        assert!(!f.autostart.user.parent().unwrap().exists());
        for name in [
            "usr/bin/clivuu",
            "opt/Clivuu/bin/clivuu",
            "Apps/Clivuu 100% $`\"\\.AppImage",
        ] {
            let launch = f.launch(name);
            assert!(f.autostart.set(true, Some(&launch)).unwrap());
            let file = Autostart::read(&f.autostart.user).unwrap();
            assert_eq!(
                file.string(GROUP, "Exec").unwrap(),
                launch.desktop_exec().unwrap()
            );
            // GLib parses the actual freedesktop Exec grammar: no shell,
            // no field-code interpolation, even for literal percent/quotes.
            let app = gio::DesktopAppInfo::from_keyfile(&file).unwrap_or_else(|| {
                panic!("invalid desktop launcher: {name}, {:?}", file.to_data())
            });
            app.launch(&[], None::<&gio::AppLaunchContext>).unwrap();
            let result = f.root.join(format!("{name}.result"));
            for _ in 0..100 {
                if result.exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert_eq!(
                std::fs::read_to_string(result).unwrap(),
                f.root.join(name).to_str().unwrap()
            );
            assert_eq!(
                std::fs::metadata(&f.autostart.user)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert!(!f.autostart.set(false, None).unwrap());
            assert!(!f.autostart.user.exists());
        }
    }
    #[test]
    fn disabling_system_startup_writes_a_user_hidden_override() {
        let f = Fixture::new();
        let launch = f.launch("bin/clivuu");
        std::fs::create_dir_all(f.autostart.system[0].parent().unwrap()).unwrap();
        std::fs::write(
            &f.autostart.system[0],
            Autostart::entry(&launch).unwrap().to_data(),
        )
        .unwrap();
        assert!(f.autostart.enabled().unwrap());
        assert!(!f.autostart.set(false, None).unwrap());
        assert!(
            Autostart::read(&f.autostart.user)
                .unwrap()
                .boolean(GROUP, "Hidden")
                .unwrap()
        );
        f.autostart.refresh(&launch).unwrap();
        assert!(!f.autostart.enabled().unwrap());
        assert!(f.autostart.set(true, Some(&launch)).unwrap());
    }
    #[test]
    fn refresh_updates_only_enabled_entries_owned_by_clivuu() {
        let f = Fixture::new();
        let old = f.launch("old/clivuu");
        let new = f.launch("new/clivuu");
        f.autostart.refresh(&new).unwrap();
        assert!(!f.autostart.user.exists());
        f.autostart.set(true, Some(&old)).unwrap();
        f.autostart.refresh(&new).unwrap();
        assert_eq!(
            Autostart::read(&f.autostart.user)
                .unwrap()
                .string(GROUP, "Exec")
                .unwrap(),
            new.desktop_exec().unwrap()
        );
        let foreign = Autostart::entry(&old).unwrap();
        foreign.remove_key(GROUP, OWNED).unwrap();
        f.autostart.write(&foreign).unwrap();
        f.autostart.refresh(&new).unwrap();
        assert_eq!(
            std::fs::read_to_string(&f.autostart.user).unwrap(),
            foreign.to_data()
        );
        assert!(f.autostart.set(true, None).is_err());
        assert_eq!(
            std::fs::read_to_string(&f.autostart.user).unwrap(),
            foreign.to_data()
        );
    }
}
