//! Saves shared with a linked AzotoTV (`peer.rs`): a game downloaded from the
//! other machine (or by it) keeps one set of saves per profile on both, unless
//! one of them says "Only here" for that game (`saves.json`: `mode`).
//! By itself a game shares them while each profile has the same mods on, on
//! both sides: a game modded on one machine only would read saves it can't
//! understand. Steam games are left to Steam Cloud.
//!
//! Where a game keeps them: save "roots", folders of the player's named the same
//! on every machine (`home`, `wine`, `switch`, `wii`, `3ds`, …, see [`root`]).
//! Inside them the game's own "units" (its folders, like
//! `home/.local/share/Terraria`): some are known (a Windows game's whole user
//! folder in its Wine prefix, a Wii disc's NAND title), the rest is learned:
//! what a game writes while it runs becomes its units (`players/<id>/saves.json`).
//!
//! When: before a game starts and after it ends, for its profile. The side
//! that changed since the last sync (both remember its digest) gives its saves
//! to the other; when both changed, the newest one wins. What a sync replaces
//! is kept until the next one in `players/<id>/saves-prev/<game>/`.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    net::SocketAddr,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::StreamExt;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::games::{Kind, Manifest, data_dir, player_dir};
use crate::peer::{self, Peer};

// ---- where a game keeps them ------------------------------------------------------

/// A save root of this player for this game, on this machine.
fn root(kind: Kind, user: &str, id: &str, name: &str) -> Option<PathBuf> {
    let p = player_dir(user);
    match (kind, name) {
        (Kind::Steam, _) => None,
        (_, "dirs") => Some(p.join("dirs").join(id)),
        (Kind::Native, "home") => Some(p.join("home")),
        (Kind::Proton, "wine") => Some(p.join("wine").join(id).join("drive_c/users/steamuser")),
        (Kind::Switch, "switch") => crate::eden::save_dir(&p),
        // saves of the console, not of a profile (some games keep them there)
        (Kind::Switch, "switch-device") => Some(crate::eden::eden_data().join("nand/user/save/0000000000000000").join("0".repeat(32))),
        (Kind::Wii, "wii") => Some(p.join("wii/title/00010000")),
        (Kind::N3ds, "3ds") => Some(n3ds_root(&p)),
        (Kind::Minecraft, "minecraft") => Some(p.join("minecraft")),
        _ => None,
    }
}

/// The roots whose units are learned while the game runs.
fn learned_roots(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Native => &["home"],
        Kind::Switch => &["switch", "switch-device"],
        Kind::Wii => &["wii"],
        Kind::N3ds => &["3ds"],
        _ => &[],
    }
}

/// The player's 3DS SD card, where the game saves are: `Nintendo 3DS/<id0>/<id1>`
/// (Azahar's ids are zeros, unless the 3DS came from a real one).
fn n3ds_root(player: &Path) -> PathBuf {
    let base = player.join("3ds/sdmc/Nintendo 3DS");
    let first = |d: &Path| {
        fs::read_dir(d).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).min()
    };
    first(&base)
        .and_then(|id0| first(&id0))
        .unwrap_or_else(|| base.join("0".repeat(32)).join("0".repeat(32)))
}

/// Folders never part of the saves (caches, temporary files, logs).
fn skip_name(n: &str) -> bool {
    const SKIP: [&str; 20] = [
        "temp", "tmp", "cache", "caches", "shadercache", "shadercaches", "dxcache", "d3dscache", "glcache", "gpucache",
        "nv_cache", "code cache", "crashdumps", "crashes", "crash-reports", "inetcache", "webcache", "logs", "microsoft", "nvidia corporation",
    ];
    SKIP.contains(&n.to_lowercase().as_str()) || n.ends_with(".azototv-part")
}

/// How many folders deep a game's unit is in the player's home, for a path in
/// it (None: not saves, like caches and the sound server's settings).
fn home_depth(rel: &[&str]) -> Option<usize> {
    const NOT: [&str; 16] = [
        ".cache", ".nv", ".pki", ".dbus", ".steam", ".config/pulse", ".config/fontconfig", ".local/share/Trash",
        ".local/share/vulkan", ".local/share/mesa_shader_cache", ".local/share/applications", ".local/share/icons",
        ".local/share/fonts", ".local/share/recently-used.xbel", ".local/share/Steam", ".local/state/wireplumber",
    ];
    let s = rel.join("/");
    if NOT.iter().any(|p| s == *p || s.starts_with(&format!("{p}/"))) {
        return None;
    }
    let base = match rel {
        [".local", "share" | "state", ..] => 2,
        [".config", ..] => 1,
        _ => 0,
    };
    // engines that keep every game in one folder: the game's own inside it
    let extra = match rel.get(base) {
        Some(&"unity3d") => 3, // unity3d/<company>/<game>
        Some(&"godot") => 3,   // godot/app_userdata/<game>
        Some(&"GSE Saves") => 2,
        _ => 1,
    };
    Some((base + extra).min(rel.len()))
}

