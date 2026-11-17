//! The TV itself (a Hisense on VIDAA): "Spegni" turns it off, and waking
//! AzotoTV turns it back on and switches it to AzotoTV's HDMI input. The TV
//! has no HDMI-CEC towards the server (that output is on the NVIDIA card) and
//! ignores wake-on-LAN, so this goes through the MQTT broker the TV runs for
//! Hisense's phone app (port 36669, TLS with the app's client certificate).
//! While the TV is in its light standby the broker stays up, so it can be
//! turned on as well as off; about 6 minutes later it drops into deep standby,
//! is gone from the network and only its own remote wakes it. AzotoTV also
//! follows the TV's state, so when it is turned on with any remote it is
//! switched to AzotoTV's input (it opens on its own home otherwise) and the
//! screens wake.
//!
//! Protocol (from github.com/warrenrees/myhisense-tv, VIDAA_PROTOCOL_ANALYSIS.md):
//! a login derived from a made-up MAC-like `uuid` and the clock, a one-time
//! pairing with a PIN shown on the TV, then an access token (2 days) used as
//! the MQTT password, renewed with the refresh token (30 days, renewed too).
//!
//! Files in `~/.config/azototv/tv/` (no folder: no TV control):
//! - `tv.json`: `{"host": "192.168.1.50", "uuid": "aa:bb:...", "input": "HDMI3"}`
//! - `client.pem`, `client.key`: the VIDAA app's client certificate (from its
//!   APK, `res/raw/client_mobile_android.p12`; not in the repo)
//! - `token.json`: the tokens from the pairing (`mqtt_username` = the login
//!   they were issued to); AzotoTV keeps it renewed.
//!
//! The TV can forget the pairing (someone removes AzotoTV from its devices):
//! it then refuses even the refresh token. AzotoTV notices, sends a
//! notification and shows START › Collega la TV, where the PIN the TV shows
//! is typed on AzotoTV's own keypad (`tv_pair_start`, then `tv_pair_pin`).

use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use md5::{Digest, Md5};
use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature},
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime, pem::PemObject},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex,
    time::timeout,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio_rustls::{TlsConnector, client::TlsStream};

use crate::{
    jellyfin::config_dir,
    notify::{Notification, Notify},
};

const PORT: u16 = 36669;
const PATTERN: &str = "38D65DC30F45109A369A86FCE866A85B";
const VALUE_SUFFIX: &str = "h!i@s#$v%i^d&a*a";
const TIME_XOR: u64 = 0x5698_1477_2b03_a968;
const UPNP_RC: &str = "urn:schemas-upnp-org:service:RenderingControl:1";
const STATE_TOPIC: &str = "/remoteapp/mobile/broadcast/ui_service/state";
/// Sent (empty) a few seconds before the TV leaves the network.
const SLEEP_TOPIC: &str = "/remoteapp/mobile/broadcast/platform_service/actions/tvsleep";
/// How soon after the TV comes back on the network it is noticed.
const WATCH_RETRY: Duration = Duration::from_secs(5);
/// How often to check whether the access token needs renewing.
const RENEW_EVERY: Duration = Duration::from_secs(6 * 3600);
/// While the PIN is on the TV, the pairing connection is pinged this often.
const PAIR_PING: Duration = Duration::from_secs(30);

// the errors that mean "pair again"
const NOT_PAIRED: &str = "not paired (no token.json)";
const EXPIRED: &str = "the pairing expired: pair AzotoTV with the TV again";
const REFUSED: &str = "the TV refused the login: pair AzotoTV with the TV again";

#[derive(Deserialize)]
struct Config {
    host: String,
    uuid: String,
    /// The input AzotoTV is on, as the TV names it ("HDMI3").
    input: Option<String>,
}

