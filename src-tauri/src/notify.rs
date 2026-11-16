//! Notifications: one feed for the whole box (it's a shared TV). A new one pops
//! up as a toast over whatever is on screen (overlay.rs; the UI draws it itself
//! when the overlay isn't available) and stays in the list behind the START
//! menu's "Notifiche", where A opens what it is about.
//!
//! Sources: arr.rs (films / episodes that just arrived) and the live TV
//! reminders kept here ("Ricordamelo" on a programme: a toast a minute before).
//! The family's telegiornali (`TG`) get reminders by themselves, from the
//! guide: ten minutes before, and again when they start unless that channel is
//! already on (then the player offers "Guarda il Tg1" on A). A profile can turn
//! those off (profile.rs): no toast while it's the one on the TV, and not in its list.
//! State: `~/.local/share/azototv/notifications.json`.

use std::{
    collections::HashMap,
    fs,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::{games::data_dir, jellyfin::Jf, overlay, pad, player::Player};

/// Kept in the list.
const KEEP: usize = 60;
/// Reminders fire this long before the programme starts.
const REMIND_BEFORE: u64 = 60_000;
/// A reminder found later than this after the start (AzotoTV was off) is dropped.
const REMIND_LATE: u64 = 5 * 60_000;

/// Telegiornali announced without anyone asking: channel, programme name in the
/// guide, usual local start times (minutes after midnight, give or take `TG_SLACK`).
const TG: &[(&str, &str, &[u32])] = &[
    ("Rai 1", "Tg1", &[13 * 60 + 30, 20 * 60]),
    ("Rai 2", "Tg2", &[13 * 60, 20 * 60 + 30]),
    ("Rai 3", "Tg3", &[14 * 60 + 20, 19 * 60]),
];
const TG_SLACK: u32 = 20;
/// The first toast comes this long before a TG.
const TG_BEFORE: u64 = 10 * 60_000;
/// How long the player offers "Guarda il Tg1" once it has started.
const TG_OFFER: u64 = 60_000;
/// The guide is read again this often (it's kept 26 h ahead).
const TG_REFRESH: Duration = Duration::from_secs(30 * 60);

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: u64,
    /// unix ms
    pub time: u64,
    /// "movie" | "episode" | "reminder" | "onair" (a TG just started) | "test"
    pub kind: String,
    pub kicker: String,
    pub title: String,
    pub body: String,
    /// Jellyfin image path (`/Items/<id>/Images/Primary?...`)
    pub image: Option<String>,
    /// the image is a channel logo, not a poster
    #[serde(default)]
    pub logo: bool,
    /// what A opens: a Jellyfin item, or a live TV channel
    pub item_id: Option<String>,
    pub channel_id: Option<String>,
    /// Jellyfin users it is meant for (who requested the film), if anyone
    #[serde(default)]
    pub users: Vec<String>,
    /// one of the automatic TG alerts (a profile can turn them off, profile.rs)
    #[serde(default)]
    pub tg: bool,
}

/// "Ricordamelo" on a programme in the guide.
#[derive(Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub program_id: String,
    pub channel_id: String,
    /// "Rai 1"
    pub channel: String,
    pub name: String,
    pub episode: Option<String>,
    /// unix ms
    pub start: u64,
    /// channel logo (Jellyfin image path)
    pub image: Option<String>,
    /// Jellyfin user who set it
    #[serde(default)]
    pub user: String,
    /// set by itself from `TG`, not by someone
    #[serde(default)]
    pub tg: bool,
    /// (tg) the ten-minute toast has been shown
    #[serde(default)]
    pub warned: bool,
}

/// What a due reminder says.
enum Stage {
    /// starts in this many minutes
    Soon(u64),
    Started,
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    /// newest first
    items: Vec<Notification>,
    /// Jellyfin user -> `time` of the newest notification they have seen
    seen: HashMap<String, u64>,
    reminders: Vec<Reminder>,
    /// arr.rs: the last history record handled, per service
    #[serde(default)]
    cursors: HashMap<String, u64>,
}

#[derive(Serialize)]
pub struct Feed {
    items: Vec<Notification>,
    /// newer than what this profile has seen
    unread: usize,
    reminders: Vec<Reminder>,
}

/// What the UI gets with `notify` (a new one).
#[derive(Clone, Serialize)]
struct Event {
    n: Notification,
    /// the overlay is showing it; otherwise the UI shows its own toast (if `toast`)
    overlay: bool,
    toast: bool,
}

