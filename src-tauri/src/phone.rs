//! Phones: a small web app for the family's phones (Android through a thin
//! WebView app, iPhone as a home-screen web app), served on the LAN at port
//! 8686. A profile pairs a phone from START > Collega un telefono (a QR code
//! and a 4-digit code that are good for that profile only); the phone keeps a
//! token, and the same phone can be paired with more profiles. A phone:
//! - is a remote: the TV remote's actions (remote.rs), over a WebSocket
//! - types into the TV's on-screen keyboard (`phone-text`)
//! - sets the TV's volume (UPnP, tv.rs) and turns it off / on (`phone-power`)
//! - sends a link to the TV (`phone-send`: YouTube in the YouTube app, any
//!   other video through yt-dlp)
//! - adds photos to the Galleria's albums, as Immich's TV user (immich.rs),
//!   and shows them on the TV (`phone-show`)
//!
//! A phone controls the TV only while one of its profiles is on, and the
//! profile picker (to choose one of them). The UI checks what it gets
//! (`phone-key` & co. carry the phone's profiles) and otherwise asks on screen
//! whether to switch to one of the phone's profiles; here only what the UI
//! doesn't see is checked: presses for mpv while a video plays, the volume.
//!
//! The UI tells phones what the TV is doing through `phone_state` (the
//! keyboard is open, the screens are off, whose profile is on); phones get it
//! on every change, the keyboard's text only if they may type into it.
//! Paired phones live in ~/.local/share/azototv/phones.json (mode 600).
//! Nothing runs while no phone is connected.
//!
//! The iPhone app (a separate project) uses the same API, but
//! signs in without the TV: it lists the profiles that have a PIN
//! (`/api/app/profiles`) and logs into one with it (`/api/app/login`), which
//! pairs the phone with that profile like a code would. Profiles without a PIN
//! can't be opened from the network. `/api/app/home` tells it the profile's
//! apps (the TV's UI reports them with `phone_apps`, they live in its storage)
//! and `/api/app/games` the Mobile games.

