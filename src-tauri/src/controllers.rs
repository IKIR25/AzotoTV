//! The controllers connected to the TV, as players 1-4 in the order they were
//! connected (the order azotopad gives them in games: see its `Session`), for
//! START › Controller and the screen before a multiplayer game.
//!
//! A pad is any evdev device with gamepad buttons and a stick that AzotoTV can
//! open, except azotopad's own virtual Xbox pads (they exist only while a game
//! runs). Phones that joined as controllers are pads too (`ExternalPad`, see
//! phone.rs). Joy-Cons go through joycond: until someone presses L+R (the
//! two as one pad, "Nintendo Switch Combined Joy-Cons") or SL+SR (one alone,
//! sideways) their device stays root's, so they are only counted as waiting.
//!
//! While the screen is open (`watch(true)`): Bluetooth is scanning and every
//! controller found is paired, trusted (it reconnects by itself from then on
//! with any button) and connected; `controller-input` (the pad's node) is
//! emitted when a pad is used, so the screen can light its card up.

use std::{
    collections::HashMap,
    io::Write,
    os::fd::AsRawFd,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use evdev::{EventType, raw_stream::RawDevice};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Players that get a number (azotopad makes up to 4 virtual pads).
const PLAYERS: usize = 4;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Pad {
    /// /dev/input/event* (the key for `controller-input`)
    pub node: String,
    pub name: String,
    /// joycons (a pair), joycon-l, joycon-r, procon, phone, xbox, playstation, pad
    pub kind: &'static str,
    /// from 1; None past the 4th
    pub player: Option<usize>,
    /// percent (Joy-Cons only tell a level: 5, 25, 50, 75, 100)
    pub battery: Option<u8>,
    pub charging: bool,
    pub wireless: bool,
    /// a Joy-Con pair with its Joy-Cons off: joycond keeps the pad, but it
    /// comes after the others (in a game it plays once it's used again)
    pub asleep: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Overview {
    pub pads: Vec<Pad>,
    /// Joy-Cons connected but not yet pressed L+R / SL+SR
    pub waiting: usize,
    pub bluetooth: Bluetooth,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Bluetooth {
    /// on, off (and couldn't be switched on), none (no adapter), unknown
    pub state: String,
    /// while the screen is open
    pub scanning: bool,
    /// what pairing is doing right now, newest last ("Joy-Con (L): associato")
    pub news: Vec<String>,
}

// ---- the list ----------------------------------------------------------------

fn sys_input(node: &Path) -> Option<PathBuf> {
    let name = node.file_name()?;
    std::fs::canonicalize(Path::new("/sys/class/input").join(name)).ok()
}

/// azotopad's virtual pad for a game (not a controller someone holds)
fn is_game_pad(node: &Path, dev: &RawDevice) -> bool {
    let id = dev.input_id();
    (id.vendor(), id.product()) == (0x045e, 0x028e)
        && sys_input(node).is_some_and(|p| p.to_string_lossy().contains("/devices/virtual/"))
}

fn describe(dev: &RawDevice) -> (&'static str, String) {
    let id = dev.input_id();
    let name = dev.name().unwrap_or("Controller").trim().to_string();
    match (id.vendor(), id.product()) {
        (0x057e, 0x2008) => ("joycons", "Joy-Con".into()),
        (0x057e, 0x2006) => ("joycon-l", t!("Left Joy-Con")),
        (0x057e, 0x2007) => ("joycon-r", t!("Right Joy-Con")),
        (0x057e, 0x2009) => ("procon", "Pro Controller".into()),
        id if id == azotopad::EXTERNAL_PAD_ID => ("phone", name),
        (0x045e, _) => ("xbox", name),
        (0x054c, _) => ("playstation", name),
        _ => ("pad", name),
    }
}

/// `power_supply` of the HID device behind an input node (Bluetooth pads, hid-nintendo).
fn battery_of(sys: &Path) -> Option<(u8, bool)> {
    for dir in sys.ancestors().skip(1).take(4) {
        let Ok(entries) = std::fs::read_dir(dir.join("power_supply")) else { continue };
        if let Some(b) = entries.flatten().find_map(|e| read_battery(&e.path())) {
            return Some(b);
        }
    }
    None
}

/// (percent, charging) of a /sys/class/power_supply/<x> battery.
fn read_battery(ps: &Path) -> Option<(u8, bool)> {
    let read = |f: &str| std::fs::read_to_string(ps.join(f)).map(|s| s.trim().to_string()).ok();
    let pct = read("capacity").and_then(|c| c.parse::<u8>().ok()).or_else(|| {
        Some(match read("capacity_level")?.as_str() {
            "Critical" => 5,
            "Low" => 25,
            "Normal" => 50,
            "High" => 75,
            "Full" => 100,
            _ => return None,
        })
    })?;
    Some((pct.min(100), read("status").as_deref() == Some("Charging")))
}

use azotopad::held_joycons;

fn pads() -> Vec<(PathBuf, RawDevice)> {
    let mut found: Vec<(PathBuf, RawDevice)> = evdev::raw_stream::enumerate()
        .filter(|(node, dev)| azotopad::is_pad(dev) && !is_game_pad(node, dev))
        .collect();
    // the order they were connected in, like azotopad
    found.sort_by_key(|(node, _)| std::fs::metadata(node).map(|m| (m.ctime(), m.ctime_nsec())).unwrap_or((i64::MAX, 0)));
    found
}

pub fn list() -> (Vec<Pad>, usize) {
    let mut out: Vec<Pad> = pads()
        .into_iter()
        .enumerate()
        .map(|(i, (node, dev))| {
            let (kind, name) = describe(&dev);
            let sys = sys_input(&node);
            let (battery, charging) = sys.as_deref().and_then(battery_of).map_or((None, false), |(b, c)| (Some(b), c));
            let bt = dev.input_id().bus_type() == evdev::BusType::BUS_BLUETOOTH;
            Pad {
                node: node.to_string_lossy().into_owned(),
                name,
                kind,
                player: (i < PLAYERS).then_some(i + 1),
                battery,
                charging,
                wireless: bt || kind == "joycons" || kind == "phone",
                asleep: false,
            }
        })
        .collect();
    // a pair's battery is the lower of its two Joy-Cons; which Joy-Cons make
    // which pair only joycond knows, so this works with one pair
    let held = held_joycons();
    let pairs = out.iter().filter(|p| p.kind == "joycons").count();
    let waiting = held.len().saturating_sub(pairs * 2);
    if pairs == 1 {
        let levels: Vec<(u8, bool)> = held.iter().filter_map(|s| battery_of(s)).collect();
        let pair = out.iter_mut().find(|p| p.kind == "joycons").unwrap();
        if held.is_empty() {
            pair.asleep = true;
        } else if let Some(&(low, _)) = levels.iter().min_by_key(|(b, _)| *b) {
            pair.battery = Some(low);
            pair.charging = levels.iter().all(|(_, c)| *c);
        }
    }
    // in a game an asleep pair takes the first free player once it's used
    // again (azotopad), so it comes after the others
    out.sort_by_key(|p| p.asleep);
    for (i, p) in out.iter_mut().enumerate() {
        p.player = (i < PLAYERS).then_some(i + 1);
    }
    (out, waiting)
}

// ---- Low battery -------------------------------------------------------------

/// How often the controllers' batteries are read (a few sysfs files).
const BATTERY_EVERY: Duration = Duration::from_secs(60);

/// How low a controller battery is: 0 fine, 1 low (≤ 25%), 2 critical (≤ 10%).
fn battery_alarm(pct: u8) -> u8 {
    match pct {
        0..=10 => 2,
        11..=25 => 1,
        _ => 0,
    }
}

/// Controllers' batteries: (power_supply name, controller name, percent, charging).
/// Peripheral batteries (scope Device) of input devices: Joy-Cons one by one,
/// Pro Controller, PlayStation and Xbox pads...
fn batteries() -> Vec<(String, String, u8, bool)> {
    let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let read = |f: &str| std::fs::read_to_string(e.path().join(f)).map(|s| s.trim().to_string()).ok();
            if read("scope").as_deref() != Some("Device") {
                return None;
            }
            let (pct, charging) = read_battery(&e.path())?;
            let name = std::fs::read_dir(e.path().join("device/input"))
                .ok()?
                .flatten()
                .filter_map(|i| std::fs::read_to_string(i.path().join("name")).ok())
                .map(|n| n.trim().to_string())
                .find(|n| !n.contains("IMU") && !n.contains("Motion"))?;
            Some((e.file_name().to_string_lossy().into_owned(), name, pct, charging))
        })
        .collect()
}

/// Warns once when a controller's battery gets low and once more when it's
/// nearly flat (a toast over everything, games too); again only after it
/// was charged (seen charging, or back at 50% or more: Joy-Cons charge on
/// the Switch, away from the TV).
pub fn watch_batteries(app: AppHandle) {
    std::thread::spawn(move || {
        let mut warned: HashMap<String, u8> = HashMap::new();
        loop {
            for (id, name, pct, charging) in batteries() {
                let alarm = if charging { 0 } else { battery_alarm(pct) };
                let before = warned.get(&id).copied().unwrap_or(0);
                if alarm > before {
                    let body = if alarm == 2 { &t!("It will turn off soon: charge it") } else { "Mettilo in carica appena puoi" };
                    eprintln!("[controllers] {name}: battery {pct}%");
                    let shown = crate::overlay::show(
                        &app,
                        crate::overlay::Toast {
                            kicker: "Batteria scarica".into(),
                            title: name.clone(),
                            body: body.into(),
                            image: None,
                            logo: false,
                        },
                    );
                    if !shown {
                        let _ = app.emit("controller-battery", t!("{name}: battery low. {body}", name = name, body = body));
                    }
                }
                let charged = charging || pct >= 50;
                warned.insert(id, if charged { 0 } else { alarm.max(before) });
            }
            std::thread::sleep(BATTERY_EVERY);
        }
    });
}

/// Lights each controller's player number (see `azotopad::lights`), now and
/// again a little later: joycond lights a pair's Joy-Cons its own way right
/// after they connect. Outside games only (azotopad numbers them in games).
pub fn show_lights_soon() {
    std::thread::spawn(|| {
        for wait in [300, 2000] {
            std::thread::sleep(Duration::from_millis(wait));
            if crate::pad::paused() {
                return;
            }
            for p in list().0 {
                azotopad::lights::show(Path::new(&p.node), p.player.filter(|_| !p.asleep));
            }
        }
    });
}

/// The player (from 1) a pad's node has now.
pub fn player_of(node: &Path) -> Option<usize> {
    let node = node.to_string_lossy();
    list().0.into_iter().find(|p| p.node == node).and_then(|p| p.player)
}

// ---- Bluetooth pairing ---------------------------------------------------------

/// `bluetoothctl <args>`'s output; empty after 3 s (it waits forever for a
/// bluetoothd that isn't running).
fn ctl(args: &[&str]) -> String {
    let Ok(mut child) =
        Command::new("bluetoothctl").args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
    else {
        return String::new();
    };
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return String::new();
            }
        }
    }
    let mut out = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = std::io::Read::read_to_string(&mut o, &mut out);
    }
    out
}

