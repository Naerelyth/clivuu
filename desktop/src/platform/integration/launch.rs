//! A durable entry point shared by login startup and application restart.
//! Package formats affect discovery only; no shell command or old action is replayed.
use clivuu_core::i18n::t;
use std::{
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::PathBuf,
    process::Command,
};
#[derive(Clone, Debug)]
pub struct Launch {
    program: PathBuf,
    portable: bool,
}
impl Launch {
    pub fn current() -> Result<Self, String> {
        // AppImage's extracted CLI/service cache is intentionally ephemeral.
        // The runtime supplies the original, durable image path to every role.
        if let Some(image) = std::env::var_os("APPIMAGE") {
            return Self::at(image.into(), true);
        }
        if let Some(cli) = std::env::var_os("CLIVUU_BINARY") {
            return Self::at(cli.into(), false);
        }
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let cli = executable.with_file_name("clivuu");
        Self::at(if cli.is_file() { cli } else { executable }, false)
    }
    pub(super) fn at(program: PathBuf, portable: bool) -> Result<Self, String> {
        let metadata =
            std::fs::metadata(&program).map_err(|e| format!("{}: {e}", program.display()))?;
        if !program.is_absolute()
            || !metadata.is_file()
            || metadata.permissions().mode() & 0o111 == 0
        {
            return Err(t("The application launcher is unavailable"));
        }
        Ok(Self { program, portable })
    }
    pub fn desktop_exec(&self) -> Result<String, String> {
        let program = self
            .program
            .to_str()
            .ok_or_else(|| t("The launcher path is not valid UTF-8"))?;
        // Desktop Entry forbids '=' in its executable name/path. GLib also
        // validates argv[0] before expanding %%: a percent-containing portable
        // path must be an argument to a standard executable instead.
        if program.contains('=') {
            return Err(t("Desktop launchers cannot use a path containing ="));
        }
        if program.contains('%') {
            let env = gtk::glib::find_program_in_path("env")
                .ok_or_else(|| t("The application launcher is unavailable"))?;
            return Ok(format!(
                "{} -- {}",
                quote(
                    env.to_str()
                        .ok_or_else(|| t("The launcher path is not valid UTF-8"))?
                ),
                quote(program)
            ));
        }
        Ok(quote(program))
    }
    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        // Fresh startup must select its display again, and an updated portable
        // image must publish its own payload rather than inheriting an old cache.
        for name in ["CLIVUU_BINARY", "CLIVUU_LAYER_PRELOADED", "GDK_BACKEND"] {
            command.env_remove(name);
        }
        if self.portable {
            for name in [
                "CLIVUU_DESKTOP_DIR",
                "CLIVUU_DESKTOP_BINARY",
                "APPDIR",
                "APPIMAGE",
            ] {
                command.env_remove(name);
            }
        }
        if let Ok(preload) = std::env::var("LD_PRELOAD") {
            command.env(
                "LD_PRELOAD",
                crate::platform::display::without_layer_shell(&preload),
            );
        }
        command
    }
    /// Called only after GTK shutdown and dropping the old GApplication/bus owner.
    pub fn exec(self) -> std::io::Error {
        self.command().exec()
    }
}
fn quote(value: &str) -> String {
    let mut output = String::from("\"");
    for c in value.chars() {
        if matches!(c, '"' | '\\' | '$' | '`') {
            output.push('\\');
        }
        if c == '%' {
            output.push('%');
        }
        output.push(c);
    }
    output.push('"');
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_exec_quotes_one_literal_program_without_replaying_capture_arguments() {
        assert_eq!(
            quote("/Applications/Clivuu $`\"\\ 50%.AppImage"),
            "\"/Applications/Clivuu \\$\\`\\\"\\\\ 50%%.AppImage\""
        );
    }
    #[test]
    fn native_restart_uses_the_stable_entry_after_an_in_place_package_upgrade() {
        let root =
            std::env::temp_dir().join(format!("clivuu-launch-{}", gtk::glib::uuid_string_random()));
        std::fs::create_dir_all(root.join("usr/bin")).unwrap();
        std::fs::create_dir_all(root.join("usr/share/clivuu/desktop/bin")).unwrap();
        let target = root.join("usr/share/clivuu/desktop/bin/clivuu");
        std::fs::write(&target, "#!/bin/sh\nexit 11\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
        let entry = root.join("usr/bin/clivuu");
        std::os::unix::fs::symlink(&target, &entry).unwrap();
        let launch = Launch::at(entry.clone(), false).unwrap();
        let replacement = target.with_extension("new");
        std::fs::write(
            &replacement,
            "#!/bin/sh\n[ \"$#\" = 0 ] || exit 99\nexit 22\n",
        )
        .unwrap();
        std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::rename(replacement, target).unwrap();
        assert_eq!(launch.command().get_program(), entry.as_os_str());
        assert_eq!(launch.command().status().unwrap().code(), Some(22));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn portable_restarts_discard_cache_overrides_and_keep_the_original_program() {
        let launch = Launch {
            program: PathBuf::from("/Apps/Clivuu.AppImage"),
            portable: true,
        };
        let command = launch.command();
        assert_eq!(command.get_program(), "/Apps/Clivuu.AppImage");
        assert_eq!(command.get_args().count(), 0);
        for name in [
            "CLIVUU_DESKTOP_DIR",
            "CLIVUU_DESKTOP_BINARY",
            "APPIMAGE",
            "CLIVUU_LAYER_PRELOADED",
            "GDK_BACKEND",
        ] {
            assert!(
                command
                    .get_envs()
                    .any(|(key, value)| key == name && value.is_none())
            );
        }
    }
}