use std::{
    collections::HashMap,
    ffi::CStr,
    fs,
    io::Read,
    net::Ipv4Addr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    body::Body,
    extract::{
        DefaultBodyLimit, Path as UrlPath, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{any, get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{io::AsyncWriteExt, sync::watch};

use crate::{games::Games, immich::Immich, player::Player, tv::Tv};

const PORT: u16 = 8686;
/// a pairing code works for this long after the TV shows it
const PAIR_TTL: Duration = Duration::from_secs(10 * 60);
/// wrong codes before the TV makes a new one
const PAIR_TRIES: u32 = 5;
/// over a video, "this phone is paired with..." shows at most this often
const TELL_EVERY: Duration = Duration::from_secs(8);
/// wrong PINs from the app in a row before that profile can't be opened from a phone for `LOGIN_WAIT`
const LOGIN_TRIES: u32 = 5;
const LOGIN_WAIT: Duration = Duration::from_secs(15 * 60);

/// A Jellyfin profile a phone is paired with.
#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Device {
    /// shown to the UI (the token never is)
    id: String,
    token: String,
    /// what the phone is ("iPhone", "Galaxy S23 di Luca")
    #[serde(default)]
    label: String,
    #[serde(default)]
    users: Vec<Account>,
    paired: u64,
}

impl Device {
    fn has(&self, user: &str) -> bool {
        !user.is_empty() && self.users.iter().any(|u| u.id == user)
    }

    fn names(&self) -> String {
        let n: Vec<&str> = self.users.iter().map(|u| u.name.as_str()).collect();
        match n.as_slice() {
            [] => String::new(),
            [one] => one.to_string(),
            [rest @ .., last] => format!("{} e {last}", rest.join(", ")),
        }
    }

    /// What the UI needs to check a request (and to name the phone's profiles).
    fn who(&self) -> Value {
        json!({ "id": self.id, "users": self.users })
    }
}

struct Pairing {
    code: String,
    until: Instant,
    tries: u32,
    /// the profile that showed the code
    user: Account,
}

/// What the TV's screen shows for the pairing (START > Collega un telefono).
#[derive(Serialize)]
pub struct PairingInfo {
    code: String,
    /// the page's address, with the code (the QR code opens it)
    url: String,
    /// the address alone, to type in
    address: String,
    /// the QR code of `url` as an SVG
    qr: String,
}

/// A phone on the pairing screen (paired with the profile that opened it).
#[derive(Serialize)]
pub struct PairedPhone {
    id: String,
    label: String,
    /// the phone's other profiles
    others: Vec<String>,
    paired: u64,
}

pub struct Phone {
    devices: Mutex<Vec<Device>>,
    pairing: Mutex<Option<Pairing>>,
    /// the TV's state, as phones see it: `text` (the keyboard is open:
    /// {value, label, secret}), `volume` (0-100, null = unknown), `dark`
    /// (screens off), `user` / `user_name` (the profile phones need, "" when
    /// none), `picker` (on the profile picker: every phone may move around)
    state: watch::Sender<Value>,
    /// the volume being set: presses add up even before the TV answers
    volume: tokio::sync::Mutex<Option<u32>>,
    /// when each phone was last told it's paired with other profiles
    told: Mutex<HashMap<String, Instant>>,
    /// the app's wrong PINs per profile: how many in a row, and the last one
    logins: Mutex<HashMap<String, (u32, Instant)>>,
    /// each profile's apps on its home, in order (from the TV's UI, `phone_apps`)
    apps: Mutex<HashMap<String, Vec<String>>>,
    /// this AzotoTV's id, so the app knows it found the same TV again
    server_id: String,
}

fn file() -> PathBuf {
    crate::games::data_dir().join("phones.json")
}

fn apps_file() -> PathBuf {
    crate::games::data_dir().join("phone-apps.json")
}

/// A random id made once and kept (`phone-server-id`).
fn server_id() -> String {
    let path = crate::games::data_dir().join("phone-server-id");
    if let Ok(id) = fs::read_to_string(&path) {
        let id = id.trim();
        if id.len() == 16 {
            return id.to_string();
        }
    }
    let id = random_hex(8);
    let _ = fs::create_dir_all(path.parent().unwrap());
    let _ = fs::write(&path, &id);
    id
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    if fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf)).is_err() {
        // never without randomness
        panic!("no /dev/urandom");
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn random_code() -> String {
    let n = u32::from_str_radix(&random_hex(4), 16).unwrap_or(0);
    format!("{:04}", n % 10_000)
}

/// May this phone act on the TV now (one of its profiles is on, or the picker)?
fn allowed(dev: &Device, s: &Value) -> bool {
    s["picker"] == true || dev.has(s["user"].as_str().unwrap_or(""))
}

impl Phone {
    pub fn new() -> Arc<Phone> {
        let devices = fs::read_to_string(file())
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|mut d| {
                // the first version had one profile, as `user` / `user_name`
                if d.get("users").is_none() {
                    let id = d["user"].as_str().unwrap_or("").to_string();
                    let name = d["user_name"].as_str().unwrap_or("").to_string();
                    d["users"] = if id.is_empty() { json!([]) } else { json!([{ "id": id, "name": name }]) };
                }
                if d["label"].as_str().is_none_or(str::is_empty) {
                    d["label"] = json!("Telefono");
                }
                serde_json::from_value::<Device>(d).ok()
            })
            .filter(|d| !d.users.is_empty())
            .collect();
        let (state, _) =
            watch::channel(json!({ "text": null, "volume": null, "dark": false, "user": "", "user_name": "", "picker": false }));
        Arc::new(Phone {
            devices: Mutex::new(devices),
            pairing: Mutex::new(None),
            state,
            volume: Default::default(),
            told: Mutex::new(HashMap::new()),
            logins: Mutex::new(HashMap::new()),
            apps: Mutex::new(fs::read_to_string(apps_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()),
            server_id: server_id(),
        })
    }

    fn save(&self, devices: &[Device]) {
        use std::os::unix::fs::OpenOptionsExt;
        let path = file();
        let _ = fs::create_dir_all(path.parent().unwrap());
        let tmp = path.with_extension("json.tmp");
        let ok = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .and_then(|mut f| std::io::Write::write_all(&mut f, serde_json::to_string_pretty(devices).unwrap().as_bytes()))
            .and_then(|_| fs::rename(&tmp, &path));
        if let Err(e) = ok {
            eprintln!("[phone] cannot save phones.json: {e}");
        }
    }

    fn device(&self, token: &str) -> Option<Device> {
        if token.is_empty() {
            return None;
        }
        self.devices.lock().unwrap().iter().find(|d| d.token == token).cloned()
    }

    /// Shows a new code for `user`'s profile (and keeps it for 10 minutes).
    pub fn start_pairing(&self, user: Account) -> PairingInfo {
        let code = random_code();
        *self.pairing.lock().unwrap() = Some(Pairing { code: code.clone(), until: Instant::now() + PAIR_TTL, tries: 0, user });
        let ip = lan_ips().into_iter().next().unwrap_or(Ipv4Addr::LOCALHOST);
        let port = port();
        let address = if port == 80 { format!("http://{ip}") } else { format!("http://{ip}:{port}") };
        let url = format!("{address}/#c={code}");
        if std::env::var_os("AZOTOTV_DEBUG").is_some() {
            eprintln!("[phone] pairing code {code} ({url})");
        }
        let qr = qrcode::QrCode::new(url.as_bytes())
            .map(|q| {
                q.render::<qrcode::render::svg::Color>()
                    .dark_color(qrcode::render::svg::Color("#000000"))
                    .light_color(qrcode::render::svg::Color("#ffffff"))
                    .quiet_zone(true)
                    .build()
            })
            .unwrap_or_default();
        PairingInfo { code, url, address, qr }
    }

    pub fn stop_pairing(&self) {
        *self.pairing.lock().unwrap() = None;
    }

    /// A phone typed `code`: paired with the profile that showed it. A phone
    /// that is already paired (`token`) gets that profile too.
    fn pair(&self, code: &str, token: &str, label: &str) -> Result<Device, &'static str> {
        let user = {
            let mut p = self.pairing.lock().unwrap();
            let Some(cur) = p.as_mut().filter(|c| Instant::now() < c.until) else {
                return Err("Apri «Collega un telefono» sulla TV");
            };
            if code.trim() != cur.code {
                cur.tries += 1;
                if cur.tries >= PAIR_TRIES {
                    *p = None;
                    return Err("Troppi tentativi: apri di nuovo «Collega un telefono» sulla TV");
                }
                return Err("Codice sbagliato");
            }
            p.take().unwrap().user
        };
        Ok(self.add_user(token, label, user))
    }

    /// `user` on the phone with `token` (a new phone when it has none, or an unknown one).
    fn add_user(&self, token: &str, label: &str, user: Account) -> Device {
        let label: String = label.trim().chars().take(40).collect();
        let mut list = self.devices.lock().unwrap();
        let dev = match list.iter_mut().find(|d| !token.is_empty() && d.token == token) {
            Some(d) => {
                if !d.has(&user.id) {
                    d.users.push(user);
                }
                if !label.is_empty() {
                    d.label = label;
                }
                d.clone()
            }
            None => {
                let d = Device {
                    id: random_hex(4),
                    token: random_hex(24),
                    label: if label.is_empty() { "Telefono".into() } else { label },
                    users: vec![user],
                    paired: now(),
                };
                list.push(d.clone());
                d
            }
        };
        self.save(&list);
        dev
    }

    /// The app opens `user`'s profile with its PIN. Only profiles with a PIN;
    /// after `LOGIN_TRIES` wrong ones the profile is closed to phones for `LOGIN_WAIT`.
    fn login(&self, user: Account, pin: &str, token: &str, label: &str) -> Result<Device, String> {
        if !crate::profile::has_pin(&user.id) {
            return Err("Questo profilo non ha un PIN: mettilo dalla TV (START › PIN del profilo)".into());
        }
        {
            let mut logins = self.logins.lock().unwrap();
            if let Some(&(n, at)) = logins.get(&user.id) {
                if at.elapsed() >= LOGIN_WAIT {
                    logins.remove(&user.id);
                } else if n >= LOGIN_TRIES {
                    let mins = (LOGIN_WAIT - at.elapsed()).as_secs().div_ceil(60);
                    return Err(format!("Troppi tentativi: riprova tra {mins} minuti"));
                }
            }
        }
        if crate::profile::check_pin(&user.id, pin)? {
            self.logins.lock().unwrap().remove(&user.id);
            Ok(self.add_user(token, label, user))
        } else {
            let mut logins = self.logins.lock().unwrap();
            let e = logins.entry(user.id.clone()).or_insert((0, Instant::now()));
            e.0 += 1;
            e.1 = Instant::now();
            if e.0 >= LOGIN_TRIES {
                eprintln!("[phone] {} wrong PINs for {} from a phone: closed to phones for a while", e.0, user.name);
                return Err(format!("PIN sbagliato. Troppi tentativi: riprova tra {} minuti", LOGIN_WAIT.as_secs() / 60));
            }
            Err("PIN sbagliato".into())
        }
    }

    /// The phone with `token` signs out of `user`'s profile (and is forgotten with its last one).
    fn logout(&self, token: &str, user: &str) -> Option<String> {
        let mut list = self.devices.lock().unwrap();
        let id = list.iter().find(|d| d.token == token).map(|d| d.id.clone());
        for d in list.iter_mut().filter(|d| d.token == token) {
            d.users.retain(|u| u.id != user);
        }
        list.retain(|d| !d.users.is_empty());
        self.save(&list);
        id
    }

    /// The TV's UI says which apps `user` has on the home, in order.
    pub fn set_apps(&self, user: &str, apps: Vec<String>) {
        if user.is_empty() {
            return;
        }
        let mut all = self.apps.lock().unwrap();
        if all.get(user) == Some(&apps) {
            return;
        }
        all.insert(user.to_string(), apps);
        if let Ok(t) = serde_json::to_string_pretty(&*all) {
            let _ = fs::write(apps_file(), t);
        }
    }

    /// `user`'s apps; a profile the TV hasn't reported yet has what every profile starts with.
    fn apps_of(&self, user: &str) -> Vec<String> {
        self.apps.lock().unwrap().get(user).cloned().unwrap_or_else(|| vec!["live".into(), "store".into()])
    }

    /// The phones paired with `user`'s profile.
    pub fn list(&self, user: &str) -> Vec<PairedPhone> {
        self.devices
            .lock()
            .unwrap()
            .iter()
            .filter(|d| d.has(user))
            .map(|d| PairedPhone {
                id: d.id.clone(),
                label: d.label.clone(),
                others: d.users.iter().filter(|u| u.id != user).map(|u| u.name.clone()).collect(),
                paired: d.paired,
            })
            .collect()
    }

    /// The phone is no longer paired with `user`'s profile (nor at all, if that was its last one).
    pub fn forget(&self, id: &str, user: &str) {
        let mut list = self.devices.lock().unwrap();
        for d in list.iter_mut().filter(|d| d.id == id) {
            d.users.retain(|u| u.id != user);
        }
        list.retain(|d| !d.users.is_empty());
        self.save(&list);
    }

    /// The UI's part of the state (`text`, `dark`, `user`, `user_name`, `picker`), merged in.
    pub fn set_state(&self, part: Value) {
        let Value::Object(part) = part else { return };
        self.state.send_modify(|s| {
            for (k, v) in part {
                if k != "volume" {
                    s[k] = v;
                }
            }
        });
    }

    /// The TV's page made for touch, when one is open (the UI's `game` with a
    /// screen while no game runs). */
    fn touch_page(&self) -> Option<String> {
        let s = self.state.borrow();
        let g = &s["game"];
        (g["screen"] == true).then(|| g["name"].as_str().unwrap_or("").to_string())
    }

    fn set_volume_state(&self, v: Option<u32>) {
        self.state.send_modify(|s| s["volume"] = json!(v));
    }

    /// Asks the TV for its volume (a phone just connected).
    async fn refresh_volume(&self, tv: &Tv) {
        if !tv.enabled() {
            return;
        }
        let mut cur = self.volume.lock().await;
        let v = tv.volume().await.ok();
        *cur = v;
        self.set_volume_state(v);
    }

    /// One step up or down.
    async fn change_volume(&self, tv: &Tv, delta: i32) {
        let mut cur = self.volume.lock().await;
        let from = match *cur {
            Some(v) => v,
            None => match tv.volume().await {
                Ok(v) => v,
                Err(_) => return self.set_volume_state(None),
            },
        };
        let to = (from as i32 + delta).clamp(0, 100) as u32;
        match tv.set_volume(to).await {
            Ok(()) => {
                *cur = Some(to);
                self.set_volume_state(Some(to));
            }
            Err(e) => {
                eprintln!("[phone] volume: {e}");
                *cur = None;
                self.set_volume_state(None);
            }
        }
    }

    /// Over a video (the UI can't ask there): a note that the phone belongs to other profiles.
    fn tell(&self, app: &AppHandle, dev: &Device) {
        {
            let mut told = self.told.lock().unwrap();
            if told.get(&dev.id).is_some_and(|t| t.elapsed() < TELL_EVERY) {
                return;
            }
            told.insert(dev.id.clone(), Instant::now());
        }
        let on = self.state.borrow()["user_name"].as_str().unwrap_or("").to_string();
        let toast = crate::overlay::Toast {
            kicker: "Telefono".into(),
            title: format!("Questo telefono è collegato a {}", dev.names()),
            body: if on.is_empty() { String::new() } else { format!("Sulla TV c'è il profilo di {on}") },
            image: None,
            logo: false,
        };
        if !crate::overlay::show(app, toast) {
            let _ = app.emit("phone-denied", dev.who());
        }
    }

    /// The name a phone's request goes under: the TV's profile if it's one of the phone's.
    fn from(&self, dev: &Device) -> String {
        let s = self.state.borrow();
        let on = s["user"].as_str().unwrap_or("");
        dev.users.iter().find(|u| u.id == on).or(dev.users.first()).map(|u| u.name.clone()).unwrap_or_default()
    }

    pub fn start(self: &Arc<Self>, app: AppHandle) {
        let ctx = Ctx { app, phone: self.clone() };
        let router = Router::new()
            .route("/", get(page))
            .route("/manifest.webmanifest", get(manifest))
            .route("/{file}", get(static_file))
            .route("/api/hello", get(hello))
            .route("/api/pair", post(pair))
            .route("/api/me", get(me))
            .route("/api/send", post(send))
            .route("/api/albums", get(albums).post(new_album))
            .route("/api/thumb/{id}", get(thumb))
            .route("/api/upload", post(upload).layer(DefaultBodyLimit::disable()))
            .route("/api/show", post(show))
            .route("/api/added", post(added))
            .route("/api/diag", post(diag))
            .route("/api/app/profiles", get(app_profiles))
            .route("/api/app/avatar/{id}", get(app_avatar))
            .route("/api/app/login", post(app_login))
            .route("/api/app/logout", post(app_logout))
            .route("/api/app/home", get(app_home))
            .route("/api/app/games", get(app_games))
            .route("/api/app/art/{id}/{kind}", get(app_art))
            .route("/api/app/album/{id}", get(app_album))
            .route("/api/app/original/{id}", get(app_original))
            .route("/api/app/play/stop", post(app_play_stop))
            .route("/api/app/screen", post(app_screen))
            .route("/api/app/screen/ready", post(app_screen_ready))
            .route("/api/app/screen/stop", post(app_screen_stop))
            .route("/api/app/play/{id}", post(app_play))
            .route("/api/app/play/{id}/go", post(app_play_go))
            .route("/api/app/jf/{k}/{user}/{*rest}", any(app_jf))
            .route("/api/app/tv/watch", post(app_tv_watch))
            .route("/api/app/tv/take", post(app_tv_take))
            .route("/api/app/seerr", post(app_seerr))
            .route("/api/app/seerr/status", get(app_seerr_status))
            .route("/api/app/dl", get(app_dl))
            .route("/ws", get(ws))
            .with_state(ctx);
        tauri::async_runtime::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(("0.0.0.0", port())).await {
                Ok(l) => l,
                Err(e) => return eprintln!("[phone] cannot listen on port {}: {e}", port()),
            };
            eprintln!("[phone] listening on port {}", port());
            if let Err(e) = axum::serve(listener, router).await {
                eprintln!("[phone] server stopped: {e}");
            }
        });
    }
}

