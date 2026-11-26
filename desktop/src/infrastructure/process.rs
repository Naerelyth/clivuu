//! Cancellable subprocess IO without a shell, synchronous waits or UI workers.
use clivuu_core::i18n::t;
use gtk::{gio, glib, prelude::*};
struct Child {
    process: gio::Subprocess,
    complete: bool,
}
impl Drop for Child {
    fn drop(&mut self) {
        if !self.complete {
            self.process.force_exit();
        }
    }
}
pub async fn output(argv: &[&str], cancel: &gio::Cancellable) -> Result<String, String> {
    let process = gio::Subprocess::newv(
        &argv.iter().map(std::ffi::OsStr::new).collect::<Vec<_>>(),
        gio::SubprocessFlags::STDOUT_PIPE,
    )
    .map_err(|e| e.to_string())?;
    read_output(process, cancel).await
}

pub async fn read_output(
    process: gio::Subprocess,
    cancel: &gio::Cancellable,
) -> Result<String, String> {
    let mut child = Child {
        process,
        complete: false,
    };
    gio::CancellableFuture::new(
        async {
            let stream = child.process.stdout_pipe().unwrap();
            let mut bytes = Vec::new();
            loop {
                let block = stream
                    .read_bytes_future(65536, glib::Priority::DEFAULT)
                    .await
                    .map_err(|e| e.to_string())?;
                if block.is_empty() {
                    break;
                }
                if bytes.len() + block.len() > 8 * 1024 * 1024 {
                    return Err(t("The request is too large"));
                }
                bytes.extend_from_slice(&block);
            }
            child
                .process
                .wait_check_future()
                .await
                .map_err(|e| e.to_string())?;
            child.complete = true;
            String::from_utf8(bytes).map_err(|e| e.to_string())
        },
        cancel.clone(),
    )
    .await
    .map_err(|_| t("Operation cancelled"))?
}
