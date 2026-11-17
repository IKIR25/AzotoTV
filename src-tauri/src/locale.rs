//! Language and region. The interface is written in English; other
//! languages are dictionaries from the English text to theirs
//! (`i18n/<language>.json`, shared with the web interface and the phone
//! page). Text without a translation stays English. When one English text
//! needs two translations, a note after a bar tells them apart and is never
//! shown: `t!("Play|game")`.
//!
//! The language is the system's (`LC_ALL` / `LC_MESSAGES` / `LANG`) unless
//! `~/.config/azototv/settings.json` says otherwise
//! (`{"language": "it", "region": "IT"}`; the START menu writes it). The
//! region is what YouTube and dates go by.
//!
//! In Rust: `t!("TV is off")`, `t!("Can't reach Seerr: {e}", e = err)`.

use std::{
    collections::HashMap,
    fs,
    sync::{OnceLock, RwLock},
};

use serde_json::{Value, json};

/// The languages there is a dictionary for (English needs none).
pub const LANGUAGES: [(&str, &str); 2] = [("en", "English"), ("it", "Italiano")];

const IT: &str = include_str!("../../i18n/it.json");

struct State {
    lang: String,
    region: String,
}

fn state() -> &'static RwLock<State> {
    static STATE: OnceLock<RwLock<State>> = OnceLock::new();
    STATE.get_or_init(|| {
        let settings = settings();
        let env = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty())).unwrap_or_default();
        let (env_lang, env_region) = parse_posix(&env);
        // tests check the English text, whatever the machine they run on speaks
        let lang = if cfg!(test) { "en".into() } else { settings["language"].as_str().map(str::to_lowercase).unwrap_or(env_lang) };
        let region = settings["region"].as_str().map(str::to_uppercase).unwrap_or(env_region);
        RwLock::new(State { lang: supported(&lang).to_string(), region })
    })
}

fn settings_path() -> std::path::PathBuf {
    crate::jellyfin::config_dir().join("settings.json")
}

fn settings() -> Value {
    fs::read_to_string(settings_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)
}

/// "it_IT.UTF-8" -> ("it", "IT"); "C", "POSIX" or nothing -> ("en", "US").
fn parse_posix(s: &str) -> (String, String) {
    let s = s.split(['.', '@']).next().unwrap_or("");
    let (lang, region) = s.split_once('_').unwrap_or((s, ""));
    let ok = |x: &str, n: std::ops::RangeInclusive<usize>| n.contains(&x.len()) && x.chars().all(|c| c.is_ascii_alphabetic());
    if !ok(lang, 2..=3) {
        return ("en".into(), "US".into());
    }
    let region = if ok(region, 2..=2) { region.to_uppercase() } else { "US".into() };
    (lang.to_lowercase(), region)
}

fn supported(lang: &str) -> &'static str {
    LANGUAGES.iter().map(|(id, _)| *id).find(|id| *id == lang).unwrap_or("en")
}

/// "en", "it".
pub fn lang() -> String {
    state().read().unwrap().lang.clone()
}

/// "US", "IT".
pub fn region() -> String {
    state().read().unwrap().region.clone()
}

/// The language as films' audio and subtitle tracks name it: "ita,it".
pub fn track_langs() -> String {
    let lang = lang();
    let long = match lang.as_str() {
        "it" => "ita",
        _ => "eng",
    };
    format!("{long},{lang}")
}

/// The language as Steam names it (games that ask the Steam emulator).
pub fn steam_language() -> &'static str {
    match lang().as_str() {
        "it" => "italian",
        _ => "english",
    }
}

/// What the interface needs to know (also injected into its page at start).
pub fn info() -> Value {
    let s = state().read().unwrap();
    json!({
        "lang": s.lang,
        "region": s.region,
        "languages": LANGUAGES.iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
    })
}

/// A new language, from now on and at the next start.
pub fn set_lang(lang: &str) -> Result<(), String> {
    if supported(lang) != lang {
        return Err(format!("no such language: {lang}"));
    }
    let mut v = settings();
    if !v.is_object() {
        v = json!({});
    }
    v["language"] = json!(lang);
    let path = settings_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| format!("{}: {e}", path.display()))?;
    state().write().unwrap().lang = lang.to_string();
    Ok(())
}

fn dictionary(lang: &str) -> Option<&'static HashMap<String, String>> {
    static ITALIAN: OnceLock<HashMap<String, String>> = OnceLock::new();
    match lang {
        "it" => Some(ITALIAN.get_or_init(|| serde_json::from_str(IT).unwrap_or_default())),
        _ => None,
    }
}

/// The dictionary of the language in use, as JSON (`{}` for English): what
/// the phone page translates with.
pub fn dictionary_json() -> String {
    match lang().as_str() {
        "it" => IT.to_string(),
        _ => "{}".into(),
    }
}

fn translate(lang: &str, en: &str) -> String {
    dictionary(lang).and_then(|d| d.get(en)).filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| en.split('|').next().unwrap_or(en).to_string())
}

/// `en` in the language in use.
pub fn t(en: &str) -> String {
    translate(&lang(), en)
}

/// Puts `{name}` values into a translated text.
pub fn fill(mut text: String, values: &[(&str, String)]) -> String {
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// `t!("text")`, `t!("text with {a}", a = value)`: the text in the language in use.
#[macro_export]
macro_rules! t {
    ($text:literal) => {
        $crate::locale::t($text)
    };
    ($text:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::locale::fill($crate::locale::t($text), &[$((stringify!($name), ($value).to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_locales() {
        assert_eq!(parse_posix("it_IT.UTF-8"), ("it".into(), "IT".into()));
        assert_eq!(parse_posix("en_GB"), ("en".into(), "GB".into()));
        assert_eq!(parse_posix("de_DE@euro"), ("de".into(), "DE".into()));
        assert_eq!(parse_posix("C.UTF-8"), ("en".into(), "US".into()));
        assert_eq!(parse_posix("POSIX"), ("en".into(), "US".into()));
        assert_eq!(parse_posix(""), ("en".into(), "US".into()));
        assert_eq!(supported("de"), "en");
        assert_eq!(supported("it"), "it");
    }

    #[test]
    fn the_italian_dictionary_is_sound() {
        let d = dictionary("it").unwrap();
        assert!(!d.is_empty(), "i18n/it.json doesn't parse or is empty");
        // a translation keeps every {placeholder} of its English text
        for (en, it) in d {
            let names = |s: &str| {
                let mut v: Vec<String> = s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(n, _)| n.to_string())).collect();
                v.sort();
                v
            };
            assert_eq!(names(en), names(it), "placeholders differ in {en:?}");
        }
    }

    #[test]
    fn fills_named_values() {
        assert_eq!(fill("{n} of {total}".into(), &[("n", 2.to_string()), ("total", "5".into())]), "2 of 5");
        assert_eq!(translate("en", "TV is off"), "TV is off");
        assert_eq!(translate("en", "Play|game"), "Play");
        assert_eq!(translate("it", "a text nobody translated"), "a text nobody translated");
    }
}