fn port() -> u16 {
    std::env::var("AZOTOTV_PHONE_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(PORT)
}

/// The server's LAN addresses, wired first (the phone page's address).
fn lan_ips() -> Vec<Ipv4Addr> {
    let mut out: Vec<(bool, Ipv4Addr)> = Vec::new();
    unsafe {
        let mut ifs: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifs) != 0 {
            return vec![];
        }
        let mut cur = ifs;
        while !cur.is_null() {
            let ifa = &*cur;
            cur = ifa.ifa_next;
            if ifa.ifa_addr.is_null() || (*ifa.ifa_addr).sa_family as i32 != libc::AF_INET {
                continue;
            }
            let name = CStr::from_ptr(ifa.ifa_name).to_string_lossy().to_string();
            if ["lo", "docker", "br-", "veth", "virbr", "tailscale", "tun", "wg"].iter().any(|p| name.starts_with(p)) {
                continue;
            }
            let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
            let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
            if !ip.is_private() {
                continue;
            }
            let wireless = std::path::Path::new("/sys/class/net").join(&name).join("wireless").exists();
            out.push((wireless, ip));
        }
        libc::freeifaddrs(ifs);
    }
    out.sort_by_key(|(wireless, _)| *wireless);
    out.into_iter().map(|(_, ip)| ip).collect()
}

/// A YouTube video id in a shared link (watch, youtu.be, Shorts, Music, embeds).
fn youtube_id(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.trim_start_matches("www.").trim_start_matches("m.").trim_start_matches("music.");
    let valid = |id: &str| id.len() == 11 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let id = match host {
        "youtu.be" => path.split(['?', '/', '#']).next()?.to_string(),
        "youtube.com" | "youtube-nocookie.com" => {
            let (p, query) = path.split_once('?').unwrap_or((path, ""));
            let mut seg = p.split('/');
            match seg.next()? {
                "watch" => query.split('&').find_map(|kv| kv.strip_prefix("v="))?.split('#').next()?.to_string(),
                "shorts" | "embed" | "live" | "v" => seg.next()?.split('#').next()?.to_string(),
                _ => return None,
            }
        }
        _ => return None,
    };
    valid(&id).then_some(id)
}

/// The first http(s) link in what a phone shared ("Look at this! https://...").
fn first_link(text: &str) -> Option<String> {
    let start = text.find("https://").or_else(|| text.find("http://"))?;
    let link: String = text[start..].chars().take_while(|c| !c.is_whitespace()).collect();
    Some(link.trim_end_matches(['.', ',', ')', '"', '\'']).to_string())
}

/// Milliseconds since 1970 as an ISO date (what Immich wants).
fn iso(ms: u64) -> String {
    let secs = ms / 1000;
    let (y, m, d) = crate::immich::civil_from_days((secs / 86_400) as i64);
    let t = secs % 86_400;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", t / 3600, t / 60 % 60, t % 60, ms % 1000)
}

// ---- the web server ------------------------------------------------------

#[derive(Clone)]
struct Ctx {
    app: AppHandle,
    phone: Arc<Phone>,
}

type Params = Query<HashMap<String, String>>;

fn fail(code: StatusCode, msg: &str) -> Response {
    (code, Json(json!({ "error": msg }))).into_response()
}

/// The phone behind a request: its token comes as `x-azoto-key` or `?k=`
/// (images and the WebSocket can't send headers).
fn auth(ctx: &Ctx, headers: &HeaderMap, q: &HashMap<String, String>) -> Result<Device, Response> {
    let token = headers.get("x-azoto-key").and_then(|v| v.to_str().ok()).or(q.get("k").map(String::as_str)).unwrap_or("");
    ctx.phone.device(token).ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "Telefono non collegato"))
}

/// The web app's files: built in, or from AZOTOTV_PHONE_DIR while working on them.
fn asset(name: &str) -> Option<(Vec<u8>, &'static str)> {
    let mime = match name.rsplit('.').next()? {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "apk" => "application/vnd.android.package-archive",
        _ => return None,
    };
    if let Some(dir) = std::env::var_os("AZOTOTV_PHONE_DIR") {
        if !name.contains('/') && !name.starts_with('.') {
            return fs::read(PathBuf::from(dir).join(name)).ok().map(|d| (d, mime));
        }
    }
    let data: &[u8] = match name {
        "index.html" => include_bytes!("../phone/index.html"),
        "app.js" => include_bytes!("../phone/app.js"),
        "app.css" => include_bytes!("../phone/app.css"),
        "icon.svg" => include_bytes!("../phone/icon.svg"),
        "icon-192.png" => include_bytes!("../phone/icon-192.png"),
        "icon-512.png" => include_bytes!("../phone/icon-512.png"),
        "apple-touch-icon.png" => include_bytes!("../phone/apple-touch-icon.png"),
        // the Android app (android/build.sh; not in the repo)
        "AzotoTV.apk" => include_bytes!("../phone/AzotoTV.apk"),
        _ => return None,
    };
    // an empty file stands in for an Android app that wasn't built (build.rs)
    (!data.is_empty()).then(|| (data.to_vec(), mime))
}