pub struct Notify {
    store: Mutex<Store>,
    app: OnceLock<AppHandle>,
}

fn path() -> std::path::PathBuf {
    data_dir().join("notifications.json")
}

impl Notify {
    pub fn new() -> Arc<Notify> {
        let store = fs::read_to_string(path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Arc::new(Notify { store: Mutex::new(store), app: OnceLock::new() })
    }

    fn save(&self, st: &Store) {
        let p = path();
        if let Some(d) = p.parent() {
            let _ = fs::create_dir_all(d);
        }
        let tmp = p.with_extension("json.tmp");
        if fs::write(&tmp, serde_json::to_string(st).unwrap()).is_ok() {
            let _ = fs::rename(&tmp, &p);
        }
    }

    // ---- arr.rs cursors ----

    pub fn cursor(&self, key: &str) -> Option<u64> {
        self.store.lock().unwrap().cursors.get(key).copied()
    }

    pub fn set_cursor(&self, key: &str, id: u64) {
        let mut st = self.store.lock().unwrap();
        st.cursors.insert(key.to_string(), id);
        self.save(&st);
    }

    // ---- the feed ----

    /// Add a notification; `toast`: also pop it up on screen.
    pub async fn push(&self, mut n: Notification, toast: bool) {
        {
            let mut st = self.store.lock().unwrap();
            let t = now_ms();
            // unique and increasing, also for several in the same millisecond
            n.id = st.items.first().map_or(t, |f| f.id.max(t - 1) + 1);
            n.time = t;
            st.items.insert(0, n.clone());
            st.items.truncate(KEEP);
            self.save(&st);
        }
        eprintln!("[notify] {} | {} | {}", n.kicker, n.title, n.body);
        let Some(app) = self.app.get() else { return };
        let mut overlay = false;
        if toast {
            let image = match &n.image {
                Some(p) => app.state::<Arc<Jf>>().image(p).await.ok().map(|(b, _)| b),
                None => None,
            };
            overlay = overlay::show(
                app,
                overlay::Toast {
                    kicker: n.kicker.clone(),
                    title: n.title.clone(),
                    body: n.body.clone(),
                    image,
                    logo: n.logo,
                },
            );
        }
        let _ = app.emit("notify", Event { n, overlay, toast });
    }

    pub fn feed(&self, user: &str) -> Feed {
        let tg = crate::profile::tg_alerts(user);
        let st = self.store.lock().unwrap();
        let seen = st.seen.get(user).copied().unwrap_or(0);
        let items: Vec<Notification> = st.items.iter().filter(|n| tg || !n.tg).cloned().collect();
        Feed {
            unread: items.iter().filter(|n| n.time > seen).count(),
            items,
            reminders: st.reminders.iter().filter(|r| tg || !r.tg).cloned().collect(),
        }
    }

    /// The profile has looked at the list.
    pub fn seen(&self, user: &str) {
        let mut st = self.store.lock().unwrap();
        let newest = st.items.first().map_or(0, |n| n.time);
        if st.seen.get(user) != Some(&newest) {
            st.seen.insert(user.to_string(), newest);
            self.save(&st);
        }
    }

    pub fn clear(&self) {
        let mut st = self.store.lock().unwrap();
        st.items.clear();
        self.save(&st);
    }

    // ---- live TV reminders ----

    /// Set or remove the reminder for `r.program_id`; returns whether it is set now.
    pub fn toggle_reminder(&self, r: Reminder) -> bool {
        let mut st = self.store.lock().unwrap();
        let before = st.reminders.len();
        st.reminders.retain(|x| x.program_id != r.program_id);
        let set = st.reminders.len() == before;
        if set {
            st.reminders.push(r);
            st.reminders.sort_by_key(|x| x.start);
        }
        self.save(&st);
        set
    }

    /// Reminders that are due, late ones dropped. A reminder goes once it has
    /// fired; a TG's stays after its ten-minute toast, until it starts.
    fn due(&self) -> Vec<(Reminder, Stage)> {
        let t = now_ms();
        let mut st = self.store.lock().unwrap();
        let mut out = Vec::new();
        let mut changed = false;
        st.reminders.retain_mut(|r| {
            let before = if r.tg { TG_BEFORE } else { REMIND_BEFORE };
            if t + before < r.start {
                return true;
            }
            let soon = Stage::Soon((r.start.saturating_sub(t) + 59_999) / 60_000);
            if r.tg && t < r.start {
                if !r.warned {
                    r.warned = true;
                    changed = true;
                    out.push((r.clone(), soon));
                }
                return true;
            }
            changed = true;
            if t < r.start + REMIND_LATE {
                out.push((r.clone(), if t >= r.start { Stage::Started } else { soon }));
            }
            false
        });
        if changed {
            self.save(&st);
        }
        out
    }

    /// The TGs of the next 26 hours from the guide become reminders (the ones
    /// the guide no longer lists go). Returns how many are set.
    async fn tg_refresh(&self, jf: &Jf) -> Result<usize, String> {
        let chans = jf.request_shared("/LiveTv/Channels", &[("Limit".into(), "500".into())]).await?;
        // channel id -> (name, rule, logo)
        let mut ids: HashMap<String, (String, usize, Option<String>)> = HashMap::new();
        for c in chans["Items"].as_array().into_iter().flatten() {
            let (Some(id), Some(name)) = (c["Id"].as_str(), c["Name"].as_str()) else { continue };
            if let Some(rule) = TG.iter().position(|r| r.0.eq_ignore_ascii_case(name.trim())) {
                let logo = c["ImageTags"]["Primary"]
                    .as_str()
                    .map(|tag| format!("/Items/{id}/Images/Primary?maxWidth=200&quality=85&v={tag}"));
                ids.insert(id.to_string(), (name.trim().to_string(), rule, logo));
            }
        }
        if ids.is_empty() {
            return Ok(0);
        }
        let t = now_ms();
        let q = [
            ("ChannelIds", ids.keys().cloned().collect::<Vec<_>>().join(",")),
            ("MinStartDate", iso_utc(t)),
            ("MaxStartDate", iso_utc(t + 26 * 3_600_000)),
            ("SortBy", "StartDate".into()),
            ("Limit", "500".into()),
        ]
        .map(|(k, v)| (k.to_string(), v));
        let progs = jf.request_shared("/LiveTv/Programs", &q).await?;
        let mut found = Vec::new();
        for p in progs["Items"].as_array().into_iter().flatten() {
            let Some((channel, rule, logo)) = p["ChannelId"].as_str().and_then(|c| ids.get(c)) else { continue };
            let (_, name, times) = TG[*rule];
            if !p["Name"].as_str().is_some_and(|n| n.trim().eq_ignore_ascii_case(name)) {
                continue;
            }
            let (Some(id), Some(start)) = (p["Id"].as_str(), p["StartDate"].as_str().and_then(parse_utc)) else { continue };
            let m = local_minutes(start);
            if !times.iter().any(|&x| m.abs_diff(x) <= TG_SLACK) {
                continue;
            }
            found.push(Reminder {
                program_id: id.to_string(),
                channel_id: p["ChannelId"].as_str().unwrap_or_default().to_string(),
                channel: channel.clone(),
                name: name.to_string(),
                episode: None,
                start,
                image: logo.clone(),
                user: String::new(),
                tg: true,
                warned: false,
            });
        }
        let mut st = self.store.lock().unwrap();
        let before = st.reminders.len();
        st.reminders.retain(|r| !r.tg || r.start <= t || found.iter().any(|f| f.program_id == r.program_id));
        let mut changed = st.reminders.len() != before;
        for f in found {
            if !st.reminders.iter().any(|r| r.program_id == f.program_id) {
                st.reminders.push(f);
                changed = true;
            }
        }
        if changed {
            st.reminders.sort_by_key(|x| x.start);
            self.save(&st);
        }
        Ok(st.reminders.iter().filter(|r| r.tg).count())
    }

    async fn fire(&self, app: &AppHandle, r: Reminder, stage: Stage) {
        // the profile on the TV (or the last one, on the picker) doesn't want the TGs:
        // only into the list, for the others
        let quiet = r.tg
            && app.state::<Arc<Jf>>().session().is_some_and(|s| !crate::profile::tg_alerts(&s.user_id));
        let at = chrono_hhmm(r.start);
        let mut body = match &r.episode {
            Some(e) => format!("Alle {at} · {e}"),
            None => format!("Alle {at}"),
        };
        let (kind, kicker) = match stage {
            Stage::Soon(1) => ("reminder", format!("Tra un minuto · {}", r.channel)),
            Stage::Soon(m) => ("reminder", format!("Tra {m} minuti · {}", r.channel)),
            Stage::Started if !r.tg => ("reminder", format!("Inizia ora · {}", r.channel)),
            Stage::Started => {
                let pl = app.state::<Arc<Player>>();
                if pl.channel().as_deref() == Some(r.channel_id.as_str()) {
                    return; // already watching it
                }
                if quiet {
                    // nothing offered
                } else if pl.active() {
                    let until = (now_ms() + TG_OFFER) / 1000;
                    pl.offer_channel(&r.channel_id, &format!("Guarda il {}", r.name), &format!("Inizia ora su {}", r.channel), until)
                        .await;
                } else if !pad::paused() {
                    body.push_str(" · START per guardarlo");
                }
                ("onair", format!("Inizia ora · {}", r.channel))
            }
        };
        self.push(
            Notification {
                id: 0,
                time: 0,
                kind: kind.into(),
                kicker,
                title: r.name.clone(),
                body,
                image: r.image.clone(),
                logo: true,
                item_id: None,
                channel_id: Some(r.channel_id.clone()),
                users: if r.user.is_empty() { vec![] } else { vec![r.user.clone()] },
                tg: r.tg,
            },
            !quiet,
        )
        .await;
    }

    /// Start the reminder clock (call once, from setup).
    pub fn start(self: &Arc<Self>, app: AppHandle) {
        let _ = self.app.set(app);
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                for (r, stage) in me.due() {
                    if let Some(app) = me.app.get() {
                        me.fire(app, r, stage).await;
                    }
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });
        // the TGs, from the guide (once a profile is signed in, then every half hour)
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            let mut said = String::new();
            loop {
                let jf = me.app.get().map(|a| a.state::<Arc<Jf>>().inner().clone());
                let r = match jf {
                    Some(jf) => me.tg_refresh(&jf).await,
                    None => Err("no app".into()),
                };
                let line = match r {
                    Ok(n) => format!("{n} TG in the next 26 h"),
                    Err(e) => format!("guide unavailable: {e}"),
                };
                if line != said {
                    eprintln!("[notify] {line}");
                    said = line;
                }
                tokio::time::sleep(TG_REFRESH).await;
            }
        });
        // Dev: AZOTOTV_NOTIFY_TEST=<item id or empty> pops a sample notification after 6 s.
        if let Ok(item) = std::env::var("AZOTOTV_NOTIFY_TEST") {
            let me = self.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(6)).await;
                let item = (!item.is_empty()).then_some(item);
                me.push(
                    Notification {
                        id: 0,
                        time: 0,
                        kind: "test".into(),
                        kicker: "Pronto da guardare".into(),
                        title: "Notifica di prova".into(),
                        body: "Film · richiesto da Ada".into(),
                        image: item.as_ref().map(|i| format!("/Items/{i}/Images/Primary?maxHeight=300")),
                        logo: false,
                        item_id: item,
                        channel_id: None,
                        users: vec![],
                        tg: false,
                    },
                    true,
                )
                .await;
            });
        }
    }
}