/// The unit a changed file belongs to ("home/.local/share/Terraria").
fn unit_of(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    let root = parts.next()?;
    let rel: Vec<&str> = parts.collect();
    let n = match root {
        "home" => home_depth(&rel)?,
        "switch" | "switch-device" | "wii" => 1,
        "3ds" => match rel.as_slice() {
            ["title" | "extdata", _, _, _, ..] => 3,
            _ => return None,
        },
        _ => return None,
    };
    (rel.len() >= n && n > 0).then(|| format!("{root}/{}", rel[..n].join("/")))
}

/// A logical path ("wine/AppData/Roaming/x.sav") on this machine.
fn resolve(kind: Kind, user: &str, id: &str, logical: &str) -> Option<PathBuf> {
    let (name, rest) = logical.split_once('/').unwrap_or((logical, ""));
    let base = root(kind, user, id, name)?;
    let rel = Path::new(rest);
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(if rest.is_empty() { base } else { base.join(rel) })
}

fn hex4(disc: &str) -> String {
    disc.bytes().take(4).map(|b| format!("{b:02x}")).collect()
}

/// The game's units: the known ones and the learned ones.
fn units(user: &str, id: &str, m: &Manifest) -> BTreeSet<String> {
    let mut u = with_player(user, |p| p.get(id).map(|g| g.units.clone()).unwrap_or_default());
    match m.kind {
        Kind::Proton => {
            u.insert("wine".into());
        }
        Kind::Minecraft => {
            u.insert("minecraft".into());
        }
        Kind::Wii => {
            let disc = crate::games::expand_home(&m.exe);
            let disc = if disc.is_absolute() { disc } else { crate::library::game_dir(id).join(disc) };
            if let Some(d) = crate::library::disc_id(&disc) {
                u.insert(format!("wii/{}", hex4(&d)));
            }
        }
        _ => {}
    }
    if !m.player_dirs.is_empty() {
        u.insert("dirs".into());
    }
    u.retain(|x| resolve(m.kind, user, id, x).is_some());
    u
}

// ---- what is there ------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
struct FileState {
    size: u64,
    /// milliseconds since 1970
    mtime: i64,
}

type Files = BTreeMap<String, FileState>;

