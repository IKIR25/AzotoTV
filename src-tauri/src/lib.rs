mod arr;
mod azahar;
mod controllers;
mod dolphin;
mod downloads;
mod eden;
mod games;
mod jellyfin;
mod kwin;
mod bgm;
mod mii;

/// Tests that write player data share one data folder, set once: the
/// environment is the whole process's and tests run in parallel.
#[cfg(test)]
pub(crate) fn test_data_home() -> std::path::PathBuf {
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("azototv-test-{}", std::process::id()));
        // SAFETY: set once, before any test reads it (they all come through here)
        unsafe { std::env::set_var("XDG_DATA_HOME", &dir) };
        dir
    })
    .clone()
}
mod minecraft;
mod mods;
mod music;
mod notify;
mod overlay;
mod pad;
mod phone;
mod phonejf;
mod phonescreen;
mod phonestream;
mod player;
mod profile;
mod remote;
mod immich;
mod seerr;
mod songs;
mod sound;
mod steamemu;
mod stream;
mod tv;
mod youtube;
mod ytaccount;
mod ytdl;

use std::sync::Arc;

use games::Games;
use jellyfin::{Jf, Session};
use music::Music;
use notify::Notify;
use player::{PlayArgs, Player};
use immich::Immich;
use seerr::Seerr;
use songs::Songs;
use youtube::Yt;
use ytaccount::Account;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

type Jfs<'a> = State<'a, Arc<Jf>>;
type Seerrs<'a> = State<'a, Arc<Seerr>>;

// ---- Jellyfin commands --------------------------------------------------

/// Sessions handed to JS never carry the token.
fn redact(mut s: Session) -> Session {
    s.token.clear();
    s
}

#[tauri::command]
async fn jf_login(jf: Jfs<'_>, base: String, user: String, pass: String) -> Result<Session, String> {
    jf.login(&base, &user, &pass).await.map(redact)
}

#[tauri::command]
fn jf_session(jf: Jfs<'_>) -> Option<Session> {
    jf.session().map(redact)
}

#[tauri::command]
fn jf_server(jf: Jfs<'_>) -> Option<String> {
    jf.server()
}

#[tauri::command]
async fn jf_set_server(jf: Jfs<'_>, base: String) -> Result<String, String> {
    jf.set_server(&base).await
}

#[tauri::command]
async fn jf_profiles(jf: Jfs<'_>) -> Result<jellyfin::Profiles, String> {
    let mut list = jf.profiles().await?;
    for p in &mut list.profiles {
        p.pin = profile::has_pin(&p.id);
    }
    Ok(list)
}

#[tauri::command]
fn jf_switch(jf: Jfs<'_>, user_id: String) -> Result<Session, String> {
    jf.switch(&user_id).map(redact)
}

/// Back to the profile picker; the profile stays signed in.
#[tauri::command]
fn jf_deactivate(jf: Jfs<'_>) {
    jf.deactivate();
}

/// Forget a profile on this device and revoke its token.
#[tauri::command]
async fn jf_sign_out(jf: Jfs<'_>, user_id: String) -> Result<(), String> {
    jf.sign_out(&user_id).await;
    Ok(())
}

#[tauri::command]
async fn jf_request(
    jf: Jfs<'_>,
    method: String,
    path: String,
    query: Vec<(String, String)>,
    body: Option<Value>,
) -> Result<Value, String> {
    jf.request(&method, &path, &query, body).await
}

// ---- Seerr (requests) ------------------------------------------------------

fn jf_user(jf: &Jf) -> Result<String, String> {
    jf.session().map(|s| s.user_id).ok_or_else(|| "not logged in".into())
}

#[tauri::command]
async fn seerr_status(jf: Jfs<'_>, se: Seerrs<'_>) -> Result<seerr::Status, String> {
    Ok(se.status(&jf_user(&jf)?).await)
}

#[tauri::command]
async fn seerr_request(
    jf: Jfs<'_>,
    se: Seerrs<'_>,
    method: String,
    path: String,
    query: Vec<(String, String)>,
    body: Option<Value>,
) -> Result<Value, String> {
    se.request(&jf_user(&jf)?, &method, &path, &query, body).await
}

