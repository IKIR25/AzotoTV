//! START › Services: the optional services (Seerr, Immich, Radarr / Sonarr,
//! music downloads, SteamGridDB, Google sign-in) set up from the TV instead of by hand.
//!
//! Each service is one of the config files in `~/.config/azototv/` (see
//! `docs/SETUP.md`); this page only reads and writes the fields listed here and
//! keeps anything else in the file (Immich's `shareWith`, say). Saving tests
//! the connection first, writes the file owner-only (API keys) and the
//! interface then restarts AzotoTV, since every service reads its file once at
//! start. A Jellyfin API key a service needs is made by AzotoTV itself, with
//! the signed-in profile's rights (it has to be a Jellyfin administrator, which
//! is also who may open this page at all).

use std::{fs, path::PathBuf, time::Duration};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::jellyfin::{Jf, config_dir};

#[derive(Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Url,
    Text,
    Secret,
    /// a folder on this machine
    Dir,
}

struct FieldDef {
    /// where in the file: "radarr.url"
    key: &'static str,
    kind: Kind,
    optional: bool,
    /// what an empty field starts as on this page
    default: &'static str,
}

struct Def {
    id: &'static str,
    file: &'static str,
    fields: &'static [FieldDef],
    /// keys AzotoTV fills by itself on save when missing
    jellyfin_key: Option<&'static str>,
    jellyfin_url: Option<&'static str>,
}

const fn f(key: &'static str, kind: Kind, optional: bool, default: &'static str) -> FieldDef {
    FieldDef { key, kind, optional, default }
}

const DEFS: &[Def] = &[
    Def {
        id: "seerr",
        file: "seerr.json",
        fields: &[f("url", Kind::Url, false, "http://localhost:5055"), f("apiKey", Kind::Secret, false, "")],
        jellyfin_key: None,
        jellyfin_url: None,
    },
    Def {
        id: "immich",
        file: "immich.json",
        fields: &[
            f("url", Kind::Url, false, "http://localhost:2283"),
            f("apiKey", Kind::Secret, false, ""),
            f("uploadKey", Kind::Secret, true, ""),
        ],
        jellyfin_key: None,
        jellyfin_url: None,
    },
    Def {
        id: "arr",
        file: "arr.json",
        fields: &[
            f("radarr.url", Kind::Url, true, "http://localhost:7878"),
            f("radarr.apiKey", Kind::Secret, true, ""),
            f("sonarr.url", Kind::Url, true, "http://localhost:8989"),
            f("sonarr.apiKey", Kind::Secret, true, ""),
            f("rdt.url", Kind::Url, true, ""),
            f("rdt.username", Kind::Text, true, ""),
            f("rdt.password", Kind::Secret, true, ""),
        ],
        jellyfin_key: Some("jellyfinKey"),
        jellyfin_url: None,
    },
    Def {
        id: "music",
        file: "music.json",
        fields: &[f("dir", Kind::Dir, false, "")],
        jellyfin_key: Some("jellyfinKey"),
        jellyfin_url: Some("jellyfinUrl"),
    },
    Def {
        id: "steamgriddb",
        file: "steamgriddb.json",
        fields: &[f("apiKey", Kind::Secret, false, "")],
        jellyfin_key: None,
        jellyfin_url: None,
    },
    Def {
        id: "youtube",
        file: "youtube.json",
        fields: &[f("clientId", Kind::Text, false, ""), f("clientSecret", Kind::Secret, false, "")],
        jellyfin_key: None,
        jellyfin_url: None,
    },
];

/// The words on the page, per service and field (kept here so the English
/// text is in one place, translated through `t!`).
fn title(id: &str) -> (String, String) {
    match id {
        "seerr" => ("Seerr".into(), t!("Requests: ask for films and series that aren't in the library")),
        "immich" => ("Immich".into(), t!("Gallery, photo screensaver, photos sent from phones")),
        "arr" => ("Radarr / Sonarr".into(), t!("\"Ready to watch\" notifications and download progress")),
        "music" => (t!("Music downloads"), t!("Songs found on YouTube Music saved into the library")),
        "steamgriddb" => ("SteamGridDB".into(), t!("Pictures for games added from the TV, consoles too")),
        "youtube" => (t!("Google sign-in"), t!("A profile's own Google account in YouTube")),
        _ => (id.into(), String::new()),
    }
}

