//! Native game controller input (gilrs/evdev). The WebView's Gamepad API drops
//! hat-style D-pads on some pads (e.g. Nacon GC100), so the UI gets controller
//! events from here instead: `pad` (action name), `pad-info` (device name) and
//! `pad-style` (whose buttons the hints should name, when the pad used changes).
//! A fires on release; held for HOLD it fires `a-hold` instead (tvOS-style
//! long press, e.g. to rearrange the home tiles).
//! While a game runs the pads belong to the game (see games.rs / azotopad):
//! `set_paused(true)` makes this reader ignore everything.

use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use gilrs::{Axis, Button, EventType, Gilrs};
use tauri::{AppHandle, Emitter};

const REPEAT_DELAY: Duration = Duration::from_millis(350);
const REPEAT_RATE: Duration = Duration::from_millis(110);
const HOLD: Duration = Duration::from_millis(550);
const STICK_ON: f32 = 0.6;
const STICK_OFF: f32 = 0.35;

/// Name of the first connected pad, for the UI (it may start listening late).
static PAD_NAME: Mutex<String> = Mutex::new(String::new());

/// True while a game owns the pads.
static PAUSED: AtomicBool = AtomicBool::new(false);

pub fn set_paused(paused: bool) {
    PAUSED.store(paused, Ordering::Relaxed);
    if !paused {
        // the game numbered the controllers its own way
        crate::controllers::show_lights_soon();
    }
}

/// A game owns the input (the TV remote checks this too, see remote.rs).
pub fn paused() -> bool {
    PAUSED.load(Ordering::Relaxed)
}

pub fn pad_name() -> Option<String> {
    let n = PAD_NAME.lock().unwrap().clone();
    (!n.is_empty()).then_some(n)
}

/// Nintendo pads by their printed letters (A on the right), see `azotopad::nintendo_letters`.
fn by_letter(b: Button, nintendo: bool) -> Button {
    match (nintendo, b) {
        (true, Button::South) => Button::East,
        (true, Button::East) => Button::South,
        (true, Button::North) => Button::West,
        (true, Button::West) => Button::North,
        _ => b,
    }
}

/// The button names printed on a pad: xbox, nintendo, playstation, or pad
/// (anything else; Xbox-like names, HOME).
fn style(vendor: Option<u16>) -> &'static str {
    match vendor {
        Some(0x045e) => "xbox",
        Some(0x057e) => "nintendo",
        Some(0x054c) => "playstation",
        _ => "pad",
    }
}

fn button_action(b: Button) -> Option<&'static str> {
    Some(match b {
        Button::DPadUp => "up",
        Button::DPadDown => "down",
        Button::DPadLeft => "left",
        Button::DPadRight => "right",
        Button::South => "a",
        Button::East => "b",
        Button::West => "x",
        Button::North => "y",
        Button::Start => "start",
        Button::Select => "select",
        Button::LeftTrigger => "lb",
        Button::RightTrigger => "rb",
        Button::LeftTrigger2 => "lt",
        Button::RightTrigger2 => "rt",
        _ => return None,
    })
}

fn repeats(action: &str) -> bool {
    matches!(action, "up" | "down" | "left" | "right" | "lb" | "rb")
}

