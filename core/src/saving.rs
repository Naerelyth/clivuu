//! File-name templates shared by configuration, preview and
//! publication. No desktop, clock, or GUI dependency is needed here.
use crate::{Result, i18n::t};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    Image,
    Video,
}
#[derive(Clone, Copy, Debug)]
pub struct DateParts {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millisecond: u32,
}
impl DateParts {
    pub const EXAMPLE: Self = Self {
        year: 2026,
        month: 11,
        day: 5,
        weekday: 4,
        hour: 20,
        minute: 22,
        second: 56,
        millisecond: 123,
    };
}

#[derive(Clone, Debug)]
pub struct NameContext {
    pub date: DateParts,
    pub timezone: String,
    pub uuid: String,
    pub unix_seconds: i64,
    pub unix_millis: i64,
    pub os: String,
    pub computer_name: String,
    pub user_name: String,
}
impl NameContext {
    pub fn example() -> Self {
        Self {
            date: DateParts::EXAMPLE,
            timezone: "CST".into(),
            uuid: "6a8361b1-7343-42da-b5d8-24617167a0c6".into(),
            unix_seconds: 1793881376,
            unix_millis: 1793881376123,
            os: "Linux".into(),
            computer_name: "computer".into(),
            user_name: "user".into(),
        }
    }
}