pub struct Tv {
    dir: PathBuf,
    cfg: Option<Config>,
    /// one conversation with the TV at a time (and one token renewal)
    lock: Mutex<()>,
    /// UPnP (volume)
    http: reqwest::Client,
    /// the TV refuses AzotoTV's tokens: START shows "Collega la TV"
    unpaired: AtomicBool,
    /// a pairing waiting for the PIN typed in the UI, and which one it is
    pairing: Mutex<Option<Pairing>>,
    pair_gen: AtomicU64,
    app: OnceLock<AppHandle>,
}

/// A pairing started from the UI: the TV shows the PIN, this waits for it.
struct Pairing {
    c: Conn,
    cid: String,
    /// the login the tokens will be issued to
    user: String,
    id: u64,
}

fn md5_upper(s: &str) -> String {
    Md5::digest(s.as_bytes()).iter().map(|b| format!("{b:02X}")).collect()
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The TV sends times as strings, durations as numbers.
fn num(v: &Value) -> i64 {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
}

/// Token `kind` ("accesstoken" / "refreshtoken") still good for another hour.
fn valid(tok: &Value, kind: &str) -> bool {
    let until = num(&tok[format!("{kind}_time")]) + num(&tok[format!("{kind}_duration_day")]) * 86400;
    tok[kind].as_str().is_some_and(|s| !s.is_empty()) && now_secs() < until - 3600
}

impl Tv {
    pub fn load() -> Arc<Tv> {
        let dir = config_dir().join("tv");
        let cfg = fs::read_to_string(dir.join("tv.json")).ok().and_then(|s| serde_json::from_str(&s).ok());
        let http = reqwest::Client::builder().timeout(Duration::from_secs(3)).build().expect("http client");
        Arc::new(Tv {
            dir,
            cfg,
            lock: Mutex::new(()),
            http,
            unpaired: AtomicBool::new(false),
            pairing: Mutex::new(None),
            pair_gen: AtomicU64::new(0),
            app: OnceLock::new(),
        })
    }

    pub fn enabled(&self) -> bool {
        self.cfg.is_some()
    }

    fn client_id(&self, cfg: &Config) -> String {
        let race = md5_upper(&format!("{PATTERN}${}", cfg.uuid));
        format!("{}$his${}_vidaacommon_001", cfg.uuid, &race[..6])
    }

    fn token(&self) -> Result<Value, String> {
        let s = fs::read_to_string(self.dir.join("token.json")).map_err(|_| NOT_PAIRED)?;
        serde_json::from_str(&s).map_err(|e| e.to_string())
    }

    fn save_token(&self, tok: &Value) {
        use std::os::unix::fs::OpenOptionsExt;
        let tmp = self.dir.join("token.json.tmp");
        let ok = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .and_then(|mut f| std::io::Write::write_all(&mut f, tok.to_string().as_bytes()))
            .and_then(|_| fs::rename(&tmp, self.dir.join("token.json")));
        if let Err(e) = ok {
            eprintln!("[tv] cannot save token.json: {e}");
        }
    }

    /// Asks for a new access token with the refresh token.
    async fn renew(&self, cfg: &Config, mut tok: Value) -> Result<Value, String> {
        let user = tok["mqtt_username"].as_str().unwrap_or_default().to_string();
        let refresh = tok["refreshtoken"].as_str().unwrap_or_default().to_string();
        let cid = self.client_id(cfg);
        let mut c = Conn::open(&self.dir, cfg, &cid, &user, &refresh).await?;
        let reply = format!("/remoteapp/mobile/{cid}/platform_service/data/tokenissuance");
        c.subscribe(&[&reply]).await?;
        c.publish(
            &format!("/remoteapp/tv/platform_service/{cid}/data/gettoken"),
            json!({ "refreshtoken": refresh }).to_string().as_bytes(),
        )
        .await?;
        let until = Instant::now() + Duration::from_secs(6);
        while let Some((topic, payload)) = c.next(until).await {
            if topic != reply {
                continue;
            }
            let new: Value = serde_json::from_slice(&payload).unwrap_or_default();
            if new["accesstoken"].as_str().is_some_and(|s| !s.is_empty()) {
                if let (Some(t), Some(n)) = (tok.as_object_mut(), new.as_object()) {
                    t.extend(n.clone());
                }
                self.save_token(&tok);
                eprintln!("[tv] access token renewed");
                c.close().await;
                return Ok(tok);
            }
        }
        Err("no new token from the TV".into())
    }

    /// Whether the TV has stopped taking AzotoTV's tokens; tells the UI
    /// (`tv-unpaired`) and, the first time, sends a notification.
    async fn set_unpaired(&self, unpaired: bool) {
        if self.unpaired.swap(unpaired, Ordering::Relaxed) == unpaired {
            return;
        }
        eprintln!("[tv] {}", if unpaired { "not paired any more" } else { "paired" });
        let Some(app) = self.app.get() else { return };
        let _ = app.emit("tv-unpaired", unpaired);
        if unpaired {
            let n = Notification {
                id: 0,
                time: 0,
                kind: "tv".into(),
                kicker: "TV".into(),
                title: t!("The TV no longer takes AzotoTV's commands"),
                body: t!("Turn off and automatic turning on don't work: START › Connect the TV"),
                image: None,
                logo: false,
                item_id: None,
                channel_id: None,
                users: Vec::new(),
                tg: false,
            };
            app.state::<Arc<Notify>>().push(n, true).await;
        }
    }

    /// Notes what a connection attempt says about the pairing.
    async fn check<T>(&self, r: Result<T, String>) -> Result<T, String> {
        match &r {
            Ok(_) => self.set_unpaired(false).await,
            Err(e) if [NOT_PAIRED, EXPIRED, REFUSED].contains(&e.as_str()) => self.set_unpaired(true).await,
            Err(_) => {} // off, unreachable...
        }
        r
    }

    /// A connection with a valid access token (renewing it first if needed).
    async fn session(&self) -> Result<(Conn, String), String> {
        let r = self.open_session().await;
        self.check(r).await
    }

    async fn open_session(&self) -> Result<(Conn, String), String> {
        let cfg = self.cfg.as_ref().ok_or("no tv.json")?;
        let mut tok = self.token()?;
        if !valid(&tok, "accesstoken") {
            if !valid(&tok, "refreshtoken") {
                return Err(EXPIRED.into());
            }
            tok = self.renew(cfg, tok).await?;
        }
        let cid = self.client_id(cfg);
        let user = tok["mqtt_username"].as_str().unwrap_or_default();
        let c = Conn::open(&self.dir, cfg, &cid, user, tok["accesstoken"].as_str().unwrap_or_default()).await?;
        Ok((c, cid))
    }

    /// Turns the TV on (and onto AzotoTV's input) or off. Does nothing when it
    /// already is; the power key is a toggle, so the TV's state is read first.
    /// `only_ours`: turn it off only while it shows AzotoTV's input (AzotoTV
    /// going dark on its own mustn't switch off a TV someone is watching).
    pub async fn power(&self, on: bool, only_ours: bool) -> Result<(), String> {
        let _one = self.lock.lock().await;
        let cfg = self.cfg.as_ref().ok_or("no tv.json")?;
        let (mut c, cid) = self.session().await?;
        c.subscribe(&[STATE_TOPIC]).await?;
        // the state topic is retained: the current state comes right away
        let mut state = c.state(Instant::now() + Duration::from_secs(3)).await;
        let asleep = |s: &Option<Value>| s.as_ref().is_some_and(|s| s["statetype"] == "fake_sleep_0");
        let key = format!("/remoteapp/tv/remote_service/{cid}/actions/sendkey");
        if !on {
            let ours = |s: &Value| s["statetype"] == "sourceswitch" && s["sourceid"].as_str() == cfg.input.as_deref();
            if state.as_ref().is_some_and(|s| !asleep(&state) && (!only_ours || ours(s))) {
                c.publish(&key, b"KEY_POWER").await?;
                eprintln!("[tv] off");
            }
            c.close().await;
            return Ok(());
        }
        if asleep(&state) {
            c.publish(&key, b"KEY_POWER").await?;
            eprintln!("[tv] on");
            // fake_sleep_1 while waking, then its home screen
            let until = Instant::now() + Duration::from_secs(8);
            loop {
                state = c.state(until).await;
                match &state {
                    Some(s) if s["statetype"].as_str().is_some_and(|t| t.starts_with("fake_sleep")) => {}
                    _ => break,
                }
            }
        }
        if let Some(input) = &cfg.input {
            let current = state.as_ref().and_then(|s| s["sourceid"].as_str());
            if current != Some(input.as_str()) {
                c.publish(
                    &format!("/remoteapp/tv/ui_service/{cid}/actions/changesource"),
                    json!({ "sourceid": input, "sourcename": input }).to_string().as_bytes(),
                )
                .await?;
                eprintln!("[tv] input {input}");
            }
        }
        c.close().await;
        Ok(())
    }

    /// One UPnP RenderingControl action on the TV (its real volume; the TV
    /// asked once on screen to allow this, on 2026-10-19). Returns the reply.
    async fn rendering(&self, action: &str, args: &str) -> Result<String, String> {
        let cfg = self.cfg.as_ref().ok_or("no tv.json")?;
        let body = format!(
            "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
             s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} \
             xmlns:u=\"{UPNP_RC}\"><InstanceID>0</InstanceID><Channel>Master</Channel>{args}</u:{action}></s:Body></s:Envelope>"
        );
        let r = self
            .http
            .post(format!("http://{}:38400/upnp/control/mingusrcr", cfg.host))
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header("SOAPACTION", format!("\"{UPNP_RC}#{action}\""))
            .body(body)
            .send()
            .await
            .map_err(|_| t!("Can't reach the TV"))?;
        if !r.status().is_success() {
            return Err(format!("TV: {action} HTTP {}", r.status()));
        }
        r.text().await.map_err(|e| e.to_string())
    }

    /// The TV's volume, 0-100.
    pub async fn volume(&self) -> Result<u32, String> {
        let xml = self.rendering("GetVolume", "").await?;
        let v = xml.split("<CurrentVolume>").nth(1).and_then(|r| r.split('<').next()).ok_or("TV: no volume in reply")?;
        v.trim().parse().map_err(|_| format!("TV: bad volume {v}"))
    }

    pub async fn set_volume(&self, v: u32) -> Result<(), String> {
        self.rendering("SetVolume", &format!("<DesiredVolume>{}</DesiredVolume>", v.min(100))).await.map(|_| ())
    }

    /// Follows the TV's state: when it comes to AzotoTV's input (turned on
    /// there, or switched to it with its remote) the UI is told (`tv-on`: the
    /// screens wake). The TV is never switched to AzotoTV on its own: turned
    /// on for normal TV it stays there (and while a PC streams, AzotoTV's
    /// output is off: switching would leave it on "no signal"). The connection
    /// drops when the TV leaves the network and is tried again every few s.
    fn watch(self: &Arc<Self>, app: AppHandle) {
        let tv = self.clone();
        tauri::async_runtime::spawn(async move {
            // Some(false) once seen asleep or elsewhere: AzotoTV's input next means it came to us
            let mut ours = None;
            let mut last_err = String::new();
            loop {
                // the pairing uses the same client id: logging in would drop it
                if tv.pairing.lock().await.is_some() {
                    tokio::time::sleep(WATCH_RETRY).await;
                    continue;
                }
                if let Err(e) = tv.follow(&app, &mut ours).await {
                    if e != last_err {
                        eprintln!("[tv] watch: {e}");
                        last_err = e;
                    }
                } else {
                    last_err.clear();
                }
                tokio::time::sleep(WATCH_RETRY).await;
            }
        });
    }

    /// `ours`: whether the TV shows AzotoTV's input (Some(false) also when
    /// asleep; None until its state is known).
    async fn follow(&self, app: &AppHandle, ours: &mut Option<bool>) -> Result<(), String> {
        let cfg = self.cfg.as_ref().ok_or("no tv.json")?;
        let (mut c, _) = self.session().await?;
        c.subscribe(&[STATE_TOPIC, SLEEP_TOPIC]).await?;
        loop {
            let Some((topic, payload)) = c.recv(Duration::from_secs(25)).await? else {
                c.send(&[0xC0, 0]).await?; // PINGREQ
                continue;
            };
            let state: Value = serde_json::from_slice(&payload).unwrap_or_default();
            let kind = state["statetype"].as_str().unwrap_or_default();
            let now = if topic == SLEEP_TOPIC || kind == "fake_sleep_0" {
                false
            } else if topic == STATE_TOPIC && !kind.is_empty() && !kind.starts_with("fake_sleep") {
                kind == "sourceswitch" && cfg.input.is_some() && state["sourceid"].as_str() == cfg.input.as_deref()
            } else {
                continue;
            };
            if now && *ours == Some(false) {
                eprintln!("[tv] showing AzotoTV");
                let _ = app.emit("tv-on", ());
            }
            *ours = Some(now);
        }
    }

    /// Keeps the pairing alive (renews the access token once it's a day old,
    /// when the TV is reachable; it lasts two) and follows the TV's state.
    pub fn start(self: &Arc<Self>, app: AppHandle) {
        if !self.enabled() {
            return;
        }
        let _ = self.app.set(app.clone());
        self.watch(app);
        let tv = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                if let (Some(cfg), Ok(tok)) = (tv.cfg.as_ref(), tv.token()) {
                    let half = num(&tok["accesstoken_duration_day"]) * 86400 / 2;
                    if now_secs() > num(&tok["accesstoken_time"]) + half && valid(&tok, "refreshtoken") {
                        let _one = tv.lock.lock().await;
                        let r = tv.renew(cfg, tok).await;
                        if let Err(e) = tv.check(r).await {
                            eprintln!("[tv] token renewal: {e}");
                        }
                    }
                }
                tokio::time::sleep(RENEW_EVERY).await;
            }
        });
    }
}

