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
//! (with Range, so a download picks up where it stopped).
//!
//! What goes: a game's whole folder (symbolic links left out: those are the
//! players' saves, see `games::link_player_dirs`), plus its executable or ROM
//! when it lives elsewhere (it lands in `ext/` and the `game.json` that goes
//! is changed to point there). Steam games and Minecraft stay where they are.
//! The new folder's `game.json` says where it came from (`"from"`).

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
    http::{HeaderMap, StatusCode, header},
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

fn peer(id: &str) -> Result<Peer, String> {
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
        .with_state(Ctx { app, lists: Default::default() })
}

fn err(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(json!({ "error": msg.into() }))).into_response()
}

/// The calling peer, when its secret is right. Its address is kept up to
/// date (a computer's changes now and then).
fn caller(h: &HeaderMap, from: SocketAddr) -> Result<Peer, Response> {
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
    /// bytes, everything that goes
    pub size: u64,
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
                    size: files.iter().map(|e| e.size).sum(),
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
    path: String,
    src: Source,
    size: u64,
    exec: bool,
}

#[derive(Clone)]
enum Source {
    File(PathBuf),
    /// the changed game.json
    Bytes(Arc<Vec<u8>>),
}

#[derive(Serialize, Deserialize)]
struct FileInfo {
    path: String,
    size: u64,
    #[serde(default)]
    exec: bool,
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
            out.push(Entry { path: rel.clone(), size: meta.len(), exec: is_exec(&meta), src: Source::File(p) });
        }
        m["exe"] = json!(rel);
        Ok(())
    };
    ext(&mut m, &mut out)?;
    if let Some(chapters) = m["chapters"].as_array_mut() {
        for c in chapters {
            ext(c, &mut out)?;
        }
    }
    if let Some(o) = m.as_object_mut() {
        o.remove("added");
        o.insert("from".into(), json!({ "peer": my_id(), "game": game }));
    }
    let bytes = Arc::new(serde_json::to_string_pretty(&m).unwrap_or_default().into_bytes());
    out.retain(|e| e.path != "game.json");
    out.push(Entry { path: "game.json".into(), size: bytes.len() as u64, exec: false, src: Source::Bytes(bytes) });
    Ok(out)
}

fn is_exec(m: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    m.permissions().mode() & 0o111 != 0
}

