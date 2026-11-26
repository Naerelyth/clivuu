//! Shared UTF-8 catalogs; no gettext runtime or GUI library is linked.
use std::{
    collections::HashMap,
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Language(usize);
struct Catalog {
    id: &'static str,
    name: &'static str,
    aliases: &'static [&'static str],
    source: &'static str,
    messages: OnceLock<HashMap<String, String>>,
}
include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));
impl Language {
    pub fn id(self) -> &'static str {
        CATALOGS[self.0].id
    }
    pub fn native_name(self) -> &'static str {
        CATALOGS[self.0].name
    }
}
pub fn languages() -> impl ExactSizeIterator<Item = Language> {
    (0..CATALOGS.len()).map(Language)
}
// Zero means uninitialized; explicit preferences replace early worker fallbacks.
struct Locale(AtomicUsize);
impl Locale {
    const fn new() -> Self {
        Self(AtomicUsize::new(0))
    }
    fn initialize(&self, language: Language, explicit: bool) {
        if explicit {
            self.0.store(language.0 + 1, Ordering::Relaxed);
        } else {
            let _ =
                self.0
                    .compare_exchange(0, language.0 + 1, Ordering::Relaxed, Ordering::Relaxed);
        }
    }
    fn language(&self) -> Language {
        self.0
            .load(Ordering::Relaxed)
            .checked_sub(1)
            .map(Language)
            .unwrap_or(DEFAULT_LANGUAGE)
    }
}
static LANGUAGE: Locale = Locale::new();
fn resolve(preference: &str) -> Option<Language> {
    let base = preference.trim().split('.').next().unwrap_or("");
    if base.eq_ignore_ascii_case("C") || base.eq_ignore_ascii_case("POSIX") {
        return Some(DEFAULT_LANGUAGE);
    }
    resolve_tag(preference, &CATALOGS).map(Language)
}
fn resolve_tag(preference: &str, catalogs: &[Catalog]) -> Option<usize> {
    let mut tag = preference
        .trim()
        .split(['.', '@'])
        .next()?
        .replace('_', "-");
    loop {
        if let Some(index) = catalogs.iter().position(|c| {
            c.id.eq_ignore_ascii_case(&tag)
                || c.aliases.iter().any(|a| a.eq_ignore_ascii_case(&tag))
        }) {
            return Some(index);
        }
        let end = tag.rfind('-')?;
        tag.truncate(end);
    }
}
pub fn language_for(preference: &str) -> Language {
    preference
        .split(':')
        .find_map(resolve)
        .unwrap_or(DEFAULT_LANGUAGE)
}

