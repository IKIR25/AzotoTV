//! Films and episodes that just arrived. Polls Radarr's and Sonarr's import
//! history every 30 s (no webhooks to set up, and whatever finished while
//! AzotoTV was off is caught up on start), asks Jellyfin to scan the library
//! that got the file right away instead of at its next scheduled scan, waits
//! until Jellyfin lists it (with its poster), then posts a notification naming
//! who asked for it in Seerr. A better copy of something already in the
//! library (an upgrade: Radarr / Sonarr had imported it before) is scanned
//! too, but not announced.
//!
//! Config `~/.config/azototv/arr.json` (mode 600, keys stay in Rust):
//! `{"jellyfinKey": "...", "radarr": {"url", "apiKey"}, "sonarr": {"url", "apiKey"}}`
//! (`jellyfinKey`: an API key, scanning a library needs an admin).

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::Deserialize;
use serde_json::Value;

use crate::{
    jellyfin::{Jf, config_dir},
    notify::{Notification, Notify, now_ms},
    seerr::Seerr,
};

const POLL: Duration = Duration::from_secs(30);
/// How long to wait for Jellyfin to list a new file (and fetch its poster).
const FIND_FOR: Duration = Duration::from_secs(6 * 60);
/// Imports older than this when first seen (AzotoTV was off) only go to the list.
const TOAST_WITHIN: u64 = 2 * 3600 * 1000;
/// Jellyfin had the item this long before the import: it's an upgrade.
const UPGRADE_SLACK: u64 = 3600 * 1000;

#[derive(Deserialize, Clone)]
struct Service {
    url: String,
    #[serde(rename = "apiKey")]
    api_key: String,
}

#[derive(Deserialize)]
struct Config {
    #[serde(rename = "jellyfinKey")]
    jellyfin_key: Option<String>,
    radarr: Option<Service>,
    sonarr: Option<Service>,
}

/// One imported file.
struct Import {
    /// the history record
    record: u64,
    episode_id: Option<u64>,
    date: u64,
    path: String,
    season: Option<u64>,
    episode: Option<u64>,
}

/// A film, or a series' episodes imported together (a season pack is one notification).
struct Group {
    movie: bool,
    /// Radarr's movieId / Sonarr's seriesId
    key: u64,
    title: String,
    year: Option<u64>,
    tmdb: Option<u64>,
    imports: Vec<Import>,
}

pub struct Arr {
    client: reqwest::Client,
    cfg: Config,
    jf: Arc<Jf>,
    seerr: Arc<Seerr>,
    notify: Arc<Notify>,
    /// `--arr-check`: print instead of posting, keep the cursors
    dry: bool,
    /// last error per service, logged once (not every 30 s while it is down)
    errors: Mutex<HashMap<&'static str, String>>,
}

/// RFC 3339 (`2026-10-07T14:30:00Z`, fractions allowed) to unix ms.
fn parse_date(s: &str) -> Option<u64> {
    let (d, t) = s.split_once('T')?;
    let mut d = d.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let t = t.trim_end_matches('Z');
    let mut t = t.split(':');
    let (hh, mm) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let ss = t.next().and_then(|x| x.split('.').next()?.parse::<i64>().ok()).unwrap_or(0);
    // days from civil (Howard Hinnant)
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + hh * 3600 + mm * 60 + ss) * 1000) as u64)
}

fn load() -> Option<Config> {
    let s = fs::read_to_string(config_dir().join("arr.json")).ok()?;
    serde_json::from_str(&s).map_err(|e| eprintln!("[arr] arr.json: {e}")).ok()
}

impl Arr {
    pub fn new(jf: Arc<Jf>, seerr: Arc<Seerr>, notify: Arc<Notify>, dry: bool) -> Option<Arr> {
        let cfg = load()?;
        let client = reqwest::Client::builder().timeout(Duration::from_secs(20)).build().ok()?;
        Some(Arr { client, cfg, jf, seerr, notify, dry, errors: Mutex::new(HashMap::new()) })
    }

    /// Poll forever (spawned from setup).
    pub fn start(self) {
        let me = Arc::new(self);
        tauri::async_runtime::spawn(async move {
            loop {
                me.tick(None).await;
                tokio::time::sleep(POLL).await;
            }
        });
    }