fn serve(name: &str) -> Response {
    match asset(name) {
        // no-cache: phones check for a new version every time (a quick 304)
        Some((data, mime)) => ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache")], data).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn page() -> Response {
    serve("index.html")
}

async fn static_file(UrlPath(file): UrlPath<String>) -> Response {
    serve(&file)
}

/// The home-screen app opens with the phone's token (an iPhone's home-screen
/// app doesn't share Safari's storage).
async fn manifest(State(ctx): State<Ctx>, Query(q): Params) -> Response {
    let start = match q.get("k").filter(|k| ctx.phone.device(k).is_some()) {
        Some(k) => format!("/?k={k}"),
        None => "/".into(),
    };
    let m = json!({
        "name": "AzotoTV",
        "short_name": "AzotoTV",
        "start_url": start,
        "scope": "/",
        "display": "standalone",
        "background_color": "#07080f",
        "theme_color": "#07080f",
        "icons": [
            { "src": "/icon-192.png", "sizes": "192x192", "type": "image/png" },
            { "src": "/icon-512.png", "sizes": "512x512", "type": "image/png" },
        ],
    });
    ([(header::CONTENT_TYPE, "application/manifest+json")], m.to_string()).into_response()
}

/// For the Android app looking for the TV on the network; `server` tells the
/// iPhone app it's the same AzotoTV it signed into.
async fn hello(State(ctx): State<Ctx>) -> Json<Value> {
    Json(json!({ "app": "azototv", "server": ctx.phone.server_id, "api": 2 }))
}

#[derive(Deserialize)]
struct PairReq {
    code: String,
    /// what the phone is ("iPhone", the Android phone's name)
    #[serde(default)]
    device: String,
}

/// A phone typed the code: a new phone gets its token, a paired one
/// (`x-azoto-key`) one more profile.
async fn pair(State(ctx): State<Ctx>, headers: HeaderMap, Json(r): Json<PairReq>) -> Response {
    let token = headers.get("x-azoto-key").and_then(|v| v.to_str().ok()).unwrap_or("");
    match ctx.phone.pair(&r.code, token, &r.device) {
        Ok(d) => {
            eprintln!("[phone] paired {} with {}", d.id, d.names());
            let _ = ctx.app.emit("phone-paired", d.who());
            Json(json!({ "token": d.token, "label": d.label, "users": d.users })).into_response()
        }
        Err(msg) => fail(StatusCode::FORBIDDEN, msg),
    }
}

async fn me(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params) -> Response {
    match auth(&ctx, &headers, &q) {
        Ok(d) => Json(json!({ "label": d.label, "users": d.users })).into_response(),
        Err(r) => r,
    }
}

#[derive(Deserialize)]
struct SendReq {
    url: String,
}

/// "Manda alla TV": the UI plays it (YouTube through the YouTube app, so it
/// lands in the profile's history; anything else through yt-dlp). `ask`: the
/// TV is on another profile, it asks first.
async fn send(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, Json(r): Json<SendReq>) -> Response {
    let d = match auth(&ctx, &headers, &q) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let Some(url) = first_link(&r.url) else { return fail(StatusCode::BAD_REQUEST, "Non è un link") };
    if ctx.app.state::<Arc<Games>>().running().is_some() {
        return fail(StatusCode::CONFLICT, "Sulla TV c'è un gioco aperto");
    }
    let mut ev = match youtube_id(&url) {
        Some(id) => json!({ "kind": "youtube", "id": id }),
        None => {
            let host = url.split_once("://").map(|(_, r)| r.split(['/', '?']).next().unwrap_or("")).unwrap_or("");
            json!({ "kind": "url", "url": url, "host": host.trim_start_matches("www.") })
        }
    };
    ev["from"] = json!(ctx.phone.from(&d));
    ev["phone"] = d.who();
    let kind = ev["kind"].clone();
    let ask = !d.has(ctx.phone.state.borrow()["user"].as_str().unwrap_or(""));
    let _ = ctx.app.emit("phone-send", ev);
    Json(json!({ "ok": true, "kind": kind, "ask": ask })).into_response()
}

async fn albums(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params) -> Response {
    if let Err(r) = auth(&ctx, &headers, &q) {
        return r;
    }
    let im = ctx.app.state::<Arc<Immich>>().inner().clone();
    if !im.can_upload() {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "Aggiunta di foto non configurata sulla TV");
    }
    match im.all_albums(true).await {
        Ok(list) => Json(list).into_response(),
        Err(e) => fail(StatusCode::BAD_GATEWAY, &e),
    }
}

#[derive(Deserialize)]
struct AlbumReq {
    name: String,
}

async fn new_album(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, Json(r): Json<AlbumReq>) -> Response {
    if let Err(r) = auth(&ctx, &headers, &q) {
        return r;
    }
    let name = r.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return fail(StatusCode::BAD_REQUEST, "Nome dell'album non valido");
    }
    let im = ctx.app.state::<Arc<Immich>>().inner().clone();
    match im.create_album(name).await {
        Ok(a) => Json(a).into_response(),
        Err(e) => fail(StatusCode::BAD_GATEWAY, &e),
    }
}

async fn thumb(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, UrlPath(id): UrlPath<String>) -> Response {
    if let Err(r) = auth(&ctx, &headers, &q) {
        return r;
    }
    let im = ctx.app.state::<Arc<Immich>>().inner().clone();
    let size = if q.get("size").map(String::as_str) == Some("preview") { "preview" } else { "thumbnail" };
    match im.image(&format!("/{id}/{size}")).await {
        Ok((data, mime)) => ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "private, max-age=86400".into())], data).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// One photo or video, as the request body (`?album=&name=&modified=<ms>`):
/// saved to a temporary file, then uploaded to Immich and added to the album.
async fn upload(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, body: Body) -> Response {
    if let Err(r) = auth(&ctx, &headers, &q) {
        return r;
    }
    let im = ctx.app.state::<Arc<Immich>>().inner().clone();
    if !im.can_upload() {
        return fail(StatusCode::SERVICE_UNAVAILABLE, "Aggiunta di foto non configurata sulla TV");
    }
    let mime = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("application/octet-stream").to_string();
    if !(mime.starts_with("image/") || mime.starts_with("video/")) {
        return fail(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Solo foto e video");
    }
    // only the file name, never a path
    let name: String = q.get("name").map(|n| n.rsplit(['/', '\\']).next().unwrap_or("").to_string()).unwrap_or_default();
    let name = if name.is_empty() || name.starts_with('.') { "foto.jpg".to_string() } else { name };
    let modified = q.get("modified").and_then(|m| m.parse::<u64>().ok()).map(iso).unwrap_or_else(|| iso(now() * 1000));

    let dir = crate::jellyfin::cache_dir().join("phone-upload");
    let _ = fs::create_dir_all(&dir);
    let tmp = dir.join(random_hex(8));
    let res = async {
        let mut f = tokio::fs::File::create(&tmp).await.map_err(|e| e.to_string())?;
        let mut stream = body.into_data_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "Invio interrotto".to_string())?;
            f.write_all(&chunk).await.map_err(|e| e.to_string())?;
        }
        f.flush().await.map_err(|e| e.to_string())?;
        drop(f);
        let (id, duplicate) = im.upload(&tmp, &name, &mime, &modified).await?;
        if let Some(album) = q.get("album").filter(|a| !a.is_empty()) {
            im.add_to_album(album, std::slice::from_ref(&id)).await?;
        }
        Ok::<_, String>((id, duplicate))
    }
    .await;
    let _ = tokio::fs::remove_file(&tmp).await;
    match res {
        Ok((id, duplicate)) => Json(json!({ "id": id, "duplicate": duplicate })).into_response(),
        Err(e) => {
            eprintln!("[phone] upload {name}: {e}");
            fail(StatusCode::BAD_GATEWAY, &e)
        }
    }
}

#[derive(Deserialize)]
struct ShowReq {
    #[serde(default)]
    album: Option<String>,
    #[serde(default)]
    ids: Vec<String>,
}

/// "Mostra sulla TV": the photos just sent, or a whole album, as a slideshow.
async fn show(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, Json(r): Json<ShowReq>) -> Response {
    let d = match auth(&ctx, &headers, &q) {
        Ok(d) => d,
        Err(r) => return r,
    };
    if ctx.app.state::<Arc<Games>>().running().is_some() {
        return fail(StatusCode::CONFLICT, "Sulla TV c'è un gioco aperto");
    }
    let ids: Vec<String> = r.ids.into_iter().take(200).collect();
    if ids.is_empty() && r.album.is_none() {
        return fail(StatusCode::BAD_REQUEST, "Niente da mostrare");
    }
    let im = ctx.app.state::<Arc<Immich>>().inner().clone();
    let assets = if ids.is_empty() { vec![] } else { im.assets_by_id(&ids).await };
    let ask = !d.has(ctx.phone.state.borrow()["user"].as_str().unwrap_or(""));
    let _ = ctx.app.emit("phone-show", json!({ "album": r.album, "assets": assets, "from": ctx.phone.from(&d), "phone": d.who() }));
    Json(json!({ "ok": true, "ask": ask })).into_response()
}

