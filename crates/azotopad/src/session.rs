//! A running mapping: grabs every real pad (so the game never sees it),
//! reads it, and drives the virtual device(s) the profile asks for.
//! Dropping or stopping the session releases the pads and removes the
//! virtual devices.
//!
//! `xbox`: each real pad gets its own virtual pad, i.e. its own player, in the
//! order they were plugged in (up to `players`, at most 4; the ones after
//! share the last player's, so in a one-player game every pad is player 1). A player
//! keeps their virtual pad when their real one goes away, and the next pad
//! plugged in takes the free player. `keys`: all pads drive the one keyboard
//! and mouse.
//!
//! A phone is a pad too ([`ExternalPad`]): a virtual pad of its own that
//! takes a player like a real pad plugged in, but can't close the game.
//!
//! A Joy-Con pair whose Joy-Cons are off (joycond keeps its pad) takes no
//! player until it is used: see [`held_joycons`].
//!
//! [`Session::serve_motion`] also sends each player's controller motion to
//! the Switch/Wii emulators (see [`crate::motion`]).

use std::{
    collections::HashMap,
    io,
    os::fd::{AsRawFd, RawFd},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use evdev::{
    AbsInfo, AbsoluteAxisCode as Abs, AttributeSet, BusType, EventSummary, EventType, FFEffect, FFEffectCode,
    FFEffectData, FFEffectKind, InputEvent, InputId, KeyCode as Key, PropType, RelativeAxisCode as Rel, UInputCode,
    UinputAbsSetup,
    raw_stream::RawDevice,
    uinput::VirtualDevice,
};

use crate::{
    mapping::{Ax, BTN_DIGI, BTN_JOYSTICK, Btn, Mapping, Raw, State},
    motion::{Imu, Motion, Sensors, SharedSensors},
    profile::{Input, KeyMap, Out, Profile, StickOut},
};

const RESCAN: Duration = Duration::from_secs(2);
const QUIT_HOLD: Duration = Duration::from_millis(1000);
const DEADZONE: f32 = 0.15;
const STICK_ON: f32 = 0.5;
const STICK_OFF: f32 = 0.3;
const SCROLL_RATE: f32 = 15.0;
const MAX_PLAYERS: usize = 4;

/// vendor/product of a pad the session grabbed (for SDL_GAMECONTROLLER_IGNORE_DEVICES)
pub type PadId = (u16, u16);

pub struct Session {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pads: Arc<Mutex<Vec<(PadId, bool)>>>,
    taps: Sender<(Vec<Key>, bool)>,
    sensors: SharedSensors,
    motion: Option<Motion>,
}

/// Presses keys on the session's virtual keyboard (e.g. F4 for fullscreen at
/// startup). Every profile has that keyboard, also `xbox`.
#[derive(Clone)]
pub struct Tapper(Sender<(Vec<Key>, bool)>);

impl Tapper {
    /// Press and release `keys` in order.
    pub fn tap(&self, keys: Vec<Key>) {
        let _ = self.0.send((keys, false));
    }

    /// Press `keys` together (a shortcut like Alt+T), then release them.
    pub fn combo(&self, keys: Vec<Key>) {
        let _ = self.0.send((keys, true));
    }
}

impl Session {
    /// Starts mapping. `sensitivity` scales the stick-driven cursor (1.0 = the
    /// profile's speed; a per-player setting). `on_quit` runs (once per hold)
    /// when the guide button is held, or back+start on pads without one.
    /// `players`: how many play together (1 = every pad drives player 1).
    pub fn start(profile: &Profile, sensitivity: f32, players: usize, on_quit: impl Fn() + Send + 'static) -> io::Result<Session> {
        let mut out = Output::new(profile, sensitivity)?;
        // across a stream Home is the other machine's (it closes its own game)
        let guide_through = matches!(profile, Profile::Stream);
        let stop = Arc::new(AtomicBool::new(false));
        let pads = Arc::new(Mutex::new(Vec::new()));
        let own = out.nodes();
        let sensors = SharedSensors::default();
        let mut ctx = Ctx {
            pads: Vec::new(),
            ids: pads.clone(),
            last_scan: None,
            own,
            players: players.max(1),
            imus: Vec::new(),
            sensors: sensors.clone(),
            woke: false,
        };
        // every player's virtual pad exists before the game starts looking
        ctx.scan();
        ctx.assign(&mut out);
        let stop2 = stop.clone();
        let (taps, tap_rx): (Sender<(Vec<Key>, bool)>, Receiver<(Vec<Key>, bool)>) = channel();
        let thread = std::thread::Builder::new().name("azotopad".into()).spawn(move || {
            let debug = std::env::var_os("AZOTOPAD_DEBUG").is_some();
            let mut prev = vec![State::default(); out.slots()];
            let mut quit_since: Option<Instant> = None;
            let mut last = Instant::now();
            while !stop2.load(Ordering::Relaxed) {
                if ctx.last_scan.is_none_or(|t| t.elapsed() >= RESCAN) {
                    ctx.scan();
                    ctx.assign(&mut out);
                }
                let timeout = if out.moving(&prev[0]) { 8 } else { 250 };
                ctx.read(timeout, &out.ff_fds());
                if std::mem::take(&mut ctx.woke) {
                    ctx.assign(&mut out);
                }
                for (slot, effect) in out.rumble_requests() {
                    ctx.rumble(slot, effect);
                }
                let mut s = vec![State::default(); out.slots()];
                let mut quit = false;
                for p in &ctx.pads {
                    if let Some(i) = p.slot {
                        let st = p.map.state(&p.raw);
                        // real pads alone can quit the game (Home held), not phones
                        quit |= !guide_through
                            && !p.external
                            && (st.pressed(Btn::Guide) || (!p.guide && st.pressed(Btn::Back) && st.pressed(Btn::Start)));
                        s[i].merge(&st);
                    }
                }
                prev.resize(s.len(), State::default());
                if debug && s != prev {
                    eprintln!("[azotopad] {s:?}");
                }
                let now = Instant::now();
                let dt = (now - last).as_secs_f32().min(0.05);
                last = now;
                if let Err(e) = out.update(&prev, &s, dt, guide_through) {
                    eprintln!("[azotopad] output: {e}");
                }
                while let Ok((keys, combo)) = tap_rx.try_recv() {
                    if let Err(e) = out.tap(&keys, combo) {
                        eprintln!("[azotopad] tap: {e}");
                    }
                }

                match (quit, quit_since) {
                    (true, None) => quit_since = Some(now),
                    (true, Some(t)) if now - t >= QUIT_HOLD => {
                        quit_since = Some(now + Duration::from_secs(3600));
                        on_quit();
                    }
                    (false, _) => quit_since = None,
                    _ => {}
                }
                prev = s;
            }
            // releases the grabs and removes the virtual devices
            drop(ctx);
            drop(out);
        })?;
        Ok(Session { stop, thread: Some(thread), pads, taps, sensors, motion: None })
    }

    /// Which players' controllers have motion (Joy-Cons, Pro Controller).
    pub fn motion_players(&self) -> Vec<bool> {
        self.sensors.lock().unwrap().iter().map(|s| s.main.is_some()).collect()
    }

    /// Sends the players' controller motion (Joy-Cons, Pro Controller) to
    /// the emulator over DSU, until the session ends.
    pub fn serve_motion(&mut self) -> io::Result<()> {
        if self.motion.is_none() {
            self.motion = Some(Motion::start(self.sensors.clone())?);
        }
        Ok(())
    }

    pub fn tapper(&self) -> Tapper {
        Tapper(self.taps.clone())
    }

    /// vendor/product of the real pads currently grabbed
    pub fn pads(&self) -> Vec<PadId> {
        self.pads.lock().unwrap().iter().map(|(id, _)| *id).collect()
    }

    /// How many grabbed pads play (an asleep Joy-Con pair doesn't, yet).
    pub fn playing(&self) -> usize {
        self.pads.lock().unwrap().iter().filter(|(_, has)| *has).count()
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.motion = None;
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Pad {
    path: PathBuf,
    dev: RawDevice,
    map: Mapping,
    raw: Raw,
    /// the player (virtual pad) it drives; given right after it is grabbed
    slot: Option<usize>,
    /// a phone's [`ExternalPad`]
    external: bool,
    /// a Joy-Con pair that was asleep when grabbed: no player until it sends input
    dormant: bool,
    /// it can rumble (FF_RUMBLE, and AzotoTV may write to it)
    rumbles: bool,
    /// the effect replaying its player's rumble, uploaded on first use
    rumble: Option<FFEffect>,
    /// scans left that (re)light its player number (joycond may light a
    /// pair's Joy-Cons its own way right after they connect)
    light: u8,
    /// it has a Home / Guide button; without one Back+Start held quits instead
    /// (with one, Back+Start stays the game's: Tomodachi's recomp opens its
    /// settings with - and + held)
    guide: bool,
}

struct Ctx {
    pads: Vec<Pad>,
    /// every grabbed pad, and whether it has a player
    ids: Arc<Mutex<Vec<(PadId, bool)>>>,
    last_scan: Option<Instant>,
    /// our own virtual devices, never grabbed
    own: Vec<PathBuf>,
    players: usize,
    /// Nintendo controllers' motion devices, in the order they appeared
    imus: Vec<ImuDev>,
    sensors: SharedSensors,
    /// a dormant pad sent input: it needs a player
    woke: bool,
}

struct ImuDev {
    path: PathBuf,
    name: String,
    /// the controller's MAC, like its pad device's
    uniq: String,
}

/// The virtual pad joycond makes of two Joy-Cons (L + R pressed together)
const COMBINED_JOYCONS: PadId = (0x057e, 0x2008);

/// A Joy-Con's or Pro Controller's motion device (hid-nintendo's "... (IMU)").
fn is_nintendo_imu(dev: &RawDevice) -> bool {
    dev.input_id().vendor() == 0x057e && dev.properties().contains(PropType::ACCELEROMETER)
}

/// Joy-Cons joycond holds (in a pair, or waiting for L+R / SL+SR): their
/// device exists but only root can open it.
pub fn held_joycons() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/sys/class/input") else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("event"))
        .filter(|e| {
            let read = |f: &str| std::fs::read_to_string(e.path().join("device").join(f)).unwrap_or_default();
            let name = read("name");
            read("id/vendor").trim() == "057e" && name.contains("Joy-Con") && !name.contains("IMU") && !name.contains("Combined")
        })
        .filter(|e| std::fs::File::open(PathBuf::from("/dev/input").join(e.file_name())).is_err())
        .filter_map(|e| std::fs::canonicalize(e.path()).ok())
        .collect()
}

fn is_pair(dev: &RawDevice) -> bool {
    let id = dev.input_id();
    (id.vendor(), id.product()) == COMBINED_JOYCONS
}

/// When the device node appeared (= when the controller was switched on).
fn ctime(path: &PathBuf) -> (i64, i64) {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).map(|m| (m.ctime(), m.ctime_nsec())).unwrap_or((i64::MAX, 0))
}

/// Whether Sunshine's virtual pads count (see [`set_stream_pads`]).
static STREAM_PADS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Sunshine (game streaming) keeps the virtual pad of a Moonlight client
/// after it leaves, for days: with `false` it isn't a controller (no player,
/// not on the Controller screen) until a client streams again.
pub fn set_stream_pads(on: bool) {
    STREAM_PADS.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn is_stream_pad(dev: &RawDevice) -> bool {
    dev.name().is_some_and(|n| n.starts_with("Sunshine "))
}

/// A game controller: gamepad/joystick buttons and a stick (keyboards and
/// mice have neither, TV remotes no stick).
pub fn is_pad(dev: &RawDevice) -> bool {
    if is_stream_pad(dev) && !STREAM_PADS.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    let keys = dev.supported_keys();
    let has_btn = keys.is_some_and(|k| {
        k.contains(Key::BTN_SOUTH) || k.iter().any(|c| (BTN_JOYSTICK..BTN_DIGI).contains(&c.0))
    });
    let has_stick = dev.supported_absolute_axes().is_some_and(|a| a.contains(Abs::ABS_X));
    has_btn && has_stick
}

impl Ctx {
    fn scan(&mut self) {
        self.last_scan = Some(Instant::now());
        let mut found: Vec<(PathBuf, RawDevice)> = Vec::new();
        self.imus.clear();
        for (path, dev) in evdev::raw_stream::enumerate() {
            if is_nintendo_imu(&dev) {
                let name = dev.name().unwrap_or("").to_string();
                let uniq = dev.unique_name().unwrap_or("").to_string();
                self.imus.push(ImuDev { path, name, uniq });
            } else if !self.pads.iter().any(|p| p.path == path) && !self.own.contains(&path) && is_pad(&dev) {
                found.push((path, dev));
            }
        }
        // players in the order the pads were plugged in (when their node appeared)
        found.sort_by_key(|(path, _)| ctime(path));
        self.imus.sort_by_key(|i| ctime(&i.path));
        // joycond keeps a pair's pad while both Joy-Cons are off; asleep it
        // must not take a player (it would be player 1 while someone else
        // plays), so a pair waits for its first input unless every pair has
        // a held Joy-Con behind it (like the Controller screen's `asleep`)
        let pairs = self.pads.iter().filter(|p| is_pair(&p.dev)).count() + found.iter().filter(|(_, d)| is_pair(d)).count();
        let asleep = found.iter().any(|(_, d)| is_pair(d)) && pairs > held_joycons().len().div_ceil(2);
        for (path, mut dev) in found {
            if let Err(e) = dev.grab() {
                eprintln!("[azotopad] cannot grab {}: {e}", path.display());
                continue;
            }
            unsafe {
                let fd = dev.as_raw_fd();
                libc::fcntl(fd, libc::F_SETFL, libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK);
            }
            let map = Mapping::for_device(&dev, &path);
            eprintln!(
                "[azotopad] grabbed {} \"{}\" ({})",
                path.display(),
                dev.name().unwrap_or("?"),
                if map.from_db { format!("mapping: {}", map.name) } else { "kernel codes".into() }
            );
            let raw = Raw::new(&dev);
            let id = dev.input_id();
            let external = (id.vendor(), id.product()) == EXTERNAL_PAD_ID;
            let dormant = asleep && is_pair(&dev);
            if dormant {
                eprintln!("[azotopad] {} looks asleep: no player until it is used", path.display());
            }
            let rumbles = dev.supported_ff().is_some_and(|f| f.contains(FFEffectCode::FF_RUMBLE));
            let guide = map.has_guide(&dev);
            self.pads.push(Pad { path, dev, map, raw, slot: None, external, dormant, rumbles, rumble: None, light: 0, guide });
        }
        self.publish();
    }

    fn publish(&self) {
        *self.ids.lock().unwrap() = self
            .pads
            .iter()
            .map(|p| ((p.dev.input_id().vendor(), p.dev.input_id().product()), p.slot.is_some()))
            .collect();
    }

    /// Gives each new pad the first free player, adding a virtual pad when
    /// every existing one is taken.
    fn assign(&mut self, out: &mut Output) {
        let max = out.max_slots().min(self.players);
        let mut taken: Vec<usize> = self.pads.iter().filter_map(|p| p.slot).collect();
        let mut free = |out: &mut Output, own: &mut Vec<PathBuf>| {
            let want = (0..max).find(|s| !taken.contains(s)).unwrap_or(max - 1);
            while out.slots() <= want {
                match out.add_pad() {
                    Ok(nodes) => own.extend(nodes),
                    Err(e) => {
                        eprintln!("[azotopad] cannot add a virtual pad: {e}");
                        break;
                    }
                }
            }
            let slot = want.min(out.slots() - 1);
            taken.push(slot);
            slot
        };
        for i in 0..self.pads.len() {
            if self.pads[i].slot.is_none() && !self.pads[i].dormant {
                let slot = free(out, &mut self.own);
                self.pads[i].slot = Some(slot);
                self.pads[i].light = 2;
                if max > 1 {
                    eprintln!("[azotopad] {} is player {}", self.pads[i].path.display(), slot + 1);
                }
            }
        }
        for p in self.pads.iter_mut().filter(|p| p.light > 0) {
            p.light -= 1;
            crate::lights::show(&p.path, p.slot.map(|s| s + 1));
        }
        let sensors = self.player_sensors(out.slots());
        let mut shared = self.sensors.lock().unwrap();
        if *shared != sensors {
            for (i, s) in sensors.iter().enumerate() {
                if let Some(m) = &s.main {
                    let left = s.left.as_ref().map(|l| format!(" + {}", l.path.display())).unwrap_or_default();
                    eprintln!("[azotopad] player {} motion: {}{left}", i + 1, m.path.display());
                }
            }
            *shared = sensors;
        }
        drop(shared);
        self.publish();
    }

    /// Where each player's motion comes from: their first controller that
    /// has an IMU. A single Joy-Con or a Pro Controller has its own (same
    /// MAC); a pair joined by joycond has no MAC, so the pairs take the
    /// Joy-Cons that aren't used alone, in the order both appeared.
    fn player_sensors(&self, slots: usize) -> Vec<Sensors> {
        let alone: Vec<&str> = self.pads.iter().filter_map(|p| p.dev.unique_name()).filter(|u| !u.is_empty()).collect();
        let alone = &alone;
        let paired = |side: &'static str| {
            self.imus.iter().filter(move |i| i.name.starts_with(side) && !alone.contains(&i.uniq.as_str()))
        };
        let (mut lefts, mut rights) = (paired("Joy-Con (L)"), paired("Joy-Con (R)"));
        let imu = |i: &ImuDev, flip: bool| Imu { path: i.path.clone(), flip };
        let mut out = vec![Sensors::default(); slots];
        for p in self.pads.iter().filter(|p| p.slot.is_some()) {
            let s = if is_pair(&p.dev) {
                match (lefts.next(), rights.next()) {
                    (l, Some(r)) => Sensors { main: Some(imu(r, false)), left: l.map(|l| imu(l, false)) },
                    (l, None) => Sensors { main: l.map(|l| imu(l, false)), left: None },
                }
            } else {
                let uniq = p.dev.unique_name().unwrap_or("");
                match self.imus.iter().find(|i| !uniq.is_empty() && i.uniq == uniq) {
                    // alone, a Joy-Con is held sideways
                    Some(i) => Sensors { main: Some(imu(i, i.name.starts_with("Joy-Con (R)"))), left: None },
                    None => continue,
                }
            };
            if let Some(slot) = p.slot.filter(|&s| s < slots) {
                if out[slot].main.is_none() {
                    out[slot] = s;
                }
            }
        }
        out
    }

    /// Waits up to `timeout_ms` for input (or for a request on one of `also`,
    /// the virtual pads) and applies it to the raw states.
    fn read(&mut self, timeout_ms: i32, also: &[RawFd]) {
        let mut fds: Vec<libc::pollfd> = self
            .pads
            .iter()
            .map(|p| p.dev.as_raw_fd())
            .chain(also.iter().copied())
            .map(|fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 })
            .collect();
        // with no pads at all this just sleeps
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
        let mut gone = Vec::new();
        for (i, p) in self.pads.iter_mut().enumerate() {
            if fds[i].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                gone.push(i);
                continue;
            }
            if fds[i].revents & libc::POLLIN == 0 {
                continue;
            }
            match p.dev.fetch_events() {
                Ok(evs) => {
                    if p.dormant {
                        p.dormant = false;
                        self.woke = true;
                        eprintln!("[azotopad] {} woke up", p.path.display());
                    }
                    for ev in evs {
                        match ev.event_type() {
                            EventType::KEY => {
                                p.raw.keys.insert(ev.code(), ev.value() != 0);
                            }
                            EventType::ABSOLUTE => p.raw.set_abs(ev.code(), ev.value()),
                            _ => {}
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => gone.push(i),
            }
        }
        for i in gone.into_iter().rev() {
            let p = self.pads.remove(i);
            eprintln!("[azotopad] lost {}", p.path.display());
        }
        if !self.pads.is_empty() || self.ids.lock().unwrap().len() != 0 {
            self.publish();
        }
    }
}

impl Ctx {
    /// Plays (Some) or stops (None) a player's rumble on their real pads.
    fn rumble(&mut self, slot: usize, effect: Option<FFEffectData>) {
        for p in self.pads.iter_mut().filter(|p| p.slot == Some(slot) && p.rumbles) {
            let done = match (effect, &mut p.rumble) {
                (Some(data), Some(r)) => r.update(data).and_then(|_| r.play(1)),
                (Some(data), None) => p.dev.upload_ff_effect(data).and_then(|mut r| {
                    r.play(1)?;
                    p.rumble = Some(r);
                    Ok(())
                }),
                (None, Some(r)) => r.stop(),
                (None, None) => Ok(()),
            };
            if let Err(e) = done {
                eprintln!("[azotopad] {} can't rumble: {e}", p.path.display());
                p.rumbles = false;
                p.rumble = None;
            }
        }
    }
}

/// A player's virtual Xbox pad. Games rumble it like a real one (FF_RUMBLE):
/// the effects they upload are kept here and replayed on the player's real
/// pads (see `Ctx::rumble`).
struct XPad {
    dev: VirtualDevice,
    effects: HashMap<i16, FFEffectData>,
}

/// A player's pad: what games and emulators expect (xpad's names).
fn xbox_pad() -> io::Result<XPad> {
    XPad::new("Microsoft X-Box 360 pad", InputId::new(BusType::BUS_USB, 0x045e, 0x028e, 0x0114))
}

/// Effects a game may upload to a virtual pad at once.
const FF_EFFECTS: u32 = 16;

impl XPad {
    fn new(name: &str, id: InputId) -> io::Result<XPad> {
        let dev = pad_device(name, id)?;
        // drained in the loop, never waited on (an upload waits for our answer)
        unsafe {
            let fd = dev.as_raw_fd();
            libc::fcntl(fd, libc::F_SETFL, libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK);
        }
        Ok(XPad { dev, effects: HashMap::new() })
    }

    /// Answers the game's uploads/erases; returns the effects it started
    /// (Some) or stopped (None).
    fn requests(&mut self) -> Vec<Option<FFEffectData>> {
        let mut out = Vec::new();
        let evs: Vec<InputEvent> = match self.dev.fetch_events() {
            Ok(evs) => evs.collect(),
            Err(_) => return out,
        };
        for ev in evs {
            match ev.destructure() {
                EventSummary::UInput(ev, UInputCode::UI_FF_UPLOAD, ..) => {
                    let Ok(mut up) = self.dev.process_ff_upload(ev) else { continue };
                    let id = match up.effect_id() {
                        -1 => (0..FF_EFFECTS as i16).find(|i| !self.effects.contains_key(i)),
                        id => Some(id),
                    };
                    match id {
                        Some(id) => {
                            up.set_effect_id(id);
                            self.effects.insert(id, up.effect());
                            up.set_retval(0);
                        }
                        None => up.set_retval(-libc::ENOSPC),
                    }
                }
                EventSummary::UInput(ev, UInputCode::UI_FF_ERASE, ..) => {
                    if let Ok(er) = self.dev.process_ff_erase(ev) {
                        self.effects.remove(&(er.effect_id() as i16));
                    }
                }
                // value = how many times to play it, 0 = stop
                EventSummary::ForceFeedback(_, code, value) if code.0 < FF_EFFECTS as u16 => {
                    if let Some(&data) = self.effects.get(&(code.0 as i16)) {
                        out.push((value > 0).then_some(data));
                    }
                }
                _ => {}
            }
        }
        out
    }
}

enum Output {
    /// one pad per player, and a keyboard for taps
    Xbox(Vec<XPad>, VirtualDevice),
    Keys(KeysOut),
}

/// vendor/product of [`ExternalPad`]s (AzotoTV's own virtual devices are 0x4e54)
pub const EXTERNAL_PAD_ID: PadId = (0x4e54, 0x0010);

/// A pad driven from outside, e.g. a phone: a virtual pad of its own, so
/// AzotoTV's menus and every game session see it like a real pad plugged in
/// (taking the next player), except it can't close a game. Dropping it
/// unplugs it.
///
/// It rumbles like the players' pads: a thread of its own answers the
/// game's (azotopad's) uploads at once, so nothing waits on whoever drives
/// the pad, and hands each rumble to `on_rumble`.
pub struct ExternalPad {
    pad: Arc<Mutex<XPad>>,
    state: State,
    node: Option<PathBuf>,
    stop: Arc<AtomicBool>,
}

/// A rumble for an [`ExternalPad`]: how long (ms, 0 = stop) and how strong
/// (0..=65535, the stronger motor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rumble {
    pub ms: u16,
    pub strength: u16,
}

impl ExternalPad {
    pub fn new(name: &str, on_rumble: impl Fn(Rumble) + Send + 'static) -> io::Result<ExternalPad> {
        let (v, p) = EXTERNAL_PAD_ID;
        let mut pad = XPad::new(name, InputId::new(BusType::BUS_VIRTUAL, v, p, 1))?;
        let node = event_nodes(&mut pad.dev).into_iter().next();
        let fd = pad.dev.as_raw_fd();
        let pad = Arc::new(Mutex::new(pad));
        let stop = Arc::new(AtomicBool::new(false));
        let (pad2, stop2) = (pad.clone(), stop.clone());
        std::thread::Builder::new().name("external-pad".into()).spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
                if unsafe { libc::poll(&mut pfd, 1, 250) } <= 0 {
                    continue;
                }
                let requests = pad2.lock().unwrap().requests();
                for r in requests {
                    on_rumble(match r {
                        Some(FFEffectData { kind: FFEffectKind::Rumble { strong_magnitude, weak_magnitude }, replay, .. }) => {
                            // length 0 = until stopped
                            let ms = if replay.length == 0 { 1000 } else { replay.length.min(5000) };
                            Rumble { ms, strength: strong_magnitude.max(weak_magnitude) }
                        }
                        _ => Rumble { ms: 0, strength: 0 },
                    });
                }
            }
        })?;
        Ok(ExternalPad { pad, state: State::default(), node, stop })
    }

    /// Its /dev/input/event* node.
    pub fn node(&self) -> Option<&PathBuf> {
        self.node.as_ref()
    }

    pub fn set(&mut self, s: State) -> io::Result<()> {
        xbox_update(&mut self.pad.lock().unwrap().dev, &self.state, &s, false)?;
        self.state = s;
        Ok(())
    }
}