/// MAC -> name, from `bluetoothctl devices [filter]`
fn devices(filter: Option<&str>) -> HashMap<String, String> {
    let out = ctl(&["devices"].into_iter().chain(filter).collect::<Vec<_>>());
    out.lines()
        .filter_map(|l| {
            let mut p = l.strip_prefix("Device ")?.splitn(2, ' ');
            Some((p.next()?.to_string(), p.next().unwrap_or("").to_string()))
        })
        .collect()
}

/// A controller, from its Bluetooth info: the gaming icon (Joy-Cons, Pro,
/// Xbox, DualSense...) or a known name.
fn is_controller(info: &str, name: &str) -> bool {
    info.lines().any(|l| l.trim() == "Icon: input-gaming")
        || ["Joy-Con", "Pro Controller", "Xbox Wireless", "Wireless Controller", "DualSense", "8BitDo"]
            .iter()
            .any(|k| name.contains(k))
}

fn adapter_state() -> String {
    let show = ctl(&["show"]);
    if show.trim().is_empty() || show.contains("No default controller") {
        return "none".into();
    }
    if show.lines().any(|l| l.trim() == "Powered: yes") { "on".into() } else { "off".into() }
}

/// The interactive `bluetoothctl` that scans and pairs (discovery lasts as
/// long as the client that asked for it), with its own pairing agent.
struct Scanner {
    child: Child,
    stdin: ChildStdin,
}

