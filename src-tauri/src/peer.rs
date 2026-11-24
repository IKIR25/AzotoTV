//! Linked AzotoTVs: two installs (the TV's server and a computer, say) see
//! each other's games and download them, in both directions, so a game
//! plays on whichever machine is in front of you, with its own hardware.
//!
//! Linking: START › Linked AzotoTVs on one of them, as a Jellyfin
//! administrator. The other AzotoTV is the one next to Jellyfin (found by
//! itself) or an address typed in. Linking proves itself with Jellyfin: the
//! other side asks its own Jellyfin whether the token it got belongs to an
//! administrator, so both must use the same Jellyfin and nobody types a code.
//! Both then keep the same secret (`~/.local/share/azototv/peers.json`, mode
//! 600) and every request carries it.
//!
//! The API is on the phones' port (phone.rs, 8686), under `/api/peer/`:
//! `hello` (who is this, no secret), `link`, `unlink`, `games` (what can be
//! downloaded), `art/<game>/<kind>`, `files/<game>` and `file/<game>/<path>`
//! (with Range, so a download picks up where it stopped), and `pack/<game>`
//! (many small files in one answer: a folder of thousands of 20 KB files
//! would otherwise cost a request each, and crawl).
//!
//! What goes: a game's whole folder (symbolic links to somewhere else left
//! out: those are the players' saves, see `games::link_player_dirs`; links
//! inside the folder go as links), plus its executable or ROM
//! when it lives elsewhere (it lands in `ext/` and the `game.json` that goes
//! is changed to point there). Steam games and Minecraft stay where they are.
//! The new folder's `game.json` says where it came from (`"from"`).
//! Never: logs, build leftovers, `*.prev` folders, and what the game's
//! `"peer": {"skip": [...]}` lists (paths in its folder). Caches, and what
//! `"peer": {"local": [...]}` lists, go once and then stay each machine's own.
//!
//! Settings that name the other machine's game folder by its full path (a
//! recomp's `dvd_root = "/home/…/Games/x/DATA"`) name this machine's once
//! they're here: the file list says where the folder is there (header
//! `x-azoto-dir`), and the small files that came get that path replaced.
//! A game can also prepare itself for the machine it came to, after the
//! download and after each update (`"peer": {"prepare": "game/x.sh"}`, run in
//! its folder with its `env`): shaders for this graphics card, say.
//!
//! Updates: the folder keeps a record of what came (`.azototv-from.json`,
//! each file's size and time there). When the other side's copy changes (an
//! update, a new mod), the game page offers Update, which fetches only the
//! files that changed there, removes what it no longer has, and leaves alone
//! what changed only here.

use std::{
    collections::HashMap,
    fs,
    io::Read,
    net::SocketAddr,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, Path as UrlPath, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::games::{Kind, data_dir};
use crate::jellyfin::Jf;

// ---- who is linked --------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
pub struct Peer {
    pub id: String,
    pub name: String,
    /// "http://192.168.1.10:8686"
    pub url: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    secret: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Store {
    /// this install's own id
    id: String,
    #[serde(default)]
    peers: Vec<Peer>,
}

fn store_path() -> PathBuf {
    data_dir().join("peers.json")
}

fn random_hex(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    let _ = fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    b.iter().map(|x| format!("{x:02x}")).collect()
}

static STORE: Mutex<Option<Store>> = Mutex::new(None);

fn with_store<T>(f: impl FnOnce(&mut Store) -> T) -> T {
    let mut g = STORE.lock().unwrap();
    let s = g.get_or_insert_with(|| {
        let mut s: Store = fs::read_to_string(store_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if s.id.is_empty() {
            s.id = random_hex(8);
        }
        s
    });
    let before = serde_json::to_value(&*s).ok();
    let out = f(s);
    if serde_json::to_value(&*s).ok() != before || !store_path().exists() {
        if let Ok(v) = serde_json::to_value(&*s) {
            let _ = crate::services::write(&store_path(), &v);
        }
    }
    out
}

fn my_id() -> String {
    with_store(|s| s.id.clone())
}

/// This machine's name, as the other AzotoTV shows it.
fn my_name() -> String {
    let mut buf = [0u8; 256];
    let n = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    let host = if n == 0 { String::from_utf8_lossy(&buf[..buf.iter().position(|b| *b == 0).unwrap_or(0)]).to_string() } else { String::new() };
    if host.is_empty() { "AzotoTV".into() } else { host }
}

pub(crate) fn peer(id: &str) -> Result<Peer, String> {
    with_store(|s| s.peers.iter().find(|p| p.id == id).cloned()).ok_or_else(|| t!("That AzotoTV isn't linked any more"))
}

// ---- the API other AzotoTVs call ---------------------------------------------------

#[derive(Clone)]
struct Ctx {
    app: AppHandle,
    /// file lists, kept a while: a download asks for each file by its path
    lists: Arc<Mutex<HashMap<String, (Instant, Arc<Vec<Entry>>)>>>,
}

pub fn routes(app: AppHandle) -> Router {
    Router::new()
        .route("/api/peer/hello", get(hello))
        .route("/api/peer/link", post(link_in))
        .route("/api/peer/unlink", post(unlink_in))
        .route("/api/peer/games", get(games_out))
        .route("/api/peer/art/{game}/{kind}", get(art_out))
        .route("/api/peer/files/{game}", get(files_out))
        .route("/api/peer/file/{game}/{*path}", get(file_out))
        .route("/api/peer/pack/{game}", post(pack_out))
        .with_state(Ctx { app: app.clone(), lists: Default::default() })
        .merge(crate::saves::routes(app))
}

fn err(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(json!({ "error": msg.into() }))).into_response()
}

/// The calling peer, when its secret is right. Its address is kept up to
/// date (a computer's changes now and then).
pub(crate) fn caller(h: &HeaderMap, from: SocketAddr) -> Result<Peer, Response> {
    let id = h.get("x-azoto-peer").and_then(|v| v.to_str().ok()).unwrap_or("");
    let secret = h.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    with_store(|s| {
        let p = s.peers.iter_mut().find(|p| p.id == id && !secret.is_empty() && p.secret == secret)?;
        if let Ok(mut u) = reqwest::Url::parse(&p.url) {
            let ip = from.ip().to_canonical();
            if u.host_str() != Some(&ip.to_string()) && u.set_ip_host(ip).is_ok() {
                p.url = u.as_str().trim_end_matches('/').to_string();
            }
        }
        Some(p.clone())
    })
    .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "not linked"))
}

async fn hello() -> Json<Value> {
    Json(json!({ "app": "AzotoTV", "id": my_id(), "name": my_name(), "version": env!("CARGO_PKG_VERSION") }))
}

#[derive(Deserialize)]
struct LinkIn {
    id: String,
    name: String,
    port: u16,
    secret: String,
    /// a Jellyfin token of the one linking: it has to be an administrator's
    token: String,
}

async fn link_in(State(ctx): State<Ctx>, ConnectInfo(from): ConnectInfo<SocketAddr>, Json(r): Json<LinkIn>) -> Response {
    if r.secret.len() < 32 || r.id.is_empty() || r.id == my_id() {
        return err(StatusCode::BAD_REQUEST, "bad link request");
    }
    let jf = ctx.app.state::<Arc<Jf>>().inner().clone();
    match jf.is_admin_token(&r.token).await {
        Ok(true) => {}
        Ok(false) => return err(StatusCode::FORBIDDEN, t!("Only a Jellyfin administrator can link AzotoTVs")),
        Err(e) => return err(StatusCode::FORBIDDEN, t!("This AzotoTV's Jellyfin doesn't know that profile: both have to use the same Jellyfin ({e})", e = e)),
    }
    let ip = from.ip().to_canonical();
    let url = if ip.is_ipv6() { format!("http://[{ip}]:{}", r.port) } else { format!("http://{ip}:{}", r.port) };
    let name = r.name.chars().take(60).collect::<String>();
    eprintln!("[peer] linked with {name} ({url})");
    with_store(|s| {
        s.peers.retain(|p| p.id != r.id);
        s.peers.push(Peer { id: r.id, name, url, secret: r.secret });
    });
    let _ = ctx.app.emit("peers-changed", ());
    Json(json!({ "id": my_id(), "name": my_name() })).into_response()
}

async fn unlink_in(State(ctx): State<Ctx>, ConnectInfo(from): ConnectInfo<SocketAddr>, h: HeaderMap) -> Response {
    let p = match caller(&h, from) {
        Ok(p) => p,
        Err(r) => return r,
    };
    with_store(|s| s.peers.retain(|x| x.id != p.id));
    eprintln!("[peer] {} unlinked itself", p.name);
    let _ = ctx.app.emit("peers-changed", ());
    StatusCode::NO_CONTENT.into_response()
}

/// A game another AzotoTV can download.
#[derive(Serialize, Deserialize, Clone)]
pub struct Offer {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: Kind,
    pub platform: Option<String>,
    pub steam_appid: Option<u32>,
    pub description: String,
    pub year: Option<String>,
    pub genres: Vec<String>,
    pub art: Vec<String>,
    pub art_stamp: u64,
    pub players: u32,
    /// bytes: the game (with its mods, updates, DLC)
    pub size: u64,
    /// bytes: the emulator and what it needs, for a machine that has none
    #[serde(default)]
    pub runner_size: u64,
}

