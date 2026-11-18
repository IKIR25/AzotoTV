//! Games: the library on disk and launching a game.
//!
//! Library: the game folders (`$AZOTOTV_GAMES` or `~/Games` unless
//! `games.json` lists others, see `library.rs`), one folder per game with a
//! `game.json` (the frontend only ever sends a game id, never a command):
//!
//! ```json
//! {"name": "Slime Rancher", "type": "native", "exe": "SlimeRancher.x86_64",
//!  "steam_appid": 433340, "pad": {"mode": "xbox"}}
//! ```
//! `type` is `native`, `proton`, `switch` (see `eden.rs`) or `wii` (`dolphin.rs`); `exe` is relative
//! to the game folder, absolute or `~/...`; optional `args`, `env`. `pad` is an azotopad profile (default xbox).
//! `mods` / `mod_loader`: mods each profile switches on and off on the game
//! page, applied at launch (see `mods.rs`).
//! Next to it, optionally: `info.json` (description, genres, developers, year,
//! trailers [{name, url}]) and `art/{cover,hero,logo,header}.{jpg,png}`, served
//! as `game://localhost/<id>/<kind>` (taken from Steam once, so no network).
//! A game sold in parts lists them as `chapters: [{name, subtitle?,
//! description?, exe, args?, pad?, type?, applet?}]` (each overrides the game's own
//! exe/args/pad/type; picture `art/chapter-<n>.jpg`, from 1).
//! `applet: "mii"` (Switch) opens one of Eden's system applets instead of a
//! game (the Mii editor: see `eden::Applet`); `exe` is then not needed. The game page lists
//! them like a series' episodes; they share the game's saves (same Wine prefix).
//! `player_dirs: ["game/portal2/SAVE", "game/portal2/cfg"]` are folders
//! inside the game where it keeps saves and settings (instead of HOME): each
//! becomes a link to the player's own copy (`link_player_dirs`).
//! `platform: "mobile"` puts a game under Mobile in Giochi (a port of a phone
//! game), `"n64"` / `"nes"` under N64 / NES (native ports of console games),
//! `"arcade"` under Arcade (a native `run.sh` starting MAME on a ROM set);
//! otherwise the type decides (PC, Switch, Wii, 3DS).
//! `phone: {"web": "https://…"}` / `phone: {"args": ["--phone"], "env": {…}}`:
//! the game in the iPhone app's Giochi (Mobile games only). `web` opens the
//! game's web version on the phone; `args`/`env` start its touch "phone mode"
//! on the server, streamed to the phone (not built yet).
//! `fullscreen_key: "f4"` is for games that start windowed and have their own
//! fullscreen key (Undertale's looks better than any scaler): the key is
//! pressed until the game's window really is fullscreen. `gamescope: true`
//! runs a windowed game inside gamescope, fullscreen at the screen's resolution.
//! A Windows game's `prefix-seed/` folder (laid out like a Wine prefix) is
//! copied into each player's prefix at launch, without overwriting anything;
//! a native game's `home-seed/` likewise into the player's home (first
//! settings, the ROM a port reads its assets from).
//!
//! Each player (Jellyfin user id) gets their own saves under
//! `~/.local/share/azototv/players/<id>/`: native games run with HOME and the
//! XDG config/data dirs pointed there, Windows games get their own Wine prefix
//! per game, cloned from a base prefix (`cp --reflink`, nearly free on btrfs).
//!
//! While a game runs, the real pads are handed to azotopad (the game sees the
//! profile's virtual devices), AzotoTV's own pad input is paused and its window
//! hidden. Holding Home (or Back+Start) for 1 s closes the game.

use std::{
    collections::HashMap,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicI32, Ordering},
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Native,
    Proton,
    /// a Switch game (`exe` = .nsp/.xci), run by Eden
    Switch,
    /// a Wii game (`exe` = the disc), run by Dolphin
    Wii,
    /// a 3DS game (`exe` = the decrypted .cci/.3ds), run by Azahar; its
    /// bottom screen goes to the laptop panel, for the phone (azahar.rs)
    #[serde(rename = "3ds")]
    N3ds,
    /// a Prism Launcher instance (`exe` = its folder)
    Minecraft,
    /// installed with Steam on this machine, started through it (`steam_appid`; see `steam.rs`)
    Steam,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Manifest {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: Kind,
    pub exe: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub steam_appid: Option<u32>,
    #[serde(default)]
    pub pad: Option<Value>,
    #[serde(default)]
    pub fullscreen_key: Option<String>,
    #[serde(default)]
    pub gamescope: bool,
    #[serde(default)]
    pub chapters: Vec<Chapter>,
    /// how many can play together on this TV (local multiplayer / split screen):
    /// above 1, the Controller screen comes first (who is player 1, 2…)
    #[serde(default = "one_player")]
    pub players: u32,
    /// folders inside the game where it keeps saves and settings (Source games
    /// do): each player gets their own copy, see `link_player_dirs`
    #[serde(default)]
    pub player_dirs: Vec<String>,
    /// Wii games: how the Wii Remote is held, "sideways" (alone) or, by
    /// default, upright with the Nunchuk (see `dolphin::Layout`)
    #[serde(default)]
    pub wiimote: Option<String>,
    /// "mobile" / "switch" / "n64" / "nes" / "arcade": the Giochi tab, for ports (else the type decides)
    #[serde(default)]
    pub platform: Option<String>,
    /// Switch: an Eden system applet instead of a game ("mii")
    #[serde(default)]
    pub applet: Option<String>,
    /// mods each profile can switch on (see `mods.rs`)
    #[serde(default)]
    pub mods: Vec<crate::mods::ModSpec>,
    /// in place while at least one mod is on (a mod loader)
    #[serde(default)]
    pub mod_loader: Vec<crate::mods::Place>,
    /// how the iPhone app plays it (Mobile games), see the top of this file
    #[serde(default)]
    pub phone: Option<PhoneMode>,
    /// added from the TV: "pc" (a game where it is) or "rom" (found in a ROM folder), see library.rs
    #[serde(default)]
    pub added: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct PhoneMode {
    /// the game's own web version: the phone opens it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web: Option<String>,
    /// added to the game's args for its touch "phone mode" (streamed)
    #[serde(default, skip_serializing)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing)]
    pub env: HashMap<String, String>,
}

/// A Mobile game as the iPhone app sees it.
#[derive(Serialize)]
pub struct PhoneGame {
    pub id: String,
    pub name: String,
    /// which of cover / hero / logo / header exist
    pub art: Vec<&'static str>,
    /// opened on the phone (its web version)
    pub web: Option<String>,
    /// has a touch phone mode, to stream from the server
    pub stream: bool,
}

fn one_player() -> u32 {
    1
}

#[derive(Deserialize, Clone, Debug)]
pub struct Chapter {
    pub name: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub exe: String,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub pad: Option<Value>,
    /// a chapter run by another emulator than the game's (one "Mii" entry:
    /// the Wii's Mii Channel and the Switch's Mii editor)
    #[serde(default, rename = "type")]
    pub kind: Option<Kind>,
    #[serde(default)]
    pub applet: Option<String>,
}

impl Manifest {
    /// The manifest as it applies to one chapter (games without chapters: as is).
    fn for_chapter(&self, chapter: Option<usize>) -> Result<Manifest, String> {
        if self.chapters.is_empty() {
            return Ok(self.clone());
        }
        let c = self.chapters.get(chapter.unwrap_or(0)).ok_or_else(|| t!("chapter not found"))?;
        let mut m = self.clone();
        m.exe = c.exe.clone();
        if let Some(a) = &c.args {
            m.args = a.clone();
        }
        if c.pad.is_some() {
            m.pad = c.pad.clone();
        }
        if let Some(k) = c.kind {
            m.kind = k;
        }
        if c.applet.is_some() {
            m.applet = c.applet.clone();
        }
        Ok(m)
    }
}

/// `info.json`: what the game pages show.
#[derive(Deserialize, Serialize, Clone, Default)]
pub struct Info {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub developers: Vec<String>,
    #[serde(default)]
    pub year: Option<String>,
    #[serde(default)]
    pub trailers: Vec<Trailer>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Trailer {
    pub name: String,
    pub url: String,
}

/// A player's history with a game (`players/<id>/games.json`).
#[derive(Deserialize, Serialize, Clone, Copy, Default)]
pub struct Played {
    /// unix seconds
    pub last: u64,
    pub seconds: u64,
    /// the chapter played last (games with chapters)
    #[serde(default)]
    pub chapter: Option<usize>,
}

const ART: [&str; 4] = ["cover", "hero", "logo", "header"];

/// What the frontend gets for each game.
#[derive(Serialize, Clone)]
pub struct GameInfo {
    pub id: String,
    pub name: String,
    /// when its art last changed (unix seconds): pictures are asked for with
    /// it, so new art isn't taken from the web view's cache
    pub art_stamp: u64,
    /// added from the TV ("pc" / "rom"), see library.rs
    pub added: Option<String>,
    #[serde(rename = "type")]
    pub kind: Kind,
    /// "mobile", "n64", "nes", "arcade", or none (the type decides)
    pub platform: Option<String>,
    pub steam_appid: Option<u32>,
    pub description: String,
    pub genres: Vec<String>,
    pub developers: Vec<String>,
    pub year: Option<String>,
    /// trailer names (played with `game_trailer(id, index)`)
    pub trailers: Vec<String>,
    /// which of cover / hero / logo / header exist
    pub art: Vec<&'static str>,
    /// "xbox" or "keys": how the pad reaches the game
    pub pad: String,
    /// keys games driven like a mouse get a cursor sensitivity
    pub cursor: bool,
    pub played: Option<Played>,
    /// empty: the game is played as a whole
    pub chapters: Vec<ChapterInfo>,
    /// Minecraft: this profile's worlds, most recently played first
    pub worlds: Vec<crate::minecraft::World>,
    /// how many can play together (local multiplayer)
    pub players: u32,
    /// games run with the Steam API emulator: (earned, total) for this profile
    pub achievements: Option<(usize, usize)>,
    /// the game's mods, and which this profile has on
    pub mods: Vec<crate::mods::ModInfo>,
}

#[derive(Serialize, Clone)]
pub struct ChapterInfo {
    pub name: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    /// `art/chapter-<n>` exists (served as `game://localhost/<id>/chapter-<n>`)
    pub art: bool,
}

#[derive(Serialize, Clone)]
struct Ended {
    id: String,
    seconds: u64,
    /// "exit" (the game closed), "quit" (Home held), "error"
    reason: String,
    error: Option<String>,
}

struct Running {
    id: String,
    pgid: Arc<AtomicI32>,
    /// started from the iPhone app in its phone mode (streamed): ends with the stream
    phone: bool,
    /// closed on purpose (Home held, or a phone's Home): it ends as "quit"
    quit: Arc<std::sync::atomic::AtomicBool>,
}

pub struct Games {
    running: Mutex<Option<Running>>,
}

pub(crate) fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into())
}