/// Dominant stick direction with hysteresis (enter at ON, leave below OFF).
fn stick_dir(sx: f32, sy: f32, current: Option<&'static str>) -> Option<&'static str> {
    let (ax, ay) = (sx.abs(), sy.abs());
    let threshold = if current.is_some() { STICK_OFF } else { STICK_ON };
    if ax.max(ay) < threshold {
        return None;
    }
    Some(if ax >= ay {
        if sx > 0.0 { "right" } else { "left" }
    } else if sy > 0.0 {
        "up"
    } else {
        "down"
    })
}

pub fn spawn(app: AppHandle) {
    let debug = std::env::var_os("AZOTOTV_DEBUG").is_some();
    let _ = std::thread::Builder::new().name("gamepad".into()).spawn(move || {
        let mut gilrs = match Gilrs::new() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("[pad] cannot initialise controller input: {e}");
                return;
            }
        };
        for (_, g) in gilrs.gamepads() {
            eprintln!("[pad] found \"{}\" (mapping: {:?})", g.name(), g.mapping_source());
            *PAD_NAME.lock().unwrap() = g.name().to_string();
        }
        crate::controllers::show_lights_soon();

        // (source, action) -> next repeat time; source is "pad" or "stick"
        let mut held: HashMap<(&'static str, &'static str), Instant> = HashMap::new();
        let (mut sx, mut sy) = (0.0_f32, 0.0_f32);
        let mut stick: Option<&'static str> = None;
        // the right stick's sideways position as last sent (`pad-turn`)
        let mut turn = 0.0_f32;
        // when A went down; None once released or once the hold has fired
        let mut a_down: Option<Instant> = None;
        let mut last_style = "";

        let fire = |action: &'static str| {
            if debug {
                eprintln!("[pad] -> {action}");
            }
            let _ = app.emit("pad", action);
        };

        loop {
            // poll fast only while something can fire on its own (a held
            // direction repeating, A becoming a hold); otherwise sleep until
            // an event, so an idle pad doesn't wake the CPU 66 times a second
            let busy = a_down.is_some() || !held.is_empty();
            let wait = Duration::from_millis(if busy { 15 } else { 1000 });
            if let Some(ev) = gilrs.next_event_blocking(Some(wait)) {
                let pad = gilrs.connected_gamepad(ev.id);
                let nintendo = pad.is_some_and(|g| {
                    matches!((g.vendor_id(), g.product_id()), (Some(v), Some(p)) if azotopad::nintendo_letters(v, p))
                });
                if let Some(g) = pad.filter(|_| matches!(ev.event, EventType::ButtonPressed(..) | EventType::AxisChanged(..))) {
                    let s = style(g.vendor_id());
                    if s != last_style && !PAUSED.load(Ordering::Relaxed) {
                        last_style = s;
                        let _ = app.emit("pad-style", s);
                    }
                }
                match ev.event {
                    // a game's virtual pad (azotopad) comes and goes too: the
                    // name is always the first pad still connected
                    EventType::Connected | EventType::Disconnected => {
                        let name = gilrs.gamepads().next().map(|(_, g)| g.name().to_string()).unwrap_or_default();
                        eprintln!("[pad] {:?}; now: {name:?}", ev.event);
                        *PAD_NAME.lock().unwrap() = name.clone();
                        if !PAUSED.load(Ordering::Relaxed) {
                            crate::controllers::show_lights_soon();
                        }
                        held.clear();
                        stick = None;
                        a_down = None;
                        let _ = app.emit("pad-info", name);
                    }
                    _ if PAUSED.load(Ordering::Relaxed) => {
                        held.clear();
                        stick = None;
                        a_down = None;
                    }
                    EventType::ButtonPressed(b, _) => {
                        if debug {
                            eprintln!("[pad] button {b:?}");
                        }
                        match button_action(by_letter(b, nintendo)) {
                            Some("a") => a_down = Some(Instant::now()),
                            Some(a) => {
                                fire(a);
                                if repeats(a) {
                                    held.insert(("pad", a), Instant::now() + REPEAT_DELAY);
                                }
                            }
                            None => {}
                        }
                    }
                    EventType::ButtonReleased(b, _) => match button_action(by_letter(b, nintendo)) {
                        Some("a") => {
                            if a_down.take().is_some() {
                                fire("a");
                            }
                        }
                        Some(a) => {
                            held.remove(&("pad", a));
                        }
                        None => {}
                    },
                    // the right stick turns things (a Mii): its position, in
                    // steps of 0.1 with a dead zone, only when it changes
                    EventType::AxisChanged(Axis::RightStickX, v, _) => {
                        let q = if v.abs() < 0.25 { 0.0 } else { (v * 10.0).round() / 10.0 };
                        if q != turn {
                            turn = q;
                            let _ = app.emit("pad-turn", q);
                        }
                    }
                    EventType::AxisChanged(axis @ (Axis::LeftStickX | Axis::LeftStickY), v, _) => {
                        if axis == Axis::LeftStickX { sx = v } else { sy = v }
                        let next = stick_dir(sx, sy, stick);
                        if next != stick {
                            if let Some(old) = stick {
                                held.remove(&("stick", old));
                            }
                            if let Some(new) = next {
                                fire(new);
                                held.insert(("stick", new), Instant::now() + REPEAT_DELAY);
                            }
                            stick = next;
                        }
                    }
                    _ => {}
                }
            }
            let now = Instant::now();
            if a_down.is_some_and(|t| now - t >= HOLD) {
                a_down = None;
                fire("a-hold");
            }
            for ((_, action), next) in held.iter_mut() {
                if now >= *next {
                    fire(action);
                    *next = now + REPEAT_RATE;
                }
            }
        }
    });
}