async fn games_out(State(ctx): State<Ctx>, ConnectInfo(from): ConnectInfo<SocketAddr>, h: HeaderMap) -> Response {
    if let Err(r) = caller(&h, from) {
        return r;
    }
    let lists = ctx.lists.clone();
    let offers = tauri::async_runtime::spawn_blocking(move || {
        crate::games::list("")
            .into_iter()
            .filter_map(|g| {
                let files = cached_files(&lists, &g.id).ok()?;
                Some(Offer {
                    size: files.iter().filter(|e| e.runner.is_none()).map(|e| e.size).sum(),
                    runner_size: files.iter().filter(|e| e.runner.is_some()).map(|e| e.size).sum(),
                    id: g.id,
                    name: g.name,
                    kind: g.kind,
                    platform: g.platform,
                    steam_appid: g.steam_appid,
                    description: g.description,
                    year: g.year,
                    genres: g.genres,
                    art: g.art.iter().map(|s| s.to_string()).collect(),
                    art_stamp: g.art_stamp,
                    players: g.players,
                })
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    Json(offers).into_response()
}

async fn art_out(ConnectInfo(from): ConnectInfo<SocketAddr>, h: HeaderMap, UrlPath((game, kind)): UrlPath<(String, String)>) -> Response {
    if let Err(r) = caller(&h, from) {
        return r;
    }
    match crate::games::art(&format!("/{game}/{kind}")) {
        Ok((data, ct)) => ([(header::CONTENT_TYPE, ct)], data).into_response(),
        Err(e) => err(StatusCode::NOT_FOUND, e),
    }
}

/// One file of a game as it goes: where it lands (relative to the new game
/// folder) and where it is here.
#[derive(Clone)]
struct Entry {
    /// relative to the new game folder, or `~/…` (relative to the home:
    /// an emulator, its keys, a Switch game's updates)
    path: String,
    src: Source,
    size: u64,
    exec: bool,
    /// part of an emulator rather than of the game ("eden", "proton"…)
    runner: Option<&'static str>,
    /// last changed (ms since the epoch), for a file
    mtime: Option<u64>,
    /// comes with the download, then each machine keeps its own (a shader
    /// cache, settings): never part of an update
    once: bool,
}

#[derive(Clone)]
enum Source {
    File(PathBuf),
    /// the changed game.json
    Bytes(Arc<Vec<u8>>),
    /// a symbolic link to something in the same game folder: its target,
    /// relative to the link's own folder (so it still works after the move)
    Link(String),
}

#[derive(Serialize, Deserialize)]
struct FileInfo {
    path: String,
    size: u64,
    #[serde(default)]
    exec: bool,
    /// a link to make instead of a file to fetch
    #[serde(default, skip_serializing_if = "Option::is_none")]
    link: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    runner: Option<String>,
    /// this version of the file: same stamp, same file (see [`Entry::stamp`])
    #[serde(default)]
    stamp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mtime: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    once: bool,
}

impl Entry {
    fn stamp(&self) -> String {
        match &self.src {
            Source::File(_) => format!("{}-{}", self.size, self.mtime.unwrap_or(0)),
            Source::Bytes(b) => {
                use md5::{Digest, Md5};
                format!("{:x}", Md5::digest(b.as_slice()))
            }
            Source::Link(t) => format!("link:{t}"),
        }
    }
}

fn mtime_ms(m: &fs::Metadata) -> Option<u64> {
    m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// Folders that never go to the other machine: logs, build leftovers, the
/// copy kept from before an update.
fn never_goes(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(), "log" | "logs" | "crash_dumps" | "zig-cache" | ".zig-cache" | "art.old") || name.ends_with(".prev")
}

/// Folders that go once and then stay each machine's own: caches change with every game played.
fn goes_once(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(), "cache" | "shader_cache" | "shadercache" | "pipeline_cache")
}

/// `rel` is `p` or inside it ("data/x" is in "data", not in "dat").
fn under(rel: &str, p: &str) -> bool {
    let p = p.trim_end_matches('/');
    !p.is_empty() && (rel == p || rel.strip_prefix(p).is_some_and(|r| r.starts_with('/')))
}

fn cached_files(lists: &Mutex<HashMap<String, (Instant, Arc<Vec<Entry>>)>>, game: &str) -> Result<Arc<Vec<Entry>>, String> {
    const KEEP: Duration = Duration::from_secs(600);
    if let Some((t, l)) = lists.lock().unwrap().get(game) {
        if t.elapsed() < KEEP {
            return Ok(l.clone());
        }
    }
    let l = Arc::new(files(game)?);
    lists.lock().unwrap().insert(game.to_string(), (Instant::now(), l.clone()));
    Ok(l)
}

/// Everything that goes with a game, or why it can't go.
fn files(game: &str) -> Result<Vec<Entry>, String> {
    let dir = crate::library::game_dir(game);
    let text = fs::read_to_string(dir.join("game.json")).map_err(|e| e.to_string())?;
    let mut m: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let kind: Kind = serde_json::from_value(m["type"].clone()).map_err(|e| e.to_string())?;
    if matches!(kind, Kind::Steam | Kind::Minecraft) {
        return Err("stays on its machine".into());
    }
    let mut out = Vec::new();
    walk(&dir, &dir, &mut out);
    let orig_exe = m["exe"].as_str().filter(|e| !e.is_empty()).map(|e| if e.starts_with('/') || e.starts_with("~/") { crate::games::expand_home(e) } else { dir.join(e) });
    // mods (and their loader) that are switched on sit where the emulator
    // finds them, not in the game folder (mods.rs moves them): they go from
    // there, to their place in the folder
    let mut taken: Vec<PathBuf> = Vec::new();
    let places = m["mods"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|md| md["files"].as_array().into_iter().flatten())
        .chain(m["mod_loader"].as_array().into_iter().flatten());
    for pl in places {
        let (Some(stored), Some(active)) = (pl["stored"].as_str(), pl["active"].as_str()) else { continue };
        let stored = stored.trim_end_matches('/');
        if !Path::new(stored).components().all(|c| matches!(c, Component::Normal(_))) || dir.join(stored).exists() {
            continue;
        }
        let a = crate::games::expand_home(active);
        if a.exists() {
            taken.extend(fs::canonicalize(&a));
            add_tree(&a, stored, None, &mut out);
        }
    }
    if kind == Kind::Switch {
        if let Some(exe) = &orig_exe {
            switch_extras(exe, &taken, &mut out);
        }
    }
    runner_files(kind, orig_exe.as_deref(), &mut out);
    // an executable or ROM elsewhere goes into ext/, and the game.json that goes points there
    let ext = |m: &mut Value, out: &mut Vec<Entry>| -> Result<(), String> {
        let Some(e) = m["exe"].as_str().filter(|e| !e.is_empty()) else { return Ok(()) };
        let p = if e.starts_with('/') || e.starts_with("~/") { crate::games::expand_home(e) } else { return Ok(()) };
        if p.starts_with(&dir) {
            m["exe"] = json!(p.strip_prefix(&dir).unwrap_or(&p).to_string_lossy());
            return Ok(());
        }
        let meta = fs::metadata(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        if !meta.is_file() {
            return Err("its executable is a folder elsewhere".into());
        }
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "game".into());
        let rel = format!("ext/{name}");
        if !out.iter().any(|x: &Entry| x.path == rel) {
            out.push(Entry { path: rel.clone(), size: meta.len(), exec: is_exec(&meta), src: Source::File(p), runner: None, mtime: mtime_ms(&meta), once: false });
        }
        m["exe"] = json!(rel);
        Ok(())
    };
    ext(&mut m, &mut out)?;
    // (get_mut: indexing would add "chapters": null to a game without them)
    if let Some(chapters) = m.get_mut("chapters").and_then(Value::as_array_mut) {
        for c in chapters {
            ext(c, &mut out)?;
        }
    }
    if let Some(o) = m.as_object_mut() {
        o.remove("added");
        o.insert("from".into(), json!({ "peer": my_id(), "game": game }));
    }
    // what stays here (`"peer": {"skip": [...]}`, paths in the game folder: an
    // archive the game was made from) and what each machine keeps its own
    // after the first download (`"local"`: settings the game rewrites)
    let list = |k: &str| -> Vec<String> { m["peer"][k].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect() };
    let (skip, local) = (list("skip"), list("local"));
    out.retain(|e| {
        let names = || e.path.trim_start_matches("~/").split('/');
        e.path != "game.json" && !skip.iter().any(|p| under(&e.path, p)) && !names().any(never_goes)
    });
    for e in &mut out {
        let names = || e.path.trim_start_matches("~/").split('/');
        e.once = e.runner.is_none() && (local.iter().any(|p| under(&e.path, p)) || names().any(goes_once));
    }
    let bytes = Arc::new(serde_json::to_string_pretty(&m).unwrap_or_default().into_bytes());
    // last: a download or update that stops halfway leaves the old one
    out.push(Entry { path: "game.json".into(), size: bytes.len() as u64, exec: false, src: Source::Bytes(bytes), runner: None, mtime: None, once: false });
    Ok(out)
}

/// A file or a whole folder, landing at `dest` (relative to the game folder, or `~/…`).
fn add_tree(src: &Path, dest: &str, runner: Option<&'static str>, out: &mut Vec<Entry>) {
    let Ok(meta) = fs::metadata(src) else { return };
    if meta.is_file() {
        out.push(Entry { path: dest.to_string(), size: meta.len(), exec: is_exec(&meta), src: Source::File(src.to_path_buf()), runner, mtime: mtime_ms(&meta), once: false });
        return;
    }
    let mut inner = Vec::new();
    walk(src, src, &mut inner);
    out.extend(inner.into_iter().map(|e| Entry { path: format!("{dest}/{}", e.path), runner, ..e }));
}

/// A Switch game's updates and DLC: what's in `~/Eden/Extra` (Eden's
/// external content folder) under the game's name ("Mario Kart 8 Deluxe
/// [0100…][v…][Update].nsp", a folder "Tomodachi Life" for "Tomodachi Life
/// Living the Dream.nsp"). They land in the same folder on the other side.
fn switch_extras(exe: &Path, taken: &[PathBuf], out: &mut Vec<Entry>) {
    let Some(stem) = exe.file_stem().map(|s| s.to_string_lossy().to_string()) else { return };
    let base = stem.split(" [").next().unwrap_or(&stem).trim().to_lowercase();
    let extra = crate::games::home().join("Eden/Extra");
    for e in fs::read_dir(&extra).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let is_dir = e.path().is_dir();
        let n = if is_dir { name.to_lowercase() } else { Path::new(&name).file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default() };
        let ours = n.starts_with(&base) || (is_dir && n.len() >= 6 && base.starts_with(&n));
        let canon = fs::canonicalize(e.path()).unwrap_or(e.path());
        if ours && !taken.iter().any(|t| canon.starts_with(t) || t.starts_with(&canon)) {
            add_tree(&e.path(), &format!("~/Eden/Extra/{name}"), None, out);
        }
    }
}

/// What a kind of game needs besides itself, when the other machine lacks it
/// (it skips whatever it has): Eden with the keys and firmware; Dolphin's HD
/// texture packs and settings for this disc; Azahar's 3DS template; a Proton.
fn runner_files(kind: Kind, exe: Option<&Path>, out: &mut Vec<Entry>) {
    match kind {
        Kind::Switch => {
            let data = crate::eden::eden_data();
            add_tree(&crate::eden::appimage(), "~/Eden/Eden.appimage", Some("eden"), out);
            add_tree(&data.join("keys"), "~/.local/share/eden/keys", Some("eden"), out);
            add_tree(&data.join("nand/system/Contents/registered"), "~/.local/share/eden/nand/system/Contents/registered", Some("eden"), out);
        }
        Kind::Wii => {
            let Some(id) = exe.and_then(crate::library::disc_id) else { return };
            let user = data_dir().join("dolphin");
            // packs and settings are named by the disc's ID or its first 3 letters
            let fits = |name: &str| name.len() >= 3 && id.starts_with(name.split('.').next().unwrap_or(name));
            for sub in ["Load/Textures", "GameSettings"] {
                for e in fs::read_dir(user.join(sub)).into_iter().flatten().flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if fits(&name) {
                        add_tree(&e.path(), &format!("~/.local/share/azototv/dolphin/{sub}/{name}"), None, out);
                    }
                }
            }
        }
        Kind::N3ds => add_tree(&data_dir().join("azahar"), "~/.local/share/azototv/azahar", Some("azahar"), out),
        Kind::Proton => {
            if let Some(p) = crate::games::proton_dir() {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "proton".into());
                add_tree(&p, &format!("~/.local/share/Steam/compatibilitytools.d/{name}"), Some("proton"), out);
            }
        }
        _ => {}
    }
}

