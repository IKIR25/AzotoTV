//! Galleria: the family photos in Immich (a container on the server).
//!
//! AzotoTV signs in as Immich's "TV" user with a read-only API key from
//! `~/.config/azototv/immich.json` (`{"url", "apiKey"}`, mode 600; the keys
//! never leave Rust). The TV user owns the family albums (shared back to the
//! family as editors) and sees the ones shared with it; the Galleria only ever
//! shows what is in an album.
//!
//! Phones add photos through AzotoTV (phone.rs) with a second key that may
//! only upload and add to / create albums: `"uploadKey"`, and `"shareWith"` =
//! the Immich user ids every album made from a phone is shared with (editors).
//!
//! Pictures reach the webview through `im://localhost/<asset id>/<size>`,
//! cached on disk; videos play in mpv straight from Immich (the original file,
//! mpv handles every old format).

use std::{
    collections::HashMap,
    fs,
    sync::Mutex,
    time::{Duration, Instant, SystemTime},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::jellyfin::{cache_dir, config_dir};

/// pictures not looked at for this long are dropped from the disk cache at startup
const CACHE_DAYS: u64 = 30;
/// the album list and "on this day" are asked for again after this
const TTL: Duration = Duration::from_secs(10 * 60);
const PAGE: u32 = 250;

#[derive(Deserialize)]
struct Config {
    url: String,
    #[serde(rename = "apiKey")]
    api_key: String,
    #[serde(rename = "uploadKey", default)]
    upload_key: Option<String>,
    #[serde(rename = "shareWith", default)]
    share_with: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct Album {
    pub id: String,
    pub name: String,
    pub description: String,
    pub count: u32,
    /// asset id of the album's cover
    pub cover: String,
    /// dates of the oldest and newest picture (YYYY-MM-DD, empty if unknown)
    pub start: String,
    pub end: String,
    /// who shared it ("" for the TV's own albums)
    pub owner: String,
    /// newest first ("desc") or oldest first, as its owner set it
    pub newest_first: bool,
}

#[derive(Clone, Serialize)]
pub struct Asset {
    pub id: String,
    pub video: bool,
    /// video length in seconds (0 for photos)
    pub seconds: f64,
    /// when it was taken, local time as written by the camera (YYYY-MM-DDTHH:MM:SS)
    pub taken: String,
    pub width: u32,
    pub height: u32,
    /// "Roma, Italia" when the photo has a position
    pub place: String,
    pub caption: String,
}

#[derive(Serialize)]
pub struct AssetPage {
    pub assets: Vec<Asset>,
    /// the next page to ask for, if there is one
    pub next: Option<u32>,
}

/// "On this day": photos taken on today's date in an earlier year.
#[derive(Clone, Serialize)]
pub struct Memory {
    pub year: i32,
    pub assets: Vec<Asset>,
}

pub struct Immich {
    client: reqwest::Client,
    config: Option<Config>,
    albums: Mutex<Option<(Instant, Vec<Album>)>>,
    memories: Mutex<HashMap<String, (Instant, Vec<Memory>)>>,
    /// the TV user's own id
    me: Mutex<Option<String>>,
}

impl Immich {
    pub fn new() -> Self {
        let config = fs::read_to_string(config_dir().join("immich.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Config>(&s).ok())
            .map(|c| Config { url: c.url.trim_end_matches('/').to_string(), ..c });
        Immich {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("http client"),
            config,
            albums: Mutex::new(None),
            memories: Mutex::new(HashMap::new()),
            me: Mutex::new(None),
        }
    }

    fn cfg(&self) -> Result<&Config, String> {
        self.config.as_ref().ok_or_else(|| t!("Gallery isn't set up"))
    }

    pub fn enabled(&self) -> bool {
        self.config.is_some()
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<Value, String> {
        let c = self.cfg()?;
        req.header("x-api-key", &c.api_key)
            .send()
            .await
            .map_err(|_| t!("The photo server isn't answering"))?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())
    }

    async fn get(&self, path: &str) -> Result<Value, String> {
        let url = format!("{}/api{path}", self.cfg()?.url);
        self.send(self.client.get(url)).await
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        let url = format!("{}/api{path}", self.cfg()?.url);
        self.send(self.client.post(url).json(&body)).await
    }

    /// The TV user's id (asked once).
    async fn me(&self) -> Option<String> {
        if let Some(id) = self.me.lock().unwrap().clone() {
            return Some(id);
        }
        let id = self.get("/users/me").await.ok()?["id"].as_str()?.to_string();
        *self.me.lock().unwrap() = Some(id.clone());
        Some(id)
    }

    /// The TV's albums and the ones shared with it, most recently added to first.
    /// The albums with something in them (the Galleria's).
    pub async fn albums(&self, fresh: bool) -> Result<Vec<Album>, String> {
        Ok(self.all_albums(fresh).await?.into_iter().filter(|a| a.count > 0).collect())
    }

    /// Every album, empty ones too (phones: an album just made is empty until photos arrive).
    pub async fn all_albums(&self, fresh: bool) -> Result<Vec<Album>, String> {
        if !fresh {
            if let Some((at, list)) = &*self.albums.lock().unwrap() {
                if at.elapsed() < TTL {
                    return Ok(list.clone());
                }
            }
        }
        let v = self.get("/albums").await?;
        let me = self.me().await;
        let mut list: Vec<(String, Album)> = v
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| {
                let s = |k: &str| a[k].as_str().unwrap_or_default().to_string();
                let album = Album {
                    id: s("id"),
                    name: s("albumName"),
                    description: s("description"),
                    count: a["assetCount"].as_u64().unwrap_or(0) as u32,
                    cover: s("albumThumbnailAssetId"),
                    start: day(&s("startDate")),
                    end: day(&s("endDate")),
                    owner: a["albumUsers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|u| u["role"] == "owner" && u["user"]["id"].as_str() != me.as_deref())
                        .and_then(|u| u["user"]["name"].as_str())
                        .unwrap_or_default()
                        .to_string(),
                    newest_first: a["order"].as_str() != Some("asc"),
                };
                let changed = a["lastModifiedAssetTimestamp"].as_str().unwrap_or(a["updatedAt"].as_str().unwrap_or_default());
                (changed.to_string(), album)
            })
            .collect();
        list.sort_by(|a, b| b.0.cmp(&a.0));
        let list: Vec<Album> = list.into_iter().map(|(_, a)| a).collect();
        *self.albums.lock().unwrap() = Some((Instant::now(), list.clone()));
        Ok(list)
    }

    async fn search(&self, mut body: Value) -> Result<(Vec<Asset>, Option<u32>), String> {
        body["withExif"] = json!(true);
        let v = self.post("/search/metadata", body).await?;
        let assets = v["assets"]["items"].as_array().into_iter().flatten().filter_map(asset).collect();
        let next = v["assets"]["nextPage"].as_str().and_then(|p| p.parse().ok());
        Ok((assets, next))
    }

    /// One page of an album's photos and videos, in the album's own order.
    pub async fn assets(&self, album: &str, page: u32) -> Result<AssetPage, String> {
        let newest_first = self.albums(false).await?.iter().find(|a| a.id == album).is_none_or(|a| a.newest_first);
        let (assets, next) = self
            .search(json!({
                "albumIds": [album],
                "page": page.max(1),
                "size": PAGE,
                "order": if newest_first { "desc" } else { "asc" },
            }))
            .await?;
        Ok(AssetPage { assets, next })
    }

    /// Photos taken on `today` (YYYY-MM-DD, the TV's local date) in earlier
    /// years, newest year first, from every shared album.
    pub async fn on_this_day(&self, today: &str) -> Result<Vec<Memory>, String> {
        if let Some((at, m)) = self.memories.lock().unwrap().get(today) {
            if at.elapsed() < TTL {
                return Ok(m.clone());
            }
        }
        let (Some(year), Some(md)) = (today.get(..4).and_then(|y| y.parse::<i32>().ok()), today.get(5..10)) else {
            return Err("bad date".into());
        };
        let month = &today[..7];
        let albums = self.albums(false).await?;
        // the album's months tell which years have pictures in this month: only those are searched
        let mut wanted: Vec<(String, i32)> = Vec::new();
        for a in &albums {
            let v = self.get(&format!("/timeline/buckets?albumId={}", a.id)).await?;
            for b in v.as_array().into_iter().flatten() {
                let tb = b["timeBucket"].as_str().unwrap_or_default();
                let Some(y) = tb.get(..4).and_then(|y| y.parse::<i32>().ok()) else { continue };
                if y < year && tb.get(5..7) == month.get(5..7) {
                    wanted.push((a.id.clone(), y));
                }
            }
        }
        let mut by_year: HashMap<i32, Vec<Asset>> = HashMap::new();
        let mut seen = std::collections::HashSet::new();
        for (album, y) in wanted {
            // a day either side: the filter works on UTC times, the date we want is the local one
            let (after, before) = (shift_day(&format!("{y}-{md}"), -1), shift_day(&format!("{y}-{md}"), 1));
            let (assets, _) = self
                .search(json!({
                    "albumIds": [album],
                    "takenAfter": format!("{after}T00:00:00.000Z"),
                    "takenBefore": format!("{before}T23:59:59.999Z"),
                    "size": 100,
                    "order": "asc",
                }))
                .await?;
            for a in assets {
                if a.taken.get(5..10) == Some(md) && a.taken.starts_with(&y.to_string()) && seen.insert(a.id.clone()) {
                    by_year.entry(y).or_default().push(a);
                }
            }
        }
        let mut list: Vec<Memory> = by_year.into_iter().map(|(year, assets)| Memory { year, assets }).collect();
        list.sort_by(|a, b| b.year.cmp(&a.year));
        let mut cache = self.memories.lock().unwrap();
        cache.retain(|d, _| d == today);
        cache.insert(today.to_string(), (Instant::now(), list.clone()));
        Ok(list)
    }

    /// Up to `n` random pictures (no videos) from the shared albums, or from
    /// those of `only` that are still shared (one no longer shared is skipped).
    /// Immich's own `/search/random` finds nothing for a user who owns no
    /// pictures (the TV), so: a couple of random pages of each album, then a
    /// random pick of what they hold.
    pub async fn random(&self, n: u32, only: Option<Vec<String>>) -> Result<Vec<Asset>, String> {
        let albums: Vec<Album> =
            self.albums(false).await?.into_iter().filter(|a| only.as_ref().is_none_or(|o| o.contains(&a.id))).collect();
        if albums.is_empty() {
            return Ok(vec![]);
        }
        let mut seed = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
        let mut rand = move |below: u32| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % below.max(1) as u64) as u32
        };
        let per = n.div_ceil(albums.len() as u32).max(1) as usize;
        let mut out = Vec::new();
        for a in albums {
            // the count includes videos: a page past the photos comes back empty, then the first one is used
            let pages = a.count.div_ceil(PAGE).max(1);
            let first = rand(pages) + 1;
            let mut picks = vec![first];
            if pages > 1 {
                picks.push((first + rand(pages - 1)) % pages + 1);
            }
            let mut got: Vec<Asset> = Vec::new();
            for page in picks {
                let body = |page: u32| json!({ "albumIds": [a.id], "page": page, "size": PAGE, "type": "IMAGE" });
                let (mut v, _) = self.search(body(page)).await?;
                if v.is_empty() && page > 1 && got.is_empty() {
                    (v, _) = self.search(body(1)).await?;
                }
                for x in v {
                    if !got.iter().any(|g| g.id == x.id) {
                        got.push(x);
                    }
                }
            }
            shuffle(&mut got, rand(u32::MAX) as u64 | 1);
            got.truncate(per);
            out.extend(got);
        }
        shuffle(&mut out, rand(u32::MAX) as u64 | 1);
        out.truncate(n as usize);
        Ok(out)
    }

    /// `/<asset id>/<thumbnail|preview>` -> the picture, cached on disk.
    pub async fn image(&self, path: &str) -> Result<(Vec<u8>, String), String> {
        let (id, size) = path.trim_start_matches('/').split_once('/').ok_or("bad image path")?;
        if !is_uuid(id) || !matches!(size, "thumbnail" | "preview") {
            return Err("bad image path".into());
        }
        let dir = cache_dir().join("immich");
        let file = dir.join(format!("{id}.{size}"));
        if let Ok(data) = fs::read(&file) {
            return Ok((data, sniff(&file)));
        }
        let c = self.cfg()?;
        let resp = self
            .client
            .get(format!("{}/api/assets/{id}/thumbnail", c.url))
            .query(&[("size", size)])
            .header("x-api-key", &c.api_key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("image HTTP {}", resp.status()));
        }
        let data = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
        if fs::create_dir_all(&dir).is_ok() {
            let _ = fs::write(&file, &data);
        }
        Ok((data, sniff(&file)))
    }

    // ---- adding photos (phones, see phone.rs) ----

    pub fn can_upload(&self) -> bool {
        self.config.as_ref().is_some_and(|c| c.upload_key.is_some())
    }

    fn upload_key(&self) -> Result<(&Config, &str), String> {
        let c = self.cfg()?;
        let k = c.upload_key.as_deref().ok_or_else(|| t!("Adding photos isn't set up"))?;
        Ok((c, k))
    }

    async fn send_upload(&self, req: reqwest::RequestBuilder) -> Result<Value, String> {
        let (_, key) = self.upload_key()?;
        let r = req
            .header("x-api-key", key)
            .send()
            .await
            .map_err(|_| t!("The photo server isn't answering"))?;
        if !r.status().is_success() {
            let code = r.status();
            let msg = r.json::<Value>().await.ok().and_then(|v| v["message"].as_str().map(str::to_string));
            return Err(msg.unwrap_or_else(|| format!("Immich: HTTP {code}")));
        }
        r.json().await.map_err(|e| e.to_string())
    }

    /// A new album of the TV's, shared with the family (`shareWith`).
    pub async fn create_album(&self, name: &str) -> Result<Album, String> {
        let (c, _) = self.upload_key()?;
        let users: Vec<Value> = c.share_with.iter().map(|id| json!({ "userId": id, "role": "editor" })).collect();
        let mut body = json!({ "albumName": name });
        if !users.is_empty() {
            body["albumUsers"] = json!(users);
        }
        let a = self.send_upload(self.client.post(format!("{}/api/albums", c.url)).json(&body)).await?;
        *self.albums.lock().unwrap() = None;
        Ok(Album {
            id: a["id"].as_str().unwrap_or_default().to_string(),
            name: name.to_string(),
            description: String::new(),
            count: 0,
            cover: String::new(),
            start: String::new(),
            end: String::new(),
            owner: String::new(),
            newest_first: true,
        })
    }

    /// Uploads a file as the TV user. Returns the asset id and whether Immich
    /// already had it (then nothing new is stored).
    pub async fn upload(
        &self,
        file: &std::path::Path,
        name: &str,
        mime: &str,
        modified: &str,
    ) -> Result<(String, bool), String> {
        let (c, _) = self.upload_key()?;
        let f = tokio::fs::File::open(file).await.map_err(|e| e.to_string())?;
        let len = f.metadata().await.map_err(|e| e.to_string())?.len();
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(f));
        let part = reqwest::multipart::Part::stream_with_length(body, len)
            .file_name(name.to_string())
            .mime_str(mime)
            .map_err(|e| e.to_string())?;
        // the file's own date is only a fallback: Immich reads the photo's EXIF / the video's metadata
        let form = reqwest::multipart::Form::new()
            .text("fileCreatedAt", modified.to_string())
            .text("fileModifiedAt", modified.to_string())
            .text("filename", name.to_string())
            .part("assetData", part);
        let req = self.client.post(format!("{}/api/assets", c.url)).multipart(form).timeout(Duration::from_secs(1800));
        let v = self.send_upload(req).await?;
        let id = v["id"].as_str().ok_or("Immich: no asset id")?.to_string();
        Ok((id, v["status"] == "duplicate"))
    }

    pub async fn add_to_album(&self, album: &str, ids: &[String]) -> Result<(), String> {
        let (c, _) = self.upload_key()?;
        if !is_uuid(album) {
            return Err("bad album id".into());
        }
        self.send_upload(self.client.put(format!("{}/api/albums/{album}/assets", c.url)).json(&json!({ "ids": ids })))
            .await?;
        *self.albums.lock().unwrap() = None;
        Ok(())
    }

    /// Photos / videos by id (what a phone just sent), in that order.
    pub async fn assets_by_id(&self, ids: &[String]) -> Vec<Asset> {
        let mut out = Vec::new();
        for id in ids.iter().filter(|id| is_uuid(id)) {
            if let Some(a) = self.get(&format!("/assets/{id}")).await.ok().as_ref().and_then(asset) {
                out.push(a);
            }
        }
        out
    }

    /// A photo's or video's original file, for a phone to view or play (`range`: the player's Range header).
    pub async fn original(&self, id: &str, range: Option<&str>) -> Result<reqwest::Response, String> {
        if !is_uuid(id) {
            return Err("bad asset id".into());
        }
        let c = self.cfg()?;
        let mut req = self.client.get(format!("{}/api/assets/{id}/original", c.url)).header("x-api-key", &c.api_key);
        if let Some(range) = range {
            req = req.header("range", range);
        }
        req.send().await.map_err(|e| e.to_string())
    }

    /// What mpv needs to play a video: its URL and the key header.
    pub fn video(&self, id: &str) -> Result<(String, String), String> {
        if !is_uuid(id) {
            return Err("bad asset id".into());
        }
        let c = self.cfg()?;
        Ok((format!("{}/api/assets/{id}/original", c.url), format!("x-api-key: {}", c.api_key)))
    }
}

