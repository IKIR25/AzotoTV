//! Jellyfin access layer. All network traffic goes through here so the webview
//! never touches the network directly (no CORS / mixed-content issues, tokens
//! never reach JS, and images get a disk cache).
//!
//! Profiles: every user who signed in on this device keeps their own session
//! (token) in `~/.config/azototv/profiles.json`, so switching profile is instant.
//! Each profile has its own DeviceId: Jellyfin may revoke other tokens issued
//! to the same DeviceId on login.

use std::{fs, path::PathBuf, sync::Mutex, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const CLIENT: &str = "AzotoTV";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub base: String,
    pub token: String,
    pub user_id: String,
    pub user_name: String,
    /// DeviceId the token was issued to (empty = the old shared one).
    #[serde(default)]
    pub device_id: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    /// Server URL (also known while nobody is signed in, for the profile picker).
    base: Option<String>,
    /// user_id of the active profile
    current: Option<String>,
    sessions: Vec<Session>,
    /// The server's public users as last seen, so the picker still works while
    /// Jellyfin is down.
    #[serde(default)]
    public: Vec<PublicUser>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct PublicUser {
    id: String,
    name: String,
    image_tag: Option<String>,
    has_password: bool,
}

/// A user on the profile picker.
#[derive(Serialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub image_tag: Option<String>,
    pub has_password: bool,
    /// we hold a token for this user on this device
    pub signed_in: bool,
    /// asks AzotoTV's PIN on the picker (filled in by lib.rs from profile.rs)
    pub pin: bool,
}

/// The profile picker's list; `offline`: Jellyfin didn't answer, this is the saved list.
#[derive(Serialize)]
pub struct Profiles {
    pub profiles: Vec<Profile>,
    pub offline: bool,
}

pub struct Jf {
    client: reqwest::Client,
    store: Mutex<Store>,
    host: String,
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(fallback)
    })
}

pub(crate) fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("azototv")
}

pub(crate) fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("azototv")
}

pub(crate) fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// GET an image through the on-disk cache (`~/.cache/azototv/img`), keyed by URL.
pub(crate) async fn cached_image(
    req: reqwest::RequestBuilder,
    url: &str,
) -> Result<(Vec<u8>, String), String> {
    let key = format!("{:016x}", fnv1a(url));
    let dir = cache_dir().join("img");
    let data_path = dir.join(&key);
    let type_path = dir.join(format!("{key}.type"));
    if let (Ok(data), Ok(ct)) = (fs::read(&data_path), fs::read_to_string(&type_path)) {
        return Ok((data, ct));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("image HTTP {}", resp.status()));
    }
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let data = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
    if fs::create_dir_all(&dir).is_ok() {
        let _ = fs::write(&data_path, &data);
        let _ = fs::write(&type_path, &ct);
    }
    Ok((data, ct))
}