fn is_exec(m: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    m.permissions().mode() & 0o111 != 0
}

/// `to` as seen from the folder `from` ("../../game/x"); both inside the same root.
fn relative(from: &Path, to: &Path) -> PathBuf {
    let (f, t): (Vec<_>, Vec<_>) = (from.components().collect(), to.components().collect());
    let common = f.iter().zip(&t).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..f.len() {
        out.push("..");
    }
    for c in &t[common..] {
        out.push(c);
    }
    out
}

/// The files under `dir`. Symbolic links come along when they point at a
/// file or folder inside the game folder (data shared into `home-seed/`);
/// the others (a player's saves, see `games::link_player_dirs`) stay here.
fn walk(root: &Path, dir: &Path, out: &mut Vec<Entry>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        // what AzotoTV itself keeps in there (where a game came from, a file half downloaded)
        if e.file_name().to_string_lossy().starts_with(".azototv-") {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(e.path()) else { continue };
        if meta.is_symlink() {
            let (Ok(target), Ok(root_c)) = (fs::canonicalize(e.path()), fs::canonicalize(root)) else { continue };
            let Ok(rel) = e.path().strip_prefix(root).map(Path::to_path_buf) else { continue };
            if (target.is_file() || target.is_dir()) && target.starts_with(&root_c) {
                let inside = target.strip_prefix(&root_c).unwrap_or(&target);
                let link = relative(rel.parent().unwrap_or(Path::new("")), inside);
                out.push(Entry {
                    path: rel.to_string_lossy().to_string(),
                    size: 0,
                    exec: false,
                    src: Source::Link(link.to_string_lossy().to_string()),
                    runner: None,
                    mtime: None,
                    once: false,
                });
            }
        } else if meta.is_dir() {
            walk(root, &e.path(), out);
        } else if meta.is_file() {
            let rel = e.path().strip_prefix(root).map(|r| r.to_string_lossy().to_string()).unwrap_or_default();
            if !rel.is_empty() && !rel.ends_with(".log") {
                out.push(Entry { path: rel, size: meta.len(), exec: is_exec(&meta), src: Source::File(e.path()), runner: None, mtime: mtime_ms(&meta), once: false });
            }
        }
    }
}

async fn files_out(State(ctx): State<Ctx>, ConnectInfo(from): ConnectInfo<SocketAddr>, h: HeaderMap, UrlPath(game): UrlPath<String>) -> Response {
    if let Err(r) = caller(&h, from) {
        return r;
    }
    let lists = ctx.lists.clone();
    let g = game.clone();
    match tauri::async_runtime::spawn_blocking(move || {
        // a new download: the list as it is now
        lists.lock().unwrap().remove(&g);
        cached_files(&lists, &g)
    })
    .await
    {
        Ok(Ok(l)) => {
            // where the game's folder is here: settings that name it by its full
            // path name the other machine's instead, once they're there
            let dir = HeaderValue::from_bytes(crate::library::game_dir(&game).as_os_str().as_encoded_bytes()).unwrap_or(HeaderValue::from_static(""));
            let list = l
                .iter()
                .map(|e| FileInfo {
                    path: e.path.clone(),
                    size: e.size,
                    exec: e.exec,
                    link: match &e.src {
                        Source::Link(t) => Some(t.clone()),
                        _ => None,
                    },
                    runner: e.runner.map(str::to_string),
                    stamp: e.stamp(),
                    mtime: e.mtime,
                    once: e.once,
                })
                .collect::<Vec<_>>();
            ([(HeaderName::from_static(DIR_HEADER), dir)], Json(list)).into_response()
        }
        Ok(Err(e)) => err(StatusCode::NOT_FOUND, e),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn file_out(
    State(ctx): State<Ctx>,
    ConnectInfo(from): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    UrlPath((game, path)): UrlPath<(String, String)>,
) -> Response {
    if let Err(r) = caller(&h, from) {
        return r;
    }
    let lists = ctx.lists.clone();
    let g = game.clone();
    let list = match tauri::async_runtime::spawn_blocking(move || cached_files(&lists, &g)).await {
        Ok(Ok(l)) => l,
        Ok(Err(e)) => return err(StatusCode::NOT_FOUND, e),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    // only what the list has: no path from outside ever reaches the disk
    let Some(e) = list.iter().find(|e| e.path == path) else { return err(StatusCode::NOT_FOUND, "no such file") };
    let start: u64 = h
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| v.strip_suffix('-'))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .min(e.size);
    let status = if start > 0 { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK };
    let body = match &e.src {
        Source::Link(_) => return err(StatusCode::NOT_FOUND, "a link, not a file"),
        Source::Bytes(b) => Body::from(b[start as usize..].to_vec()),
        Source::File(p) => {
            use tokio::io::AsyncSeekExt;
            let mut f = match tokio::fs::File::open(p).await {
                Ok(f) => f,
                Err(x) => return err(StatusCode::NOT_FOUND, x.to_string()),
            };
            if start > 0 && f.seek(std::io::SeekFrom::Start(start)).await.is_err() {
                return err(StatusCode::INTERNAL_SERVER_ERROR, "seek");
            }
            Body::from_stream(tokio_util::io::ReaderStream::with_capacity(f, 1 << 20))
        }
    };
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, e.size - start)
        .body(body)
        .unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "response"))
}

/// Small files go many to an answer (see [`pack_out`]); bigger ones on their own.
const PACK_SMALL: u64 = 1 << 20;
/// At most this many files, or this many bytes, in one answer.
const PACK_FILES: usize = 2000;
const PACK_BYTES: u64 = 32 << 20;
/// In a pack, the length that says "not this one": the client then asks for it on its own.
const PACK_NONE: u64 = u64::MAX;

