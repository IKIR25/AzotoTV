//! Game mods, switched on and off per profile on the game page (like a
//! series' episodes), applied when the game starts.
//!
//! In `game.json`:
//!
//! ```json
//! "mods": [{"id": "fan-tracks", "name": "Fan Tracks", "subtitle": "Piste nuove",
//!           "description": "...",
//!           "files": [
//!             {"stored": "mods/fan-tracks",
//!              "active": "~/.local/share/eden/load/<title id>/Fan Tracks"},
//!             {"stored": "updates/1.1.nsp", "active": "~/Eden/Extra/1.1.nsp"},
//!             {"stored": "updates/1.2.nsp", "active": "~/Eden/Extra/1.2.nsp", "when": "off"}]}],
//! "mod_loader": [{"stored": "mods/loader-exefs",
//!                 "active": "~/.local/share/eden/sdmc/atmosphere/contents/<title id>/exefs"}]
//! ```
//!
//! Every file or folder has one home in the game folder (`stored`) and one
//! place where the emulator finds it (`active`): it is *moved* between the
//! two, never copied or deleted, so a mod costs no extra space and a crash
//! halfway leaves nothing half-written. `when: "off"` is for what must go away
//! while the mod is on (a newer update the mod can't run with). `mod_loader`
//! is active while at least one mod is on (a game's mod loader).
//! `"always": true` mods are on for everyone and not listed on the page.
//! Picture: `art/mod-<id>.{jpg,png,webp}`.
//!
//! Each profile's choice is in `players/<id>/mods.json` (`{game: [mod ids]}`).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::games::{home, player_dir};