#[derive(Deserialize)]
struct AddedReq {
    album: String,
    n: u32,
}

/// A phone finished adding photos: a note on the TV, and the Galleria reloads.
async fn added(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, Json(r): Json<AddedReq>) -> Response {
    let d = match auth(&ctx, &headers, &q) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let _ = ctx.app.emit("phone-added", json!({ "album": r.album, "n": r.n, "from": ctx.phone.from(&d) }));
    Json(json!({ "ok": true })).into_response()
}

/// A phone's screen as its browser sees it (sizes, safe areas), into the log:
/// for layout problems that only show on a real phone.
async fn diag(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, Json(info): Json<Value>) -> Response {
    let d = match auth(&ctx, &headers, &q) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let s = info.to_string();
    eprintln!("[phone] {} screen: {}", d.id, s.chars().take(800).collect::<String>());
    Json(json!({ "ok": true })).into_response()
}

// ---- the iPhone app ----------------------------------------------------------

/// The profiles the app can open: only those with a PIN.
async fn app_profiles(State(ctx): State<Ctx>) -> Response {
    let jf = ctx.app.state::<Arc<crate::jellyfin::Jf>>().inner().clone();
    match jf.profiles().await {
        Ok(list) => {
            let v: Vec<Value> = list
                .profiles
                .into_iter()
                .filter(|p| crate::profile::has_pin(&p.id))
                .map(|p| json!({ "id": p.id, "name": p.name, "image": p.image_tag.is_some() }))
                .collect();
            Json(v).into_response()
        }
        Err(e) => fail(StatusCode::BAD_GATEWAY, &e),
    }
}

/// A profile's picture, from Jellyfin (public there too).
async fn app_avatar(State(ctx): State<Ctx>, UrlPath(id): UrlPath<String>) -> Response {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return StatusCode::NOT_FOUND.into_response();
    }
    let jf = ctx.app.state::<Arc<crate::jellyfin::Jf>>().inner().clone();
    let Some(base) = jf.server() else { return StatusCode::NOT_FOUND.into_response() };
    let url = format!("{base}/Users/{id}/Images/Primary?maxHeight=256&quality=90");
    let res = async {
        let r = reqwest::Client::new().get(url).timeout(Duration::from_secs(8)).send().await.ok()?.error_for_status().ok()?;
        let mime = r.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("image/jpeg").to_string();
        Some((mime, r.bytes().await.ok()?))
    }
    .await;
    match res {
        Some((mime, data)) => ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "max-age=3600".into())], data).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize)]
struct LoginReq {
    user: String,
    pin: String,
    /// what the phone is ("iPhone di Ada")
    #[serde(default)]
    device: String,
}

/// The app opens a profile with its PIN: a new phone gets its token, a known one (`x-azoto-key`) one more profile.
async fn app_login(State(ctx): State<Ctx>, headers: HeaderMap, Json(r): Json<LoginReq>) -> Response {
    let token = headers.get("x-azoto-key").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let jf = ctx.app.state::<Arc<crate::jellyfin::Jf>>().inner().clone();
    let profiles = match jf.profiles().await {
        Ok(p) => p.profiles,
        Err(e) => return fail(StatusCode::BAD_GATEWAY, &e),
    };
    let Some(p) = profiles.into_iter().find(|p| p.id == r.user) else { return fail(StatusCode::NOT_FOUND, "Profilo non trovato") };
    let user = Account { id: p.id, name: p.name };
    let phone = ctx.phone.clone();
    let res = tauri::async_runtime::spawn_blocking(move || phone.login(user, &r.pin, &token, &r.device)).await;
    match res {
        Ok(Ok(d)) => {
            eprintln!("[phone] app {} signed into {}", d.id, d.names());
            let _ = ctx.app.emit("phone-paired", d.who());
            Json(json!({ "token": d.token, "id": d.id, "users": d.users, "server": ctx.phone.server_id })).into_response()
        }
        Ok(Err(msg)) => fail(StatusCode::FORBIDDEN, &msg),
        Err(_) => fail(StatusCode::INTERNAL_SERVER_ERROR, "errore"),
    }
}

#[derive(Deserialize)]
struct LogoutReq {
    user: String,
}

async fn app_logout(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, Json(r): Json<LogoutReq>) -> Response {
    let d = match auth(&ctx, &headers, &q) {
        Ok(d) => d,
        Err(r) => return r,
    };
    if let Some(id) = ctx.phone.logout(&d.token, &r.user) {
        crate::phonejf::forget(&ctx.app.state::<Arc<crate::jellyfin::Jf>>(), &id, &r.user);
    }
    Json(json!({ "ok": true })).into_response()
}

/// What the app's home shows for `?user=` (one of the phone's profiles).
async fn app_home(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params) -> Response {
    let d = match auth(&ctx, &headers, &q) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let user = q.get("user").map(String::as_str).unwrap_or("");
    if !d.has(user) {
        return fail(StatusCode::FORBIDDEN, "Profilo non collegato a questo telefono");
    }
    Json(json!({ "apps": ctx.phone.apps_of(user), "users": d.users })).into_response()
}

async fn app_games(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params) -> Response {
    if let Err(r) = auth(&ctx, &headers, &q) {
        return r;
    }
    let games = tauri::async_runtime::spawn_blocking(crate::games::phone_games).await.unwrap_or_default();
    Json(games).into_response()
}

async fn app_art(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, UrlPath((id, kind)): UrlPath<(String, String)>) -> Response {
    if let Err(r) = auth(&ctx, &headers, &q) {
        return r;
    }
    match crate::games::art(&format!("{id}/{kind}")) {
        Ok((data, mime)) => ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "private, max-age=86400".into())], data).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// One page of an album's photos and videos (`?page=`), for the phone's Galleria.
async fn app_album(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, UrlPath(id): UrlPath<String>) -> Response {
    if let Err(r) = auth(&ctx, &headers, &q) {
        return r;
    }
    let im = ctx.app.state::<Arc<Immich>>().inner().clone();
    let page = q.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
    match im.assets(&id, page).await {
        Ok(p) => Json(p).into_response(),
        Err(e) => fail(StatusCode::BAD_GATEWAY, &e),
    }
}

#[derive(Deserialize)]
struct PlayReq {
    user: String,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    fps: u32,
}

/// A Mobile game in its phone mode, part 1: the stream (phonestream.rs). The
/// phone connects to it, then calls `/go`.
async fn app_play(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, UrlPath(id): UrlPath<String>, Json(r): Json<PlayReq>) -> Response {
    let d = match auth(&ctx, &headers, &q) {
        Ok(d) => d,
        Err(r) => return r,
    };
    if !d.has(&r.user) {
        return fail(StatusCode::FORBIDDEN, "Profilo non collegato a questo telefono");
    }
    let games = tauri::async_runtime::spawn_blocking(crate::games::phone_games).await.unwrap_or_default();
    if !games.iter().any(|g| g.id == id && g.stream) {
        return fail(StatusCode::NOT_FOUND, "Questo gioco non si può giocare sul telefono");
    }
    if let Some(g) = ctx.app.state::<Arc<Games>>().running() {
        let name = games.iter().find(|x| x.id == g).map(|x| x.name.clone()).unwrap_or(g);
        return fail(StatusCode::CONFLICT, &format!("Sulla TV c'è già {name}"));
    }
    if ctx.app.state::<Arc<Player>>().active() {
        return fail(StatusCode::CONFLICT, "Sulla TV si sta guardando qualcosa");
    }
    // the address the phone reached us at: the stream goes the same way
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|h| h.rsplit_once(':').map_or(h, |(a, _)| a).to_string())
        .unwrap_or_else(|| lan_ips().into_iter().next().unwrap_or(Ipv4Addr::LOCALHOST).to_string());
    // the panel is 1920x1080: the phone shows it as it is (the game is 16:9)
    let (w, h) = (if r.width == 0 { 1920 } else { r.width.min(1920) }, if r.height == 0 { 1080 } else { r.height.min(1080) });
    let fps = if r.fps == 0 { 60 } else { r.fps.min(60) };
    match crate::phonestream::start(&host, w, h, fps).await {
        Ok(session) => {
            eprintln!("[phone] {} streams {id} ({w}x{h}@{fps})", d.names());
            Json(session).into_response()
        }
        Err(e) => {
            eprintln!("[phone] stream for {id}: {e}");
            fail(StatusCode::SERVICE_UNAVAILABLE, &e)
        }
    }
}