/// The files asked for (a JSON list of paths from `files/<game>`), one after
/// the other, each as its length (8 bytes, little-endian) and then its bytes.
/// One the list doesn't have, a link, a big one or one that can't be read:
/// [`PACK_NONE`] and no bytes.
async fn pack_out(
    State(ctx): State<Ctx>,
    ConnectInfo(from): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    UrlPath(game): UrlPath<String>,
    Json(paths): Json<Vec<String>>,
) -> Response {
    if let Err(r) = caller(&h, from) {
        return r;
    }
    if paths.len() > PACK_FILES {
        return err(StatusCode::BAD_REQUEST, "too many files");
    }
    let lists = ctx.lists.clone();
    let g = game.clone();
    let list = match tauri::async_runtime::spawn_blocking(move || cached_files(&lists, &g)).await {
        Ok(Ok(l)) => l,
        Ok(Err(e)) => return err(StatusCode::NOT_FOUND, e),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    // only what the list has: no path from outside ever reaches the disk
    let by_path: HashMap<&str, &Entry> = list.iter().map(|e| (e.path.as_str(), e)).collect();
    let srcs: Vec<Option<Source>> = paths.iter().map(|p| by_path.get(p.as_str()).filter(|e| e.size <= PACK_SMALL).map(|e| e.src.clone())).collect();
    let stream = futures_util::stream::unfold(srcs.into_iter(), |mut it| async move {
        let src = it.next()?;
        let bytes = match src {
            Some(Source::File(p)) => tokio::fs::read(&p).await.ok().filter(|b| b.len() as u64 <= PACK_SMALL),
            Some(Source::Bytes(b)) => Some(b.to_vec()),
            Some(Source::Link(_)) | None => None,
        };
        let mut out = Vec::with_capacity(8 + bytes.as_ref().map_or(0, Vec::len));
        match bytes {
            Some(b) => {
                out.extend_from_slice(&(b.len() as u64).to_le_bytes());
                out.extend_from_slice(&b);
            }
            None => out.extend_from_slice(&PACK_NONE.to_le_bytes()),
        }
        Some((Ok::<_, std::io::Error>(out), it))
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "response"))
}

// ---- calling the others -------------------------------------------------------------------

pub(crate) fn client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder().connect_timeout(Duration::from_secs(3)).timeout(timeout).build().expect("http client")
}

fn authed(c: &reqwest::Client, p: &Peer, path: &str) -> reqwest::RequestBuilder {
    call(c, reqwest::Method::GET, p, path)
}

pub(crate) fn call(c: &reqwest::Client, method: reqwest::Method, p: &Peer, path: &str) -> reqwest::RequestBuilder {
    c.request(method, format!("{}{path}", p.url)).header("x-azoto-peer", my_id()).bearer_auth(&p.secret)
}

/// "192.168.1.5" / "http://host:8686/" -> "http://192.168.1.5:8686"
fn norm_url(s: &str) -> String {
    let s = s.trim().trim_end_matches('/');
    let s = if s.contains("://") { s.to_string() } else { format!("http://{s}") };
    match reqwest::Url::parse(&s) {
        Ok(mut u) => {
            if u.port().is_none() {
                let _ = u.set_port(Some(crate::phone::port()));
            }
            u.as_str().trim_end_matches('/').to_string()
        }
        Err(_) => s,
    }
}

#[derive(Serialize)]
pub struct Found {
    pub url: String,
    pub id: String,
    pub name: String,
    pub linked: bool,
}

async fn hello_at(url: &str) -> Result<Found, String> {
    let v: Value = client(Duration::from_secs(4))
        .get(format!("{url}/api/peer/hello"))
        .send()
        .await
        .map_err(|_| t!("No AzotoTV answers at {url}", url = url))?
        .json()
        .await
        .map_err(|_| t!("That isn't an AzotoTV, or it's too old to link"))?;
    if v["app"] != "AzotoTV" {
        return Err(t!("That isn't an AzotoTV, or it's too old to link"));
    }
    let id = v["id"].as_str().unwrap_or("").to_string();
    Ok(Found { url: url.into(), linked: with_store(|s| s.peers.iter().any(|p| p.id == id)), name: v["name"].as_str().unwrap_or("AzotoTV").into(), id })
}

/// The AzotoTV next to Jellyfin, when there's one and it isn't this one.
pub async fn find(jf: &Jf) -> Option<Found> {
    let base = jf.server()?;
    let host = reqwest::Url::parse(&base).ok()?.host_str()?.to_string();
    let f = hello_at(&norm_url(&host)).await.ok()?;
    (f.id != my_id()).then_some(f)
}

/// Links this AzotoTV with the one at `url` (both ways).
pub async fn link(jf: &Jf, url: &str) -> Result<Peer, String> {
    crate::services::admin(jf).await?;
    let url = norm_url(url);
    let there = hello_at(&url).await?;
    if there.id == my_id() {
        return Err(t!("That's this AzotoTV"));
    }
    let token = jf.session().ok_or("not logged in")?.token;
    let secret = random_hex(32);
    let resp = client(Duration::from_secs(15))
        .post(format!("{url}/api/peer/link"))
        .json(&json!({ "id": my_id(), "name": my_name(), "port": crate::phone::port(), "secret": secret, "token": token }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let ok = resp.status().is_success();
    let v: Value = resp.json().await.unwrap_or_default();
    if !ok {
        return Err(v["error"].as_str().unwrap_or("link refused").to_string());
    }
    let p = Peer { id: there.id, name: v["name"].as_str().unwrap_or(&there.name).to_string(), url, secret };
    with_store(|s| {
        s.peers.retain(|x| x.id != p.id);
        s.peers.push(p.clone());
    });
    eprintln!("[peer] linked with {} ({})", p.name, p.url);
    Ok(Peer { secret: String::new(), ..p })
}

pub async fn unlink(jf: &Jf, id: &str) -> Result<(), String> {
    crate::services::admin(jf).await?;
    if let Ok(p) = peer(id) {
        // tell it (it may be off: then it finds out at its next request)
        let _ = client(Duration::from_secs(4)).post(format!("{}/api/peer/unlink", p.url)).header("x-azoto-peer", my_id()).bearer_auth(&p.secret).send().await;
    }
    with_store(|s| s.peers.retain(|p| p.id != id));
    Ok(())
}

#[derive(Serialize)]
pub struct PeerState {
    #[serde(flatten)]
    pub peer: Peer,
    pub online: bool,
}

/// The linked AzotoTVs and which of them answer now.
pub async fn status() -> Vec<PeerState> {
    let ps = with_store(|s| s.peers.clone());
    let checks = ps.into_iter().map(|p| async move {
        let online = authed(&client(Duration::from_secs(3)), &p, "/api/peer/hello").send().await.is_ok_and(|r| r.status().is_success());
        PeerState { peer: Peer { secret: String::new(), ..p }, online }
    });
    futures_util::future::join_all(checks).await
}

/// A game on another AzotoTV, as the Giochi page shows it.
#[derive(Serialize)]
pub struct Remote {
    pub peer: String,
    pub peer_name: String,
    #[serde(flatten)]
    pub offer: Offer,
    /// downloaded here already (its folder came from there)
    pub here: Option<String>,
    /// what this machine lacks to play it ("Eden"), when something
    pub missing: Option<String>,
    /// and whether it comes along with the download (else: install it yourself)
    pub brings: bool,
}

/// Where each local game came from: (peer, game) -> local id.
pub(crate) fn origins() -> HashMap<(String, String), String> {
    crate::library::ids()
        .into_iter()
        .filter_map(|id| {
            let v: Value = serde_json::from_str(&fs::read_to_string(crate::library::game_dir(&id).join("game.json")).ok()?).ok()?;
            Some(((v["from"]["peer"].as_str()?.to_string(), v["from"]["game"].as_str()?.to_string()), id))
        })
        .collect()
}

/// This library's games: (id, Steam app id, plain name).
fn local_games() -> Vec<(String, Option<u32>, String)> {
    crate::library::ids()
        .into_iter()
        .filter_map(|id| {
            let v: Value = serde_json::from_str(&fs::read_to_string(crate::library::game_dir(&id).join("game.json")).ok()?).ok()?;
            let appid = v["steam_appid"].as_u64().map(|a| a as u32);
            let name = plain(v["name"].as_str()?);
            Some((id, appid, name))
        })
        .collect()
}

/// A name to compare games by: "Slime Rancher™: 2" -> "slimerancher2".
fn plain(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Every linked AzotoTV's games (the ones that answer).
pub async fn games() -> Vec<Remote> {
    let ps = with_store(|s| s.peers.clone());
    let lists = ps.into_iter().map(|p| async move {
        let r = authed(&client(Duration::from_secs(20)), &p, "/api/peer/games").send().await.ok()?;
        if r.status() == StatusCode::UNAUTHORIZED {
            eprintln!("[peer] {} doesn't know this AzotoTV any more", p.name);
            return None;
        }
        let offers: Vec<Offer> = r.json().await.ok()?;
        Some((p, offers))
    });
    let from = origins();
    let local = local_games();
    let mut out = Vec::new();
    for (p, offers) in futures_util::future::join_all(lists).await.into_iter().flatten() {
        for o in offers {
            let lack = missing(o.kind, o.platform.as_deref());
            // downloaded from there, or the same game got here another way (Steam)
            let same = from.get(&(p.id.clone(), o.id.clone())).cloned().or_else(|| {
                let name = plain(&o.name);
                local.iter().find(|(_, a, n)| (o.steam_appid.is_some() && *a == o.steam_appid) || *n == name).map(|(id, _, _)| id.clone())
            });
            out.push(Remote {
                here: same,
                brings: lack.as_ref().is_some_and(|(_, b)| *b),
                missing: lack.map(|(n, _)| n),
                peer: p.id.clone(),
                peer_name: p.name.clone(),
                offer: o,
            });
        }
    }
    out
}

/// What this machine lacks to run a kind of game, and whether the download
/// brings it (Eden and Proton from the other AzotoTV, Dolphin and Azahar from
/// Flathub) or it's a system package to install by hand.
pub fn missing(kind: Kind, platform: Option<&str>) -> Option<(String, bool)> {
    let have = |cmd: &str| std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()));
    if platform == Some("arcade") && !Path::new("/usr/lib/mame/mame").exists() {
        return Some(("MAME".into(), false));
    }
    let (name, brings) = match kind {
        Kind::Proton if !have("umu-run") => ("umu-launcher", false),
        Kind::Proton if !crate::games::has_proton() => ("Proton", true),
        Kind::Switch if !crate::eden::appimage().exists() => ("Eden", true),
        Kind::Wii if !crate::dolphin::installed() => ("Dolphin", true),
        Kind::N3ds if !crate::azahar::installed() => ("Azahar", true),
        _ => return None,
    };
    Some((name.into(), brings))
}

/// `peer://localhost/<peer>/<game>/<kind>`: another AzotoTV's game art.
pub async fn art(path: &str) -> Result<(Vec<u8>, String), String> {
    let mut it = path.trim_start_matches('/').split('/');
    let (Some(pid), Some(game), Some(kind), None) = (it.next(), it.next(), it.next(), it.next()) else { return Err("bad art path".into()) };
    let p = peer(pid)?;
    let r = authed(&client(Duration::from_secs(15)), &p, &format!("/api/peer/art/{game}/{kind}")).send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("HTTP {}", r.status()));
    }
    let ct = r.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("image/jpeg").to_string();
    Ok((r.bytes().await.map_err(|e| e.to_string())?.to_vec(), ct))
}

