//! The iPhone app's Jellyfin (TV, Diretta TV, Musica on the phone).
//!
//! Each phone gets its own Jellyfin login for each of its profiles, without a
//! password: AzotoTV starts a Quick Connect request as the phone's device and
//! approves it with the TV's token for that profile. So the phone is a device
//! of its own in Jellyfin ("iPhone di Ada"), with its own sessions, and resume
//! points are the profile's, shared with the TV. The tokens stay here
//! (`phone-jellyfin.json`, mode 600) and never reach the phone: the app talks to
//! Jellyfin through `/api/app/jf/{key}/{user}/<Jellyfin path>` (phone.rs), which
//! adds the token. The phone's key is in the path, not in a header, because the
//! video player can't send headers and resolves HLS playlists' relative links
//! against that path.
//!
//! Only what the app needs passes (browsing, images, streams, playback reports,
//! watched / favourite, playlists): nothing that changes the library or users.

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{LazyLock, Mutex},
    time::Duration,
};

use axum::{
    body::Body,
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use crate::jellyfin::Jf;

const CLIENT: &str = "AzotoTV iPhone";
const VERSION: &str = env!("CARGO_PKG_VERSION");

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder().connect_timeout(Duration::from_secs(5)).build().expect("http client")
});

/// "phone id:user id" -> that phone's Jellyfin token for that profile.
static TOKENS: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()));

/// Quick Connect requests one at a time (two first requests of a new phone would make two logins).
static LOGIN: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

fn file() -> PathBuf {
    crate::games::data_dir().join("phone-jellyfin.json")
}

fn save(map: &HashMap<String, String>) {
    use std::os::unix::fs::OpenOptionsExt;
    let path = file();
    let tmp = path.with_extension("json.tmp");
    let ok = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| std::io::Write::write_all(&mut f, serde_json::to_string_pretty(map).unwrap().as_bytes()))
        .and_then(|_| fs::rename(&tmp, &path));
    if let Err(e) = ok {
        eprintln!("[phonejf] cannot save: {e}");
    }
}

fn key(phone: &str, user: &str) -> String {
    format!("{phone}:{user}")
}

/// The phone's Jellyfin DeviceId for a profile (one per profile: a login may
/// end the other tokens of the same device).
fn device_id(phone: &str, user: &str) -> String {
    format!("azototv-phone-{phone}-{:08x}", crate::jellyfin::fnv1a(user) as u32)
}

fn auth_header(label: &str, device: &str, token: Option<&str>) -> String {
    let label = label.replace('"', "'");
    let mut h = format!("MediaBrowser Client=\"{CLIENT}\", Device=\"{label}\", DeviceId=\"{device}\", Version=\"{VERSION}\"");
    if let Some(t) = token {
        h.push_str(&format!(", Token=\"{t}\""));
    }
    h
}

/// The phone's token for `user`: the one it has, or a new one through Quick Connect.
async fn token(jf: &Jf, phone: &str, label: &str, user: &str) -> Result<String, String> {
    if let Some(t) = TOKENS.lock().unwrap().get(&key(phone, user)) {
        return Ok(t.clone());
    }
    let _one = LOGIN.lock().await;
    if let Some(t) = TOKENS.lock().unwrap().get(&key(phone, user)) {
        return Ok(t.clone());
    }
    let tv = jf.session_of(user).ok_or("Questo profilo non è più aperto sulla TV")?;
    let base = tv.base.trim_end_matches('/').to_string();
    let dev = device_id(phone, user);
    let call = |method: Method, path: &str, auth: String, body: Option<Value>| {
        let mut rb = HTTP.request(method, format!("{base}{path}")).header("Authorization", auth).timeout(Duration::from_secs(10));
        if let Some(b) = body {
            rb = rb.json(&b);
        }
        async move {
            let r = rb.send().await.map_err(|e| e.to_string())?;
            let status = r.status();
            if !status.is_success() {
                return Err(format!("Jellyfin: HTTP {status}"));
            }
            let bytes = r.bytes().await.map_err(|e| e.to_string())?;
            Ok::<Value, String>(serde_json::from_slice(&bytes).unwrap_or(Value::Null))
        }
    };
    let init = call(Method::POST, "/QuickConnect/Initiate", auth_header(label, &dev, None), None).await?;
    let (Some(code), Some(secret)) = (init["Code"].as_str(), init["Secret"].as_str()) else {
        return Err("Quick Connect non risponde".into());
    };
    // the TV's own login for the profile says yes
    let tv_auth = format!(
        "MediaBrowser Client=\"AzotoTV\", Device=\"AzotoTV\", DeviceId=\"{}\", Version=\"{VERSION}\", Token=\"{}\"",
        tv.device_id, tv.token
    );
    call(Method::POST, &format!("/QuickConnect/Authorize?code={code}&userId={user}"), tv_auth, None).await?;
    let auth = call(Method::POST, "/Users/AuthenticateWithQuickConnect", auth_header(label, &dev, None), Some(json!({ "Secret": secret }))).await?;
    let t = auth["AccessToken"].as_str().ok_or("Jellyfin non ha dato l'accesso")?.to_string();
    eprintln!("[phonejf] phone {phone} has its own Jellyfin login for {}", auth["User"]["Name"].as_str().unwrap_or(user));
    let mut all = TOKENS.lock().unwrap();
    all.insert(key(phone, user), t.clone());
    save(&all);
    Ok(t)
}

