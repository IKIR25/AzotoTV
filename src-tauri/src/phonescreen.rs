//! The phone as the touch screen of the Switch game on the TV: the iPhone app
//! shows the game's picture and its touches go back as a real touchscreen,
//! which Eden and the recomps read like any touchscreen (SDL / Qt).
//!
//! The picture comes from a second Sunshine, started here only while a phone
//! uses it (transient user unit `azototv-screen`, its files in
//! `sunshine-screen/` of AzotoTV's data). The main one (stream.rs) can't do
//! it: it streams the laptop panel, and during a game on the TV the panel is
//! off (games.rs `set_panel`). So this one copies the screen the game is on:
//! - on the TV: the TV's output, scanned out by the NVIDIA, encoded there
//!   (NVENC; KMS numbering on the NVIDIA only, where the TV is 0);
//! - while a PC plays it through Moonlight (stream.rs `active`): the panel,
//!   as the main Sunshine does (Intel, VAAPI).
//!
//! No sound (`stream_audio = disabled`, and the session plays it on the host):
//! the phone is only a screen. Its ports are the main ones + 1000 (HTTPS
//! 48984, RTSP 49010, UDP 48998-49000), open to the LAN only.
//!
//! Touches: Sunshine's virtual "libvirtualhid Touchscreen" is a touchscreen
//! for KWin, which maps it to one output; [`ready`] maps it to the screen
//! being sent, so a finger lands where the phone shows it.
//!
//! - a two-screen game (3DS): the panel too, which then is a screen of its
//!   own holding the second screen (the 3DS's bottom one: games.rs
//!   `split_screens`, kwin.rs), while the TV shows the main one.

use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::Mutex,
    time::Duration,
};

use crate::phonestream::{Session, Sunshine};

const HTTPS: &str = "https://127.0.0.1:48984";
const PORT: u16 = 48989;
const UNIT: &str = "azototv-screen";
/// What AzotoTV calls itself in this Sunshine's state.
const NAME: &str = "AzotoTV";

const INTEL: &str = "/dev/dri/by-path/pci-0000:00:02.0-render";
const NVIDIA: &str = "/dev/dri/by-path/pci-0000:01:00.0-render";

/// The screen the running instance sends (None: not running).
static TARGET: Mutex<Option<Target>> = Mutex::new(None);