/// "~/x" -> $HOME/x; anything else as it is.
pub(crate) fn expand_home(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if p == "~" => home(),
        None => PathBuf::from(p),
    }
}

pub(crate) fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"))
        .join("azototv")
}

pub(crate) fn player_dir(user: &str) -> PathBuf {
    data_dir().join("players").join(user)
}

/// A flatpak app installed for this user or the whole system.
pub(crate) fn flatpak_installed(app: &str) -> bool {
    [home().join(".local/share/flatpak/app").join(app), PathBuf::from("/var/lib/flatpak/app").join(app)].iter().any(|p| p.is_dir())
}

pub(crate) fn has_proton() -> bool {
    proton_path().is_some()
}

/// The Proton this machine runs Windows games with (its folder).
pub(crate) fn proton_dir() -> Option<PathBuf> {
    proton_path()
}

/// "GE-Proton11-12" -> [11, 12], to sort versions as numbers (11-12 after 11-9).
fn version_key(name: &str) -> Vec<u32> {
    name.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect()
}

/// Proton for umu: $AZOTOTV_PROTON, else the distro's proton-cachyos, else the
/// newest GE-Proton in Steam's compatibilitytools.d, else a Proton Steam
/// installed for its own games (on a computer with Steam).
fn proton_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("AZOTOTV_PROTON") {
        return Some(p.into());
    }
    let distro = PathBuf::from("/usr/share/steam/compatibilitytools.d/proton-cachyos-slr");
    if distro.join("proton").exists() {
        return Some(distro);
    }
    let steam = home().join(".local/share/Steam");
    let newest = |dir: PathBuf, prefix: &str| {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("proton").exists() && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(prefix)))
            .collect();
        v.sort_by_key(|p| version_key(&p.file_name().unwrap_or_default().to_string_lossy()));
        v.pop()
    };
    newest(steam.join("compatibilitytools.d"), "GE-Proton").or_else(|| newest(steam.join("steamapps/common"), "Proton"))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('.') && !id.contains('/') && !id.contains('\0')
}