impl Drop for ExternalPad {
    fn drop(&mut self) {
        // the thread ends within a poll; the device goes with the last Arc
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// An Xbox-like pad that takes FF_RUMBLE effects (someone must answer its
/// uploads: an unanswered one hangs the uploader for up to 30 s).
fn pad_device(name: &str, id: InputId) -> io::Result<VirtualDevice> {
    let stick = AbsInfo::new(0, -32768, 32767, 16, 128, 0);
    let trig = AbsInfo::new(0, 0, 255, 0, 0, 0);
    let hat = AbsInfo::new(0, -1, 1, 0, 0, 0);
    let mut b = VirtualDevice::builder()?
        .name(name)
        .input_id(id)
        .with_keys(&key_set(XBOX_BUTTONS.iter().map(|&(_, k)| k).chain([Key::BTN_MODE])))?;
    for (a, info) in [
        (Abs::ABS_X, stick), (Abs::ABS_Y, stick), (Abs::ABS_RX, stick), (Abs::ABS_RY, stick),
        (Abs::ABS_Z, trig), (Abs::ABS_RZ, trig), (Abs::ABS_HAT0X, hat), (Abs::ABS_HAT0Y, hat),
    ] {
        b = b.with_absolute_axis(&UinputAbsSetup::new(a, info))?;
    }
    b.with_ff(&AttributeSet::from_iter([FFEffectCode::FF_RUMBLE]))?.with_ff_effects_max(FF_EFFECTS).build()
}

fn event_nodes(d: &mut VirtualDevice) -> Vec<PathBuf> {
    d.enumerate_dev_nodes_blocking()
        .map(|nodes| nodes.flatten().filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event"))).collect())
        .unwrap_or_default()
}

fn keyboard() -> io::Result<VirtualDevice> {
    VirtualDevice::builder()?
        .name("AzotoTV keyboard")
        .input_id(InputId::new(BusType::BUS_VIRTUAL, 0x4e54, 0x0001, 1))
        .with_keys(&key_set((1..=248).map(Key)))?
        .build()
}

struct KeysOut {
    kbd: VirtualDevice,
    mouse: VirtualDevice,
    map: KeyMap,
    /// how many inputs hold each key/button down
    held: HashMap<u16, u32>,
    /// directions currently pressed by each stick (up, down, left, right)
    dirs: [[bool; 4]; 2],
    /// sub-pixel / sub-notch remainders
    frac: [f32; 3],
}

fn key_set(keys: impl IntoIterator<Item = Key>) -> AttributeSet<Key> {
    let mut s = AttributeSet::new();
    for k in keys {
        s.insert(k);
    }
    s
}

const XBOX_BUTTONS: [(Btn, Key); 10] = [
    (Btn::A, Key::BTN_SOUTH),
    (Btn::B, Key::BTN_EAST),
    // xpad reports X/Y as BTN_X/BTN_Y, which are BTN_NORTH/BTN_WEST
    (Btn::X, Key::BTN_NORTH),
    (Btn::Y, Key::BTN_WEST),
    (Btn::LB, Key::BTN_TL),
    (Btn::RB, Key::BTN_TR),
    (Btn::Back, Key::BTN_SELECT),
    (Btn::Start, Key::BTN_START),
    (Btn::LeftStick, Key::BTN_THUMBL),
    (Btn::RightStick, Key::BTN_THUMBR),
];

impl Output {
    /// /dev/input/event* nodes of the virtual devices
    fn nodes(&mut self) -> Vec<PathBuf> {
        let devs: Vec<&mut VirtualDevice> = match self {
            Output::Xbox(pads, k) => pads.iter_mut().map(|p| &mut p.dev).chain([k]).collect(),
            Output::Keys(k) => vec![&mut k.kbd, &mut k.mouse],
        };
        devs.into_iter().flat_map(event_nodes).collect()
    }

    /// players that have a virtual device
    fn slots(&self) -> usize {
        match self {
            Output::Xbox(pads, _) => pads.len(),
            Output::Keys(_) => 1,
        }
    }

    fn max_slots(&self) -> usize {
        match self {
            Output::Xbox(..) => MAX_PLAYERS,
            Output::Keys(_) => 1,
        }
    }

    /// One more player's virtual pad; returns its device nodes.
    fn add_pad(&mut self) -> io::Result<Vec<PathBuf>> {
        let Output::Xbox(pads, _) = self else { return Ok(Vec::new()) };
        let mut d = xbox_pad()?;
        let nodes = event_nodes(&mut d.dev);
        pads.push(d);
        Ok(nodes)
    }

    /// The virtual pads' fds, to wake up for the game's rumble requests.
    fn ff_fds(&self) -> Vec<RawFd> {
        match self {
            Output::Xbox(pads, _) => pads.iter().map(|p| p.dev.as_raw_fd()).collect(),
            Output::Keys(_) => Vec::new(),
        }
    }

    /// (player, effect started / None = stopped) the game asked for.
    fn rumble_requests(&mut self) -> Vec<(usize, Option<FFEffectData>)> {
        let Output::Xbox(pads, _) = self else { return Vec::new() };
        pads.iter_mut().enumerate().flat_map(|(i, p)| p.requests().into_iter().map(move |r| (i, r))).collect()
    }

    fn new(profile: &Profile, sensitivity: f32) -> io::Result<Output> {
        match profile {
            // player 1's pad exists from the start, so the game finds a pad
            // even if none is switched on yet
            Profile::Xbox | Profile::Stream => Ok(Output::Xbox(vec![xbox_pad()?], keyboard()?)),
            Profile::Keys { map, mouse_speed } => {
                let map = KeyMap::parse(map, mouse_speed * sensitivity)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                let kbd = keyboard()?;
                let mut rel = AttributeSet::new();
                for r in [Rel::REL_X, Rel::REL_Y, Rel::REL_WHEEL] {
                    rel.insert(r);
                }
                let mouse = VirtualDevice::builder()?
                    .name("AzotoTV mouse")
                    .input_id(InputId::new(BusType::BUS_VIRTUAL, 0x4e54, 0x0002, 1))
                    .with_keys(&key_set([Key::BTN_LEFT, Key::BTN_RIGHT, Key::BTN_MIDDLE]))?
                    .with_relative_axes(&rel)?
                    .build()?;
                Ok(Output::Keys(KeysOut { kbd, mouse, map, held: HashMap::new(), dirs: [[false; 4]; 2], frac: [0.0; 3] }))
            }
        }
    }

    /// One key after the other, or (`combo`) all held together.
    fn tap(&mut self, keys: &[Key], combo: bool) -> io::Result<()> {
        let kbd = match self {
            Output::Xbox(_, k) => k,
            Output::Keys(k) => &mut k.kbd,
        };
        let key = |kbd: &mut VirtualDevice, k: &Key, v| kbd.emit(&[InputEvent::new(EventType::KEY.0, k.0, v)]);
        if combo {
            for k in keys {
                key(kbd, k, 1)?;
                std::thread::sleep(Duration::from_millis(40));
            }
            for k in keys.iter().rev() {
                key(kbd, k, 0)?;
                std::thread::sleep(Duration::from_millis(20));
            }
            return Ok(());
        }
        for k in keys {
            key(kbd, k, 1)?;
            std::thread::sleep(Duration::from_millis(40));
            key(kbd, k, 0)?;
        }
        Ok(())
    }

    /// Whether a stick is driving the mouse/wheel, so the loop must keep ticking.
    fn moving(&self, s: &State) -> bool {
        let Output::Keys(k) = self else { return false };
        let active = |stick: Option<StickOut>, x: Ax, y: Ax| {
            matches!(stick, Some(StickOut::Mouse | StickOut::Scroll)) && s.axis(x).hypot(s.axis(y)) > DEADZONE
        };
        active(k.map.left, Ax::LX, Ax::LY) || active(k.map.right, Ax::RX, Ax::RY)
    }

    /// `prev`/`s`: one state per player. `guide`: Home goes to the virtual pads too.
    fn update(&mut self, prev: &[State], s: &[State], dt: f32, guide: bool) -> io::Result<()> {
        match self {
            Output::Xbox(pads, _) => {
                for (i, pad) in pads.iter_mut().enumerate() {
                    xbox_update(&mut pad.dev, &prev[i], &s[i], guide)?;
                }
                Ok(())
            }
            Output::Keys(k) => k.update(&prev[0], &s[0], dt),
        }
    }
}

fn xbox_update(dev: &mut VirtualDevice, prev: &State, s: &State, guide: bool) -> io::Result<()> {
    let mut evs = Vec::new();
    if guide && prev.pressed(Btn::Guide) != s.pressed(Btn::Guide) {
        evs.push(InputEvent::new(EventType::KEY.0, Key::BTN_MODE.0, s.pressed(Btn::Guide) as i32));
    }
    for (b, k) in XBOX_BUTTONS {
        if prev.pressed(b) != s.pressed(b) {
            evs.push(InputEvent::new(EventType::KEY.0, k.0, s.pressed(b) as i32));
        }
    }
    let hat = |s: &State| {
        (s.pressed(Btn::Right) as i32 - s.pressed(Btn::Left) as i32,
         s.pressed(Btn::Down) as i32 - s.pressed(Btn::Up) as i32)
    };
    let (h0, h1) = (hat(prev), hat(s));
    if h0.0 != h1.0 { evs.push(InputEvent::new(EventType::ABSOLUTE.0, Abs::ABS_HAT0X.0, h1.0)); }
    if h0.1 != h1.1 { evs.push(InputEvent::new(EventType::ABSOLUTE.0, Abs::ABS_HAT0Y.0, h1.1)); }
    let stick = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0) as i32;
    let trig = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as i32;
    for (a, code, f) in [
        (Ax::LX, Abs::ABS_X, stick as fn(f32) -> i32), (Ax::LY, Abs::ABS_Y, stick),
        (Ax::RX, Abs::ABS_RX, stick), (Ax::RY, Abs::ABS_RY, stick),
        (Ax::LT, Abs::ABS_Z, trig), (Ax::RT, Abs::ABS_RZ, trig),
    ] {
        let v = f(s.axis(a));
        if v != f(prev.axis(a)) {
            evs.push(InputEvent::new(EventType::ABSOLUTE.0, code.0, v));
        }
    }
    if !evs.is_empty() {
        dev.emit(&evs)?;
    }
    Ok(())
}

fn active(s: &State, i: Input) -> bool {
    match i {
        Input::Btn(b) => s.pressed(b),
        Input::Trigger(a) => s.axis(a) > 0.5,
    }
}

impl KeysOut {
    fn press(&mut self, out: Out, down: bool, kbd: &mut Vec<InputEvent>, mouse: &mut Vec<InputEvent>) {
        let (code, evs) = match out {
            Out::Key(k) => (k.0, kbd),
            Out::Mouse(k) => (k.0, mouse),
            Out::Wheel(n) => {
                if down {
                    mouse.push(InputEvent::new(EventType::RELATIVE.0, Rel::REL_WHEEL.0, n));
                }
                return;
            }
        };
        let n = self.held.entry(code).or_insert(0);
        let was = *n > 0;
        if down { *n += 1 } else { *n = n.saturating_sub(1) }
        if was != (*n > 0) {
            evs.push(InputEvent::new(EventType::KEY.0, code, (*n > 0) as i32));
        }
    }

    fn update(&mut self, prev: &State, s: &State, dt: f32) -> io::Result<()> {
        let (mut kbd, mut mouse) = (Vec::new(), Vec::new());
        for i in 0..self.map.buttons.len() {
            let (input, out) = self.map.buttons[i];
            let (was, now) = (active(prev, input), active(s, input));
            if was != now {
                self.press(out, now, &mut kbd, &mut mouse);
            }
        }
        for (side, stick, x, y) in [(0, self.map.left, Ax::LX, Ax::LY), (1, self.map.right, Ax::RX, Ax::RY)] {
            let (x, y) = (s.axis(x), s.axis(y));
            match stick {
                Some(StickOut::Keys(keys)) => {
                    // each axis on its own, so diagonals press two keys
                    let vals = [-y, y, -x, x];
                    for d in 0..4 {
                        let on = vals[d] > if self.dirs[side][d] { STICK_OFF } else { STICK_ON };
                        if on != self.dirs[side][d] {
                            self.dirs[side][d] = on;
                            self.press(Out::Key(keys[d]), on, &mut kbd, &mut mouse);
                        }
                    }
                }
                Some(StickOut::Mouse) => {
                    let mag = x.hypot(y);
                    if mag > DEADZONE {
                        let t = ((mag - DEADZONE) / (1.0 - DEADZONE)).min(1.0);
                        let scale = t * t * self.map.mouse_speed * dt / mag;
                        self.frac[0] += x * scale;
                        self.frac[1] += y * scale;
                        for (i, rel) in [(0, Rel::REL_X), (1, Rel::REL_Y)] {
                            let whole = self.frac[i].trunc();
                            if whole != 0.0 {
                                self.frac[i] -= whole;
                                mouse.push(InputEvent::new(EventType::RELATIVE.0, rel.0, whole as i32));
                            }
                        }
                    }
                }
                Some(StickOut::Scroll) => {
                    if y.abs() > DEADZONE {
                        let t = ((y.abs() - DEADZONE) / (1.0 - DEADZONE)).min(1.0);
                        self.frac[2] += -y.signum() * t * SCROLL_RATE * dt;
                        let whole = self.frac[2].trunc();
                        if whole != 0.0 {
                            self.frac[2] -= whole;
                            mouse.push(InputEvent::new(EventType::RELATIVE.0, Rel::REL_WHEEL.0, whole as i32));
                        }
                    }
                }
                None => {}
            }
        }
        if !kbd.is_empty() {
            self.kbd.emit(&kbd)?;
        }
        if !mouse.is_empty() {
            self.mouse.emit(&mouse)?;
        }
        Ok(())
    }
}