#[derive(Deserialize, Clone, Debug)]
pub struct ModSpec {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub files: Vec<Place>,
    /// on for every profile, not switchable
    #[serde(default)]
    pub always: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Place {
    /// inside the game folder (or absolute / `~/...`)
    pub stored: String,
    /// where the emulator looks (absolute or `~/...`)
    pub active: String,
    #[serde(default)]
    pub when: When,
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum When {
    /// in place while the mod is on
    #[default]
    On,
    /// in place while the mod is off
    Off,
}

/// What the game page gets for each mod.
#[derive(Serialize, Clone)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    /// `art/mod-<id>` exists (served as `game://localhost/<game>/mod-<id>`)
    pub art: bool,
    /// on for this profile
    pub on: bool,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn path(dir: &Path, p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if Path::new(p).is_absolute() => PathBuf::from(p),
        None => dir.join(p),
    }
}

fn state_path(user: &str) -> PathBuf {
    player_dir(user).join("mods.json")
}

fn read_state(user: &str) -> HashMap<String, Vec<String>> {
    std::fs::read_to_string(state_path(user)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// The mods on for this profile in game `game`: the ones it switched on (that
/// the game still has) and the `always` ones.
pub fn enabled(user: &str, game: &str, specs: &[ModSpec]) -> Vec<String> {
    let chosen = read_state(user).remove(game).unwrap_or_default();
    specs.iter().filter(|s| s.always || chosen.contains(&s.id)).map(|s| s.id.clone()).collect()
}

/// Switch mod `id` of `game` on or off for this profile; returns the ids now on.
pub fn set(user: &str, game: &str, specs: &[ModSpec], id: &str, on: bool) -> Result<Vec<String>, String> {
    match specs.iter().find(|s| s.id == id) {
        None => return Err(format!("unknown mod: {id}")),
        Some(s) if s.always => return Err(t!("{a} is always on", a = s.name)),
        _ => {}
    }
    let mut all = read_state(user);
    let list = all.entry(game.to_string()).or_default();
    list.retain(|x| x != id && specs.iter().any(|s| &s.id == x && !s.always));
    if on {
        list.push(id.to_string());
    }
    if list.is_empty() {
        all.remove(game);
    }
    let dir = player_dir(user);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let t = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    std::fs::write(state_path(user), t).map_err(|e| e.to_string())?;
    Ok(enabled(user, game, specs))
}

pub fn info(specs: &[ModSpec], on: &[String], art: impl Fn(&str) -> bool) -> Vec<ModInfo> {
    specs
        .iter()
        .filter(|s| valid_id(&s.id) && !s.always)
        .map(|s| ModInfo {
            id: s.id.clone(),
            name: s.name.clone(),
            subtitle: s.subtitle.clone(),
            description: s.description.clone(),
            art: art(&format!("mod-{}", s.id)),
            on: on.contains(&s.id),
        })
        .collect()
}

fn exists(p: &Path) -> bool {
    p.symlink_metadata().is_ok()
}

fn mv(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::rename(from, to).map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
}

/// Puts every file of the game's mods where it belongs for the mods in `on`:
/// first everything that must go away, then what must be in place. Checks
/// everything before moving anything; a file found in both places, or in
/// neither, is an error (nothing is ever overwritten).
pub fn apply(game_dir: &Path, specs: &[ModSpec], loader: &[Place], on: &[String]) -> Result<(), String> {
    let any = !on.is_empty();
    let mut wanted: Vec<(PathBuf, PathBuf, bool)> = Vec::new(); // (stored, active, should be active)
    for s in specs {
        let mod_on = on.contains(&s.id);
        for p in &s.files {
            wanted.push((path(game_dir, &p.stored), path(game_dir, &p.active), mod_on == (p.when == When::On)));
        }
    }
    for p in loader {
        wanted.push((path(game_dir, &p.stored), path(game_dir, &p.active), any == (p.when == When::On)));
    }
    let mut leave = Vec::new();
    let mut place = Vec::new();
    for (stored, active, want) in &wanted {
        match (exists(stored), exists(active), want) {
            (true, true, _) => return Err(t!("mod: {a} is in both {b} and {c}", a = file_name(stored), b = stored.display(), c = active.display())),
            (false, false, _) => return Err(format!("mod: missing file: {}", stored.display())),
            (true, false, true) => place.push((stored, active)),
            (false, true, false) => leave.push((active, stored)),
            _ => {}
        }
    }
    for (from, to) in leave.into_iter().chain(place) {
        mv(from, to)?;
        eprintln!("[mods] {} -> {}", from.display(), to.display());
    }
    Ok(())
}

/// Every (stored, active) pair of the game's mods and loader, as paths.
pub fn places(game_dir: &Path, specs: &[ModSpec], loader: &[Place]) -> Vec<(PathBuf, PathBuf)> {
    specs
        .iter()
        .flat_map(|s| s.files.iter())
        .chain(loader)
        .map(|p| (path(game_dir, p.stored.trim_end_matches('/')), path(game_dir, &p.active)))
        .collect()
}

/// Where `p` (a path in the game folder) is now: in the emulator's place when
/// it belongs to a mod that's switched on.
pub fn whereabouts(p: &Path, places: &[(PathBuf, PathBuf)]) -> PathBuf {
    for (stored, active) in places {
        if let Ok(rest) = p.strip_prefix(stored) {
            if !exists(stored) && exists(active) {
                return if rest.as_os_str().is_empty() { active.clone() } else { active.join(rest) };
            }
        }
    }
    p.to_path_buf()
}

/// Every mod file back in the game folder (before the folder changes: an
/// update from another AzotoTV); the next start puts them in place again.
pub fn park(game_dir: &Path, specs: &[ModSpec], loader: &[Place]) -> Result<(), String> {
    for (stored, active) in places(game_dir, specs, loader) {
        if !exists(&stored) && exists(&active) {
            mv(&active, &stored)?;
            eprintln!("[mods] {} -> {} (parked)", active.display(), stored.display());
        }
    }
    Ok(())
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("azototv-mods-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn spec(dir: &Path) -> (Vec<ModSpec>, Vec<Place>) {
        let p = |s: &str, a: &str, w: When| Place { stored: s.into(), active: dir.join(a).to_string_lossy().into(), when: w };
        let tracks = ModSpec {
            id: "tracks".into(),
            name: "Tracks".into(),
            subtitle: None,
            description: None,
            always: false,
            files: vec![
                p("mods/tracks", "load/Tracks", When::On),
                p("updates/303.nsp", "extra/303.nsp", When::On),
                p("updates/401.nsp", "extra/401.nsp", When::Off),
            ],
        };
        (vec![tracks], vec![p("mods/exefs", "sd/exefs", When::On)])
    }

    #[test]
    fn swaps_files_both_ways() {
        let d = tmp("swap");
        for f in ["mods/tracks/romfs", "mods/exefs"] {
            std::fs::create_dir_all(d.join(f)).unwrap();
        }
        std::fs::create_dir_all(d.join("updates")).unwrap();
        std::fs::create_dir_all(d.join("extra")).unwrap();
        std::fs::write(d.join("updates/303.nsp"), "3").unwrap();
        std::fs::write(d.join("extra/401.nsp"), "4").unwrap();
        let (specs, loader) = spec(&d);

        // off and already in the off state: nothing moves
        apply(&d, &specs, &loader, &[]).unwrap();
        assert!(d.join("extra/401.nsp").exists() && d.join("updates/303.nsp").exists());

        apply(&d, &specs, &loader, &["tracks".into()]).unwrap();
        assert!(d.join("load/Tracks/romfs").is_dir());
        assert!(d.join("extra/303.nsp").exists() && !d.join("extra/401.nsp").exists());
        assert!(d.join("updates/401.nsp").exists() && d.join("sd/exefs").is_dir());

        // applying the same state again is a no-op
        apply(&d, &specs, &loader, &["tracks".into()]).unwrap();

        apply(&d, &specs, &loader, &[]).unwrap();
        assert!(d.join("mods/tracks/romfs").is_dir() && !d.join("load/Tracks").exists());
        assert!(d.join("extra/401.nsp").exists() && d.join("updates/303.nsp").exists());
        assert!(d.join("mods/exefs").is_dir() && !d.join("sd/exefs").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn never_overwrites_and_checks_first() {
        let d = tmp("conflict");
        std::fs::create_dir_all(d.join("mods/tracks")).unwrap();
        std::fs::create_dir_all(d.join("mods/exefs")).unwrap();
        std::fs::create_dir_all(d.join("updates")).unwrap();
        std::fs::create_dir_all(d.join("extra")).unwrap();
        std::fs::write(d.join("updates/303.nsp"), "3").unwrap();
        std::fs::write(d.join("extra/303.nsp"), "other").unwrap(); // in both places
        std::fs::write(d.join("extra/401.nsp"), "4").unwrap();
        let (specs, loader) = spec(&d);
        assert!(apply(&d, &specs, &loader, &["tracks".into()]).is_err());
        // nothing moved
        assert!(d.join("mods/tracks").is_dir() && d.join("extra/401.nsp").exists() && !d.join("sd/exefs").exists());
        assert_eq!(std::fs::read_to_string(d.join("extra/303.nsp")).unwrap(), "other");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ids() {
        assert!(valid_id("tracks") && valid_id("big-robot_2"));
        assert!(!valid_id("") && !valid_id("../x") && !valid_id("a b"));
    }
}
