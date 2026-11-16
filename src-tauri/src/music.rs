//! Background music: a second, headless mpv (no window, no input) that plays a
//! queue while the UI stays in charge of the screen and the controller. The
//! queue lives in JS; mpv holds it as its playlist. Every track is reported to
//! Jellyfin (start / progress / stop) like the video player does.

use std::{
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{unix::OwnedWriteHalf, UnixStream},
    process::Command,
    sync::Mutex,
    time::interval,
};

use crate::jellyfin::Jf;

const TICKS: f64 = 10_000_000.0;

/// What the UI needs to draw the player; sent as `music-state`.
#[derive(Clone, Serialize, Default)]
pub struct State {
    /// false once the queue has finished or was stopped
    pub active: bool,
    /// Jellyfin id of the current track
    pub item_id: Option<String>,
    /// its position in the queue (mpv's playlist order = the JS queue order)
    pub index: i64,
    pub time: f64,
    pub duration: f64,
    pub paused: bool,
}

#[derive(Default)]
pub struct Music {
    busy: AtomicBool,
    writer: Mutex<Option<OwnedWriteHalf>>,
}

fn debug() -> bool {
    std::env::var_os("AZOTOTV_DEBUG").is_some()
}

fn sock_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("azototv-music.sock")
}

/// Track URL (token included; it only travels over the IPC socket, never argv or JS).
/// `yt:<video id>`: a song from YouTube Music, streamed through yt-dlp.
fn track_url(base: &str, token: &str, id: &str) -> String {
    match id.strip_prefix("yt:") {
        Some(v) => format!("https://www.youtube.com/watch?v={v}"),
        None => format!("{base}/Audio/{id}/stream?static=true&mediaSourceId={id}&api_key={token}"),
    }
}

/// Track id back from a track URL (mpv's `path`), so JS never sees the URL.
fn id_from_path(path: &str) -> Option<String> {
    if let Some(v) = path.split("youtube.com/watch?v=").nth(1) {
        return Some(format!("yt:{}", v.split('&').next()?));
    }
    let rest = path.split("/Audio/").nth(1)?;
    Some(rest.split('/').next()?.to_string())
}

/// YouTube songs aren't Jellyfin's: nothing to report for them.
fn reported(id: &str) -> bool {
    !id.starts_with("yt:")
}

impl Music {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    async fn send(&self, args: Vec<Value>) {
        if let Some(w) = self.writer.lock().await.as_mut() {
            let mut line = json!({ "command": args }).to_string();
            line.push('\n');
            let _ = w.write_all(line.as_bytes()).await;
        }
    }

    pub async fn command(&self, args: Vec<Value>) {
        self.send(args).await;
    }

