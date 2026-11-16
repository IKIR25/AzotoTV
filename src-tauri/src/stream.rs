//! Game streaming: Sunshine on the server, Moonlight (or a browser) elsewhere,
//! with the TV left alone.
//!
//! Sunshine copies the picture from a screen that is on: with every screen off
//! it refuses the stream ("Is a display connected and turned on?", checked
//! before any of its prep commands run). So the laptop panel never switches
//! off: "screens off" ([`screens`]) disables the TV's output and dims the
//! panel to 1% (kept visible on purpose, a way in if something breaks).
//! Measured 2026-11-06: package ~1.7 W and NVIDIA P8 either way.
//!
//! AzotoTV follows Sunshine's log ([`spawn`]): on `CLIENT CONNECTED` it wakes
//! its UI on the panel without the TV (the `stream` event; `tv_power` and
//! `screens(true)` do nothing while streaming), games keep the panel on
//! (games.rs `set_panel`), and on `CLIENT DISCONNECTED` it goes back to
//! screens off if the TV was off. The log, not Sunshine's prep commands:
//! those don't run when Moonlight resumes a session, nor when its window is
//! just closed.
//!
//! Sound: Sunshine makes its own sink the default while a client is in and
//! puts back the previous one after; with the TV's output disabled that is the
//! laptop's muted speakers, so the TV's HDMI is made the default again whenever
//! the TV comes back ([`tv_audio`]).
//!
//! Sunshine runs in the session (kiosk.sh) with KMS capture of the panel and
//! the Intel iGPU's VAAPI encoder: the panel is scanned out by the Intel, and
//! the NVIDIA can't import its tiled frames (~9 ms host latency measured).

use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};

/// The laptop panel: the screen Sunshine streams.
pub(crate) const PANEL: &str = "eDP-1";

static STREAMING: AtomicBool = AtomicBool::new(false);

/// A Moonlight client is connected.
pub fn active() -> bool {
    STREAMING.load(Ordering::Relaxed)
}

fn kiosk() -> bool {
    std::env::var_os("AZOTOTV_FULLSCREEN").is_some()
}

fn kscreen(args: &[String]) {
    match Command::new("kscreen-doctor").args(args).stdin(Stdio::null()).output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => eprintln!("[stream] kscreen-doctor {args:?}: {}", String::from_utf8_lossy(&o.stderr).trim()),
        Err(e) => eprintln!("[stream] kscreen-doctor: {e}"),
    }
}

/// Every screen but the panel (the TV), as KWin has them.
fn others() -> Vec<Value> {
    let Ok(o) = Command::new("kscreen-doctor").arg("-j").stdin(Stdio::null()).output() else { return Vec::new() };
    let Ok(j) = serde_json::from_slice::<Value>(&o.stdout) else { return Vec::new() };
    j["outputs"].as_array().into_iter().flatten().filter(|o| o["name"] != PANEL).cloned().collect()
}

/// The TV's output (KWin's name), when it is on.
pub(crate) fn tv_output() -> Option<String> {
    others().iter().find(|o| o["enabled"] == true).and_then(|o| o["name"].as_str().map(str::to_string))
}

fn tv_enabled() -> bool {
    others().iter().any(|o| o["enabled"] == true)
}

/// The panel's backlight (sysfs name, current, max).
fn backlight() -> Option<(String, u32, u32)> {
    let dir = fs::read_dir("/sys/class/backlight").ok()?.flatten().next()?.path();
    let read = |f: &str| fs::read_to_string(dir.join(f)).ok()?.trim().parse::<u32>().ok();
    Some((dir.file_name()?.to_string_lossy().into_owned(), read("brightness")?, read("max_brightness")?))
}

/// Through logind (the session may, root isn't needed).
fn set_backlight(name: &str, v: u32) {
    let r = Command::new("busctl")
        .args(["call", "org.freedesktop.login1", "/org/freedesktop/login1/session/auto"])
        .args(["org.freedesktop.login1.Session", "SetBrightness", "ssu", "backlight", name, &v.to_string()])
        .stdin(Stdio::null())
        .output();
    match r {
        Ok(o) if o.status.success() => {}
        Ok(o) => eprintln!("[stream] backlight {v}: {}", String::from_utf8_lossy(&o.stderr).trim()),
        Err(e) => eprintln!("[stream] backlight: {e}"),
    }
}