/// How far a requested film / series is (the bar on the request page).
#[tauri::command]
async fn dl_progress(
    jf: Jfs<'_>,
    dl: State<'_, Arc<downloads::Downloads>>,
    media: String,
    tmdb: u64,
) -> Result<Option<downloads::Progress>, String> {
    jf_user(&jf)?;
    dl.progress(&media, tmdb).await
}

/// Serve an image (or the error) to the webview.
fn image_response(r: Result<(Vec<u8>, String), String>) -> tauri::http::Response<Vec<u8>> {
    match r {
        Ok((data, ct)) => tauri::http::Response::builder()
            .status(200)
            .header("Content-Type", ct)
            .header("Cache-Control", "max-age=31536000, immutable")
            .body(data)
            .unwrap(),
        Err(e) => tauri::http::Response::builder()
            .status(404)
            .header("Content-Type", "text/plain")
            .body(e.into_bytes())
            .unwrap(),
    }
}

// ---- playback ------------------------------------------------------------

#[tauri::command]
async fn player_play(
    app: AppHandle,
    jf: Jfs<'_>,
    pl: State<'_, Arc<Player>>,
    music: State<'_, Arc<Music>>,
    args: PlayArgs,
) -> Result<(), String> {
    music.shutdown().await; // a film/episode takes over the screen and the speakers
    pl.inner().clone().play(app, jf.inner().clone(), args).await
}

/// Live TV: change channel in the running player.
#[tauri::command]
async fn player_channel(
    jf: Jfs<'_>,
    pl: State<'_, Arc<Player>>,
    channel_id: String,
    title: String,
    subtitle: String,
    extra: Value,
) -> Result<(), String> {
    pl.switch_channel(&jf, &channel_id, &title, &subtitle, &extra).await
}

#[tauri::command]
async fn player_cmd(pl: State<'_, Arc<Player>>, args: Vec<Value>) -> Result<(), String> {
    pl.command(args).await;
    Ok(())
}

#[tauri::command]
async fn player_stop(pl: State<'_, Arc<Player>>) -> Result<(), String> {
    pl.command(vec![Value::from("quit")]).await;
    Ok(())
}

// ---- music ---------------------------------------------------------------

/// Play `ids` as the queue, starting at `index` (state arrives as `music-state`).
#[tauri::command]
async fn music_play(
    app: AppHandle,
    jf: Jfs<'_>,
    music: State<'_, Arc<Music>>,
    ids: Vec<String>,
    index: usize,
) -> Result<(), String> {
    music.inner().clone().play(app, jf.inner().clone(), ids, index).await
}

/// Reorder the queue (shuffle) without interrupting `ids[index]`, the current track.
#[tauri::command]
async fn music_requeue(jf: Jfs<'_>, music: State<'_, Arc<Music>>, ids: Vec<String>, index: usize) -> Result<(), String> {
    music.requeue(&jf, ids, index).await
}

/// Raw mpv command for the music player, e.g. ["cycle", "pause"].
#[tauri::command]
async fn music_cmd(music: State<'_, Arc<Music>>, args: Vec<Value>) -> Result<(), String> {
    music.command(args).await;
    Ok(())
}

#[tauri::command]
async fn music_stop(music: State<'_, Arc<Music>>) -> Result<(), String> {
    music.shutdown().await;
    Ok(())
}

// ---- games -----------------------------------------------------------------

/// The library, with the current profile's play history.
#[tauri::command]
fn game_list(jf: Jfs<'_>) -> Result<Vec<games::GameInfo>, String> {
    Ok(games::list(&jf_user(&jf)?))
}

/// The current profile's achievements in a game (Steam API emulator games).
#[tauri::command]
fn game_achievements(jf: Jfs<'_>, id: String) -> Result<Vec<steamemu::Achievement>, String> {
    games::achievements(&jf_user(&jf)?, &id)
}

/// Switch one of a game's mods on or off for the current profile; returns the game's mods.
#[tauri::command]
fn game_set_mod(jf: Jfs<'_>, id: String, mod_id: String, on: bool) -> Result<Vec<mods::ModInfo>, String> {
    games::set_mod(&jf_user(&jf)?, &id, &mod_id, on)
}