    /// `--arr-check <n>`: go through the last n imports of each service once,
    /// printing what would be posted (Jellyfin does get the library scans).
    pub async fn check(self, n: usize) {
        let me = Arc::new(self);
        me.tick(Some(n)).await;
        // the per-group lookups run as tasks: wait until they are done
        let until = std::time::Instant::now() + FIND_FOR + Duration::from_secs(15);
        while Arc::strong_count(&me) > 1 && std::time::Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    // ---- Radarr / Sonarr ----

    async fn get(&self, svc: &Service, path_query: &str) -> Result<Value, String> {
        let url = format!("{}/api/v3{path_query}", svc.url.trim_end_matches('/'));
        self
            .client
            .get(url)
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

    async fn history(&self, svc: &Service, movie: bool) -> Result<Vec<Value>, String> {
        let include = if movie { "includeMovie=true" } else { "includeSeries=true&includeEpisode=true" };
        // eventType 3 = downloadFolderImported
        let v = self
            .get(svc, &format!("/history?page=1&pageSize=50&sortKey=date&sortDirection=descending&eventType=3&{include}"))
            .await?;
        Ok(v["records"].as_array().cloned().unwrap_or_default())
    }

    /// Radarr / Sonarr imported this film / episode before: the import is an upgrade.
    async fn imported_before(&self, g: &Group, i: &Import) -> bool {
        let (svc, q) = if g.movie {
            (&self.cfg.radarr, format!("/history/movie?movieId={}&eventType=3", g.key))
        } else {
            (&self.cfg.sonarr, format!("/history/series?seriesId={}&eventType=3", g.key))
        };
        let Some(svc) = svc else { return false };
        let Ok(v) = self.get(svc, &q).await else { return false };
        v.as_array().into_iter().flatten().any(|r| {
            r["id"].as_u64().is_some_and(|id| id < i.record)
                && (g.movie || r["episodeId"].as_u64() == i.episode_id)
        })
    }

    async fn tick(self: &Arc<Self>, replay: Option<usize>) {
        let mut groups: Vec<(Group, u64)> = Vec::new();
        for (key, svc, movie) in [("radarr", &self.cfg.radarr, true), ("sonarr", &self.cfg.sonarr, false)] {
            let Some(svc) = svc else { continue };
            let recs = match self.history(svc, movie).await {
                Ok(r) => {
                    if self.errors.lock().unwrap().remove(key).is_some() {
                        eprintln!("[arr] {key}: reachable again");
                    }
                    r
                }
                Err(e) => {
                    if self.errors.lock().unwrap().insert(key, e.clone()).as_ref() != Some(&e) {
                        eprintln!("[arr] {key}: {e}");
                    }
                    continue;
                }
            };
            let top = recs.iter().filter_map(|r| r["id"].as_u64()).max().unwrap_or(0);
            let fresh: Vec<&Value> = match replay {
                Some(n) => recs.iter().take(n).collect(),
                None => {
                    let Some(cursor) = self.notify.cursor(key) else {
                        // first run: start from now, don't announce the whole history
                        self.notify.set_cursor(key, top);
                        continue;
                    };
                    if top <= cursor {
                        continue;
                    }
                    self.notify.set_cursor(key, top);
                    recs.iter().filter(|r| r["id"].as_u64().is_some_and(|id| id > cursor)).collect()
                }
            };
            for g in group(&fresh, movie) {
                groups.push((g, 0));
            }
        }
        if groups.is_empty() {
            return;
        }
        // one scan per library, then each group waits for its files
        let libs = self.libraries().await;
        for (g, lib) in groups.iter_mut() {
            *lib = libs
                .iter()
                .position(|(_, locs)| locs.iter().any(|l| g.imports[0].path.starts_with(l.as_str())))
                .map_or(usize::MAX as u64, |i| i as u64);
        }
        let mut scanned: Vec<u64> = Vec::new();
        for (_, lib) in &groups {
            if let Some((id, _)) = libs.get(*lib as usize) {
                if !scanned.contains(lib) {
                    scanned.push(*lib);
                    self.refresh(id).await;
                }
            }
        }
        for (g, lib) in groups {
            let me = self.clone();
            let lib = libs.get(lib as usize).map(|(id, _)| id.clone());
            tauri::async_runtime::spawn(async move { me.announce(g, lib, replay.is_some()).await });
        }
    }

    // ---- Jellyfin (with the API key) ----

    async fn jf(&self, method: reqwest::Method, path: &str, query: &[(&str, &str)]) -> Result<Value, String> {
        let key = self.cfg.jellyfin_key.as_deref().ok_or("no jellyfinKey")?;
        let base = self.jf.server().ok_or("no server")?;
        let r = self
            .client
            .request(method, format!("{base}{path}"))
            .header("Authorization", format!("MediaBrowser Client=\"AzotoTV\", Token=\"{key}\""))
            .query(query)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let b = r.bytes().await.map_err(|e| e.to_string())?;
        if b.is_empty() { Ok(Value::Null) } else { serde_json::from_slice(&b).map_err(|e| e.to_string()) }
    }

    /// (library item id, its folders)
    async fn libraries(&self) -> Vec<(String, Vec<String>)> {
        let v = match self.jf(reqwest::Method::GET, "/Library/VirtualFolders", &[]).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[arr] Jellyfin libraries: {e}");
                return vec![];
            }
        };
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| {
                let id = l["ItemId"].as_str()?.to_string();
                let locs = l["Locations"].as_array()?.iter().filter_map(|x| Some(format!("{}/", x.as_str()?.trim_end_matches('/')))).collect();
                Some((id, locs))
            })
            .collect()
    }

    /// The library's "scan for new files" (what the Jellyfin dashboard does).
    async fn refresh(&self, lib: &str) {
        let q = [
            ("Recursive", "true"),
            ("MetadataRefreshMode", "Default"),
            ("ImageRefreshMode", "Default"),
            ("ReplaceAllMetadata", "false"),
            ("ReplaceAllImages", "false"),
        ];
        match self.jf(reqwest::Method::POST, &format!("/Items/{lib}/Refresh"), &q).await {
            Ok(_) => eprintln!("[arr] Jellyfin scanning library {lib}"),
            Err(e) => eprintln!("[arr] Jellyfin scan: {e}"),
        }
    }

    /// The library's newest items whose file is one of `paths`.
    async fn find(&self, lib: &str, movie: bool, paths: &[&str]) -> BTreeMap<String, Value> {
        let q = [
            ("ParentId", lib),
            ("Recursive", "true"),
            ("IncludeItemTypes", if movie { "Movie" } else { "Episode" }),
            // without a user, films in a collection are otherwise listed as the collection
            ("CollapseBoxSetItems", "false"),
            ("SortBy", "DateCreated"),
            ("SortOrder", "Descending"),
            ("Limit", "200"),
            ("Fields", "Path,DateCreated,ProductionYear"),
        ];
        let v = self.jf(reqwest::Method::GET, "/Items", &q).await.unwrap_or(Value::Null);
        v["Items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| {
                let p = i["Path"].as_str()?;
                paths.contains(&p).then(|| (p.to_string(), i.clone()))
            })
            .collect()
    }

    /// Requesters as the profiles people know: the Jellyfin user's name, and
    /// nobody for hidden (service) accounts, e.g. the one Seerr itself signs in
    /// with, which made the early requests.
    async fn profiles(&self, who: Vec<(String, String)>) -> Vec<(String, String)> {
        if who.is_empty() {
            return who;
        }
        let Ok(users) = self.jf(reqwest::Method::GET, "/Users", &[]).await else { return who };
        who.into_iter()
            .filter_map(|(name, id)| {
                let u = users.as_array()?.iter().find(|u| u["Id"].as_str() == Some(id.as_str()));
                match u {
                    Some(u) if u["Policy"]["IsHidden"].as_bool() == Some(true) => None,
                    Some(u) => Some((u["Name"].as_str().unwrap_or(&name).to_string(), id)),
                    None => Some((name, id)),
                }
            })
            .collect()
    }

    // ---- the notification ----

    async fn announce(&self, g: Group, lib: Option<String>, replay: bool) {
        let paths: Vec<&str> = g.imports.iter().map(|i| i.path.as_str()).collect();
        let mut found = BTreeMap::new();
        if let Some(lib) = &lib {
            let until = std::time::Instant::now() + FIND_FOR;
            loop {
                found = self.find(lib, g.movie, &paths).await;
                // listed with the poster = Jellyfin also has the metadata (title, year)
                let poster = |i: &Value| {
                    if g.movie { i["ImageTags"]["Primary"].is_string() } else { i["SeriesPrimaryImageTag"].is_string() }
                };
                let ready = found.len() == paths.len() && found.values().all(poster);
                if ready {
                    break;
                }
                if std::time::Instant::now() >= until {
                    eprintln!("[arr] {}: Jellyfin doesn't list all of it yet, announcing anyway", g.title);
                    break;
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
        // upgrades: imported before, or Jellyfin already had the film / episode
        let mut new: Vec<&Import> = Vec::new();
        for i in &g.imports {
            let created = found.get(&i.path).and_then(|v| v["DateCreated"].as_str()).and_then(parse_date);
            let upgrade = created.is_some_and(|c| c + UPGRADE_SLACK < i.date) || self.imported_before(&g, i).await;
            if !upgrade {
                new.push(i);
            }
        }
        if new.is_empty() {
            eprintln!("[arr] {}: an upgrade of what the library had, not announced", g.title);
            return;
        }

        let season = new.iter().filter_map(|i| i.season).min();
        let who = match g.tmdb {
            Some(t) => self.seerr.requesters(if g.movie { "movie" } else { "tv" }, t, if g.movie { None } else { season }).await,
            None => vec![],
        };
        let who = self.profiles(who).await;
        let by = if who.is_empty() {
            None
        } else {
            Some(format!("richiesto da {}", who.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")))
        };
        let first = new.first().and_then(|i| found.get(&i.path));
        let n = if g.movie {
            let year = first.and_then(|i| i["ProductionYear"].as_u64()).or(g.year);
            let body = [year.map(|y| y.to_string()), Some("Film".into()), by.clone().or(Some("nuovo nella libreria".into()))];
            Notification {
                id: 0,
                time: 0,
                kind: "movie".into(),
                kicker: "Pronto da guardare".into(),
                title: first.and_then(|i| i["Name"].as_str()).unwrap_or(&g.title).to_string(),
                body: body.into_iter().flatten().collect::<Vec<_>>().join(" · "),
                image: first.and_then(|i| {
                    Some(format!("/Items/{}/Images/Primary?maxHeight=300&v={}", i["Id"].as_str()?, i["ImageTags"]["Primary"].as_str()?))
                }),
                logo: false,
                item_id: first.and_then(|i| i["Id"].as_str()).map(str::to_string),
                channel_id: None,
                users: who.iter().map(|(_, id)| id.clone()).filter(|id| !id.is_empty()).collect(),
                tg: false,
            }
        } else {
            let mut seasons: Vec<u64> = new.iter().filter_map(|i| i.season).collect();
            seasons.sort();
            seasons.dedup();
            let what = if new.len() == 1 {
                let i = new[0];
                let ep = found.get(&i.path).and_then(|v| v["Name"].as_str()).map(str::to_string);
                let se = format!("S{} E{}", i.season.unwrap_or(0), i.episode.unwrap_or(0));
                match ep {
                    Some(e) => format!("{se} · {e}"),
                    None => se,
                }
            } else if seasons.len() == 1 {
                format!("Stagione {} · {} episodi", seasons[0], new.len())
            } else {
                format!("Stagioni {}–{} · {} episodi", seasons[0], seasons[seasons.len() - 1], new.len())
            };
            let series = |k: &str| first.and_then(|i| i[k].as_str()).map(str::to_string);
            Notification {
                id: 0,
                time: 0,
                kind: "episode".into(),
                kicker: if new.len() == 1 { "Nuovo episodio".into() } else { "Nuovi episodi".into() },
                title: series("SeriesName").unwrap_or_else(|| g.title.clone()),
                body: [Some(what), by].into_iter().flatten().collect::<Vec<_>>().join(" · "),
                image: series("SeriesId")
                    .zip(series("SeriesPrimaryImageTag"))
                    .map(|(id, tag)| format!("/Items/{id}/Images/Primary?maxHeight=300&v={tag}")),
                logo: false,
                item_id: series("SeriesId"),
                channel_id: None,
                users: who.iter().map(|(_, id)| id.clone()).filter(|id| !id.is_empty()).collect(),
                tg: false,
            }
        };
        let newest = new.iter().map(|i| i.date).max().unwrap_or(0);
        let toast = replay || now_ms().saturating_sub(newest) < TOAST_WITHIN;
        if self.dry {
            println!("{}  toast={toast}", serde_json::to_string(&n).unwrap());
            return;
        }
        self.notify.push(n, toast).await;
    }
}

/// History records -> one group per film, one per series (in import order).
fn group(recs: &[&Value], movie: bool) -> Vec<Group> {
    let mut out: Vec<(u64, Group)> = Vec::new();
    for r in recs.iter().rev() {
        let Some(path) = r["data"]["importedPath"].as_str() else { continue };
        let m = if movie { &r["movie"] } else { &r["series"] };
        let key = if movie { r["movieId"].as_u64() } else { r["seriesId"].as_u64() }.unwrap_or(0);
        let imp = Import {
            record: r["id"].as_u64().unwrap_or(0),
            episode_id: r["episodeId"].as_u64(),
            date: r["date"].as_str().and_then(parse_date).unwrap_or(0),
            path: path.to_string(),
            season: r["episode"]["seasonNumber"].as_u64(),
            episode: r["episode"]["episodeNumber"].as_u64(),
        };
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some((_, g)) if !g.imports.iter().any(|i| i.path == imp.path) => g.imports.push(imp),
            Some(_) => {}
            None => out.push((
                key,
                Group {
                    movie,
                    key,
                    title: m["title"].as_str().unwrap_or("?").to_string(),
                    year: m["year"].as_u64(),
                    tmdb: m["tmdbId"].as_u64().filter(|t| *t > 0),
                    imports: vec![imp],
                },
            )),
        }
    }
    out.into_iter().map(|(_, g)| g).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates() {
        assert_eq!(super::parse_date("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(super::parse_date("2026-10-07T14:30:00Z"), Some(1_791_383_400_000));
        assert_eq!(super::parse_date("2026-10-07T15:28:31.9780983Z"), Some(1_791_386_911_000));
    }
}
