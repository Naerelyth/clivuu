//! Shared save planning and atomic publication. Clock/directory lookup stays
//! here; encoding and blocking file operations run on the caller's task pool.
use clivuu_core::{
    config,
    i18n::t,
    saving::{self, DateParts, Media, NameContext},
};
use gtk::glib;
use serde_json::{Map, Value};
use std::{
    fs::{self, OpenOptions},
    io,
    os::unix::{ffi::OsStrExt, fs::DirBuilderExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy)]
pub enum Mode {
    Manual,
    Quick,
    Automatic,
}
pub fn now() -> Result<NameContext, String> {
    let micros = glib::real_time();
    let d =
        glib::DateTime::from_unix_local(micros.div_euclid(1_000_000)).map_err(|e| e.to_string())?;
    Ok(NameContext {
        date: DateParts {
            year: d.year(),
            month: d.month() as u32,
            day: d.day_of_month() as u32,
            weekday: d.day_of_week() as u32,
            hour: d.hour() as u32,
            minute: d.minute() as u32,
            second: d.second() as u32,
            millisecond: micros.rem_euclid(1_000_000) as u32 / 1000,
        },
        timezone: d.timezone_abbreviation().into(),
        uuid: glib::uuid_string_random().into(),
        unix_seconds: micros.div_euclid(1_000_000),
        unix_millis: micros.div_euclid(1000),
        os: std::env::consts::OS.into(),
        computer_name: glib::host_name().into(),
        user_name: glib::user_name().to_string_lossy().into(),
    })
}
pub fn prefix(media: Media) -> &'static str {
    match media {
        Media::Image => "image",
        Media::Video => "video",
    }
}
pub fn name(
    values: &Map<String, Value>,
    media: Media,
    mode: Mode,
    time: &NameContext,
) -> Result<String, String> {
    let mode = match mode {
        Mode::Manual => "manual",
        Mode::Quick => "quick",
        Mode::Automatic => "auto",
    };
    let key = format!("{}-{mode}-name", prefix(media));
    let render = if mode == "manual" {
        saving::render_name
    } else {
        saving::render
    };
    let mut name =
        render(values[&key].as_str().unwrap_or(""), time, media).map_err(|e| e.to_string())?;
    if media == Media::Image && mode == "manual" && values["image-remember-extension"] == true {
        name = Path::new(&name)
            .with_extension(values["image-last-extension"].as_str().unwrap_or("png"))
            .to_string_lossy()
            .into();
    }
    Ok(name)
}
pub fn directory(values: &Map<String, Value>, media: Media, mode: Mode) -> PathBuf {
    let key = match (media, mode) {
        (Media::Image, Mode::Automatic) => "image-auto-directory",
        (Media::Image, _) => "image-quick-directory",
        (Media::Video, _) => "video-directory",
    };
    if let Some(path) = values[key].as_str().filter(|s| !s.is_empty()) {
        return PathBuf::from(path);
    }
    if matches!(mode, Mode::Automatic) {
        return directory(values, media, Mode::Quick);
    }
    glib::user_special_dir(match media {
        Media::Image => glib::UserDirectory::Pictures,
        Media::Video => glib::UserDirectory::Videos,
    })
    .unwrap_or_else(|| {
        glib::home_dir().join(if media == Media::Image {
            "Pictures"
        } else {
            "Videos"
        })
    })
    .join("Clivuu")
}
pub fn target(
    values: &Map<String, Value>,
    media: Media,
    mode: Mode,
    time: &NameContext,
) -> Result<PathBuf, String> {
    Ok(directory(values, media, mode).join(name(values, media, mode, time)?))
}
pub fn section_keys(section: &str) -> Vec<String> {
    config::schema()
        .into_iter()
        .filter(|(_, v)| v["section"] == section)
        .map(|(k, _)| k)
        .collect()
}

pub(super) fn publish_file(source: &Path, destination: &Path) -> io::Result<()> {
    let source = std::ffi::CString::new(source.as_os_str().as_bytes())?;
    let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())?;
    // SAFETY: both paths remain owned and NUL-terminated; no-replace is atomic.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
struct Staging {
    directory: PathBuf,
    file: PathBuf,
}
impl Staging {
    fn new(target: &Path) -> io::Result<Self> {
        let parent = target
            .parent()
            .ok_or_else(|| io::Error::other(t("Cannot determine the save directory")))?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
        let directory = parent.join(format!(".clivuu-save-{}", glib::uuid_string_random()));
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let file = directory.join("complete");
        Ok(Self { directory, file })
    }
    fn commit(&self, target: &Path) -> io::Result<PathBuf> {
        for n in 0..u32::MAX {
            let path = saving::numbered(target, n);
            match publish_file(&self.file, &path) {
                Ok(()) => return Ok(path),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::other(t("No unused file name is available")))
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.file);
        let _ = fs::remove_dir(&self.directory);
    }
}
pub fn write_unique(png: &[u8], target: &Path, quality: i32) -> Result<PathBuf, String> {
    let stage = Staging::new(target).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage.file)
        .map_err(|e| e.to_string())?;
    clivuu_core::image::export::write(
        clivuu_core::image::export::Source::Png(png),
        target,
        quality,
        &mut file,
    )
    .map_err(|e| e.to_string())?;
    stage.commit(target).map_err(|e| e.to_string())
}
pub fn unused(target: &Path) -> Result<PathBuf, String> {
    (0..u32::MAX)
        .map(|n| saving::numbered(target, n))
        .find(|p| !p.exists())
        .ok_or_else(|| t("Cannot choose an unused file name"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_saves_create_folders_and_never_replace_existing_files() {
        let root = PathBuf::from(format!(
            "/tmp/clivuu-save-test-{}",
            glib::uuid_string_random()
        ));
        let target = root.join("nested/a.png");
        let first = write_unique(b"first", &target, 100).unwrap();
        let second = write_unique(b"second", &target, 100).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(first).unwrap(), b"first");
        assert_eq!(fs::read(&second).unwrap(), b"second");
        assert!(!root.join("nested").read_dir().unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".clivuu")
        }));
        fs::remove_dir_all(root).unwrap();
    }
}