pub fn initialize(preference: Option<&str>) {
    if preference.is_none() && LANGUAGE.0.load(Ordering::Relaxed) != 0 {
        return;
    }
    let selected = std::env::var("CLIVUU_LANGUAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| preference.map(str::to_owned));
    let language = if let Some(selected) = selected.filter(|s| s != "system") {
        language_for(&selected)
    } else {
        let values = ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
            .map(|key| std::env::var(key).unwrap_or_default());
        system_language(values.each_ref().map(String::as_str))
    };
    LANGUAGE.initialize(language, preference.is_some());
}

pub fn translate_for(language: Language, message: &str) -> String {
    let lookup = |language: Language| {
        CATALOGS[language.0]
            .messages
            .get_or_init(|| {
                serde_json::from_str(CATALOGS[language.0].source)
                    .expect("validated built-in translation catalog")
            })
            .get(message)
    };
    lookup(language)
        .or_else(|| lookup(DEFAULT_LANGUAGE))
        .map_or(message, String::as_str)
        .to_owned()
}

/// Translate desktop actions using the process's system locale, independently
/// of the configured application language and CLIVUU_LANGUAGE.
pub fn translate_system(message: &str) -> String {
    let values = ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .map(|key| std::env::var(key).unwrap_or_default());
    translate_for(
        system_language(values.each_ref().map(String::as_str)),
        message,
    )
}

fn system_language([languages, all, messages, lang]: [&str; 4]) -> Language {
    let locale = [all, messages, lang]
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or("C");
    if matches!(locale.split('.').next(), Some("C" | "POSIX")) {
        return DEFAULT_LANGUAGE;
    }
    language_for(if languages.is_empty() {
        locale
    } else {
        languages
    })
}

pub fn current_language() -> Language {
    initialize(None);
    LANGUAGE.language()
}

pub fn t(message: &str) -> String {
    initialize(None);
    translate_for(LANGUAGE.language(), message)
}

pub fn tf(message: &str, values: &[&str]) -> String {
    interpolate(&t(message), values)
}

fn interpolate(template: &str, values: &[&str]) -> String {
    let mut output = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        output.push_str(&rest[..open]);
        rest = &rest[open..];
        if let Some(close) = rest.find('}')
            && let Ok(index) = rest[1..close].parse::<usize>()
            && let Some(value) = values.get(index)
        {
            output.push_str(value);
            rest = &rest[close + 1..];
        } else {
            output.push('{');
            rest = &rest[1..];
        }
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_actions_follow_locale_precedence_and_available_translations() {
        for (values, expected) in [
            (["", "", "", "zh_CN.UTF-8"], "在文件管理器中显示"),
            (
                ["", "en_US.UTF-8", "zh_CN.UTF-8", "zh_CN.UTF-8"],
                "Show in Files",
            ),
            (["", "", "zh_CN.UTF-8", "en_US.UTF-8"], "在文件管理器中显示"),
            (
                ["zz_ZZ:zh_CN:en", "", "", "zz_ZZ.UTF-8"],
                "在文件管理器中显示",
            ),
            (["en:zh_CN", "", "", "zh_CN.UTF-8"], "Show in Files"),
            (["zh_CN", "C", "", "zh_CN.UTF-8"], "Show in Files"),
            (["zh_CN", "C.UTF-8", "", "zh_CN.UTF-8"], "Show in Files"),
            (["zh_CN", "POSIX", "", "en_US.UTF-8"], "Show in Files"),
            (["", "", "", "zz_ZZ.UTF-8"], "Show in Files"),
            (["", "", "", ""], "Show in Files"),
        ] {
            let system = system_language(values);
            let app = Locale::new();
            for override_language in languages() {
                app.initialize(override_language, true);
                assert_eq!(translate_for(system, "Show in Files"), expected);
            }
        }
    }
    #[test]
    fn language_fallback_and_shared_catalog() {
        assert_eq!(language_for("zh_CN.UTF-8"), language_for("zh-CN"));
        assert_eq!(language_for("en_GB.UTF-8"), language_for("en"));
        assert_eq!(language_for("zz_ZZ.UTF-8"), language_for("en"));
        assert_eq!(
            translate_for(language_for("zh-CN"), "Copy image"),
            "复制图片"
        );
        assert_eq!(
            translate_for(language_for("en"), "Copy image"),
            "Copy image"
        );
    }
    #[test]
    fn configured_locale_replaces_early_errors_and_fallback_cannot_reset_it() {
        let locale = Locale::new();
        locale.initialize(language_for("en"), false);
        assert_eq!(translate_for(locale.language(), "Copy image"), "Copy image");
        locale.initialize(language_for("zh-CN"), true);
        locale.initialize(language_for("en"), false);
        assert_eq!(translate_for(locale.language(), "Copy image"), "复制图片");
        locale.initialize(language_for("en"), true);
        assert_eq!(locale.language(), language_for("en"));
    }
    #[test]
    fn registered_languages_drive_locale_tags_and_configuration() {
        let choices = crate::config::schema()["language"]["choices"].clone();
        for language in languages() {
            assert!(!language.native_name().is_empty());
            assert!(
                choices
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == language.id())
            );
            let locale = format!(
                "{}.UTF-8",
                language.id().replace('-', "_").to_ascii_uppercase()
            );
            assert_eq!(language_for(&locale), language);
        }
        assert_eq!(language_for("unknown:C:zh-CN"), DEFAULT_LANGUAGE);
        assert!(crate::config::validate("language", &serde_json::json!("not-registered")).is_err());
    }
    #[test]
    fn locale_matching_accepts_a_new_catalog_without_language_branches() {
        let catalogs = [Catalog {
            id: "fr-FR",
            name: "Français",
            aliases: &["fr"],
            source: "{}",
            messages: OnceLock::new(),
        }];
        for tag in ["fr_FR.UTF-8", "fr-CA", "FR-fr", "fr"] {
            assert_eq!(resolve_tag(tag, &catalogs), Some(0));
        }
        assert_eq!(resolve_tag("zh-CN", &catalogs), None);
    }
    #[test]
    fn interpolation_preserves_literal_user_text() {
        assert_eq!(
            interpolate("{0}: {1}", &["file{1}.png", "error"]),
            "file{1}.png: error"
        );
    }
}