impl Scanner {
    fn start() -> Option<Scanner> {
        // an agent that never asks (nobody would answer its yes/no)
        let mut child = Command::new("bluetoothctl")
            .args(["--agent", "NoInputNoOutput"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdin = child.stdin.take()?;
        let mut s = Scanner { child, stdin };
        s.send("default-agent");
        s.send("scan on");
        Some(s)
    }

    fn send(&mut self, cmd: &str) {
        let _ = writeln!(self.stdin, "{cmd}");
        let _ = self.stdin.flush();
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        self.send("scan off");
        self.send("quit");
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default)]
struct Tries {
    /// MACs looked at that are not controllers
    skip: Vec<String>,
    /// MAC -> when the last step was asked
    last: HashMap<String, Instant>,
    /// paired before the screen opened (None until the first pass)
    old: Option<Vec<String>>,
    /// paired again from scratch once already
    redone: Vec<String>,
}

/// One pass: every controller in sight gets one step closer to connected.
fn pair_step(scan: &mut Scanner, tries: &mut Tries, news: &Mutex<Vec<String>>) {
    let paired = devices(Some("Paired"));
    let connected = devices(Some("Connected"));
    let old = tries.old.get_or_insert_with(|| paired.keys().cloned().collect()).clone();
    let say = |m: String| {
        eprintln!("[controllers] {m}");
        let mut n = news.lock().unwrap();
        n.push(m);
        let len = n.len();
        if len > 4 {
            n.drain(..len - 4);
        }
    };
    for (mac, name) in devices(None) {
        if tries.skip.contains(&mac) || connected.contains_key(&mac) {
            continue;
        }
        if tries.last.get(&mac).is_some_and(|t| t.elapsed() < Duration::from_secs(6)) {
            continue; // the last step is still going
        }
        let info = ctl(&["info", &mac]);
        if !is_controller(&info, &name) {
            tries.skip.push(mac);
            continue;
        }
        // RSSI only shows for a device seen by this scan: in pairing mode
        let in_sight = info.lines().any(|l| l.trim_start().starts_with("RSSI:"));
        if paired.contains_key(&mac) {
            if in_sight && old.contains(&mac) && !tries.redone.contains(&mac) {
                // paired on an earlier day, but now in pairing mode (e.g. it
                // was on the Switch meanwhile and lost our key): pair from
                // scratch, once (RSSI stays on for everything this scan saw)
                say(t!("{name}: pairing again", name = name));
                scan.send(&format!("remove {mac}"));
                tries.redone.push(mac.clone());
            } else {
                scan.send(&format!("trust {mac}"));
                scan.send(&format!("connect {mac}"));
            }
        } else if in_sight {
            say(t!("{name}: pairing…", name = name));
            scan.send(&format!("pair {mac}"));
        } else {
            continue;
        }
        tries.last.insert(mac, Instant::now());
    }
    for (mac, name) in &connected {
        if tries.last.remove(mac).is_some() {
            scan.send(&format!("trust {mac}"));
            say(t!("{name}: connected", name = name));
        }
    }
}

// ---- the screen --------------------------------------------------------------

pub struct Controllers {
    watching: Mutex<Option<Arc<AtomicBool>>>,
    bt: Mutex<Bluetooth>,
    news: Arc<Mutex<Vec<String>>>,
}

impl Controllers {
    pub fn new() -> Arc<Controllers> {
        Arc::new(Controllers { watching: Mutex::new(None), bt: Mutex::new(Bluetooth::default()), news: Arc::default() })
    }

    pub fn overview(&self) -> Overview {
        let (pads, waiting) = list();
        let mut bt = self.bt.lock().unwrap().clone();
        bt.news = self.news.lock().unwrap().clone();
        Overview { pads, waiting, bluetooth: bt }
    }

    /// The screen opened (scan, pair, report presses) or closed.
    pub fn watch(self: &Arc<Self>, app: &AppHandle, on: bool) {
        let mut w = self.watching.lock().unwrap();
        if let Some(stop) = w.take() {
            stop.store(true, Ordering::Relaxed);
        }
        app.state::<Arc<crate::phone::Phone>>().set_state(serde_json::json!({ "controllers": on }));
        if !on {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        *w = Some(stop.clone());
        self.news.lock().unwrap().clear();
        let (me, stop2) = (self.clone(), stop.clone());
        let _ = std::thread::Builder::new().name("bt-pair".into()).spawn(move || me.pair_loop(&stop2));
        let (app, stop2) = (app.clone(), stop);
        let _ = std::thread::Builder::new().name("pad-light".into()).spawn(move || presses(&app, &stop2));
    }

    fn pair_loop(&self, stop: &AtomicBool) {
        self.bt.lock().unwrap().state = "unknown".into();
        let mut state = adapter_state();
        if state == "off" {
            // soft-blocked or just powered down
            let _ = Command::new("rfkill").args(["unblock", "bluetooth"]).status();
            std::thread::sleep(Duration::from_millis(500));
            ctl(&["power", "on"]);
            state = adapter_state();
        }
        let scanner = if state == "on" { Scanner::start() } else { None };
        *self.bt.lock().unwrap() = Bluetooth { state, scanning: scanner.is_some(), news: Vec::new() };
        let Some(mut scanner) = scanner else { return };
        let mut tries = Tries::default();
        while !stop.load(Ordering::Relaxed) {
            pair_step(&mut scanner, &mut tries, &self.news);
            for _ in 0..15 {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        drop(scanner);
        self.bt.lock().unwrap().scanning = false;
    }
}

/// Emits `controller-input` (the node) when a pad is pressed or its stick
/// pushed, at most every 300 ms per pad. Opens the pads alongside the menus
/// and games (no grab).
fn presses(app: &AppHandle, stop: &AtomicBool) {
    // node, device, axis ranges (code -> min, max)
    let mut open: Vec<(PathBuf, RawDevice, HashMap<u16, (i32, i32)>)> = Vec::new();
    let mut last_scan = Instant::now() - Duration::from_secs(10);
    let mut lit: HashMap<PathBuf, Instant> = HashMap::new();
    while !stop.load(Ordering::Relaxed) {
        if last_scan.elapsed() >= Duration::from_secs(1) {
            last_scan = Instant::now();
            let now: Vec<(PathBuf, RawDevice)> = pads();
            open.retain(|(n, ..)| now.iter().any(|(m, _)| m == n));
            for (node, dev) in now {
                if !open.iter().any(|(n, ..)| *n == node) {
                    unsafe {
                        let fd = dev.as_raw_fd();
                        libc::fcntl(fd, libc::F_SETFL, libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK);
                    }
                    let mut ranges = HashMap::new();
                    if let (Some(axes), Ok(info)) = (dev.supported_absolute_axes(), dev.get_abs_state()) {
                        for a in axes.iter() {
                            let i = &info[a.0 as usize];
                            ranges.insert(a.0, (i.minimum, i.maximum));
                        }
                    }
                    open.push((node, dev, ranges));
                }
            }
        }
        let mut fds: Vec<libc::pollfd> =
            open.iter().map(|(_, d, _)| libc::pollfd { fd: d.as_raw_fd(), events: libc::POLLIN, revents: 0 }).collect();
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 200) };
        for (i, (node, dev, ranges)) in open.iter_mut().enumerate() {
            if fds[i].revents & libc::POLLIN == 0 {
                continue;
            }
            let Ok(evs) = dev.fetch_events() else { continue };
            let used = evs.into_iter().any(|e| match e.event_type() {
                EventType::KEY => e.value() == 1,
                EventType::ABSOLUTE => pushed(e.code(), e.value(), ranges.get(&e.code()).copied()),
                _ => false,
            });
            if used && lit.get(node).is_none_or(|t| t.elapsed() >= Duration::from_millis(300)) {
                lit.insert(node.clone(), Instant::now());
                let _ = app.emit("controller-input", node.to_string_lossy());
            }
        }
    }
}

/// A D-pad hat pressed, a trigger pulled, a stick pushed well off centre.
fn pushed(code: u16, v: i32, range: Option<(i32, i32)>) -> bool {
    use evdev::AbsoluteAxisCode as Abs;
    if (Abs::ABS_HAT0X.0..=Abs::ABS_HAT3Y.0).contains(&code) {
        return v != 0;
    }
    let Some((min, max)) = range.filter(|(a, b)| b > a) else { return false };
    let n = (v - min) as f32 / (max - min) as f32; // 0..1
    if [Abs::ABS_Z.0, Abs::ABS_RZ.0, Abs::ABS_GAS.0, Abs::ABS_BRAKE.0].contains(&code) {
        n > 0.5 // triggers rest at their minimum
    } else {
        (n * 2.0 - 1.0).abs() > 0.6
    }
}

// ---- commands ------------------------------------------------------------------

#[tauri::command]
pub fn controllers_list(c: tauri::State<'_, Arc<Controllers>>) -> Overview {
    c.overview()
}

#[tauri::command]
pub fn controllers_watch(app: AppHandle, c: tauri::State<'_, Arc<Controllers>>, on: bool) {
    c.watch(&app, on);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs /dev/uinput: a phone's pad shows up as the next player.
    #[test]
    #[ignore]
    fn phone_pad_is_listed() {
        let before = list().0.len();
        let pad = azotopad::ExternalPad::new("Telefono di Prova", |_| {}).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let (pads, waiting) = list();
        eprintln!("{pads:#?}\nwaiting {waiting}");
        let me = pads.iter().find(|p| p.name == "Telefono di Prova").expect("listed");
        assert_eq!(me.kind, "phone");
        assert_eq!(me.player, Some(before + 1).filter(|&n| n <= PLAYERS));
        assert_eq!(player_of(pad.node().unwrap()), me.player);
    }

    #[test]
    fn pushes() {
        use evdev::AbsoluteAxisCode as Abs;
        assert!(pushed(Abs::ABS_HAT0X.0, -1, None));
        assert!(!pushed(Abs::ABS_HAT0Y.0, 0, None));
        assert!(!pushed(Abs::ABS_X.0, 1000, Some((-32768, 32767))));
        assert!(pushed(Abs::ABS_X.0, -30000, Some((-32768, 32767))));
        assert!(!pushed(Abs::ABS_X.0, 128, Some((0, 255))));
        assert!(!pushed(Abs::ABS_Z.0, 0, Some((0, 255))));
        assert!(pushed(Abs::ABS_RZ.0, 255, Some((0, 255))));
    }
}