fn asset(a: &Value) -> Option<Asset> {
    let id = a["id"].as_str()?.to_string();
    let ex = &a["exifInfo"];
    let s = |v: &Value| v.as_str().unwrap_or_default().trim().to_string();
    let place = [s(&ex["city"]), s(&ex["country"])].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(", ");
    // the photo's own clock time: localDateTime is the camera's time, labelled UTC
    let taken = a["localDateTime"].as_str().or(a["fileCreatedAt"].as_str()).unwrap_or_default();
    let (mut w, mut h) = (a["width"].as_u64().unwrap_or(0), a["height"].as_u64().unwrap_or(0));
    if w == 0 || h == 0 {
        (w, h) = (ex["exifImageWidth"].as_u64().unwrap_or(0), ex["exifImageHeight"].as_u64().unwrap_or(0));
        // EXIF sizes are before rotation
        if matches!(ex["orientation"].as_str(), Some("5" | "6" | "7" | "8")) {
            (w, h) = (h, w);
        }
    }
    Some(Asset {
        id,
        video: a["type"].as_str() == Some("VIDEO"),
        seconds: duration(&a["duration"]),
        taken: taken.get(..19).unwrap_or(taken).to_string(),
        width: w as u32,
        height: h as u32,
        place,
        caption: s(&ex["description"]),
    })
}

