//! Where the games are, and adding them from the TV (Giochi › Add a game,
//! Game folders, a game's Edit page).
//!
//! `~/.config/azototv/games.json` (written by the Game folders page):
//!
//! ```json
//! {"folders": ["~/Games", "/mnt/disk/Games"], "roms": ["~/Games/roms"], "hidden": []}
//! ```
//! - `folders`: game libraries, one folder per game with its `game.json` (see
//!   `games.rs`). The first one is where AzotoTV puts the games it adds.
//!   Default: `$AZOTOTV_GAMES`, else `~/Games`.
//! - `roms`: folders searched, subfolders too, for console games: Switch
//!   (`.nsp`, `.xci`; updates and DLC are left out by their title ID), Wii
//!   (`.rvz`, `.wia`, `.wbfs`, `.iso`; GameCube discs are left out) and 3DS
//!   (`.3ds`, `.cci`, `.cxi`). Each new one gets a game folder in the first
//!   library (`"added": "rom"`), its art and description looked up online
//!   once. Default: `roms` inside the first library.
//! - `hidden`: ROMs taken out of AzotoTV, so the search doesn't bring them back
//!   (and Steam games, as `steam:<appid>`: Steam's installed games are added
//!   by themselves, see `steam.rs`).
//!
//! A game added from the TV stays where it is: its `game.json` points at the
//! executable (`"added": "pc"`). Art: SteamGridDB when it has a key (START ›
//! Services), Steam's own for Steam games, GameTDB box art for Wii and 3DS
//! discs. Description: Steam's, else Wikipedia's in the interface's language.

use std::{
    collections::HashSet,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::games::{Info, Kind, Trailer, expand_home, home};
use crate::jellyfin::config_dir;

// ---- folders ------------------------------------------------------------------

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folders: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roms: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<String>,
}

fn config_path() -> PathBuf {
    config_dir().join("games.json")
}

fn config() -> Config {
    fs::read_to_string(config_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_config(c: &Config) -> Result<(), String> {
    crate::services::write(&config_path(), &serde_json::to_value(c).map_err(|e| e.to_string())?)
}

fn default_folder() -> String {
    std::env::var("AZOTOTV_GAMES").unwrap_or_else(|_| "~/Games".into())
}

/// The game libraries as written in games.json ("~/Games").
pub fn folder_names() -> Vec<String> {
    config().folders.filter(|f| !f.is_empty()).unwrap_or_else(|| vec![default_folder()])
}

/// The ROM folders as written.
pub fn rom_names() -> Vec<String> {
    config().roms.unwrap_or_else(|| vec![format!("{}/roms", folder_names()[0].trim_end_matches('/'))])
}

pub fn libraries() -> Vec<PathBuf> {
    folder_names().iter().map(|f| expand_home(f)).collect()
}

/// Where new games go.
pub fn main_library() -> PathBuf {
    libraries().into_iter().next().unwrap_or_else(|| home().join("Games"))
}

/// A game's folder: in the first library that has it.
pub fn game_dir(id: &str) -> PathBuf {
    libraries().into_iter().map(|l| l.join(id)).find(|d| d.join("game.json").is_file()).unwrap_or_else(|| main_library().join(id))
}

/// Every game, by id (its folder's name); with the same id in two
/// libraries the first one wins.
pub fn ids() -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for lib in libraries() {
        for e in fs::read_dir(&lib).into_iter().flatten().flatten() {
            let id = e.file_name().to_string_lossy().to_string();
            if e.path().join("game.json").is_file() && seen.insert(id.clone()) {
                out.push(id);
            }
        }
    }
    out
}

#[derive(Serialize)]
pub struct Folders {
    folders: Vec<String>,
    roms: Vec<String>,
    /// ROMs taken out ("hidden"), so the page can bring them back
    hidden: usize,
}

pub fn folders() -> Folders {
    Folders { folders: folder_names(), roms: rom_names(), hidden: config().hidden.len() }
}

/// New lists of folders (each must exist). `unhide`: forget the ROMs taken out.
pub fn set_folders(folders: Vec<String>, roms: Vec<String>, unhide: bool) -> Result<Folders, String> {
    let tidy = |v: Vec<String>| -> Result<Vec<String>, String> {
        let mut out: Vec<String> = Vec::new();
        for f in v {
            let f = f.trim().trim_end_matches('/').to_string();
            if f.is_empty() || out.contains(&f) {
                continue;
            }
            if !expand_home(&f).is_dir() {
                return Err(t!("There is no folder {dir}", dir = f));
            }
            out.push(f);
        }
        Ok(out)
    };
    let folders = tidy(folders)?;
    if folders.is_empty() {
        return Err(t!("At least one game folder is needed"));
    }
    let mut c = config();
    c.folders = Some(folders);
    c.roms = Some(tidy(roms)?);
    if unhide {
        c.hidden.clear();
    }
    save_config(&c)?;
    Ok(self::folders())
}

/// Makes `dir` one of the game folders, when it isn't (a game downloaded
/// from another AzotoTV into a folder of its own).
pub fn add_library(dir: &Path) -> Result<(), String> {
    let canon_dir = canon(dir);
    if libraries().iter().any(|l| canon(l) == canon_dir) {
        return Ok(());
    }
    let mut c = config();
    let mut folders = folder_names();
    folders.push(dir.to_string_lossy().to_string());
    c.folders = Some(folders);
    save_config(&c)
}

// ---- browsing the disk (folder and file pickers) -------------------------------

#[derive(Serialize)]
pub struct Entry {
    name: String,
    path: String,
    dir: bool,
    /// looks like something to start (an .exe, a Linux binary, a ROM)
    game: bool,
}

#[derive(Serialize)]
pub struct Listing {
    path: String,
    parent: Option<String>,
    entries: Vec<Entry>,
    /// home and the mounted disks: [name, path]
    places: Vec<(String, String)>,
}

const NOT_GAMES: [&str; 8] = ["unins", "crashpad", "unitycrashhandler", "vc_redist", "dxsetup", "dotnet", "uninstall", "setup"];

fn is_game_file(p: &Path) -> bool {
    let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if NOT_GAMES.iter().any(|n| name.starts_with(n)) {
        return false;
    }
    if rom_system(p).is_some() {
        return true;
    }
    match p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("exe" | "x86_64" | "x86" | "sh" | "appimage") => true,
        Some(_) => false,
        // an executable ELF file
        None => {
            use std::os::unix::fs::PermissionsExt;
            let exec = fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
            let mut magic = [0u8; 4];
            exec && fs::File::open(p).and_then(|mut f| f.read_exact(&mut magic)).is_ok() && &magic == b"\x7fELF"
        }
    }
}

