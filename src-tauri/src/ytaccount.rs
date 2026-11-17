//! A profile's own Google account on YouTube (optional, and only on a profile
//! with a PIN). Signing in works like the TV's own YouTube app: the TV shows a
//! code, the person approves it on their phone at google.com/device, and
//! AzotoTV keeps a refresh token in the profile's folder (`google.json`, 600).
//! No password is ever typed on the TV.
//!
//! The account is only read: its home feed, the newest videos of its
//! subscriptions. Likes, subscriptions and history made on AzotoTV stay in
//! AzotoTV (youtube.svelte.ts), so the local recommendations keep learning and
//! take over if Google ever stops accepting this.
//!
//! Google accepts these tokens only from YouTube's TV client (TVHTML5): the web
//! client answers 400. Its pages are made of `tileRenderer`s.

use std::{
    collections::HashMap,
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::games::player_dir;
use crate::youtube::{avatar_url, dedup, find_all, last_thumb, parse_length, text, Video};

/// The OAuth client AzotoTV signs in as: `~/.config/azototv/youtube.json`,
/// `{"clientId": "...", "clientSecret": "..."}`. Google gives these device
/// codes only to YouTube on TV's own client, so that is the one to put there
/// (it is not in the repo). No file: no Google sign-in.
fn client() -> Result<(String, String), String> {
    let missing = || t!("Google sign-in isn't set up (youtube.json)");
    let t = fs::read_to_string(crate::jellyfin::config_dir().join("youtube.json")).map_err(|_| missing())?;
    let v: Value = serde_json::from_str(&t).map_err(|e| format!("youtube.json: {e}"))?;
    match (v["clientId"].as_str(), v["clientSecret"].as_str()) {
        (Some(id), Some(secret)) if !id.is_empty() && !secret.is_empty() => Ok((id.into(), secret.into())),
        _ => Err(missing()),
    }
}
const SCOPES: &str = "http://gdata.youtube.com https://www.googleapis.com/auth/youtube";
const OAUTH: &str = "https://www.youtube.com/o/oauth2";
const TV_UA: &str = "Mozilla/5.0 (ChromiumStylePlatform) Cobalt/Version";
const TV_VERSION: &str = "7.20250923.13.00";
/// pages of the home feed read after the first (each brings a few shelves)
const HOME_PAGES: usize = 2;

#[derive(Serialize, Deserialize, Default)]
struct Stored {
    refresh_token: String,
    #[serde(default)]
    access_token: String,
    /// unix seconds
    #[serde(default)]
    expires: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    handle: String,
    #[serde(default)]
    photo: Option<String>,
}

/// What JS gets of a linked account (never the tokens).
#[derive(Serialize)]
pub struct Info {
    pub name: String,
    /// "@nome"
    pub handle: String,
    pub photo: Option<String>,
}

/// What the TV shows while waiting for the approval.
#[derive(Serialize)]
pub struct Code {
    /// "CZW-HZG-SHY"
    pub code: String,
    /// where to type it
    pub url: String,
    /// the QR code of `url` as an SVG
    pub qr: String,
    /// seconds
    pub expires_in: u64,
}

/// Event `yt-account`: how a sign-in started with `start` ended.
#[derive(Serialize, Clone)]
struct Outcome {
    user: String,
    ok: bool,
    /// the account's name (ok) or what went wrong
    text: String,
}

#[derive(Serialize)]
pub struct Shelf {
    pub title: String,
    pub videos: Vec<Video>,
}

#[derive(Serialize)]
pub struct Home {
    pub shelves: Vec<Shelf>,
    /// the Shorts shelves, apart (youtube.svelte.ts keeps them on a leash)
    pub shorts: Vec<Video>,
}

pub struct Account {
    client: reqwest::Client,
    /// sign-ins waiting for approval: profile → attempt (a newer one, or `cancel`, stops the older)
    pending: Mutex<HashMap<String, u64>>,
    /// one token refresh at a time (the home and subscriptions ask together)
    refreshing: tokio::sync::Mutex<()>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn load(user: &str) -> Option<Stored> {
    if user.is_empty() {
        return None;
    }
    let t = fs::read_to_string(player_dir(user).join("google.json")).ok()?;
    serde_json::from_str::<Stored>(&t).ok().filter(|s| !s.refresh_token.is_empty())
}

fn save(user: &str, s: &Stored) -> Result<(), String> {
    let dir = player_dir(user);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(dir.join("google.json"))
        .map_err(|e| e.to_string())?;
    f.write_all(serde_json::to_string_pretty(s).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())
}

fn forget(user: &str) {
    let _ = fs::remove_file(player_dir(user).join("google.json"));
}

/// Whether `user` has a Google account linked (profile.rs: the PIN can't go while it has).
pub fn linked(user: &str) -> bool {
    load(user).is_some()
}

impl Account {
    pub fn new() -> Arc<Self> {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().expect("http client");
        Arc::new(Self { client, pending: Mutex::new(HashMap::new()), refreshing: tokio::sync::Mutex::new(()) })
    }

    async fn post(&self, url: &str, body: Value) -> Result<Value, String> {
        let resp = self.client.post(url).json(&body).send().await.map_err(|e| t!("Can't reach Google: {e}", e = e))?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        // the OAuth answers carry their own error codes, with a 4xx
        if !status.is_success() && v["error"].is_null() {
            return Err(format!("Google HTTP {status}"));
        }
        Ok(v)
    }

    pub fn info(&self, user: &str) -> Option<Info> {
        load(user).map(|s| Info { name: s.name, handle: s.handle, photo: s.photo })
    }

    /// Ask Google for a code, then wait (in the background, up to the code's
    /// life) for the person to approve it; the end comes as event `yt-account`.
    pub async fn start(self: &Arc<Self>, app: AppHandle, user: String) -> Result<Code, String> {
        if !crate::profile::has_pin(&user) {
            return Err(t!("Linking a Google account needs a PIN on this profile"));
        }
        let (client_id, _) = client()?;
        let device_id = format!("{:032x}", crate::jellyfin::fnv1a(&format!("azototv-{user}")));
        let r = self
            .post(&format!("{OAUTH}/device/code"), json!({ "client_id": client_id, "scope": SCOPES, "device_id": device_id, "device_model": "ytlr::" }))
            .await?;
        let (Some(device_code), Some(code)) = (r["device_code"].as_str(), r["user_code"].as_str()) else {
            return Err(t!("Google gives no code: {a}", a = r["error"].as_str().unwrap_or("risposta strana")));
        };
        let url = r["verification_url"].as_str().unwrap_or("https://www.google.com/device").to_string();
        let expires_in = r["expires_in"].as_u64().unwrap_or(1800);
        let interval = r["interval"].as_u64().unwrap_or(5).max(1);

        let attempt = now() * 1000 + u64::from(rand_u16());
        self.pending.lock().unwrap().insert(user.clone(), attempt);
        let (me, device_code) = (self.clone(), device_code.to_string());
        tauri::async_runtime::spawn(async move {
            let outcome = me.wait(&user, attempt, &device_code, interval, expires_in).await;
            let mut pending = me.pending.lock().unwrap();
            if pending.get(&user) != Some(&attempt) {
                return; // cancelled, or a newer code took over
            }
            pending.remove(&user);
            drop(pending);
            let (ok, text) = match outcome {
                Ok(name) => (true, name),
                Err(e) => (false, e),
            };
            let _ = app.emit("yt-account", Outcome { user, ok, text });
        });

        // the QR opens the page with the code already in (Google ignores it if it can't)
        let qr_url = format!("{url}?user_code={code}");
        let qr = qrcode::QrCode::new(qr_url.as_bytes())
            .map(|q| {
                q.render::<qrcode::render::svg::Color>()
                    .dark_color(qrcode::render::svg::Color("#000000"))
                    .light_color(qrcode::render::svg::Color("#ffffff"))
                    .quiet_zone(true)
                    .build()
            })
            .unwrap_or_default();
        Ok(Code { code: code.into(), url, qr, expires_in })
    }

    /// Stop waiting for `user`'s code (the sign-in screen was left).
    pub fn cancel(&self, user: &str) {
        self.pending.lock().unwrap().remove(user);
    }

    async fn wait(&self, user: &str, attempt: u64, device_code: &str, mut interval: u64, life: u64) -> Result<String, String> {
        let (client_id, client_secret) = client()?;
        let until = now() + life;
        while now() < until {
            tokio::time::sleep(Duration::from_secs(interval)).await;
            if self.pending.lock().unwrap().get(user) != Some(&attempt) {
                return Err("annullato".into());
            }
            let r = match self
                .post(
                    &format!("{OAUTH}/token"),
                    json!({ "client_id": client_id, "client_secret": client_secret, "code": device_code, "grant_type": "http://oauth.net/grant_type/device/1.0" }),
                )
                .await
            {
                Ok(r) => r,
                Err(_) => continue, // the network hiccupped: keep waiting
            };
            match r["error"].as_str() {
                Some("authorization_pending") => continue,
                Some("slow_down") => {
                    interval += 5;
                    continue;
                }
                Some("access_denied") => return Err(t!("Access refused on the phone")),
                Some("expired_token") => return Err(t!("Code expired")),
                Some(e) => return Err(format!("Google: {e}")),
                None => {}
            }
            let Some(refresh) = r["refresh_token"].as_str() else { return Err(t!("Google didn't give access")) };
            let mut s = Stored {
                refresh_token: refresh.into(),
                access_token: r["access_token"].as_str().unwrap_or_default().into(),
                expires: now() + r["expires_in"].as_u64().unwrap_or(3600),
                ..Default::default()
            };
            if let Ok((name, handle, photo)) = self.whoami(&s.access_token).await {
                (s.name, s.handle, s.photo) = (name, handle, photo);
            }
            save(user, &s)?;
            return Ok(if s.name.is_empty() { "Account Google".into() } else { s.name });
        }
        Err(t!("Code expired"))
    }

    /// Unlink: Google forgets the token too.
    pub async fn unlink(&self, user: &str) {
        if let Some(s) = load(user) {
            let _ = self.client.post("https://oauth2.googleapis.com/revoke").form(&[("token", s.refresh_token.as_str())]).send().await;
        }
        forget(user);
    }

    /// A working access token for `user`, refreshed when it's about to expire.
    /// A token Google no longer takes (removed from the Google account) unlinks.
    async fn token(&self, user: &str) -> Result<String, String> {
        let _one = self.refreshing.lock().await;
        let mut s = load(user).ok_or_else(|| t!("no Google account"))?;
        if !s.access_token.is_empty() && now() + 60 < s.expires {
            return Ok(s.access_token);
        }
        let (client_id, client_secret) = client()?;
        let r = self
            .post(
                &format!("{OAUTH}/token"),
                json!({ "client_id": client_id, "client_secret": client_secret, "refresh_token": s.refresh_token, "grant_type": "refresh_token" }),
            )
            .await?;
        match (r["access_token"].as_str(), r["error"].as_str()) {
            (Some(t), _) => {
                s.access_token = t.into();
                s.expires = now() + r["expires_in"].as_u64().unwrap_or(3600);
                save(user, &s)?;
                Ok(s.access_token)
            }
            (None, Some("invalid_grant")) => {
                forget(user);
                Err("revoked".into())
            }
            (None, e) => Err(format!("Google: {}", e.unwrap_or(&t!("no token")))),
        }
    }

    async fn call(&self, token: &str, endpoint: &str, mut body: Value) -> Result<Value, String> {
        body["context"] = json!({ "client": { "clientName": "TVHTML5", "clientVersion": TV_VERSION, "hl": crate::locale::lang(), "gl": crate::locale::region() } });
        let resp = self
            .client
            .post(format!("https://www.youtube.com/youtubei/v1/{endpoint}?prettyPrint=false"))
            .header("User-Agent", TV_UA)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| t!("Can't reach YouTube: {e}", e = e))?;
        if !resp.status().is_success() {
            return Err(format!("YouTube HTTP {}", resp.status()));
        }
        resp.json().await.map_err(|e| e.to_string())
    }

    /// The account's name, @handle and picture.
    async fn whoami(&self, token: &str) -> Result<(String, String, Option<String>), String> {
        let v = self.call(token, "account/accounts_list", json!({})).await?;
        let first = |key: &str| find_all(&v, key).into_iter().map(text).find(|t| !t.is_empty()).unwrap_or_default();
        let photo = find_all(&v, "accountPhoto").into_iter().find_map(last_thumb).map(|u| avatar_url(&u));
        Ok((first("accountName"), first("channelHandle"), photo))
    }

    /// The account's YouTube home: its shelves of videos, and its Shorts apart.
    pub async fn home(&self, user: &str) -> Result<Home, String> {
        let token = self.token(user).await?;
        let mut v = self.call(&token, "browse", json!({ "browseId": "FEwhat_to_watch" })).await?;
        let (mut shelves, mut shorts) = (vec![], vec![]);
        for page in 0..=HOME_PAGES {
            for s in find_all(&v, "shelfRenderer") {
                let title = find_all(&s["headerRenderer"], "title").into_iter().map(text).find(|t| !t.is_empty()).unwrap_or_default();
                let (normal, short): (Vec<Video>, Vec<Video>) = tiles_in(s).into_iter().partition(|x| !x.short);
                shorts.extend(short);
                if !normal.is_empty() {
                    shelves.push(Shelf { title, videos: normal });
                }
            }
            let next = find_all(&v, "nextContinuationData").into_iter().find_map(|c| c["continuation"].as_str()).map(str::to_string);
            match next {
                Some(c) if page < HOME_PAGES => v = self.call(&token, "browse", json!({ "continuation": c })).await.unwrap_or(Value::Null),
                _ => break,
            }
        }
        // a video shows once, in its first shelf
        let mut seen = std::collections::HashSet::new();
        for s in &mut shelves {
            s.videos.retain(|x| seen.insert(x.id.clone()));
        }
        shelves.retain(|s| s.videos.len() >= 3);
        dedup(&mut shorts);
        Ok(Home { shelves, shorts })
    }

    /// The newest videos of the account's subscriptions (no Shorts), newest first.
    pub async fn subscriptions(&self, user: &str) -> Result<Vec<Video>, String> {
        let token = self.token(user).await?;
        let v = self.call(&token, "browse", json!({ "browseId": "FEsubscriptions" })).await?;
        Ok(tiles_in(&v).into_iter().filter(|x| !x.short).collect())
    }
}

fn rand_u16() -> u16 {
    let mut b = [0u8; 2];
    let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut b);
    u16::from_le_bytes(b)
}

