//! Minecraft through Prism Launcher. A game.json with `"type": "minecraft"` points
//! `exe` at a Prism instance folder (`~/.local/share/PrismLauncher/instances/<name>`).
//!
//! Each AzotoTV player plays as their own Prism account: an offline one named like
//! the profile, created on first launch, unless `players/<id>/minecraft.json`
//! links another one (`{"account": "Steve"}` for a Microsoft account). Worlds
//! and screenshots live in the player's folder (the instance's `saves` and
//! `screenshots` are links to it). The settings in [`PLAYER_FILES`] (video, shaders,
//! Voxy, controller...) are copied in before the game and back after it, so
//! everyone keeps their own; a player's first game starts from the shared defaults
//! in `<instance>/azototv-defaults/`, laid out like the game folder.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize)]
struct Link {
    account: String,
}

/// The instance's game folder (`minecraft`, or `.minecraft` in older instances).
fn game_dir(instance: &Path) -> PathBuf {
    let dot = instance.join(".minecraft");
    if dot.is_dir() && !instance.join("minecraft").is_dir() { dot } else { instance.join("minecraft") }
}

fn player_dir(player: &Path) -> PathBuf {
    player.join("minecraft")
}

/// What each player keeps their own copy of, relative to the game folder (and
/// to the player's folder). Iris also keeps each shader pack's settings in
/// `shaderpacks/<pack>.txt`: those follow the player too, see [`copy_shader_settings`].
const PLAYER_FILES: [&str; 7] = [
    "options.txt",
    "config/controlify",
    "config/iris.properties",
    "config/sodium-options.json",
    "config/sodium-extra-options.json",
    "config/voxy-config.json",
    "config/dynamic_fps.json",
];

fn defaults_dir(instance: &Path) -> PathBuf {
    instance.join("azototv-defaults")
}

/// Minecraft player names: 3-16 letters, digits or `_`.
fn offline_name(user_name: &str) -> String {
    let mut n: String = user_name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(16).collect();
    while n.len() < 3 {
        n.push('_');
    }
    n
}

/// The UUID Minecraft gives an offline player (Java's `UUID.nameUUIDFromBytes`).
fn offline_uuid(name: &str) -> String {
    let mut h: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    h[6] = (h[6] & 0x0f) | 0x30;
    h[8] = (h[8] & 0x3f) | 0x80;
    hex(&h)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn random_hex() -> Result<String, String> {
    let mut b = [0u8; 16];
    use std::io::Read;
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).map_err(|e| e.to_string())?;
    Ok(hex(&b))
}

/// The Prism account (its profile name, what `-a` takes) this player plays as,
/// added to accounts.json as an offline account if needed.
fn account(player: &Path, user_name: &str, file: &Path) -> Result<String, String> {
    let raw = std::fs::read_to_string(file).unwrap_or_else(|_| r#"{"accounts": [], "formatVersion": 3}"#.into());
    let mut accounts: Value = serde_json::from_str(&raw).map_err(|e| format!("accounts.json: {e}"))?;
    if !accounts["accounts"].is_array() {
        accounts["accounts"] = json!([]);
    }
    let names: Vec<String> = accounts["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["profile"]["name"].as_str().map(String::from))
        .collect();
    let link = player.join("minecraft.json");
    if let Some(l) = std::fs::read_to_string(&link).ok().and_then(|s| serde_json::from_str::<Link>(&s).ok()) {
        if names.contains(&l.account) {
            return Ok(l.account);
        }
    }
    let name = offline_name(user_name);
    let account = match names.iter().find(|n| n.eq_ignore_ascii_case(&name)) {
        Some(n) => n.clone(),
        None => {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            accounts["accounts"].as_array_mut().unwrap().push(json!({
                "entitlement": {"canPlayMinecraft": true, "ownsMinecraft": true},
                "profile": {
                    "capes": [],
                    "id": offline_uuid(&name),
                    "name": name,
                    "skin": {"id": "", "url": "", "variant": ""},
                },
                "type": "Offline",
                "ygg": {"extra": {"clientToken": random_hex()?, "userName": name}, "iat": now, "token": "0"},
            }));
            let tmp = file.with_extension("json.azototv");
            std::fs::write(&tmp, serde_json::to_string_pretty(&accounts).unwrap()).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, file).map_err(|e| e.to_string())?;
            eprintln!("[minecraft] created the offline account {name}");
            name
        }
    };
    std::fs::write(&link, serde_json::to_string(&Link { account: account.clone() }).unwrap()).map_err(|e| e.to_string())?;
    Ok(account)
}

/// Points `<game>/<name>` at the player's own folder. A real folder found there
/// (worlds made from the desktop) is kept aside as `<name>.prism`.
fn link_dir(game: &Path, name: &str, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
    let p = game.join(name);
    match std::fs::symlink_metadata(&p) {
        Ok(m) if m.is_dir() => {
            if std::fs::remove_dir(&p).is_err() {
                let mut aside = game.join(format!("{name}.prism"));
                let mut i = 1;
                while aside.exists() {
                    aside = game.join(format!("{name}.prism{i}"));
                    i += 1;
                }
                std::fs::rename(&p, &aside).map_err(|e| e.to_string())?;
            }
        }
        Ok(_) => std::fs::remove_file(&p).map_err(|e| e.to_string())?,
        Err(_) => {}
    }
    std::os::unix::fs::symlink(target, &p).map_err(|e| e.to_string())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_dir_all(to);
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to.join(e.file_name()))?;
        } else {
            std::fs::copy(e.path(), to.join(e.file_name()))?;
        }
    }
    Ok(())
}