fn mtime_ms(md: &fs::Metadata) -> i64 {
    md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Every file under `p` (named `logical`), symlinks and caches left out;
/// `keep` decides about each folder (by its logical path).
fn collect(p: &Path, logical: &str, keep: &dyn Fn(&str) -> bool, out: &mut Files) {
    let Ok(md) = fs::symlink_metadata(p) else { return };
    if md.is_file() {
        out.insert(logical.to_string(), FileState { size: md.len(), mtime: mtime_ms(&md) });
    } else if md.is_dir() && keep(logical) {
        for e in fs::read_dir(p).into_iter().flatten().flatten() {
            let Some(n) = e.file_name().to_str().map(str::to_string) else { continue };
            if !skip_name(&n) {
                collect(&e.path(), &format!("{logical}/{n}"), keep, out);
            }
        }
    }
}

fn manifest(kind: Kind, user: &str, id: &str, units: &BTreeSet<String>) -> Files {
    let mut out = Files::new();
    for u in units {
        if let Some(p) = resolve(kind, user, id, u) {
            collect(&p, u, &|_| true, &mut out);
        }
    }
    out
}

fn digest(f: &Files) -> String {
    let mut h = Md5::new();
    for (p, s) in f {
        h.update(format!("{p}\0{}\0{}\n", s.size, s.mtime));
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn newest(f: &Files) -> i64 {
    f.values().map(|s| s.mtime).max().unwrap_or(0)
}

// ---- learning what a game writes ----------------------------------------------------

/// The learned roots as they were when the game started.
static SNAPSHOT: Mutex<Option<(String, String, Files)>> = Mutex::new(None);

fn learn_walk(kind: Kind, user: &str, id: &str) -> Files {
    let mut out = Files::new();
    for r in learned_roots(kind) {
        if let Some(p) = root(kind, user, id, r) {
            let keep = |l: &str| *r != "home" || l == "home" || home_depth(&l["home/".len()..].split('/').collect::<Vec<_>>()).is_some();
            collect(&p, r, &keep, &mut out);
        }
    }
    out
}

/// Just before the game starts.
pub fn snapshot(user: &str, id: &str, kind: Kind) {
    let files = if learned_roots(kind).is_empty() { Files::new() } else { learn_walk(kind, user, id) };
    *SNAPSHOT.lock().unwrap() = Some((user.to_string(), id.to_string(), files));
}

/// After it ends: what it changed becomes its units.
fn learn(user: &str, id: &str, kind: Kind) {
    let Some((u, g, before)) = SNAPSHOT.lock().unwrap().take() else { return };
    if u != user || g != id || learned_roots(kind).is_empty() {
        return;
    }
    let after = learn_walk(kind, user, id);
    let changed = after.iter().filter(|(p, s)| before.get(*p) != Some(s)).map(|(p, _)| p).chain(before.keys().filter(|p| !after.contains_key(*p)));
    let found: BTreeSet<String> = changed.filter_map(|p| unit_of(p)).collect();
    if found.is_empty() {
        return;
    }
    with_player(user, |p| {
        let g = p.entry(id.to_string()).or_default();
        for u in found {
            if !g.units.contains(&u) {
                eprintln!("[saves] {id}: {u}");
                g.units.insert(u);
            }
        }
    });
}

// ---- remembered: per machine and per player ---------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Shared,
    Local,
}

#[derive(Serialize, Deserialize, Clone)]
struct Pair {
    peer: String,
    game: String,
}

#[derive(Serialize, Deserialize, Default)]
struct GameSet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<Mode>,
    /// the other machine's copy of a game that went from here to there
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pair: Option<Pair>,
}

#[derive(Serialize, Deserialize, Default)]
struct PlayerGame {
    #[serde(default)]
    units: BTreeSet<String>,
    /// peer id -> digest of the saves at the last sync with it
    #[serde(default)]
    synced: BTreeMap<String, String>,
}

static LOCK: Mutex<()> = Mutex::new(());

fn with_json<S: Serialize + for<'a> Deserialize<'a> + Default, T>(path: &Path, f: impl FnOnce(&mut S) -> T) -> T {
    let _g = LOCK.lock().unwrap();
    let mut s: S = fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let before = serde_json::to_string(&s).ok();
    let out = f(&mut s);
    let now = serde_json::to_string_pretty(&s).ok();
    if now.is_some() && serde_json::to_string(&s).ok() != before {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, now.unwrap()).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
    out
}

fn with_machine<T>(f: impl FnOnce(&mut BTreeMap<String, GameSet>) -> T) -> T {
    with_json(&data_dir().join("saves.json"), f)
}

fn with_player<T>(user: &str, f: impl FnOnce(&mut BTreeMap<String, PlayerGame>) -> T) -> T {
    with_json(&player_dir(user).join("saves.json"), f)
}

fn mode_of(id: &str) -> Option<Mode> {
    with_machine(|m| m.get(id).and_then(|g| g.mode))
}

/// Where the game's other copy is: the machine it came from, or the one it went to.
fn pair_of(id: &str) -> Option<Pair> {
    let v: Value = serde_json::from_str(&fs::read_to_string(crate::library::game_dir(id).join("game.json")).ok()?).ok()?;
    if let (Some(p), Some(g)) = (v["from"]["peer"].as_str(), v["from"]["game"].as_str()) {
        return Some(Pair { peer: p.into(), game: g.into() });
    }
    with_machine(|m| m.get(id).and_then(|g| g.pair.clone()))
}

fn set_pair(id: &str, pair: Option<Pair>) {
    with_machine(|m| {
        let g = m.entry(id.to_string()).or_default();
        g.pair = pair;
        if g.mode.is_none() && g.pair.is_none() {
            m.remove(id);
        }
    });
}

fn synced_with(user: &str, id: &str, peer: &str) -> Option<String> {
    with_player(user, |p| p.get(id).and_then(|g| g.synced.get(peer).cloned()))
}

fn record(user: &str, id: &str, peer: &str, units: &BTreeSet<String>, digest: &str) {
    with_player(user, |p| {
        let g = p.entry(id.to_string()).or_default();
        g.units.extend(units.iter().filter(|u| !matches!(u.as_str(), "wine" | "minecraft" | "dirs") && !u.starts_with("wii/")).cloned());
        g.synced.insert(peer.to_string(), digest.to_string());
    });
}

fn mods_on(user: &str, id: &str, m: &Manifest) -> Vec<String> {
    let mut v = crate::mods::enabled(user, id, &m.mods);
    v.sort();
    v
}

// ---- changing them ----------------------------------------------------------------

/// Keeps what is about to be replaced (the last sync's only).
fn keep_prev(kind: Kind, user: &str, id: &str, files: &Files) {
    let dir = player_dir(user).join("saves-prev").join(id);
    let _ = fs::remove_dir_all(&dir);
    for p in files.keys() {
        if let Some(src) = resolve(kind, user, id, p) {
            let dest = dir.join(p);
            if let Some(d) = dest.parent() {
                let _ = fs::create_dir_all(d);
            }
            let _ = fs::copy(&src, &dest);
        }
    }
}

fn set_mtime(path: &Path, ms: i64) -> std::io::Result<()> {
    let t = UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64);
    fs::File::options().write(true).open(path)?.set_modified(t)
}