/// A two-screen game runs with its second screen on the panel.
static SECOND_SCREEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The panel holds a two-screen game's second screen (games.rs), or no more.
pub fn set_second_screen(on: bool) {
    SECOND_SCREEN.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether the panel holds a two-screen game's second screen.
pub fn second_screen() -> bool {
    SECOND_SCREEN.load(std::sync::atomic::Ordering::Relaxed)
}

#[derive(Clone, PartialEq, Debug)]
enum Target {
    /// the TV's output (KWin's name)
    Tv(String),
    Panel,
}

impl Target {
    fn output(&self) -> &str {
        match self {
            Target::Tv(name) => name,
            Target::Panel => crate::stream::PANEL,
        }
    }
}

fn dir() -> PathBuf {
    crate::games::data_dir().join("sunshine-screen")
}

fn config(target: &Target) -> String {
    let d = dir();
    let d = d.display();
    let (encoder, adapter) = match target {
        Target::Tv(_) => ("nvenc", NVIDIA),
        Target::Panel => ("vaapi", INTEL),
    };
    format!(
        "# written by AzotoTV (phonescreen.rs) at every start\n\
         sunshine_name = AzotoTV schermo\n\
         locale = it\n\
         port = {PORT}\n\
         origin_web_ui_allowed = pc\n\
         file_state = {d}/sunshine_state.json\n\
         credentials_file = {d}/sunshine_state.json\n\
         pkey = {d}/key.pem\n\
         cert = {d}/cert.pem\n\
         file_apps = {d}/apps.json\n\
         log_path = {d}/sunshine.log\n\
         stream_audio = disabled\n\
         keyboard = disabled\n\
         controller = disabled\n\
         capture = kms\n\
         output_name = 0\n\
         encoder = {encoder}\n\
         adapter_name = {adapter}\n"
    )
}

/// The config for `target`, the app list, and AzotoTV's certificate among
/// the paired clients (written while this Sunshine is stopped).
fn write_files(target: &Target) -> Result<(), String> {
    let d = dir();
    fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let err = |e: std::io::Error| format!("{}: {e}", d.display());
    fs::write(d.join("sunshine.conf"), config(target)).map_err(err)?;
    // Sunshine aborts on an app list without "env"
    fs::write(d.join("apps.json"), r#"{"env":{},"apps":[{"name":"AzotoTV"}]}"#).map_err(err)?;
    let (cert, _) = crate::phonestream::identity()?;
    let state = d.join("sunshine_state.json");
    let mut v: serde_json::Value = fs::read_to_string(&state)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({ "root": {} }));
    let paired = v["root"]["named_devices"]
        .as_array()
        .is_some_and(|l| l.iter().any(|x| x["cert"].as_str().map(str::trim) == Some(cert.trim())));
    if !paired {
        v["root"]["named_devices"] = serde_json::json!([{ "name": NAME, "cert": cert, "uuid": "4E495452-4F54-5600-0000-53434845524D" }]);
        fs::write(&state, serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(err)?;
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&state, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn systemctl(args: &[&str]) -> bool {
    Command::new("systemctl").arg("--user").args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// This Sunshine sending `target`, started (again) if it isn't. Blocking.
fn ensure_running(target: &Target) -> Result<(), String> {
    let mut current = TARGET.lock().unwrap();
    if current.as_ref() == Some(target) && systemctl(&["is-active", "--quiet", UNIT]) {
        return Ok(());
    }
    systemctl(&["stop", UNIT]);
    systemctl(&["reset-failed", UNIT]);
    *current = None;
    write_files(target)?;
    eprintln!("[phonescreen] starting Sunshine for {}", target.output());
    let mut cmd = Command::new("systemd-run");
    cmd.args(["--user", "--collect", "--quiet", "--unit", UNIT]);
    for var in ["WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "DISPLAY"] {
        if let Ok(v) = std::env::var(var) {
            cmd.arg("-E").arg(format!("{var}={v}"));
        }
    }
    let o = cmd.arg("/usr/bin/sunshine").arg(dir().join("sunshine.conf")).stdin(Stdio::null()).output().map_err(|e| format!("systemd-run: {e}"))?;
    if !o.status.success() {
        return Err(format!("systemd-run: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    // it answers once it found its encoder (~3 s with NVENC)
    for _ in 0..60 {
        std::thread::sleep(Duration::from_millis(250));
        if std::net::TcpStream::connect_timeout(&"127.0.0.1:48984".parse().unwrap(), Duration::from_millis(200)).is_ok() {
            *current = Some(target.clone());
            return Ok(());
        }
        if !systemctl(&["is-active", "--quiet", UNIT]) {
            break;
        }
    }
    systemctl(&["stop", UNIT]);
    Err("Il secondo Sunshine non è partito".into())
}

/// The screen the game is on now.
fn target() -> Result<Target, String> {
    if crate::stream::active() || SECOND_SCREEN.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(Target::Panel);
    }
    crate::stream::tv_output().map(Target::Tv).ok_or_else(|| "La TV è spenta".into())
}

/// A stream of the game's screen for the phone at `host`.
pub async fn start(host: &str, width: u32, height: u32, fps: u32) -> Result<Session, String> {
    let target = tauri::async_runtime::spawn_blocking(target).await.map_err(|e| e.to_string())??;
    let s = tauri::async_runtime::spawn_blocking(move || {
        ensure_running(&target)?;
        Sunshine::connect(HTTPS)
    })
    .await
    .map_err(|e| e.to_string())??;
    crate::phonestream::launch(&s, host, width, height, fps, true).await
}

/// The phone is connected: its touches go to the screen it sees.
pub async fn ready() {
    let Some(output) = TARGET.lock().unwrap().as_ref().map(|t| t.output().to_string()) else { return };
    // Sunshine makes the touchscreen when the client comes in
    for _ in 0..30 {
        if tauri::async_runtime::spawn_blocking({
            let output = output.clone();
            move || map_touch(&output)
        })
        .await
        .unwrap_or(false)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    eprintln!("[phonescreen] no touchscreen to map");
}

fn busctl(args: &[&str]) -> Option<String> {
    let o = Command::new("busctl").arg("--user").args(args).stdin(Stdio::null()).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Every Sunshine touchscreen KWin has, mapped to `output`. Whether there was one.
fn map_touch(output: &str) -> bool {
    let Some(tree) = busctl(&["--list", "tree", "org.kde.KWin"]) else { return false };
    let mut found = false;
    for path in tree.lines().map(str::trim).filter(|p| p.starts_with("/org/kde/KWin/InputDevice/event")) {
        let name = busctl(&["get-property", "org.kde.KWin", path, "org.kde.KWin.InputDevice", "name"]).unwrap_or_default();
        if !name.contains("libvirtualhid Touchscreen") {
            continue;
        }
        found = true;
        if busctl(&["set-property", "org.kde.KWin", path, "org.kde.KWin.InputDevice", "outputName", "s", output]).is_none() {
            eprintln!("[phonescreen] can't map {path} to {output}");
        }
    }
    if found {
        eprintln!("[phonescreen] touches go to {output}");
    }
    found
}

/// The phone is done (or the game ended): session closed, Sunshine stopped.
pub async fn stop() {
    if TARGET.lock().unwrap().is_none() {
        return;
    }
    if let Ok(Ok(s)) = tauri::async_runtime::spawn_blocking(|| Sunshine::connect(HTTPS)).await {
        let _ = s.get("cancel", &[]).await;
    }
    tauri::async_runtime::spawn_blocking(stop_now).await.ok();
}

/// Blocking: this Sunshine stopped (game ended, AzotoTV starting).
pub fn stop_now() {
    let mut current = TARGET.lock().unwrap();
    if current.take().is_some() || systemctl(&["is-active", "--quiet", UNIT]) {
        eprintln!("[phonescreen] stopping its Sunshine");
        systemctl(&["stop", UNIT]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tv_is_sent_by_the_nvidia() {
        let c = config(&Target::Tv("HDMI-A-1".into()));
        assert!(c.contains("encoder = nvenc\n") && c.contains(NVIDIA) && c.contains("port = 48989\n") && c.contains("stream_audio = disabled\n"));
        let p = config(&Target::Panel);
        assert!(p.contains("encoder = vaapi\n") && p.contains(INTEL));
        assert_eq!(Target::Panel.output(), "eDP-1");
    }
}