fn places() -> Vec<(String, String)> {
    let mut v = vec![(t!("Home|folder"), home().to_string_lossy().to_string())];
    let user = std::env::var("USER").unwrap_or_default();
    for root in [format!("/run/media/{user}"), "/media".into(), "/mnt".into()] {
        for e in fs::read_dir(&root).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                v.push((e.file_name().to_string_lossy().to_string(), e.path().to_string_lossy().to_string()));
            }
        }
    }
    v
}

pub fn browse(path: Option<String>) -> Result<Listing, String> {
    let p = path.filter(|p| !p.is_empty()).map(|p| expand_home(&p)).unwrap_or_else(home);
    let rd = fs::read_dir(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut entries: Vec<Entry> = rd
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| {
            let path = e.path();
            let dir = path.is_dir();
            Entry { name: e.file_name().to_string_lossy().to_string(), game: !dir && is_game_file(&path), path: path.to_string_lossy().to_string(), dir }
        })
        .collect();
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(Listing {
        path: p.to_string_lossy().to_string(),
        parent: p.parent().map(|q| q.to_string_lossy().to_string()),
        entries,
        places: places(),
    })
}

// ---- ROMs -----------------------------------------------------------------------

fn rom_system(p: &Path) -> Option<Kind> {
    match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "nsp" | "xci" => Some(Kind::Switch),
        "rvz" | "wia" | "wbfs" | "iso" => Some(Kind::Wii),
        "3ds" | "cci" | "cxi" => Some(Kind::N3ds),
        _ => None,
    }
}

/// A Wii / GameCube disc image's header: its 6-letter ID and title.
#[derive(Debug, PartialEq)]
struct Disc {
    id: String,
    title: String,
    wii: bool,
}