/// Writes one file of the saves (whole, then in place).
fn write_file(dest: &Path, data: &[u8], mtime: i64) -> Result<(), String> {
    if let Some(d) = dest.parent() {
        fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = PathBuf::from(format!("{}.azototv-part", dest.display()));
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    set_mtime(&tmp, mtime).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

/// Removes a file of the saves, and the folders it leaves empty (inside its root).
fn remove_file(kind: Kind, user: &str, id: &str, logical: &str) {
    let Some(p) = resolve(kind, user, id, logical) else { return };
    let _ = fs::remove_file(&p);
    let root = resolve(kind, user, id, logical.split('/').next().unwrap_or(""));
    let mut d = p.parent();
    while let Some(dir) = d {
        if Some(dir) == root.as_deref() || fs::remove_dir(dir).is_err() {
            break;
        }
        d = dir.parent();
    }
}

// ---- the API the other AzotoTV calls ------------------------------------------------

#[derive(Clone)]
struct Ctx {
    app: AppHandle,
}

pub fn routes(app: AppHandle) -> Router {
    Router::new()
        .route("/api/peer/saves/pair", post(pair_in))
        .route("/api/peer/saves/state", post(state_in))
        .route("/api/peer/saves/begin", post(begin_in))
        .route("/api/peer/saves/file", get(file_get).put(file_put))
        .route("/api/peer/saves/finish", post(finish_in))
        .with_state(Ctx { app })
}

fn bad(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(serde_json::json!({ "error": msg.into() }))).into_response()
}

fn valid_user(u: &str) -> bool {
    !u.is_empty() && u.len() <= 64 && u.chars().all(|c| c.is_ascii_alphanumeric())
}

/// The game here and its manifest, for a request about it.
fn game_here(game: &str, user: &str) -> Result<Manifest, Response> {
    if !valid_user(user) || !crate::library::ids().iter().any(|g| g == game) {
        return Err(bad(StatusCode::NOT_FOUND, "no such game"));
    }
    match crate::games::read_manifest(game) {
        Ok(m) if m.kind != Kind::Steam => Ok(m),
        _ => Err(bad(StatusCode::NOT_FOUND, "no such game")),
    }
}

/// Saves aren't touched while their game runs here.
fn not_playing(ctx: &Ctx, game: &str) -> Result<(), Response> {
    let running = ctx.app.try_state::<Arc<crate::games::Games>>().and_then(|g| g.running());
    if running.as_deref() == Some(game) { Err(bad(StatusCode::CONFLICT, "playing")) } else { Ok(()) }
}

#[derive(Deserialize, Serialize)]
struct PairIn {
    /// this machine's game
    game: String,
    /// the copy over there
    theirs: String,
}

/// The other machine downloaded one of our games: where its copy is.
async fn pair_in(ConnectInfo(from): ConnectInfo<SocketAddr>, h: HeaderMap, Json(r): Json<PairIn>) -> Response {
    let p = match peer::caller(&h, from) {
        Ok(p) => p,
        Err(e) => return e,
    };
    if !crate::library::ids().contains(&r.game) {
        return bad(StatusCode::NOT_FOUND, "no such game");
    }
    set_pair(&r.game, Some(Pair { peer: p.id, game: r.theirs }));
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize, Serialize)]
struct StateIn {
    game: String,
    user: String,
    /// the caller's own id for it
    theirs: String,
    units: BTreeSet<String>,
}

