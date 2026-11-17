//! Games that need Steam run with a Steam API emulator (gbe_fork) in place of
//! their `steam_api` library: they start without Steam, in Italian, and keep
//! achievements. The library is swapped once, when the game is added
//! (`session/steam-emu/install.sh`), which also links the emulator's
//! `steam_settings` folder (achievement list, icons, app id) as
//! `<library>/<id>/steam_settings`: that link is how AzotoTV knows.
//!
//! The emulator keeps saves, stats and achievements in its "GSE Saves" folder:
//! under `$XDG_DATA_HOME` for native games, in the Wine prefix's AppData for
//! Windows ones. Both are already the player's own (games.rs), so every
//! profile has its own. Here: the player's name and a fixed Steam id for the
//! emulator, the achievement list for the game page, and a toast while
//! playing when one unlocks.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;

use crate::games::{Kind, player_dir};

/// The emulator's settings folder for game `id`, if the game uses it.
pub fn settings_dir(id: &str) -> Option<PathBuf> {
    let d = crate::library::game_dir(id).join("steam_settings");
    d.is_dir().then_some(d)
}

/// The player's "GSE Saves" folder for this game (where the emulator looks, see
/// games.rs for the HOME / prefix each kind of game gets).
fn saves_root(kind: Kind, user: &str, id: &str) -> Option<PathBuf> {
    let p = player_dir(user);
    match kind {
        Kind::Native => Some(p.join("home/.local/share/GSE Saves")),
        Kind::Proton => Some(p.join("wine").join(id).join("drive_c/users/steamuser/AppData/Roaming/GSE Saves")),
        _ => None,
    }
}

/// The same Steam id for a profile in every game (a 32-bit account id from the
/// Jellyfin user id; games may tie saves to it).
fn steam_id(user: &str) -> u64 {
    let mut h: u32 = 0x811c9dc5;
    for b in user.bytes() {
        h = (h ^ b as u32).wrapping_mul(0x01000193);
    }
    76561197960265728 + ((h & 0x7fff_ffff) | 1) as u64
}

/// Before a launch: the emulator's user settings in the player's saves folder.
/// (The game's own `steam_settings/configs.user.ini` sets only the language,
/// so these apply.)
pub fn prepare(kind: Kind, user: &str, user_name: &str, id: &str) -> Result<(), String> {
    let Some(root) = saves_root(kind, user, id) else { return Ok(()) };
    let dir = root.join("settings");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name: String = user_name.chars().filter(|c| !c.is_control()).collect();
    let ini = format!(
        "[user::general]\naccount_name={}\naccount_steamid={}\nlanguage={}\n",
        if name.trim().is_empty() { t!("Player") } else { name.trim().to_string() },
        steam_id(user),
        crate::locale::steam_language()
    );
    std::fs::write(dir.join("configs.user.ini"), ini).map_err(|e| e.to_string())
}

/// One entry of the emulator's `achievements.json`.
#[derive(Deserialize)]
struct Def {
    name: String,
    #[serde(rename = "displayName", default)]
    display_name: Value,
    #[serde(default)]
    description: Value,
    #[serde(default)]
    hidden: Value,
    #[serde(default)]
    icon: String,
}

/// A text that is either a plain string or one per language.
fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Object(m) => ["italian", "english"]
            .iter()
            .find_map(|k| m.get(*k))
            .or_else(|| m.values().next())
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn defs(id: &str) -> Vec<Def> {
    let Some(d) = settings_dir(id) else { return Vec::new() };
    std::fs::read_to_string(d.join("achievements.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn app_id(id: &str) -> Option<String> {
    let t = std::fs::read_to_string(settings_dir(id)?.join("steam_appid.txt")).ok()?;
    let t = t.trim();
    (!t.is_empty() && t.bytes().all(|b| b.is_ascii_digit())).then(|| t.to_string())
}

#[derive(Deserialize)]
struct Earned {
    #[serde(default)]
    earned: bool,
    #[serde(default)]
    earned_time: u64,
}

/// What the player has earned in game `id`: API name -> unix time. None: no
/// file yet, or caught halfway through a write.
fn read_earned(kind: Kind, user: &str, id: &str) -> Option<HashMap<String, u64>> {
    let path = saves_root(kind, user, id)?.join(app_id(id)?).join("achievements.json");
    let all: HashMap<String, Earned> = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    Some(all.into_iter().filter(|(_, e)| e.earned).map(|(k, e)| (k, e.earned_time)).collect())
}

fn earned(kind: Kind, user: &str, id: &str) -> HashMap<String, u64> {
    read_earned(kind, user, id).unwrap_or_default()
}

/// An achievement as the game page shows it.
#[derive(Serialize)]
pub struct Achievement {
    pub name: String,
    pub description: String,
    /// a secret one: name and description stay hidden until earned
    pub hidden: bool,
    /// when it was earned (unix seconds)
    pub earned: Option<u64>,
    /// its picture: `game://localhost/<id>/ach-<icon>`
    pub icon: usize,
}

/// Game `id`'s achievements for this player: earned first (newest first), then
/// the rest in the game's order.
pub fn achievements(kind: Kind, user: &str, id: &str) -> Vec<Achievement> {
    let got = earned(kind, user, id);
    let mut v: Vec<Achievement> = defs(id)
        .into_iter()
        .enumerate()
        .map(|(i, d)| Achievement {
            earned: got.get(&d.name).copied(),
            name: text(&d.display_name),
            description: text(&d.description),
            hidden: matches!(&d.hidden, Value::String(s) if s == "1") || d.hidden.as_i64() == Some(1),
            icon: i,
        })
        .collect();
    v.sort_by_key(|a| (a.earned.is_none(), std::cmp::Reverse(a.earned.unwrap_or(0))));
    v
}

/// (earned, total), for games with achievements.
pub fn summary(kind: Kind, user: &str, id: &str) -> Option<(usize, usize)> {
    let total = defs(id).len();
    (total > 0).then(|| (earned(kind, user, id).len(), total))
}

/// The picture of achievement `index` (`game://localhost/<id>/ach-<index>`).
pub fn icon(id: &str, index: usize) -> Option<PathBuf> {
    let d = settings_dir(id)?;
    let rel = defs(id).into_iter().nth(index)?.icon;
    let p = Path::new(&rel);
    if rel.is_empty() || p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return None;
    }
    Some(d.join(p)).filter(|p| p.is_file())
}

/// While the game runs: a toast for every achievement it unlocks. Stops when
/// `stop` is set.
pub fn watch(app: AppHandle, kind: Kind, user: String, id: String, stop: Arc<AtomicBool>) {
    let defs = defs(&id);
    if defs.is_empty() {
        return;
    }
    let mut known = earned(kind, &user, &id);
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_secs(2));
        let Some(now) = read_earned(kind, &user, &id) else { continue };
        for (i, d) in defs.iter().enumerate() {
            if now.contains_key(&d.name) && !known.contains_key(&d.name) {
                let toast = crate::overlay::Toast {
                    kicker: "Obiettivo sbloccato".into(),
                    title: text(&d.display_name),
                    body: text(&d.description),
                    image: icon(&id, i).and_then(|p| std::fs::read(p).ok()),
                    logo: false,
                };
                crate::overlay::show(&app, toast);
            }
        }
        known = now;
    }
}