/// `azototv --tv pair|on|off|idle-off|renew`, on the server (no UI). `pair`: the TV
/// shows a PIN, typed here; it saves `token.json`.
pub fn cli(cmd: &str) {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    let tv = Tv::load();
    let r = rt.block_on(async {
        match cmd {
            "pair" => tv.pair().await,
            "on" => tv.power(true, false).await,
            "off" => tv.power(false, false).await,
            "idle-off" => tv.power(false, true).await,
            "renew" => {
                let cfg = tv.cfg.as_ref().ok_or("no tv.json")?;
                tv.renew(cfg, tv.token()?).await.map(|_| ())
            }
            _ => Err("usage: azototv --tv pair|on|off|idle-off|renew".into()),
        }
    });
    match r {
        Ok(()) => eprintln!("[tv] ok"),
        Err(e) => eprintln!("[tv] {e}"),
    }
}

impl Tv {
    /// One-time pairing: a login made from the uuid and the clock, the PIN
    /// from the TV's screen (`pair_pin`), then the first tokens.
    async fn pair_begin(&self) -> Result<Pairing, String> {
        let cfg = self.cfg.as_ref().ok_or("no tv.json")?;
        let ts = now_secs() as u64;
        let user = format!("his${}", ts ^ TIME_XOR);
        let digits: u64 = ts.to_string().bytes().map(|b| (b - b'0') as u64).sum();
        let value = md5_upper(&format!("his{}{VALUE_SUFFIX}", digits % 10));
        let pass = md5_upper(&format!("{ts}${}", &value[..6]));
        let cid = self.client_id(cfg);
        let mut c = Conn::open(&self.dir, cfg, &cid, &user, &pass).await?;
        let base = format!("/remoteapp/mobile/{cid}");
        c.subscribe(&[
            &format!("{base}/ui_service/data/authenticationcode"),
            &format!("{base}/platform_service/data/tokenissuance"),
        ])
        .await?;
        c.publish(
            &format!("/remoteapp/tv/ui_service/{cid}/actions/vidaa_app_connect"),
            json!({ "app_version": 2, "connect_result": 0, "device_type": "Mobile App" }).to_string().as_bytes(),
        )
        .await?;
        let id = self.pair_gen.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(Pairing { c, cid, user, id })
    }