pub(crate) fn read_manifest(id: &str) -> Result<Manifest, String> {
    if !valid_id(id) {
        return Err(t!("not a valid game: {id}", id = id));
    }
    let path = crate::library::game_dir(id).join("game.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_info(id: &str) -> Info {
    std::fs::read_to_string(crate::library::game_dir(id).join("info.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn art_path(id: &str, kind: &str) -> Option<PathBuf> {
    let dir = crate::library::game_dir(id).join("art");
    ["jpg", "png", "webp"].iter().map(|e| dir.join(format!("{kind}.{e}"))).find(|p| p.exists())
}

/// `game://localhost/<id>/<kind>`
pub fn art(path: &str) -> Result<(Vec<u8>, String), String> {
    let mut it = path.trim_start_matches('/').split('/');
    let (Some(id), Some(kind), None) = (it.next(), it.next(), it.next()) else { return Err("bad art path".into()) };
    let chapter = kind.strip_prefix("chapter-").is_some_and(|n| n.parse::<u16>().is_ok());
    let achievement = kind.strip_prefix("ach-").and_then(|n| n.parse::<usize>().ok());
    let mod_art = kind.strip_prefix("mod-").is_some_and(crate::mods::valid_id);
    if !valid_id(id) || !(ART.contains(&kind) || chapter || mod_art || achievement.is_some()) {
        return Err("bad art path".into());
    }
    let p = match achievement {
        Some(i) => crate::steamemu::icon(id, i),
        None => art_path(id, kind),
    }
    .ok_or("no art")?;
    let ct = match p.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    };
    std::fs::read(&p).map(|d| (d, ct.to_string())).map_err(|e| e.to_string())
}

fn played_path(user: &str) -> PathBuf {
    player_dir(user).join("games.json")
}

pub(crate) fn read_played(user: &str) -> HashMap<String, Played> {
    std::fs::read_to_string(played_path(user)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn record_played(user: &str, id: &str, seconds: u64, chapter: Option<usize>) {
    let mut all = read_played(user);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let e = all.entry(id.to_string()).or_default();
    e.last = now;
    e.seconds += seconds;
    if chapter.is_some() {
        e.chapter = chapter;
    }
    if let Ok(t) = serde_json::to_string_pretty(&all) {
        let _ = std::fs::write(played_path(user), t);
    }
}

pub fn list(user: &str) -> Vec<GameInfo> {
    let played = read_played(user);
    let mut v: Vec<GameInfo> = crate::library::ids()
        .into_iter()
        .filter_map(|id| {
            let m = read_manifest(&id).ok()?;
            if m.added.as_deref() == Some("rom") && !Path::new(&m.exe).exists() {
                return None; // its ROM is gone (another disk, deleted)
            }
            if m.kind == Kind::Steam && !m.steam_appid.is_some_and(crate::steam::is_installed) {
                return None; // uninstalled from Steam
            }
            let info = read_info(&id);
            // the pad as the first chapter has it (chapters may override the game's)
            let first = m.for_chapter(None).ok()?;
            let profile: Option<azotopad::Profile> = first.pad.and_then(|p| serde_json::from_value(p).ok());
            let (pad, cursor) = match profile {
                Some(azotopad::Profile::Keys { map, .. }) => ("keys", map.values().any(|t| t == "mouse")),
                _ => ("xbox", false),
            };
            let art_stamp = std::fs::metadata(crate::library::game_dir(&id).join("art"))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            Some(GameInfo {
                art_stamp,
                added: m.added.clone(),
                name: m.name,
                kind: m.kind,
                platform: m.platform,
                steam_appid: m.steam_appid,
                description: info.description,
                genres: info.genres,
                developers: info.developers,
                year: info.year,
                trailers: info.trailers.into_iter().map(|t| t.name).collect(),
                art: ART.iter().copied().filter(|k| art_path(&id, k).is_some()).collect(),
                pad: pad.into(),
                cursor,
                played: played.get(&id).copied(),
                chapters: m
                    .chapters
                    .iter()
                    .enumerate()
                    .map(|(i, c)| ChapterInfo {
                        name: c.name.clone(),
                        subtitle: c.subtitle.clone(),
                        description: c.description.clone(),
                        art: art_path(&id, &format!("chapter-{}", i + 1)).is_some(),
                    })
                    .collect(),
                worlds: if m.kind == Kind::Minecraft { crate::minecraft::worlds(&player_dir(user)) } else { Vec::new() },
                players: m.players.max(1),
                achievements: crate::steamemu::summary(m.kind, user, &id),
                mods: crate::mods::info(&m.mods, &crate::mods::enabled(user, &id, &m.mods), |k| art_path(&id, k).is_some()),
                id,
            })
        })
        .collect();
    v.sort_by_key(|g| g.name.to_lowercase());
    v
}

/// The Mobile games, each with how the phone can play it (none yet: listed, not playable).
pub fn phone_games() -> Vec<PhoneGame> {
    let mut v: Vec<PhoneGame> = crate::library::ids()
        .into_iter()
        .filter_map(|id| {
            let m = read_manifest(&id).ok()?;
            if m.platform.as_deref() != Some("mobile") {
                return None;
            }
            let phone = m.phone.unwrap_or_default();
            let art = ART.iter().copied().filter(|k| art_path(&id, k).is_some()).collect();
            let stream = !phone.args.is_empty() || !phone.env.is_empty();
            Some(PhoneGame { id, name: m.name, art, web: phone.web, stream })
        })
        .collect();
    v.sort_by_key(|g| g.name.to_lowercase());
    v
}

/// This profile's achievements in game `id` (empty: the game has none).
pub fn achievements(user: &str, id: &str) -> Result<Vec<crate::steamemu::Achievement>, String> {
    let m = read_manifest(id)?;
    Ok(crate::steamemu::achievements(m.kind, user, id))
}

/// Switch one of the game's mods on or off for this profile (applied at the next launch).
pub fn set_mod(user: &str, id: &str, mod_id: &str, on: bool) -> Result<Vec<crate::mods::ModInfo>, String> {
    let m = read_manifest(id)?;
    let now = crate::mods::set(user, id, &m.mods, mod_id, on)?;
    Ok(crate::mods::info(&m.mods, &now, |k| art_path(id, k).is_some()))
}

pub fn trailer_url(id: &str, index: usize) -> Result<(String, String), String> {
    read_manifest(id)?; // checks the id
    let t = read_info(id).trailers.into_iter().nth(index).ok_or_else(|| t!("trailer not found"))?;
    Ok((t.url, t.name))
}

/// Per-player settings (`settings.json` in the player's folder).
#[derive(Deserialize, Serialize, Clone)]
pub struct Settings {
    #[serde(default = "one")]
    pub cursor_sensitivity: f32,
}

fn one() -> f32 {
    1.0
}

pub fn settings(user: &str) -> Settings {
    std::fs::read_to_string(player_dir(user).join("settings.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Settings { cursor_sensitivity: 1.0 })
}

pub fn save_settings(user: &str, mut s: Settings) -> Result<Settings, String> {
    s.cursor_sensitivity = s.cursor_sensitivity.clamp(0.2, 3.0);
    let dir = player_dir(user);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let t = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("settings.json"), t).map_err(|e| e.to_string())?;
    Ok(s)
}


/// Games and videos run on the stronger GPU. On a laptop whose desktop is
/// drawn by the integrated one (so the menus don't wake the other up, see
/// session.sh) that takes an environment: NVIDIA's PRIME render offload
/// variables, which also make the NVIDIA Vulkan's only device (Eden's
/// "device 0"). `system.rs` knows which, if any.
pub fn gpu_env() -> impl Iterator<Item = (&'static String, &'static String)> {
    crate::system::system().gpu_env.iter().map(|(k, v)| (k, v))
}

/// Where the machine allows it (system.rs, azototv-no-turbo.service), the
/// CPU's turbo stays off for the menus, so a laptop under the TV stays cool
/// and quiet, and comes on only while a game runs.
pub fn set_turbo(on: bool) {
    // the kiosk only: a computer's CPU settings are its owner's
    if std::env::var_os("AZOTOTV_FULLSCREEN").is_none() {
        return;
    }
    let Some((file, yes, no)) = &crate::system::system().turbo else { return };
    if let Err(e) = std::fs::write(file, if on { yes } else { no }) {
        eprintln!("[games] turbo {}: {e}", if on { "on" } else { "off" });
    }
}

/// A laptop's own screen (system.rs), when there is one: the TV mirrors it
/// (kwinoutputconfig.json), which costs a second copy of every frame and makes
/// games pace themselves to the panel instead of the TV. It is switched off
/// while a game runs.
fn panel() -> Option<&'static str> {
    crate::system::system().panel.as_deref()
}

/// The panel on/off (kiosk only: a dev run leaves the PC's screens alone).
/// Disabling it puts the TV at 1080p 60 Hz if it was above (a 4K TV picked
/// on its own would make every game push 4K frames); enabling it brings the
/// mirror back exactly as it was.
pub fn set_panel(on: bool) {
    // streaming: the panel is the screen Sunshine sends
    if std::env::var_os("AZOTOTV_FULLSCREEN").is_none() || (!on && crate::stream::active()) {
        return;
    }
    let Some(panel) = panel() else { return };
    if on {
        // a two-screen game's split first, then the usual
        unsplit_screens();
    }
    let mut args = vec![format!("output.{panel}.{}", if on { "enable" } else { "disable" })];
    if !on {
        args.extend(tv_1080p());
    }
    kscreen(&args);
}

fn kscreen(args: &[String]) -> bool {
    match Command::new("kscreen-doctor").args(args).stdin(Stdio::null()).output() {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            eprintln!("[games] kscreen-doctor {args:?}: {}", String::from_utf8_lossy(&o.stderr).trim());
            false
        }
        Err(e) => {
            eprintln!("[games] kscreen-doctor {args:?}: {e}");
            false
        }
    }
}

fn kscreen_json() -> Option<serde_json::Value> {
    let o = Command::new("kscreen-doctor").arg("-j").stdin(Stdio::null()).output().ok()?;
    serde_json::from_slice(&o.stdout).ok()
}

/// The screens as they were before a two-screen game split them (kept on
/// disk: a crash in the middle of the game must not leave them split).
fn split_file() -> PathBuf {
    data_dir().join("screens-before-split.json")
}

/// The TV, when a two-screen game can have the panel as its second screen:
/// the kiosk (a dev run leaves the PC's screens alone), the TV on, and no PC
/// playing through Moonlight (which sends the panel).
fn split_tv() -> Option<String> {
    if std::env::var_os("AZOTOTV_FULLSCREEN").is_none() || crate::stream::active() || panel().is_none() {
        return None;
    }
    crate::stream::tv_output()
}

/// For a two-screen game (3DS, Wii U): the panel stops mirroring the TV and
/// becomes a screen of its own, right of it, both 1920x1080 at scale 1.
/// The game's second window goes there (kwin.rs), and the phone shows it
/// (phonescreen.rs streams the panel). Whether it worked.
fn split_screens(tv: &str) -> bool {
    let Some(panel) = panel() else { return false };
    let Some(j) = kscreen_json() else { return false };
    let outputs: Vec<serde_json::Value> = j["outputs"].as_array().cloned().unwrap_or_default();
    let name_of = |id: &serde_json::Value| outputs.iter().find(|o| &o["id"] == id).and_then(|o| o["name"].as_str()).unwrap_or("").to_string();
    let saved: Vec<serde_json::Value> = outputs
        .iter()
        .filter(|o| o["connected"] == true)
        .map(|o| {
            serde_json::json!({
                "name": o["name"], "enabled": o["enabled"], "pos": o["pos"], "scale": o["scale"],
                "mode": o["currentModeId"], "priority": o["priority"],
                "mirror": if o["replicationSource"].as_u64().unwrap_or(0) > 0 { name_of(&o["replicationSource"]) } else { String::new() },
            })
        })
        .collect();
    if let Err(e) = std::fs::write(split_file(), serde_json::to_string_pretty(&saved).unwrap_or_default()) {
        eprintln!("[games] {}: {e}", split_file().display());
        return false;
    }
    let size = |m: &serde_json::Value| (m["size"]["width"].as_u64().unwrap_or(0), m["size"]["height"].as_u64().unwrap_or(0));
    let hz = |m: &serde_json::Value| (m["refreshRate"].as_f64().unwrap_or(0.0) - 60.0).abs();
    let mode_1080 = |name: &str| {
        let o = outputs.iter().find(|o| o["name"] == name)?;
        let modes = o["modes"].as_array()?;
        modes.iter().filter(|m| size(m) == (1920, 1080)).min_by(|a, b| hz(a).total_cmp(&hz(b)))?["id"].as_str().map(str::to_string)
    };
    let (Some(tv_mode), Some(panel_mode)) = (mode_1080(tv), mode_1080(panel)) else {
        eprintln!("[games] split: no 1080p mode on {tv} or {panel}");
        let _ = std::fs::remove_file(split_file());
        return false;
    };
    let args: Vec<String> = [
        format!("output.{tv}.mirror.none"),
        format!("output.{panel}.enable"),
        format!("output.{tv}.mode.{tv_mode}"),
        format!("output.{tv}.scale.1"),
        format!("output.{tv}.position.0,0"),
        format!("output.{panel}.mode.{panel_mode}"),
        format!("output.{panel}.scale.1"),
        format!("output.{panel}.position.1920,0"),
        format!("output.{tv}.priority.1"),
        format!("output.{panel}.priority.2"),
    ]
    .into();
    eprintln!("[games] split screens: {tv} + {panel}");
    if kscreen(&args) {
        true
    } else {
        unsplit_screens();
        false
    }
}

/// The screens back as before [`split_screens`] (if they were split).
fn unsplit_screens() {
    let Ok(t) = std::fs::read_to_string(split_file()) else { return };
    let saved: Vec<serde_json::Value> = serde_json::from_str(&t).unwrap_or_default();
    let mut args = Vec::new();
    let mut mirrors = Vec::new();
    for o in &saved {
        let Some(name) = o["name"].as_str() else { continue };
        if o["enabled"] != true {
            args.push(format!("output.{name}.disable"));
            continue;
        }
        args.push(format!("output.{name}.enable"));
        if let Some(m) = o["mode"].as_str() {
            args.push(format!("output.{name}.mode.{m}"));
        }
        if let Some(s) = o["scale"].as_f64() {
            args.push(format!("output.{name}.scale.{s}"));
        }
        args.push(format!("output.{name}.position.{},{}", o["pos"]["x"].as_i64().unwrap_or(0), o["pos"]["y"].as_i64().unwrap_or(0)));
        match o["mirror"].as_str() {
            Some(src) if !src.is_empty() => mirrors.push(format!("output.{name}.mirror.{src}")),
            _ => {
                if let Some(p) = o["priority"].as_u64() {
                    args.push(format!("output.{name}.priority.{p}"));
                }
            }
        }
    }
    args.extend(mirrors);
    eprintln!("[games] screens back: {args:?}");
    kscreen(&args);
    let _ = std::fs::remove_file(split_file());
}

/// kscreen-doctor arguments that put every other screen bigger than 1080p
/// at 1920x1080, the mode closest to 60 Hz, scale 1.
fn tv_1080p() -> Vec<String> {
    let Ok(o) = Command::new("kscreen-doctor").arg("-j").stdin(Stdio::null()).output() else { return Vec::new() };
    let Ok(j) = serde_json::from_slice::<serde_json::Value>(&o.stdout) else { return Vec::new() };
    let size = |m: &serde_json::Value| (m["size"]["width"].as_u64().unwrap_or(0), m["size"]["height"].as_u64().unwrap_or(0));
    let mut args = Vec::new();
    for out in j["outputs"].as_array().into_iter().flatten() {
        let name = out["name"].as_str().unwrap_or("");
        if Some(name) == panel() || out["enabled"] != true {
            continue;
        }
        let modes = out["modes"].as_array().cloned().unwrap_or_default();
        let current = modes.iter().find(|m| m["id"] == out["currentModeId"]).map(size).unwrap_or((0, 0));
        if current.0 <= 1920 {
            continue;
        }
        let hz = |m: &serde_json::Value| (m["refreshRate"].as_f64().unwrap_or(0.0) - 60.0).abs();
        let best = modes.iter().filter(|m| size(m) == (1920, 1080)).min_by(|a, b| hz(a).total_cmp(&hz(b)));
        if let Some(id) = best.and_then(|m| m["id"].as_str()) {
            args.push(format!("output.{name}.mode.{id}"));
            args.push(format!("output.{name}.scale.1"));
        }
    }
    args
}

/// On the TV, games driven by the stick-mouse show the system pointer in their
/// menus: the default one is a small black arrow you can't find from the sofa.
/// (On a computer they keep the desktop's own.)
const TV_CURSOR: [(&str, &str); 2] = [("XCURSOR_THEME", "Breeze_Light"), ("XCURSOR_SIZE", "72")];

/// Every game's env. The shader cache is shared by every player (same Linux
/// user), so the driver must never trim it: the next player would compile everything again.
const GAME_ENV: [(&str, &str); 2] = [
    ("__GL_SHADER_DISK_CACHE_SKIP_CLEANUP", "1"),
    // Proton's xalia (pad control for launcher windows) runs next to every
    // game once a pad exists: no use here, and it costs CPU
    ("PROTON_USE_XALIA", "0"),
];

/// `umu-run` with the env every Wine call for this prefix needs.
fn umu(prefix: &Path, proton: &Path, game_id: &str) -> Command {
    let mut c = Command::new("umu-run");
    c.env("WINEPREFIX", prefix).env("PROTONPATH", proton).env("GAMEID", game_id);
    c
}

/// Proton's own wineserver for a prefix: `-w` waits until Wine has stopped
/// (and written its registry), `-k` stops it. (Through umu-run, "wineserver"
/// would be looked up as a Windows program.)
fn wineserver(prefix: &Path, proton: &Path, arg: &str) {
    let _ = Command::new(proton.join("files/bin/wineserver"))
        .arg(arg)
        .env("WINEPREFIX", prefix)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn run_quiet(mut c: Command) -> Result<(), String> {
    let out = c.stdin(Stdio::null()).output().map_err(|e| format!("umu-run: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(format!("umu-run: {}", out.status)) }
}

const WINEBUS: &str = r"HKLM\System\CurrentControlSet\Services\winebus";

/// The game's Wine prefix for this player, cloned from the base prefix on first
/// use. Keyboard/mouse games get Wine's controller support switched off: some
/// (FNaF) crash at startup when Wine shows them a pad, and they don't need one.
fn ensure_prefix(prefix: &Path, proton: &Path, no_pads: bool) -> Result<(), String> {
    let base = data_dir().join("wine-base");
    if !base.join("drive_c").exists() {
        eprintln!("[games] creating the base Wine prefix (first run, takes a minute)");
        std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
        let mut c = umu(&base, proton, "umu-default");
        c.args(["wineboot", "-u"]);
        run_quiet(c)?;
        wineserver(&base, proton, "-w");
    }
    if !prefix.exists() {
        std::fs::create_dir_all(prefix.parent().unwrap()).map_err(|e| e.to_string())?;
        let st = Command::new("cp")
            .args(["-a", "--reflink=auto"])
            .arg(&base)
            .arg(prefix)
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            return Err(t!("couldn't copy the Wine prefix"));
        }
    }
    let flag = prefix.join(".azototv-nopads");
    if no_pads != flag.exists() {
        for (name, on, off) in [("Enable SDL", "0", "1"), ("DisableHidraw", "1", "0"), ("DisableInput", "1", "0")] {
            let mut c = umu(prefix, proton, "umu-default");
            c.args(["reg", "add", WINEBUS, "/v", name, "/t", "REG_DWORD", "/d", if no_pads { on } else { off }, "/f"]);
            run_quiet(c)?;
        }
        wineserver(prefix, proton, "-w");
        if no_pads {
            let _ = std::fs::write(&flag, "");
        } else {
            let _ = std::fs::remove_file(&flag);
        }
    }
    Ok(())
}

/// Copies the game's `prefix-seed/` (`home-seed/`) into the player's prefix
/// (home), never over a file that is already there: e.g. a save with only
/// Geometry Dash's "terms accepted" flag, because its terms dialog takes no pad
/// or key, only a mouse.
fn seed_prefix(seed: &Path, prefix: &Path) -> Result<(), String> {
    if !seed.is_dir() {
        return Ok(());
    }
    let st = Command::new("cp")
        .args(["-a", "--update=none", "--reflink=auto"])
        .arg(seed.join("."))
        .arg(prefix)
        .status()
        .map_err(|e| e.to_string())?;
    if st.success() { Ok(()) } else { Err(t!("couldn't copy {a}", a = seed.display())) }
}

// ---- fullscreen key (X11 games, i.e. everything under Wine) ---------------------

/// Each `player_dirs` folder becomes a link to the player's own copy in
/// `players/<id>/dirs/<game id>/<path>`. The game's own folder is kept as
/// `<path>.orig` (the first time) and is what a new player's copy starts from.
fn link_player_dirs(dir: &Path, player: &Path, id: &str, paths: &[String]) -> Result<(), String> {
    for rel in paths {
        let r = Path::new(rel);
        if r.is_absolute() || r.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            return Err(t!("player_dirs: not a valid path: {rel}", rel = rel));
        }
        let path = dir.join(r);
        let orig = PathBuf::from(format!("{}.orig", path.display()));
        let is_link = path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink());
        if path.exists() && !is_link {
            if orig.exists() {
                return Err(format!("{} and {} both exist", path.display(), orig.display()));
            }
            std::fs::rename(&path, &orig).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        std::fs::create_dir_all(&orig).map_err(|e| e.to_string())?;
        let mine = player.join("dirs").join(id).join(r);
        if !mine.exists() {
            std::fs::create_dir_all(mine.parent().unwrap()).map_err(|e| e.to_string())?;
            let st = Command::new("cp")
                .args(["-a", "--reflink=auto"])
                .arg(&orig)
                .arg(&mine)
                .status()
                .map_err(|e| e.to_string())?;
            if !st.success() {
                return Err(t!("couldn't copy {rel}", rel = rel));
            }
        }
        if is_link {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        std::os::unix::fs::symlink(&mine, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn xprop(args: &[&str]) -> Option<String> {
    let out = Command::new("xprop").args(args).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The active X window and the pid that owns it.
fn active_window() -> Option<(String, u32)> {
    let root = xprop(&["-root", "_NET_ACTIVE_WINDOW"])?;
    let win = root.rsplit(' ').next()?.trim().to_string();
    if !win.starts_with("0x") || win == "0x0" {
        return None;
    }
    let pid = xprop(&["-id", &win, "_NET_WM_PID"])?.rsplit(' ').next()?.trim().parse().ok()?;
    Some((win, pid))
}

fn is_fullscreen(win: &str) -> bool {
    xprop(&["-id", win, "_NET_WM_STATE"]).is_some_and(|s| s.contains("_NET_WM_STATE_FULLSCREEN"))
}

/// Whether `pid` is `ancestor` or runs under it (Proton starts the game in a
/// session of its own, so the process group doesn't tell).
fn descends_from(mut pid: u32, ancestor: u32) -> bool {
    for _ in 0..64 {
        if pid == ancestor {
            return true;
        }
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { return false };
        // "pid (comm) state ppid ...": comm may contain spaces, so split after ')'
        let Some(ppid) = stat.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(1)?.parse().ok())
        else {
            return false;
        };
        if ppid <= 1 {
            return false;
        }
        pid = ppid;
    }
    false
}

/// Presses `key` until the game's window is fullscreen. Games ignore keys while
/// they load behind their first window (Undertale: ~5 s), so: wait for the
/// game's window to be active, then press and check, every 2 s, a few times.
/// Checking before each press means it never toggles fullscreen back off.
/// Eden opens its system applets only from its menu: once its window has
/// the focus, walk the menu with the keyboard, then close Eden when the
/// applet ends (it would fall back to its game list, not to AzotoTV).
/// The profile's Miis onto the console now; what the game changed back
/// into the profile's Mii inbox when it ends (the returned job). A failure
/// never stops the game: the console keeps the Miis it had.
fn mii_sync(user: &str, console: crate::mii::Console) -> Box<dyn FnOnce() + Send> {
    if let Err(e) = crate::mii::before_game(user, console) {
        eprintln!("[mii] {console:?} sync: {e}");
    }
    let user = user.to_string();
    Box::new(move || match crate::mii::after_game(&user, console) {
        Ok(true) => eprintln!("[mii] {console:?}: Miis changed in the game, in the inbox"),
        Ok(false) => {}
        Err(e) => eprintln!("[mii] {console:?} read back: {e}"),
    })
}

fn open_applet(eden_pid: u32, applet: crate::eden::Applet, tapper: azotopad::Tapper) {
    let alive = || unsafe { libc::kill(eden_pid as i32, 0) } == 0;
    let start = Instant::now();
    let mut focused: Option<Instant> = None;
    // the list of games must be in place, or the menu ignores the keys
    while focused.is_none_or(|t| t.elapsed() < Duration::from_millis(1500)) {
        std::thread::sleep(Duration::from_millis(300));
        if !alive() || start.elapsed() > Duration::from_secs(60) {
            eprintln!("[games] applet: Eden's window never showed up");
            return;
        }
        let ours = active_window().is_some_and(|(_, pid)| descends_from(pid, eden_pid));
        if !ours {
            focused = None;
        } else if focused.is_none() {
            focused = Some(Instant::now());
        }
    }
    for attempt in 1..=2 {
        for step in applet.menu_keys() {
            match step {
                crate::eden::Keys::Combo(k) => tapper.combo(k),
                crate::eden::Keys::Tap(k) => tapper.tap(k),
            }
            std::thread::sleep(Duration::from_millis(700));
        }
        let pressed = Instant::now();
        while pressed.elapsed() < Duration::from_secs(15) {
            std::thread::sleep(Duration::from_millis(500));
            if !alive() {
                return;
            }
            if crate::eden::applet_running(applet) {
                eprintln!("[games] applet {applet:?} open (attempt {attempt})");
                while alive() {
                    std::thread::sleep(Duration::from_millis(500));
                    if crate::eden::emulation_stopped() {
                        eprintln!("[games] applet closed: closing Eden");
                        unsafe { libc::kill(-(eden_pid as i32), libc::SIGTERM) };
                        return;
                    }
                }
                return;
            }
        }
        // a dialog or a slow start ate the keys: close any open menu, try again
        tapper.tap(vec![azotopad::KeyCode::KEY_ESC, azotopad::KeyCode::KEY_ESC]);
        std::thread::sleep(Duration::from_millis(500));
    }
    eprintln!("[games] applet {applet:?} did not open");
}

fn make_fullscreen(game_pid: u32, key: azotopad::KeyCode, tapper: azotopad::Tapper) {
    const GRACE: Duration = Duration::from_secs(3);
    const RETRY: Duration = Duration::from_secs(2);
    let start = Instant::now();
    let mut seen: Option<Instant> = None;
    let mut last_tap: Option<Instant> = None;
    let mut taps = 0;
    while start.elapsed() < Duration::from_secs(120) && taps < 10 {
        std::thread::sleep(Duration::from_millis(500));
        if unsafe { libc::kill(game_pid as i32, 0) } != 0 {
            return; // the game is gone
        }
        let Some((win, pid)) = active_window() else { continue };
        if !descends_from(pid, game_pid) {
            continue;
        }
        if is_fullscreen(&win) {
            eprintln!("[games] fullscreen after {taps} key press(es)");
            return;
        }
        let now = Instant::now();
        let since = *seen.get_or_insert(now);
        if now - since >= GRACE && last_tap.is_none_or(|t| now - t >= RETRY) {
            tapper.tap(vec![key]);
            last_tap = Some(now);
            taps += 1;
        }
    }
    eprintln!("[games] could not make the game fullscreen ({taps} key presses)");
}

/// SIGTERM the game's process group, SIGKILL whatever is left 3 s later.
fn terminate(pgid: &AtomicI32) {
    let g = pgid.load(Ordering::Relaxed);
    if g <= 0 {
        return;
    }
    unsafe { libc::kill(-g, libc::SIGTERM) };
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        unsafe { libc::kill(-g, libc::SIGKILL) };
    });
}

/// Longest the loading screen waits for the game's window.
const LOADING_MAX: Duration = Duration::from_secs(60);
/// How long the loading screen stays behind the game's first window: an
/// emulator's (Eden's) or a windowed start (Undertale) isn't fullscreen yet,
/// and a fullscreen one covers it anyway.
const LOADING_BEHIND: Duration = Duration::from_secs(10);

/// Keeps the UI up as the loading screen until the game's window covers it:
/// a new window opens above AzotoTV and takes the focus (tested in the kiosk
/// session with Wayland and X11 windows). A little later the UI hides
/// (nothing left rendering behind the game) and `game-shown` tells it.
/// `done`: the game ended first, or the UI is already hidden.
fn hide_when_shown(app: AppHandle, done: Arc<Mutex<bool>>) {
    std::thread::spawn(move || {
        let w = app.get_webview_window("main");
        let t = Instant::now();
        let mut covered: Option<Instant> = None;
        while t.elapsed() < LOADING_MAX && !*done.lock().unwrap() {
            match covered {
                None if !w.as_ref().is_some_and(|w| w.is_focused().unwrap_or(false)) => {
                    eprintln!("[games] window shown after {:.1}s", t.elapsed().as_secs_f32());
                    covered = Some(Instant::now());
                }
                Some(c) if c.elapsed() >= LOADING_BEHIND => break,
                _ => {}
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let mut done = done.lock().unwrap();
        if !*done {
            *done = true;
            set_ui_visible(&app, false);
            let _ = app.emit("game-shown", ());
        }
    });
}

fn set_ui_visible(app: &AppHandle, visible: bool) {
    let Some(w) = app.get_webview_window("main") else { return };
    if visible {
        let _ = w.show();
        if std::env::var_os("AZOTOTV_FULLSCREEN").is_some() {
            let _ = w.set_fullscreen(true);
        }
        let _ = w.set_focus();
    } else {
        let _ = w.hide();
    }
}

impl Games {
    pub fn new() -> Arc<Games> {
        // a crash in the middle of a game must not leave them so
        set_turbo(false);
        crate::kwin::stop();
        set_panel(true);
        // a phone screen left from before a restart
        crate::phonescreen::stop_now();
        Arc::new(Games { running: Mutex::new(None) })
    }

    pub fn running(&self) -> Option<String> {
        self.running.lock().unwrap().as_ref().map(|r| r.id.clone())
    }

    /// The running game, when a phone can be its touch screen (phonescreen.rs):
    /// a Switch game (Eden or its PC version) or a 3DS one (its bottom screen),
    /// not one the phone itself plays.
    pub fn screen_game(&self) -> Option<String> {
        let id = self.running.lock().unwrap().as_ref().filter(|r| !r.phone).map(|r| r.id.clone())?;
        let m = read_manifest(&id).ok()?;
        (matches!(m.kind, Kind::Switch | Kind::N3ds) || m.platform.as_deref() == Some("switch")).then_some(id)
    }

    /// The part of the screen stream the phone shows (x, y, w, h, 0-1), when
    /// not all of it: a 3DS game's bottom screen (azahar.rs `bottom_screen`).
    pub fn screen_crop(&self) -> Option<[f64; 4]> {
        let id = self.screen_game()?;
        (read_manifest(&id).ok()?.kind == Kind::N3ds).then(|| crate::azahar::bottom_screen(crate::phonescreen::second_screen()))
    }

    /// Close the game if it is one the phone started (its stream ended).
    /// Whether there was one.
    pub fn stop_phone(&self) -> bool {
        match self.running.lock().unwrap().as_ref() {
            Some(r) if r.phone => {
                eprintln!("[games] the phone left: closing {}", r.id);
                terminate(&r.pgid);
                true
            }
            _ => false,
        }
    }

    /// Close the running game as holding Home on a pad does (a phone's Home).
    /// Which one it was.
    pub fn quit(&self) -> Option<String> {
        let r = self.running.lock().unwrap();
        let r = r.as_ref()?;
        r.quit.store(true, Ordering::Relaxed);
        terminate(&r.pgid);
        Some(r.id.clone())
    }

    /// Close the running game (app exit).
    pub fn shutdown(&self) {
        if let Some(r) = self.running.lock().unwrap().as_ref() {
            terminate(&r.pgid);
        }
    }

    /// Prepares and starts the game; returns once it runs. `game-ended` is
    /// emitted when it exits. Blocking (prefix setup can take a while).
    /// `user` is the Jellyfin user id, `user_name` their name (for the Switch profile).
    pub fn launch(
        self: Arc<Self>,
        app: AppHandle,
        user: &str,
        user_name: &str,
        id: &str,
        chapter: Option<usize>,
        world: Option<String>,
        phone: bool,
    ) -> Result<(), String> {
        let m = read_manifest(id)?;
        if phone && m.phone.as_ref().is_none_or(|p| p.args.is_empty() && p.env.is_empty()) {
            return Err(t!("this game has no phone version"));
        }
        let chapter = if m.chapters.is_empty() { None } else { Some(chapter.unwrap_or(0)) };
        let m = m.for_chapter(chapter)?;
        let pgid = Arc::new(AtomicI32::new(0));
        {
            let mut r = self.running.lock().unwrap();
            if let Some(cur) = r.as_ref() {
                return Err(t!("{a} is already running", a = cur.id));
            }
            *r = Some(Running { id: id.to_string(), pgid: pgid.clone(), phone, quit: Default::default() });
        }
        // the other machine's saves, when they are newer (a game shared with a linked AzotoTV)
        crate::saves::before(&app, user, user_name, id, m.kind);
        let result = self.clone().start(app, user, user_name, id, chapter, world.as_deref(), &m, pgid, phone);
        if result.is_err() {
            *self.running.lock().unwrap() = None;
        }
        result
    }

    fn start(
        self: Arc<Self>,
        app: AppHandle,
        user: &str,
        user_name: &str,
        id: &str,
        chapter: Option<usize>,
        world: Option<&str>,
        m: &Manifest,
        pgid: Arc<AtomicI32>,
        phone: bool,
    ) -> Result<(), String> {
        let dir = crate::library::game_dir(id);
        let exe = match m.exe.strip_prefix("~/") {
            Some(rest) => home().join(rest),
            None if Path::new(&m.exe).is_absolute() => PathBuf::from(&m.exe),
            None => dir.join(&m.exe),
        };
        let applet = match (&m.applet, m.kind) {
            (Some(a), Kind::Switch) => Some(crate::eden::Applet::parse(a)?),
            (Some(_), _) => return Err(t!("applet: only for Switch games")),
            _ => None,
        };
        if m.kind == Kind::Steam && !m.steam_appid.is_some_and(crate::steam::is_installed) {
            return Err(t!("Steam doesn't have this game installed"));
        }
        if applet.is_none() && m.kind != Kind::Steam && !exe.exists() {
            return Err(t!("executable not found: {a}", a = exe.display()));
        }
        let cwd = exe.parent().unwrap_or(&dir).to_path_buf();
        let profile: azotopad::Profile = match &m.pad {
            Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("pad: {e}"))?,
            None => azotopad::Profile::Xbox,
        };
        let fullscreen_key = match &m.fullscreen_key {
            Some(k) => Some(azotopad::key(k).ok_or_else(|| format!("fullscreen_key: unknown key {k}"))?),
            None => None,
        };
        crate::bgm::stop(); // the Mii app's music, if a game starts from there
        let player = player_dir(user);
        std::fs::create_dir_all(&player).map_err(|e| e.to_string())?;
        link_player_dirs(&dir, &player, id, &m.player_dirs)?;
        if !m.mods.is_empty() {
            // this profile's mods in place (the last player's go away)
            crate::mods::apply(&dir, &m.mods, &m.mod_loader, &crate::mods::enabled(user, id, &m.mods))?;
        }

        // a two-screen game gets the panel as its second screen, when the TV is on
        let split_tv = if m.kind == Kind::N3ds { split_tv() } else { None };
        let mut cmd;
        let mut prefix_env: Option<(PathBuf, PathBuf, String)> = None;
        let mut after: Option<Box<dyn FnOnce() + Send>> = None;
        match m.kind {
            Kind::Native => {
                // the game's saves and settings land in the player's own home
                let h = player.join("home");
                for d in [".config", ".local/share"] {
                    std::fs::create_dir_all(h.join(d)).map_err(|e| e.to_string())?;
                }
                cmd = Command::new(&exe);
                cmd.env("HOME", &h)
                    .env("XDG_CONFIG_HOME", h.join(".config"))
                    .env("XDG_DATA_HOME", h.join(".local/share"))
                    // shader caches are shared, not per player
                    .env("XDG_CACHE_HOME", home().join(".cache"));
                seed_prefix(&dir.join("home-seed"), &h)?;
            }
            Kind::Proton => {
                let proton = proton_path().ok_or_else(|| t!("Proton not found"))?;
                let prefix = player.join("wine").join(id);
                let no_pads = matches!(profile, azotopad::Profile::Keys { .. });
                ensure_prefix(&prefix, &proton, no_pads)?;
                seed_prefix(&dir.join("prefix-seed"), &prefix)?;
                let game_id = m.steam_appid.map(|a| format!("umu-{a}")).unwrap_or_else(|| format!("umu-{id}"));
                cmd = umu(&prefix, &proton, &game_id);
                cmd.arg(&exe);
                prefix_env = Some((prefix, proton, game_id));
            }
            Kind::Switch => {
                cmd = match applet {
                    Some(_) => crate::eden::applet_command(&player, user_name)?,
                    None => crate::eden::command(&player, user_name, &exe)?,
                };
                after = Some(mii_sync(user, crate::mii::Console::Switch));
            }
            Kind::Wii => {
                cmd = crate::dolphin::command(&player, &exe, crate::dolphin::Layout::from_manifest(m.wiimote.as_deref()))?;
                after = Some(mii_sync(user, crate::mii::Console::Wii));
            }
            Kind::N3ds => {
                cmd = crate::azahar::command(&player, user_name, &exe, split_tv.is_some())?;
            }
            Kind::Steam => {
                cmd = crate::steam::command(m.steam_appid.unwrap_or_default());
            }
            Kind::Minecraft => {
                cmd = crate::minecraft::command(&player, user_name, &exe, world)?;
                let (p, i) = (player.clone(), exe.clone());
                after = Some(Box::new(move || crate::minecraft::save(&p, &i)));
            }
        }
        let steam_emu = crate::steamemu::settings_dir(id).is_some();
        if steam_emu {
            crate::steamemu::prepare(m.kind, user, user_name, id)?;
        }
        cmd.args(&m.args).current_dir(&cwd).stdin(Stdio::null());
        if let Some(a) = m.steam_appid.filter(|_| m.kind != Kind::Steam) {
            // tells Steamworks the game was started by Steam, so it doesn't try to restart through it
            cmd.env("SteamAppId", a.to_string()).env("SteamGameId", a.to_string());
        }
        cmd.envs(gpu_env());
        cmd.envs(GAME_ENV);
        if std::env::var_os("AZOTOTV_FULLSCREEN").is_some() {
            cmd.envs(TV_CURSOR);
        }
        cmd.envs(&m.env);
        if phone {
            // its touch "phone mode" (read from the streamed touchscreen)
            let p = m.phone.clone().unwrap_or_default();
            cmd.args(&p.args).envs(&p.env);
        }
        if m.gamescope {
            // gamescope needs the real output size, or it stretches its 720p default
            let (w, h) = app
                .get_webview_window("main")
                .and_then(|w| w.current_monitor().ok().flatten())
                .map(|mon| (mon.size().width, mon.size().height))
                .unwrap_or((1920, 1080));
            let mut g = Command::new("gamescope");
            g.args(["-f", "-W", &w.to_string(), "-H", &h.to_string(), "--"])
                .arg(cmd.get_program())
                .args(cmd.get_args())
                .current_dir(&cwd)
                .stdin(Stdio::null());
            for (k, v) in cmd.get_envs() {
                match v {
                    Some(v) => g.env(k, v),
                    None => g.env_remove(k),
                };
            }
            cmd = g;
        }
        let log = std::fs::File::create(player.join(format!("{id}.log"))).ok();
        if let Some(f) = log {
            if let Ok(f2) = f.try_clone() {
                cmd.stdout(f).stderr(f2);
            }
        }
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            })
        };

        // the music would play over the game
        let music = app.state::<Arc<crate::music::Music>>().inner().clone();
        tauri::async_runtime::block_on(music.shutdown());

        crate::pad::set_paused(true);
        let quit = self.running.lock().unwrap().as_ref().map(|r| r.quit.clone()).unwrap_or_default();
        let (pg, q) = (pgid.clone(), quit.clone());
        // a one-player game listens to player 1 only: every pad switched on drives it
        let players = m.players.max(1) as usize;
        // a Steam game's controllers are Steam's (Steam Input), not azotopad's
        let session = (m.kind != Kind::Steam).then(|| {
            azotopad::Session::start(&profile, settings(user).cursor_sensitivity.clamp(0.2, 3.0), players, move || {
                eprintln!("[games] Home held: closing the game");
                q.store(true, Ordering::Relaxed);
                terminate(&pg);
            })
        });
        let mut session = match session.transpose() {
            Ok(s) => s,
            Err(e) => {
                crate::pad::set_paused(false);
                return Err(format!("controller: {e}"));
            }
        };
        // let udev publish the virtual devices before the game looks for pads
        std::thread::sleep(Duration::from_millis(300));
        if let (Some(session), Kind::Switch | Kind::Wii | Kind::N3ds) = (session.as_mut(), m.kind) {
            // the Joy-Cons' motion (the emulator reads it over DSU)
            if let Err(e) = session.serve_motion() {
                eprintln!("[games] motion: {e}");
            }
        }
        if let (Some(session), Kind::Switch | Kind::Wii) = (session.as_ref(), m.kind) {
            // one player per controller switched on now (the Controller screen
            // came first): a connected Wii Remote nobody holds still shows
            // its pointer (Galaxy's player 2 star)
            let n = session.playing().clamp(1, players);
            let connected =
                if m.kind == Kind::Switch {
                    crate::eden::connect_players(n)
                } else {
                    crate::dolphin::connect_players(
                        n,
                        &session.motion_players(),
                        crate::dolphin::Layout::from_manifest(m.wiimote.as_deref()),
                    )
                };
            if let Err(e) = connected {
                eprintln!("[games] players: {e}");
            }
        }
        let ignore: Vec<String> = session.iter().flat_map(|s| s.pads()).map(|(v, p)| format!("0x{v:04x}/0x{p:04x}")).collect();
        if !ignore.is_empty() {
            cmd.env("SDL_GAMECONTROLLER_IGNORE_DEVICES", ignore.join(","));
        }

        // before the spawn: the game then finds the TV as its screen
        let split = match &split_tv {
            Some(tv) if split_screens(tv) => {
                let w = crate::kwin::Windows {
                    classes: &["Azahar", "org.azahar_emu.Azahar", "azototv"],
                    // Azahar's UI is in Italian (azahar.rs `Paths\language=it`)
                    second: &["| Finestra secondaria", "| Secondary Window"],
                };
                if let Err(e) = crate::kwin::place_screens(&w, tv, panel().unwrap_or_default()) {
                    eprintln!("[games] placing the screens: {e}");
                }
                crate::phonescreen::set_second_screen(true);
                true
            }
            _ => {
                set_panel(false);
                false
            }
        };
        crate::saves::snapshot(user, id, m.kind);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                if split {
                    crate::kwin::stop();
                    crate::phonescreen::set_second_screen(false);
                }
                set_panel(true);
                if let Some(s) = session.take() {
                    s.stop();
                }
                crate::pad::set_paused(false);
                return Err(format!("{}: {e}", exe.display()));
            }
        };
        pgid.store(child.id() as i32, Ordering::Relaxed);
        eprintln!("[games] started {id} (pid {})", child.id());
        set_turbo(true);
        if let (Some(key), Some(session)) = (fullscreen_key, &session) {
            let (tapper, pid) = (session.tapper(), child.id());
            std::thread::spawn(move || make_fullscreen(pid, key, tapper));
        }
        if let (Some(a), Some(session)) = (applet, &session) {
            let (tapper, pid) = (session.tapper(), child.id());
            std::thread::spawn(move || open_applet(pid, a, tapper));
        }
        let hidden = Arc::new(Mutex::new(false));
        hide_when_shown(app.clone(), hidden.clone());

        let id = id.to_string();
        let user = user.to_string();
        let kind = m.kind;
        let stop_watch = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if steam_emu {
            let (app, kind, user, id, stop) = (app.clone(), m.kind, user.clone(), id.clone(), stop_watch.clone());
            std::thread::spawn(move || crate::steamemu::watch(app, kind, user, id, stop));
        }
        let started = Instant::now();
        std::thread::Builder::new()
            .name("game".into())
            .spawn(move || {
                let status = child.wait();
                stop_watch.store(true, Ordering::Relaxed);
                // Wine leaves wineserver and helpers behind; stop them for this prefix
                if let Some((prefix, proton, _)) = prefix_env {
                    wineserver(&prefix, &proton, "-k");
                }
                unsafe { libc::kill(-pgid.load(Ordering::Relaxed), libc::SIGKILL) };
                if let Some(f) = after {
                    f();
                }
                set_turbo(false);
                // a phone that was its touch screen: nothing left to show
                crate::phonescreen::stop_now();
                if split {
                    crate::kwin::stop();
                    crate::phonescreen::set_second_screen(false);
                }
                set_panel(true);
                if let Some(s) = session.take() {
                    s.stop();
                }
                crate::pad::set_paused(false);
                *hidden.lock().unwrap() = true;
                set_ui_visible(&app, true);
                *self.running.lock().unwrap() = None;
                let seconds = started.elapsed().as_secs();
                record_played(&user, &id, seconds, chapter);
                crate::saves::after(&app, &user, &id, kind);
                let (reason, error) = match status {
                    _ if quit.load(Ordering::Relaxed) => ("quit", None),
                    Ok(_) => ("exit", None),
                    Err(e) => ("error", Some(e.to_string())),
                };
                eprintln!("[games] {id} ended ({reason}) after {seconds}s");
                let _ = app.emit("game-ended", Ended { id, seconds, reason: reason.into(), error });
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manifests with mods (a racing game: fan tracks with an update swap; a
    /// fighting game: characters + the loader) must parse, or the game
    /// would silently vanish from the library.
    #[test]
    fn manifests_with_mods_parse() {
        let racer: Manifest = serde_json::from_str(RACER).unwrap();
        assert_eq!(racer.mods.len(), 1);
        assert_eq!(racer.mods[0].files[2].when, crate::mods::When::Off);
        let fighter: Manifest = serde_json::from_str(FIGHTER).unwrap();
        assert_eq!(fighter.mods.len(), 4);
        assert_eq!(fighter.mod_loader.len(), 1);
        assert!(fighter.mods.iter().all(|m| crate::mods::valid_id(&m.id) && !m.always));
    }

    const RACER: &str = r#"{
  "name": "Kart Racer",
  "type": "switch",
  "exe": "~/Eden/Games/Kart Racer.nsp",
  "players": 4,
  "mods": [
    {
      "id": "fan-tracks",
      "name": "Fan Tracks",
      "subtitle": "Piste dei fan",
      "description": "Piste nuove fatte dai fan. Con la mod attiva il gioco parte alla versione 1.1, quella che serve alla mod.",
      "files": [
        {"stored": "mods/fan-tracks", "active": "~/.local/share/eden/load/0100000000001000/Fan Tracks"},
        {"stored": "updates/Kart Racer update v1.1.nsp", "active": "~/Eden/Extra/Kart Racer update v1.1.nsp"},
        {"stored": "updates/Kart Racer update v1.2.nsp", "active": "~/Eden/Extra/Kart Racer update v1.2.nsp", "when": "off"}
      ]
    }
  ]
}"#;
    const FIGHTER: &str = r#"{
  "name": "Arena Fighter",
  "type": "switch",
  "exe": "~/Eden/Games/Arena Fighter.nsp",
  "players": 4,
  "mod_loader": [
    {"stored": "mods/_loader-exefs", "active": "~/.local/share/eden/sdmc/atmosphere/contents/0100000000002000/exefs"}
  ],
  "mods": [
    {
      "id": "knight",
      "name": "Knight",
      "subtitle": "Nuovo personaggio",
      "description": "Un cavaliere con mosse tutte sue e 8 colori.",
      "files": [{"stored": "mods/Knight", "active": "~/.local/share/eden/sdmc/mods/Knight"}]
    },
    {
      "id": "robot",
      "name": "Robot",
      "subtitle": "Nuovo personaggio",
      "description": "Un robot con mosse tutte sue e 8 colori.",
      "files": [{"stored": "mods/Robot", "active": "~/.local/share/eden/sdmc/mods/Robot"}]
    },
    {
      "id": "wizard",
      "name": "Wizard",
      "subtitle": "Nuovo personaggio",
      "description": "Un mago con animazioni, voce e annunciatore suoi.",
      "files": [{"stored": "mods/Wizard", "active": "~/.local/share/eden/sdmc/mods/Wizard"}]
    },
    {
      "id": "pirate-costume",
      "name": "Pirate",
      "subtitle": "Costume",
      "description": "Il costume principale di un personaggio diventa un pirata. Gli altri colori restano com'erano.",
      "files": [{"stored": "mods/Pirate", "active": "~/.local/share/eden/sdmc/mods/Pirate"}]
    }
  ]
}"#;
}
