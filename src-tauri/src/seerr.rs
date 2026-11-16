//! Seerr (media requests) access layer.
//!
//! Uses the admin API key from `~/.config/azototv/seerr.json` (`{"url", "apiKey"}`)
//! and acts as the Seerr user linked to the logged-in Jellyfin user via the
//! `X-API-User` header. With the key alone Seerr acts as user 1, an ADMIN, so:
//! - JS can only reach the catalogue/request endpoints in `ALLOWED`, never
//!   settings or users, and the key never leaves Rust;
//! - a Jellyfin user without a linked Seerr account only gets read-only
//!   catalogue access (search/details/discover), never requests.

use std::{collections::HashMap, fs, sync::Mutex, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::jellyfin::{cached_image, config_dir};

#[derive(Deserialize)]
struct Config {
    url: String,
    #[serde(rename = "apiKey")]
    api_key: String,
}

#[derive(Clone)]
struct Linked {
    id: u64,
    name: String,
    permissions: u64,
}

#[derive(Serialize)]
pub struct Status {
    pub enabled: bool,
    /// Seerr display name of the linked user, if any.
    pub user: Option<String>,
    pub can_request: bool,
}

// Seerr permission bits that allow making requests (ADMIN, REQUEST, REQUEST_MOVIE, REQUEST_TV).
const CAN_REQUEST: u64 = 2 | 32 | 262_144 | 524_288;

/// (method, path prefix, needs a linked user)
const ALLOWED: &[(&str, &str, bool)] = &[
    ("GET", "/search", false),
    ("GET", "/movie/", false),
    ("GET", "/tv/", false),
    ("GET", "/discover/", false),
    ("GET", "/request", true),
    ("POST", "/request", true),
];

pub struct Seerr {
    client: reqwest::Client,
    config: Option<Config>,
    /// Jellyfin user id -> linked Seerr user (None = not linked)
    users: Mutex<HashMap<String, Option<Linked>>>,
}

/// RFC 3986 query string (`a=b%20c&...`). Seerr's request validator rejects
/// form-style encoding (space as `+`, which is what reqwest's `.query()` does).
fn encode_query(query: &[(String, String)]) -> String {
    let enc = |s: &str| {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect::<String>()
    };
    query
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn norm_id(id: &str) -> String {
    id.replace('-', "").to_ascii_lowercase()
}

impl Seerr {
    pub fn new() -> Self {
        let config = fs::read_to_string(config_dir().join("seerr.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Config>(&s).ok())
            .map(|c| Config { url: c.url.trim_end_matches('/').to_string(), ..c });
        Seerr {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .expect("http client"),
            config,
            users: Mutex::new(HashMap::new()),
        }
    }

    fn cfg(&self) -> Result<&Config, String> {
        self.config.as_ref().ok_or_else(|| "Seerr non configurato".to_string())
    }

    /// Look up (and cache) the Seerr user linked to a Jellyfin user id.
    async fn linked(&self, jf_user: &str, refresh: bool) -> Result<Option<Linked>, String> {
        let key = norm_id(jf_user);
        if !refresh {
            if let Some(l) = self.users.lock().unwrap().get(&key) {
                return Ok(l.clone());
            }
        }
        let c = self.cfg()?;
        let v: Value = self
            .client
            .get(format!("{}/api/v1/user", c.url))
            .query(&[("take", "1000")])
            .header("X-Api-Key", &c.api_key)
            .send()
            .await
            .map_err(|e| format!("Seerr non raggiungibile: {e}"))?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let found = v["results"].as_array().into_iter().flatten().find_map(|u| {
            if norm_id(u["jellyfinUserId"].as_str()?) != key {
                return None;
            }
            Some(Linked {
                id: u["id"].as_u64()?,
                name: u["displayName"].as_str().unwrap_or_default().to_string(),
                permissions: u["permissions"].as_u64().unwrap_or(0),
            })
        });
        self.users.lock().unwrap().insert(key, found.clone());
        Ok(found)
    }

    pub async fn status(&self, jf_user: &str) -> Status {
        if self.config.is_none() {
            return Status { enabled: false, user: None, can_request: false };
        }
        match self.linked(jf_user, true).await {
            Ok(l) => Status {
                enabled: true,
                can_request: l.as_ref().is_some_and(|l| l.permissions & CAN_REQUEST != 0),
                user: l.map(|l| l.name),
            },
            Err(_) => Status { enabled: false, user: None, can_request: false },
        }
    }

    /// JSON request to `/api/v1{path}`, restricted to `ALLOWED`.
    pub async fn request(
        &self,
        jf_user: &str,
        method: &str,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        if path.contains("..") || path.contains('?') || path.contains('#') {
            return Err("percorso non valido".into());
        }
        let rule = ALLOWED
            .iter()
            .find(|(m, p, _)| *m == method && path.starts_with(p))
            .ok_or_else(|| format!("Seerr: {method} {path} non consentito"))?;
        let linked = self.linked(jf_user, false).await?;
        if rule.2 && linked.is_none() {
            return Err("Il tuo utente non è collegato a Seerr".into());
        }
        let c = self.cfg()?;
        let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
        let mut url = format!("{}/api/v1{path}", c.url);
        if !query.is_empty() {
            url = format!("{url}?{}", encode_query(query));
        }
        let mut rb = self.client.request(m, url).header("X-Api-Key", &c.api_key);
        if let Some(l) = &linked {
            rb = rb.header("X-API-User", l.id.to_string());
        }
        if let Some(b) = body {
            rb = rb.json(&b);
        }
        let resp = rb.send().await.map_err(|e| format!("Seerr non raggiungibile: {e}"))?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            // Seerr explains refusals in {"message": ...}
            let msg = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|v| v["message"].as_str().map(str::to_string))
                .unwrap_or_else(|| format!("HTTP {status}"));
            return Err(msg);
        }
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|e| format!("risposta Seerr non valida: {e}"))
    }

    /// Who asked for a film (`media` "movie") or a series ("tv", the requests
    /// that include `season`), newest request first: (Seerr name, Jellyfin user id).
    /// For notifications (arr.rs), never handed to JS as such.
    pub async fn requesters(&self, media: &str, tmdb: u64, season: Option<u64>) -> Vec<(String, String)> {
        let Ok(c) = self.cfg() else { return vec![] };
        let v: Value = match self
            .client
            .get(format!("{}/api/v1/{media}/{tmdb}", c.url))
            .header("X-Api-Key", &c.api_key)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r.json().await.unwrap_or(Value::Null),
            _ => return vec![],
        };
        let mut reqs: Vec<&Value> = v["mediaInfo"]["requests"].as_array().into_iter().flatten().collect();
        reqs.retain(|r| {
            let seasons = r["seasons"].as_array().map(Vec::as_slice).unwrap_or_default();
            season.is_none_or(|s| seasons.is_empty() || seasons.iter().any(|x| x["seasonNumber"].as_u64() == Some(s)))
        });
        reqs.sort_by(|a, b| b["createdAt"].as_str().cmp(&a["createdAt"].as_str()));
        let mut out: Vec<(String, String)> = Vec::new();
        for r in reqs {
            let u = &r["requestedBy"];
            let name = u["displayName"].as_str().unwrap_or_default().to_string();
            let jf = u["jellyfinUserId"].as_str().map(norm_id).unwrap_or_default();
            if !name.is_empty() && !out.iter().any(|(n, _)| *n == name) {
                out.push((name, jf));
            }
        }
        out
    }

    /// TMDB artwork (`/w342/abc.jpg`), through the disk cache.
    pub async fn tmdb_image(&self, path: &str) -> Result<(Vec<u8>, String), String> {
        if path.contains("..") || !path.starts_with('/') {
            return Err("bad image path".into());
        }
        let url = format!("https://image.tmdb.org/t/p{path}");
        cached_image(self.client.get(&url), &url).await
    }
}
