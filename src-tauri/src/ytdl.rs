//! yt-dlp for the players (mpv's ytdl hook) and the song downloads.

use std::{fs, os::unix::fs::PermissionsExt};

use tokio::process::Command;

const WAIT_PY: &str = include_str!("ytdl-wait.py");

/// AzotoTV's own yt-dlp, updated daily by a user timer (YouTube changes faster
/// than the distro package); the system one otherwise.
pub fn ytdlp() -> String {
    let own = crate::games::data_dir().join("bin/yt-dlp");
    if own.is_file() { own.display().to_string() } else { "yt-dlp".into() }
}

/// Point mpv's ytdl hook at `ytdl-wait.py`, which holds yt-dlp's answer until
/// YouTube's links may be opened (they 403 for the first few seconds). If the
/// wrapper can't start, the hook falls back to yt-dlp itself.
pub fn hook(cmd: &mut Command) {
    let real = ytdlp();
    let dir = crate::games::data_dir().join("bin");
    let wrap = dir.join("ytdl-wait");
    let ok = fs::create_dir_all(&dir).is_ok()
        && fs::write(&wrap, WAIT_PY).is_ok()
        && fs::set_permissions(&wrap, fs::Permissions::from_mode(0o755)).is_ok();
    let path = if ok { format!("{}:{real}", wrap.display()) } else { real.clone() };
    cmd.env("AZOTOTV_YTDLP", real)
        .arg(format!("--script-opts=ytdl_hook-ytdl_path={path}"));
}
