//! Shared, GUI-independent INI settings. Only non-default JSON values are saved.
use crate::{
    Result,
    i18n::{t, tf},
};
use serde_json::{Map, Value};
use std::{
    env, fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

pub fn directory(variable: &str, fallback: &str) -> Result<PathBuf> {
    if let Some(path) = env::var_os(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(path.join("clivuu"));
    }
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| t("HOME must be an absolute path"))?;
    Ok(home.join(fallback).join("clivuu"))
}

pub fn path() -> Result<PathBuf> {
    Ok(directory("XDG_CONFIG_HOME", ".config")?.join("config.ini"))
}
pub fn schema() -> Map<String, Value> {
    let mut schema: Map<String, Value> =
        serde_json::from_str(include_str!("../config-defaults.json"))
            .expect("built-in configuration schema");
    schema["language"]["choices"] = Value::from(
        std::iter::once("system")
            .chain(crate::i18n::languages().map(|l| l.id()))
            .collect::<Vec<_>>(),
    );
    schema
}
pub fn default_image_quality() -> i32 {
    schema()["image-quality"]["default"]
        .as_i64()
        .expect("JPEG default") as i32
}
pub fn image_quality(values: &Map<String, Value>) -> i32 {
    values
        .get("image-quality")
        .and_then(Value::as_i64)
        .map(|v| v as i32)
        .unwrap_or_else(default_image_quality)
}
/// Accept ordinary RGB input while keeping stored color values canonical RGBA.
pub fn normalize(key: &str, mut value: Value) -> Result<Value> {
    let specs = schema();
    let rgba = |text: &str| {
        if text.len() == 7 && text.starts_with('#') {
            format!("{text}ff")
        } else {
            text.to_owned()
        }
    };
    if key == "pin-shadow-tint" && value.as_str() == Some("") {
        value = specs[key]["default"].clone();
    }
    if let Some(spec) = specs.get(key) {
        if spec["color"] == true
            && spec["alpha"] == true
            && let Some(text) = value.as_str()
        {
            value = Value::from(rgba(text));
        } else if spec["color_items"] == true
            && let Some(colors) = value.as_array_mut()
        {
            for color in colors {
                if let Some(text) = color.as_str() {
                    *color = Value::from(rgba(text));
                }
            }
        }
    }
    validate(key, &value)?;
    Ok(value)
}
const SECTIONS: &[&str] = &[
    "general",
    "capture.display",
    "capture.behavior",
    "pins",
    "pins.display",
    "pins.behavior",
    "pins.conversion",
    "output",
    "save.image",
    "save.video",
    "recording",
    "scrolling",
    "ocr",
    "shortcuts",
    "control.mouse",
    "annotation",
];
pub fn validate(key: &str, value: &Value) -> Result<()> {
    let spec = schema()
        .remove(key)
        .ok_or_else(|| tf("Unknown configuration key: {0}", &[key]))?;
    let default = &spec["default"];
    let valid = if default.is_string() {
        value.is_string()
    } else if default.is_boolean() {
        value.is_boolean()
    } else if default.is_array() {
        value.as_array().is_some_and(|a| {
            a.len() <= spec["max_items"].as_u64().unwrap_or(8) as usize
                && a.iter().all(|v| {
                    v.as_str().is_some_and(|s| {
                        s.len() <= 256
                            && !s.contains(['\n', '\r', '\0'])
                            && (spec["color_items"] != true
                                || (matches!(s.len(), 7 | 9)
                                    && s.starts_with('#')
                                    && s[1..].bytes().all(|b| b.is_ascii_hexdigit())))
                    })
                })
        })
    } else {
        value.as_i64().is_some_and(|n| {
            n >= spec["min"].as_i64().unwrap() && n <= spec["max"].as_i64().unwrap()
        })
    };
    let text = value.as_str().unwrap_or("");
    if spec["mouse_binding"] == true {
        let valid = crate::pins::mouse::Binding::parse(text).is_some_and(|binding| {
            crate::pins::mouse::Action::ALL
                .into_iter()
                .find(|action| action.key() == key)
                .is_some_and(|action| action.accepts(binding))
        });
        if !valid {
            return Err(tf("Invalid value for configuration key: {0}", &[key]).into());
        }
    }
    if !valid
        || text.len() > 4096
        || text.contains(['\0', '\n', '\r'])
        || spec["choices"]
            .as_array()
            .is_some_and(|a| !a.contains(value))
        || (spec["absolute"] == true && !text.is_empty() && !Path::new(text).is_absolute())
        || (spec["color"] == true
            && !(spec["optional"] == true && text.is_empty())
            && !(text.len() == if spec["alpha"] == true { 9 } else { 7 }
                && text.starts_with('#')
                && text[1..].bytes().all(|b| b.is_ascii_hexdigit())))
    {
        return Err(tf("Invalid value for configuration key: {0}", &[key]).into());
    }
    if let Some(media) = spec["filename"].as_str() {
        let render = if key.ends_with("-manual-name") {
            crate::saving::render_name
        } else {
            crate::saving::render
        };
        render(
            text,
            &crate::saving::NameContext::example(),
            if media == "image" {
                crate::saving::Media::Image
            } else {
                crate::saving::Media::Video
            },
        )?;
    }
    Ok(())
}

