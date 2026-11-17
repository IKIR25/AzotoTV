//! AzotoTV's own settings of a profile (`profile.json` in its player folder):
//! an optional PIN asked on "who's watching" (off by default), and whether the
//! automatic TG alerts (notify.rs) are for them.

use std::{
    collections::HashMap,
    fs,
    sync::Mutex,
    time::{Duration, Instant},
};

use ring::{digest, rand::SecureRandom};
use serde::{Deserialize, Serialize};

use crate::games::player_dir;

/// Wrong PINs in a row before the picker makes you wait `WAIT`.
const TRIES: u32 = 5;
const WAIT: Duration = Duration::from_secs(30);

#[derive(Serialize, Deserialize)]
struct Stored {
    /// "<salt hex>$<sha256(salt + pin) hex>"
    #[serde(default)]
    pin: Option<String>,
    #[serde(default = "yes")]
    tg_alerts: bool,
}

fn yes() -> bool {
    true
}

impl Default for Stored {
    fn default() -> Self {
        Stored { pin: None, tg_alerts: true }
    }
}

/// What JS gets (never the PIN itself).
#[derive(Serialize)]
pub struct Prefs {
    pub pin: bool,
    pub tg_alerts: bool,
    /// some programmes are announced by themselves here (reminders.json), so
    /// `tg_alerts` is worth showing
    pub auto_reminders: bool,
}

fn load(user: &str) -> Stored {
    if user.is_empty() {
        return Stored::default();
    }
    fs::read_to_string(player_dir(user).join("profile.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(user: &str, s: &Stored) -> Result<(), String> {
    let dir = player_dir(user);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let t = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    fs::write(dir.join("profile.json"), t).map_err(|e| e.to_string())
}

pub fn prefs(user: &str) -> Prefs {
    let s = load(user);
    Prefs { pin: s.pin.is_some(), tg_alerts: s.tg_alerts, auto_reminders: crate::notify::has_rules() }
}

pub fn has_pin(user: &str) -> bool {
    load(user).pin.is_some()
}

pub fn tg_alerts(user: &str) -> bool {
    load(user).tg_alerts
}

pub fn set_tg_alerts(user: &str, on: bool) -> Result<(), String> {
    let mut s = load(user);
    s.tg_alerts = on;
    save(user, &s)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hash(salt: &str, pin: &str) -> String {
    hex(digest::digest(&digest::SHA256, format!("{salt}{pin}").as_bytes()).as_ref())
}

/// Set the PIN (4 digits), or take it away (`None`).
pub fn set_pin(user: &str, pin: Option<&str>) -> Result<(), String> {
    let mut s = load(user);
    s.pin = match pin {
        // a linked Google account (ytaccount.rs) is only behind the PIN
        None if crate::ytaccount::linked(user) => return Err(t!("first unlink the Google account from YouTube")),
        None => None,
        Some(p) if p.len() == 4 && p.bytes().all(|b| b.is_ascii_digit()) => {
            let mut salt = [0u8; 16];
            ring::rand::SystemRandom::new().fill(&mut salt).map_err(|_| t!("no random numbers available"))?;
            let salt = hex(&salt);
            Some(format!("{salt}${}", hash(&salt, p)))
        }
        Some(_) => return Err(t!("The PIN has 4 digits")),
    };
    save(user, &s)
}

/// Wrong tries in a row, and since when the last one has to wait, per profile.
static FAILS: Mutex<Option<HashMap<String, (u32, Instant)>>> = Mutex::new(None);

/// Whether `pin` opens `user` (a profile without a PIN: always). After `TRIES`
/// wrong ones in a row it refuses for `WAIT`, saying how long.
pub fn check_pin(user: &str, pin: &str) -> Result<bool, String> {
    let Some(stored) = load(user).pin else { return Ok(true) };
    let mut fails = FAILS.lock().unwrap();
    let fails = fails.get_or_insert_with(HashMap::new);
    if let Some(&(n, at)) = fails.get(user) {
        let left = WAIT.saturating_sub(at.elapsed());
        if n >= TRIES && !left.is_zero() {
            return Err(t!("Too many tries: try again in {a} seconds", a = left.as_secs().max(1)));
        }
    }
    let ok = stored.split_once('$').is_some_and(|(salt, h)| hash(salt, pin) == h);
    if ok {
        fails.remove(user);
    } else {
        let e = fails.entry(user.to_string()).or_insert((0, Instant::now()));
        e.0 += 1;
        e.1 = Instant::now();
    }
    Ok(ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_and_tries() {
        crate::test_data_home();
        let u = "test-user";
        assert!(!prefs(u).pin && prefs(u).tg_alerts);
        assert_eq!(check_pin(u, "0000"), Ok(true)); // no PIN: anything opens it
        assert!(set_pin(u, Some("12a4")).is_err());
        set_pin(u, Some("1234")).unwrap();
        assert!(prefs(u).pin);
        assert_eq!(check_pin(u, "1234"), Ok(true));
        for _ in 0..TRIES {
            assert_eq!(check_pin(u, "9999"), Ok(false));
        }
        assert!(check_pin(u, "1234").is_err()); // waiting now, even with the right one
        set_tg_alerts(u, false).unwrap();
        set_pin(u, None).unwrap();
        assert!(!prefs(u).pin && !prefs(u).tg_alerts);
    }
}