fn drop_token(phone: &str, user: &str) -> Option<String> {
    let mut all = TOKENS.lock().unwrap();
    let t = all.remove(&key(phone, user));
    if t.is_some() {
        save(&all);
    }
    t
}

/// The phone left `user`'s profile (or was unpaired): its Jellyfin login ends too.
pub fn forget(jf: &Jf, phone: &str, user: &str) {
    let Some(t) = drop_token(phone, user) else { return };
    let Some(base) = jf.server() else { return };
    let auth = auth_header("", &device_id(phone, user), Some(&t));
    tauri::async_runtime::spawn(async move {
        let _ = HTTP
            .post(format!("{}/Sessions/Logout", base.trim_end_matches('/')))
            .header("Authorization", auth)
            .timeout(Duration::from_secs(10))
            .send()
            .await;
    });
}

/// May a phone ask Jellyfin this? Browsing and playing yes, changing the library or users no.
fn allowed(method: &Method, path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    let seg: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    let first = seg.first().copied().unwrap_or("");
    match *method {
        Method::GET => matches!(
            first,
            "items" | "useritems" | "userviews" | "shows" | "videos" | "audio" | "livetv" | "mediasegments" | "search"
                | "genres" | "persons" | "artists" | "musicgenres" | "studios" | "playlists" | "movies"
        ) || (first == "users" && (seg.len() == 2 && seg[1] == "me" || seg.get(2) == Some(&"images"))),
        Method::POST => {
            (first == "items" && seg.get(2) == Some(&"playbackinfo"))
                || (first == "sessions" && seg.get(1) == Some(&"playing"))
                || matches!(first, "userplayeditems" | "userfavoriteitems" | "playlists")
                || (first == "livetv" && seg.get(1) == Some(&"livestreams"))
        }
        Method::DELETE => {
            matches!(first, "userplayeditems" | "userfavoriteitems")
                || (first == "playlists" && seg.get(2) == Some(&"items"))
                || (first == "videos" && seg.get(1) == Some(&"activeencodings"))
        }
        _ => false,
    }
}

/// HLS playlists name the token in some links (`api_key=`): out, the proxy adds it.
fn scrub(text: &str, token: &str) -> String {
    let mut s = text.to_string();
    for k in ["api_key", "ApiKey", "api_Key"] {
        s = s.replace(&format!("?{k}={token}&"), "?").replace(&format!("&{k}={token}"), "").replace(&format!("?{k}={token}"), "");
    }
    s.replace(token, "")
}

/// One request of the phone, passed to Jellyfin as the phone's login for `user`.
pub async fn proxy(
    jf: &Jf,
    phone: &str,
    label: &str,
    user: &str,
    method: Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: Vec<u8>,
) -> Response {
    if !allowed(&method, path) {
        return (StatusCode::FORBIDDEN, axum::Json(json!({ "error": "Non permesso dal telefono" }))).into_response();
    }
    let Some(base) = jf.server() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let url = match query {
        Some(q) if !q.is_empty() => format!("{}/{path}?{q}", base.trim_end_matches('/')),
        _ => format!("{}/{path}", base.trim_end_matches('/')),
    };
    let dev = device_id(phone, user);
    // a token Jellyfin no longer knows (revoked from its dashboard): a new login, once
    for attempt in 0..2 {
        let t = match token(jf, phone, label, user).await {
            Ok(t) => t,
            Err(e) => return (StatusCode::BAD_GATEWAY, axum::Json(json!({ "error": e }))).into_response(),
        };
        let mut rb = HTTP.request(method.clone(), &url).header("Authorization", auth_header(label, &dev, Some(&t)));
        for name in [header::RANGE, header::CONTENT_TYPE, header::ACCEPT, header::IF_NONE_MATCH, header::IF_MODIFIED_SINCE] {
            if let Some(v) = headers.get(&name) {
                rb = rb.header(name, v);
            }
        }
        if !body.is_empty() {
            rb = rb.body(body.clone());
        }
        let resp = match rb.send().await {
            Ok(r) => r,
            Err(e) => return (StatusCode::BAD_GATEWAY, axum::Json(json!({ "error": e.to_string() }))).into_response(),
        };
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
            drop_token(phone, user);
            continue;
        }
        let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let ctype = resp.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_ascii_lowercase();
        let mut out = Response::builder().status(status);
        let playlist = ctype.contains("mpegurl") || path.to_ascii_lowercase().ends_with(".m3u8");
        for name in [
            header::CONTENT_TYPE,
            header::CONTENT_RANGE,
            header::ACCEPT_RANGES,
            header::CACHE_CONTROL,
            header::ETAG,
            header::LAST_MODIFIED,
            header::CONTENT_DISPOSITION,
        ] {
            if let Some(v) = resp.headers().get(name.as_str()) {
                out = out.header(name, v.as_bytes());
            }
        }
        if playlist {
            let text = resp.text().await.unwrap_or_default();
            return out.body(Body::from(scrub(&text, &t))).unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response());
        }
        if let Some(v) = resp.headers().get(header::CONTENT_LENGTH) {
            out = out.header(header::CONTENT_LENGTH, v.as_bytes());
        }
        return out.body(Body::from_stream(resp.bytes_stream())).unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response());
    }
    StatusCode::UNAUTHORIZED.into_response()
}
