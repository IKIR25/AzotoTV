//! TV remote input. Anything that looks like a remote (the rc device of an
//! HDMI-CEC adapter or an IR receiver, a FLIRC) is grabbed, so KWin doesn't
//! also turn its keys into keyboard input, and its keys become the same actions
//! as the controller's (see pad.rs): `remote` events for the UI, or
//! `script-message azoto-input <key> remote` to mpv while the video player is
//! open. While a game runs the remote is ignored (games are played with pads).
//! Remotes can be plugged in at any time: /dev/input is watched with inotify.
//!
//! The devices are root:input, so they need a udev rule giving the seat user
//! access (session/70-azototv-remote.rules).

use std::{
    collections::HashMap,
    ffi::CString,
    io,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use evdev::{EventType, KeyCode as Key, raw_stream::RawDevice};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::player::Player;

const REPEAT_DELAY: Duration = Duration::from_millis(350);
const REPEAT_RATE: Duration = Duration::from_millis(110);
const HOLD: Duration = Duration::from_millis(550);

fn key_action(k: Key) -> Option<&'static str> {
    Some(match k {
        Key::KEY_UP => "up",
        Key::KEY_DOWN => "down",
        Key::KEY_LEFT => "left",
        Key::KEY_RIGHT => "right",
        Key::KEY_OK | Key::KEY_SELECT | Key::KEY_ENTER | Key::KEY_KPENTER => "a",
        Key::KEY_BACK | Key::KEY_EXIT | Key::KEY_ESC | Key::KEY_BACKSPACE => "b",
        Key::KEY_MENU | Key::KEY_CONTEXT_MENU | Key::KEY_OPTION | Key::KEY_SETUP | Key::KEY_ROOT_MENU | Key::KEY_INFO => {
            "start"
        }
        Key::KEY_HOMEPAGE => "home",
        Key::KEY_PLAYPAUSE | Key::KEY_PLAY | Key::KEY_PAUSE | Key::KEY_PLAYCD | Key::KEY_PAUSECD => "play",
        Key::KEY_STOP | Key::KEY_STOPCD => "stop",
        Key::KEY_FASTFORWARD | Key::KEY_FORWARD => "ff",
        Key::KEY_REWIND => "rew",
        Key::KEY_NEXT | Key::KEY_NEXTSONG => "next",
        Key::KEY_PREVIOUS | Key::KEY_PREVIOUSSONG => "prev",
        Key::KEY_CHANNELUP => "chup",
        Key::KEY_CHANNELDOWN => "chdown",
        _ => return None,
    })
}

fn repeats(action: &str) -> bool {
    matches!(action, "up" | "down" | "left" | "right" | "ff" | "rew")
}

/// What mpv's azoto-osd.lua gets for an action (it knows the remote-only ones
/// it can use; seeking keys become its ±60 s shoulders, a held OK is just OK).
fn osd_key(action: &str) -> &str {
    match action {
        "ff" | "next" => "rb",
        "rew" | "prev" => "lb",
        "home" => "stop",
        "a-hold" => "a",
        a => a,
    }
}

/// A remote: the input device of an rc (CEC / IR) device, a FLIRC, or anything
/// with arrows and OK that is neither a keyboard nor a pad.
fn is_remote(path: &Path, dev: &RawDevice) -> bool {
    let Some(keys) = dev.supported_keys() else { return false };
    let arrows = [Key::KEY_UP, Key::KEY_DOWN, Key::KEY_LEFT, Key::KEY_RIGHT].iter().all(|&k| keys.contains(k));
    if !arrows || keys.contains(Key::BTN_SOUTH) {
        return false;
    }
    let name = dev.name().unwrap_or("").to_lowercase();
    if name.starts_with("azototv ") {
        return false; // azotopad's virtual keyboard / mouse
    }
    // /sys/class/input/eventN/device -> .../rc/rcM/inputK for rc-core devices
    let sys = path.file_name().map(|n| Path::new("/sys/class/input").join(n).join("device"));
    let rc = sys.and_then(|p| std::fs::canonicalize(p).ok()).is_some_and(|p| p.to_string_lossy().contains("/rc/rc"));
    let ok = keys.contains(Key::KEY_OK) || keys.contains(Key::KEY_SELECT);
    let keyboard = [Key::KEY_Q, Key::KEY_W, Key::KEY_E, Key::KEY_R, Key::KEY_T, Key::KEY_Y].iter().all(|&k| keys.contains(k));
    rc || name.contains("flirc") || (ok && !keyboard)
}

struct Remote {
    path: PathBuf,
    dev: RawDevice,
}

fn open_remote(path: &Path) -> Option<Remote> {
    let mut dev = RawDevice::open(path).ok()?;
    if !is_remote(path, &dev) {
        return None;
    }
    if let Err(e) = dev.grab() {
        eprintln!("[remote] cannot grab {}: {e}", path.display());
        return None;
    }
    let fd = dev.as_raw_fd();
    unsafe {
        libc::fcntl(fd, libc::F_SETFL, libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK);
    }
    eprintln!("[remote] found \"{}\" ({})", dev.name().unwrap_or("?"), path.display());
    Some(Remote { path: path.to_path_buf(), dev })
}

fn event_nodes() -> Vec<PathBuf> {
    std::fs::read_dir("/dev/input")
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event")))
                .collect()
        })
        .unwrap_or_default()
}

