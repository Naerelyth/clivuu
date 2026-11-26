//! GUI-free client for the single native desktop service. Desktop-specific
//! capture and surface capabilities are selected inside that service.
use clivuu_core::{Result, i18n::t, protocol};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub fn execute(request: Value) -> Result<()> {
    let connection = zbus::blocking::connection::Builder::session()?
        .method_timeout(Duration::from_secs(7200))
        .build()?;
    let bus = zbus::blocking::fdo::DBusProxy::new(&connection)?;
    if !bus.name_has_owner(protocol::DESKTOP_DESTINATION.try_into()?)? {
        if request["command"] == "status" {
            println!(
                "{}",
                json!({"running":false,"protocol":protocol::VERSION,"backend":"rust"})
            );
            return Ok(());
        }
        let binary = desktop_binary()?;
        use std::os::unix::fs::OpenOptionsExt;
        let diagnostic = env::var_os("CLIVUU_DESKTOP_LOG")
            .map(|p| {
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .mode(0o600)
                    .open(p)
            })
            .transpose()?;
        let mut command = Command::new(binary);
        command
            .env("CLIVUU_BINARY", env::current_exe()?)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(diagnostic.map_or_else(Stdio::null, Stdio::from));
        let mut child = command.spawn()?;
        let mut running = false;
        for _ in 0..100 {
            if bus.name_has_owner(protocol::DESKTOP_DESTINATION.try_into()?)? {
                running = true;
                break;
            }
            if child.try_wait()?.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if !running {
            return Err(t("The desktop service did not start; set CLIVUU_DESKTOP_LOG for diagnostics and check GTK4, libadwaita and the desktop surface backend").into());
        }
    }
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        protocol::DESKTOP_DESTINATION,
        protocol::OBJECT_PATH,
        protocol::INTERFACE,
    )?;
    // GApplication owns the bus name before its startup handler exports
    // Desktop1. Probe only the read-only status method during that interval;
    // retrying the requested operation could duplicate a capture or import.
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = json!({"protocol":protocol::VERSION,"command":"status"}).to_string();
    let ready: String = loop {
        match proxy.call("Execute", &status) {
            Ok(reply) => break reply,
            Err(zbus::Error::MethodError(name, _, _))
                if matches!(
                    name.as_str(),
                    "org.freedesktop.DBus.Error.UnknownMethod"
                        | "org.freedesktop.DBus.Error.UnknownObject"
                ) && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    };
    if request["command"] == "status" {
        println!("{ready}");
        return Ok(());
    }
    let reply: String = proxy.call("Execute", &(request.to_string(),))?;
    println!("{reply}");
    Ok(())
}
fn candidates(executable: &Path, directory: Option<PathBuf>) -> Vec<PathBuf> {
    let mut paths = vec![];
    if let Some(directory) = directory {
        paths.push(directory.join("bin/clivuu-desktop"));
    }
    if let Some(parent) = executable.parent() {
        paths.push(parent.join("clivuu-desktop"));
        if let Some(prefix) = parent.parent() {
            paths.push(prefix.join("share/clivuu/desktop/bin/clivuu-desktop"));
            paths.push(prefix.join("libexec/clivuu-desktop"));
        }
    }
    paths
}
fn desktop_binary() -> Result<PathBuf> {
    if let Some(path) = env::var_os("CLIVUU_DESKTOP_BINARY") {
        return Ok(PathBuf::from(path));
    }
    candidates(&env::current_exe()?,env::var_os("CLIVUU_DESKTOP_DIR").map(PathBuf::from)).into_iter().find(|p|p.is_file()).ok_or_else(||t("Desktop components were not found; install the complete package or set CLIVUU_DESKTOP_DIR").into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_bundled_and_system_native_services_without_script_runtime() {
        let paths = candidates(
            Path::new("/usr/bin/clivuu"),
            Some("/tmp/private-package".into()),
        );
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/tmp/private-package/bin/clivuu-desktop"),
                "/usr/bin/clivuu-desktop".into(),
                "/usr/share/clivuu/desktop/bin/clivuu-desktop".into(),
                "/usr/libexec/clivuu-desktop".into()
            ]
        );
    }
}