/// Immich gives video length in ms (v3) or as "H:MM:SS.ffffff" (older versions).
fn duration(v: &Value) -> f64 {
    if let Some(ms) = v.as_f64() {
        return ms / 1000.0;
    }
    let Some(s) = v.as_str() else { return 0.0 };
    s.split(':').fold(0.0, |acc, part| acc * 60.0 + part.parse::<f64>().unwrap_or(0.0))
}

fn day(iso: &str) -> String {
    iso.get(..10).unwrap_or_default().to_string()
}

/// YYYY-MM-DD moved by `days` (only ever ±1 here).
fn shift_day(date: &str, days: i64) -> String {
    let p: Vec<i64> = date.split('-').filter_map(|x| x.parse().ok()).collect();
    let [y, m, d] = p[..] else { return date.to_string() };
    // days since the epoch (civil calendar), then back
    let n = days_from_civil(y, m, d) + days;
    let (y, m, d) = civil_from_days(n);
    format!("{y:04}-{m:02}-{d:02}")
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(crate) fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Thumbnails are WebP, previews JPEG (Immich's defaults): tell them apart by their bytes.
fn sniff(file: &std::path::Path) -> String {
    let mut head = [0u8; 12];
    let _ = fs::File::open(file).and_then(|mut f| std::io::Read::read(&mut f, &mut head));
    if &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" { "image/webp" } else { "image/jpeg" }.into()
}

fn shuffle<T>(v: &mut [T], mut seed: u64) {
    for i in (1..v.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        v.swap(i, (seed % (i as u64 + 1)) as usize);
    }
}

/// Drop pictures not looked at for a while (opening one refreshes its time).
pub fn prune_cache() {
    let Ok(dir) = fs::read_dir(cache_dir().join("immich")) else { return };
    let max = Duration::from_secs(CACHE_DAYS * 86_400);
    for e in dir.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.accessed().or_else(|_| m.modified()))
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > max);
        if old {
            let _ = fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_roll_over() {
        assert_eq!(shift_day("2024-03-01", -1), "2024-02-29");
        assert_eq!(shift_day("2023-12-31", 1), "2024-01-01");
        assert_eq!(shift_day("1970-01-01", -1), "1969-12-31");
    }

    #[test]
    fn durations() {
        assert_eq!(duration(&json!(12000)), 12.0);
        assert_eq!(duration(&json!("0:01:02.500000")), 62.5);
        assert_eq!(duration(&Value::Null), 0.0);
    }

    #[test]
    fn asset_fields() {
        let a = asset(&json!({
            "id": "00000000-0000-4000-8000-000000000001", "type": "IMAGE", "duration": null,
            "localDateTime": "2025-05-09T15:30:00.000Z", "width": null, "height": null,
            "exifInfo": {"exifImageWidth": 4000, "exifImageHeight": 3000, "orientation": "6", "city": "Roma", "country": "Italia", "description": " Al mare "}
        }))
        .unwrap();
        assert_eq!((a.width, a.height), (3000, 4000));
        assert_eq!(a.taken, "2025-05-09T15:30:00");
        assert_eq!(a.place, "Roma, Italia");
        assert_eq!(a.caption, "Al mare");
    }
}