fn local_tm(ms: u64) -> libc::tm {
    let t = (ms / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm
}

/// Local "HH:MM" of a unix-ms time (the box runs in Italian local time).
fn chrono_hhmm(ms: u64) -> String {
    let tm = local_tm(ms);
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

/// Local minutes after midnight of a unix-ms time.
fn local_minutes(ms: u64) -> u32 {
    let tm = local_tm(ms);
    (tm.tm_hour * 60 + tm.tm_min) as u32
}

/// Days since 1970-01-01 of a proleptic Gregorian date, and back (H. Hinnant's algorithms).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Jellyfin's UTC dates ("2026-10-20T18:00:00.0000000Z") to unix ms.
fn parse_utc(s: &str) -> Option<u64> {
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let days = days_from_civil(n(0..4)?, n(5..7)?, n(8..10)?);
    let secs = days * 86_400 + n(11..13)? * 3600 + n(14..16)? * 60 + n(17..19)?;
    u64::try_from(secs * 1000).ok()
}

/// unix ms to "2026-10-20T18:00:00Z".
fn iso_utc(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let r = secs.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", r / 3600, r / 60 % 60, r % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_dates() {
        let t = parse_utc("2026-10-20T18:00:00.0000000Z").unwrap();
        assert_eq!(t, 1_792_519_200_000);
        assert_eq!(iso_utc(t), "2026-10-20T18:00:00Z");
        assert_eq!(iso_utc(parse_utc("2028-02-29T23:59:59Z").unwrap()), "2028-02-29T23:59:59Z");
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_utc("garbage"), None);
    }
}