/// The files under `dir` (symbolic links left out).
fn walk(root: &Path, dir: &Path, out: &mut Vec<Entry>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(meta) = fs::symlink_metadata(e.path()) else { continue };
        if meta.is_dir() {
            walk(root, &e.path(), out);
        } else if meta.is_file() {
            let rel = e.path().strip_prefix(root).map(|r| r.to_string_lossy().to_string()).unwrap_or_default();
            if !rel.is_empty() && !rel.ends_with(".log") {
                out.push(Entry { path: rel, size: meta.len(), exec: is_exec(&meta), src: Source::File(e.path()) });
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
        Ok(Ok(l)) => Json(l.iter().map(|e| FileInfo { path: e.path.clone(), size: e.size, exec: e.exec }).collect::<Vec<_>>()).into_response(),
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

// ---- calling the others -------------------------------------------------------------------

fn client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder().connect_timeout(Duration::from_secs(3)).timeout(timeout).build().expect("http client")
}

fn authed(c: &reqwest::Client, p: &Peer, path: &str) -> reqwest::RequestBuilder {
    c.get(format!("{}{path}", p.url)).header("x-azoto-peer", my_id()).bearer_auth(&p.secret)
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
}

/// Where each local game came from: (peer, game) -> local id.
fn origins() -> HashMap<(String, String), String> {
    crate::library::ids()
        .into_iter()
        .filter_map(|id| {
            let v: Value = serde_json::from_str(&fs::read_to_string(crate::library::game_dir(&id).join("game.json")).ok()?).ok()?;
            Some(((v["from"]["peer"].as_str()?.to_string(), v["from"]["game"].as_str()?.to_string()), id))
        })
        .collect()
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
    let mut out = Vec::new();
    for (p, offers) in futures_util::future::join_all(lists).await.into_iter().flatten() {
        for o in offers {
            out.push(Remote {
                here: from.get(&(p.id.clone(), o.id.clone())).cloned(),
                missing: missing(o.kind),
                peer: p.id.clone(),
                peer_name: p.name.clone(),
                offer: o,
            });
        }
    }
    out
}

/// What this machine lacks to run a kind of game, in words.
pub fn missing(kind: Kind) -> Option<String> {
    let have = |cmd: &str| std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()));
    match kind {
        Kind::Proton if !have("umu-run") => Some("umu-launcher".into()),
        Kind::Proton if !crate::games::has_proton() => Some("Proton".into()),
        Kind::Switch if !crate::eden::appimage().exists() => Some("Eden".into()),
        Kind::Wii if !crate::dolphin::installed() => Some("Dolphin".into()),
        Kind::N3ds if !crate::azahar::installed() => Some("Azahar".into()),
        _ => None,
    }
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
        let p = peer(peer_id)?;
        let into = crate::games::expand_home(into);
        fs::create_dir_all(&into).map_err(|e| format!("{}: {e}", into.display()))?;
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
            };
            jobs.insert(key.clone(), Job { cancel: cancel.clone(), progress });
        }
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            let r = me.run(&app, &key, &p, &into, &cancel).await;
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

    async fn run(&self, app: &AppHandle, key: &(String, String), p: &Peer, into: &Path, cancel: &AtomicBool) -> Result<String, String> {
        let game = &key.1;
        let c = client(Duration::from_secs(60 * 60 * 6));
        let r = authed(&c, p, &format!("/api/peer/files/{game}")).send().await.map_err(|_| t!("{name} doesn't answer", name = p.name))?;
        if !r.status().is_success() {
            let v: Value = r.json().await.unwrap_or_default();
            return Err(v["error"].as_str().unwrap_or("can't be downloaded").to_string());
        }
        let list: Vec<FileInfo> = r.json().await.map_err(|e| e.to_string())?;
        let total: u64 = list.iter().map(|f| f.size).sum();
        // the same folder each time, so a download that stopped picks up again
        let part = into.join(format!(".azototv-download-{}-{game}", &p.id));
        fs::create_dir_all(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        let have: u64 = list.iter().map(|f| fs::metadata(part.join(&f.path)).map_or(0, |m| m.len().min(f.size))).sum();
        if let Some(free) = free_space(into) {
            if total.saturating_sub(have) + (256 << 20) > free {
                return Err(t!("Not enough space: it needs {need}, there are {free}", need = gb(total - have), free = gb(free)));
            }
        }
        self.update(app, key, |pr| {
            pr.total = total;
            pr.done = have;
        });
        let (mut done, mut mark, mut last) = (have, have, Instant::now());
        for f in &list {
            let rel = Path::new(&f.path);
            if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
                return Err(format!("bad path from the other AzotoTV: {}", f.path));
            }
            let dest = part.join(rel);
            let got = fs::metadata(&dest).map_or(0, |m| m.len());
            if got == f.size {
                continue;
            }
            if let Some(d) = dest.parent() {
                fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
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
            let mut out = fs::OpenOptions::new().create(true).write(true).append(resumed).truncate(!resumed).open(&dest).map_err(|e| e.to_string())?;
            done -= if resumed { 0 } else { got.min(f.size) };
            while let Some(chunk) = r.chunk().await.map_err(|_| t!("{name} stopped answering", name = p.name))? {
                if cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".into());
                }
                std::io::Write::write_all(&mut out, &chunk).map_err(|e| e.to_string())?;
                done += chunk.len() as u64;
                if last.elapsed() >= Duration::from_millis(700) {
                    let speed = ((done - mark) as f64 / last.elapsed().as_secs_f64()) as u64;
                    (mark, last) = (done, Instant::now());
                    self.update(app, key, |pr| {
                        pr.done = done;
                        pr.speed = speed;
                    });
                }
            }
            if f.exec {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(0o755));
            }
        }
        // whole: it becomes a game of the library
        let id = free_id(into, game);
        let dest = into.join(&id);
        fs::rename(&part, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
        crate::library::add_library(into)?;
        eprintln!("[peer] {} downloaded from {} into {}", game, p.name, dest.display());
        self.update(app, key, |pr| pr.done = total);
        Ok(id)
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

fn urlencode(s: &str) -> String {
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
    fn addresses() {
        assert_eq!(norm_url("192.168.1.5"), "http://192.168.1.5:8686");
        assert_eq!(norm_url("http://tv.lan:9000/"), "http://tv.lan:9000");
        assert_eq!(urlencode("a b+ü.x"), "a%20b%2B%C3%BC.x");
    }
}