fn disc_header(path: &Path) -> Option<Disc> {
    let mut f = fs::File::open(path).ok()?;
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).ok()?;
    // .wbfs: the disc starts after its 0x200-byte header; .rvz/.wia keep a
    // copy of the disc's header uncompressed at 0x58 (after header 1, 0x48
    // bytes, and the first 16 of header 2)
    let off = match &magic {
        b"WBFS" => 0x200,
        b"RVZ\x01" | b"WIA\x01" => 0x58,
        _ => 0,
    };
    f.seek(SeekFrom::Start(off)).ok()?;
    let mut h = [0u8; 0x60];
    f.read_exact(&mut h).ok()?;
    parse_disc(&h)
}

fn parse_disc(h: &[u8; 0x60]) -> Option<Disc> {
    let wii = h[0x18..0x1c] == [0x5D, 0x1C, 0x9E, 0xA3];
    let gc = h[0x1c..0x20] == [0xC2, 0x33, 0x9F, 0x3D];
    let id = std::str::from_utf8(&h[..6]).ok()?.to_string();
    if !(wii || gc) || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let t = &h[0x20..0x60];
    let end = t.iter().position(|&b| b == 0).unwrap_or(t.len());
    Some(Disc { id, title: String::from_utf8_lossy(&t[..end]).trim().to_string(), wii })
}

/// A 3DS game's product code ("CTR-P-AREE" -> "AREE").
fn product_code(path: &Path) -> Option<String> {
    let mut f = fs::File::open(path).ok()?;
    let mut h = [0u8; 0x200];
    f.read_exact(&mut h).ok()?;
    // .3ds/.cci (NCSD): partition 0's offset in 0x200 units at 0x120; .cxi is the NCCH itself
    let ncch = if &h[0x100..0x104] == b"NCSD" { u32::from_le_bytes(h[0x120..0x124].try_into().ok()?) as u64 * 0x200 } else { 0 };
    f.seek(SeekFrom::Start(ncch)).ok()?;
    let mut n = [0u8; 0x160];
    f.read_exact(&mut n).ok()?;
    if &n[0x100..0x104] != b"NCCH" {
        return None;
    }
    let code = String::from_utf8_lossy(&n[0x150..0x160]).trim_end_matches('\0').to_string();
    let last = code.rsplit('-').next()?.to_string();
    (last.len() == 4 && last.chars().all(|c| c.is_ascii_alphanumeric())).then_some(last)
}

/// "[0100152000022000]" in a Switch file's name.
fn title_id(name: &str) -> Option<String> {
    name.split('[').filter_map(|p| p.split(']').next()).find(|p| p.len() == 16 && p.chars().all(|c| c.is_ascii_hexdigit())).map(str::to_uppercase)
}

/// "Mario_Kart (USA) [v0]" -> "Mario Kart".
fn clean_name(stem: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for c in stem.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(if c == '_' { ' ' } else { c }),
            _ => {}
        }
    }
    let s = out.split_whitespace().collect::<Vec<_>>().join(" ");
    s.trim_matches(|c: char| c == '-' || c == '.' || c.is_whitespace()).to_string()
}

/// What a ROM is: its name, kind and the code its box art is found by.
struct Rom {
    name: String,
    kind: Kind,
    code: Option<String>,
}

fn identify(path: &Path) -> Option<Rom> {
    let kind = rom_system(path)?;
    let file = path.file_name()?.to_string_lossy().to_string();
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let mut name = clean_name(&stem);
    let mut code = None;
    match kind {
        Kind::Switch => {
            // a base game's title ID ends in 000 (updates 800, DLC others)
            if title_id(&file).is_some_and(|t| !t.ends_with("000")) {
                return None;
            }
        }
        Kind::Wii => {
            let d = disc_header(path)?;
            if !d.wii {
                return None; // GameCube: no launcher for it yet
            }
            // a file named after its ID ("RMCE01") or nothing useful
            if name.len() < 3 || name.eq_ignore_ascii_case(&d.id) {
                name = titlecase(&d.title);
            }
            code = Some(d.id);
        }
        Kind::N3ds => code = product_code(path),
        _ => {}
    }
    (!name.is_empty()).then_some(Rom { name, kind, code })
}

/// "MARIO KART WII" -> "Mario Kart Wii" (titles in disc headers are often all capitals).
fn titlecase(s: &str) -> String {
    if s.chars().any(|c| c.is_lowercase()) {
        return s.to_string();
    }
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_string() + &c.as_str().to_lowercase()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn walk(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if depth > 0 {
                walk(&p, depth - 1, out);
            }
        } else if rom_system(&p).is_some() {
            out.push(p);
        }
    }
}

