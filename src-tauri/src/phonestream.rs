//! Mobile games streamed to the iPhone app (its own Moonlight client, built
//! on moonlight-common-c), through the same Sunshine as stream.rs.
//!
//! AzotoTV asks Sunshine for the stream, not the phone: Sunshine hands the
//! next RTSP connection to the pending launch whoever makes it (rtsp.cpp), so
//! the phone gets the session URL and the input key (`rikey`/`rikeyid`) from
//! here and needs no certificate or Moonlight pairing of its own. AzotoTV is a
//! paired Moonlight client of Sunshine (a certificate it makes once and writes
//! into Sunshine's state, [`ensure_paired`]).
//!
//! The order on the phone: `/api/app/play/{id}` (this module's [`start`]),
//! connect, then `/api/app/play/{id}/go`, which launches the game with its
//! phone mode (game.json `phone.args`) once stream.rs saw the client come in:
//! while streaming the panel stays on (games.rs `set_panel`), so the game opens
//! on the screen Sunshine sends. The phone leaving ends the game (stream.rs).
//!
//! The phone as a touch screen for a game on the TV uses a second Sunshine
//! with the same client code ([`Sunshine::connect`], [`launch`]), see
//! phonescreen.rs.

use std::{fs, path::PathBuf, process::Command, time::Duration};

use serde::Serialize;

/// Sunshine's HTTPS (paired clients) port.
const HTTPS: &str = "https://127.0.0.1:47984";
/// The Sunshine app the phone streams: the desktop (sunshine-apps.json).
const APP: &str = "AzotoTV";
const UNIT: &str = "app-dev.lizardbyte.app.Sunshine.service";
/// What AzotoTV calls itself among Sunshine's clients.
const NAME: &str = "AzotoTV (telefoni)";

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn state_path() -> PathBuf {
    home().join(".config/sunshine/sunshine_state.json")
}

/// AzotoTV's client certificate and key for Sunshine (PEM), made once.
pub(crate) fn identity() -> Result<(String, String), String> {
    let dir = crate::games::data_dir();
    let (cert, key) = (dir.join("sunshine-client.pem"), dir.join("sunshine-client.key"));
    if !cert.exists() || !key.exists() {
        let o = Command::new("openssl")
            .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256", "-days", "7300", "-subj", "/CN=AzotoTV"])
            .arg("-keyout")
            .arg(&key)
            .arg("-out")
            .arg(&cert)
            .output()
            .map_err(|e| format!("openssl: {e}"))?;
        if !o.status.success() {
            return Err(format!("openssl: {}", String::from_utf8_lossy(&o.stderr).trim()));
        }
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&key, fs::Permissions::from_mode(0o600));
    }
    let read = |p: &PathBuf| fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    Ok((read(&cert)?, read(&key)?))
}

/// AzotoTV among Sunshine's paired clients: added to its state with Sunshine
/// stopped (it rewrites the file from memory), then started again. Only when
/// missing, so once.
fn ensure_paired(cert: &str) -> Result<(), String> {
    let text = fs::read_to_string(state_path()).map_err(|e| format!("Sunshine: {e}"))?;
    let mut state: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("Sunshine: {e}"))?;
    let devices = state["root"]["named_devices"].as_array().cloned().unwrap_or_default();
    if devices.iter().any(|d| d["cert"].as_str().map(str::trim) == Some(cert.trim())) {
        return Ok(());
    }
    if crate::stream::active() {
        return Err("Sunshine è occupato: riprova dopo".into());
    }
    eprintln!("[phonestream] pairing AzotoTV with Sunshine");
    let systemctl = |what: &str| Command::new("systemctl").args(["--user", what, UNIT]).status();
    systemctl("stop").map_err(|e| e.to_string())?;
    // read again: Sunshine writes it when it stops
    let text = fs::read_to_string(state_path()).map_err(|e| format!("Sunshine: {e}"))?;
    state = serde_json::from_str(&text).map_err(|e| format!("Sunshine: {e}"))?;
    let mut uuid = [0u8; 16];
    random(&mut uuid);
    let entry = serde_json::json!({ "name": NAME, "cert": cert, "uuid": hex(&uuid).to_uppercase() });
    match state["root"]["named_devices"].as_array_mut() {
        Some(list) => list.push(entry),
        None => state["root"]["named_devices"] = serde_json::json!([entry]),
    }
    let written = fs::write(state_path(), serde_json::to_string_pretty(&state).unwrap_or_default());
    systemctl("start").map_err(|e| e.to_string())?;
    written.map_err(|e| format!("Sunshine: {e}"))?;
    // Sunshine finds an encoder in a few seconds
    std::thread::sleep(Duration::from_secs(4));
    Ok(())
}

