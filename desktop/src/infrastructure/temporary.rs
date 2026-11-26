//! Private, operation-owned temporary data. Destruction does not block GTK.
use super::tasks::{TaskError, TaskPool};
use gtk::{gio, glib};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};
pub struct Directory(PathBuf);
impl Directory {
    pub async fn new(
        pool: &Rc<TaskPool>,
        prefix: &'static str,
        cancel: &gio::Cancellable,
    ) -> Result<Arc<Self>, String> {
        let task = pool
            .submit(move |token| {
                use std::os::unix::fs::DirBuilderExt;
                token.check()?;
                let path = PathBuf::from(format!(
                    "/tmp/clivuu-{prefix}-{}",
                    glib::uuid_string_random()
                ));
                std::fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&path)
                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                Ok(Arc::new(Self(path)))
            })
            .map_err(|e| e.to_string())?;
        gio::CancellableFuture::new(task.result(), cancel.clone())
            .await
            .map_err(|_| clivuu_core::i18n::t("Operation cancelled"))?
            .map_err(|e| e.to_string())
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
    pub async fn write(
        self: &Arc<Self>,
        pool: &Rc<TaskPool>,
        bytes: Vec<u8>,
        cancel: &gio::Cancellable,
    ) -> Result<PathBuf, String> {
        let directory = self.clone();
        let task = pool
            .submit(move |token| {
                use std::{io::Write, os::unix::fs::OpenOptionsExt};
                token.check()?;
                let path = directory
                    .0
                    .join(format!("{}.png", glib::uuid_string_random()));
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&path)
                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                file.write_all(&bytes)
                    .map_err(|e| TaskError::Failed(e.to_string()))?;
                token.check()?;
                Ok(path)
            })
            .map_err(|e| e.to_string())?;
        gio::CancellableFuture::new(task.result(), cancel.clone())
            .await
            .map_err(|_| clivuu_core::i18n::t("Operation cancelled"))?
            .map_err(|e| e.to_string())
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let path = self.0.clone();
        let _ = std::thread::Builder::new()
            .name("clivuu-file-cleanup".into())
            .spawn(move || {
                let _ = std::fs::remove_dir_all(path);
            });
    }
}