/// The panel's brightness before it was dimmed: in the runtime dir, so a
/// restart of AzotoTV while dark still brings it back (gone at reboot).
fn saved_path() -> PathBuf {
    let run = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    run.join("azototv-backlight")
}

/// The TV's sound: the NVIDIA's HDMI audio (as in wireplumber-hdmi-suspend.conf).
const TV_SINK: &str = "alsa_output.pci-0000_01_00.1.";
/// Sunshine's own outputs, which it makes the default while a client is in.
const SUNSHINE_SINKS: [&str; 3] = ["sink-sunshine-stereo", "sink-sunshine-surround51", "sink-sunshine-surround71"];

fn pactl(args: &[&str]) -> Option<String> {
    let o = Command::new("pactl").args(args).stdin(Stdio::null()).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// The TV's HDMI sound as the default output again. With the TV's output
/// disabled its sink is gone, so the default falls to the laptop's (muted)
/// speakers, and Sunshine "restores" that one when a client leaves; WirePlumber
/// then keeps preferring it. The sink comes back a moment after the output.
fn tv_audio() {
    std::thread::spawn(|| {
        for _ in 0..20 {
            let sinks = pactl(&["list", "short", "sinks"]).unwrap_or_default();
            if let Some(tv) = sinks.lines().filter_map(|l| l.split('\t').nth(1)).find(|n| n.starts_with(TV_SINK)) {
                if pactl(&["get-default-sink"]).is_none_or(|d| d.trim() != tv) {
                    eprintln!("[stream] default sound output -> {tv}");
                    let _ = pactl(&["set-default-sink", tv]);
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

/// Sunshine's outputs unmuted at 100%: WirePlumber restores whatever volume
/// they were once left at (they were saved muted at ~0, 2026-11-06).
fn sunshine_audio() {
    for s in SUNSHINE_SINKS {
        let _ = pactl(&["set-sink-mute", s, "0"]);
        let _ = pactl(&["set-sink-volume", s, "100%"]);
    }
}

/// Screens on: the TV's output back (mirroring the panel again, as KWin had
/// it) and the panel's brightness. Off: the TV's output disabled, the panel
/// at 1%. While streaming, on does nothing: the TV stays as it is.
pub fn screens(on: bool) {
    if !kiosk() {
        return;
    }
    if on {
        if active() {
            return;
        }
        let mut args: Vec<String> = others()
            .iter()
            .filter(|o| o["connected"] == true && o["enabled"] != true)
            .filter_map(|o| o["name"].as_str().map(|n| format!("output.{n}.enable")))
            .collect();
        if !args.is_empty() {
            kscreen(&args);
        }
        // DPMS: off in versions before this one; a no-op otherwise
        args = vec!["--dpms".into(), "on".into()];
        kscreen(&args);
        tv_audio();
        if let (Some((name, now, _)), Ok(s)) = (backlight(), fs::read_to_string(saved_path())) {
            if let Ok(v) = s.trim().parse::<u32>() {
                if v != now {
                    set_backlight(&name, v);
                }
            }
            let _ = fs::remove_file(saved_path());
        }
    } else {
        let args: Vec<String> = others()
            .iter()
            .filter(|o| o["enabled"] == true)
            .filter_map(|o| o["name"].as_str().map(|n| format!("output.{n}.disable")))
            .collect();
        if !args.is_empty() {
            kscreen(&args);
        }
        if let Some((name, now, max)) = backlight() {
            let dim = (max / 100).max(1);
            if now > dim {
                if !saved_path().exists() {
                    let _ = fs::write(saved_path(), now.to_string());
                }
                set_backlight(&name, dim);
            }
        }
    }
}

fn set(app: &AppHandle, on: bool) {
    if active() == on {
        return;
    }
    STREAMING.store(on, Ordering::Relaxed);
    // a client's pad, which Sunshine keeps after it leaves, is a controller only meanwhile
    azotopad::set_stream_pads(on);
    // the TV was off: back to screens off when the stream ends
    let tv = tv_enabled();
    let dark = !on && !tv;
    if on {
        sunshine_audio();
    } else if tv {
        tv_audio();
    }
    eprintln!("[stream] client {}", if on { "connected" } else { "disconnected" });
    if !on {
        // a game the phone was playing has nobody left to play it
        use tauri::Manager;
        if let Some(g) = app.try_state::<std::sync::Arc<crate::games::Games>>() {
            g.stop_phone();
        }
    }
    let _ = app.emit("stream", json!({ "on": on, "dark": dark }));
}

/// The last client event in a piece of Sunshine's log. A session Sunshine
/// ends itself (`/cancel`, the phone's game closed; a client that vanished)
/// logs no CLIENT DISCONNECTED, but Sunshine always puts back the sound
/// output it took: a default sink that isn't its own means the stream is over.
fn last_event(text: &str) -> Option<bool> {
    text.lines().rev().find_map(|l| {
        if l.contains("CLIENT CONNECTED") {
            Some(true)
        } else if l.contains("CLIENT DISCONNECTED") {
            Some(false)
        } else if let Some(sink) = l.split("Setting default sink to: [").nth(1) {
            (!sink.starts_with("sink-sunshine")).then_some(false)
        } else {
            None
        }
    })
}

/// Reads what Sunshine's log gained since `pos`. Sunshine starts a new log
/// when it starts (the old one is renamed), so a shorter file or a new inode
/// means read from the top.
fn follow(app: &AppHandle, path: &PathBuf, pos: &mut u64, ino: &mut u64) {
    let Ok(meta) = fs::metadata(path) else { return };
    if meta.ino() != *ino || meta.len() < *pos {
        (*pos, *ino) = (0, meta.ino());
    }
    if meta.len() <= *pos {
        return;
    }
    let Ok(mut f) = fs::File::open(path) else { return };
    let mut buf = Vec::new();
    if f.seek(SeekFrom::Start(*pos)).is_err() || f.take(meta.len() - *pos).read_to_end(&mut buf).is_err() {
        return;
    }
    // whole lines only: the rest is read next time
    if let Some(end) = buf.iter().rposition(|&b| b == b'\n') {
        *pos += end as u64 + 1;
        if let Some(on) = last_event(&String::from_utf8_lossy(&buf[..end])) {
            set(app, on);
        }
    }
}

/// Follows Sunshine's log (TV mode only). The first read happens here, before
/// the UI loads: a restart during a stream must know about it before the UI's
/// "screens on" at start (which would take the sound back to the TV).
pub fn spawn(app: AppHandle) {
    if !kiosk() {
        return;
    }
    let Some(home) = std::env::var_os("HOME") else { return };
    let path = PathBuf::from(home).join(".config/sunshine/sunshine.log");
    let (mut pos, mut ino) = (0u64, 0u64);
    follow(&app, &path, &mut pos, &mut ino);
    // (set() only runs on a change: nobody streaming at start needs saying too)
    azotopad::set_stream_pads(active());
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(1));
            follow(&app, &path, &mut pos, &mut ino);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_client_event_wins() {
        let log = "[x]: Info: CLIENT CONNECTED\n[x]: Info: Creating encoder\n[x]: Info: CLIENT DISCONNECTED\n";
        assert_eq!(last_event(log), Some(false));
        assert_eq!(last_event("[x]: Info: CLIENT DISCONNECTED\n[x]: Info: CLIENT CONNECTED"), Some(true));
        assert_eq!(last_event("[x]: Info: Starting main loop"), None);
        // a session cancelled by Sunshine: no DISCONNECTED, only the sound put back
        let cancelled = "[x]: Info: CLIENT CONNECTED\n[x]: Info: Setting default sink to: [sink-sunshine-stereo]\n[x]: Info: Setting default sink to: [alsa_output.pci-0000_01_00.1.hdmi-stereo]\n";
        assert_eq!(last_event(cancelled), Some(false));
        assert_eq!(last_event("[x]: Info: CLIENT CONNECTED\n[x]: Info: Setting default sink to: [sink-sunshine-stereo]\n"), Some(true));
    }
}