    /// The PIN the TV shows (4 digits): on success the tokens are saved.
    async fn pair_pin(&self, p: Pairing, pin: &str) -> Result<(), String> {
        let Pairing { mut c, cid, user, .. } = p;
        let base = format!("/remoteapp/mobile/{cid}");
        let (auth, token) =
            (format!("{base}/ui_service/data/authenticationcode"), format!("{base}/platform_service/data/tokenissuance"));
        c.publish(
            &format!("/remoteapp/tv/ui_service/{cid}/actions/authenticationcode"),
            json!({ "authNum": pin }).to_string().as_bytes(),
        )
        .await?;
        let until = Instant::now() + Duration::from_secs(8);
        let mut accepted = false;
        while let Some((topic, payload)) = c.next(until).await {
            let v: Value = serde_json::from_slice(&payload).unwrap_or_default();
            if topic == auth {
                if num(&v["result"]) != 1 {
                    return Err(format!("PIN refused ({v})"));
                }
                accepted = true;
                c.publish(
                    &format!("/remoteapp/tv/platform_service/{cid}/data/gettoken"),
                    json!({ "refreshtoken": "" }).to_string().as_bytes(),
                )
                .await?;
                c.publish(&format!("/remoteapp/tv/ui_service/{cid}/actions/authenticationcodeclose"), b"").await?;
            } else if topic == token && v["accesstoken"].as_str().is_some_and(|s| !s.is_empty()) {
                let mut tok = v;
                tok["mqtt_username"] = json!(user);
                self.save_token(&tok);
                c.close().await;
                self.set_unpaired(false).await;
                return Ok(());
            }
        }
        Err(if accepted { "PIN accepted but no token came" } else { "no answer to the PIN" }.into())
    }