fn random(buf: &mut [u8]) {
    use ring::rand::SecureRandom;
    let _ = ring::rand::SystemRandom::new().fill(buf);
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `<name>value</name>`'s value, the first one.
fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{name}>"))? + start;
    Some(xml[start..end].trim())
}

/// The root's `status_code` and `status_message`.
fn status(xml: &str) -> (u32, String) {
    let attr = |a: &str| {
        let k = format!("{a}=\"");
        let s = xml.find(&k)? + k.len();
        Some(xml[s..s + xml[s..].find('"')?].to_string())
    };
    (attr("status_code").and_then(|c| c.parse().ok()).unwrap_or(0), attr("status_message").unwrap_or_default())
}

/// A Sunshine AzotoTV is a paired client of, at `base` (its HTTPS port).
pub(crate) struct Sunshine {
    http: reqwest::Client,
    base: &'static str,
}

impl Sunshine {
    /// The main Sunshine (stream.rs's), AzotoTV paired into it first.
    fn new() -> Result<Sunshine, String> {
        ensure_paired(&identity()?.0)?;
        Sunshine::connect(HTTPS)
    }

    pub(crate) fn connect(base: &'static str) -> Result<Sunshine, String> {
        let (cert, key) = identity()?;
        use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        let certs = CertificateDer::pem_slice_iter(cert.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("certificato: {e}"))?;
        let key = PrivateKeyDer::from_pem_slice(key.as_bytes()).map_err(|e| format!("certificato: {e}"))?;
        let mut tls = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .dangerous()
            // Sunshine's own self-signed certificate, on this machine
            .with_custom_certificate_verifier(std::sync::Arc::new(crate::tv::AnyServer(provider)))
            .with_client_auth_cert(certs, key)
            .map_err(|e| format!("certificato: {e}"))?;
        // Sunshine refuses a resumed TLS session (no client certificate in
        // it): every other request failed with "InternalError"
        tls.resumption = rustls::client::Resumption::disabled();
        let http = reqwest::Client::builder()
            .use_preconfigured_tls(tls)
            // Sunshine closes the connection after every answer
            .pool_max_idle_per_host(0)
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Sunshine { http, base })
    }

    pub(crate) async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<String, String> {
        let mut q: Vec<(&str, String)> = vec![("uniqueid", "0123456789ABCDEF".into()), ("uuid", uuid())];
        q.extend_from_slice(query);
        let r = self.http.get(format!("{}/{path}", self.base)).query(&q).send().await;
        let text = r.map_err(|e| format!("Sunshine non risponde: {e}"))?.text().await.map_err(|e| e.to_string())?;
        match status(&text) {
            (200, _) => Ok(text),
            (code, msg) => Err(format!("Sunshine: {msg} ({code})")),
        }
    }

    async fn app_id(&self) -> Result<String, String> {
        let list = self.get("applist", &[]).await?;
        list.split("<App>")
            .skip(1)
            .find(|a| tag(a, "AppTitle") == Some(APP))
            .and_then(|a| tag(a, "ID"))
            .map(str::to_string)
            .ok_or_else(|| format!("Sunshine non ha l'app {APP}"))
    }
}