/// Play one of a game's trailers in the video player.
#[tauri::command]
async fn game_trailer(
    app: AppHandle,
    jf: Jfs<'_>,
    pl: State<'_, Arc<Player>>,
    music: State<'_, Arc<Music>>,
    id: String,
    index: usize,
    title: String,
) -> Result<(), String> {
    let (url, name) = games::trailer_url(&id, index)?;
    music.shutdown().await;
    let args: PlayArgs = serde_json::from_value(serde_json::json!({
        "item_id": format!("trailer:{id}"),
        "media_type": "Video",
        "start_ticks": 0,
        "title": title,
        "subtitle": name,
        "trailer": url,
    }))
    .map_err(|e| e.to_string())?;
    pl.inner().clone().play(app, jf.inner().clone(), args).await
}

#[tauri::command]
fn game_settings(jf: Jfs<'_>) -> Result<games::Settings, String> {
    Ok(games::settings(&jf_user(&jf)?))
}

#[tauri::command]
fn game_save_settings(jf: Jfs<'_>, settings: games::Settings) -> Result<games::Settings, String> {
    games::save_settings(&jf_user(&jf)?, settings)
}

/// Id of the game running now, if any.
#[tauri::command]
fn game_running(g: State<'_, Arc<Games>>) -> Option<String> {
    g.running()
}

/// Start a game (for games with chapters: which one, default the first) for
/// the current profile; `game-ended` follows when it exits.
#[tauri::command]
async fn game_launch(
    app: AppHandle,
    jf: Jfs<'_>,
    g: State<'_, Arc<Games>>,
    id: String,
    chapter: Option<usize>,
    world: Option<String>,
) -> Result<(), String> {
    let s = jf.session().ok_or("not logged in")?;
    let g = g.inner().clone();
    tauri::async_runtime::spawn_blocking(move || g.launch(app, &s.user_id, &s.user_name, &id, chapter, world, false))
        .await
        .map_err(|e| e.to_string())?
        .inspect_err(|e| eprintln!("[games] launch failed: {e}"))
}

// ---- AzotoTV's own profile settings (profile.rs) -----------------------------

#[tauri::command]
fn profile_prefs(jf: Jfs<'_>) -> Result<profile::Prefs, String> {
    Ok(profile::prefs(&jf_user(&jf)?))
}

/// The active profile's PIN: 4 digits, or none (`null`).
#[tauri::command]
fn profile_set_pin(jf: Jfs<'_>, pin: Option<String>) -> Result<(), String> {
    profile::set_pin(&jf_user(&jf)?, pin.as_deref())
}

/// The picker: whether `pin` opens that profile.
#[tauri::command]
fn profile_check_pin(user_id: String, pin: String) -> Result<bool, String> {
    profile::check_pin(&user_id, &pin)
}

#[tauri::command]
fn profile_set_tg_alerts(jf: Jfs<'_>, on: bool) -> Result<(), String> {
    profile::set_tg_alerts(&jf_user(&jf)?, on)
}

// ---- the Mii app (mii.rs) ----------------------------------------------------

#[tauri::command]
fn mii_library(jf: Jfs<'_>) -> Result<serde_json::Value, String> {
    mii::library(&jf_user(&jf)?)
}

/// Adds or replaces (same `id`) Miis in the active profile's library; `to`
/// gives them to another profile instead (sending a copy).
#[tauri::command]
fn mii_put(jf: Jfs<'_>, miis: Vec<serde_json::Value>, to: Option<String>) -> Result<(), String> {
    let user = match to {
        Some(u) => u,
        None => jf_user(&jf)?,
    };
    mii::put(&user, miis)
}

#[tauri::command]
fn mii_people_put(jf: Jfs<'_>, people: Vec<serde_json::Value>, to: Option<String>) -> Result<(), String> {
    let user = match to {
        Some(u) => u,
        None => jf_user(&jf)?,
    };
    mii::put_people(&user, people)
}

#[tauri::command]
fn mii_person_remove(jf: Jfs<'_>, id: String) -> Result<(), String> {
    mii::remove_person(&jf_user(&jf)?, &id)
}

#[tauri::command]
fn mii_remove(jf: Jfs<'_>, id: String) -> Result<(), String> {
    mii::remove(&jf_user(&jf)?, &id)
}

#[tauri::command]
fn mii_reorder(jf: Jfs<'_>, ids: Vec<String>) -> Result<(), String> {
    mii::reorder(&jf_user(&jf)?, &ids)
}

/// FFLResHigh.dat, to draw Miis with (raw bytes, ~4 MB, asked once).
#[tauri::command]
fn mii_ffl_resource() -> Result<tauri::ipc::Response, String> {
    mii::ffl_resource().map(tauri::ipc::Response::new)
}