fn load_store() -> Store {
    let dir = config_dir();
    if let Some(s) = fs::read_to_string(dir.join("profiles.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Store>(&s).ok())
    {
        return s;
    }
    // migrate the single-user session.json of earlier versions
    match fs::read_to_string(dir.join("session.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Session>(&s).ok())
    {
        Some(s) => Store {
            base: Some(s.base.clone()),
            current: Some(s.user_id.clone()),
            sessions: vec![s],
            public: Vec::new(),
        },
        None => Store::default(),
    }
}

impl Jf {
    pub fn new() -> Self {
        let host = fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "device".into())
            .trim()
            .to_string();
        Jf {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("http client"),
            store: Mutex::new(load_store()),
            host,
        }
    }

    fn save(&self, st: &Store) {
        let dir = config_dir();
        if fs::create_dir_all(&dir).is_ok() {
            let path = dir.join("profiles.json");
            let _ = fs::write(&path, serde_json::to_string(st).unwrap());
            // tokens inside: owner-only
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            }
            let _ = fs::remove_file(dir.join("session.json"));
        }
    }

    fn auth_header(&self, device_id: &str, token: Option<&str>) -> String {
        let device_id = if device_id.is_empty() { format!("azototv-{}", self.host) } else { device_id.to_string() };
        let mut h = format!(
            "MediaBrowser Client=\"{CLIENT}\", Device=\"{CLIENT}\", DeviceId=\"{device_id}\", Version=\"{VERSION}\""
        );
        if let Some(t) = token {
            h.push_str(&format!(", Token=\"{t}\""));
        }
        h
    }

    /// The active profile's session.
    pub fn session(&self) -> Option<Session> {
        let st = self.store.lock().unwrap();
        let cur = st.current.as_ref()?;
        st.sessions.iter().find(|s| &s.user_id == cur).cloned()
    }

    /// The session this device holds for `user_id` (signed in on the TV), active or not.
    pub fn session_of(&self, user_id: &str) -> Option<Session> {
        self.store.lock().unwrap().sessions.iter().find(|s| s.user_id == user_id).cloned()
    }

    pub fn server(&self) -> Option<String> {
        self.store.lock().unwrap().base.clone()
    }

    /// Check that `base` is a Jellyfin server and remember it. Returns its name.
    pub async fn set_server(&self, base: &str) -> Result<String, String> {
        let base = base.trim().trim_end_matches('/').to_string();
        let v: Value = self
            .client
            .get(format!("{base}/System/Info/Public"))
            .send()
            .await
            .map_err(|e| t!("can't reach the server: {e}", e = e))?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|_| t!("this doesn't look like a Jellyfin server"))?;
        let mut st = self.store.lock().unwrap();
        if st.base.as_deref() != Some(base.as_str()) {
            st.public.clear(); // another server's users
        }
        st.base = Some(base);
        self.save(&st);
        Ok(v["ServerName"].as_str().unwrap_or("Jellyfin").to_string())
    }

    async fn public_users(&self, base: &str) -> Result<Vec<PublicUser>, String> {
        let users: Vec<Value> = self
            .client
            .get(format!("{base}/Users/Public"))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| t!("can't reach the server: {e}", e = e))?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        Ok(users
            .iter()
            .filter_map(|u| {
                Some(PublicUser {
                    id: u["Id"].as_str()?.to_string(),
                    name: u["Name"].as_str().unwrap_or_default().to_string(),
                    image_tag: u["PrimaryImageTag"].as_str().map(str::to_string),
                    has_password: u["HasPassword"].as_bool().unwrap_or(true),
                })
            })
            .collect())
    }

    /// Users shown on the login screen of the server, marked if we hold a token.
    /// If the server doesn't answer, the list it gave last time.
    pub async fn profiles(&self) -> Result<Profiles, String> {
        let base = self.server().ok_or_else(|| t!("no server"))?;
        let fetched = self.public_users(&base).await;
        let mut st = self.store.lock().unwrap();
        let offline = match fetched {
            Ok(users) => {
                if st.public != users {
                    st.public = users;
                    self.save(&st);
                }
                false
            }
            Err(e) if st.public.is_empty() && st.sessions.is_empty() => return Err(e),
            Err(_) => true,
        };
        let mut out: Vec<Profile> = st
            .public
            .iter()
            .map(|u| Profile {
                signed_in: st.sessions.iter().any(|s| s.user_id == u.id),
                id: u.id.clone(),
                name: u.name.clone(),
                image_tag: u.image_tag.clone(),
                has_password: u.has_password,
                pin: false,
            })
            .collect();
        // hidden users we are signed in as still get a tile
        for s in st.sessions.iter().filter(|s| Some(&s.base) == st.base.as_ref()) {
            if !out.iter().any(|p| p.id == s.user_id) {
                out.push(Profile {
                    id: s.user_id.clone(),
                    name: s.user_name.clone(),
                    image_tag: None,
                    has_password: true,
                    signed_in: true,
                    pin: false,
                });
            }
        }
        Ok(Profiles { profiles: out, offline })
    }

    /// Make a signed-in profile the active one.
    pub fn switch(&self, user_id: &str) -> Result<Session, String> {
        let mut st = self.store.lock().unwrap();
        let s = st
            .sessions
            .iter()
            .find(|s| s.user_id == user_id)
            .cloned()
            .ok_or_else(|| t!("profile not signed in"))?;
        st.current = Some(user_id.to_string());
        self.save(&st);
        Ok(s)
    }

    /// Leave the active profile without forgetting its token (back to the picker).
    pub fn deactivate(&self) {
        let mut st = self.store.lock().unwrap();
        st.current = None;
        self.save(&st);
    }

    /// Forget a profile on this device (and revoke its token on the server).
    pub async fn sign_out(&self, user_id: &str) {
        let removed = {
            let mut st = self.store.lock().unwrap();
            let i = st.sessions.iter().position(|s| s.user_id == user_id);
            let removed = i.map(|i| st.sessions.remove(i));
            if st.current.as_deref() == Some(user_id) {
                st.current = None;
            }
            self.save(&st);
            removed
        };
        if let Some(s) = removed {
            let _ = self
                .client
                .post(format!("{}/Sessions/Logout", s.base))
                .header("Authorization", self.auth_header(&s.device_id, Some(&s.token)))
                .send()
                .await;
        }
    }

    pub async fn login(&self, base: &str, user: &str, pass: &str) -> Result<Session, String> {
        let base = base.trim().trim_end_matches('/').to_string();
        let device_id = format!("azototv-{}-{:08x}", self.host, fnv1a(&user.to_lowercase()) as u32);
        let resp = self
            .client
            .post(format!("{base}/Users/AuthenticateByName"))
            .header("Authorization", self.auth_header(&device_id, None))
            .json(&serde_json::json!({ "Username": user, "Pw": pass }))
            .send()
            .await
            .map_err(|e| t!("can't reach the server: {e}", e = e))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(t!("Wrong password"));
        }
        if !resp.status().is_success() {
            return Err(t!("sign-in failed ({a})", a = resp.status()));
        }
        let v: Value = resp.json().await.map_err(|e| e.to_string())?;
        let s = Session {
            base: base.clone(),
            token: v["AccessToken"].as_str().ok_or("no token in reply")?.to_string(),
            user_id: v["User"]["Id"].as_str().ok_or("no user id")?.to_string(),
            user_name: v["User"]["Name"].as_str().unwrap_or(user).to_string(),
            device_id,
        };
        let mut st = self.store.lock().unwrap();
        st.sessions.retain(|x| x.user_id != s.user_id);
        st.sessions.push(s.clone());
        st.current = Some(s.user_id.clone());
        st.base = Some(base);
        self.save(&st);
        Ok(s)
    }

    /// Generic JSON request relative to the server base URL, as the active profile.
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        let s = self.session().ok_or("not logged in")?;
        self.request_as(&s, method, path, query, body).await
    }

    /// Like `request`, for data that is the same for every profile (the live TV
    /// guide): as the active profile, or else any signed-in one (on the picker).
    pub async fn request_shared(&self, path: &str, query: &[(String, String)]) -> Result<Value, String> {
        let s = self.session().or_else(|| self.store.lock().unwrap().sessions.first().cloned());
        self.request_as(&s.ok_or("not logged in")?, "GET", path, query, None).await
    }

    async fn request_as(
        &self,
        s: &Session,
        method: &str,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
        let mut rb = self
            .client
            .request(method, format!("{}{}", s.base, path))
            .header("Authorization", self.auth_header(&s.device_id, Some(&s.token)))
            .query(query);
        if let Some(b) = body {
            rb = rb.json(&b);
        }
        let resp = rb.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err("unauthorized".into());
        }
        if !status.is_success() {
            return Err(format!("{path}: HTTP {status}"));
        }
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|e| format!("bad JSON from {path}: {e}"))
    }

    /// Fetch an image (path + query relative to the server), via the disk cache.
    /// Works without a profile too (user pictures on the picker are public).
    pub async fn image(&self, path_and_query: &str) -> Result<(Vec<u8>, String), String> {
        let (base, auth) = match self.session() {
            Some(s) => (s.base.clone(), self.auth_header(&s.device_id, Some(&s.token))),
            None => (self.server().ok_or_else(|| t!("no server"))?, self.auth_header("", None)),
        };
        let url = format!("{base}{path_and_query}");
        let req = self.client.get(&url).header("Authorization", auth);
        cached_image(req, &url).await
    }
}