pub fn parse(contents: &str) -> Result<Map<String, Value>> {
    let specs = schema();
    let mut values: Map<String, Value> = specs
        .iter()
        .map(|(k, v)| (k.clone(), v["default"].clone()))
        .collect();
    let mut legacy_colors = Map::new();
    let mut legacy_mask_opacity = None;
    let mut legacy_mask_color = false;
    let mut section = "";
    let mut seen = std::collections::HashSet::new();
    for line in contents
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(['#', ';']))
    {
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            // Read the former flat layout, but always write the grouped layout.
            if name != "clivuu" && !SECTIONS.contains(&name) {
                return Err(t("Invalid Clivuu configuration file").into());
            }
            section = name;
            continue;
        }
        if section.is_empty() || line.starts_with('[') {
            return Err(t("Invalid Clivuu configuration file").into());
        }
        let (key, raw) = line
            .split_once('=')
            .ok_or_else(|| t("Invalid Clivuu configuration file"))?;
        let key = key.trim();
        if !seen.insert(key) {
            return Err(t("Duplicate configuration key").into());
        }
        let mut value: Value = serde_json::from_str(raw.trim())?;
        if key == "capture-mask-opacity" {
            if (section != "capture.display" && section != "clivuu")
                || value.as_u64().is_none_or(|n| n > 100)
            {
                return Err(t("Invalid Clivuu configuration file").into());
            }
            legacy_mask_opacity = value.as_u64();
            continue;
        }
        if key == "capture-mask-color"
            && let Some(color) = value.as_str().filter(|color| color.len() == 7)
        {
            // RGB-only files used a separate opacity, whose default was 44%.
            value = Value::from(format!("{color}70"));
            legacy_mask_color = true;
        }
        // Retired video-copy overrides must not invalidate the remaining
        // configuration. They are never returned or written back.
        if matches!(
            key,
            "video-auto-save"
                | "video-auto-name"
                | "video-auto-directory"
                | "video-format"
                | "video-save-mode"
        ) {
            if section != "save.video" && section != "clivuu" {
                return Err(t("Invalid Clivuu configuration file").into());
            }
            continue;
        }
        if key == "image-quality" && value == -1 {
            value = Value::from(100);
        }
        let legacy_section = match key {
            "pin-active-color" | "pin-idle-color" => Some("pins"),
            "capture-border-color" => Some("capture.display"),
            _ => None,
        };
        if let Some(expected) = legacy_section {
            if let Some(color) = value.as_str().filter(|color| color.len() == 7) {
                value = Value::from(format!("{color}ff"));
            }
            validate("accent-color", &value)?;
            if section != "clivuu" && section != expected {
                return Err(t("Invalid Clivuu configuration file").into());
            }
            legacy_colors.insert(key.into(), value);
            continue;
        }
        if let Some(media) = specs.get(key).and_then(|s| s["filename"].as_str())
            && value.as_str()
                == Some(if media == "image" {
                    "Clivuu_$yyyy-MM-dd_HH-mm-ss$.png"
                } else {
                    "Clivuu_$yyyy-MM-dd_HH-mm-ss$.mp4"
                })
        {
            value = specs[key]["default"].clone();
        }
        value = normalize(key, value)?;
        if section != "clivuu"
            && specs[key]["section"] != section
            && !(key == "video-directory" && section == "output")
        {
            return Err(t("Invalid Clivuu configuration file").into());
        }
        values.insert(key.into(), value);
    }
    if !seen.contains("accent-color") {
        for key in ["pin-active-color", "capture-border-color", "pin-idle-color"] {
            if let Some(color) = legacy_colors.remove(key) {
                values.insert("accent-color".into(), color);
                break;
            }
        }
    }
    if let Some(percent) = legacy_mask_opacity
        && (legacy_mask_color || !seen.contains("capture-mask-color"))
    {
        let color = values["capture-mask-color"].as_str().unwrap();
        values.insert(
            "capture-mask-color".into(),
            Value::from(format!("{}{:02x}", &color[..7], (percent * 255 + 50) / 100)),
        );
    }
    Ok(values)
}