/// The Mii app's music (a Mii maker theme, looped) on or off.
#[tauri::command]
fn bgm_play(track: String) -> Result<(), String> {
    bgm::play(&track)
}

#[tauri::command]
fn bgm_stop() {
    bgm::stop()
}

/// Mii databases games changed, for the library to merge.
#[tauri::command]
fn mii_inbox(jf: Jfs<'_>) -> Result<serde_json::Value, String> {
    mii::inbox_list(&jf_user(&jf)?)
}

#[tauri::command]
fn mii_inbox_done(jf: Jfs<'_>, names: Vec<String>) -> Result<(), String> {
    mii::inbox_done(&jf_user(&jf)?, &names)
}

/// The Miis on the consoles right now (Eden's, this profile's Wii), raw.
#[tauri::command]
fn mii_sources(jf: Jfs<'_>) -> Result<serde_json::Value, String> {
    mii::sources(&jf_user(&jf)?)
}

// ---- notifications -----------------------------------------------------------

/// The list (all of them: it's a shared TV), with what this profile hasn't seen.
#[tauri::command]
fn notify_feed(jf: Jfs<'_>, n: State<'_, Arc<Notify>>) -> notify::Feed {
    n.feed(&jf.session().map(|s| s.user_id).unwrap_or_default())
}

/// The profile opened the list.
#[tauri::command]
fn notify_seen(jf: Jfs<'_>, n: State<'_, Arc<Notify>>) -> Result<(), String> {
    n.seen(&jf_user(&jf)?);
    Ok(())
}

#[tauri::command]
fn notify_clear(n: State<'_, Arc<Notify>>) {
    n.clear();
}

/// "Ricordamelo" on a programme (again: forget it). Returns whether it is set.
#[tauri::command]
fn reminder_toggle(jf: Jfs<'_>, n: State<'_, Arc<Notify>>, mut reminder: notify::Reminder) -> bool {
    reminder.user = jf.session().map(|s| s.user_id).unwrap_or_default();
    n.toggle_reminder(reminder)
}

/// An interface sound ("move", "select", "back"): see sound.rs.
#[tauri::command]
fn ui_sound(name: String) {
    if let Some(s) = sound::Sound::parse(&name) {
        sound::play(s);
    }
}

// ---- system (START menu) ---------------------------------------------------