fn date(format: &str, context: &NameContext) -> Result<String> {
    let time = context.date;
    let months = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let days = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    let month = months[(time.month.saturating_sub(1) as usize).min(11)];
    let day = days[(time.weekday.saturating_sub(1) as usize).min(6)];
    let mut rest = format;
    let mut output = String::new();
    while !rest.is_empty() {
        if let Some(quoted) = rest.strip_prefix('\'') {
            let end = quoted
                .find('\'')
                .ok_or_else(|| t("Unclosed quote in file name template"))?;
            output.push_str(&quoted[..end]);
            rest = &quoted[end + 1..];
            continue;
        }
        let matched = [
            ("yyyy", format!("{:04}", time.year)),
            ("yy", format!("{:02}", time.year.rem_euclid(100))),
            ("MMMM", month.to_owned()),
            ("MMM", month[..3].to_owned()),
            ("MM", format!("{:02}", time.month)),
            ("M", time.month.to_string()),
            ("dddd", day.to_owned()),
            ("ddd", day[..3].to_owned()),
            ("dd", format!("{:02}", time.day)),
            ("d", time.day.to_string()),
            ("HH", format!("{:02}", time.hour)),
            ("H", time.hour.to_string()),
            ("hh", format!("{:02}", time.hour)),
            ("h", time.hour.to_string()),
            ("mm", format!("{:02}", time.minute)),
            ("m", time.minute.to_string()),
            ("ss", format!("{:02}", time.second)),
            ("s", time.second.to_string()),
            ("zzz", format!("{:03}", time.millisecond)),
            ("z", time.millisecond.to_string()),
            ("t", context.timezone.clone()),
        ]
        .into_iter()
        .find(|(token, _)| rest.starts_with(token));
        if let Some((token, value)) = matched {
            output.push_str(&value);
            rest = &rest[token.len()..];
        } else {
            let c = rest.chars().next().unwrap();
            if c.is_ascii_alphabetic() {
                return Err(t("Unknown date token in file name template").into());
            }
            output.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    Ok(output)
}

/// `$yyyy-MM-dd_HH-mm-ss$` expands local date/time. `$$` is a literal `$`.
/// Relative subfolders are supported; absolute paths and parent traversal are rejected.
pub fn render(template: &str, context: &NameContext, media: Media) -> Result<String> {
    let mut rest = template;
    let mut output = String::new();
    while let Some(start) = rest.find('$') {
        output.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let end = rest
            .find('$')
            .ok_or_else(|| t("Unclosed date expression in file name template"))?;
        match &rest[..end] {
            "" => output.push('$'),
            "uuid" => output.push_str(&context.uuid),
            "unix" => output.push_str(&context.unix_seconds.to_string()),
            "unix_ms" => output.push_str(&context.unix_millis.to_string()),
            format => output.push_str(&date(format, context)?),
        }
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    let mut expanded = String::new();
    let mut tail = output.as_str();
    while let Some(start) = tail.find('%') {
        expanded.push_str(&tail[..start]);
        tail = &tail[start + 1..];
        let end = tail
            .find('%')
            .ok_or_else(|| t("Unclosed variable in file name template"))?;
        let value = match &tail[..end] {
            "" => "%",
            "os" => &context.os,
            "computername" => &context.computer_name,
            "username" => &context.user_name,
            _ => return Err(t("Unknown variable in file name template").into()),
        };
        expanded.extend(value.chars().map(|c| {
            if c.is_control() || matches!(c, '/' | '\\') {
                '_'
            } else {
                c
            }
        }));
        tail = &tail[end + 1..];
    }
    expanded.push_str(tail);
    let mut output = expanded;
    if output.is_empty()
        || output.len() > 4096
        || output.chars().any(|c| c.is_control() || c == '\\')
        || Path::new(&output)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        || output
            .split('/')
            .any(|part| part.is_empty() || part.len() > 240)
    {
        return Err(t("Use a relative path without '..' (up to 240 bytes per component)").into());
    }
    let path = Path::new(&output);
    let hidden_stem = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'));
    if path.extension().is_none() && !hidden_stem {
        let default_ext = match media {
            Media::Image => "png",
            Media::Video => "mp4",
        };
        output = format!("{output}.{default_ext}");
    } else {
        extension(path, media)?;
    }
    if Path::new(&output)
        .file_name()
        .and_then(|n| n.to_str())
        .is_none_or(|n| n.len() > 240)
    {
        return Err(t("Use a relative path without '..' (up to 240 bytes per component)").into());
    }
    Ok(output)
}

pub fn render_name(template: &str, context: &NameContext, media: Media) -> Result<String> {
    let name = render(template, context, media)?;
    if name.contains('/') {
        return Err(t("Manual save templates must contain only a file name").into());
    }
    Ok(name)
}

pub fn extension(path: &Path, media: Media) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if match media {
        Media::Image => crate::image::export::Format::from_path(path).is_ok(),
        Media::Video => crate::recording::Format::from_path(path).is_some(),
    } {
        Ok(ext)
    } else {
        Err(t(match media {
            Media::Image => "Choose a PNG or JPEG file name",
            Media::Video => "Choose a WebM, MP4 or GIF file name",
        })
        .into())
    }
}

pub fn numbered(path: &Path, number: u32) -> PathBuf {
    if number == 0 {
        return path.to_owned();
    }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{stem} ({number}).{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn templates_are_identical_for_preview_and_files_and_cannot_traverse() {
        assert_eq!(
            render(
                "Clivuu_$yyyy-MM-dd_HH-mm-ss$",
                &NameContext::example(),
                Media::Image
            )
            .unwrap(),
            "Clivuu_2026-11-05_20-22-56.png"
        );
        assert_eq!(
            render(
                "Clivuu_$yyyy-MM-dd_HH-mm-ss$",
                &NameContext::example(),
                Media::Video
            )
            .unwrap(),
            "Clivuu_2026-11-05_20-22-56.mp4"
        );
        assert_eq!(
            render(
                "Clivuu_$yyyy-MM-dd_HH-mm-ss$.png",
                &NameContext::example(),
                Media::Image
            )
            .unwrap(),
            "Clivuu_2026-11-05_20-22-56.png"
        );
        assert_eq!(
            render(
                "视频_$yy-M-d_HH-mm-ss-zzz$.webm",
                &NameContext::example(),
                Media::Video
            )
            .unwrap(),
            "视频_26-11-5_20-22-56-123.webm"
        );
        assert_eq!(
            render("$$$yyyy$.jpg", &NameContext::example(), Media::Image).unwrap(),
            "$2026.jpg"
        );
        assert_eq!(
            render(
                "$yyyy$/$MM$/Clivuu_$yyyy-MM-dd_HH-mm-ss$",
                &NameContext::example(),
                Media::Image
            )
            .unwrap(),
            "2026/11/Clivuu_2026-11-05_20-22-56.png"
        );
        for name in [
            "../$yyyy$",
            "/$yyyy$",
            "$yyyy",
            "$unknown$",
            "x.gif",
            "x.",
            ".png",
            ".hidden",
            "a\n",
        ] {
            assert!(
                render(name, &NameContext::example(), Media::Image).is_err(),
                "{name}"
            );
        }
        assert!(render("x.png", &NameContext::example(), Media::Video).is_err());
        assert_eq!(
            render(
                "$dddd_ddd_MMMM_MMM_t$_%os%_%username%_$uuid$_$unix$_$unix_ms$",
                &NameContext::example(),
                Media::Image
            )
            .unwrap(),
            "Thursday_Thu_November_Nov_CST_Linux_user_6a8361b1-7343-42da-b5d8-24617167a0c6_1793881376_1793881376123.png"
        );
        assert_eq!(
            render(
                "$yyyy$/$MM$/Clivuu_$yyyy-MM-dd_HH-mm-ss$.png",
                &NameContext::example(),
                Media::Image
            )
            .unwrap(),
            "2026/11/Clivuu_2026-11-05_20-22-56.png"
        );
        assert!(render_name("$yyyy$/x", &NameContext::example(), Media::Image).is_err());
        let mut unsafe_value = NameContext::example();
        unsafe_value.user_name = "../user\n".into();
        assert_eq!(
            render("%username%.png", &unsafe_value, Media::Image).unwrap(),
            ".._user_.png"
        );
        assert_eq!(
            numbered(Path::new("/tmp/截图.png"), 2),
            Path::new("/tmp/截图 (2).png")
        );
    }
}
