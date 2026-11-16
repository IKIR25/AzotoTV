//! Musica: songs found on YouTube Music, downloaded into the music library so
//! everyone has them in Jellyfin (and in their playlists).
//!
//! Config `~/.config/azototv/music.json` (mode 600, only on the server):
//! `{"dir": "/media/Musica/AzotoTV", "jellyfinUrl": "http://localhost:8096", "jellyfinKey": "<admin API key>"}`.
//! The folder is a subfolder of the "Musica" library on purpose: the nightly
//! playlist sync (`yt-playlist-sync.sh`) only manages the files at the top of
//! the library and never touches what's in here. The admin key is needed to
//! tell Jellyfin about a new file and to add it to a profile's playlist; it
//! stays in Rust.
//!
//! One download at a time, a pause between them (YouTube throttles bursts);
//! the queue is saved, so a restart carries on.

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::{process::Command, sync::Notify};

use crate::{jellyfin::config_dir, youtube::Song};

/// between two downloads (YouTube 403s a burst of them)
const PAUSE: Duration = Duration::from_secs(8);
/// how long to wait for Jellyfin to list a new file
const SCAN_WAIT: Duration = Duration::from_secs(120);

#[derive(Deserialize)]
struct Config {
    dir: PathBuf,
    #[serde(rename = "jellyfinUrl")]
    jellyfin_url: String,
    #[serde(rename = "jellyfinKey")]
    jellyfin_key: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Downloading,
    /// downloaded, waiting for Jellyfin to list it
    Adding,
    Done,
    Error,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Job {
    pub song: Song,
    /// Jellyfin user who asked first
    pub user: String,
    /// (playlist, its owner) to add it to once it's in the library
    pub playlists: Vec<(String, String)>,
    pub state: JobState,
    #[serde(default)]
    pub error: String,
}

/// Sent to the UI as `songs-changed`.
#[derive(Clone, Serialize)]
pub struct Status {
    /// downloading is possible on this device
    pub enabled: bool,
    pub jobs: Vec<Job>,
    /// YouTube id -> Jellyfin item id of every song already in the library
    pub library: HashMap<String, String>,
}

pub struct Songs {
    client: reqwest::Client,
    config: Option<Config>,
    jobs: Mutex<Vec<Job>>,
    library: Mutex<(Option<Instant>, HashMap<String, String>)>,
    wake: Notify,
}

fn queue_file() -> PathBuf {
    crate::games::data_dir().join("music-downloads.json")
}

/// "Title [dQw4w9WgXcQ].m4a" -> "dQw4w9WgXcQ"
fn yt_id(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?;
    let open = name.rfind('[')?;
    let close = name[open..].find(']')? + open;
    let id = &name[open + 1..close];
    (id.len() == 11 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then_some(id)
}

impl Songs {
    pub fn new() -> Arc<Self> {
        let config = fs::read_to_string(config_dir().join("music.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Config>(&s).ok())
            .filter(|c| c.dir.is_dir())
            .map(|c| Config { jellyfin_url: c.jellyfin_url.trim_end_matches('/').to_string(), ..c });
        // unfinished downloads from the last run
        let jobs: Vec<Job> = fs::read_to_string(queue_file())
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<Job>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|j| matches!(j.state, JobState::Queued | JobState::Downloading | JobState::Adding))
            .map(|j| Job { state: JobState::Queued, ..j })
            .collect();
        Arc::new(Songs {
            client: reqwest::Client::builder().timeout(Duration::from_secs(30)).build().expect("http client"),
            config,
            jobs: Mutex::new(jobs),
            library: Mutex::new((None, HashMap::new())),
            wake: Notify::new(),
        })
    }

    fn save(&self) {
        let jobs = self.jobs.lock().unwrap().clone();
        let _ = fs::create_dir_all(crate::games::data_dir());
        let _ = fs::write(queue_file(), serde_json::to_string(&jobs).unwrap_or_default());
    }

    async fn jf(&self, method: reqwest::Method, path: &str, query: &[(&str, &str)], body: Option<Value>) -> Result<Value, String> {
        let c = self.config.as_ref().ok_or("downloads not set up")?;
        let mut req = self
            .client
            .request(method, format!("{}{path}", c.jellyfin_url))
            .query(query)
            .header("Authorization", format!("MediaBrowser Client=\"AzotoTV\", Token=\"{}\"", c.jellyfin_key));
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    /// YouTube id -> Jellyfin item, for every song file whose name ends in "[id]"
    /// (ours and the playlist sync's). Asked again at most every 30 s.
    async fn library(&self, fresh: bool) -> HashMap<String, String> {
        if !fresh {
            let l = self.library.lock().unwrap();
            if l.0.is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
                return l.1.clone();
            }
        }
        let Ok(v) = self
            .jf(reqwest::Method::GET, "/Items", &[("Recursive", "true"), ("IncludeItemTypes", "Audio"), ("Fields", "Path"), ("Limit", "20000")], None)
            .await
        else {
            return self.library.lock().unwrap().1.clone();
        };
        let map: HashMap<String, String> = v["Items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|it| Some((yt_id(it["Path"].as_str()?)?.to_string(), it["Id"].as_str()?.to_string())))
            .collect();
        *self.library.lock().unwrap() = (Some(Instant::now()), map.clone());
        map
    }

    pub async fn status(&self, fresh: bool) -> Status {
        let library = if self.config.is_some() { self.library(fresh).await } else { HashMap::new() };
        Status { enabled: self.config.is_some(), jobs: self.jobs.lock().unwrap().clone(), library }
    }

    async fn changed(&self, app: Option<&AppHandle>, fresh: bool) {
        if let Some(app) = app {
            let _ = app.emit("songs-changed", self.status(fresh).await);
        }
    }

    /// Download `song` for `user` and put it in `playlists` (of that user) when it's in the library.
    /// Already in the library: only the playlists. Already queued: the playlists are added to its job.
    pub async fn download(&self, app: &AppHandle, song: Song, user: String, playlists: Vec<String>) -> Result<(), String> {
        if self.config.is_none() {
            return Err("Scaricare è possibile solo sul server".into());
        }
        if let Some(item) = self.library(false).await.get(&song.id).cloned() {
            for p in &playlists {
                self.add_to_playlist(p, &item, &user).await?;
            }
            return Ok(());
        }
        {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.retain(|j| !(j.song.id == song.id && j.state == JobState::Error));
            match jobs.iter_mut().find(|j| j.song.id == song.id && j.state != JobState::Done) {
                Some(j) => {
                    for p in playlists {
                        let t = (p, user.clone());
                        if !j.playlists.contains(&t) {
                            j.playlists.push(t);
                        }
                    }
                }
                None => {
                    let playlists = playlists.into_iter().map(|p| (p, user.clone())).collect();
                    jobs.push(Job { song, user, playlists, state: JobState::Queued, error: String::new() })
                }
            }
        }
        self.save();
        self.wake.notify_one();
        self.changed(Some(app), false).await;
        Ok(())
    }

    /// Scan the library that holds `path` (only that one, not films and series).
    async fn rescan(&self, path: &str) {
        let Ok(v) = self.jf(reqwest::Method::GET, "/Library/VirtualFolders", &[], None).await else { return };
        for lib in v.as_array().into_iter().flatten() {
            let holds = lib["Locations"].as_array().into_iter().flatten().filter_map(Value::as_str).any(|l| path.starts_with(l));
            if let (true, Some(id)) = (holds, lib["ItemId"].as_str()) {
                let _ = self
                    .jf(reqwest::Method::POST, &format!("/Items/{id}/Refresh"), &[("Recursive", "true"), ("MetadataRefreshMode", "Default"), ("ImageRefreshMode", "Default")], None)
                    .await;
            }
        }
    }

    async fn add_to_playlist(&self, playlist: &str, item: &str, user: &str) -> Result<(), String> {
        // already in it: not twice
        let have = self.jf(reqwest::Method::GET, &format!("/Playlists/{playlist}/Items"), &[("userId", user)], None).await?;
        if have["Items"].as_array().into_iter().flatten().any(|x| x["Id"].as_str() == Some(item)) {
            return Ok(());
        }
        self.jf(reqwest::Method::POST, &format!("/Playlists/{playlist}/Items"), &[("ids", item), ("userId", user)], None)
            .await
            .map(|_| ())
    }

    fn set(&self, id: &str, state: JobState, error: String) {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(j) = jobs.iter_mut().find(|j| j.song.id == id && j.state != JobState::Done) {
            j.state = state;
            j.error = error;
        }
        // finished ones are kept a while for the UI, but not forever
        let done = jobs.iter().filter(|j| j.state == JobState::Done).count();
        if done > 30 {
            let mut skip = done - 30;
            jobs.retain(|j| {
                let drop = j.state == JobState::Done && skip > 0;
                if drop {
                    skip -= 1;
                }
                !drop
            });
        }
    }

    /// The download worker: started once, sleeps while there's nothing to do.
    pub fn start(self: &Arc<Self>, app: AppHandle) {
        if self.config.is_none() {
            return;
        }
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let next = me.jobs.lock().unwrap().iter().find(|j| j.state == JobState::Queued).cloned();
                let Some(job) = next else {
                    me.wake.notified().await;
                    continue;
                };
                me.set(&job.song.id, JobState::Downloading, String::new());
                me.save();
                me.changed(Some(&app), false).await;
                let result = me.run(Some(&app), &job).await;
                match &result {
                    Ok(()) => me.set(&job.song.id, JobState::Done, String::new()),
                    Err(e) => {
                        eprintln!("[songs] {}: {e}", job.song.id);
                        me.set(&job.song.id, JobState::Error, e.clone());
                    }
                }
                me.save();
                me.changed(Some(&app), result.is_ok()).await;
                tokio::time::sleep(PAUSE).await;
            }
        });
    }

    async fn run(&self, app: Option<&AppHandle>, job: &Job) -> Result<(), String> {
        let c = self.config.as_ref().ok_or("downloads not set up")?;
        let out = Command::new(crate::ytdl::ytdlp())
            .args([
                "--no-playlist",
                "--no-progress",
                "--quiet",
                "--no-warnings",
                // the original AAC when there is one (no re-encoding), else converted
                "-f",
                "bestaudio[ext=m4a]/bestaudio",
                "-x",
                "--audio-format",
                "m4a",
                "--embed-metadata",
                "--embed-thumbnail",
                "--convert-thumbnails",
                "jpg",
                // the video's 16:9 picture cropped to the square album cover
                "--ppa",
                "ThumbnailsConvertor+ffmpeg_o:-c:v mjpeg -vf crop=\"'if(gt(ih,iw),iw,ih)':'if(gt(iw,ih),ih,iw)'\"",
                "--print",
                "after_move:filepath",
                "-o",
            ])
            .arg(c.dir.join("%(artist,uploader)s - %(track,title)s [%(id)s].%(ext)s"))
            .arg(format!("https://music.youtube.com/watch?v={}", job.song.id))
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| format!("yt-dlp: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("download non riuscito").to_string());
        }
        let path = String::from_utf8_lossy(&out.stdout).lines().last().unwrap_or_default().trim().to_string();
        self.set(&job.song.id, JobState::Adding, String::new());
        self.changed(app, false).await;
        if std::env::var_os("AZOTOTV_DEBUG").is_some() {
            eprintln!("[songs] downloaded {path}");
        }

        // tell Jellyfin about the file (same path inside its container), then wait for it
        let _ = self
            .jf(reqwest::Method::POST, "/Library/Media/Updated", &[], Some(json!({ "Updates": [{ "Path": path, "UpdateType": "Created" }] })))
            .await;
        let start = Instant::now();
        let mut rescanned = false;
        let item = loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if let Some(i) = self.library(true).await.get(&job.song.id) {
                break i.clone();
            }
            // Jellyfin queues "a file appeared" for a minute or more: scan the
            // music library now (a few seconds, it only has songs)
            if !rescanned && start.elapsed() > Duration::from_secs(5) {
                rescanned = true;
                self.rescan(&path).await;
            }
            if start.elapsed() > SCAN_WAIT {
                return Err("scaricato, ma Jellyfin non lo vede ancora".into());
            }
        };
        // the latest playlists asked for while it downloaded count too
        let playlists = self
            .jobs
            .lock()
            .unwrap()
            .iter()
            .find(|j| j.song.id == job.song.id)
            .map(|j| j.playlists.clone())
            .unwrap_or_default();
        for (p, owner) in playlists {
            self.add_to_playlist(&p, &item, &owner).await?;
        }
        Ok(())
    }
}