/// "Riavvia": reboot the machine. Only in TV mode (AZOTOTV_FULLSCREEN), so a dev
/// run can't reboot the desktop it runs on. There is no power off: the machine
/// is the home server (Jellyfin, downloads…), "Spegni" is a standby in the UI.
#[tauri::command]
async fn system_reboot() -> Result<(), String> {
    if std::env::var_os("AZOTOTV_FULLSCREEN").is_none() {
        return Err("disponibile solo sulla TV".into());
    }
    let out = tauri::async_runtime::spawn_blocking(|| std::process::Command::new("systemctl").arg("reboot").output())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Screens off / on after a long idle (stream.rs `screens`: the TV's output
/// off and the panel dimmed, so Sunshine can still stream it). With the TV off
/// KWin draws only to the panel on the Intel iGPU, so the NVIDIA stays in its
/// lowest power state. TV mode only (AZOTOTV_FULLSCREEN), so a dev run leaves
/// the desktop's screens alone.
#[tauri::command]
async fn display_power(on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || stream::screens(on)).await.map_err(|e| e.to_string())
}

/// Leave AzotoTV, back to the desktop session it runs in.
#[tauri::command]
fn system_exit(app: AppHandle) {
    app.exit(0);
}

/// Dev convenience: credentials from the environment (never from source).
#[tauri::command]
fn dev_credentials() -> Option<(String, String, String)> {
    Some((
        std::env::var("AZOTOTV_JF_URL").ok()?,
        std::env::var("AZOTOTV_JF_USER").ok()?,
        std::env::var("AZOTOTV_JF_PASS").ok()?,
    ))
}

/// True when the app runs with AZOTOTV_DEBUG set (extra tracing).
#[tauri::command]
fn debug_enabled() -> bool {
    std::env::var_os("AZOTOTV_DEBUG").is_some()
}

/// Name of the connected controller, if any.
#[tauri::command]
fn pad_info() -> Option<String> {
    pad::pad_name()
}

/// Dev: AZOTOTV_KEYS=Right,Right,Down replays arrow presses and logs the focus.
#[tauri::command]
fn dev_keys() -> Option<String> {
    std::env::var("AZOTOTV_KEYS").ok()
}

/// Dev: AZOTOTV_AUTOPLAY=<itemId>:<seconds> plays an item, stops it, exits.
#[tauri::command]
fn dev_autoplay() -> Option<String> {
    std::env::var("AZOTOTV_AUTOPLAY").ok()
}

#[tauri::command]
async fn yt_search(yt: State<'_, Arc<Yt>>, query: String, recent: Option<bool>) -> Result<youtube::SearchResult, String> {
    yt.search(&query, recent.unwrap_or(false)).await
}

#[tauri::command]
async fn yt_video(yt: State<'_, Arc<Yt>>, id: String) -> Result<youtube::VideoPage, String> {
    yt.video(&id).await
}

#[tauri::command]
async fn yt_comments(yt: State<'_, Arc<Yt>>, token: String) -> Result<youtube::Comments, String> {
    yt.comments(&token).await
}

#[tauri::command]
async fn yt_channel(yt: State<'_, Arc<Yt>>, id: String) -> Result<youtube::ChannelPage, String> {
    yt.channel(&id).await
}

/// Newest normal videos (or only Shorts) of the subscribed channels.
#[tauri::command]
async fn yt_feed(yt: State<'_, Arc<Yt>>, channels: Vec<String>, shorts: bool) -> Result<Vec<youtube::Video>, String> {
    Ok(yt.inner().clone().feed(channels, shorts).await)
}

// ---- the active profile's Google account (ytaccount.rs) ----

#[tauri::command]
fn yt_account(jf: Jfs<'_>, acc: State<'_, Arc<Account>>) -> Result<Option<ytaccount::Info>, String> {
    Ok(acc.info(&jf_user(&jf)?))
}

/// Start signing in: the code to approve on a phone; the end comes as event `yt-account`.
#[tauri::command]
async fn yt_link_start(app: AppHandle, jf: Jfs<'_>, acc: State<'_, Arc<Account>>) -> Result<ytaccount::Code, String> {
    acc.inner().clone().start(app, jf_user(&jf)?).await
}

#[tauri::command]
fn yt_link_cancel(jf: Jfs<'_>, acc: State<'_, Arc<Account>>) -> Result<(), String> {
    acc.cancel(&jf_user(&jf)?);
    Ok(())
}

#[tauri::command]
async fn yt_unlink(jf: Jfs<'_>, acc: State<'_, Arc<Account>>) -> Result<(), String> {
    acc.unlink(&jf_user(&jf)?).await;
    Ok(())
}

#[tauri::command]
async fn yt_account_home(jf: Jfs<'_>, acc: State<'_, Arc<Account>>) -> Result<ytaccount::Home, String> {
    acc.home(&jf_user(&jf)?).await
}

#[tauri::command]
async fn yt_account_subs(jf: Jfs<'_>, acc: State<'_, Arc<Account>>) -> Result<Vec<youtube::Video>, String> {
    acc.subscriptions(&jf_user(&jf)?).await
}

/// Play a YouTube video in the AzotoTV player (mpv + yt-dlp), from `start` seconds.
#[tauri::command]
async fn yt_play(
    app: AppHandle,
    jf: Jfs<'_>,
    pl: State<'_, Arc<Player>>,
    music: State<'_, Arc<Music>>,
    id: String,
    title: String,
    subtitle: String,
    start: f64,
) -> Result<(), String> {
    music.shutdown().await;
    let args: PlayArgs = serde_json::from_value(serde_json::json!({
        "item_id": format!("yt:{id}"),
        "media_type": "Video",
        "start_ticks": (start.max(0.0) * 10_000_000.0) as i64,
        "title": title,
        "subtitle": subtitle,
        "youtube": id,
    }))
    .map_err(|e| e.to_string())?;
    pl.inner().clone().play(app, jf.inner().clone(), args).await
}

/// A video a phone sent ("Manda alla TV", anything but YouTube): mpv opens
/// the link through yt-dlp.
#[tauri::command]
async fn phone_play(
    app: AppHandle,
    jf: Jfs<'_>,
    pl: State<'_, Arc<Player>>,
    music: State<'_, Arc<Music>>,
    url: String,
    title: String,
    subtitle: String,
) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("not a link".into());
    }
    music.shutdown().await;
    let args: PlayArgs = serde_json::from_value(serde_json::json!({
        "item_id": "phone",
        "media_type": "Video",
        "start_ticks": 0,
        "title": title,
        "subtitle": subtitle,
        "trailer": url,
    }))
    .map_err(|e| e.to_string())?;
    pl.inner().clone().play(app, jf.inner().clone(), args).await
}