pub fn read_file(path: &Path) -> Result<Map<String, Value>> {
    match fs::read_to_string(path) {
        Ok(contents) if contents.len() <= 65536 => parse(&contents),
        Ok(_) => Err(t("The configuration file is too large").into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => parse(""),
        Err(e) => Err(e.into()),
    }
}
pub fn read() -> Result<Map<String, Value>> {
    read_file(&path()?)
}

pub fn set_file(path: &Path, key: &str, value: Value) -> Result<()> {
    apply_file(path, Map::from_iter([(key.into(), value)]))
}

pub fn apply_file(path: &Path, changes: Map<String, Value>) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let changes: Map<String, Value> = changes
        .into_iter()
        .map(|(key, value)| normalize(&key, value).map(|value| (key, value)))
        .collect::<Result<_>>()?;
    if changes.is_empty() {
        return Ok(());
    }
    let parent = path.parent().ok_or("configuration parent")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    // Keep the lock inode stable across atomic config replacements. Kernel
    // releases the advisory lock even if a CLI or settings process crashes.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(parent.join("config.lock"))?;
    lock.lock()?;
    let mut values = read_file(path)?;
    values.extend(changes);
    let specs = schema();
    let mut output =
        String::from("# Clivuu settings: non-default JSON values, grouped by function.\n");
    for section in SECTIONS {
        let overrides: Vec<_> = values
            .iter()
            .filter(|(key, value)| {
                specs[*key]["section"] == *section && **value != specs[*key]["default"]
            })
            .collect();
        if overrides.is_empty() {
            continue;
        }
        output.push_str(&format!("\n[{section}]\n"));
        for (key, value) in overrides {
            output.push_str(&format!("{key} = {value}\n"));
        }
    }
    if fs::read_to_string(path).ok().as_deref() == Some(&output) {
        return Ok(());
    }
    let temporary = parent.join(format!(".config-{}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(output.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub fn set(key: &str, raw: &str) -> Result<()> {
    let spec = schema()
        .remove(key)
        .ok_or_else(|| tf("Unknown configuration key: {0}", &[key]))?;
    let value = if spec["default"].is_string() && !raw.starts_with('"') {
        Value::String(raw.into())
    } else {
        serde_json::from_str(raw)?
    };
    set_file(&path()?, key, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rgba_colors_preserve_alpha_and_import_rgb_opacity_files() {
        assert_eq!(
            normalize("capture-mask-color", Value::from("#123456")).unwrap(),
            "#123456ff"
        );
        assert_eq!(
            normalize("accent-color", Value::from("#bd93f9")).unwrap(),
            "#bd93f9ff"
        );
        assert_eq!(
            normalize("custom-colors", Value::from(vec!["#123456", "#aabbcc80"])).unwrap(),
            Value::from(vec!["#123456ff", "#aabbcc80"])
        );
        let old = "[capture.display]\ncapture-mask-color=\"#123456\"\ncapture-mask-opacity=73\ncapture-anchor-color=\"#aabbcc\"\n[general]\naccent-color=\"#bd93f9\"\n";
        let values = parse(old).unwrap();
        assert_eq!(values["capture-mask-color"], "#123456ba");
        assert_eq!(values["capture-anchor-color"], "#aabbccff");
        assert_eq!(values["accent-color"], "#bd93f9ff");
        assert!(!values.contains_key("capture-mask-opacity"));
        assert_eq!(
            parse("[capture.display]\ncapture-mask-color=\"#12345601\"\ncapture-mask-opacity=73\n")
                .unwrap()["capture-mask-color"],
            "#12345601"
        );
        assert_eq!(
            parse("[capture.display]\ncapture-mask-opacity=100\n").unwrap()["capture-mask-color"],
            "#000000ff"
        );
        assert!(parse("[capture.display]\ncapture-mask-opacity=101\n").is_err());
        assert!(parse("[general]\ncapture-mask-opacity=30\n").is_err());
        for (key, spec) in schema().iter().filter(|(_, spec)| spec["color"] == true) {
            assert_eq!(spec["alpha"], true);
            for color in ["#12345600", "#12345680", "#123456ff"] {
                validate(key, &Value::from(color)).unwrap();
            }
            assert!(validate(key, &Value::from("#123456gg")).is_err());
        }
    }
    #[test]
    fn pin_shadow_default_and_empty_legacy_tint_are_independent_of_accent() {
        let values =
            parse("[general]\naccent-color=\"#ff000080\"\n[pins.display]\npin-shadow-tint=\"\"\n")
                .unwrap();
        assert_eq!(values["pin-shadow-tint"], "#909090ff");
        assert_eq!(
            normalize("pin-shadow-tint", Value::from("#12345680")).unwrap(),
            "#12345680"
        );
        assert_eq!(
            parse("").unwrap()["pin-shadow-tint"],
            values["pin-shadow-tint"]
        );
    }
    #[test]
    fn jpeg_quality_defaults_and_legacy_default_migrate() {
        assert_eq!(parse("").unwrap()["image-quality"], 100);
        assert_eq!(
            parse("[save.image]\nimage-quality=-1").unwrap()["image-quality"],
            100
        );
        assert!(validate("image-quality", &Value::from(-1)).is_err());
        for q in [0, 100] {
            validate("image-quality", &Value::from(q)).unwrap();
        }
        assert!(validate("image-quality", &Value::from(101)).is_err());
    }
    #[test]
    fn saving_legacy_directory_migrates_and_both_media_share_name_validation() {
        let dir = PathBuf::from(format!("/tmp/clivuu-save-config-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.ini");
        fs::write(&path, "[output]\nvideo-directory=\"/tmp/old-videos\"\n").unwrap();
        set_file(
            &path,
            "image-quick-name",
            Value::from("$yyyy$/$MM$/%username%_$uuid$_$unix$_$unix_ms$.png"),
        )
        .unwrap();
        set_file(
            &path,
            "video-quick-name",
            Value::from("$yyyy$/$MM$/%username%_$uuid$_$unix$_$unix_ms$.webm"),
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("[save.image]") && text.contains("[save.video]"));
        assert!(!text.contains("[output]"));
        assert_eq!(
            read_file(&path).unwrap()["video-directory"],
            "/tmp/old-videos"
        );
        for key in [
            "image-manual-name",
            "image-quick-name",
            "image-auto-name",
            "video-manual-name",
            "video-quick-name",
        ] {
            assert_eq!(parse("").unwrap()[key], "Clivuu_$yyyy-MM-dd_HH-mm-ss$");
            validate(key, &Value::from("shot_$yyyy$")).unwrap();
        }
        for media in ["image", "video"] {
            for mode in ["manual", "quick", "auto"] {
                assert!(
                    validate(&format!("{media}-{mode}-name"), &Value::from("../bad.png")).is_err()
                );
            }
        }
        assert_eq!(
            parse("[save.image]\nimage-manual-name=\"Clivuu_$yyyy-MM-dd_HH-mm-ss$.png\"\n[save.video]\nvideo-quick-name=\"Clivuu_$yyyy-MM-dd_HH-mm-ss$.mp4\"\n")
                .unwrap()["image-manual-name"],
            "Clivuu_$yyyy-MM-dd_HH-mm-ss$"
        );
        assert_eq!(
            parse("[save.video]\nvideo-quick-name=\"Clivuu_$yyyy-MM-dd_HH-mm-ss$.mp4\"\n").unwrap()
                ["video-quick-name"],
            "Clivuu_$yyyy-MM-dd_HH-mm-ss$"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn retired_video_copies_do_not_break_existing_settings_or_survive_a_write() {
        let root = PathBuf::from(format!(
            "/tmp/clivuu-video-retirement-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("config.ini");
        let old = "[save.video]\nvideo-format=\"gif\"\nvideo-save-mode=\"manual\"\nvideo-auto-save=true\nvideo-auto-name=\"backup_$uuid$.webm\"\nvideo-auto-directory=\"/tmp/video-backups\"\n[save.image]\nimage-auto-save=true\nimage-auto-directory=\"/tmp/image-backups\"\nimage-quality=91\n[general]\ntheme=\"dark\"\n";
        fs::write(&path, old).unwrap();
        let values = read_file(&path).unwrap();
        assert!(!values.contains_key("video-format"));
        assert_eq!(values["image-auto-save"], true);
        assert_eq!(values["image-quality"], 91);
        assert_eq!(values["theme"], "dark");
        for key in [
            "video-auto-save",
            "video-auto-name",
            "video-auto-directory",
            "video-format",
            "video-save-mode",
        ] {
            assert!(!values.contains_key(key));
            assert!(!schema().contains_key(key));
            assert!(validate(key, &Value::Bool(false)).is_err());
        }
        set_file(&path, "video-save-notify", Value::from(false)).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(!saved.contains("video-auto-"));
        let values = read_file(&path).unwrap();
        assert_eq!(values["video-save-notify"], false);
        assert_eq!(values["image-auto-directory"], "/tmp/image-backups");
        assert!(
            parse("[clivuu]\nvideo-auto-save=true\nimage-auto-save=true\n").unwrap()["image-auto-save"]
                == true
        );
        assert!(parse("[general]\nvideo-auto-save=true\n").is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_colors_migrate_without_breaking_strict_config() {
        let legacy = "[pins]\npin-active-color=\"#123456\"\npin-idle-color=\"#654321\"\n[capture.display]\ncapture-border-color=\"#aabbcc\"\n";
        assert_eq!(parse(legacy).unwrap()["accent-color"], "#123456ff");
        assert_eq!(
            parse(&format!("{legacy}[general]\naccent-color=\"#bd93f9\"\n")).unwrap()["accent-color"],
            "#bd93f9ff"
        );
        assert!(parse("[general]\npin-active-color=\"#123456\"").is_err());
        assert!(parse("[pins]\npin-active-color=\"bad\"").is_err());
        assert!(parse("[general]\naccent-colour=\"#123456\"").is_err());
        let dir = PathBuf::from(format!(
            "/tmp/clivuu-config-migration-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.ini");
        fs::write(&path, legacy).unwrap();
        set_file(&path, "theme", Value::from("dark")).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(
            !saved.contains("pin-active-color")
                && !saved.contains("pin-idle-color")
                && !saved.contains("capture-border-color")
        );
        assert_eq!(read_file(&path).unwrap()["accent-color"], "#123456ff");
        set_file(&path, "accent-color", Value::from("#bd93f9ff")).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("accent-color"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn custom_rgba_palette_is_bounded_and_reset_removes_override() {
        let dir = PathBuf::from(format!("/tmp/clivuu-palette-test-{}", std::process::id()));
        let path = dir.join("config.ini");
        let colors = Value::from(vec!["#aabbcc80"; 16]);
        set_file(&path, "custom-colors", colors.clone()).unwrap();
        assert_eq!(read_file(&path).unwrap()["custom-colors"], colors);
        assert!(fs::read_to_string(&path).unwrap().contains("[annotation]"));
        for invalid in [
            Value::from(vec!["#aabbcc"; 17]),
            Value::from(vec!["#aabbccxx"]),
            Value::from(vec!["#abc"]),
            Value::from(vec!["aabbcc"]),
        ] {
            assert!(validate("custom-colors", &invalid).is_err());
        }
        set_file(&path, "custom-colors", Value::from(Vec::<String>::new())).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("custom-colors") && !text.contains("[annotation]"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn strict_ini_roundtrip_and_concurrent_updates() {
        let dir = PathBuf::from(format!("/tmp/clivuu-config-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.ini");
        let mut workers = vec![];
        for (key, value) in [
            ("language", Value::from("en")),
            ("max-scroll-frames", Value::from(81)),
            ("video-directory", Value::from("/tmp/录像 #1")),
        ] {
            let path = path.clone();
            workers.push(std::thread::spawn(move || {
                set_file(&path, key, value).unwrap()
            }));
        }
        for w in workers {
            w.join().unwrap();
        }
        let values = read_file(&path).unwrap();
        assert_eq!(values["language"], "en");
        assert_eq!(values["max-scroll-frames"], 81);
        assert_eq!(values["video-directory"], "/tmp/录像 #1");
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[general]\nlanguage = \"en\""));
        assert!(contents.contains("[scrolling]\nmax-scroll-frames = 81"));
        assert!(contents.contains("[save.video]\nvideo-directory = "));
        assert!(!contents.contains("capture-hide-pins"));
        set_file(&path, "language", Value::from("system")).unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert!(!contents.contains("language"));
        assert!(!contents.contains("[general]"));
        assert_eq!(read_file(&path).unwrap()["language"], "system");
        // A legacy file is migrated on write, including redundant defaults.
        fs::write(&path, "[clivuu]\nlanguage=\"en\"\ncapture-hide-pins=true\n").unwrap();
        set_file(&path, "language", Value::from("system")).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains('['));
        assert!(
            SECTIONS.iter().all(|section| *section == "output"
                || schema().values().any(|v| v["section"] == *section))
        );
        assert!(
            schema()
                .values()
                .all(|v| SECTIONS.contains(&v["section"].as_str().unwrap()))
        );
        for s in [
            "[clivuu]\nlanguage=\"en\"\nlanguage=\"en\"",
            "[other]\nx=1",
            "[clivuu]\npin-storage=\"bad\"",
            "[clivuu]\nscroll-interval=1",
            "[clivuu]\ncapture-hide-pins=1",
            "[clivuu]\ncapture-border-width=0",
            "[general]\ncapture-hide-pins=false",
            "[general]\nlanguage=\"en\"\n[clivuu]\nlanguage=\"en\"",
        ] {
            assert!(parse(s).is_err());
        }
        assert_eq!(
            parse("[clivuu]\ncapture-hide-pins=false").unwrap()["capture-hide-pins"],
            false
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