/// `azototv --song-check <YouTube id> [playlist id user id]`: one download through
/// the real pipeline (yt-dlp, Jellyfin, playlist), printed instead of shown.
pub async fn check(id: &str, playlist: Option<(String, String)>) {
    let songs = Songs::new();
    if songs.config.is_none() {
        return eprintln!("no ~/.config/azototv/music.json (or its folder is missing)");
    }
    let song = Song { id: id.into(), title: id.into(), ..Default::default() };
    let job = Job {
        song,
        user: playlist.as_ref().map(|p| p.1.clone()).unwrap_or_default(),
        playlists: playlist.into_iter().collect(),
        state: JobState::Queued,
        error: String::new(),
    };
    songs.jobs.lock().unwrap().push(job.clone());
    let t = Instant::now();
    match songs.run(None, &job).await {
        Ok(()) => println!("ok in {:.0} s; in the library as {:?}", t.elapsed().as_secs_f64(), songs.library(true).await.get(id)),
        Err(e) => println!("failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_from_names() {
        assert_eq!(yt_id("/media/Musica/7 Weeks & 3 Days [95Imop32u44].flac"), Some("95Imop32u44"));
        assert_eq!(yt_id("/media/Musica/AzotoTV/Vasco Rossi - Albachiara [wXFFtkQZlo8].m4a"), Some("wXFFtkQZlo8"));
        assert_eq!(yt_id("/media/Musica/Album [Live]/01.flac"), None);
    }
}