/// Songs on YouTube Music (Musica's search).
#[tauri::command]
async fn ytm_search(yt: State<'_, Arc<Yt>>, query: String) -> Result<Vec<youtube::Song>, String> {
    yt.songs(&query).await
}

/// Downloads (queue, what's in the library already); `fresh`: ask Jellyfin again.
#[tauri::command]
async fn songs_status(songs: State<'_, Arc<Songs>>, fresh: Option<bool>) -> Result<songs::Status, String> {
    Ok(songs.status(fresh.unwrap_or(false)).await)
}

/// Download a YouTube song into the library, then into these playlists of the current profile.
#[tauri::command]
async fn songs_download(
    app: AppHandle,
    jf: Jfs<'_>,
    songs: State<'_, Arc<Songs>>,
    song: youtube::Song,
    playlists: Vec<String>,
) -> Result<(), String> {
    songs.download(&app, song, jf_user(&jf)?, playlists).await
}

type Ims<'a> = State<'a, Arc<Immich>>;

/// Whether the Galleria has a server (`~/.config/azototv/immich.json`).
#[tauri::command]
fn im_enabled(im: Ims<'_>) -> bool {
    im.enabled()
}

#[tauri::command]
async fn im_albums(im: Ims<'_>, fresh: Option<bool>) -> Result<Vec<immich::Album>, String> {
    im.albums(fresh.unwrap_or(false)).await
}

#[tauri::command]
async fn im_assets(im: Ims<'_>, album: String, page: u32) -> Result<immich::AssetPage, String> {
    im.assets(&album, page).await
}

#[tauri::command]
async fn im_on_this_day(im: Ims<'_>, today: String) -> Result<Vec<immich::Memory>, String> {
    im.on_this_day(&today).await
}

#[tauri::command]
async fn im_random(im: Ims<'_>, n: u32, albums: Option<Vec<String>>) -> Result<Vec<immich::Asset>, String> {
    im.random(n.min(200), albums).await
}

/// Play a family video in the AzotoTV player, straight from Immich.
#[tauri::command]
async fn im_play(
    app: AppHandle,
    jf: Jfs<'_>,
    im: Ims<'_>,
    pl: State<'_, Arc<Player>>,
    music: State<'_, Arc<Music>>,
    id: String,
    title: String,
    subtitle: String,
) -> Result<(), String> {
    let source = im.video(&id)?;
    music.shutdown().await;
    let args: PlayArgs = serde_json::from_value(serde_json::json!({
        "item_id": format!("im:{id}"),
        "media_type": "Video",
        "start_ticks": 0,
        "title": title,
        "subtitle": subtitle,
        "photo_video": source,
    }))
    .map_err(|e| e.to_string())?;
    pl.inner().clone().play(app, jf.inner().clone(), args).await
}

/// Dev: AZOTOTV_GAME=<id> launches a game from the library after login.
#[tauri::command]
fn dev_game() -> Option<String> {
    std::env::var("AZOTOTV_GAME").ok()
}

/// Dev/screenshots: open a specific route after login (AZOTOTV_ROUTE=/item/<id>).
#[tauri::command]
fn dev_route() -> Option<String> {
    std::env::var("AZOTOTV_ROUTE").ok()
}

/// Dev: AZOTOTV_SAVER_SECS=<n> starts the screensaver after n idle seconds.
#[tauri::command]
fn dev_saver_secs() -> Option<u32> {
    std::env::var("AZOTOTV_SAVER_SECS").ok()?.parse().ok()
}

/// Dev: a line from the UI on stderr (scripted runs, AZOTOTV_DEBUG tracing).
#[tauri::command]
fn dev_log(line: String) {
    eprintln!("[dev] {line}");
}