fn canon(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Every file some game.json starts (its exe and its chapters').
fn known_files() -> HashSet<PathBuf> {
    let mut set = HashSet::new();
    for id in ids() {
        let dir = game_dir(&id);
        let Some(v) = fs::read_to_string(dir.join("game.json")).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) else { continue };
        let exes = std::iter::once(&v["exe"]).chain(v["chapters"].as_array().into_iter().flatten().map(|c| &c["exe"]));
        for e in exes.filter_map(Value::as_str) {
            let p = if e.starts_with("~/") || e.starts_with('/') { expand_home(e) } else { dir.join(e) };
            set.insert(canon(&p));
        }
    }
    set
}

/// A folder name for a new game: "Mario Kart Wii" -> "mario-kart-wii" (-2, -3… when taken).
fn new_id(name: &str) -> String {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let mut base: String = slug.trim_matches('-').chars().take(40).collect();
    base = base.trim_end_matches('-').to_string();
    if base.is_empty() {
        base = "game".into();
    }
    let taken = |id: &str| libraries().iter().any(|l| l.join(id).exists());
    if !taken(&base) {
        return base;
    }
    (2..).map(|n| format!("{base}-{n}")).find(|id| !taken(id)).unwrap()
}

fn kind_json(k: Kind) -> Value {
    serde_json::to_value(k).unwrap_or(Value::Null)
}

/// Writes a new game's folder in the first library.
fn create(name: &str, kind: Kind, exe: &Path, added: &str, extra: Value) -> Result<String, String> {
    let id = new_id(name);
    let dir = main_library().join(&id);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut m = json!({ "name": name, "type": kind_json(kind), "exe": exe.to_string_lossy(), "added": added });
    if let (Some(o), Some(x)) = (m.as_object_mut(), extra.as_object()) {
        o.extend(x.clone());
    }
    fs::write(dir.join("game.json"), serde_json::to_string_pretty(&m).unwrap_or_default()).map_err(|e| e.to_string())?;
    Ok(id)
}

/// What to look the art and description up by.
#[derive(Clone)]
pub struct Hint {
    id: String,
    name: String,
    kind: Kind,
    steam_appid: Option<u32>,
    code: Option<String>,
}

impl Hint {
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Looks for ROMs nobody has added yet and adds them. Returns what to fetch
/// art for (the new ones).
pub fn scan() -> Vec<Hint> {
    let cfg = config();
    let hidden: HashSet<PathBuf> = cfg.hidden.iter().map(|h| canon(Path::new(h))).collect();
    let known = known_files();
    let mut files = Vec::new();
    for dir in rom_names() {
        walk(&expand_home(&dir), 3, &mut files);
    }
    files.sort();
    let mut new = scan_steam(&cfg);
    for f in files {
        let c = canon(&f);
        if known.contains(&c) || hidden.contains(&c) {
            continue;
        }
        let Some(rom) = identify(&f) else { continue };
        match create(&rom.name, rom.kind, &c, "rom", json!({})) {
            Ok(id) => {
                eprintln!("[library] new: {} ({})", rom.name, c.display());
                new.push(Hint { id, name: rom.name, kind: rom.kind, steam_appid: None, code: rom.code });
            }
            Err(e) => eprintln!("[library] {}: {e}", c.display()),
        }
    }
    new
}

/// Steam games installed since the last look (see `steam.rs`). A game the
/// library has already, under that Steam app id, isn't added twice.
fn scan_steam(cfg: &Config) -> Vec<Hint> {
    let apps = crate::steam::installed();
    if apps.is_empty() {
        return Vec::new();
    }
    let known: HashSet<u64> = ids()
        .iter()
        .filter_map(|id| fs::read_to_string(game_dir(id).join("game.json")).ok())
        .filter_map(|t| serde_json::from_str::<Value>(&t).ok()?["steam_appid"].as_u64())
        .collect();
    let mut new = Vec::new();
    for app in apps {
        if known.contains(&u64::from(app.appid)) || cfg.hidden.contains(&format!("steam:{}", app.appid)) {
            continue;
        }
        match create(&app.name, Kind::Steam, Path::new(""), "steam", json!({ "steam_appid": app.appid })) {
            Ok(id) => {
                eprintln!("[library] new from Steam: {}", app.name);
                crate::steam::copy_art(app.appid, &game_dir(&id));
                new.push(Hint { id, name: app.name, kind: Kind::Steam, steam_appid: Some(app.appid), code: None });
            }
            Err(e) => eprintln!("[library] {}: {e}", app.name),
        }
    }
    new
}

// ---- adding, changing, removing --------------------------------------------------

#[derive(Deserialize)]
pub struct NewGame {
    exe: String,
    name: String,
    #[serde(default)]
    steam_appid: Option<u32>,
    #[serde(default)]
    players: Option<u32>,
}

pub fn add(g: NewGame) -> Result<Hint, String> {
    let exe = canon(&expand_home(&g.exe));
    if !exe.is_file() {
        return Err(t!("There is no file {file}", file = exe.display()));
    }
    if known_files().contains(&exe) {
        return Err(t!("That game is already in AzotoTV"));
    }
    let name = g.name.trim();
    if name.is_empty() {
        return Err(t!("The game needs a name"));
    }
    let rom = identify(&exe);
    let kind = match (&rom, exe.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref()) {
        (Some(r), _) => r.kind,
        (None, Some("exe")) => Kind::Proton,
        _ => Kind::Native,
    };
    let mut extra = json!({});
    if let Some(a) = g.steam_appid {
        extra["steam_appid"] = json!(a);
    }
    if let Some(p) = g.players.filter(|p| *p > 1) {
        extra["players"] = json!(p.min(8));
    }
    let added = if rom.is_some() { "rom" } else { "pc" };
    let id = create(name, kind, &exe, added, extra)?;
    let mut hidden = config();
    let before = hidden.hidden.len();
    hidden.hidden.retain(|h| canon(Path::new(h)) != exe);
    if hidden.hidden.len() != before {
        save_config(&hidden)?;
    }
    Ok(Hint { id, name: name.into(), kind, steam_appid: g.steam_appid, code: rom.and_then(|r| r.code) })
}

fn manifest_value(id: &str) -> Result<(PathBuf, Value), String> {
    let p = game_dir(id).join("game.json");
    let v = fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).ok_or_else(|| t!("not a valid game: {id}", id = id))?;
    Ok((p, v))
}

#[derive(Deserialize)]
pub struct Change {
    name: String,
    players: u32,
    /// None: no Steam game; Some: this one
    steam_appid: Option<u32>,
    /// look the art and description up again
    #[serde(default)]
    refresh: bool,
}

/// Changes what the Edit page shows; Some(hint) when the art should be looked up again.
pub fn edit(id: &str, c: Change) -> Result<Option<Hint>, String> {
    let (path, mut v) = manifest_value(id)?;
    let name = c.name.trim();
    if name.is_empty() {
        return Err(t!("The game needs a name"));
    }
    let appid_changed = v["steam_appid"].as_u64() != c.steam_appid.map(u64::from);
    v["name"] = json!(name);
    if c.players > 1 {
        v["players"] = json!(c.players.min(8));
    } else if let Some(o) = v.as_object_mut() {
        o.remove("players");
    }
    match c.steam_appid {
        Some(a) => v["steam_appid"] = json!(a),
        None => {
            if let Some(o) = v.as_object_mut() {
                o.remove("steam_appid");
            }
        }
    }
    fs::write(&path, serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| e.to_string())?;
    if !(c.refresh || appid_changed) {
        return Ok(None);
    }
    // new art: the old goes (kept as art.old), and the description
    let dir = game_dir(id);
    let _ = fs::remove_dir_all(dir.join("art.old"));
    let _ = fs::rename(dir.join("art"), dir.join("art.old"));
    let _ = fs::rename(dir.join("info.json"), dir.join("info.json.old"));
    Ok(Some(hint_for(id)?))
}

fn hint_for(id: &str) -> Result<Hint, String> {
    let (_, v) = manifest_value(id)?;
    let kind: Kind = serde_json::from_value(v["type"].clone()).map_err(|e| e.to_string())?;
    let exe = v["exe"].as_str().map(|e| if e.starts_with('/') || e.starts_with("~/") { expand_home(e) } else { game_dir(id).join(e) });
    let code = exe.as_deref().and_then(identify).and_then(|r| r.code);
    Ok(Hint { id: id.into(), name: v["name"].as_str().unwrap_or(id).into(), kind, steam_appid: v["steam_appid"].as_u64().map(|a| a as u32), code })
}

/// Takes a game out of AzotoTV. Never deletes a game's own files: a folder
/// AzotoTV made (an added game: game.json, info.json, art) goes, anything
/// else only stops being a game (game.json -> game.json.removed). A ROM is
/// also remembered as taken out, so the search doesn't add it again.
pub fn remove(id: &str) -> Result<(), String> {
    let (path, v) = manifest_value(id)?;
    let dir = game_dir(id);
    if let (Some("steam"), Some(a)) = (v["added"].as_str(), v["steam_appid"].as_u64()) {
        let mut c = config();
        c.hidden.push(format!("steam:{a}"));
        save_config(&c)?;
    }
    if v["added"].as_str() == Some("rom") {
        if let Some(e) = v["exe"].as_str() {
            let mut c = config();
            c.hidden.push(canon(&expand_home(e)).to_string_lossy().to_string());
            save_config(&c)?;
        }
    }
    const OURS: [&str; 5] = ["game.json", "info.json", "info.json.old", "art", "art.old"];
    let only_ours = v["added"].is_string()
        && fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten().all(|e| OURS.contains(&e.file_name().to_string_lossy().as_ref()));
    if only_ours {
        fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))
    } else {
        fs::rename(&path, path.with_extension("json.removed")).map_err(|e| e.to_string())
    }
}