// ---- downloading -------------------------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct Progress {
    pub peer: String,
    pub game: String,
    pub name: String,
    pub done: u64,
    pub total: u64,
    /// bytes per second, lately
    pub speed: u64,
    /// "running", "done", "error", "cancelled"
    pub state: String,
    pub error: Option<String>,
    /// the new game's id, once done
    pub id: Option<String>,
    /// what's happening after the files ("Installing Dolphin")
    pub step: Option<String>,
    /// an update of this local game, rather than a new one
    pub update: Option<String>,
}

struct Job {
    cancel: Arc<AtomicBool>,
    progress: Progress,
}

#[derive(Default)]
pub struct Downloads {
    jobs: Mutex<HashMap<(String, String), Job>>,
}

impl Downloads {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn list(&self) -> Vec<Progress> {
        self.jobs.lock().unwrap().values().map(|j| j.progress.clone()).collect()
    }

    pub fn cancel(&self, peer: &str, game: &str) {
        if let Some(j) = self.jobs.lock().unwrap().get(&(peer.into(), game.into())) {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn update(&self, app: &AppHandle, key: &(String, String), f: impl FnOnce(&mut Progress)) {
        let p = {
            let mut jobs = self.jobs.lock().unwrap();
            let Some(j) = jobs.get_mut(key) else { return };
            f(&mut j.progress);
            j.progress.clone()
        };
        let _ = app.emit("peer-download", p);
    }

    /// Starts downloading `game` from `peer` into the library folder `into`.
    pub fn start(self: &Arc<Self>, app: AppHandle, peer_id: &str, game: &str, name: &str, into: &str) -> Result<(), String> {
        let into = crate::games::expand_home(into);
        fs::create_dir_all(&into).map_err(|e| format!("{}: {e}", into.display()))?;
        self.spawn(app, peer_id, game, name, Target::New(into))
    }

    /// Brings the local game `id` (downloaded from another AzotoTV) up to
    /// date with the other side's copy: only what changed comes.
    pub fn start_update(self: &Arc<Self>, app: AppHandle, id: &str) -> Result<(), String> {
        let (peer_id, game) = origin(id).ok_or_else(|| t!("This game didn't come from another AzotoTV"))?;
        let name = crate::games::read_manifest(id).map(|m| m.name).unwrap_or_else(|_| id.to_string());
        self.spawn(app, &peer_id, &game, &name, Target::Update(id.to_string()))
    }

    /// The local game being updated now, if `id` is.
    pub fn updating(&self, id: &str) -> bool {
        self.jobs.lock().unwrap().values().any(|j| j.progress.state == "running" && j.progress.update.as_deref() == Some(id))
    }

    fn spawn(self: &Arc<Self>, app: AppHandle, peer_id: &str, game: &str, name: &str, target: Target) -> Result<(), String> {
        let p = peer(peer_id)?;
        let key = (peer_id.to_string(), game.to_string());
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut jobs = self.jobs.lock().unwrap();
            if jobs.get(&key).is_some_and(|j| j.progress.state == "running") {
                return Ok(());
            }
            let progress = Progress {
                peer: peer_id.into(),
                game: game.into(),
                name: name.into(),
                done: 0,
                total: 0,
                speed: 0,
                state: "running".into(),
                error: None,
                id: None,
                step: None,
                update: match &target {
                    Target::Update(id) => Some(id.clone()),
                    Target::New(_) => None,
                },
            };
            jobs.insert(key.clone(), Job { cancel: cancel.clone(), progress });
        }
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            let r = me.run(&app, &key, &p, &target, &cancel).await;
            let cancelled = cancel.load(Ordering::Relaxed);
            me.update(&app, &key, |pr| match r {
                Ok(id) => {
                    pr.state = "done".into();
                    pr.id = Some(id);
                }
                Err(_) if cancelled => pr.state = "cancelled".into(),
                Err(e) => {
                    pr.state = "error".into();
                    pr.error = Some(e);
                }
            });
            let _ = app.emit("games-changed", ());
        });
        Ok(())
    }

    async fn run(&self, app: &AppHandle, key: &(String, String), p: &Peer, target: &Target, cancel: &AtomicBool) -> Result<String, String> {
        let game = &key.1;
        let c = client(Duration::from_secs(60 * 60 * 6));
        let Listing { files: list, dir: there } = file_list(&c, p, game).await?;
        // where the game's files go: a folder of its own until the download is
        // whole (the same each time, so one that stopped picks up again), or
        // the game's folder when it's here already
        let (dir, space_on) = match target {
            Target::New(into) => (into.join(format!(".azototv-download-{}-{game}", p.id)), into.clone()),
            Target::Update(id) => (crate::library::game_dir(id), crate::library::game_dir(id)),
        };
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let todo = match target {
            Target::New(_) => changes(&list, &dir, &[], None, true)?,
            Target::Update(id) => {
                // switched-on mods back in the folder, where the update finds them
                if let Ok(m) = crate::games::read_manifest(id) {
                    crate::mods::park(&dir, &m.mods, &m.mod_loader)?;
                }
                changes(&list, &dir, &[], read_record(&dir).as_ref(), false)?
            }
        };
        let total: u64 = todo.steps.iter().map(|x| x.f.size).sum();
        let have: u64 = todo.steps.iter().map(|x| fs::metadata(&x.tmp).map_or(0, |m| m.len().min(x.f.size))).sum();
        if let Some(free) = free_space(&space_on) {
            if total.saturating_sub(have) + (256 << 20) > free {
                return Err(t!("Not enough space: it needs {need}, there are {free}", need = gb(total - have), free = gb(free)));
            }
        }
        self.update(app, key, |pr| {
            pr.total = total;
            pr.done = have;
        });
        let (mut done, mut mark, mut last) = (have, have, Instant::now());
        let mut speed_now = |done: u64, force: bool| {
            if force || last.elapsed() >= Duration::from_millis(700) {
                let speed = ((done - mark) as f64 / last.elapsed().as_secs_f64().max(0.001)) as u64;
                (mark, last) = (done, Instant::now());
                self.update(app, key, |pr| {
                    pr.done = done;
                    pr.speed = speed;
                });
            }
        };
        // small files first, many to a request; what's left (big ones, links,
        // empty ones, or all of them when the other side is too old for
        // packs) one by one below
        let mut fetched = vec![false; todo.steps.len()];
        let small: Vec<usize> = (0..todo.steps.len())
            .filter(|&i| {
                let x = &todo.steps[i];
                x.f.link.is_none() && x.f.size > 0 && x.f.size <= PACK_SMALL && fs::metadata(&x.tmp).map_or(0, |m| m.len()) != x.f.size
            })
            .collect();
        let mut packs = Vec::new();
        let (mut pack, mut bytes) = (Vec::new(), 0u64);
        for i in small {
            let size = todo.steps[i].f.size;
            if !pack.is_empty() && (pack.len() >= PACK_FILES || bytes + size > PACK_BYTES) {
                packs.push(std::mem::take(&mut pack));
                bytes = 0;
            }
            pack.push(i);
            bytes += size;
        }
        if !pack.is_empty() {
            packs.push(pack);
        }
        for pack in packs {
            let paths: Vec<&str> = pack.iter().map(|&i| todo.steps[i].f.path.as_str()).collect();
            let r = call(&c, reqwest::Method::POST, p, &format!("/api/peer/pack/{game}")).json(&paths).send().await.map_err(|_| t!("{name} doesn't answer", name = p.name))?;
            if matches!(r.status(), StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED) {
                // an AzotoTV from before packs: one by one
                eprintln!("[peer] {} can't send packs: one file at a time", p.name);
                break;
            }
            if !r.status().is_success() {
                return Err(format!("{}: HTTP {}", p.name, r.status()));
            }
            let mut r = r;
            let (mut buf, mut next) = (Vec::<u8>::new(), 0usize);
            loop {
                // whole files out of what came so far
                while next < pack.len() && buf.len() >= 8 {
                    let len = u64::from_le_bytes(buf[..8].try_into().unwrap());
                    if len == PACK_NONE {
                        buf.drain(..8);
                        next += 1;
                        continue;
                    }
                    let i = pack[next];
                    let x = &todo.steps[i];
                    if len != x.f.size {
                        return Err(t!("{path} changed on {name} during the download: start it again", path = x.f.path, name = p.name));
                    }
                    if (buf.len() as u64) < 8 + len {
                        break;
                    }
                    if let Some(d) = x.tmp.parent() {
                        fs::create_dir_all(d).map_err(|e| e.to_string())?;
                    }
                    let got = fs::metadata(&x.tmp).map_or(0, |m| m.len());
                    fs::write(&x.tmp, &buf[8..8 + len as usize]).map_err(|e| format!("{}: {e}", x.tmp.display()))?;
                    buf.drain(..8 + len as usize);
                    finish(x)?;
                    fetched[i] = true;
                    done = done - got.min(len) + len;
                    next += 1;
                    speed_now(done, false);
                }
                if cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".into());
                }
                match r.chunk().await.map_err(|_| t!("{name} stopped answering", name = p.name))? {
                    Some(chunk) => buf.extend_from_slice(&chunk),
                    None => break,
                }
            }
            if next < pack.len() {
                return Err(t!("{name} stopped answering", name = p.name));
            }
        }
        for (i, x) in todo.steps.iter().enumerate() {
            if fetched[i] {
                continue;
            }
            let f = x.f;
            if let Some(d) = x.tmp.parent() {
                fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            if let Some(target) = &f.link {
                let _ = fs::remove_file(&x.tmp);
                std::os::unix::fs::symlink(target, &x.tmp).map_err(|e| format!("{}: {e}", x.tmp.display()))?;
                continue;
            }
            let got = fs::metadata(&x.tmp).map_or(0, |m| m.len());
            if f.size == 0 {
                // nothing to fetch, but it must be there (mods use empty
                // marker files): a missing file also reads as 0 bytes
                fs::File::create(&x.tmp).map_err(|e| format!("{}: {e}", x.tmp.display()))?;
            } else if got != f.size {
                let from = if got < f.size { got } else { 0 };
                let path: String = f.path.split('/').map(urlencode).collect::<Vec<_>>().join("/");
                let mut req = authed(&c, p, &format!("/api/peer/file/{game}/{path}"));
                if from > 0 {
                    req = req.header(header::RANGE, format!("bytes={from}-"));
                }
                let mut r = req.send().await.map_err(|_| t!("{name} doesn't answer", name = p.name))?;
                if !r.status().is_success() {
                    return Err(format!("{}: HTTP {}", f.path, r.status()));
                }
                let resumed = r.status() == StatusCode::PARTIAL_CONTENT;
                let mut out = fs::OpenOptions::new().create(true).write(true).append(resumed).truncate(!resumed).open(&x.tmp).map_err(|e| e.to_string())?;
                done -= if resumed { 0 } else { got.min(f.size) };
                while let Some(chunk) = r.chunk().await.map_err(|_| t!("{name} stopped answering", name = p.name))? {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("cancelled".into());
                    }
                    std::io::Write::write_all(&mut out, &chunk).map_err(|e| e.to_string())?;
                    done += chunk.len() as u64;
                    speed_now(done, false);
                }
            }
            finish(x)?;
        }
        for gone in &todo.removed {
            match fs::remove_file(gone) {
                Ok(()) => eprintln!("[peer] removed {}", gone.display()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => eprintln!("[peer] removing {}: {e}", gone.display()),
            }
            // and the folders it leaves empty, inside the game's folder
            let mut d = gone.parent();
            while let Some(x) = d.filter(|x| x.starts_with(&dir) && *x != dir.as_path()) {
                if fs::remove_dir(x).is_err() {
                    break;
                }
                d = x.parent();
            }
        }
        write_record(&dir, &list);
        self.update(app, key, |pr| pr.done = total);
        let (id, dest) = match target {
            Target::New(into) => {
                // whole: it becomes a game of the library
                let id = free_id(into, game);
                let dest = into.join(&id);
                fs::rename(&dir, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
                crate::library::add_library(into)?;
                eprintln!("[peer] {} downloaded from {} into {}", game, p.name, dest.display());
                // so its saves can follow (saves.rs)
                crate::saves::tell_pair(p, game, &id).await;
                (id, dest)
            }
            Target::Update(id) => {
                eprintln!("[peer] {id} updated from {} ({} files, {} removed)", p.name, todo.steps.len(), todo.removed.len());
                (id.clone(), dir)
            }
        };
        if let Some(there) = &there {
            rehome(&dest, &todo.steps, there);
        }
        let manifest: Value = fs::read_to_string(dest.join("game.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        // an emulator that comes from Flathub rather than from the other machine
        let kind: Option<Kind> = serde_json::from_value(manifest["type"].clone()).ok();
        let flatpak = match kind {
            Some(Kind::Wii) if !crate::dolphin::installed() => Some(("Dolphin", crate::dolphin::FLATPAK)),
            Some(Kind::N3ds) if !crate::azahar::installed() => Some(("Azahar", crate::azahar::FLATPAK)),
            _ => None,
        };
        if let Some((name, app_id)) = flatpak {
            self.update(app, key, |pr| pr.step = Some(t!("Installing {name}", name = name)));
            if let Err(e) = flatpak_install(app_id).await {
                // the game is here all the same: only the emulator is missing
                eprintln!("[peer] installing {app_id}: {e}");
                self.update(app, key, |pr| pr.error = Some(t!("The game is here, but {name} couldn't be installed: {e}", name = name, e = e)));
            }
            self.update(app, key, |pr| pr.step = None);
        }
        if let Some(script) = manifest["peer"]["prepare"].as_str() {
            self.update(app, key, |pr| pr.step = Some(t!("Getting it ready for this computer")));
            let (dir, script, env) = (dest.clone(), script.to_string(), manifest["env"].clone());
            let r = tauri::async_runtime::spawn_blocking(move || prepare(&dir, &script, &env)).await.unwrap_or_else(|e| Err(e.to_string()));
            if let Err(e) = r {
                // the game is here all the same, and the game itself may do it at its first start
                eprintln!("[peer] {id}: preparing: {e}");
                self.update(app, key, |pr| pr.error = Some(t!("The game is here, but getting it ready for this computer failed: {e}", e = e)));
            }
            self.update(app, key, |pr| pr.step = None);
        }
        Ok(id)
    }
}

/// Files that just came and name the other machine's game folder (`there`)
/// by its full path, a recomp's settings say, name this one (`dest`) instead.
/// Only small files without NUL bytes (settings, not game data), and only
/// the whole folder name: "/g/mkw" in "/g/mkw/x" or "/g/mkw\"", not in "/g/mkw2".
/// The file keeps the other side's time, so an update still tells it's the
/// same version (its record says what came).
fn rehome(dest: &Path, steps: &[Step], there: &str) {
    const SMALL: u64 = 1 << 20;
    let here = dest.to_string_lossy();
    if there == here {
        return;
    }
    for x in steps {
        if x.f.link.is_some() || x.f.runner.is_some() || x.f.size > SMALL || x.f.path.starts_with("~/") {
            continue;
        }
        let p = dest.join(&x.f.path);
        let Ok(text) = fs::read(&p) else { continue };
        if text.contains(&0) {
            continue;
        }
        let Some(new) = replace_dir(&text, there.as_bytes(), here.as_bytes()) else { continue };
        match fs::write(&p, new) {
            Ok(()) => {
                eprintln!("[peer] {}: now names {here}", x.f.path);
                if let (Some(ms), Ok(h)) = (x.f.mtime, fs::OpenOptions::new().write(true).open(&p)) {
                    let _ = h.set_modified(std::time::UNIX_EPOCH + Duration::from_millis(ms));
                }
            }
            Err(e) => eprintln!("[peer] {}: {e}", p.display()),
        }
    }
}

/// `text` with the folder `old` replaced by `new` where it stands as a whole
/// path (not inside a longer one), or None when it isn't there.
fn replace_dir(text: &[u8], old: &[u8], new: &[u8]) -> Option<Vec<u8>> {
    let part_of_name = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'/') || c >= 0x80;
    let mut out = Vec::with_capacity(text.len());
    let (mut i, mut hit) = (0, false);
    while i < text.len() {
        if text[i..].starts_with(old)
            && (i == 0 || !part_of_name(text[i - 1]))
            && text.get(i + old.len()).is_none_or(|&c| c == b'/' || !part_of_name(c))
        {
            out.extend_from_slice(new);
            i += old.len();
            hit = true;
        } else {
            out.push(text[i]);
            i += 1;
        }
    }
    hit.then_some(out)
}

/// Runs a game's `"peer": {"prepare": …}` script in its folder, with its
/// `env`, output in `prepare.log` there (logs never go to the other machine).
fn prepare(dir: &Path, script: &str, env: &Value) -> Result<(), String> {
    let (home_rel, rel) = checked(script)?;
    if home_rel {
        return Err(format!("not in the game's folder: {script}"));
    }
    let log = fs::File::create(dir.join("prepare.log")).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(dir.join(rel));
    cmd.current_dir(dir).stdin(std::process::Stdio::null()).stdout(log.try_clone().map_err(|e| e.to_string())?).stderr(log);
    for (k, v) in env.as_object().into_iter().flatten() {
        if let Some(v) = v.as_str() {
            cmd.env(k, v);
        }
    }
    let st = cmd.status().map_err(|e| format!("{script}: {e}"))?;
    if st.success() { Ok(()) } else { Err(format!("{script}: {st}, see prepare.log")) }
}

/// A fetched file made like the other side's: its time (so the next update
/// tells it's the same file), executable or not, and in its place.
fn finish(x: &Step) -> Result<(), String> {
    if let Some(ms) = x.f.mtime {
        if let Ok(h) = fs::OpenOptions::new().write(true).open(&x.tmp) {
            let _ = h.set_modified(std::time::UNIX_EPOCH + Duration::from_millis(ms));
        }
    }
    if x.f.exec {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&x.tmp, fs::Permissions::from_mode(0o755));
    }
    if x.tmp != x.dest {
        fs::rename(&x.tmp, &x.dest).map_err(|e| format!("{}: {e}", x.dest.display()))?;
    }
    Ok(())
}