fn label(id: &str, key: &str) -> (String, String) {
    match (id, key) {
        (_, "url") => (t!("Address"), String::new()),
        ("seerr", "apiKey") => (t!("API key"), t!("Seerr › Settings › General")),
        ("immich", "apiKey") => (t!("API key"), t!("Of the Immich user the TV signs in as: its Account settings › API keys")),
        ("immich", "uploadKey") => (t!("Upload key"), t!("A key that may only upload and make albums, for photos sent from phones")),
        ("arr", "radarr.url") => (t!("Radarr address"), String::new()),
        ("arr", "radarr.apiKey") => (t!("Radarr API key"), t!("Settings › General")),
        ("arr", "sonarr.url") => (t!("Sonarr address"), String::new()),
        ("arr", "sonarr.apiKey") => (t!("Sonarr API key"), t!("Settings › General")),
        ("arr", "rdt.url") => (t!("RDT-Client address"), t!("More precise download progress")),
        ("arr", "rdt.username") => (t!("RDT-Client user"), String::new()),
        ("arr", "rdt.password") => (t!("RDT-Client password"), String::new()),
        ("music", "dir") => (t!("Folder"), t!("A folder inside one of Jellyfin's music libraries, on this machine")),
        ("steamgriddb", "apiKey") => (t!("API key"), t!("Free: steamgriddb.com › Preferences › API")),
        ("youtube", "clientId") => (t!("OAuth client ID"), t!("YouTube on TV's own client: see ytaccount.rs")),
        ("youtube", "clientSecret") => (t!("OAuth client secret"), String::new()),
        _ => (key.into(), String::new()),
    }
}

#[derive(Serialize)]
pub struct Field {
    key: &'static str,
    label: String,
    hint: String,
    kind: Kind,
    optional: bool,
    value: String,
}

#[derive(Serialize)]
pub struct Service {
    id: &'static str,
    title: String,
    what: String,
    /// its file exists
    configured: bool,
    fields: Vec<Field>,
}

fn path(def: &Def) -> PathBuf {
    config_dir().join(def.file)
}

fn read(def: &Def) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path(def)).ok()?).ok()
}

fn get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.').try_fold(v, |v, k| v.get(k))
}

fn set(v: &mut Value, key: &str, value: Option<String>) {
    let (parents, last) = match key.rsplit_once('.') {
        Some((p, l)) => (p.split('.').collect::<Vec<_>>(), l),
        None => (Vec::new(), key),
    };
    let mut cur = v;
    for p in parents {
        if !cur.get(p).is_some_and(Value::is_object) {
            if value.is_none() {
                return;
            }
            cur[p] = json!({});
        }
        cur = &mut cur[p];
    }
    match value {
        Some(s) => cur[last] = json!(s),
        None => {
            if let Some(o) = cur.as_object_mut() {
                o.remove(last);
            }
        }
    }
}

/// Objects left without anything in them ("rdt" after its fields were cleared).
fn prune(v: &mut Value) {
    if let Some(o) = v.as_object_mut() {
        for x in o.values_mut() {
            prune(x);
        }
        o.retain(|_, x| !x.as_object().is_some_and(Map::is_empty));
    }
}

fn def(id: &str) -> Result<&'static Def, String> {
    DEFS.iter().find(|d| d.id == id).ok_or_else(|| format!("no such service: {id}"))
}

/// Only a Jellyfin administrator may see or change these (they hold keys
/// that act for everyone).
pub(crate) async fn admin(jf: &Jf) -> Result<(), String> {
    let me = jf.request("GET", "/Users/Me", &[], None).await?;
    if me["Policy"]["IsAdministrator"].as_bool() == Some(true) {
        Ok(())
    } else {
        Err(t!("Only a Jellyfin administrator can change the services"))
    }
}

pub async fn list(jf: &Jf) -> Result<Vec<Service>, String> {
    admin(jf).await?;
    Ok(DEFS
        .iter()
        .map(|d| {
            let file = read(d);
            let (title, what) = title(d.id);
            Service {
                id: d.id,
                title,
                what,
                configured: file.is_some(),
                fields: d
                    .fields
                    .iter()
                    .map(|fd| {
                        let (label, hint) = label(d.id, fd.key);
                        let value = file.as_ref().and_then(|v| get(v, fd.key)).and_then(Value::as_str).map(str::to_string);
                        Field {
                            key: fd.key,
                            label,
                            hint,
                            kind: fd.kind,
                            optional: fd.optional,
                            // a configured service's empty optional field stays empty
                            value: value.unwrap_or_else(|| if file.is_some() && fd.optional { String::new() } else { fd.default.into() }),
                        }
                    })
                    .collect(),
            }
        })
        .collect())
}

/// "localhost:5055/" -> "http://localhost:5055"
fn norm_url(s: &str) -> String {
    let s = s.trim().trim_end_matches('/');
    if s.is_empty() || s.contains("://") { s.to_string() } else { format!("http://{s}") }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("http client")
}

/// What went wrong talking to `name`, in words.
fn http_error(name: &str, url: &str, e: reqwest::Error) -> String {
    if e.is_connect() || e.is_timeout() {
        t!("{name} doesn't answer at {url}", name = name, url = url)
    } else {
        format!("{name}: {e}")
    }
}