/// `azototv --arr-check [n]`: the last n Radarr / Sonarr imports through the
/// notification pipeline once, printed instead of posted (no UI).
/// `--3ds-check <player dir> <name> <game> [split]`: a player's 3DS made
/// ready as a launch would (config, Italian + name), and the command printed.
pub fn azahar_check(player: &str, name: &str, game: &str, split: bool) {
    match azahar::command(std::path::Path::new(player), name, std::path::Path::new(game), split) {
        Ok(c) => {
            let env: Vec<String> = c.get_envs().map(|(k, v)| format!("{}={}", k.to_string_lossy(), v.map(|v| v.to_string_lossy().into_owned()).unwrap_or_default())).collect();
            let args: Vec<String> = c.get_args().map(|a| format!("{:?}", a.to_string_lossy())).collect();
            println!("{} {} {}", env.join(" "), c.get_program().to_string_lossy(), args.join(" "));
        }
        Err(e) => eprintln!("{e}"),
    }
}

pub fn arr_check(n: usize) {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    rt.block_on(async {
        let jf = Arc::new(Jf::new());
        match arr::Arr::new(jf, Arc::new(Seerr::new()), Notify::new(), true) {
            Some(a) => a.check(n).await,
            None => eprintln!("no ~/.config/azototv/arr.json"),
        }
    });
}

/// `azototv --dl-check movie|tv <tmdb id>`: a request's progress, printed (no UI).
pub fn dl_check(media: &str, tmdb: u64) {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    rt.block_on(downloads::check(media, tmdb));
}

/// `azototv --tv pair|on|off|idle-off|renew` (see tv.rs).
pub fn tv_cli(cmd: &str) {
    tv::cli(cmd)
}