enum Target {
    /// a new game, into this library folder
    New(PathBuf),
    /// this local game, from where it came
    Update(String),
}

/// The answer to `files/<game>` names the game's folder over there in this header.
const DIR_HEADER: &str = "x-azoto-dir";

/// A game's files on the other side, and where its folder is there (an
/// AzotoTV from before [`DIR_HEADER`] doesn't say).
struct Listing {
    files: Vec<FileInfo>,
    dir: Option<String>,
}

async fn file_list(c: &reqwest::Client, p: &Peer, game: &str) -> Result<Listing, String> {
    let r = authed(c, p, &format!("/api/peer/files/{game}")).send().await.map_err(|_| t!("{name} doesn't answer", name = p.name))?;
    if !r.status().is_success() {
        let v: Value = r.json().await.unwrap_or_default();
        return Err(v["error"].as_str().unwrap_or("can't be downloaded").to_string());
    }
    let dir = r.headers().get(DIR_HEADER).map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned()).filter(|d| d.starts_with('/'));
    Ok(Listing { files: r.json().await.map_err(|e| e.to_string())?, dir })
}

/// Where a local game came from: (peer id, its id there).
fn origin(id: &str) -> Option<(String, String)> {
    let v: Value = serde_json::from_str(&fs::read_to_string(crate::library::game_dir(id).join("game.json")).ok()?).ok()?;
    Some((v["from"]["peer"].as_str()?.to_string(), v["from"]["game"].as_str()?.to_string()))
}