#[derive(Deserialize, Serialize)]
struct StateOut {
    mode: Option<Mode>,
    mods: Vec<String>,
    units: BTreeSet<String>,
    /// over both sides' units
    files: Files,
    playing: bool,
}

async fn state_in(State(ctx): State<Ctx>, ConnectInfo(from): ConnectInfo<SocketAddr>, h: HeaderMap, Json(r): Json<StateIn>) -> Response {
    let p = match peer::caller(&h, from) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let m = match game_here(&r.game, &r.user) {
        Ok(m) => m,
        Err(e) => return e,
    };
    if pair_of(&r.game).is_none_or(|x| x.peer != p.id || x.game != r.theirs) {
        set_pair(&r.game, Some(Pair { peer: p.id.clone(), game: r.theirs.clone() }));
    }
    let mine = units(&r.user, &r.game, &m);
    let all: BTreeSet<String> = mine.iter().chain(r.units.iter()).filter(|u| resolve(m.kind, &r.user, &r.game, u).is_some()).cloned().collect();
    Json(StateOut {
        mode: mode_of(&r.game),
        mods: mods_on(&r.user, &r.game, &m),
        files: manifest(m.kind, &r.user, &r.game, &all),
        units: mine,
        playing: not_playing(&ctx, &r.game).is_err(),
    })
    .into_response()
}

#[derive(Deserialize, Serialize)]
struct BeginIn {
    game: String,
    user: String,
    units: BTreeSet<String>,
}

async fn begin_in(State(ctx): State<Ctx>, ConnectInfo(from): ConnectInfo<SocketAddr>, h: HeaderMap, Json(r): Json<BeginIn>) -> Response {
    if let Err(e) = peer::caller(&h, from) {
        return e;
    }
    let m = match game_here(&r.game, &r.user).and_then(|m| not_playing(&ctx, &r.game).map(|_| m)) {
        Ok(m) => m,
        Err(e) => return e,
    };
    keep_prev(m.kind, &r.user, &r.game, &manifest(m.kind, &r.user, &r.game, &r.units));
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize, Serialize)]
struct FileQ {
    game: String,
    user: String,
    path: String,
    #[serde(default)]
    mtime: i64,
}