/// `azototv --song-check <YouTube id> [playlist id] [user id]`: one Musica download, no UI.
pub fn song_check(id: &str, playlist: Option<(String, String)>) {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    rt.block_on(songs::check(id, playlist));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let jf = Arc::new(Jf::new());
    let jf_proto = jf.clone();
    let se = Arc::new(Seerr::new());
    let se_proto = se.clone();
    let notify = Notify::new();
    let yt = Yt::new();
    let yt_proto = yt.clone();
    let im = Arc::new(Immich::new());
    let songs = Songs::new();
    let im_proto = im.clone();

    let app = tauri::Builder::default()
        .manage(jf)
        .manage(se)
        .manage(Arc::new(downloads::Downloads::new()))
        .manage(notify)
        .manage(Player::new())
        .manage(Music::new())
        .manage(Games::new())
        .manage(yt)
        .manage(Account::new())
        .manage(im)
        .manage(songs)
        .manage(tv::Tv::load())
        .manage(phone::Phone::new())
        .manage(controllers::Controllers::new())
        // jf://localhost/Items/<id>/Images/Primary?maxWidth=400 -> Jellyfin, cached on disk
        .register_asynchronous_uri_scheme_protocol("jf", move |_ctx, request, responder| {
            let jf = jf_proto.clone();
            let uri = request.uri().clone();
            tauri::async_runtime::spawn(async move {
                let pq = uri
                    .path_and_query()
                    .map(|p| p.as_str().to_string())
                    .unwrap_or_default();
                responder.respond(image_response(jf.image(&pq).await));
            });
        })
        // tmdb://localhost/w342/<file>.jpg -> image.tmdb.org, cached on disk
        .register_asynchronous_uri_scheme_protocol("tmdb", move |_ctx, request, responder| {
            let se = se_proto.clone();
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn(async move {
                responder.respond(image_response(se.tmdb_image(&path).await));
            });
        })
        // game://localhost/<game id>/<cover|hero|logo|header> -> the game's art folder
        .register_asynchronous_uri_scheme_protocol("game", move |_ctx, request, responder| {
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn_blocking(move || responder.respond(image_response(games::art(&path))));
        })
        // yt://localhost/vi/<video id>/<file> and /av/<avatar URL> -> YouTube, cached on disk
        .register_asynchronous_uri_scheme_protocol("yt", move |_ctx, request, responder| {
            let yt = yt_proto.clone();
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn(async move {
                responder.respond(image_response(yt.image(&path).await));
            });
        })
        // im://localhost/<asset id>/<thumbnail|preview> -> Immich (Galleria), cached on disk
        .register_asynchronous_uri_scheme_protocol("im", move |_ctx, request, responder| {
            let im = im_proto.clone();
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn(async move {
                responder.respond(image_response(im.image(&path).await));
            });
        })
        .setup(|app| {
            std::thread::spawn(youtube::prune_cache);
            std::thread::spawn(immich::prune_cache);
            app.state::<Arc<Songs>>().start(app.handle().clone());
            pad::spawn(app.handle().clone());
            controllers::watch_batteries(app.handle().clone());
            remote::spawn(app.handle().clone());
            stream::spawn(app.handle().clone());
            app.state::<Arc<tv::Tv>>().start(app.handle().clone());
            app.state::<Arc<phone::Phone>>().start(app.handle().clone());
            // notifications: the toast overlay, reminders, new films / episodes
            overlay::init();
            let n = app.state::<Arc<Notify>>().inner().clone();
            n.start(app.handle().clone());
            let jf = app.state::<Arc<Jf>>().inner().clone();
            let se = app.state::<Arc<Seerr>>().inner().clone();
            match arr::Arr::new(jf, se, n, false) {
                Some(a) => a.start(),
                None => eprintln!("[arr] no ~/.config/azototv/arr.json: no new-arrival notifications"),
            }
            // A single-page app needs no back/forward page cache and only a small
            // memory cache (images are cached on disk by us): WebKit's
            // "document viewer" model keeps the web process smaller.
            // Measured on the TV: ~45 MB less for the web process.
            #[cfg(target_os = "linux")]
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.with_webview(|wv| {
                    use webkit2gtk::{CacheModel, WebContextExt, WebViewExt};
                    if let Some(ctx) = wv.inner().context() {
                        ctx.set_cache_model(CacheModel::DocumentViewer);
                    }
                });
            }
            // Kiosk / TV mode: AZOTOTV_FULLSCREEN=1 fills the output.
            if std::env::var_os("AZOTOTV_FULLSCREEN").is_some() {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.set_fullscreen(true);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tv::tv_power,
            tv::tv_unpaired,
            tv::tv_pair_start,
            tv::tv_pair_pin,
            tv::tv_pair_cancel,
            phone::phone_pairing,
            controllers::controllers_list,
            controllers::controllers_watch,
            phone::phone_pairing_stop,
            phone::phone_list,
            phone::phone_forget,
            phone::phone_state,
            phone::phone_screen_stop,
            phone::phone_apps,
            jf_login,
            jf_session,
            jf_server,
            jf_set_server,
            jf_profiles,
            jf_switch,
            jf_deactivate,
            jf_sign_out,
            jf_request,
            seerr_status,
            seerr_request,
            dl_progress,
            player_play,
            player_cmd,
            player_channel,
            player_stop,
            music_play,
            music_requeue,
            music_cmd,
            music_stop,
            game_list,
            game_achievements,
            game_set_mod,
            game_trailer,
            game_settings,
            game_save_settings,
            game_running,
            game_launch,
            yt_search,
            yt_video,
            yt_channel,
            yt_comments,
            yt_feed,
            yt_play,
            yt_account,
            yt_link_start,
            yt_link_cancel,
            yt_unlink,
            yt_account_home,
            yt_account_subs,
            phone_play,
            im_enabled,
            ytm_search,
            songs_status,
            songs_download,
            im_albums,
            im_assets,
            im_on_this_day,
            im_random,
            im_play,
            profile_prefs,
            mii_library,
            mii_put,
            mii_remove,
            mii_people_put,
            mii_person_remove,
            mii_reorder,
            mii_sources,
            mii_inbox,
            mii_inbox_done,
            bgm_play,
            bgm_stop,
            mii_ffl_resource,
            profile_set_pin,
            profile_check_pin,
            profile_set_tg_alerts,
            notify_feed,
            notify_seen,
            notify_clear,
            reminder_toggle,
            ui_sound,
            dev_game,
            system_reboot,
            display_power,
            system_exit,
            dev_credentials,
            dev_route,
            dev_saver_secs,
            dev_autoplay,
            dev_keys,
            pad_info,
            debug_enabled,
            dev_log
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        // Never leave an mpv window (or a game) behind when the UI goes away.
        if let tauri::RunEvent::Exit = event {
            handle.state::<Arc<Games>>().shutdown();
            let pl = handle.state::<Arc<Player>>().inner().clone();
            let music = handle.state::<Arc<Music>>().inner().clone();
            tauri::async_runtime::block_on(async move {
                pl.shutdown().await;
                music.shutdown().await;
            });
        }
    });
}
