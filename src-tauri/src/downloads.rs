//! How far along a requested film or series is: the progress bar on the
//! request page and on "In arrivo" search results.
//!
//! Follows one TMDB id through Radarr / Sonarr (is it searched, grabbed,
//! importing?) and, for what is downloading, asks RDT-Client directly. Its
//! qBittorrent imitation (what Radarr, and so Seerr, see) averages the two
//! steps of a debrid download into one number: TorBox fetching the torrent in
//! its cloud (instant when cached) and the server downloading the files from
//! TorBox. So a cached film jumps to 50% and then sits at 99%. Read apart, the
//! steps give an honest bar.
//!
//! Config: the `radarr`, `sonarr` and `rdt` (`{"url", "username", "password"}`)
//! entries of `~/.config/azototv/arr.json` (server only, mode 600). Without
//! `rdt` the bar falls back to Radarr's / Sonarr's numbers. Nothing secret
//! leaves Rust: JS only gets the stage, fractions, speed and ETA.

use std::{
    collections::HashMap,
    fs,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::jellyfin::config_dir;

#[derive(Deserialize, Clone)]
struct Service {
    url: String,
    #[serde(rename = "apiKey")]
    api_key: String,
}

#[derive(Deserialize, Clone)]
struct Rdt {
    url: String,
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct Config {
    radarr: Option<Service>,
    sonarr: Option<Service>,
    rdt: Option<Rdt>,
}

/// Where a request is. Stages in order: search → cloud → download → unpack →
/// import; `unreleased`, `wait` and `error` are side tracks.
#[derive(Serialize, Clone, Debug)]
pub struct Progress {
    /// none | unreleased | search | wait | cloud | download | unpack | import | error
    pub stage: &'static str,
    /// 0..1, the whole way from request to library (the bar)
    pub overall: f64,
    /// bytes/s of what is moving now
    pub speed: Option<u64>,
    /// seconds until the current step is done, when it can be told
    pub eta: Option<u64>,
    /// an Italian line for the side tracks (why it waits, what went wrong)
    pub note: Option<String>,
    /// series: (episodes in the library, episodes asked for)
    pub episodes: Option<(u64, u64)>,
}

impl Progress {
    fn new(stage: &'static str, overall: f64) -> Self {
        Progress { stage, overall, speed: None, eta: None, note: None, episodes: None }
    }
}

// The bar's share of each step: the TorBox cloud is often instant (cached),
// the download to the server is the long part.
const CLOUD: (f64, f64) = (0.05, 0.25);
const DOWNLOAD: (f64, f64) = (0.30, 0.65);
const UNPACK: f64 = 0.95;
const IMPORT: f64 = 0.97;
const SEARCH: f64 = 0.02;

/// Stage rank, to tell a series' most advanced download.
fn rank(stage: &str) -> u8 {
    match stage {
        "download" => 6,
        "unpack" => 5,
        "cloud" => 4,
        "import" => 3,
        "error" => 2,
        "wait" => 1,
        _ => 0,
    }
}

pub struct Downloads {
    client: reqwest::Client,
    cfg: Option<Config>,
    /// RDT-Client's login cookie
    cookie: Mutex<Option<String>>,
    /// RDT-Client's torrent list, shared by the calls of one refresh (several cards poll at once)
    torrents: tokio::sync::Mutex<Option<(Instant, Vec<Value>)>>,
    /// Sonarr's series list (tmdb id → series), refreshed every minute
    series: tokio::sync::Mutex<Option<(Instant, Vec<Value>)>>,
    /// `AZOTOTV_DL_DEMO=1`: made-up progress for UI work on a PC without arr.json
    demo: Option<Instant>,
    demo_seen: Mutex<HashMap<String, Instant>>,
}

/// Radarr's `timeleft`: `hh:mm:ss` or `d.hh:mm:ss`.
fn parse_timeleft(s: &str) -> Option<u64> {
    let (days, rest) = match s.split_once('.') {
        Some((d, r)) if !d.contains(':') => (d.parse::<u64>().ok()?, r),
        _ => (0, s),
    };
    let mut p = rest.split(':').map(|x| x.split('.').next().unwrap_or(x).parse::<u64>());
    let (h, m, sec) = (p.next()?.ok()?, p.next()?.ok()?, p.next()?.ok()?);
    Some(days * 86_400 + h * 3600 + m * 60 + sec)
}

/// RFC 3339 to unix seconds (enough for "how long ago" / "which day").
fn unix(s: &str) -> Option<i64> {
    let (d, t) = s.split_once('T')?;
    let mut d = d.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = t.trim_end_matches('Z').split(':');
    let (hh, mm) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + (153 * m + 2) / 5 + day - 1;
    Some((era * 146_097 + doe - 719_468) * 86_400 + hh * 3600 + mm * 60)
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// `2026-12-03` → `3/12/2026`
fn it_date(s: &str) -> String {
    let d: Vec<&str> = s.get(..10).unwrap_or(s).split('-').collect();
    match d.as_slice() {
        [y, m, day] => format!("{}/{}/{y}", day.trim_start_matches('0'), m.trim_start_matches('0')),
        _ => s.to_string(),
    }
}

/// A JSON field as a number, whether the API sent a number or an enum name.
fn status_is(v: &Value, num: u64, name: &str) -> bool {
    v.as_u64() == Some(num) || v.as_str().is_some_and(|s| s.eq_ignore_ascii_case(name))
}

impl Downloads {
    pub fn new() -> Self {
        let cfg = fs::read_to_string(config_dir().join("arr.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Config>(&s).map_err(|e| eprintln!("[downloads] arr.json: {e}")).ok());
        Downloads {
            client: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("http client"),
            cfg,
            cookie: Mutex::new(None),
            torrents: tokio::sync::Mutex::new(None),
            series: tokio::sync::Mutex::new(None),
            demo: std::env::var("AZOTOTV_DL_DEMO").is_ok().then(Instant::now),
            demo_seen: Mutex::new(HashMap::new()),
        }
    }

    async fn arr(&self, svc: &Service, path_query: &str) -> Result<Value, String> {
        self.client
            .get(format!("{}{path_query}", svc.url.trim_end_matches('/')))
            .header("X-Api-Key", &svc.api_key)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())
    }

    async fn rdt_login(&self, rdt: &Rdt) -> Result<String, String> {
        let resp = self
            .client
            .post(format!("{}/Api/Authentication/Login", rdt.url.trim_end_matches('/')))
            .json(&serde_json::json!({ "userName": rdt.username, "password": rdt.password }))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| format!("RDT-Client login: {e}"))?;
        let cookie = resp
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok()?.split(';').next().map(str::to_string))
            .collect::<Vec<_>>()
            .join("; ");
        *self.cookie.lock().unwrap() = Some(cookie.clone());
        Ok(cookie)
    }

    /// GET from RDT-Client's own API, logging in (again) when needed.
    async fn rdt(&self, path: &str) -> Result<Value, String> {
        let rdt = self.cfg.as_ref().and_then(|c| c.rdt.as_ref()).ok_or("no rdt")?;
        for attempt in 0..2 {
            let cookie = match self.cookie.lock().unwrap().clone() {
                Some(c) if attempt == 0 => c,
                _ => String::new(),
            };
            let cookie = if cookie.is_empty() { self.rdt_login(rdt).await? } else { cookie };
            let resp = self
                .client
                .get(format!("{}{path}", rdt.url.trim_end_matches('/')))
                .header(reqwest::header::COOKIE, cookie)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            // signed out: 401, or a redirect to the login page (HTML, not JSON)
            if resp.status().is_success() {
                if let Ok(v) = resp.json::<Value>().await {
                    return Ok(v);
                }
            }
            *self.cookie.lock().unwrap() = None;
        }
        Err("RDT-Client: accesso non riuscito".into())
    }

    /// RDT-Client's torrents, at most 2 s old.
    async fn torrents(&self) -> Option<Vec<Value>> {
        let mut t = self.torrents.lock().await;
        if let Some((at, list)) = t.as_ref() {
            if at.elapsed() < Duration::from_secs(2) {
                return Some(list.clone());
            }
        }
        let list = self.rdt("/Api/Torrents").await.ok()?.as_array()?.clone();
        *t = Some((Instant::now(), list.clone()));
        Some(list)
    }

    /// One download (a torrent hash) as RDT-Client sees it, or None when RDT-Client
    /// can't be asked or doesn't know it.
    async fn torrent(&self, hash: &str) -> Option<Progress> {
        let list = self.torrents().await?;
        let t = list.iter().find(|t| t["hash"].as_str().is_some_and(|h| h.eq_ignore_ascii_case(hash)))?;
        if let Some(e) = t["error"].as_str().filter(|e| !e.trim().is_empty()) {
            let mut p = Progress::new("error", DOWNLOAD.0);
            p.note = Some(format!("RDT-Client: {e}"));
            return Some(p);
        }
        if !t["completed"].is_null() {
            return Some(Progress::new("import", IMPORT));
        }
        let rd = (t["rdProgress"].as_f64().unwrap_or(0.0) / 100.0).clamp(0.0, 1.0);
        let size = t["rdSize"].as_u64().unwrap_or(0);
        if t["downloadsCount"].as_u64().unwrap_or(0) == 0 {
            if rd < 1.0 {
                let mut p = Progress::new("cloud", CLOUD.0 + CLOUD.1 * rd);
                let speed = t["rdSpeed"].as_u64().filter(|s| *s > 0);
                p.speed = speed;
                p.eta = speed.filter(|_| size > 0).map(|s| ((1.0 - rd) * size as f64 / s as f64) as u64);
                if status_is(&t["rdStatus"], 0, "Queued") {
                    p.stage = "wait";
                    p.note = Some("In coda su RDT-Client".into());
                } else if t["rdSeeders"].as_i64() == Some(0) && status_is(&t["rdStatus"], 3, "Downloading") {
                    p.note = Some("TorBox aspetta qualcuno che condivida il file".into());
                }
                return Some(p);
            }
            // in the cloud, the download to the server is about to start
            let mut p = Progress::new("download", DOWNLOAD.0);
            p.note = Some("In coda".into());
            return Some(p);
        }
        // the files coming down to the server: per-download stats are only in the detail
        let id = t["torrentId"].as_str()?;
        let detail = self.rdt(&format!("/Api/Torrents/Get/{id}")).await.ok()?;
        let downloads = detail["downloads"].as_array().cloned().unwrap_or_default();
        let n = downloads.len().max(1) as f64;
        let (mut frac, mut speed, mut unpacking, mut started) = (0.0, 0u64, false, false);
        for d in &downloads {
            if !d["downloadFinished"].is_null() || !d["completed"].is_null() {
                frac += 1.0;
            } else {
                let (done, total) = (d["bytesDone"].as_f64().unwrap_or(0.0), d["bytesTotal"].as_f64().unwrap_or(0.0));
                if total > 0.0 {
                    frac += (done / total).clamp(0.0, 1.0);
                }
                started |= !d["downloadStarted"].is_null();
                speed += d["speed"].as_u64().unwrap_or(0);
            }
            unpacking |= !d["unpackingStarted"].is_null() && d["unpackingFinished"].is_null();
        }
        let frac = frac / n;
        if unpacking {
            return Some(Progress::new("unpack", UNPACK));
        }
        let mut p = Progress::new("download", DOWNLOAD.0 + DOWNLOAD.1 * frac);
        if speed > 0 {
            p.speed = Some(speed);
            if size > 0 {
                p.eta = Some(((1.0 - frac) * size as f64 / speed as f64) as u64);
            }
        } else if !started {
            p.note = Some("In coda".into());
        }
        Some(p)
    }

    /// One Radarr / Sonarr queue record (a file being fetched).
    async fn record(&self, r: &Value) -> Progress {
        let state = r["trackedDownloadState"].as_str().unwrap_or("");
        let status = r["status"].as_str().unwrap_or("");
        let warn = matches!(r["trackedDownloadStatus"].as_str(), Some("warning" | "error"));
        let messages = || {
            let m: Vec<String> = r["statusMessages"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|s| s["messages"].as_array().cloned().unwrap_or_default())
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect();
            if m.is_empty() { r["errorMessage"].as_str().map(str::to_string) } else { Some(m.join(" · ")) }
        };
        match state {
            "importPending" | "importBlocked" if warn => {
                let mut p = Progress::new("error", IMPORT);
                p.note = Some(format!("Scaricato, ma non riesco a metterlo in libreria: {}", messages().unwrap_or_default()));
                return p;
            }
            "importPending" | "importing" | "imported" => return Progress::new("import", IMPORT),
            "failedPending" | "failed" => {
                let mut p = Progress::new("error", SEARCH);
                p.note = Some(format!("Download non riuscito, ne cerco un altro. {}", messages().unwrap_or_default()));
                return p;
            }
            _ => {}
        }
        if status == "delay" {
            let mut p = Progress::new("wait", SEARCH);
            p.note = Some("Trovato, aspetto qualche minuto una versione migliore".into());
            return p;
        }
        let hash = r["downloadId"].as_str().unwrap_or("");
        if let Some(p) = self.torrent(hash).await {
            return p;
        }
        // no RDT-Client: what Radarr / Sonarr know (the averaged number)
        let size = r["size"].as_f64().unwrap_or(0.0);
        let left = r["sizeleft"].as_f64().unwrap_or(0.0);
        let frac = if size > 0.0 { (1.0 - left / size).clamp(0.0, 1.0) } else { 0.0 };
        let mut p = Progress::new("download", DOWNLOAD.0 + DOWNLOAD.1 * frac);
        p.eta = r["timeleft"].as_str().and_then(parse_timeleft);
        if warn {
            p.note = messages();
        }
        p
    }

    /// Progress of a film (`media` "movie") or series ("tv") by TMDB id;
    /// None when this machine has no arr.json (the PC) or neither knows it.
    pub async fn progress(&self, media: &str, tmdb: u64) -> Result<Option<Progress>, String> {
        if let Some(start) = self.demo {
            return Ok(Some(self.demo_progress(media, tmdb, start)));
        }
        let Some(cfg) = &self.cfg else { return Ok(None) };
        if media == "movie" {
            let Some(radarr) = &cfg.radarr else { return Ok(None) };
            self.movie(radarr, tmdb).await
        } else {
            let Some(sonarr) = &cfg.sonarr else { return Ok(None) };
            self.show(sonarr, tmdb).await
        }
    }

    async fn movie(&self, radarr: &Service, tmdb: u64) -> Result<Option<Progress>, String> {
        let list = self.arr(radarr, &format!("/api/v3/movie?tmdbId={tmdb}")).await?;
        let Some(m) = list.as_array().and_then(|a| a.first()) else { return Ok(None) };
        let id = m["id"].as_u64().unwrap_or(0);
        let queue = self.arr(radarr, &format!("/api/v3/queue/details?movieId={id}")).await?;
        let records = queue.as_array().cloned().unwrap_or_default();
        if let Some(r) = records.first() {
            return Ok(Some(self.record(r).await));
        }
        if m["hasFile"].as_bool() == Some(true) {
            return Ok(Some(Progress::new("import", IMPORT)));
        }
        if m["isAvailable"].as_bool() == Some(false) {
            let mut p = Progress::new("unreleased", 0.0);
            let date = ["digitalRelease", "physicalRelease"]
                .iter()
                .filter_map(|k| m[*k].as_str())
                .filter(|d| unix(d).is_some_and(|t| t > now()))
                .min_by_key(|d| unix(d));
            p.note = Some(match date {
                Some(d) => format!("Non è ancora uscito in digitale: arriverà da solo verso il {}", it_date(d)),
                None => "Non è ancora uscito in digitale: arriverà da solo quando esce".into(),
            });
            return Ok(Some(p));
        }
        Ok(Some(searching(m["added"].as_str())))
    }

    /// Sonarr's series with this TMDB id (Sonarr is keyed by TVDB, so the list is searched).
    async fn find_series(&self, sonarr: &Service, tmdb: u64) -> Result<Option<Value>, String> {
        let mut s = self.series.lock().await;
        let fresh = s.as_ref().is_some_and(|(at, _)| at.elapsed() < Duration::from_secs(60));
        let find = |list: &[Value]| list.iter().find(|x| x["tmdbId"].as_u64() == Some(tmdb)).cloned();
        if fresh {
            if let Some(hit) = find(&s.as_ref().unwrap().1) {
                return Ok(Some(hit));
            }
        }
        // stale, or just requested (not in the cached list yet)
        let list = self.arr(sonarr, "/api/v3/series").await?.as_array().cloned().unwrap_or_default();
        let hit = find(&list);
        *s = Some((Instant::now(), list));
        Ok(hit)
    }

    async fn show(&self, sonarr: &Service, tmdb: u64) -> Result<Option<Progress>, String> {
        let Some(series) = self.find_series(sonarr, tmdb).await? else { return Ok(None) };
        let id = series["id"].as_u64().unwrap_or(0);
        // fresh statistics (the cached list's are up to a minute old)
        let fresh = self.arr(sonarr, &format!("/api/v3/series/{id}")).await.unwrap_or(series);
        let have = fresh["statistics"]["episodeFileCount"].as_u64().unwrap_or(0);
        let want = fresh["statistics"]["episodeCount"].as_u64().unwrap_or(0);
        let queue = self.arr(sonarr, &format!("/api/v3/queue/details?seriesId={id}")).await?;
        let records = queue.as_array().cloned().unwrap_or_default();

        // one progress per download (a season pack is one download for many episodes)
        let mut per: HashMap<String, Progress> = HashMap::new();
        let mut queued_eps = 0u64;
        let mut sum = 0.0;
        for r in &records {
            let key = r["downloadId"].as_str().unwrap_or("").to_string();
            if !per.contains_key(&key) {
                let p = self.record(r).await;
                per.insert(key.clone(), p);
            }
            queued_eps += 1;
            sum += per[&key].overall;
        }

        let mut p = if let Some(best) = per.values().max_by_key(|p| rank(p.stage)) {
            let mut p = best.clone();
            p.speed = per.values().filter_map(|x| x.speed).reduce(|a, b| a + b);
            p
        } else if want > 0 && have >= want {
            Progress::new("import", IMPORT)
        } else {
            searching(fresh["added"].as_str())
        };
        if want > 0 {
            // episodes in the library count whole, the ones on their way by their download
            let missing_not_queued = want.saturating_sub(have + queued_eps) as f64;
            let base = if per.is_empty() { p.overall } else { SEARCH };
            p.overall = ((have as f64 + sum + missing_not_queued * base) / want as f64).clamp(0.0, 1.0);
            p.episodes = Some((have, want));
            if per.len() > 1 {
                // the ETA of one download says little about all of them
                p.eta = None;
            }
        }
        Ok(Some(p))
    }

    /// Demo for UI work: every title walks search → cloud → download → import in ~70 s.
    fn demo_progress(&self, media: &str, tmdb: u64, start: Instant) -> Progress {
        let key = format!("{media}-{tmdb}");
        let t0 = *self.demo_seen.lock().unwrap().entry(key).or_insert(start);
        let t = (t0.elapsed().as_secs_f64() + (tmdb % 50) as f64) % 80.0;
        let mut p = if t < 6.0 {
            searching(None)
        } else if t < 16.0 {
            let f = (t - 6.0) / 10.0;
            let mut p = Progress::new("cloud", CLOUD.0 + CLOUD.1 * f);
            p.speed = Some(38_000_000);
            p.eta = Some(((1.0 - f) * 10.0) as u64);
            p
        } else if t < 66.0 {
            let f = (t - 16.0) / 50.0;
            let mut p = Progress::new("download", DOWNLOAD.0 + DOWNLOAD.1 * f);
            p.speed = Some(11_400_000);
            p.eta = Some(((1.0 - f) * 50.0 * 6.0) as u64);
            p
        } else {
            Progress::new("import", IMPORT)
        };
        if media == "tv" {
            p.episodes = Some((3, 10));
        }
        p
    }
}

/// No download yet: Radarr / Sonarr are searching the indexers.
fn searching(added: Option<&str>) -> Progress {
    let mut p = Progress::new("search", SEARCH);
    if added.and_then(unix).is_some_and(|t| now() - t > 3600) {
        p.note = Some("Non ho ancora trovato una versione da scaricare: riprovo da solo ogni tanto".into());
    }
    p
}

/// `azototv --dl-check movie|tv <tmdb id>`: print what the bar would show.
pub async fn check(media: &str, tmdb: u64) {
    let d = Downloads::new();
    match d.rdt("/Api/Torrents").await {
        Ok(v) => eprintln!("RDT-Client: ok, {} torrent(s)", v.as_array().map_or(0, Vec::len)),
        Err(e) => eprintln!("RDT-Client: {e} (the bar falls back to Radarr/Sonarr numbers)"),
    }
    match d.progress(media, tmdb).await {
        Ok(p) => println!("{p:#?}"),
        Err(e) => eprintln!("error: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeleft() {
        assert_eq!(parse_timeleft("00:04:12"), Some(252));
        assert_eq!(parse_timeleft("1.02:00:00"), Some(93_600));
        assert_eq!(parse_timeleft("00:00:05.1234"), Some(5));
    }

    #[test]
    fn dates() {
        assert_eq!(it_date("2026-12-03T00:00:00Z"), "3/12/2026");
        assert_eq!(unix("1970-01-02T00:00:00Z"), Some(86_400));
    }
}