/// In a downloaded game's folder: what came with it, path -> stamp, so an
/// update knows what changed there and what's gone, and leaves alone what
/// changed only here.
const RECORD: &str = ".azototv-from.json";

fn read_record(dir: &Path) -> Option<HashMap<String, String>> {
    let v: Value = serde_json::from_str(&fs::read_to_string(dir.join(RECORD)).ok()?).ok()?;
    serde_json::from_value(v["files"].clone()).ok()
}

fn write_record(dir: &Path, list: &[FileInfo]) {
    // an AzotoTV from before updates sends no stamps: no record (the next
    // check goes by size and writes one)
    if list.iter().any(|f| f.stamp.is_empty()) {
        return;
    }
    let files: std::collections::BTreeMap<&str, &str> = list.iter().filter(|f| f.runner.is_none() && !f.once).map(|f| (f.path.as_str(), f.stamp.as_str())).collect();
    if let Err(e) = fs::write(dir.join(RECORD), serde_json::to_string_pretty(&json!({ "files": files })).unwrap_or_default()) {
        eprintln!("[peer] {}: {e}", dir.join(RECORD).display());
    }
}

/// Where the home folder may receive files from another AzotoTV: emulators,
/// their keys and firmware, packs and updates. Nothing else, and nothing
/// that's there already gets replaced, unless it came with this game.
const HOME_OK: [&str; 8] = [
    "Eden/Eden.appimage",
    "Eden/Extra/",
    ".local/share/eden/keys/",
    ".local/share/eden/nand/system/Contents/registered/",
    ".local/share/azototv/dolphin/Load/",
    ".local/share/azototv/dolphin/GameSettings/",
    ".local/share/azototv/azahar/",
    ".local/share/Steam/compatibilitytools.d/",
];

struct Step<'a> {
    f: &'a FileInfo,
    /// where it ends up
    dest: PathBuf,
    /// where it's written meanwhile (the same, inside a new download's folder)
    tmp: PathBuf,
}

struct Changes<'a> {
    steps: Vec<Step<'a>>,
    /// what came last time and the other side doesn't have any more
    removed: Vec<PathBuf>,
}

/// A path from the other side, checked: (in the home, relative path).
fn checked(path: &str) -> Result<(bool, &Path), String> {
    let (home_rel, rel) = match path.strip_prefix("~/") {
        Some(r) => (true, Path::new(r)),
        None => (false, Path::new(path)),
    };
    if rel.as_os_str().is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("bad path from the other AzotoTV: {path}"));
    }
    if home_rel {
        let r = rel.to_string_lossy();
        if !HOME_OK.iter().any(|ok| if ok.ends_with('/') { r.starts_with(ok) } else { r == *ok }) {
            return Err(format!("not a place for it: {path}"));
        }
    }
    Ok((home_rel, rel))
}

/// What to fetch and where, for a game going into `dir` (a new download's
/// folder when `fresh`, else the game's own folder, `record` saying what came
/// last time): what this machine lacks or has in another version. Left
/// alone: what changed only here, an emulator or Proton it has already, a
/// file in the home this game didn't bring. `places`: the game's mods
/// (see [`crate::mods::whereabouts`]).
fn changes<'a>(list: &'a [FileInfo], dir: &Path, places: &[(PathBuf, PathBuf)], record: Option<&HashMap<String, String>>, fresh: bool) -> Result<Changes<'a>, String> {
    let home = crate::games::home();
    let proton_here = crate::games::has_proton();
    let mut steps = Vec::new();
    for f in list {
        let (home_rel, rel) = checked(&f.path)?;
        if !home_rel {
            if let Some(target) = &f.link {
                // only links that stay inside the game folder
                let mut depth = 0i32;
                for c in rel.parent().unwrap_or(Path::new("")).join(target).components() {
                    depth += match c {
                        Component::ParentDir => -1,
                        Component::Normal(_) => 1,
                        _ => -100,
                    };
                    if depth < 0 {
                        return Err(format!("bad link from the other AzotoTV: {}", f.path));
                    }
                }
            }
        } else if f.link.is_some() {
            continue;
        }
        if f.runner.as_deref() == Some("proton") && proton_here {
            continue;
        }
        let dest = if home_rel { home.join(rel) } else { dir.join(rel) };
        let now = if home_rel { dest.clone() } else { crate::mods::whereabouts(&dest, places) };
        let came = record.and_then(|r| r.get(&f.path));
        if let Ok(m) = fs::symlink_metadata(&now) {
            let same = f.once
                || f.runner.is_some()
                || match &f.link {
                    Some(t) => fs::read_link(&now).is_ok_and(|l| l == Path::new(t)),
                    // the other side didn't change it since (whatever happened to it here)
                    None if came == Some(&f.stamp) => true,
                    None => m.is_file() && m.len() == f.size && ((f.mtime.is_some() && mtime_ms(&m) == f.mtime) || record.is_none()),
                };
            // a file in the home is replaced only when it came with this game
            if same || (home_rel && came.is_none()) {
                continue;
            }
        }
        let tmp = if (fresh && !home_rel) || f.link.is_some() {
            dest.clone()
        } else {
            let mut t = dest.clone().into_os_string();
            t.push(".azototv-part");
            t.into()
        };
        steps.push(Step { f, dest, tmp });
    }
    let mut removed = Vec::new();
    for path in record.into_iter().flat_map(|r| r.keys()) {
        if list.iter().any(|f| &f.path == path) {
            continue;
        }
        let Ok((home_rel, rel)) = checked(path) else { continue };
        let p = if home_rel { home.join(rel) } else { crate::mods::whereabouts(&dir.join(rel), places) };
        if fs::symlink_metadata(&p).is_ok() {
            removed.push(p);
        }
    }
    Ok(Changes { steps, removed })
}