/// Part 2, once the phone is connected: the game starts in its phone mode on
/// the streamed screen.
async fn app_play_go(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, UrlPath(id): UrlPath<String>, Json(r): Json<PlayReq>) -> Response {
    let d = match auth(&ctx, &headers, &q) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let Some(user) = d.users.iter().find(|u| u.id == r.user).cloned() else {
        return fail(StatusCode::FORBIDDEN, "Profilo non collegato a questo telefono");
    };
    // stream.rs reads Sunshine's log once a second
    for _ in 0..80 {
        if crate::stream::active() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !crate::stream::active() {
        return fail(StatusCode::CONFLICT, "Lo stream non è partito");
    }
    let name = crate::games::phone_games().into_iter().find(|g| g.id == id).map(|g| g.name).unwrap_or_else(|| id.clone());
    let (app, games, game) = (ctx.app.clone(), ctx.app.state::<Arc<Games>>().inner().clone(), id.clone());
    // the TV's UI shows the loading screen, as for a game started there
    let _ = ctx.app.emit("game-started", json!({ "id": id, "name": name }));
    let launched = tauri::async_runtime::spawn_blocking(move || games.launch(app, &user.id, &user.name, &game, None, None, true)).await;
    match launched.map_err(|e| e.to_string()).and_then(|r| r) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => {
            eprintln!("[phone] launching {id}: {e}");
            let _ = ctx.app.emit("game-ended", json!({ "id": id, "seconds": 0, "reason": "error", "error": e }));
            fail(StatusCode::INTERNAL_SERVER_ERROR, &e)
        }
    }
}

#[derive(Deserialize)]
struct ScreenReq {
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    fps: u32,
}

/// The phone as the touch screen of the Switch game on the TV (phonescreen.rs):
/// the stream of its picture. The phone connects, then calls `/ready`.
async fn app_screen(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, Json(r): Json<ScreenReq>) -> Response {
    let d = match auth(&ctx, &headers, &q) {
        Ok(d) => d,
        Err(r) => return r,
    };
    // a Switch game, or a page of the TV made for touch (the Mii editor)
    let games = ctx.app.state::<Arc<Games>>();
    let Some(id) = games.screen_game().or_else(|| games.running().is_none().then(|| ctx.phone.touch_page()).flatten()) else {
        return fail(StatusCode::CONFLICT, "Sulla TV non c'è un gioco Switch");
    };
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|h| h.rsplit_once(':').map_or(h, |(a, _)| a).to_string())
        .unwrap_or_else(|| lan_ips().into_iter().next().unwrap_or(Ipv4Addr::LOCALHOST).to_string());
    let (w, h) = (if r.width == 0 { 1920 } else { r.width.min(1920) }, if r.height == 0 { 1080 } else { r.height.min(1080) });
    let fps = if r.fps == 0 { 60 } else { r.fps.min(60) };
    match crate::phonescreen::start(&host, w, h, fps).await {
        Ok(session) => {
            eprintln!("[phone] {} is the touch screen of {id} ({w}x{h}@{fps})", d.names());
            // a 3DS game: where its bottom screen is in the picture
            let mut v = serde_json::to_value(&session).unwrap_or_default();
            if let Some(crop) = games.screen_crop() {
                v["crop"] = json!(crop);
            }
            Json(v).into_response()
        }
        Err(e) => {
            eprintln!("[phone] touch screen for {id}: {e}");
            fail(StatusCode::SERVICE_UNAVAILABLE, &e)
        }
    }
}

/// The phone is connected: its touches go to the game's screen.
async fn app_screen_ready(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params) -> Response {
    if let Err(r) = auth(&ctx, &headers, &q) {
        return r;
    }
    crate::phonescreen::ready().await;
    Json(json!({ "ok": true })).into_response()
}

/// The phone closed its screen; the game goes on.
async fn app_screen_stop(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params) -> Response {
    if let Err(r) = auth(&ctx, &headers, &q) {
        return r;
    }
    crate::phonescreen::stop().await;
    Json(json!({ "ok": true })).into_response()
}

/// The phone closed the game: it ends (saving, SIGTERM) and so does Sunshine's session.
async fn app_play_stop(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params) -> Response {
    if let Err(r) = auth(&ctx, &headers, &q) {
        return r;
    }
    ctx.app.state::<Arc<Games>>().stop_phone();
    crate::phonestream::cancel().await;
    Json(json!({ "ok": true })).into_response()
}

/// The app's Jellyfin, as the phone's own login for `user` (phonejf.rs). The key is in
/// the path: the video player can't send headers, and HLS playlists link relatively.
async fn app_jf(
    State(ctx): State<Ctx>,
    method: Method,
    UrlPath((k, user, rest)): UrlPath<(String, String, String)>,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let Some(d) = ctx.phone.device(&k) else { return fail(StatusCode::UNAUTHORIZED, "Telefono non collegato") };
    if !d.has(&user) {
        return fail(StatusCode::FORBIDDEN, "Profilo non collegato a questo telefono");
    }
    let jf = ctx.app.state::<Arc<crate::jellyfin::Jf>>().inner().clone();
    crate::phonejf::proxy(&jf, &d.id, &d.label, &user, method, &rest, uri.query(), &headers, body.to_vec()).await
}

#[derive(Deserialize)]
struct WatchReq {
    user: String,
    #[serde(default)]
    item: Option<String>,
    #[serde(default)]
    ticks: Option<i64>,
    #[serde(default)]
    audio: Option<i64>,
    #[serde(default)]
    sub: Option<i64>,
    #[serde(default)]
    channel: Option<String>,
}

/// "Guarda sulla TV": a film / episode from where the phone was, or a channel, on the TV
/// as the phone's profile (the UI switches to it, or asks; phone.svelte.ts).
async fn app_tv_watch(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, Json(r): Json<WatchReq>) -> Response {
    let d = match auth(&ctx, &headers, &q) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let Some(user) = d.users.iter().find(|u| u.id == r.user).cloned() else {
        return fail(StatusCode::FORBIDDEN, "Profilo non collegato a questo telefono");
    };
    if ctx.app.state::<Arc<Games>>().running().is_some() {
        return fail(StatusCode::CONFLICT, "Sulla TV c'è un gioco aperto");
    }
    if r.item.is_none() && r.channel.is_none() {
        return fail(StatusCode::BAD_REQUEST, "Niente da guardare");
    }
    let _ = ctx.app.emit(
        "phone-watch",
        json!({ "user": user, "item": r.item, "ticks": r.ticks, "audio": r.audio, "sub": r.sub, "channel": r.channel,
                "from": user.name, "phone": d.who() }),
    );
    Json(json!({ "ok": true })).into_response()
}

#[derive(Deserialize)]
struct TakeReq {
    user: String,
}

/// "Continua qui": what the TV plays for this profile stops there, and the phone goes on from that point.
async fn app_tv_take(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, Json(r): Json<TakeReq>) -> Response {
    let d = match auth(&ctx, &headers, &q) {
        Ok(d) => d,
        Err(r) => return r,
    };
    if !d.has(&r.user) {
        return fail(StatusCode::FORBIDDEN, "Profilo non collegato a questo telefono");
    }
    if ctx.phone.state.borrow()["user"].as_str() != Some(r.user.as_str()) {
        return fail(StatusCode::CONFLICT, "Sulla TV c'è un altro profilo");
    }
    let player = ctx.app.state::<Arc<Player>>().inner().clone();
    let out = if let Some((item, ticks)) = player.now() {
        json!({ "item": item, "ticks": ticks })
    } else if let Some(ch) = player.channel() {
        json!({ "channel": ch })
    } else {
        return fail(StatusCode::NOT_FOUND, "Sulla TV non c'è niente da continuare");
    };
    // mpv quits and reports where it stopped (the phone's resume point too)
    player.shutdown().await;
    eprintln!("[phone] {} continues on the phone: {out}", d.id);
    Json(out).into_response()
}

/// `?user=` must be one of the phone's profiles.
fn phone_user(ctx: &Ctx, headers: &HeaderMap, q: &HashMap<String, String>) -> Result<String, Response> {
    let d = auth(ctx, headers, q)?;
    let user = q.get("user").cloned().unwrap_or_default();
    if !d.has(&user) {
        return Err(fail(StatusCode::FORBIDDEN, "Profilo non collegato a questo telefono"));
    }
    Ok(user)
}

/// Richieste: is Seerr there, is the profile linked to a Seerr user who may ask.
async fn app_seerr_status(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params) -> Response {
    let user = match phone_user(&ctx, &headers, &q) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let se = ctx.app.state::<Arc<crate::seerr::Seerr>>().inner().clone();
    Json(se.status(&user).await).into_response()
}

#[derive(Deserialize)]
struct SeerrReq {
    method: String,
    path: String,
    #[serde(default)]
    query: Vec<(String, String)>,
    #[serde(default)]
    body: Option<Value>,
}