    /// Stop the music (before a video starts, on exit) and let the stop be reported.
    pub async fn shutdown(&self) {
        if !self.busy.load(Ordering::SeqCst) {
            return;
        }
        self.send(vec![json!("quit")]).await;
        for _ in 0..40 {
            if !self.busy.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Replace the queue with `ids` and start at `index`. Starts mpv if needed.
    pub async fn play(self: &Arc<Self>, app: AppHandle, jf: Arc<Jf>, ids: Vec<String>, index: usize) -> Result<(), String> {
        let s = jf.session().ok_or("not logged in")?;
        if ids.is_empty() {
            return Err("empty queue".into());
        }
        if !self.busy.swap(true, Ordering::SeqCst) {
            if let Err(e) = self.spawn(app, jf.clone()).await {
                self.busy.store(false, Ordering::SeqCst);
                return Err(e);
            }
        }
        let index = index.min(ids.len() - 1);
        // the chosen track replaces whatever plays; the rest goes around it
        self.send(vec![json!("loadfile"), json!(track_url(&s.base, &s.token, &ids[index])), json!("replace")])
            .await;
        self.send(vec![json!("set_property"), json!("pause"), json!(false)]).await;
        self.fill_around(&s.base, &s.token, &ids, index).await;
        Ok(())
    }

    /// New order for the queue (shuffle on/off) without interrupting the
    /// current track, which must be `ids[index]`.
    pub async fn requeue(&self, jf: &Jf, ids: Vec<String>, index: usize) -> Result<(), String> {
        let s = jf.session().ok_or("not logged in")?;
        if !self.busy.load(Ordering::SeqCst) || index >= ids.len() {
            return Err("not playing".into());
        }
        self.send(vec![json!("playlist-clear")]).await; // keeps the current entry
        self.fill_around(&s.base, &s.token, &ids, index).await;
        Ok(())
    }

    /// With only the current entry in mpv's playlist, add the others so that it
    /// ends up at position `index`.
    async fn fill_around(&self, base: &str, token: &str, ids: &[String], index: usize) {
        for (i, id) in ids.iter().enumerate() {
            if i != index {
                self.send(vec![json!("loadfile"), json!(track_url(base, token, id)), json!("append")])
                    .await;
            }
        }
        if index > 0 {
            // "move entry 0 in front of the entry now at index+1" = it lands at `index`
            self.send(vec![json!("playlist-move"), json!(0), json!(index + 1)]).await;
        }
    }

    async fn spawn(self: &Arc<Self>, app: AppHandle, jf: Arc<Jf>) -> Result<(), String> {
        let sock = sock_path();
        let _ = std::fs::remove_file(&sock);
        let bin = std::env::var("AZOTOTV_MPV").unwrap_or_else(|_| "mpv".into());
        let mut cmd = Command::new(bin);
        cmd.args([
            "--idle=once", // quit when the queue runs out
            "--no-video",
            "--force-window=no",
            "--audio-display=no",
            "--no-terminal",
            "--input-default-bindings=no",
            "--input-gamepad=no",
            "--input-terminal=no",
            "--save-position-on-quit=no",
            "--keep-open=no",
            "--gapless-audio=weak",
            "--prefetch-playlist=yes",
            // mpv buffers up to 150 MiB of a network stream by default (~180 MB
            // of RAM for a long song); a few minutes of FLAC is plenty on a LAN
            "--demuxer-max-bytes=16MiB",
            "--demuxer-max-back-bytes=4MiB",
            // YouTube songs: the audio only
            "--ytdl-format=bestaudio/best",
        ])
        .arg(format!("--input-ipc-server={}", sock.display()));
        crate::ytdl::hook(&mut cmd);
        if let Ok(extra) = std::env::var("AZOTOTV_MPV_ARGS") {
            // video-only options (--vo=..) are harmless here
            cmd.args(extra.split_whitespace());
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(if debug() { Stdio::inherit() } else { Stdio::null() })
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("cannot start mpv: {e}"))?;

        let mut stream = None;
        for _ in 0..60 {
            if let Ok(st) = UnixStream::connect(&sock).await {
                stream = Some(st);
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let Some(stream) = stream else {
            let _ = child.kill().await;
            return Err("mpv did not open its control socket".into());
        };
        let (read, write) = stream.into_split();
        *self.writer.lock().await = Some(write);
        for (i, prop) in ["time-pos", "pause", "duration", "path", "playlist-pos"].iter().enumerate() {
            self.send(vec![json!("observe_property"), json!(i + 1), json!(prop)]).await;
        }

        let me = self.clone();
        tokio::spawn(async move {
            me.supervise(&app, &jf, read, &mut child).await;
            let _ = child.kill().await;
            *me.writer.lock().await = None;
            let _ = std::fs::remove_file(sock_path());
            me.busy.store(false, Ordering::SeqCst);
            let _ = app.emit("music-state", State::default());
        });
        Ok(())
    }

    async fn supervise(
        &self,
        app: &AppHandle,
        jf: &Arc<Jf>,
        read: tokio::net::unix::OwnedReadHalf,
        child: &mut tokio::process::Child,
    ) {
        let session_id = format!("azototv-music-{:x}", std::process::id());
        let mut lines = BufReader::new(read).lines();
        let mut tick = interval(Duration::from_secs(10));
        tick.tick().await;

        let mut st = State { active: true, index: -1, ..Default::default() };
        // id of the file mpv is opening (from `path`) / the one reported as playing
        let mut opening: Option<String> = None;
        let mut playing: Option<String> = None;
        let mut last_emit = Instant::now() - Duration::from_secs(1);

        let report = |path: &'static str, id: &str, pos: f64, paused: bool, extra: Value| {
            let jf = jf.clone();
            let mut v = json!({
                "ItemId": id,
                "MediaSourceId": id,
                "PlaySessionId": session_id,
                "PositionTicks": (pos * TICKS) as i64,
                "IsPaused": paused,
                "PlayMethod": "DirectPlay",
                "CanSeek": true,
            });
            if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
                o.extend(e.clone());
            }
            let skip = !reported(id);
            async move {
                if skip {
                    return;
                }
                if let Err(e) = jf.request("POST", path, &[], Some(v)).await {
                    eprintln!("[music] report {path}: {e}");
                }
            }
        };

        loop {
            let mut changed = false;
            tokio::select! {
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    let Ok(ev) = serde_json::from_str::<Value>(&line) else { continue };
                    match ev["event"].as_str() {
                        Some("property-change") => match ev["name"].as_str() {
                            Some("time-pos") => if let Some(p) = ev["data"].as_f64() {
                                st.time = p;
                                // the clock only needs a few updates a second
                                changed = last_emit.elapsed() >= Duration::from_millis(400);
                            },
                            Some("duration") => if let Some(d) = ev["data"].as_f64() { st.duration = d; changed = true },
                            Some("path") => opening = ev["data"].as_str().and_then(id_from_path),
                            Some("playlist-pos") => if let Some(i) = ev["data"].as_i64() { st.index = i; changed = true },
                            Some("pause") => {
                                st.paused = ev["data"].as_bool().unwrap_or(false);
                                changed = true;
                                if let Some(id) = &playing {
                                    report("/Sessions/Playing/Progress", id, st.time, st.paused,
                                        json!({"EventName": if st.paused {"pause"} else {"unpause"}})).await;
                                }
                            }
                            _ => {}
                        },
                        Some("file-loaded") => {
                            playing = opening.clone();
                            st.item_id = playing.clone();
                            st.time = 0.0;
                            changed = true;
                            if debug() { eprintln!("[music] playing {:?}", playing); }
                            if let Some(id) = &playing {
                                report("/Sessions/Playing", id, 0.0, st.paused, json!({})).await;
                            }
                        }
                        Some("end-file") => {
                            // natural end => full runtime, so Jellyfin counts the play
                            let eof = ev["reason"].as_str() == Some("eof");
                            if let Some(id) = playing.take() {
                                let pos = if eof && st.duration > 0.0 { st.duration } else { st.time };
                                report("/Sessions/Playing/Stopped", &id, pos, st.paused, json!({})).await;
                            }
                        }
                        _ => {}
                    }
                }
                _ = tick.tick() => {
                    if let Some(id) = &playing {
                        report("/Sessions/Playing/Progress", id, st.time, st.paused,
                            json!({"EventName": "timeupdate"})).await;
                    }
                }
                _ = child.wait() => break,
            }
            if changed {
                last_emit = Instant::now();
                let _ = app.emit("music-state", st.clone());
            }
        }
        if let Some(id) = playing.take() {
            report("/Sessions/Playing/Stopped", &id, st.time, st.paused, json!({})).await;
        }
        if debug() {
            eprintln!("[music] stopped");
        }
    }
}