fn copy_any(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        return copy_dir(from, to);
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::copy(from, to).map(|_| ())
}

/// Replaces the `shaderpacks/*.txt` of `to` with those of `from`.
fn copy_shader_settings(from: &Path, to: &Path) -> std::io::Result<()> {
    let txt = |d: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(d.join("shaderpacks"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "txt") && p.is_file())
            .collect()
    };
    for p in txt(to) {
        std::fs::remove_file(p)?;
    }
    for p in txt(from) {
        copy_any(&p, &to.join("shaderpacks").join(p.file_name().unwrap()))?;
    }
    Ok(())
}

/// A shader pack's name up to its version (`ComplementaryReimagined_r`, `MakeUp-UltraFast-`).
fn pack_stem(name: &str) -> String {
    name.chars().take_while(|c| !c.is_ascii_digit()).collect::<String>().to_lowercase()
}

/// Mod updates replace a shader pack with a file named after the new version,
/// and Iris then silently plays without shaders: point it at the new file.
fn fix_shader_pack(game: &Path) {
    let props = game.join("config/iris.properties");
    let Ok(text) = std::fs::read_to_string(&props) else { return };
    let Some(pack) = text.lines().find_map(|l| l.strip_prefix("shaderPack=")) else { return };
    let packs = game.join("shaderpacks");
    if pack.is_empty() || packs.join(pack).exists() {
        return;
    }
    let stem = pack_stem(pack);
    let found = std::fs::read_dir(&packs)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .find(|n| n.ends_with(".zip") && pack_stem(n) == stem);
    if let Some(new) = found {
        eprintln!("[minecraft] shader pack {pack} -> {new}");
        let text = text.replace(&format!("shaderPack={pack}"), &format!("shaderPack={new}"));
        if let Err(e) = std::fs::write(&props, text) {
            eprintln!("[minecraft] iris.properties: {e}");
        }
    }
}

/// Sets `key:value` lines of options.txt.
fn options_set(text: &str, sets: &[(&str, &str)]) -> String {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    for (k, v) in sets {
        let prefix = format!("{k}:");
        match lines.iter().position(|l| l.starts_with(&prefix)) {
            Some(i) => lines[i] = format!("{prefix}{v}"),
            None => lines.push(format!("{prefix}{v}")),
        }
    }
    lines.join("\n") + "\n"
}

/// Sets `key=value` in the `[General]` group of a Prism instance.cfg.
fn cfg_set(text: &str, sets: &[(&str, &str)]) -> String {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let start = match lines.iter().position(|l| l.trim() == "[General]") {
        Some(i) => i,
        None => {
            lines.insert(0, "[General]".into());
            0
        }
    };
    for (k, v) in sets {
        let end = lines[start + 1..].iter().position(|l| l.starts_with('[')).map_or(lines.len(), |i| start + 1 + i);
        let prefix = format!("{k}=");
        match lines[start + 1..end].iter().position(|l| l.starts_with(&prefix)) {
            Some(i) => lines[start + 1 + i] = format!("{prefix}{v}"),
            None => lines.insert(end, format!("{prefix}{v}")),
        }
    }
    lines.join("\n") + "\n"
}