    /// The CLI's pairing: the PIN is typed on stdin.
    async fn pair(&self) -> Result<(), String> {
        let p = self.pair_begin().await?;
        eprintln!("[tv] type the PIN shown on the TV and press Enter (within a minute)");
        let pin = tokio::task::spawn_blocking(|| {
            let mut l = String::new();
            std::io::stdin().read_line(&mut l).map(|_| l.trim().to_string())
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
        self.pair_pin(p, &pin).await
    }

    /// Drops a pairing the UI started (the PIN goes away from the TV).
    async fn pair_cancel(&self) {
        if let Some(Pairing { mut c, cid, .. }) = self.pairing.lock().await.take() {
            let _ = c.publish(&format!("/remoteapp/tv/ui_service/{cid}/actions/authenticationcodeclose"), b"").await;
            c.close().await;
        }
    }
}

/// The TV's certificate is self-signed (for 127.0.0.1): like Hisense's own
/// app, it is not checked; the handshake signatures still are.
#[derive(Debug)]
pub(crate) struct AnyServer(pub(crate) Arc<CryptoProvider>);

impl ServerCertVerifier for AnyServer {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(msg, cert, dss, &self.0.signature_verification_algorithms)
    }
    fn verify_tls13_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(msg, cert, dss, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Just enough MQTT 3.1.1 for this: connect, subscribe, QoS 0 publish, read.
struct Conn {
    s: TlsStream<TcpStream>,
}

fn mqtt_str(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s);
}

fn packet(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![kind];
    let mut n = body.len();
    loop {
        let mut b = (n % 128) as u8;
        n /= 128;
        if n > 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(body);
    out
}

impl Conn {
    async fn open(dir: &std::path::Path, cfg: &Config, client_id: &str, user: &str, pass: &str) -> Result<Conn, String> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let certs = CertificateDer::pem_file_iter(dir.join("client.pem"))
            .and_then(|i| i.collect::<Result<Vec<_>, _>>())
            .map_err(|e| format!("client.pem: {e}"))?;
        let key = PrivateKeyDer::from_pem_file(dir.join("client.key")).map_err(|e| format!("client.key: {e}"))?;
        let tls = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AnyServer(provider)))
            .with_client_auth_cert(certs, key)
            .map_err(|e| e.to_string())?;
        let name = ServerName::try_from(cfg.host.clone()).map_err(|e| e.to_string())?;
        let tcp = timeout(Duration::from_secs(3), TcpStream::connect((cfg.host.as_str(), PORT)))
            .await
            .map_err(|_| "the TV doesn't answer (off, or in deep standby)")?
            .map_err(|e| format!("the TV doesn't answer: {e}"))?;
        let s = timeout(Duration::from_secs(5), TlsConnector::from(Arc::new(tls)).connect(name, tcp))
            .await
            .map_err(|_| "TLS timeout")?
            .map_err(|e| format!("TLS: {e}"))?;
        let mut c = Conn { s };

        let mut body = Vec::new();
        mqtt_str(&mut body, b"MQTT");
        body.push(4); // 3.1.1
        body.push(0xC2); // username, password, clean session
        body.extend_from_slice(&60u16.to_be_bytes()); // keepalive (pairing waits for a person)
        mqtt_str(&mut body, client_id.as_bytes());
        mqtt_str(&mut body, user.as_bytes());
        mqtt_str(&mut body, pass.as_bytes());
        c.send(&packet(0x10, &body)).await?;
        match timeout(Duration::from_secs(5), c.read()).await {
            Ok(Ok((0x20, b))) if b.len() == 2 && b[1] == 0 => Ok(c),
            // 4: bad username or password, 5: not authorized
            Ok(Ok((0x20, b))) if b.len() == 2 && (b[1] == 4 || b[1] == 5) => Err(REFUSED.into()),
            Ok(Ok((0x20, b))) => Err(format!("the TV refused the login ({b:?})")),
            Ok(Ok(_)) => Err("unexpected reply to connect".into()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("no reply to connect".into()),
        }
    }

    async fn send(&mut self, p: &[u8]) -> Result<(), String> {
        self.s.write_all(p).await.map_err(|e| e.to_string())?;
        self.s.flush().await.map_err(|e| e.to_string())
    }

    /// One packet: (first byte, body).
    async fn read(&mut self) -> Result<(u8, Vec<u8>), String> {
        let kind = self.s.read_u8().await.map_err(|e| e.to_string())?;
        let (mut len, mut mult) = (0usize, 1usize);
        loop {
            let b = self.s.read_u8().await.map_err(|e| e.to_string())?;
            len += (b & 0x7f) as usize * mult;
            mult *= 128;
            if b & 0x80 == 0 || mult > 128usize.pow(4) {
                break;
            }
        }
        let mut body = vec![0; len];
        self.s.read_exact(&mut body).await.map_err(|e| e.to_string())?;
        Ok((kind, body))
    }

    async fn subscribe(&mut self, topics: &[&str]) -> Result<(), String> {
        let mut body = 1u16.to_be_bytes().to_vec();
        for t in topics {
            mqtt_str(&mut body, t.as_bytes());
            body.push(0);
        }
        self.send(&packet(0x82, &body)).await
    }

    async fn publish(&mut self, topic: &str, payload: &[u8]) -> Result<(), String> {
        let mut body = Vec::new();
        mqtt_str(&mut body, topic.as_bytes());
        body.extend_from_slice(payload);
        self.send(&packet(0x30, &body)).await
    }

    /// The next message (topic, payload) within `wait`: None if none came,
    /// an error if the connection is gone.
    async fn recv(&mut self, wait: Duration) -> Result<Option<(String, Vec<u8>)>, String> {
        let until = Instant::now() + wait;
        loop {
            let Some(left) = until.checked_duration_since(Instant::now()) else { return Ok(None) };
            let Ok(read) = timeout(left, self.read()).await else { return Ok(None) };
            let (kind, body) = read?;
            if kind >> 4 != 3 || body.len() < 2 {
                continue; // SUBACK, PINGRESP...
            }
            let tlen = u16::from_be_bytes([body[0], body[1]]) as usize;
            let qos = (kind >> 1) & 3;
            let start = 2 + tlen + if qos > 0 { 2 } else { 0 };
            if body.len() < start {
                continue;
            }
            return Ok(Some((String::from_utf8_lossy(&body[2..2 + tlen]).into_owned(), body[start..].to_vec())));
        }
    }

    /// The next message before `until`.
    async fn next(&mut self, until: Instant) -> Option<(String, Vec<u8>)> {
        self.recv(until.checked_duration_since(Instant::now())?).await.ok()?
    }

    /// The next state broadcast before `until`.
    async fn state(&mut self, until: Instant) -> Option<Value> {
        while let Some((topic, payload)) = self.next(until).await {
            if topic == STATE_TOPIC {
                return serde_json::from_slice(&payload).ok();
            }
        }
        None
    }

    async fn close(mut self) {
        let _ = self.send(&[0xE0, 0]).await;
        let _ = self.s.shutdown().await;
    }
}

/// Starts a TV power change in the background (the UI doesn't wait for the TV).
#[tauri::command]
pub fn tv_power(on: bool, only_ours: Option<bool>, tv: tauri::State<'_, Arc<Tv>>) {
    let tv = tv.inner().clone();
    // streaming (stream.rs): waking the UI must not turn the TV on
    if !tv.enabled() || (on && crate::stream::active()) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(e) = tv.power(on, only_ours.unwrap_or(false)).await {
            eprintln!("[tv] {}: {e}", if on { "on" } else { "off" });
        }
    });
}