// ---- art and description ------------------------------------------------------------

const UA: &str = "AzotoTV (https://github.com/IKIR25/AzotoTV)";

fn client() -> reqwest::Client {
    reqwest::Client::builder().user_agent(UA).timeout(Duration::from_secs(20)).build().expect("http client")
}

fn sgdb_key() -> Option<String> {
    let v: Value = serde_json::from_str(&fs::read_to_string(config_dir().join("steamgriddb.json")).ok()?).ok()?;
    v["apiKey"].as_str().filter(|k| !k.is_empty()).map(str::to_string)
}

async fn get_json(c: &reqwest::Client, url: &str, bearer: Option<&str>) -> Option<Value> {
    let mut r = c.get(url);
    if let Some(b) = bearer {
        r = r.bearer_auth(b);
    }
    let resp = r.send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json().await.ok()
}

/// Saves `url` as `art/<kind>.<jpg|png|webp>` unless that kind is there already.
async fn save_art(c: &reqwest::Client, dir: &Path, kind: &str, url: &str) -> bool {
    let art = dir.join("art");
    if ["jpg", "png", "webp"].iter().any(|e| art.join(format!("{kind}.{e}")).exists()) {
        return true;
    }
    let Ok(resp) = c.get(url).send().await else { return false };
    let ext = match resp.headers().get("content-type").and_then(|v| v.to_str().ok()) {
        Some(t) if t.contains("png") => "png",
        Some(t) if t.contains("webp") => "webp",
        Some(t) if t.contains("jpeg") || t.contains("jpg") => "jpg",
        _ => return false,
    };
    if !resp.status().is_success() {
        return false;
    }
    let Ok(bytes) = resp.bytes().await else { return false };
    fs::create_dir_all(&art).is_ok() && fs::write(art.join(format!("{kind}.{ext}")), &bytes).is_ok()
}