/// The nightly world pre-generation (session/mc-pregen) may have one of the
/// worlds open: it saves and quits first (a no-op when it isn't running).
fn stop_pregen() {
    let _ = Command::new("systemctl").args(["--user", "stop", "azototv-mc-pregen.service"]).status();
}

/// Prism forwards `-l` to a launcher that is already open and exits at once, so
/// AzotoTV would think the game ended.
fn prism_running() -> bool {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "prismlauncher"))
}

// ---- worlds ---------------------------------------------------------------------

/// One of the player's worlds, as the Giochi page shows it.
#[derive(Serialize, Clone, Debug)]
pub struct World {
    /// the folder in `saves` (what Prism's `--world` takes)
    pub id: String,
    pub name: String,
    /// "survival", "creative", "adventure" or "spectator"
    pub mode: &'static str,
    pub hardcore: bool,
    /// last played, unix seconds
    pub last: u64,
    /// the world's own 64x64 picture (Minecraft takes it on the first visit), as a data: URL
    pub icon: Option<String>,
}

/// The few fields of a level.dat the world list needs.
#[derive(Default, Debug, PartialEq)]
struct Level {
    name: Option<String>,
    last_played: Option<i64>,
    game_type: Option<i32>,
    hardcore: bool,
}

/// A reader over uncompressed NBT (big endian, named tags).
struct Nbt<'a> {
    b: &'a [u8],
    i: usize,
}

impl Nbt<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.i..self.i.checked_add(n)?)?;
        self.i += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }
    fn string(&mut self) -> Option<String> {
        let n = u16::from_be_bytes(self.take(2)?.try_into().ok()?) as usize;
        Some(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn skip_n(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }
    /// An array's element count.
    fn len(&mut self) -> Option<usize> {
        self.i32().map(|n| n.max(0) as usize)
    }
    /// Skips the payload of a tag of type `t`.
    fn skip(&mut self, t: u8) -> Option<()> {
        match t {
            1 => self.skip_n(1),
            2 => self.skip_n(2),
            3 | 5 => self.skip_n(4),
            4 | 6 => self.skip_n(8),
            7 => {
                let n = self.len()?;
                self.skip_n(n)
            }
            8 => self.string().map(|_| ()),
            9 => {
                let et = self.u8()?;
                (0..self.len()?).try_for_each(|_| self.skip(et))
            }
            10 => self.compound(|r, t, _| r.skip(t)),
            11 => {
                let n = self.len()?;
                self.skip_n(n.checked_mul(4)?)
            }
            12 => {
                let n = self.len()?;
                self.skip_n(n.checked_mul(8)?)
            }
            _ => None,
        }
    }
    /// Walks a compound's tags; `f` must read or skip each payload.
    fn compound(&mut self, mut f: impl FnMut(&mut Self, u8, &str) -> Option<()>) -> Option<()> {
        loop {
            let t = self.u8()?;
            if t == 0 {
                return Some(());
            }
            let name = self.string()?;
            f(self, t, &name)?;
        }
    }
}

fn parse_level(b: &[u8]) -> Option<Level> {
    let mut r = Nbt { b, i: 0 };
    if r.u8()? != 10 {
        return None;
    }
    r.string()?;
    let mut l = Level::default();
    r.compound(|r, t, name| match (t, name) {
        (10, "Data") => r.compound(|r, t, name| {
            match (t, name) {
                (8, "LevelName") => l.name = Some(r.string()?),
                (4, "LastPlayed") => l.last_played = Some(r.i64()?),
                (3, "GameType") => l.game_type = Some(r.i32()?),
                (1, "hardcore") => l.hardcore = r.u8()? != 0,
                // newer versions keep it with the difficulty
                (10, "difficulty_settings") => r.compound(|r, t, name| match (t, name) {
                    (1, "hardcore") => {
                        l.hardcore = r.u8()? != 0;
                        Some(())
                    }
                    _ => r.skip(t),
                })?,
                _ => r.skip(t)?,
            }
            Some(())
        }),
        _ => r.skip(t),
    })?;
    Some(l)
}

/// The player's worlds, most recently played first.
pub fn worlds(player: &Path) -> Vec<World> {
    use base64::Engine;
    use std::io::Read;
    let mut v: Vec<World> = std::fs::read_dir(player_dir(player).join("saves"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let raw = std::fs::read(dir.join("level.dat")).ok()?;
            let mut b = Vec::new();
            flate2::read::GzDecoder::new(&raw[..]).read_to_end(&mut b).ok()?;
            let l = parse_level(&b).unwrap_or_default();
            let id = e.file_name().to_string_lossy().into_owned();
            let mtime = std::fs::metadata(dir.join("level.dat"))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            Some(World {
                name: l.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| id.clone()),
                mode: match l.game_type {
                    Some(1) => "creative",
                    Some(2) => "adventure",
                    Some(3) => "spectator",
                    _ => "survival",
                },
                hardcore: l.hardcore,
                last: l.last_played.map_or(mtime, |ms| (ms / 1000).max(0) as u64),
                icon: std::fs::read(dir.join("icon.png"))
                    .ok()
                    .map(|png| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))),
                id,
            })
        })
        .collect();
    v.sort_by(|a, b| b.last.cmp(&a.last).then_with(|| a.name.cmp(&b.name)));
    v
}

