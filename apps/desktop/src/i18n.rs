//! Interface translation, as in the web app: English text is the key,
//! `t("Import…")` returns the active language's entry or the English when
//! there is none. Catalogues are JSON maps from English to the translation
//! under `apps/desktop/locales/<locale>/`, compiled in. The language is
//! `"locale"` in `~/.opencut/settings.json` (`auto` follows `LANG`,
//! `LC_ALL` or `LANGUAGE`).

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Locale {
    En,
    ZhCn,
}

impl Locale {
    pub(crate) const ALL: [Locale; 2] = [Locale::En, Locale::ZhCn];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::ZhCn => "zh-CN",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Locale::En => "English",
            Locale::ZhCn => "简体中文",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Locale> {
        Locale::ALL.into_iter().find(|locale| locale.id().eq_ignore_ascii_case(id))
    }
}

/// The Chinese catalogue files, merged.
const ZH_CN: &[&str] = &[include_str!("../locales/zh-CN/desktop.json")];

fn catalog(locale: Locale) -> &'static HashMap<String, String> {
    static ZH: OnceLock<HashMap<String, String>> = OnceLock::new();
    static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
    match locale {
        Locale::En => EN.get_or_init(HashMap::new),
        Locale::ZhCn => ZH.get_or_init(|| {
            let mut merged = HashMap::new();
            for file in ZH_CN {
                if let Ok(entries) = serde_json::from_str::<HashMap<String, String>>(file) {
                    merged.extend(entries.into_iter().filter(|(_, value)| !value.is_empty()));
                }
            }
            merged
        }),
    }
}

static ACTIVE: AtomicU8 = AtomicU8::new(0);

pub(crate) fn locale() -> Locale {
    if ACTIVE.load(Ordering::Relaxed) == 1 { Locale::ZhCn } else { Locale::En }
}

pub(crate) fn set_locale(locale: Locale) {
    ACTIVE.store(if locale == Locale::ZhCn { 1 } else { 0 }, Ordering::Relaxed);
}

/// The language a preference means: an explicit one, or the environment's.
pub(crate) fn resolve(preference: &str, environment: &[Option<String>]) -> Locale {
    if let Some(locale) = Locale::from_id(preference) {
        return locale;
    }
    for value in environment.iter().flatten() {
        let lower = value.to_ascii_lowercase();
        if lower.starts_with("zh") {
            return Locale::ZhCn;
        }
        if lower.starts_with("en") {
            return Locale::En;
        }
    }
    Locale::En
}

/// Applies the saved preference (call once at start-up).
pub(crate) fn init(preference: &str) {
    let environment: Vec<Option<String>> =
        ["LC_ALL", "LANGUAGE", "LANG"].iter().map(|name| std::env::var(name).ok()).collect();
    set_locale(resolve(preference, &environment));
}

/// The text in the active language (English when there is no entry).
pub(crate) fn t(text: &'static str) -> &'static str {
    catalog(locale()).get(text).map(String::as_str).unwrap_or(text)
}

/// `t` for text that is not a literal in this crate, such as labels from the
/// engine's catalogues (effects, masks); their entries are added by hand.
pub(crate) fn tr(text: &str) -> &str {
    catalog(locale()).get(text).map(String::as_str).unwrap_or(text)
}

/// `t` with `{name}` placeholders filled.
pub(crate) fn tf(text: &'static str, params: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = t(text).to_owned();
    for (name, value) in params {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_and_fills() {
        assert_eq!(resolve("auto", &[None, Some("zh_CN.UTF-8".into())]), Locale::ZhCn);
        assert_eq!(resolve("auto", &[Some("fr_FR".into())]), Locale::En);
        assert_eq!(resolve("en", &[Some("zh_CN".into())]), Locale::En);
        assert_eq!(resolve("zh-CN", &[]), Locale::ZhCn);
        set_locale(Locale::En);
        assert_eq!(t("Not in any catalogue"), "Not in any catalogue");
        assert_eq!(tf("{count} clips", &[("count", &3)]), "3 clips");
    }

    #[test]
    fn every_translated_string_has_a_chinese_entry() {
        // Each literal passed to t( or tf( in the desktop source must be in the catalogue.
        let zh = catalog(Locale::ZhCn);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        let mut stack = vec![dir];
        while let Some(path) = stack.pop() {
            for entry in std::fs::read_dir(&path).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") || path.ends_with("i18n.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for marker in ["t(\"", "tf(\""] {
                    let mut rest = text.as_str();
                    while let Some(index) = rest.find(marker) {
                        let before = rest[..index].chars().last();
                        rest = &rest[index + marker.len()..];
                        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        let Some(end) = rest.find('"') else { break };
                        let key = rest[..end].replace("\\\"", "\"");
                        if !zh.contains_key(&key) {
                            missing.push(format!("{}: {key}", path.display()));
                        }
                    }
                }
                let _ = &text;
            }
        }
        assert!(missing.is_empty(), "missing Chinese entries:\n{}", missing.join("\n"));
    }

    #[test]
    fn chinese_entries_keep_their_placeholders() {
        let names = |text: &str| {
            let mut found: Vec<String> = text.split('{').skip(1).filter_map(|part| part.split_once('}').map(|(name, _)| name.to_owned())).collect();
            found.sort();
            found
        };
        for (key, value) in catalog(Locale::ZhCn) {
            assert_eq!(names(key), names(value), "{key}");
        }
    }
}
