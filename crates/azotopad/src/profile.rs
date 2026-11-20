//! Per-game controller profiles, as stored in a game's `game.json`:
//!
//! ```json
//! {"mode": "xbox"}
//! {"mode": "keys", "map": {"a": "z", "b": "x", "y": "c", "start": "enter",
//!                          "dpad": "arrows", "leftstick": "arrows"}}
//! {"mode": "keys", "map": {"leftstick": "wasd", "rightstick": "mouse",
//!                          "rt": "mouse_left", "lt": "mouse_right"}, "mouse_speed": 1400}
//! ```
//! Button sources: a b x y back start ls rs lb rb lt rt up down left right
//! (`dpad` sets the four directions at once). Button targets: a key name
//! ("z", "enter", "esc", "space", "shift", "f1", or any `KEY_*`), mouse_left,
//! mouse_right, mouse_middle, wheel_up, wheel_down. Stick targets: mouse,
//! scroll, arrows, wasd. The guide button is never mapped: holding it asks
//! AzotoTV to close the game (except `{"mode": "stream"}`, which passes it on).

use std::{collections::HashMap, str::FromStr};

use evdev::KeyCode as Key;
use serde::Deserialize;

use crate::mapping::{Ax, Btn};

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Profile {
    /// The game sees a virtual Xbox 360 pad.
    Xbox,
    /// An Xbox 360 pad with its Guide button, for a game stream to another
    /// machine (`{"mode": "stream"}`): holding Home is that machine's (it
    /// closes its own game there), so it doesn't close the stream here.
    Stream,
    /// The game sees a keyboard and a mouse.
    Keys {
        #[serde(default)]
        map: HashMap<String, String>,
        /// pixels per second at full stick deflection (before the player's sensitivity)
        #[serde(default = "default_speed")]
        mouse_speed: f32,
    },
}

fn default_speed() -> f32 { 800.0 }

/// A logical input a profile can bind.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Input { Btn(Btn), Trigger(Ax) }

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Out {
    Key(Key),
    /// BTN_LEFT / BTN_RIGHT / BTN_MIDDLE
    Mouse(Key),
    /// +1 up, -1 down
    Wheel(i32),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StickOut {
    Mouse,
    Scroll,
    /// up, down, left, right
    Keys([Key; 4]),
}

pub struct KeyMap {
    pub buttons: Vec<(Input, Out)>,
    pub left: Option<StickOut>,
    pub right: Option<StickOut>,
    pub mouse_speed: f32,
}

fn input(name: &str) -> Option<Input> {
    use Btn::*;
    Some(match name {
        "a" => Input::Btn(A), "b" => Input::Btn(B), "x" => Input::Btn(X), "y" => Input::Btn(Y),
        "back" | "select" => Input::Btn(Back), "start" => Input::Btn(Start),
        "ls" => Input::Btn(LeftStick), "rs" => Input::Btn(RightStick),
        "lb" => Input::Btn(LB), "rb" => Input::Btn(RB),
        "up" => Input::Btn(Up), "down" => Input::Btn(Down),
        "left" => Input::Btn(Left), "right" => Input::Btn(Right),
        "lt" => Input::Trigger(Ax::LT), "rt" => Input::Trigger(Ax::RT),
        _ => return None,
    })
}

pub fn key(name: &str) -> Option<Key> {
    let k = match name.to_ascii_lowercase().as_str() {
        "enter" | "return" => Key::KEY_ENTER,
        "esc" | "escape" => Key::KEY_ESC,
        "space" => Key::KEY_SPACE,
        "tab" => Key::KEY_TAB,
        "backspace" => Key::KEY_BACKSPACE,
        "shift" => Key::KEY_LEFTSHIFT,
        "ctrl" | "control" => Key::KEY_LEFTCTRL,
        "alt" => Key::KEY_LEFTALT,
        "up" => Key::KEY_UP, "down" => Key::KEY_DOWN,
        "left" => Key::KEY_LEFT, "right" => Key::KEY_RIGHT,
        n => return Key::from_str(&format!("KEY_{}", n.to_ascii_uppercase()))
            .or_else(|_| Key::from_str(name)).ok(),
    };
    Some(k)
}

fn out(name: &str) -> Option<Out> {
    Some(match name {
        "mouse_left" => Out::Mouse(Key::BTN_LEFT),
        "mouse_right" => Out::Mouse(Key::BTN_RIGHT),
        "mouse_middle" => Out::Mouse(Key::BTN_MIDDLE),
        "wheel_up" => Out::Wheel(1),
        "wheel_down" => Out::Wheel(-1),
        k => Out::Key(key(k)?),
    })
}

fn four(name: &str) -> Option<[Key; 4]> {
    Some(match name {
        "arrows" => [Key::KEY_UP, Key::KEY_DOWN, Key::KEY_LEFT, Key::KEY_RIGHT],
        "wasd" => [Key::KEY_W, Key::KEY_S, Key::KEY_A, Key::KEY_D],
        _ => return None,
    })
}

fn stick(name: &str) -> Option<StickOut> {
    match name {
        "mouse" => Some(StickOut::Mouse),
        "scroll" => Some(StickOut::Scroll),
        n => four(n).map(StickOut::Keys),
    }
}

impl KeyMap {
    pub fn parse(map: &HashMap<String, String>, mouse_speed: f32) -> Result<KeyMap, String> {
        let mut km = KeyMap { buttons: Vec::new(), left: None, right: None, mouse_speed };
        for (src, dst) in map {
            let bad = || format!("unknown target \"{dst}\" for \"{src}\"");
            match src.as_str() {
                "leftstick" => km.left = Some(stick(dst).ok_or_else(bad)?),
                "rightstick" => km.right = Some(stick(dst).ok_or_else(bad)?),
                "dpad" => {
                    let k = four(dst).ok_or_else(bad)?;
                    for (b, k) in [Btn::Up, Btn::Down, Btn::Left, Btn::Right].into_iter().zip(k) {
                        km.buttons.push((Input::Btn(b), Out::Key(k)));
                    }
                }
                s => {
                    let i = input(s).ok_or_else(|| format!("unknown pad input \"{s}\""))?;
                    km.buttons.push((i, out(dst).ok_or_else(bad)?));
                }
            }
        }
        Ok(km)
    }
}