/// Richieste: Seerr as the profile's linked user, only the TV's catalogue / request calls (seerr.rs `ALLOWED`).
async fn app_seerr(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, Json(r): Json<SeerrReq>) -> Response {
    let user = match phone_user(&ctx, &headers, &q) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let se = ctx.app.state::<Arc<crate::seerr::Seerr>>().inner().clone();
    match se.request(&user, &r.method, &r.path, &r.query, r.body).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => fail(StatusCode::BAD_GATEWAY, &e),
    }
}

/// How far a requested film / series is (`?media=movie|tv&tmdb=`), like the TV's bar.
async fn app_dl(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params) -> Response {
    if let Err(r) = phone_user(&ctx, &headers, &q) {
        return r;
    }
    let media = q.get("media").map(String::as_str).unwrap_or("movie").to_string();
    let Some(tmdb) = q.get("tmdb").and_then(|t| t.parse::<u64>().ok()) else { return fail(StatusCode::BAD_REQUEST, "tmdb?") };
    let dl = ctx.app.state::<Arc<crate::downloads::Downloads>>().inner().clone();
    match dl.progress(&media, tmdb).await {
        Ok(p) => Json(json!({ "progress": p })).into_response(),
        Err(e) => fail(StatusCode::BAD_GATEWAY, &e),
    }
}

/// A photo's or video's original, streamed from Immich with Range support (the phone's video player seeks with it).
async fn app_original(State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params, UrlPath(id): UrlPath<String>) -> Response {
    if let Err(r) = auth(&ctx, &headers, &q) {
        return r;
    }
    let im = ctx.app.state::<Arc<Immich>>().inner().clone();
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let resp = match im.original(&id, range).await {
        Ok(r) => r,
        Err(e) => return fail(StatusCode::BAD_GATEWAY, &e),
    };
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut out = Response::builder().status(status);
    for name in [header::CONTENT_TYPE, header::CONTENT_LENGTH, header::CONTENT_RANGE, header::ACCEPT_RANGES, header::ETAG, header::LAST_MODIFIED] {
        if let Some(v) = resp.headers().get(name.as_str()).and_then(|v| v.to_str().ok()) {
            out = out.header(name, v);
        }
    }
    out.body(Body::from_stream(resp.bytes_stream())).unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

async fn ws(ws: WebSocketUpgrade, State(ctx): State<Ctx>, headers: HeaderMap, Query(q): Params) -> Response {
    match auth(&ctx, &headers, &q) {
        Ok(d) => ws.on_upgrade(move |socket| session(ctx, d.token, socket)),
        Err(r) => r,
    }
}

#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Msg {
    /// a remote action (remote.rs)
    Key { a: String },
    /// the whole text of the TV's keyboard field; `enter` = done
    Text { v: String, #[serde(default)] enter: bool },
    /// volume steps (±1)
    Vol { d: i32 },
    /// screens and TV off, or back on
    Power,
    /// become a controller: a pad of its own, menus and games alike, until it leaves (any profile)
    Join,
    /// the controller's state: buttons as bits (azotopad's `Btn` order), sticks -1..1, triggers 0..1
    Pad {
        b: u32,
        #[serde(default)]
        lx: f32,
        #[serde(default)]
        ly: f32,
        #[serde(default)]
        rx: f32,
        #[serde(default)]
        ry: f32,
        #[serde(default)]
        lt: f32,
        #[serde(default)]
        rt: f32,
    },
    /// stop being a controller
    Leave,
}

/// How long a phone's button is down at least. azotopad folds the events it
/// reads together into one state, and games look at the pad once a frame (or
/// Azahar every ~10 ms), so a press released at once would never be seen.
const MIN_PRESS: Duration = Duration::from_millis(50);

/// A phone being a controller: its own pad, and the task that tells it which
/// player it is (whenever that changes: a pad before it went away).
struct PhonePad {
    pad: azotopad::ExternalPad,
    tell: tauri::async_runtime::JoinHandle<()>,
}

impl Drop for PhonePad {
    fn drop(&mut self) {
        self.tell.abort();
    }
}

/// What the TV calls the phone's pad: "Telefono di Ada" (uinput takes up to 80 bytes).
fn pad_name(dev: &Device) -> String {
    let names = dev.names();
    let mut name = match (names.is_empty(), dev.label.is_empty()) {
        (false, _) => format!("Telefono di {names}"),
        (true, false) => dev.label.clone(),
        (true, true) => "Telefono".into(),
    };
    while name.len() > 79 {
        name.pop();
    }
    name
}

/// Sends `{"t":"player","player":n}` now and whenever the phone's player
/// changes (null: connected, but past the 4th player).
fn tell_player(
    node: Option<PathBuf>,
    id: String,
    direct: tokio::sync::mpsc::UnboundedSender<Value>,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let mut told: Option<Option<usize>> = None;
        loop {
            let n = node.clone();
            let player =
                tauri::async_runtime::spawn_blocking(move || n.and_then(|n| crate::controllers::player_of(&n))).await.ok().flatten();
            // udev takes a moment to publish a new pad
            let settled = player.is_some() || started.elapsed() > Duration::from_secs(2);
            if settled && told != Some(player) {
                eprintln!("[phone] {id} is player {player:?}");
                if direct.send(json!({ "t": "player", "player": player })).is_err() {
                    break;
                }
                told = Some(player);
            }
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    })
}

/// Passes a game's rumble on to the phone (`{"t":"rumble","ms":n}`, 0 = stop;
/// it vibrates for that long). Games may update it every frame: a new one
/// goes out only if it changes, or the last is ending.
fn phone_rumble(direct: tokio::sync::mpsc::UnboundedSender<Value>) -> impl Fn(azotopad::Rumble) + Send + 'static {
    // until when the phone is vibrating
    let until = std::sync::Mutex::new(Instant::now());
    move |r: azotopad::Rumble| {
        let mut until = until.lock().unwrap();
        let now = Instant::now();
        // a faint one isn't worth a phone's full-strength motor
        let ms = if r.strength < 0x1000 { 0 } else { r.ms };
        let busy = *until > now + Duration::from_millis(60);
        if ms == 0 && *until <= now || ms > 0 && busy {
            return;
        }
        *until = now + Duration::from_millis(ms.into());
        let _ = direct.send(json!({ "t": "rumble", "ms": ms }));
    }
}

/// A phone's controller state as azotopad's (the guide button never: it would quit the game).
fn pad_state(b: u32, ax: [f32; 6]) -> azotopad::State {
    let mut s = azotopad::State::default();
    for (i, on) in s.btn.iter_mut().enumerate() {
        *on = b & (1 << i) != 0 && i != azotopad::Btn::Guide as usize;
    }
    for (i, v) in ax.into_iter().enumerate() {
        s.ax[i] = if i < 4 { v.clamp(-1.0, 1.0) } else { v.clamp(0.0, 1.0) };
    }
    s
}

/// What a phone is told: everything, but the keyboard's text only if it may type there.
fn view(dev: &Device, s: &Value) -> String {
    if s["text"].is_null() || allowed(dev, s) {
        return s.to_string();
    }
    let mut v = s.clone();
    v["text"] = Value::Null;
    v.to_string()
}