/// GET `url` with an API key header; Ok(the JSON) or why not.
async fn get_json(name: &str, base: &str, path: &str, header: &str, key: &str) -> Result<Value, String> {
    let resp = client().get(format!("{base}{path}")).header(header, key).send().await.map_err(|e| http_error(name, base, e))?;
    match resp.status().as_u16() {
        200..=299 => resp.json().await.map_err(|_| t!("{name}: that address isn't {name}", name = name)),
        401 | 403 => Err(t!("{name} refused the key", name = name)),
        404 => Err(t!("{name}: that address isn't {name}", name = name)),
        s => Err(format!("{name}: HTTP {s}")),
    }
}

/// The values the page sent, cleaned up, by key ("" = left empty).
fn clean(def: &Def, values: &Map<String, Value>) -> Vec<(&'static str, String)> {
    def.fields
        .iter()
        .map(|fd| {
            let v = values.get(fd.key).and_then(Value::as_str).unwrap_or("").trim();
            (fd.key, if fd.kind == Kind::Url { norm_url(v) } else { v.to_string() })
        })
        .collect()
}

fn value<'a>(vals: &'a [(&str, String)], key: &str) -> &'a str {
    vals.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_str()).unwrap_or("")
}

/// Tries the values: Ok(one line per thing that works) or the first problem.
async fn test(def: &Def, vals: &[(&'static str, String)]) -> Result<Vec<String>, String> {
    for fd in def.fields {
        if !fd.optional && value(vals, fd.key).is_empty() {
            let (label, _) = label(def.id, fd.key);
            return Err(t!("{field} is missing", field = label));
        }
    }
    let mut ok = Vec::new();
    match def.id {
        "seerr" => {
            let v = get_json("Seerr", value(vals, "url"), "/api/v1/settings/main", "X-Api-Key", value(vals, "apiKey")).await?;
            ok.push(t!("Seerr answers ({name})", name = v["applicationTitle"].as_str().unwrap_or("Seerr")));
        }
        "immich" => {
            let url = value(vals, "url");
            let me = get_json("Immich", url, "/api/users/me", "x-api-key", value(vals, "apiKey")).await?;
            ok.push(t!("Immich: signed in as {name}", name = me["name"].as_str().unwrap_or("?")));
            let up = value(vals, "uploadKey");
            if !up.is_empty() {
                get_json("Immich", url, "/api/albums", "x-api-key", up).await.map_err(|e| format!("{} ({e})", t!("Upload key")))?;
                ok.push(t!("Upload key works"));
            }
        }
        "arr" => {
            for (name, svc) in [("Radarr", "radarr"), ("Sonarr", "sonarr")] {
                let (url, key) = (value(vals, &format!("{svc}.url")), value(vals, &format!("{svc}.apiKey")));
                if key.is_empty() {
                    continue;
                }
                let v = get_json(name, url, "/api/v3/system/status", "X-Api-Key", key).await?;
                ok.push(format!("{name} {}", v["version"].as_str().unwrap_or("")));
            }
            if ok.is_empty() {
                return Err(t!("Fill in Radarr, Sonarr or both"));
            }
            let rdt = value(vals, "rdt.url");
            if !rdt.is_empty() {
                let resp = client()
                    .post(format!("{rdt}/Api/Authentication/Login"))
                    .json(&json!({ "userName": value(vals, "rdt.username"), "password": value(vals, "rdt.password") }))
                    .send()
                    .await
                    .map_err(|e| http_error("RDT-Client", rdt, e))?;
                if !resp.status().is_success() {
                    return Err(t!("RDT-Client refused the user and password"));
                }
                ok.push(t!("RDT-Client: signed in"));
            }
        }
        "music" => {
            let dir = PathBuf::from(crate::games::expand_home(value(vals, "dir")));
            if !dir.is_dir() {
                return Err(t!("There is no folder {dir}", dir = dir.display()));
            }
            let probe = dir.join(".azototv-write-test");
            fs::write(&probe, b"").map_err(|_| t!("AzotoTV can't write in {dir}", dir = dir.display()))?;
            let _ = fs::remove_file(probe);
            ok.push(t!("The folder is there and AzotoTV can write in it"));
        }
        "steamgriddb" => {
            let resp = client()
                .get("https://www.steamgriddb.com/api/v2/search/autocomplete/mario")
                .bearer_auth(value(vals, "apiKey"))
                .send()
                .await
                .map_err(|e| http_error("SteamGridDB", "steamgriddb.com", e))?;
            if !resp.status().is_success() {
                return Err(t!("{name} refused the key", name = "SteamGridDB"));
            }
            ok.push(t!("SteamGridDB answers"));
        }
        _ => ok.push(t!("It gets tried when a profile signs in to Google")),
    }
    Ok(ok)
}

pub async fn try_values(id: &str, values: Map<String, Value>) -> Result<Vec<String>, String> {
    let def = def(id)?;
    test(def, &clean(def, &values)).await
}

/// AzotoTV's own Jellyfin API key: the one it made before, or a new one.
async fn jellyfin_key(jf: &Jf) -> Result<String, String> {
    const APP: &str = "AzotoTV";
    let find = |v: Value| {
        v["Items"].as_array()?.iter().rev().find(|k| k["AppName"].as_str() == Some(APP))?["AccessToken"].as_str().map(str::to_string)
    };
    if let Some(k) = find(jf.request("GET", "/Auth/Keys", &[], None).await?) {
        return Ok(k);
    }
    jf.request("POST", "/Auth/Keys", &[("app".into(), APP.into())], None).await?;
    find(jf.request("GET", "/Auth/Keys", &[], None).await?).ok_or_else(|| t!("Jellyfin didn't make an API key"))
}

/// Tests, then writes the file (owner-only). Returns what the test said.
pub async fn save(jf: &Jf, id: &str, values: Map<String, Value>) -> Result<Vec<String>, String> {
    admin(jf).await?;
    let def = def(id)?;
    let vals = clean(def, &values);
    let lines = test(def, &vals).await?;
    let mut file = read(def).filter(Value::is_object).unwrap_or_else(|| json!({}));
    for (key, v) in &vals {
        set(&mut file, key, (!v.is_empty()).then(|| v.clone()));
    }
    if let Some(k) = def.jellyfin_key {
        if get(&file, k).and_then(Value::as_str).is_none_or(str::is_empty) {
            set(&mut file, k, Some(jellyfin_key(jf).await?));
        }
    }
    if let Some(k) = def.jellyfin_url {
        if get(&file, k).and_then(Value::as_str).is_none_or(str::is_empty) {
            set(&mut file, k, jf.server());
        }
    }
    prune(&mut file);
    write(&path(def), &file)?;
    Ok(lines)
}

/// Turns a service off: its file goes (kept as `<file>.off`, to bring back by hand).
pub async fn remove(jf: &Jf, id: &str) -> Result<(), String> {
    admin(jf).await?;
    let p = path(def(id)?);
    fs::rename(&p, p.with_extension("json.off")).map_err(|e| format!("{}: {e}", p.display()))
}

pub(crate) fn write(p: &std::path::Path, v: &Value) -> Result<(), String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::io::Write;
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = p.with_extension("tmp");
    let mut out = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    out.write_all(serde_json::to_string_pretty(v).unwrap_or_default().as_bytes()).map_err(|e| e.to_string())?;
    let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    fs::rename(&tmp, p).map_err(|e| format!("{}: {e}", p.display()))
}

/// Started by kiosk.sh, which starts it again whenever it doesn't exit cleanly.
fn under_kiosk() -> bool {
    let ppid = std::os::unix::process::parent_id();
    fs::read(format!("/proc/{ppid}/cmdline")).is_ok_and(|c| c.split(|b| *b == 0).any(|a| a.ends_with(b"kiosk.sh")))
}

/// AzotoTV again from the start, so every service reads its file anew.
pub fn restart(app: &tauri::AppHandle) {
    if under_kiosk() {
        // not 0: kiosk.sh takes a clean exit for "Exit to the desktop"
        app.exit(75);
    } else {
        app.restart();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_keys_are_set_and_cleared() {
        let mut v = json!({"jellyfinKey": "k", "radarr": {"url": "u", "apiKey": "a"}, "extra": [1]});
        set(&mut v, "sonarr.url", Some("s".into()));
        set(&mut v, "radarr.apiKey", None);
        set(&mut v, "radarr.url", None);
        set(&mut v, "rdt.url", None);
        prune(&mut v);
        assert_eq!(v, json!({"jellyfinKey": "k", "sonarr": {"url": "s"}, "extra": [1]}));
        assert_eq!(get(&v, "sonarr.url"), Some(&json!("s")));
    }

    #[test]
    fn urls_get_a_scheme_and_lose_the_slash() {
        assert_eq!(norm_url(" localhost:5055/ "), "http://localhost:5055");
        assert_eq!(norm_url("https://x.example/"), "https://x.example");
        assert_eq!(norm_url(""), "");
    }

    #[test]
    fn every_field_has_words() {
        for d in DEFS {
            assert!(!title(d.id).1.is_empty(), "{}", d.id);
            for fd in d.fields {
                assert_ne!(label(d.id, fd.key).0, fd.key, "{}.{}", d.id, fd.key);
            }
        }
    }
}