/// Whether the TV refuses AzotoTV's tokens (START › Collega la TV).
#[tauri::command]
pub fn tv_unpaired(tv: tauri::State<'_, Arc<Tv>>) -> bool {
    tv.unpaired.load(Ordering::Relaxed)
}

/// START › Collega la TV: the TV shows a PIN, typed next with `tv_pair_pin`.
/// The connection is kept alive while the PIN is on screen.
#[tauri::command]
pub async fn tv_pair_start(tv: tauri::State<'_, Arc<Tv>>) -> Result<(), String> {
    let tv = tv.inner().clone();
    if !tv.enabled() {
        return Err(t!("No TV set up"));
    }
    tv.pair_cancel().await;
    let p = tv.pair_begin().await.map_err(|e| {
        eprintln!("[tv] pairing: {e}");
        t!("The TV isn't answering: turn it on with its remote and try again")
    })?;
    let id = p.id;
    *tv.pairing.lock().await = Some(p);
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(PAIR_PING).await;
            let mut slot = tv.pairing.lock().await;
            let Some(p) = slot.as_mut().filter(|p| p.id == id) else { break };
            if p.c.send(&[0xC0, 0]).await.is_err() {
                *slot = None;
                break;
            }
        }
    });
    Ok(())
}

/// The PIN from the TV's screen. On an error the pairing is over (start again).
#[tauri::command]
pub async fn tv_pair_pin(pin: String, tv: tauri::State<'_, Arc<Tv>>) -> Result<(), String> {
    let tv = tv.inner().clone();
    let p = tv.pairing.lock().await.take().ok_or_else(|| t!("The code has expired"))?;
    tv.pair_pin(p, &pin).await.map_err(|e| {
        eprintln!("[tv] pairing: {e}");
        if e.starts_with("PIN refused") { t!("Wrong code") } else { t!("The TV didn't answer") }
    })?;
    eprintln!("[tv] paired again from the UI");
    Ok(())
}

#[tauri::command]
pub async fn tv_pair_cancel(tv: tauri::State<'_, Arc<Tv>>) -> Result<(), String> {
    tv.pair_cancel().await;
    Ok(())
}