fn region_dirs(code: &str) -> Vec<&'static str> {
    let it = crate::locale::lang() == "it";
    let mut v = match code.chars().nth(3) {
        Some('E') => vec!["US"],
        Some('J') => vec!["JA"],
        Some('K') => vec!["KO"],
        Some('D') => vec!["DE"],
        Some('F') => vec!["FR"],
        Some('I') => vec!["IT"],
        Some('S') => vec!["ES"],
        Some('P') if it => vec!["IT", "EN"],
        Some('P') => vec!["EN"],
        _ => vec![],
    };
    v.extend(["US", "EN"]);
    v
}

async fn fetch_art(c: &reqwest::Client, h: &Hint, dir: &Path) {
    if let Some(key) = sgdb_key() {
        let base = "https://www.steamgriddb.com/api/v2";
        let game = match h.steam_appid {
            Some(a) => get_json(c, &format!("{base}/games/steam/{a}"), Some(&key)).await.and_then(|v| v["data"]["id"].as_u64()),
            None => None,
        };
        let game = match game {
            Some(g) => Some(g),
            None => get_json(c, &format!("{base}/search/autocomplete/{}", urlencoding(&h.name)), Some(&key)).await.and_then(|v| v["data"][0]["id"].as_u64()),
        };
        if let Some(g) = game {
            for (kind, path) in [
                ("cover", format!("grids/game/{g}?dimensions=600x900&types=static")),
                ("header", format!("grids/game/{g}?dimensions=460x215,920x430&types=static")),
                ("hero", format!("heroes/game/{g}?types=static")),
                ("logo", format!("logos/game/{g}?types=static")),
            ] {
                if let Some(url) = get_json(c, &format!("{base}/{path}"), Some(&key)).await.and_then(|v| v["data"][0]["url"].as_str().map(str::to_string)) {
                    save_art(c, dir, kind, &url).await;
                }
            }
        }
    }
    if let Some(a) = h.steam_appid {
        let cdn = format!("https://cdn.akamai.steamstatic.com/steam/apps/{a}");
        for (kind, file) in [("cover", "library_600x900.jpg"), ("header", "header.jpg"), ("hero", "library_hero.jpg"), ("logo", "logo.png")] {
            save_art(c, dir, kind, &format!("{cdn}/{file}")).await;
        }
    }
    if let Some(code) = &h.code {
        let sys = if h.kind == Kind::N3ds { "3ds/box" } else { "wii/cover" };
        for r in region_dirs(code) {
            if save_art(c, dir, "cover", &format!("https://art.gametdb.com/{sys}/{r}/{code}.png")).await {
                break;
            }
        }
    }
}

