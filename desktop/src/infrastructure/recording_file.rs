//! Encoder output stays private until a complete file is atomically published.
//! A failed recording deliberately retains its recoverable file.
use super::saving::publish_file;
use clivuu_core::i18n::t;
use gtk::glib;
use std::{
    fs::{self, OpenOptions},
    io,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

pub struct RecordingFile {
    pub path: PathBuf,
    pub destination: PathBuf,
    directory: PathBuf,
    limit: Option<u64>,
    replace: bool,
}
fn private_directory(parent: &Path) -> io::Result<PathBuf> {
    let path = parent.join(format!(".clivuu-record-{}", glib::uuid_string_random()));
    fs::DirBuilder::new().mode(0o700).create(&path)?;
    Ok(path)
}
impl RecordingFile {
    pub fn prepare(
        destination: PathBuf,
        memory: bool,
        limit_mib: u64,
        replace: bool,
    ) -> Result<Self, String> {
        if !destination.is_absolute() {
            return Err(t("The recording output must be an absolute path"));
        }
        if destination.exists() && !replace {
            return Err(t("The recording file already exists"));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| t("Cannot create the recording directory"))?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|e| e.to_string())?;
        let directory = private_directory(if memory { Path::new("/tmp") } else { parent })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            path: directory.join("capture.mkv"),
            destination,
            directory,
            replace,
            limit: memory.then_some(limit_mib.clamp(16, 32768) * 1024 * 1024),
        })
    }
    pub fn space_notice(&self) -> Result<Option<String>, String> {
        let size = match fs::metadata(&self.path) {
            Ok(info) => info.len(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        use gtk::{gio, prelude::*};
        let free = gio::File::for_path(&self.directory)
            .query_filesystem_info("filesystem::free", gio::Cancellable::NONE)
            .map_err(|e| e.to_string())?
            .attribute_uint64("filesystem::free");
        Ok(
            (self.limit.is_some_and(|limit| size >= limit - (1024 * 1024).min(limit / 8)) || free < 16 * 1024 * 1024)
                .then(|| t("Temporary recording limit reached, recording stopped with the material preserved")),
        )
    }
    pub fn export_space(&self, encoded: &Path) -> Result<(), String> {
        // The acquisition ceiling is not an export budget. Encoding uses the
        // destination volume and only that volume's free space constrains it.
        let parent = encoded
            .parent()
            .ok_or_else(|| t("Cannot create the recording directory"))?;
        use gtk::{gio, prelude::*};
        let free = gio::File::for_path(parent)
            .query_filesystem_info("filesystem::free", gio::Cancellable::NONE)
            .map_err(|e| e.to_string())?
            .attribute_uint64("filesystem::free");
        if free < 16 * 1024 * 1024 {
            return Err(t("Insufficient temporary space to export the recording"));
        }
        Ok(())
    }
    /// Explicit user discard only, failure paths keep their recoverable bytes.
    pub fn discard(&self) -> Result<(), String> {
        for path in [&self.path, &self.path.with_file_name("material.json")] {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    }

    pub fn publish(&self) -> Result<PathBuf, String> {
        self.publish_source(&self.path)
    }
    pub fn publish_source(&self, encoded: &Path) -> Result<PathBuf, String> {
        let path = self.publish_export(encoded, &self.destination, self.replace)?;
        let _ = fs::remove_file(&self.path);
        Ok(path)
    }
    /// Commit an export without consuming the editable source material.
    pub fn publish_export(
        &self,
        encoded: &Path,
        destination: &Path,
        replace: bool,
    ) -> Result<PathBuf, String> {
        if !destination.is_absolute() || destination == self.path {
            return Err(t("The recording output must be an absolute path"));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| t("Cannot create the recording directory"))?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let source = OpenOptions::new()
            .read(true)
            .open(encoded)
            .map_err(|e| e.to_string())?;
        source.sync_all().map_err(|e| e.to_string())?;
        // Linux no-replace rename works on FAT/exFAT too; hard links do not.
        let commit = |source: &Path| {
            if replace {
                fs::rename(source, destination)
            } else {
                publish_file(source, destination)
            }
        };
        if let Err(error) = commit(encoded) {
            if error.raw_os_error() != Some(libc::EXDEV) {
                return Err(error.to_string());
            }
            let staging = private_directory(parent).map_err(|e| e.to_string())?;
            let result = (|| -> io::Result<()> {
                let path = staging.join("capture.webm");
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?;
                let mut input = source;
                io::copy(&mut input, &mut output)?;
                output.sync_all()?;
                commit(&path)
            })();
            let _ = fs::remove_file(staging.join("capture.webm"));
            let _ = fs::remove_dir(&staging);
            result.map_err(|e| e.to_string())?;
        }
        // Once committed, a directory-sync failure must not remove the video.
        if let Ok(parent) = fs::File::open(parent) {
            let _ = parent.sync_all();
        }
        let _ = fs::remove_file(encoded);
        Ok(destination.to_owned())
    }
}
impl Drop for RecordingFile {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_capture_budget_does_not_consume_destination_export_space() {
        let root = private_directory(Path::new("/tmp")).unwrap();
        let output = RecordingFile::prepare(root.join("saved.mp4"), true, 16, false).unwrap();
        fs::File::create(&output.path)
            .unwrap()
            .set_len(16 * 1024 * 1024)
            .unwrap();
        assert!(output.space_notice().unwrap().is_some());
        let scratch = root.join("export.mp4");
        fs::File::create(&scratch)
            .unwrap()
            .set_len(32 * 1024 * 1024)
            .unwrap();
        output.export_space(&scratch).unwrap();
        output.discard().unwrap();
        drop(output);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn encoded_publication_and_explicit_discard_keep_the_approved_destination_safe() {
        let root = private_directory(Path::new("/tmp")).unwrap();
        let destination = root.join("video.mp4");
        let output = RecordingFile::prepare(destination.clone(), false, 16, false).unwrap();
        fs::write(&output.path, b"private webm").unwrap();
        let encoded = output.path.with_file_name("export.mp4");
        fs::write(&encoded, b"encoded mp4").unwrap();
        output.publish_source(&encoded).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"encoded mp4");
        assert!(!encoded.exists() && !output.path.exists());
        let staging = output.directory.clone();
        drop(output);
        assert!(!staging.exists());
        let abandoned = RecordingFile::prepare(destination.clone(), false, 16, true).unwrap();
        fs::write(&abandoned.path, b"unwanted capture").unwrap();
        let staging = abandoned.directory.clone();
        abandoned.discard().unwrap();
        drop(abandoned);
        assert!(!staging.exists());
        assert_eq!(fs::read(&destination).unwrap(), b"encoded mp4");
        fs::remove_file(destination).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn publish_is_complete_and_never_replaces_an_existing_file() {
        let directory = private_directory(Path::new("/tmp")).unwrap();
        let dest = directory.join("video.webm");
        let output = RecordingFile::prepare(dest.clone(), true, 16, false).unwrap();
        fs::write(&output.path, b"complete video").unwrap();
        output.publish().unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"complete video");
        assert!(!output.path.exists());
        assert!(RecordingFile::prepare(dest.clone(), false, 16, false).is_err());
        drop(output);
        fs::remove_file(dest).unwrap();
        fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn approved_manual_replace_keeps_old_video_until_commit() {
        let directory = private_directory(Path::new("/tmp")).unwrap();
        let dest = directory.join("video.webm");
        fs::write(&dest, b"old").unwrap();
        let output = RecordingFile::prepare(dest.clone(), true, 16, true).unwrap();
        fs::write(&output.path, b"new complete").unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"old");
        output.publish().unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"new complete");
        drop(output);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn failure_keeps_recoverable_data_and_destination_races_are_safe() {
        let directory = private_directory(Path::new("/tmp")).unwrap();
        let dest = directory.join("race.webm");
        let output = RecordingFile::prepare(dest.clone(), false, 16, false).unwrap();
        fs::write(&output.path, b"recover me").unwrap();
        fs::write(&dest, b"other file").unwrap();
        assert!(output.publish().is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"other file");
        let recover = output.path.clone();
        let staging = output.directory.clone();
        drop(output);
        assert_eq!(fs::read(&recover).unwrap(), b"recover me");
        fs::remove_file(recover).unwrap();
        fs::remove_dir(staging).unwrap();
        fs::remove_file(dest).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
