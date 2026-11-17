//! Video/audio playback: drives an external mpv over its JSON IPC socket and
//! keeps Jellyfin informed (start / progress / stop) so resume points work.

use std::{
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
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

const INPUT_CONF: &str = include_str!("mpv-input.conf");
const OSD_LUA: &str = include_str!("azoto-osd.lua");

/// The overlay script with the dictionary for its texts (`tr('…')` in the
/// Lua) in the language in use on top, as the table `AZOTO_T`.
fn osd_script() -> String {
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let mut table = String::from("AZOTO_T = {\n");
    let mut seen = std::collections::HashSet::new();
    for open in ["tr('", "trf('"] {
        for call in OSD_LUA.split(open).skip(1) {
            let Some(en) = call.split('\'').next() else { continue };
            let it = crate::locale::t(en);
            if it != en && seen.insert(en) {
                table.push_str(&format!("  [{}] = {},\n", quote(en), quote(&it)));
            }
        }
    }
    table.push_str("}\n");
    table + OSD_LUA
}

const TICKS: f64 = 10_000_000.0;
/// YouTube: at most 1080p, VP9 (decoded by the GPU; AV1 isn't) or else H.264,
/// plain HTTPS streams first (their HLS copies keep dropping connections).
const YT_FORMAT: &str = "bv*[height<=1080][vcodec~='^(vp0?9|avc1)'][protocol=https]+ba[protocol=https]/bv*[height<=1080][vcodec~='^(vp0?9|avc1)']+ba/b[height<=1080]/b";

#[derive(Deserialize)]
pub struct PlayArgs {
    pub item_id: String,
    /// "Video" or "Audio"
    pub media_type: String,
    pub start_ticks: i64,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    /// Name of the episode that follows, if any (drives the "next episode" prompt).
    #[serde(default)]
    pub next_title: Option<String>,
    /// mpv audio track id (1-based among the file's audio tracks); None = mpv picks.
    #[serde(default)]
    pub aid: Option<u32>,
    /// mpv subtitle track id among the file's own subtitles, 0 = off; None = mpv picks.
    #[serde(default)]
    pub sid: Option<u32>,
    /// Subtitle files next to the video (Jellyfin "external" streams), added after load.
    #[serde(default)]
    pub ext_subs: Vec<ExtSub>,
    /// Jellyfin stream indexes of the chosen tracks (-1 = subtitles off), reported to the server.
    #[serde(default)]
    pub audio_index: Option<i64>,
    #[serde(default)]
    pub subtitle_index: Option<i64>,
    /// Live TV channel (`item_id` is the channel): the stream comes from the
    /// tuner, ▲ ▼ ask the app for another channel (`player-zap`), no reporting.
    #[serde(default)]
    pub live: bool,
    /// A game's trailer: this URL plays as is, nothing is reported to Jellyfin.
    #[serde(default)]
    pub trailer: Option<String>,
    /// A YouTube video id: streamed through yt-dlp (mpv's ytdl hook), nothing
    /// reported to Jellyfin (the app keeps its own history).
    #[serde(default)]
    pub youtube: Option<String>,
    /// A family video from the Galleria (Immich): this URL plays as is with this
    /// HTTP header (the key), nothing is reported to Jellyfin.
    #[serde(default)]
    pub photo_video: Option<(String, String)>,
    /// Extra fields for the overlay's info (live: start, stop, upnext).
    #[serde(default)]
    pub extra: Value,
}

/// The overlay's info for an item: title, subtitle, next episode, extras.
fn osd_info(title: &str, subtitle: &str, next: Option<&str>, extra: &Value) -> Value {
    let mut info = json!({ "title": title, "subtitle": subtitle, "next": next });
    if let (Some(o), Some(e)) = (info.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    info
}

/// mpv key=value list value, safe whatever it contains (commas, quotes…).
fn opt_value(v: &str) -> String {
    format!("%{}%{v}", v.len())
}

/// Where a live channel's stream is (the tuner's own URL) and the headers it needs.
async fn live_source(jf: &Jf, channel_id: &str) -> Result<(String, Option<String>), String> {
    let uid = jf.session().ok_or("not logged in")?.user_id;
    let v = jf
        .request(
            "POST",
            &format!("/Items/{channel_id}/PlaybackInfo"),
            &[("userId".into(), uid)],
            Some(json!({ "AutoOpenLiveStream": false })),
        )
        .await?;
    let src = &v["MediaSources"][0];
    let path = src["Path"].as_str().ok_or("channel without a stream")?.to_string();
    let ua = src["RequiredHttpHeaders"]["User-Agent"].as_str().map(str::to_string);
    Ok((path, ua))
}

#[derive(Deserialize)]
pub struct ExtSub {
    /// Jellyfin MediaStream index
    pub index: u32,
    pub codec: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub lang: String,
    /// show this one (otherwise it is only listed in the subtitle menu)
    #[serde(default)]
    pub select: bool,
}

/// Jellyfin URL of an external subtitle, in a format mpv reads.
fn ext_sub_url(base: &str, token: &str, item_id: &str, s: &ExtSub) -> String {
    let fmt = match s.codec.to_ascii_lowercase().as_str() {
        "ass" => "ass",
        "ssa" => "ssa",
        "webvtt" | "vtt" => "vtt",
        "pgssub" | "sup" => "sup",
        _ => "srt", // subrip and other text formats: Jellyfin converts
    };
    format!(
        "{base}/Videos/{item_id}/{item_id}/Subtitles/{}/0/Stream.{fmt}?api_key={token}",
        s.index
    )
}

#[derive(Clone, Serialize)]
pub struct Ended {
    pub item_id: String,
    /// "eof" (watched to the end), "quit" (user stopped), "error"
    pub reason: String,
    pub position_ticks: i64,
    pub duration_ticks: i64,
}

#[derive(Default)]
pub struct Player {
    busy: AtomicBool,
    /// a channel switch is replacing the stream: its end-file isn't the end
    switching: AtomicBool,
    /// the live TV channel on screen, if a channel is playing
    channel: std::sync::Mutex<Option<String>>,
    /// the Jellyfin item playing (not a channel, trailer or YouTube video) and where it is, in seconds
    now: std::sync::Mutex<Option<(String, f64)>>,
    writer: Mutex<Option<OwnedWriteHalf>>,
}

fn debug() -> bool {
    std::env::var_os("AZOTOTV_DEBUG").is_some()
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"))
        .join("azototv")
}

impl Player {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    async fn send(&self, v: Value) {
        if let Some(w) = self.writer.lock().await.as_mut() {
            let mut line = v.to_string();
            line.push('\n');
            let _ = w.write_all(line.as_bytes()).await;
        }
    }

    /// mpv is open (video, live TV, a trailer...): it owns the screen and input.
    pub fn active(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    /// The live TV channel being watched (None: no channel on screen).
    pub fn channel(&self) -> Option<String> {
        if !self.active() {
            return None;
        }
        self.channel.lock().unwrap().clone()
    }

    /// The Jellyfin film or episode playing and its position in ticks (a phone's "Continua qui").
    pub fn now(&self) -> Option<(String, i64)> {
        if !self.active() {
            return None;
        }
        self.now.lock().unwrap().as_ref().map(|(id, p)| (id.clone(), (p * TICKS) as i64))
    }

    /// A prompt in the overlay offering a live TV channel ("Guarda il Tg1"):
    /// A on it sends `player-goto` with the channel id. Shown until `until` (unix s).
    pub async fn offer_channel(&self, channel_id: &str, label: &str, sub: &str, until: u64) {
        if !self.active() {
            return;
        }
        let v = json!({ "id": format!("{channel_id}:{until}"), "label": label, "sub": sub, "channel": channel_id, "ends": until });
        self.command(vec![json!("set_property"), json!("user-data/azoto/alert"), json!(v.to_string())]).await;
    }

    /// Ask mpv to quit and give the supervisor a moment to report the stop.
    pub async fn shutdown(&self) {
        if !self.busy.load(Ordering::SeqCst) {
            return;
        }
        self.command(vec![json!("quit")]).await;
        for _ in 0..40 {
            if !self.busy.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Live TV: switch the running player to another channel.
    pub async fn switch_channel(&self, jf: &Jf, channel_id: &str, title: &str, subtitle: &str, extra: &Value) -> Result<(), String> {
        if !self.busy.load(Ordering::SeqCst) {
            return Err("not playing".into());
        }
        let (url, ua) = live_source(jf, channel_id).await?;
        let info = osd_info(title, subtitle, None, extra);
        self.command(vec![json!("set_property"), json!("user-data/azoto/info"), json!(info.to_string())])
            .await;
        let mut opts = format!("force-media-title={},deband=yes", opt_value(title));
        if let Some(ua) = &ua {
            opts.push_str(&format!(",user-agent={}", opt_value(ua)));
        }
        self.switching.store(true, Ordering::SeqCst);
        *self.channel.lock().unwrap() = Some(channel_id.to_string());
        self.command(vec![json!("loadfile"), json!(url), json!("replace"), json!(0), json!(opts)]).await;
        Ok(())
    }

    /// Run an arbitrary mpv command, e.g. ["cycle", "pause"].
    pub async fn command(&self, args: Vec<Value>) {
        self.send(json!({ "command": args })).await;
    }

    pub async fn play(self: &Arc<Self>, app: AppHandle, jf: Arc<Jf>, a: PlayArgs) -> Result<(), String> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err("playback already running".into());
        }
        match self.start(app, jf, a).await {
            Ok(()) => Ok(()),
            Err(e) => {
                self.busy.store(false, Ordering::SeqCst);
                Err(e)
            }
        }
    }

    async fn start(self: &Arc<Self>, app: AppHandle, jf: Arc<Jf>, a: PlayArgs) -> Result<(), String> {
        let s = jf.session().ok_or("not logged in")?;
        let kind = if a.media_type == "Audio" { "Audio" } else { "Videos" };
        // Direct play of the original file: mpv handles every codec/container.
        // Live TV: straight from the channel's own stream (best quality; going
        // through Jellyfin would pick the stream's first, often lowest, variant).
        let (url, user_agent) = if let Some(t) = &a.trailer {
            (t.clone(), None)
        } else if let Some(id) = &a.youtube {
            (format!("https://www.youtube.com/watch?v={id}"), None)
        } else if let Some((url, _)) = &a.photo_video {
            (url.clone(), None)
        } else if a.live {
            live_source(&jf, &a.item_id).await?
        } else {
            let url = format!(
                "{}/{}/{}/stream?static=true&mediaSourceId={}&api_key={}",
                s.base, kind, a.item_id, a.item_id, s.token
            );
            (url, None)
        };

        let conf_dir = config_dir();
        let _ = std::fs::create_dir_all(&conf_dir);
        let conf = conf_dir.join("mpv-input.conf");
        let _ = std::fs::write(&conf, INPUT_CONF);
        let osd = conf_dir.join("azoto-osd.lua");
        let _ = std::fs::write(&osd, osd_script());

        let sock = runtime_dir().join("azototv-mpv.sock");
        let _ = std::fs::remove_file(&sock);

        let bin = std::env::var("AZOTOTV_MPV").unwrap_or_else(|_| "mpv".into());
        let mut cmd = Command::new(bin);
        cmd.envs(crate::games::gpu_env());
        // the interface's language first, then English; subtitles in it only
        // when the film isn't spoken in it
        let own = crate::locale::track_langs();
        cmd.arg(format!("--alang={own},eng,en")).arg(format!("--slang={own}"));
        cmd.args([
            "--idle=yes",
            "--force-window=immediate",
            "--fullscreen",
            "--no-terminal",
            "--input-gamepad=yes",
            "--hwdec=auto-safe",
            "--subs-with-matching-audio=no",
            // our own UI (azoto-osd.lua) replaces mpv's OSC/OSD and default keys
            "--osc=no",
            "--osd-level=0",
            "--osd-bar=no",
            "--input-default-bindings=no",
            "--sub-font-size=44",
            "--sub-border-size=2.4",
            "--sub-shadow-offset=1",
            "--save-position-on-quit=no",
            "--keep-open=no",
            "--title=AzotoTV",
        ])
        .arg(format!("--input-ipc-server={}", sock.display()))
        .arg(format!("--input-conf={}", conf.display()))
        .arg(format!("--script={}", osd.display()));
        crate::ytdl::hook(&mut cmd);
        if let Ok(extra) = std::env::var("AZOTOTV_MPV_ARGS") {
            cmd.args(extra.split_whitespace());
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(if debug() { Stdio::inherit() } else { Stdio::null() })
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("cannot start mpv: {e}"))?;

        // mpv creates the socket a moment after launch.
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

        *self.channel.lock().unwrap() = a.live.then(|| a.item_id.clone());
        for (i, prop) in ["time-pos", "pause", "duration", "user-data/azoto/zap", "user-data/azoto/goto"].iter().enumerate() {
            self.command(vec![json!("observe_property"), json!(i + 1), json!(prop)]).await;
        }
        // Data for the overlay (user-data props survive script load order).
        let info = osd_info(&a.title, &a.subtitle, a.next_title.as_deref(), &a.extra);
        self.command(vec![json!("set_property"), json!("user-data/azoto/info"), json!(info.to_string())])
            .await;
        let segs = if a.live || a.trailer.is_some() || a.youtube.is_some() || a.photo_video.is_some() { vec![] } else { fetch_segments(&jf, &a.item_id).await };
        if debug() {
            eprintln!("[player] segments: {}", Value::Array(segs.clone()));
        }
        if !segs.is_empty() {
            self.command(vec![
                json!("set_property"),
                json!("user-data/azoto/segments"),
                json!(Value::Array(segs).to_string()),
            ])
            .await;
        }

        let start_secs = (a.start_ticks.max(0) as f64) / TICKS;
        let mut opts = format!("force-media-title={}", opt_value(&a.title));
        if a.live {
            // internet TV streams are low bitrate: smooth their blocky gradients
            opts.push_str(",deband=yes");
        } else {
            opts.push_str(&format!(",start={start_secs:.1}"));
        }
        if let Some(ua) = &user_agent {
            opts.push_str(&format!(",user-agent={}", opt_value(ua)));
        }
        if a.youtube.is_some() {
            opts.push_str(&format!(",ytdl-format={}", opt_value(YT_FORMAT)));
        }
        if let Some((_, header)) = &a.photo_video {
            opts.push_str(&format!(",http-header-fields={}", opt_value(header)));
        }
        if let Some(aid) = a.aid {
            opts.push_str(&format!(",aid={aid}"));
        }
        // an external subtitle is selected after it is added (see supervise)
        match (a.sid, a.ext_subs.iter().any(|s| s.select)) {
            (_, true) | (Some(0), _) => opts.push_str(",sid=no"),
            (Some(sid), _) => opts.push_str(&format!(",sid={sid}")),
            (None, false) => {}
        }
        self.command(vec![json!("loadfile"), json!(url), json!("replace"), json!(0), json!(opts)])
            .await;

        if debug() {
            // (never the Galleria's key)
            let shown = opts.split(",http-header-fields=").next().unwrap_or_default();
            eprintln!("[player] mpv started, loading item {} with {shown}", a.item_id);
        }
        let me = self.clone();
        tokio::spawn(async move {
            let ended = me.supervise(&app, &jf, a, read, &mut child).await;
            let _ = child.kill().await;
            *me.writer.lock().await = None;
            let _ = std::fs::remove_file(&sock);
            *me.channel.lock().unwrap() = None;
            me.busy.store(false, Ordering::SeqCst);
            if debug() {
                eprintln!("[player] ended: {} at {} ticks", ended.reason, ended.position_ticks);
            }
            let _ = app.emit("player-ended", ended);
        });
        Ok(())
    }

    /// Read mpv events, report to Jellyfin, return how playback ended.
    async fn supervise(
        &self,
        app: &AppHandle,
        jf: &Arc<Jf>,
        a: PlayArgs,
        read: tokio::net::unix::OwnedReadHalf,
        child: &mut tokio::process::Child,
    ) -> Ended {
        let session_id = format!("azototv-{:x}", std::process::id() as u64 ^ now_millis());
        let mut lines = BufReader::new(read).lines();
        let mut tick = interval(Duration::from_secs(10));
        tick.tick().await; // first tick fires immediately

        let (mut pos, mut duration, mut paused) = (a.start_ticks as f64 / TICKS, 0.0_f64, false);
        let mut loaded = false;
        let mut reason = String::from("quit");

        let body = |pos: f64, paused: bool, extra: Value| {
            let mut v = json!({
                "ItemId": a.item_id,
                "MediaSourceId": a.item_id,
                "PlaySessionId": session_id,
                "PositionTicks": (pos * TICKS) as i64,
                "IsPaused": paused,
                "PlayMethod": "DirectPlay",
                "CanSeek": true,
                "AudioStreamIndex": a.audio_index,
                "SubtitleStreamIndex": a.subtitle_index,
            });
            if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
                o.extend(e.clone());
            }
            v
        };
        // a channel, a trailer, a YouTube or family video: nothing to tell Jellyfin
        let no_report = a.live || a.trailer.is_some() || a.youtube.is_some() || a.photo_video.is_some();
        let report = |path: &'static str, v: Value| {
            let jf = jf.clone();
            async move {
                if no_report {
                    return;
                }
                match jf.request("POST", path, &[], Some(v)).await {
                    Ok(_) => if debug() { eprintln!("[player] report {path}: ok") },
                    Err(e) => eprintln!("[player] report {path}: {e}"),
                }
            }
        };

        *self.now.lock().unwrap() = (!no_report).then(|| (a.item_id.clone(), pos));
        loop {
            tokio::select! {
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    if debug() { eprintln!("[player] mpv: {line}"); }
                    let Ok(ev) = serde_json::from_str::<Value>(&line) else { continue };
                    match ev["event"].as_str() {
                        Some("property-change") => match ev["name"].as_str() {
                            Some("time-pos") => if let Some(p) = ev["data"].as_f64() {
                                pos = p;
                                if let Some(n) = self.now.lock().unwrap().as_mut() { n.1 = p }
                            },
                            Some("duration") => if let Some(d) = ev["data"].as_f64() { duration = d },
                            // live TV: "counter:direction" from the overlay's ▲ ▼
                            Some("user-data/azoto/zap") => if let Some(dir) = ev["data"].as_str()
                                .and_then(|z| z.split(':').nth(1)).and_then(|d| d.parse::<i64>().ok()) {
                                let _ = app.emit("player-zap", dir);
                            },
                            // "counter:channel id" from the overlay's "Guarda il Tg1" prompt
                            Some("user-data/azoto/goto") => if let Some(ch) = ev["data"].as_str()
                                .and_then(|g| g.split_once(':')).map(|(_, c)| c.to_string()) {
                                let _ = app.emit("player-goto", ch);
                            },
                            Some("pause") => {
                                paused = ev["data"].as_bool().unwrap_or(false);
                                if loaded {
                                    report("/Sessions/Playing/Progress",
                                        body(pos, paused, json!({"EventName": if paused {"pause"} else {"unpause"}}))).await;
                                }
                            }
                            _ => {}
                        },
                        Some("file-loaded") => {
                            loaded = true;
                            if let Some(s) = jf.session() {
                                for x in &a.ext_subs {
                                    let url = ext_sub_url(&s.base, &s.token, &a.item_id, x);
                                    let flag = if x.select { "select" } else { "auto" };
                                    self.command(vec![json!("sub-add"), json!(url), json!(flag), json!(x.title), json!(x.lang)]).await;
                                }
                            }
                            report("/Sessions/Playing", body(pos, paused, json!({}))).await;
                        }
                        // the old channel's stream, replaced by a channel switch
                        Some("end-file") if self.switching.swap(false, Ordering::SeqCst) => {}
                        Some("end-file") => {
                            reason = match ev["reason"].as_str() {
                                Some("eof") => "eof",
                                Some("error") => "error",
                                _ => "quit",
                            }.into();
                            self.command(vec![json!("quit")]).await;
                            break;
                        }
                        _ => {}
                    }
                }
                _ = tick.tick() => {
                    if loaded {
                        report("/Sessions/Playing/Progress",
                            body(pos, paused, json!({"EventName": "timeupdate"}))).await;
                    }
                }
                _ = child.wait() => break,
            }
        }

        *self.now.lock().unwrap() = None;
        // Natural end => report the full runtime so Jellyfin marks it watched.
        let final_pos = if reason == "eof" && duration > 0.0 { duration } else { pos };
        if loaded {
            report("/Sessions/Playing/Stopped", body(final_pos, paused, json!({}))).await;
        }
        Ended {
            item_id: a.item_id,
            reason,
            position_ticks: (final_pos * TICKS) as i64,
            duration_ticks: (duration * TICKS) as i64,
        }
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Intro / recap / outro markers from Jellyfin's MediaSegments (fed by the
/// Intro Skipper plugin). Providers often disagree and some detections are
/// absurd (a 40-minute "intro"), so keep only plausible, non-overlapping ones.
async fn fetch_segments(jf: &Jf, item_id: &str) -> Vec<Value> {
    let Ok(v) = jf
        .request("GET", &format!("/MediaSegments/{item_id}"), &[], None)
        .await
    else {
        return vec![];
    };
    let mut segs: Vec<(String, f64, f64)> = v["Items"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|s| {
            let kind = s["Type"].as_str()?.to_string();
            let start = s["StartTicks"].as_f64()? / TICKS;
            let end = s["EndTicks"].as_f64()? / TICKS;
            let len = end - start;
            let ok = match kind.as_str() {
                "Intro" => (15.0..=150.0).contains(&len),
                "Recap" => (15.0..=300.0).contains(&len),
                "Outro" => (15.0..=600.0).contains(&len),
                _ => false,
            };
            ok.then_some((kind, start, end))
        })
        .collect();
    segs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let mut kept: Vec<(String, f64, f64)> = vec![];
    for s in segs {
        if !kept.iter().any(|k| k.0 == s.0 && s.1 < k.2 && k.1 < s.2) {
            kept.push(s);
        }
    }
    kept.into_iter()
        .map(|(t, s, e)| json!({ "type": t, "s": s, "e": e }))
        .collect()
}