fn urlencoding(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Steam's text has a little HTML and its entities.
fn plain(s: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in s.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", " ").trim().to_string()
}

fn year_in(s: &str) -> Option<String> {
    s.as_bytes().windows(4).rev().map(|w| String::from_utf8_lossy(w).to_string()).find(|w| w.chars().all(|c| c.is_ascii_digit()) && (w.starts_with("19") || w.starts_with("20")))
}

async fn steam_info(c: &reqwest::Client, appid: u32) -> Option<Info> {
    let lang = crate::locale::steam_language();
    let v = get_json(c, &format!("https://store.steampowered.com/api/appdetails?appids={appid}&l={lang}"), None).await?;
    let d = &v[appid.to_string()]["data"];
    if !d.is_object() {
        return None;
    }
    let strings = |a: &Value, f: &dyn Fn(&Value) -> Option<String>| a.as_array().into_iter().flatten().filter_map(f).collect::<Vec<_>>();
    Some(Info {
        description: plain(d["short_description"].as_str().unwrap_or("")),
        genres: strings(&d["genres"], &|g| g["description"].as_str().map(str::to_string)),
        developers: strings(&d["developers"], &|g| g.as_str().map(str::to_string)),
        year: d["release_date"]["date"].as_str().and_then(year_in),
        trailers: d["movies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let url = m["hls_h264"].as_str().or(m["mp4"]["max"].as_str())?;
                Some(Trailer { name: m["name"].as_str().unwrap_or("Trailer").to_string(), url: url.to_string() })
            })
            .take(4)
            .collect(),
    })
}

async fn wiki_info(c: &reqwest::Client, name: &str) -> Option<Info> {
    for lang in [crate::locale::lang(), "en".to_string()].iter().collect::<Vec<_>>() {
        let wiki = format!("https://{lang}.wikipedia.org");
        let Some(found) = get_json(c, &format!("{wiki}/w/rest.php/v1/search/title?q={}&limit=5", urlencoding(name)), None).await else { continue };
        // the page about the game, not a namesake ("videogioco del 2008", "2008 video game")
        let page = found["pages"].as_array().into_iter().flatten().find(|p| {
            let d = p["description"].as_str().unwrap_or("").to_lowercase();
            d.contains("game") || d.contains("gioco")
        });
        let Some(page) = page else { continue };
        let year = page["description"].as_str().and_then(year_in);
        let key = page["key"].as_str()?;
        let Some(sum) = get_json(c, &format!("{wiki}/api/rest_v1/page/summary/{}", urlencoding(key)), None).await else { continue };
        let text = sum["extract"].as_str().unwrap_or("").trim().to_string();
        if !text.is_empty() {
            return Some(Info { description: text, year, ..Default::default() });
        }
    }
    None
}