/// What an update of the local game `id` would bring, when there's something.
#[derive(Serialize)]
pub struct Update {
    pub peer: String,
    pub peer_name: String,
    /// its id there
    pub game: String,
    pub files: usize,
    pub bytes: u64,
    pub removed: usize,
}

pub async fn update_info(id: &str) -> Result<Option<Update>, String> {
    let Some((peer_id, game)) = origin(id) else { return Ok(None) };
    let Ok(p) = peer(&peer_id) else { return Ok(None) };
    let list = file_list(&client(Duration::from_secs(60)), &p, &game).await?.files;
    let dir = crate::library::game_dir(id);
    let m = crate::games::read_manifest(id)?;
    let places = crate::mods::places(&dir, &m.mods, &m.mod_loader);
    let record = read_record(&dir);
    let todo = changes(&list, &dir, &places, record.as_ref(), false)?;
    if todo.steps.is_empty() && todo.removed.is_empty() {
        // a copy from before the records: now it has one
        if record.is_none() {
            write_record(&dir, &list);
        }
        return Ok(None);
    }
    Ok(Some(Update {
        peer: p.id.clone(),
        peer_name: p.name.clone(),
        game,
        files: todo.steps.len(),
        bytes: todo.steps.iter().map(|x| x.f.size).sum(),
        removed: todo.removed.len(),
    }))
}

async fn flatpak_install(app_id: &str) -> Result<(), String> {
    let run = |args: &[&str]| {
        let mut c = tokio::process::Command::new("flatpak");
        c.args(args).stdin(std::process::Stdio::null());
        c
    };
    let _ = run(&["remote-add", "--user", "--if-not-exists", "flathub", "https://dl.flathub.org/repo/flathub.flatpakrepo"]).output().await;
    let o = run(&["install", "--user", "-y", "--noninteractive", "flathub", app_id]).output().await.map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("flatpak failed").to_string())
    }
}

/// `game`, or game-2, game-3… when a folder by that name is there already.
fn free_id(into: &Path, game: &str) -> String {
    let taken = |id: &str| into.join(id).exists() || crate::library::game_dir(id).join("game.json").exists();
    if !taken(game) {
        return game.to_string();
    }
    (2..).map(|n| format!("{game}-{n}")).find(|id| !taken(id)).unwrap()
}

fn free_space(p: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0).then(|| s.f_bavail as u64 * s.f_frsize as u64)
}

fn gb(b: u64) -> String {
    format!("{:.1} GB", b as f64 / 1e9)
}

pub(crate) fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_inside_the_game() {
        assert_eq!(relative(Path::new("home-seed/.local/share/ghostship"), Path::new("game/sm64.o2r")), PathBuf::from("../../../../game/sm64.o2r"));
        assert_eq!(relative(Path::new("game"), Path::new("game/x")), PathBuf::from("x"));
    }

    fn info(path: &str, size: u64, mtime: u64) -> FileInfo {
        FileInfo { path: path.into(), size, exec: false, link: None, runner: None, stamp: format!("{size}-{mtime}"), mtime: Some(mtime), once: false }
    }

    fn put(dir: &Path, rel: &str, body: &str, mtime: u64) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, body).unwrap();
        let f = fs::OpenOptions::new().write(true).open(&p).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + Duration::from_millis(mtime)).unwrap();
    }

    fn fetched(c: &Changes) -> Vec<String> {
        let mut v: Vec<String> = c.steps.iter().map(|s| s.f.path.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn updates_bring_only_what_changed_there() {
        let dir = std::env::temp_dir().join(format!("azototv-peer-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        put(&dir, "game/a.bin", "aaaa", 1000);
        put(&dir, "game/b.bin", "bbbb", 1000);
        put(&dir, "config/settings.ini", "mine, longer than theirs", 5000);
        put(&dir, "cache/shaders", "x", 7000);
        put(&dir, "mods/old/x.txt", "old", 1000);
        let rec: HashMap<String, String> = [("game/a.bin", "4-1000"), ("game/b.bin", "4-1000"), ("config/settings.ini", "5-1000"), ("mods/old/x.txt", "3-1000")]
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let mut cache = info("cache/shaders", 9, 9000);
        cache.once = true;
        let list = vec![
            info("game/a.bin", 4, 1000),                   // the same
            info("game/b.bin", 6, 2000),                   // changed there
            info("config/settings.ini", 5, 1000),          // changed only here
            info("mods/new/y.txt", 3, 3000),               // new there
            cache,                                         // each keeps its own
        ];
        let c = changes(&list, &dir, &[], Some(&rec), false).unwrap();
        assert_eq!(fetched(&c), ["game/b.bin", "mods/new/y.txt"]);
        assert_eq!(c.removed, [dir.join("mods/old/x.txt")]);
        // written next to the file, then moved over it
        assert!(c.steps.iter().all(|s| s.tmp.to_string_lossy().ends_with(".azototv-part")));

        // a copy from before the records: the same size is the same file
        let c = changes(&list, &dir, &[], None, false).unwrap();
        assert_eq!(fetched(&c), ["config/settings.ini", "game/b.bin", "mods/new/y.txt"]);
        assert!(c.removed.is_empty());

        // a file that came with the time from there is the same, record or not
        put(&dir, "game/b.bin", "bbbbbb", 2000);
        let c = changes(&list, &dir, &[], Some(&rec), false).unwrap();
        assert_eq!(fetched(&c), ["mods/new/y.txt"]);

        // a switched-on mod's files are where the emulator finds them
        let active = dir.join("emu/load/new");
        put(&dir, "emu/load/new/y.txt", "yyy", 3000);
        let places = vec![(dir.join("mods/new"), active)];
        let c = changes(&list, &dir, &places, Some(&rec), false).unwrap();
        assert!(fetched(&c).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_name_this_machines_folder() {
        let r = |t: &str| replace_dir(t.as_bytes(), b"/srv/Games/kart", b"/home/ada/Giochi/kart").map(|v| String::from_utf8(v).unwrap());
        assert_eq!(r("dvd_root = \"/srv/Games/kart/data/DATA\"\n").as_deref(), Some("dvd_root = \"/home/ada/Giochi/kart/data/DATA\"\n"));
        assert_eq!(r("root=/srv/Games/kart\nx=/srv/Games/kart/a").as_deref(), Some("root=/home/ada/Giochi/kart\nx=/home/ada/Giochi/kart/a"));
        // another game, a longer path, or nothing to do
        assert_eq!(r("a=/srv/Games/kart2/x b=/mnt/srv/Games/kart/x"), None);
        assert_eq!(r("nothing here"), None);
    }

    #[test]
    fn folder_links_go_as_links() {
        let dir = std::env::temp_dir().join(format!("azototv-peer-links-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        put(&dir, "data/built/code.so", "x", 1000);
        put(&dir, "data/shared.ini", "y", 1000);
        put(&dir, "home-seed/.local/share/kart/settings.ini", "z", 1000);
        let seed = dir.join("home-seed/.local/share/kart");
        std::os::unix::fs::symlink(dir.join("data/built"), seed.join("built")).unwrap();
        std::os::unix::fs::symlink(dir.join("data/shared.ini"), seed.join("shared.ini")).unwrap();
        std::os::unix::fs::symlink("/etc", seed.join("elsewhere")).unwrap();
        let mut out = Vec::new();
        walk(&dir, &dir, &mut out);
        let link = |p: &str| out.iter().find(|e| e.path == p).map(|e| match &e.src {
            Source::Link(t) => t.clone(),
            _ => "a file".into(),
        });
        assert_eq!(link("home-seed/.local/share/kart/built").as_deref(), Some("../../../../data/built"));
        assert_eq!(link("home-seed/.local/share/kart/shared.ini").as_deref(), Some("../../../../data/shared.ini"));
        assert_eq!(link("home-seed/.local/share/kart/elsewhere"), None);
        // what's behind a folder link goes once, from its own place
        assert!(link("data/built/code.so").is_some() && link("home-seed/.local/share/kart/built/code.so").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_never_goes() {
        assert!(under("data/x", "data") && under("data", "data/") && !under("database", "data") && !under("x", ""));
        assert!(never_goes("logs") && never_goes("runtime.prev") && !never_goes("data"));
        assert!(goes_once("Cache") && goes_once("shader_cache") && !goes_once("caches"));
        assert!(checked("../x").is_err() && checked("~/.ssh/id").is_err() && checked("~/Eden/Extra/u.nsp").is_ok());
    }

    #[test]
    fn addresses() {
        assert_eq!(norm_url("192.168.1.5"), "http://192.168.1.5:8686");
        assert_eq!(norm_url("http://tv.lan:9000/"), "http://tv.lan:9000");
        assert_eq!(urlencode("a b+ü.x"), "a%20b%2B%C3%BC.x");
        assert_eq!(plain("Slime Rancher™: 2"), "slimerancher2");
    }
}