/// Every video tile in a TV page, in page order (channels, playlists and the like left out).
fn tiles_in(v: &Value) -> Vec<Video> {
    let mut out: Vec<Video> = find_all(v, "tileRenderer").into_iter().filter_map(tile).collect();
    dedup(&mut out);
    out
}

/// A TV tile: title, then lines of "Channel" and "views • when", the length on the picture.
fn tile(x: &Value) -> Option<Video> {
    let kind = x["contentType"].as_str().unwrap_or_default();
    let short = kind == "TILE_CONTENT_TYPE_SHORTS";
    if !(short || kind == "TILE_CONTENT_TYPE_VIDEO") {
        return None;
    }
    let cmd = &x["onSelectCommand"];
    let id = cmd["watchEndpoint"]["videoId"]
        .as_str()
        .or_else(|| cmd["reelWatchEndpoint"]["videoId"].as_str())
        .or_else(|| x["contentId"].as_str())?
        .to_string();
    let meta = &x["metadata"]["tileMetadataRenderer"];
    let lines: Vec<Vec<String>> = meta["lines"]
        .as_array()
        .map(|ls| {
            ls.iter()
                .map(|l| {
                    l["lineRenderer"]["items"]
                        .as_array()
                        .map(|it| it.iter().map(|i| text(&i["lineItemRenderer"]["text"])).filter(|t| !t.is_empty() && t != "•").collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    let channel = lines.first().and_then(|l| l.first()).cloned().unwrap_or_default();
    let stats = lines.get(1).cloned().unwrap_or_default();
    // YouTube's wording, in English or Italian (locale.rs)
    let views = stats.iter().find(|s| s.contains("view") || s.contains("visualizz")).cloned().unwrap_or_default();
    let published = stats
        .iter()
        .find(|s| s.ends_with(" ago") || s.ends_with(" fa") || s.starts_with("Streamed") || s.starts_with("Trasmesso"))
        .cloned()
        .unwrap_or_default();
    let overlays = find_all(&x["header"], "thumbnailOverlayTimeStatusRenderer");
    let length = overlays.iter().map(|o| parse_length(&text(&o["text"]))).find(|l| *l > 0).unwrap_or(0);
    let live = overlays.iter().any(|o| o["style"].as_str() == Some("LIVE"));
    // the channel's id sits in the tile's menu ("Vai al canale")
    let channel_id = find_all(x, "browseId").into_iter().filter_map(|b| b.as_str()).find(|b| b.starts_with("UC")).unwrap_or_default().to_string();
    Some(Video {
        id,
        title: text(&meta["title"]),
        channel: if short { String::new() } else { channel },
        channel_id,
        length,
        views: if short && views.is_empty() { stats.first().cloned().unwrap_or_default() } else { views },
        published,
        short,
        live,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tv_tile() {
        let x = json!({
            "contentType": "TILE_CONTENT_TYPE_VIDEO",
            "header": { "tileHeaderRenderer": { "thumbnailOverlays": [{ "thumbnailOverlayTimeStatusRenderer": { "text": { "simpleText": "14:21" }, "style": "DEFAULT" } }] } },
            "metadata": { "tileMetadataRenderer": {
                "title": { "simpleText": "Switch 2 Games" },
                "lines": [
                    { "lineRenderer": { "items": [{ "lineItemRenderer": { "text": { "runs": [{ "text": "Better Gaming" }] } } }] } },
                    { "lineRenderer": { "items": [
                        { "lineItemRenderer": { "badge": {} } },
                        { "lineItemRenderer": { "text": { "simpleText": "1,1K visualizzazioni" } } },
                        { "lineItemRenderer": { "text": { "simpleText": "•" } } },
                        { "lineItemRenderer": { "text": { "simpleText": "36 minuti fa" } } }
                    ] } }
                ]
            } },
            "onSelectCommand": { "watchEndpoint": { "videoId": "ZHVZZIZi9zk" } },
            "menu": { "browseEndpoint": { "browseId": "UCabc" } }
        });
        let v = tile(&x).unwrap();
        assert_eq!((v.id.as_str(), v.title.as_str(), v.channel.as_str(), v.channel_id.as_str()), ("ZHVZZIZi9zk", "Switch 2 Games", "Better Gaming", "UCabc"));
        assert_eq!((v.length, v.views.as_str(), v.published.as_str(), v.short), (861, "1,1K visualizzazioni", "36 minuti fa", false));
        assert!(tile(&json!({ "contentType": "TILE_CONTENT_TYPE_CHANNEL" })).is_none());
    }
}