/// Looks up what a game is missing (art kinds, info.json).
pub async fn fetch(h: Hint) {
    let c = client();
    let dir = game_dir(&h.id);
    fetch_art(&c, &h, &dir).await;
    if !dir.join("info.json").exists() {
        let info = match h.steam_appid {
            Some(a) => steam_info(&c, a).await,
            None => None,
        };
        let info = match info {
            Some(i) => Some(i),
            None => wiki_info(&c, &h.name).await,
        };
        if let Some(i) = info {
            let _ = fs::write(dir.join("info.json"), serde_json::to_string_pretty(&i).unwrap_or_default());
        }
    }
    eprintln!("[library] looked up {}", h.id);
}

#[derive(Serialize)]
pub struct SteamHit {
    appid: u32,
    name: String,
    image: Option<String>,
}

/// Steam's store search, for the Add / Edit pages.
pub async fn steam_search(term: &str) -> Result<Vec<SteamHit>, String> {
    let url = format!(
        "https://store.steampowered.com/api/storesearch/?term={}&cc={}&l={}",
        urlencoding(term.trim()),
        crate::locale::region(),
        crate::locale::steam_language()
    );
    let v = get_json(&client(), &url, None).await.ok_or_else(|| t!("Steam doesn't answer"))?;
    Ok(v["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| Some(SteamHit { appid: i["id"].as_u64()? as u32, name: i["name"].as_str()?.into(), image: i["tiny_image"].as_str().map(str::to_string) }))
        .take(8)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_lose_tags() {
        assert_eq!(clean_name("Mario_Kart_Wii (USA) (En,Fr,Es)"), "Mario Kart Wii");
        assert_eq!(clean_name("Some Game [0100152000022000][v0] - "), "Some Game");
        assert_eq!(titlecase("MARIO KART WII"), "Mario Kart Wii");
        assert_eq!(titlecase("Wii Sports"), "Wii Sports");
    }

    #[test]
    fn switch_updates_are_not_games() {
        assert_eq!(title_id("X [0100ABCD00022000][v0].nsp").as_deref(), Some("0100ABCD00022000"));
        assert_eq!(title_id("X [0100ABCD00022800][v65536].nsp").as_deref(), Some("0100ABCD00022800"));
        assert_eq!(title_id("X.nsp"), None);
    }

    #[test]
    fn disc_headers() {
        let mut h = [0u8; 0x60];
        h[..6].copy_from_slice(b"RMCP01");
        h[0x18..0x1c].copy_from_slice(&[0x5D, 0x1C, 0x9E, 0xA3]);
        h[0x20..0x2e].copy_from_slice(b"MARIO KART WII");
        assert_eq!(parse_disc(&h), Some(Disc { id: "RMCP01".into(), title: "MARIO KART WII".into(), wii: true }));
        h[0x18..0x1c].copy_from_slice(&[0; 4]);
        assert_eq!(parse_disc(&h), None);
    }

    #[test]
    fn slugs() {
        // (new_id also checks the libraries, which these names aren't in)
        assert!(new_id("Ada's Quest: Part II!").starts_with("ada-s-quest-part-ii"));
        assert!(new_id("???").starts_with("game"));
    }

    #[test]
    fn steam_text() {
        assert_eq!(plain("A <b>bold</b> &quot;run&quot; &amp; gun"), "A bold \"run\" & gun");
        assert_eq!(year_in("29 Sep, 2017").as_deref(), Some("2017"));
        assert_eq!(year_in("videogioco del 2008").as_deref(), Some("2008"));
    }
}

#[cfg(test)]
mod samples {
    /// AZOTOTV_ROM_SAMPLES=<dir> cargo test samples -- --ignored --nocapture
    #[test]
    #[ignore]
    fn identify_samples() {
        let Ok(dir) = std::env::var("AZOTOTV_ROM_SAMPLES") else { return };
        let mut files = Vec::new();
        super::walk(std::path::Path::new(&dir), 3, &mut files);
        for f in files {
            let r = super::identify(&f);
            println!("{} -> {:?}", f.display(), r.map(|r| (r.name, r.kind, r.code)));
        }
    }
}