/// A connected phone: the TV's state goes out on every change, its presses
/// come in. The phone is looked up again for every message: it may have been
/// paired with another profile, or unpaired, meanwhile.
async fn session(ctx: Ctx, token: String, socket: WebSocket) {
    let (mut tx, mut rx) = socket.split();
    let mut state = ctx.phone.state.subscribe();
    // answers meant for this phone alone (which player it is)
    let (direct, mut direct_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let out = {
        let (phone, token) = (ctx.phone.clone(), token.clone());
        tauri::async_runtime::spawn(async move {
            let mut changed = true;
            loop {
                if changed {
                    let Some(dev) = phone.device(&token) else { break };
                    let s = view(&dev, &state.borrow_and_update());
                    if tx.send(Message::Text(s.into())).await.is_err() {
                        break;
                    }
                }
                tokio::select! {
                    r = state.changed() => {
                        if r.is_err() {
                            break;
                        }
                        changed = true;
                    }
                    Some(m) = direct_rx.recv() => {
                        if tx.send(Message::Text(m.to_string().into())).await.is_err() {
                            break;
                        }
                        changed = false;
                    }
                }
            }
        })
    };
    // this phone as a controller, while it is one
    let mut pad: Option<PhonePad> = None;
    // its buttons now, and since when each is down (MIN_PRESS)
    let (mut bits, mut since) = (0u32, [Instant::now(); 32]);
    let tv = ctx.app.state::<Arc<Tv>>().inner().clone();
    {
        let (phone, tv) = (ctx.phone.clone(), tv.clone());
        tauri::async_runtime::spawn(async move { phone.refresh_volume(&tv).await });
    }
    while let Some(Ok(msg)) = rx.next().await {
        let Message::Text(text) = msg else {
            if matches!(msg, Message::Close(_)) {
                break;
            }
            continue;
        };
        let Some(dev) = ctx.phone.device(&token) else { break };
        let Ok(m) = serde_json::from_str::<Msg>(&text) else { continue };
        let ok = allowed(&dev, &ctx.phone.state.borrow());
        match m {
            Msg::Key { a } if crate::remote::known(&a) => {
                if crate::pad::paused() {
                    // a game is running: only Home does something, it closes the
                    // game like holding Home on a pad (the phone asked first)
                    if a == "home" {
                        if !ok {
                            ctx.phone.tell(&ctx.app, &dev);
                        } else if let Some(id) = ctx.app.state::<Arc<Games>>().quit() {
                            eprintln!("[games] {} pressed Home: closing {id}", dev.names());
                        }
                    }
                    continue;
                }
                if ctx.app.state::<Arc<Player>>().active() {
                    // mpv has the screen: the UI can't ask, so the check is here
                    if ok {
                        crate::remote::fire(&ctx.app, &a);
                    } else {
                        ctx.phone.tell(&ctx.app, &dev);
                    }
                } else {
                    let _ = ctx.app.emit("phone-key", json!({ "a": a, "phone": dev.who() }));
                }
            }
            Msg::Key { .. } => {}
            Msg::Text { v, enter } => {
                let v: String = v.chars().take(500).collect();
                let _ = ctx.app.emit("phone-text", json!({ "value": v, "enter": enter, "phone": dev.who() }));
            }
            Msg::Vol { d } if ok => {
                let (phone, tv) = (ctx.phone.clone(), tv.clone());
                tauri::async_runtime::spawn(async move { phone.change_volume(&tv, d.clamp(-5, 5)).await });
            }
            Msg::Vol { .. } => ctx.phone.tell(&ctx.app, &dev),
            Msg::Power => {
                let _ = ctx.app.emit("phone-power", json!({ "phone": dev.who() }));
            }
            Msg::Join => {
                pad = None; // a phone is one player
                let name = pad_name(&dev);
                let buzz = phone_rumble(direct.clone());
                let r = tauri::async_runtime::spawn_blocking(move || azotopad::ExternalPad::new(&name, buzz))
                    .await
                    .map_err(|e| std::io::Error::other(e.to_string()))
                    .and_then(|r| r);
                match r {
                    Ok(p) => {
                        let tell = tell_player(p.node().cloned(), dev.id.clone(), direct.clone());
                        pad = Some(PhonePad { pad: p, tell });
                    }
                    Err(e) => {
                        let _ = direct.send(json!({ "t": "player", "error": format!("Controller non disponibile: {e}") }));
                    }
                }
            }
            Msg::Pad { b, lx, ly, rx, ry, lt, rt } => {
                // a quick tap's press and release can come in together (Wi-Fi
                // bunches packets): the release waits until the press has lasted
                // (the triggers count as two more buttons: ZL / ZR are taps too)
                let b2 = b & 0x3fff_ffff | u32::from(lt > 0.5) << 30 | u32::from(rt > 0.5) << 31;
                let now = Instant::now();
                let wait = (0..32)
                    .filter(|&i| bits & !b2 & (1 << i) != 0)
                    .map(|i| MIN_PRESS.saturating_sub(now - since[i]))
                    .max()
                    .unwrap_or_default();
                if !wait.is_zero() {
                    tokio::time::sleep(wait).await;
                }
                for (i, t) in since.iter_mut().enumerate() {
                    if b2 & !bits & (1 << i) != 0 {
                        *t = Instant::now();
                    }
                }
                bits = b2;
                if let Some(p) = &mut pad {
                    if let Err(e) = p.pad.set(pad_state(b, [lx, ly, rx, ry, lt, rt])) {
                        eprintln!("[phone] pad: {e}");
                    }
                }
            }
            Msg::Leave => pad = None,
        }
    }
    out.abort();
}

// ---- commands (the pairing screen, the UI's state) ------------------------

/// A code for the profile `user` (`name`), shown on START > Collega un telefono.
#[tauri::command]
pub fn phone_pairing(phone: tauri::State<'_, Arc<Phone>>, user: String, name: String) -> PairingInfo {
    phone.start_pairing(Account { id: user, name })
}

#[tauri::command]
pub fn phone_pairing_stop(phone: tauri::State<'_, Arc<Phone>>) {
    phone.stop_pairing();
}

#[tauri::command]
pub fn phone_list(phone: tauri::State<'_, Arc<Phone>>, user: String) -> Vec<PairedPhone> {
    phone.list(&user)
}

#[tauri::command]
pub fn phone_forget(phone: tauri::State<'_, Arc<Phone>>, jf: tauri::State<'_, Arc<crate::jellyfin::Jf>>, id: String, user: String) {
    crate::phonejf::forget(&jf, &id, &user);
    phone.forget(&id, &user);
}

/// The apps `user` has on the home, in order (the iPhone app shows the same).
#[tauri::command]
pub fn phone_apps(phone: tauri::State<'_, Arc<Phone>>, user: String, apps: Vec<String>) {
    phone.set_apps(&user, apps);
}

/// The TV left its page made for touch: a phone showing it stops (not during
/// a game, whose screen it may be).
#[tauri::command]
pub async fn phone_screen_stop(games: tauri::State<'_, Arc<Games>>) -> Result<(), String> {
    if games.screen_game().is_none() {
        crate::phonescreen::stop().await;
    }
    Ok(())
}

#[tauri::command]
pub fn phone_state(phone: tauri::State<'_, Arc<Phone>>, state: Value) {
    phone.set_state(state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_links() {
        for (url, id) in [
            ("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=10", Some("dQw4w9WgXcQ")),
            ("https://youtu.be/dQw4w9WgXcQ?si=abc", Some("dQw4w9WgXcQ")),
            ("https://youtube.com/shorts/dQw4w9WgXcQ?feature=share", Some("dQw4w9WgXcQ")),
            ("https://m.youtube.com/watch?feature=x&v=dQw4w9WgXcQ", Some("dQw4w9WgXcQ")),
            ("https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RD", Some("dQw4w9WgXcQ")),
            ("https://www.youtube.com/@channel", None),
            ("https://www.instagram.com/reel/abc/", None),
        ] {
            assert_eq!(youtube_id(url).as_deref(), id, "{url}");
        }
    }

    #[test]
    fn links_in_text() {
        assert_eq!(first_link("Guarda! https://youtu.be/x123 bello").as_deref(), Some("https://youtu.be/x123"));
        assert_eq!(first_link("(https://example.com/a)").as_deref(), Some("https://example.com/a"));
        assert_eq!(first_link("niente"), None);
    }

    fn dev(users: &[(&str, &str)]) -> Device {
        Device {
            id: "d".into(),
            token: "t".into(),
            label: "iPhone".into(),
            users: users.iter().map(|(id, name)| Account { id: id.to_string(), name: name.to_string() }).collect(),
            paired: 0,
        }
    }

    #[test]
    fn who_may_act() {
        let d = dev(&[("ada", "Ada"), ("luca", "Luca")]);
        assert!(allowed(&d, &json!({ "user": "luca", "picker": false })));
        assert!(!allowed(&d, &json!({ "user": "paolo", "picker": false })));
        assert!(allowed(&d, &json!({ "user": "", "picker": true })));
        assert!(!allowed(&d, &json!({ "user": "", "picker": false })));
        assert_eq!(d.names(), "Ada e Luca");
        assert_eq!(dev(&[("a", "A"), ("b", "B"), ("c", "C")]).names(), "A, B e C");
        // the keyboard's text only reaches phones that may type into it
        let s = json!({ "user": "paolo", "picker": false, "text": { "value": "segreto" } });
        assert!(!view(&d, &s).contains("segreto"));
        assert!(view(&dev(&[("paolo", "Paolo")]), &s).contains("segreto"));
    }

    #[test]
    fn controller_state() {
        use azotopad::Btn;
        let s = pad_state(1 << Btn::A as u32 | 1 << Btn::Guide as u32 | 1 << Btn::Start as u32, [2.0, -0.5, 0.0, 0.0, 1.5, -1.0]);
        assert!(s.pressed(Btn::A) && s.pressed(Btn::Start));
        assert!(!s.pressed(Btn::Guide), "a phone never sends Home (it quits the game)");
        assert_eq!(s.ax, [1.0, -0.5, 0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn dates() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(1_761_000_000_123), "2025-10-20T22:40:00.123Z");
    }
}