/// `world`: one of the player's worlds (its folder) to go straight into.
pub fn command(player: &Path, user_name: &str, instance: &Path, world: Option<&str>) -> Result<Command, String> {
    if prism_running() {
        return Err("Prism Launcher è già aperto: chiudilo e riprova".into());
    }
    stop_pregen();
    if let Some(w) = world {
        if w.is_empty() || w.starts_with('.') || w.contains('/') || !player_dir(player).join("saves").join(w).join("level.dat").exists() {
            return Err(format!("mondo non trovato: {w}"));
        }
    }
    let name = instance.file_name().and_then(|n| n.to_str()).ok_or("istanza Prism non valida")?;
    let prism = instance.parent().and_then(Path::parent).ok_or("cartella Prism non valida")?;
    let account = account(player, user_name, &prism.join("accounts.json"))?;

    let game = game_dir(instance);
    let own = player_dir(player);
    for d in ["saves", "screenshots"] {
        link_dir(&game, d, &own.join(d))?;
    }
    // Controlify's settings used to be kept as `controlify`
    if own.join("controlify").is_dir() && !own.join("config/controlify").exists() {
        let _ = std::fs::create_dir_all(own.join("config"));
        let _ = std::fs::rename(own.join("controlify"), own.join("config/controlify"));
    }
    // the player's own settings, or the shared defaults the first time
    let defaults = defaults_dir(instance);
    for rel in PLAYER_FILES {
        let from = [own.join(rel), defaults.join(rel)].into_iter().find(|p| p.exists());
        if let Some(from) = from {
            copy_any(&from, &game.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
        }
    }
    let from = if own.join("options.txt").exists() || !defaults.join("shaderpacks").is_dir() { &own } else { &defaults };
    copy_shader_settings(from, &game).map_err(|e| format!("shaderpacks: {e}"))?;
    fix_shader_pack(&game);
    let options = std::fs::read_to_string(game.join("options.txt")).unwrap_or_default();
    std::fs::write(game.join("options.txt"), options_set(&options, &[("fullscreen", "true")])).map_err(|e| e.to_string())?;
    // no launcher window, and Prism quits with the game (AzotoTV waits for it)
    let cfg = instance.join("instance.cfg");
    let text = std::fs::read_to_string(&cfg).map_err(|e| format!("{}: {e}", cfg.display()))?;
    let text = cfg_set(
        &text,
        &[("OverrideMiscellaneous", "true"), ("CloseAfterLaunch", "true"), ("QuitAfterGameStop", "true")],
    );
    std::fs::write(&cfg, text).map_err(|e| e.to_string())?;

    let mut c = Command::new(std::env::var_os("AZOTOTV_PRISM").unwrap_or_else(|| "prismlauncher".into()));
    c.arg("-l").arg(name).arg("-a").arg(&account);
    if let Some(w) = world {
        c.arg("-w").arg(w);
    }
    eprintln!("[minecraft] {name} as {account}, world {world:?}");
    Ok(c)
}

/// After the game: the player keeps the settings they changed.
pub fn save(player: &Path, instance: &Path) {
    let (game, own) = (game_dir(instance), player_dir(player));
    for rel in PLAYER_FILES {
        if game.join(rel).exists() {
            if let Err(e) = copy_any(&game.join(rel), &own.join(rel)) {
                eprintln!("[minecraft] {rel}: {e}");
            }
        }
    }
    if let Err(e) = copy_shader_settings(&game, &own) {
        eprintln!("[minecraft] shaderpacks: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_players_match_minecraft() {
        // what Minecraft itself gives the offline player "Steve"
        assert_eq!(offline_uuid("Steve"), "5627dd98e6be3c21b8a8e92344183641");
        assert_eq!(offline_name("TV"), "TV_");
        assert_eq!(offline_name("azoto-dev"), "azotodev");
        assert_eq!(offline_name("Ada"), "Ada");
    }

    fn tag(out: &mut Vec<u8>, t: u8, name: &str) {
        out.push(t);
        out.extend((name.len() as u16).to_be_bytes());
        out.extend(name.as_bytes());
    }

    #[test]
    fn level_dat_fields_are_found() {
        let mut b = Vec::new();
        tag(&mut b, 10, "");
        tag(&mut b, 1, "before");
        b.push(7);
        tag(&mut b, 10, "Data");
        tag(&mut b, 10, "Version"); // a nested compound to skip
        tag(&mut b, 8, "Name");
        b.extend(4u16.to_be_bytes());
        b.extend(b"26.2");
        b.push(0);
        tag(&mut b, 9, "ServerBrands"); // a list to skip
        b.push(3);
        b.extend(2i32.to_be_bytes());
        b.extend(1i32.to_be_bytes());
        b.extend(2i32.to_be_bytes());
        tag(&mut b, 8, "LevelName");
        b.extend(10u16.to_be_bytes());
        b.extend("Città Pro".as_bytes());
        tag(&mut b, 4, "LastPlayed");
        b.extend(1_760_000_000_123i64.to_be_bytes());
        tag(&mut b, 3, "GameType");
        b.extend(1i32.to_be_bytes());
        tag(&mut b, 10, "difficulty_settings");
        tag(&mut b, 1, "hardcore");
        b.push(1);
        b.push(0);
        b.push(0); // end of Data
        b.push(0); // end of the root
        assert_eq!(
            parse_level(&b),
            Some(Level {
                name: Some("Città Pro".into()),
                last_played: Some(1_760_000_000_123),
                game_type: Some(1),
                hardcore: true
            })
        );
        assert_eq!(parse_level(&b[..b.len() - 9]), None); // cut short
    }

    #[test]
    fn shader_packs_match_across_versions() {
        assert_eq!(pack_stem("ComplementaryReimagined_r5.9.zip"), pack_stem("ComplementaryReimagined_r5.9.3.zip"));
        assert_eq!(pack_stem("MakeUp-UltraFast-9.5g.zip"), "makeup-ultrafast-");
        assert_ne!(pack_stem("MakeUp-UltraFast-9.5g.zip"), pack_stem("ComplementaryReimagined_r5.9.3.zip"));
    }

    #[test]
    fn settings_are_replaced_or_added() {
        assert_eq!(options_set("lang:en_us\nfullscreen:false\n", &[("fullscreen", "true")]), "lang:en_us\nfullscreen:true\n");
        assert_eq!(options_set("lang:en_us", &[("guiScale", "0")]), "lang:en_us\nguiScale:0\n");
        assert_eq!(
            cfg_set("[General]\nCloseAfterLaunch=false\nname=X\n[UI]\na=1\n", &[("CloseAfterLaunch", "true"), ("QuitAfterGameStop", "true")]),
            "[General]\nCloseAfterLaunch=true\nname=X\nQuitAfterGameStop=true\n[UI]\na=1\n"
        );
    }
}