async fn file_get(ConnectInfo(from): ConnectInfo<SocketAddr>, h: HeaderMap, Query(q): Query<FileQ>) -> Response {
    if let Err(e) = peer::caller(&h, from) {
        return e;
    }
    let m = match game_here(&q.game, &q.user) {
        Ok(m) => m,
        Err(e) => return e,
    };
    let Some(p) = resolve(m.kind, &q.user, &q.game, &q.path).filter(|p| p.symlink_metadata().is_ok_and(|md| md.is_file())) else {
        return bad(StatusCode::NOT_FOUND, "no such file");
    };
    match tokio::fs::File::open(&p).await {
        Ok(f) => Body::from_stream(tokio_util::io::ReaderStream::new(f)).into_response(),
        Err(e) => bad(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn file_put(State(ctx): State<Ctx>, ConnectInfo(from): ConnectInfo<SocketAddr>, h: HeaderMap, Query(q): Query<FileQ>, body: Body) -> Response {
    if let Err(e) = peer::caller(&h, from) {
        return e;
    }
    let m = match game_here(&q.game, &q.user).and_then(|m| not_playing(&ctx, &q.game).map(|_| m)) {
        Ok(m) => m,
        Err(e) => return e,
    };
    // a file inside a root, never the root itself
    let Some(dest) = resolve(m.kind, &q.user, &q.game, &q.path).filter(|_| q.path.contains('/')) else {
        return bad(StatusCode::BAD_REQUEST, "not a save");
    };
    let mut data = Vec::new();
    let mut s = body.into_data_stream();
    while let Some(c) = s.next().await {
        match c {
            Ok(c) => data.extend_from_slice(&c),
            Err(e) => return bad(StatusCode::BAD_REQUEST, e.to_string()),
        }
    }
    match write_file(&dest, &data, q.mtime) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => bad(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Deserialize, Serialize)]
struct FinishIn {
    game: String,
    user: String,
    units: BTreeSet<String>,
    #[serde(default)]
    delete: Vec<String>,
    /// what the saves are now, on both sides
    digest: String,
}

async fn finish_in(State(ctx): State<Ctx>, ConnectInfo(from): ConnectInfo<SocketAddr>, h: HeaderMap, Json(r): Json<FinishIn>) -> Response {
    let p = match peer::caller(&h, from) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let m = match game_here(&r.game, &r.user).and_then(|m| not_playing(&ctx, &r.game).map(|_| m)) {
        Ok(m) => m,
        Err(e) => return e,
    };
    for d in &r.delete {
        if d.contains('/') {
            remove_file(m.kind, &r.user, &r.game, d);
        }
    }
    let units: BTreeSet<String> = r.units.iter().filter(|u| resolve(m.kind, &r.user, &r.game, u).is_some()).cloned().collect();
    let now = digest(&manifest(m.kind, &r.user, &r.game, &units));
    if now != r.digest {
        return bad(StatusCode::CONFLICT, "the saves here aren't what was sent");
    }
    record(&r.user, &r.game, &p.id, &units, &now);
    StatusCode::NO_CONTENT.into_response()
}

// ---- syncing, from this side ---------------------------------------------------------

/// After a download from `p`: tells it where its game's copy is.
pub async fn tell_pair(p: &Peer, game: &str, here: &str) {
    let body = PairIn { game: game.to_string(), theirs: here.to_string() };
    let _ = peer::call(&peer::client(Duration::from_secs(5)), reqwest::Method::POST, p, "/api/peer/saves/pair").json(&body).send().await;
}

#[derive(Clone, Copy, PartialEq)]
pub enum When {
    Before,
    After,
}

/// What happened, for the toast.
enum Outcome {
    Nothing,
    Got,
    Gave,
    /// both had changed: the newest won (true: theirs)
    Both(bool),
}

/// Syncs this player's saves of game `id` with its other copy. Err: a message
/// for the player (the other machine is off, the mods differ…).
async fn sync(user: &str, id: &str, when: When) -> Result<(Outcome, String), String> {
    let none = || Ok((Outcome::Nothing, String::new()));
    let Ok(m) = crate::games::read_manifest(id) else { return none() };
    if m.kind == Kind::Steam || !valid_user(user) {
        return none();
    }
    let Some(pair) = pair_of(id) else { return none() };
    let mode = mode_of(id);
    if mode == Some(Mode::Local) {
        return none();
    }
    let Ok(p) = peer::peer(&pair.peer) else { return none() };
    let name = p.name.clone();
    let mine = units(user, id, &m);
    let c = peer::client(Duration::from_secs(8));
    let body = StateIn { game: pair.game.clone(), user: user.to_string(), theirs: id.to_string(), units: mine.clone() };
    let r = match peer::call(&c, reqwest::Method::POST, &p, "/api/peer/saves/state").json(&body).send().await {
        Ok(r) => r,
        Err(_) if when == When::Before => return Err(t!("{name} doesn't answer: you play with this computer's saves", name = name)),
        Err(_) => return Err(t!("{name} doesn't answer: it gets these saves next time", name = name)),
    };
    if r.status() == StatusCode::NOT_FOUND {
        // their copy is gone
        if pair_of(id).is_some_and(|x| x.peer == p.id) {
            set_pair(id, None);
        }
        return none();
    }
    let theirs: StateOut = r.error_for_status().map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
    if theirs.mode == Some(Mode::Local) {
        return none();
    }
    if theirs.playing {
        return Err(t!("{name} is playing it now: the saves stay apart this time", name = name));
    }
    let forced = mode == Some(Mode::Shared) || theirs.mode == Some(Mode::Shared);
    if !forced && theirs.mods != mods_on(user, id, &m) {
        return if when == When::Before { Err(t!("Different mods than on {name}: these saves stay here", name = name)) } else { none() };
    }

    let all: BTreeSet<String> = mine.union(&theirs.units).filter(|u| resolve(m.kind, user, id, u).is_some()).cloned().collect();
    let here = manifest(m.kind, user, id, &all);
    let (l, r) = (digest(&here), digest(&theirs.files));
    let last = synced_with(user, id, &p.id);
    let finish = |delete: Vec<String>, digest: String| FinishIn { game: pair.game.clone(), user: user.to_string(), units: all.clone(), delete, digest };
    if l == r {
        if last.as_deref() != Some(&l) {
            peer::call(&c, reqwest::Method::POST, &p, "/api/peer/saves/finish").json(&finish(vec![], l.clone())).send().await.ok();
            record(user, id, &p.id, &all, &l);
        }
        return none();
    }
    let (take, outcome) = if here.is_empty() {
        (true, Outcome::Got)
    } else if theirs.files.is_empty() {
        (false, Outcome::Gave)
    } else if last.as_deref() == Some(&l) {
        (true, Outcome::Got)
    } else if last.as_deref() == Some(&r) {
        (false, Outcome::Gave)
    } else {
        let t = newest(&theirs.files) > newest(&here);
        (t, Outcome::Both(t))
    };
    let big = peer::client(Duration::from_secs(600));
    let q = |path: &str, mtime: i64| {
        format!(
            "/api/peer/saves/file?game={}&user={}&path={}&mtime={mtime}",
            peer::urlencode(&pair.game),
            peer::urlencode(user),
            peer::urlencode(path)
        )
    };
    if take {
        keep_prev(m.kind, user, id, &here);
        for (path, s) in &theirs.files {
            if here.get(path) == Some(s) {
                continue;
            }
            let dest = resolve(m.kind, user, id, path).ok_or_else(|| format!("{path}: not a save"))?;
            let data = peer::call(&big, reqwest::Method::GET, &p, &q(path, 0))
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| e.to_string())?
                .bytes()
                .await
                .map_err(|e| e.to_string())?;
            write_file(&dest, &data, s.mtime)?;
        }
        for path in here.keys().filter(|k| !theirs.files.contains_key(*k)) {
            remove_file(m.kind, user, id, path);
        }
        let now = digest(&manifest(m.kind, user, id, &all));
        if now != r {
            return Err(t!("The saves from {name} didn't arrive whole", name = name));
        }
        peer::call(&c, reqwest::Method::POST, &p, "/api/peer/saves/finish").json(&finish(vec![], r.clone())).send().await.ok();
        record(user, id, &p.id, &all, &r);
    } else {
        let begin = BeginIn { game: pair.game.clone(), user: user.to_string(), units: all.clone() };
        peer::call(&c, reqwest::Method::POST, &p, "/api/peer/saves/begin").json(&begin).send().await.and_then(|r| r.error_for_status()).map_err(|e| e.to_string())?;
        for (path, s) in &here {
            if theirs.files.get(path) == Some(s) {
                continue;
            }
            let src = resolve(m.kind, user, id, path).ok_or_else(|| format!("{path}: not a save"))?;
            let data = fs::read(&src).map_err(|e| e.to_string())?;
            peer::call(&big, reqwest::Method::PUT, &p, &q(path, s.mtime)).body(data).send().await.and_then(|r| r.error_for_status()).map_err(|e| e.to_string())?;
        }
        let delete: Vec<String> = theirs.files.keys().filter(|k| !here.contains_key(*k)).cloned().collect();
        peer::call(&c, reqwest::Method::POST, &p, "/api/peer/saves/finish")
            .json(&finish(delete, l.clone()))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.to_string())?;
        record(user, id, &p.id, &all, &l);
    }
    Ok((outcome, name))
}

#[derive(Serialize, Clone)]
struct Toast {
    game: String,
    text: String,
}

async fn sync_and_tell(app: &AppHandle, user: &str, id: &str, when: When) {
    let text = match sync(user, id, when).await {
        Ok((Outcome::Nothing, _)) => return,
        Ok((Outcome::Got, name)) => t!("Saves from {name}", name = name),
        Ok((Outcome::Gave, name)) if when == When::After => t!("Saves sent to {name}", name = name),
        Ok((Outcome::Gave, _)) => return,
        Ok((Outcome::Both(true), name)) => t!("The saves had changed here and on {name}: {name}'s are newer and were kept", name = name),
        Ok((Outcome::Both(false), name)) => t!("The saves had changed here and on {name}: these are newer and were kept", name = name),
        Err(e) => e,
    };
    eprintln!("[saves] {id}: {text}");
    let _ = app.emit("saves-sync", Toast { game: id.to_string(), text });
}

/// Before a launch (blocking, from the launch thread): the other machine's
/// saves, when they are newer.
pub fn before(app: &AppHandle, user: &str, user_name: &str, id: &str, kind: Kind) {
    if kind == Kind::Switch {
        // the Switch profile has to exist for its saves to have a place
        let _ = crate::eden::profile_for(&player_dir(user), user_name);
    }
    tauri::async_runtime::block_on(sync_and_tell(app, user, id, When::Before));
}

/// After the game ended: learn what it wrote, then give the saves to the other machine.
pub fn after(app: &AppHandle, user: &str, id: &str, kind: Kind) {
    learn(user, id, kind);
    let (app, user, id) = (app.clone(), user.to_string(), id.to_string());
    tauri::async_runtime::spawn(async move { sync_and_tell(&app, &user, &id, When::After).await });
}

// ---- the game page ------------------------------------------------------------------

#[derive(Serialize)]
pub struct Info {
    /// the machine with the other copy
    peer_name: String,
    /// "shared", "local", or null: by itself (shared while the mods are the same)
    mode: Option<Mode>,
}

/// Whether game `id` has saves to share, and how.
pub fn info(id: &str) -> Option<Info> {
    let m = crate::games::read_manifest(id).ok()?;
    if m.kind == Kind::Steam {
        return None;
    }
    let p = peer::peer(&pair_of(id)?.peer).ok()?;
    Some(Info { peer_name: p.name, mode: mode_of(id) })
}

pub fn set_mode(id: &str, mode: Option<Mode>) -> Result<(), String> {
    if !crate::library::ids().iter().any(|g| g == id) {
        return Err(t!("Game not found"));
    }
    with_machine(|m| {
        let g = m.entry(id.to_string()).or_default();
        g.mode = mode;
        if g.mode.is_none() && g.pair.is_none() {
            m.remove(id);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_of_changed_files() {
        assert_eq!(unit_of("home/.local/share/Terraria/Players/a.plr").as_deref(), Some("home/.local/share/Terraria"));
        assert_eq!(unit_of("home/.config/unity3d/Studio/Game/prefs").as_deref(), Some("home/.config/unity3d/Studio/Game"));
        assert_eq!(unit_of("home/.local/share/godot/app_userdata/Game/save.dat").as_deref(), Some("home/.local/share/godot/app_userdata/Game"));
        assert_eq!(unit_of("home/.local/share/GSE Saves/433340/remote/s.sav").as_deref(), Some("home/.local/share/GSE Saves/433340"));
        assert_eq!(unit_of("home/.mygame/save.txt").as_deref(), Some("home/.mygame"));
        assert_eq!(unit_of("home/.config/game.ini").as_deref(), Some("home/.config/game.ini"));
        assert_eq!(unit_of("home/.cache/mesa/x"), None);
        assert_eq!(unit_of("home/.config/pulse/cookie"), None);
        assert_eq!(unit_of("switch/0100152000022000/save.bin").as_deref(), Some("switch/0100152000022000"));
        assert_eq!(unit_of("3ds/title/00040000/00053f00/data/00000001/x").as_deref(), Some("3ds/title/00040000/00053f00"));
        assert_eq!(unit_of("3ds/dbs/title.db"), None);
        assert_eq!(unit_of("wine/AppData/x"), None);
    }

    #[test]
    fn paths_stay_inside() {
        let k = Kind::Native;
        assert!(resolve(k, "u1", "g", "home/.local/share/x").is_some());
        assert!(resolve(k, "u1", "g", "home/../../etc/passwd").is_none());
        assert!(resolve(k, "u1", "g", "wine/x").is_none());
        assert!(resolve(Kind::Proton, "u1", "g", "wine/AppData/x").is_some_and(|p| p.ends_with("wine/g/drive_c/users/steamuser/AppData/x")));
        assert!(resolve(Kind::Steam, "u1", "g", "dirs/x").is_none());
    }

    #[test]
    fn caches_left_out() {
        let d = std::env::temp_dir().join(format!("azoto-saves-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("AppData/Local/Temp")).unwrap();
        fs::create_dir_all(d.join("Saved Games/G")).unwrap();
        fs::write(d.join("AppData/Local/Temp/x"), "t").unwrap();
        fs::write(d.join("Saved Games/G/slot1"), "s").unwrap();
        std::os::unix::fs::symlink(d.join("Saved Games/G/slot1"), d.join("link")).unwrap();
        let mut f = Files::new();
        collect(&d, "wine", &|_| true, &mut f);
        assert_eq!(f.keys().collect::<Vec<_>>(), ["wine/Saved Games/G/slot1"]);
        fs::remove_dir_all(&d).unwrap();
    }
}