/// inotify on /dev/input: new nodes, and permission changes (udev applies the
/// uaccess ACL a moment after the node appears).
fn watch_dev_input() -> Option<i32> {
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    let dir = CString::new("/dev/input").unwrap();
    if unsafe { libc::inotify_add_watch(fd, dir.as_ptr(), libc::IN_CREATE | libc::IN_ATTRIB) } < 0 {
        unsafe { libc::close(fd) };
        return None;
    }
    Some(fd)
}

fn drain(fd: i32) {
    let mut buf = [0u8; 4096];
    while unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
}

pub fn spawn(app: AppHandle) {
    let debug = std::env::var_os("AZOTOTV_DEBUG").is_some();
    let _ = std::thread::Builder::new().name("remote".into()).spawn(move || {
        listen(debug, |action| {
            if debug {
                eprintln!("[remote] -> {action}");
            }
            fire(&app, action);
        })
    });
}

/// A remote action (also from a phone, see phone.rs) to whatever has the
/// screen: mpv's overlay while a video plays, else the UI (`remote` events).
/// Ignored while a game runs.
pub fn fire(app: &AppHandle, action: &str) {
    if crate::pad::paused() {
        return;
    }
    let pl = app.state::<Arc<Player>>().inner().clone();
    if pl.active() {
        let key = osd_key(action).to_string();
        tauri::async_runtime::spawn(async move {
            pl.command(vec![json!("script-message"), json!("azoto-input"), json!(key), json!("remote")]).await;
        });
    } else {
        let _ = app.emit("remote", action);
    }
}

/// The actions `fire` takes (a phone may only send these).
pub fn known(action: &str) -> bool {
    matches!(
        action,
        "up" | "down" | "left" | "right" | "a" | "a-hold" | "b" | "start" | "home" | "play" | "stop" | "ff" | "rew" | "next"
            | "prev" | "chup" | "chdown"
    )
}

/// Reads every remote, forever, handing each action to `fire`.
fn listen(debug: bool, fire: impl Fn(&'static str)) {
    let watch = watch_dev_input();
    let mut remotes: Vec<Remote> = Vec::new();
    // when to look through /dev/input again (at start, then after inotify events)
    let mut rescan = Some(Instant::now());

    // action -> next repeat time
    let mut held: HashMap<&'static str, Instant> = HashMap::new();
    // when OK went down; None once released or once the hold has fired
    let mut a_down: Option<Instant> = None;

    loop {
        if rescan.is_some_and(|t| Instant::now() >= t) {
            rescan = None;
            for path in event_nodes() {
                if !remotes.iter().any(|r| r.path == path) {
                    remotes.extend(open_remote(&path));
                }
            }
        }

        let mut fds: Vec<libc::pollfd> =
            remotes.iter().map(|r| libc::pollfd { fd: r.dev.as_raw_fd(), events: libc::POLLIN, revents: 0 }).collect();
        if let Some(w) = watch {
            fds.push(libc::pollfd { fd: w, events: libc::POLLIN, revents: 0 });
        }
        // sleep until an event unless something fires on its own (a held
        // key repeating, OK becoming a hold, a pending rescan)
        let now = Instant::now();
        let deadline = held
            .values()
            .copied()
            .chain(a_down.map(|t| t + HOLD))
            .chain(rescan)
            .min();
        let timeout = match (deadline, watch) {
            (Some(d), _) => d.saturating_duration_since(now).as_millis() as i32,
            // without inotify, look for new remotes every few seconds
            (None, None) => 3000,
            (None, Some(_)) => -1,
        };
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
        if n < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        if watch.is_none() && n == 0 && deadline.is_none() {
            rescan = Some(Instant::now());
        }
        if let (Some(w), Some(p)) = (watch, fds.last()) {
            if p.revents & libc::POLLIN != 0 {
                drain(w);
                // let udev finish (ACL, symlinks) before opening
                rescan = Some(Instant::now() + Duration::from_millis(500));
            }
        }

        let mut gone = Vec::new();
        for (i, r) in remotes.iter_mut().enumerate() {
            let rev = fds[i].revents;
            if rev & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                gone.push(i);
                continue;
            }
            if rev & libc::POLLIN == 0 {
                continue;
            }
            let evs = match r.dev.fetch_events() {
                Ok(evs) => evs.collect::<Vec<_>>(),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(_) => {
                    gone.push(i);
                    continue;
                }
            };
            for ev in evs {
                if ev.event_type() != EventType::KEY {
                    continue;
                }
                let k = Key(ev.code());
                if debug {
                    eprintln!("[remote] key {k:?} {}", ev.value());
                }
                let Some(action) = key_action(k) else { continue };
                match ev.value() {
                    // press (the kernel's own autorepeat, value 2, is ignored:
                    // repeats are timed here, like the controller's)
                    1 => {
                        if action == "a" {
                            a_down = Some(Instant::now());
                        } else {
                            fire(action);
                            if repeats(action) {
                                held.insert(action, Instant::now() + REPEAT_DELAY);
                            }
                        }
                    }
                    0 => {
                        if action == "a" {
                            if a_down.take().is_some() {
                                fire("a");
                            }
                        } else {
                            held.remove(action);
                        }
                    }
                    _ => {}
                }
            }
        }
        for i in gone.into_iter().rev() {
            let r = remotes.remove(i);
            eprintln!("[remote] lost {}", r.path.display());
            held.clear();
            a_down = None;
        }

        let now = Instant::now();
        if a_down.is_some_and(|t| now - t >= HOLD) {
            a_down = None;
            fire("a-hold");
        }
        for (action, next) in held.iter_mut() {
            if now >= *next {
                fire(*action);
                *next = now + REPEAT_RATE;
            }
        }
    }
}