fn uuid() -> String {
    let mut b = [0u8; 16];
    random(&mut b);
    let h = hex(&b);
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// What the phone needs to connect (moonlight-common-c's SERVER_INFORMATION
/// and the input key of STREAM_CONFIGURATION).
#[derive(Serialize)]
pub struct Session {
    /// RTSP URL, with the address the phone reached AzotoTV at
    pub url: String,
    pub address: String,
    /// AES key (hex) and IV seed for input, as Moonlight's rikey/rikeyid
    pub key: String,
    pub key_id: u32,
    pub app_version: String,
    pub gfe_version: String,
    pub codecs: u32,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

/// A stream for the phone at `host` (the address it used to reach AzotoTV):
/// Sunshine's desktop app launched with a fresh input key. A session left from
/// an earlier stream (the app still "running" with no client) is cancelled.
pub async fn start(host: &str, width: u32, height: u32, fps: u32) -> Result<Session, String> {
    if crate::stream::active() {
        return Err("C'è già qualcuno collegato in streaming".into());
    }
    let s = tauri::async_runtime::spawn_blocking(Sunshine::new).await.map_err(|e| e.to_string())??;
    launch(&s, host, width, height, fps, false).await
}

/// Sunshine's desktop app launched for a phone at `host`, with a fresh input
/// key; `host_audio`: the sound stays on this machine (Moonlight's "play audio
/// on host") instead of going to the phone.
pub(crate) async fn launch(s: &Sunshine, host: &str, width: u32, height: u32, fps: u32, host_audio: bool) -> Result<Session, String> {
    let info = s.get("serverinfo", &[]).await?;
    if tag(&info, "PairStatus") != Some("1") {
        return Err("AzotoTV non è associato a Sunshine".into());
    }
    if tag(&info, "currentgame").is_some_and(|g| g != "0") {
        s.get("cancel", &[]).await?;
    }
    let app = s.app_id().await?;
    let mut key = [0u8; 16];
    random(&mut key);
    let mut id = [0u8; 4];
    random(&mut id);
    let key_id = u32::from_be_bytes(id) & 0x7fff_ffff;
    let launch = s
        .get(
            "launch",
            &[
                ("appid", app),
                ("mode", format!("{width}x{height}x{fps}")),
                ("additionalStates", "1".into()),
                ("sops", "0".into()),
                ("rikey", hex(&key)),
                ("rikeyid", key_id.to_string()),
                ("localAudioPlayMode", if host_audio { "1" } else { "0" }.into()),
                ("surroundAudioInfo", "196610".into()),
                ("remoteControllersBitmap", "0".into()),
                ("gcmap", "0".into()),
                ("corever", "1".into()),
            ],
        )
        .await?;
    let url = tag(&launch, "sessionUrl0").ok_or("Sunshine non ha dato l'indirizzo dello stream")?;
    // rtspenc://127.0.0.1:48010 (the port follows Sunshine's) -> the phone's view of this machine
    let (scheme, rest) = url.split_once("://").ok_or("indirizzo dello stream non valido")?;
    let port = rest.rsplit_once(':').map(|(_, p)| p).ok_or("indirizzo dello stream senza porta")?;
    Ok(Session {
        url: format!("{scheme}://{host}:{port}"),
        address: host.to_string(),
        key: hex(&key),
        key_id,
        app_version: tag(&info, "appversion").unwrap_or("7.1.431.-1").to_string(),
        gfe_version: tag(&info, "GfeVersion").unwrap_or("3.23.0.74").to_string(),
        codecs: tag(&info, "ServerCodecModeSupport").and_then(|c| c.parse().ok()).unwrap_or(1),
        width,
        height,
        fps,
    })
}

/// Sunshine's session closed (the phone is done).
pub async fn cancel() {
    let Ok(Ok(s)) = tauri::async_runtime::spawn_blocking(Sunshine::new).await else { return };
    if let Err(e) = s.get("cancel", &[]).await {
        eprintln!("[phonestream] cancel: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sunshine_xml() {
        let x = r#"<?xml version="1.0"?><root status_code="200"><appversion>7.1.431.-1</appversion><sessionUrl0>rtspenc://127.0.0.1:48010</sessionUrl0></root>"#;
        assert_eq!(status(x), (200, String::new()));
        assert_eq!(tag(x, "sessionUrl0"), Some("rtspenc://127.0.0.1:48010"));
        let bad = r#"<root status_code="400" status_message="An app is already running on this host"><resume>0</resume></root>"#;
        assert_eq!(status(bad), (400, "An app is already running on this host".into()));
        let apps = "<root status_code=\"200\"><App><AppTitle>Desktop</AppTitle><ID>1</ID></App><App><AppTitle>AzotoTV</AppTitle><ID>2086294929</ID></App></root>";
        let id = apps.split("<App>").skip(1).find(|a| tag(a, "AppTitle") == Some(APP)).and_then(|a| tag(a, "ID"));
        assert_eq!(id, Some("2086294929"));
    }
}
