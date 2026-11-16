//! Turns a real pad's raw evdev state into a logical Xbox-style state, using
//! the SDL_GameControllerDB entry for the pad (same data SDL and gilrs use).
//! Pads without an entry fall back to the kernel's gamepad codes.

use std::{collections::HashMap, path::Path};

use evdev::{AbsoluteAxisCode as Abs, KeyCode as Key, raw_stream::RawDevice};

/// first joystick/gamepad button code (BTN_JOYSTICK); BTN_DIGI starts after them
pub const BTN_JOYSTICK: u16 = 0x120;
pub const BTN_DIGI: u16 = 0x140;

static DB: &str = include_str!("../gamecontrollerdb.txt");

/// Logical buttons, Xbox names.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Btn {
    A, B, X, Y, Back, Guide, Start, LeftStick, RightStick,
    LB, RB, Up, Down, Left, Right,
    /// Capture on Nintendo controllers, Share on others: no Xbox button,
    /// free for AzotoTV's own uses
    Misc1,
}
pub const BTNS: usize = 16;

/// Logical axes: sticks -1..1 (y positive = down, like SDL), triggers 0..1.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Ax { LX, LY, RX, RY, LT, RT }
pub const AXES: usize = 6;

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct State {
    pub btn: [bool; BTNS],
    pub ax: [f32; AXES],
}

impl State {
    pub fn pressed(&self, b: Btn) -> bool { self.btn[b as usize] }
    pub fn axis(&self, a: Ax) -> f32 { self.ax[a as usize] }

    /// Several pads drive the same output: buttons OR, axes keep the larger deflection.
    pub fn merge(&mut self, o: &State) {
        for i in 0..BTNS { self.btn[i] |= o.btn[i]; }
        for i in 0..AXES {
            if o.ax[i].abs() > self.ax[i].abs() { self.ax[i] = o.ax[i]; }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Target { Btn(Btn), Ax(Ax) }

#[derive(Clone, Copy, Debug)]
enum Source {
    Key(u16),
    /// axis code, half (-1, 0 = whole, +1), inverted
    Abs(u16, i8, bool),
    /// hat number, direction mask (1 up, 2 right, 4 down, 8 left)
    Hat(u16, u8),
}

pub struct Mapping {
    pub name: String,
    pub from_db: bool,
    binds: Vec<(Source, Target)>,
}

fn target(name: &str) -> Option<Target> {
    use Btn::*;
    Some(match name {
        "a" => Target::Btn(A), "b" => Target::Btn(B), "x" => Target::Btn(X), "y" => Target::Btn(Y),
        "back" => Target::Btn(Back), "guide" => Target::Btn(Guide), "start" => Target::Btn(Start),
        "leftstick" => Target::Btn(LeftStick), "rightstick" => Target::Btn(RightStick),
        "leftshoulder" => Target::Btn(LB), "rightshoulder" => Target::Btn(RB),
        "dpup" => Target::Btn(Up), "dpdown" => Target::Btn(Down),
        "dpleft" => Target::Btn(Left), "dpright" => Target::Btn(Right),
        "misc1" => Target::Btn(Misc1),
        "leftx" => Target::Ax(Ax::LX), "lefty" => Target::Ax(Ax::LY),
        "rightx" => Target::Ax(Ax::RX), "righty" => Target::Ax(Ax::RY),
        "lefttrigger" => Target::Ax(Ax::LT), "righttrigger" => Target::Ax(Ax::RT),
        _ => return None,
    })
}

/// SDL's GUID for a Linux evdev device (bus, vendor, product, version; little endian).
pub fn sdl_guid(dev: &RawDevice) -> String {
    let id = dev.input_id();
    let le = |v: u16| format!("{:02x}{:02x}", v & 0xff, v >> 8);
    format!("{}0000{}0000{}0000{}0000",
        le(id.bus_type().0), le(id.vendor()), le(id.product()), le(id.version()))
}

/// Nintendo pads on hid-nintendo / joycond (Joy-Cons, a Joy-Con pair, the
/// Pro and SNES controllers): the kernel and SDL's database name their
/// buttons by position (B, at the bottom, is Xbox's A), but players go by the
/// letters printed on them, like on the Switch.
pub fn nintendo_letters(vendor: u16, product: u16) -> bool {
    vendor == 0x057e && matches!(product, 0x2006 | 0x2007 | 0x2008 | 0x2009 | 0x200e | 0x2017)
}

/// A pad emulated from user space through uhid, e.g. Sunshine's
/// "X-Box Series Controller" for Moonlight players: it borrows a real pad's
/// IDs (Bluetooth 045e:0b13) but not its HID layout, so the database entry
/// for the real pad reads the wrong axes (Sunshine's left trigger became
/// the right stick, held fully left). The kernel codes are right for it.
fn emulated(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| std::fs::canonicalize(Path::new("/sys/class/input").join(n)).ok())
        .is_some_and(|p| p.to_string_lossy().contains("/misc/uhid/"))
}

impl Mapping {
    /// `path`: the device node (/dev/input/eventN).
    pub fn for_device(dev: &RawDevice, path: &Path) -> Mapping {
        let guid = sdl_guid(dev);
        // exact match first, then the same pad with any version (SDL does the same)
        let line = if emulated(path) { None } else {
            DB.lines().find(|l| l.starts_with(&guid)).or_else(|| {
                DB.lines().find(|l| l.len() > 32 && l[..20] == guid[..20] && l[24..28] == guid[24..28])
            })
        };
        let mut m = match line {
            Some(l) => Self::parse(dev, l),
            None => Self::kernel(dev),
        };
        let id = dev.input_id();
        if nintendo_letters(id.vendor(), id.product()) {
            use Btn::*;
            for (_, t) in &mut m.binds {
                if let Target::Btn(b) = t {
                    *b = match *b { A => B, B => A, X => Y, Y => X, o => o };
                }
            }
        }
        m
    }

    /// SDL numbers buttons in key-code order starting at BTN_JOYSTICK, then the
    /// codes below it; axes in code order skipping the hats.
    fn parse(dev: &RawDevice, line: &str) -> Mapping {
        let mut keys: Vec<u16> = dev.supported_keys().map(|k| k.iter().map(|k| k.0).collect()).unwrap_or_default();
        keys.sort_by_key(|&k| (k < BTN_JOYSTICK, k));
        let axes: Vec<u16> = dev.supported_absolute_axes()
            .map(|a| a.iter().map(|a| a.0).filter(|&c| !(Abs::ABS_HAT0X.0..=Abs::ABS_HAT3Y.0).contains(&c)).collect())
            .unwrap_or_default();

        let mut fields = line.split(',');
        fields.next();
        let name = fields.next().unwrap_or("").to_string();
        let mut binds = Vec::new();
        for f in fields {
            let Some((t, s)) = f.split_once(':') else { continue };
            let Some(t) = target(t) else { continue };
            let src = if let Some(n) = s.strip_prefix('b') {
                n.parse::<usize>().ok().and_then(|i| keys.get(i)).map(|&c| Source::Key(c))
            } else if let Some(h) = s.strip_prefix('h') {
                h.split_once('.').and_then(|(h, m)| Some(Source::Hat(h.parse().ok()?, m.parse().ok()?)))
            } else {
                let (half, rest) = match s.as_bytes().first() {
                    Some(b'+') => (1, &s[1..]),
                    Some(b'-') => (-1, &s[1..]),
                    _ => (0, s),
                };
                let (rest, inv) = match rest.strip_suffix('~') { Some(r) => (r, true), None => (rest, false) };
                rest.strip_prefix('a').and_then(|n| n.parse::<usize>().ok())
                    .and_then(|i| axes.get(i)).map(|&c| Source::Abs(c, half, inv))
            };
            if let Some(src) = src { binds.push((src, t)); }
        }
        Mapping { name, from_db: true, binds }
    }

    /// No database entry: trust the kernel's gamepad codes (xpad and friends).
    fn kernel(dev: &RawDevice) -> Mapping {
        use Btn::*;
        let mut binds: Vec<(Source, Target)> = [
            (Key::BTN_SOUTH, A), (Key::BTN_EAST, B), (Key::BTN_NORTH, X), (Key::BTN_WEST, Y),
            (Key::BTN_SELECT, Back), (Key::BTN_MODE, Guide), (Key::BTN_START, Start),
            (Key::BTN_THUMBL, LeftStick), (Key::BTN_THUMBR, RightStick),
            (Key::BTN_TL, LB), (Key::BTN_TR, RB),
            (Key::BTN_DPAD_UP, Up), (Key::BTN_DPAD_DOWN, Down),
            (Key::BTN_DPAD_LEFT, Left), (Key::BTN_DPAD_RIGHT, Right),
            (Key::KEY_RECORD, Misc1), // Share on Xbox Series pads
        ].into_iter().map(|(k, b)| (Source::Key(k.0), Target::Btn(b))).collect();
        binds.extend([
            (Source::Key(Key::BTN_TL2.0), Target::Ax(Ax::LT)),
            (Source::Key(Key::BTN_TR2.0), Target::Ax(Ax::RT)),
            (Source::Abs(Abs::ABS_X.0, 0, false), Target::Ax(Ax::LX)),
            (Source::Abs(Abs::ABS_Y.0, 0, false), Target::Ax(Ax::LY)),
            (Source::Abs(Abs::ABS_RX.0, 0, false), Target::Ax(Ax::RX)),
            (Source::Abs(Abs::ABS_RY.0, 0, false), Target::Ax(Ax::RY)),
            (Source::Abs(Abs::ABS_Z.0, 0, false), Target::Ax(Ax::LT)),
            (Source::Abs(Abs::ABS_RZ.0, 0, false), Target::Ax(Ax::RT)),
            (Source::Hat(0, 1), Target::Btn(Up)), (Source::Hat(0, 4), Target::Btn(Down)),
            (Source::Hat(0, 8), Target::Btn(Left)), (Source::Hat(0, 2), Target::Btn(Right)),
        ]);
        Mapping { name: dev.name().unwrap_or("pad").to_string(), from_db: false, binds }
    }

    /// It has a Home / Guide button: a bind to Guide whose key the device really
    /// has (the kernel mapping lists BTN_MODE for every pad).
    pub fn has_guide(&self, dev: &RawDevice) -> bool {
        let keys = dev.supported_keys();
        self.binds.iter().any(|(s, t)| {
            matches!(t, Target::Btn(Btn::Guide))
                && match s {
                    Source::Key(k) => keys.is_some_and(|ks| ks.contains(Key(*k))),
                    _ => true,
                }
        })
    }

    pub fn state(&self, raw: &Raw) -> State {
        let mut s = State::default();
        for &(src, t) in &self.binds {
            // value in -1..1 (whole axis), 0..1 (half axis, button, hat)
            let (v, whole) = match src {
                Source::Key(c) => (if raw.keys.get(&c).copied().unwrap_or(false) { 1.0 } else { 0.0 }, false),
                Source::Hat(h, m) => {
                    let x = raw.abs(Abs::ABS_HAT0X.0 + h * 2).map_or(0.0, |(v, _, _)| v as f32);
                    let y = raw.abs(Abs::ABS_HAT0Y.0 + h * 2).map_or(0.0, |(v, _, _)| v as f32);
                    let on = match m { 1 => y < 0.0, 2 => x > 0.0, 4 => y > 0.0, 8 => x < 0.0, _ => false };
                    (if on { 1.0 } else { 0.0 }, false)
                }
                Source::Abs(c, half, inv) => {
                    let Some((v, min, max)) = raw.abs(c) else { continue };
                    let mut n = if max > min { (v - min) as f32 / (max - min) as f32 * 2.0 - 1.0 } else { 0.0 };
                    if inv { n = -n; }
                    match half { 1 => (n.max(0.0), false), -1 => ((-n).max(0.0), false), _ => (n, true) }
                }
            };
            match t {
                Target::Btn(b) => s.btn[b as usize] |= v > 0.5,
                Target::Ax(a @ (Ax::LT | Ax::RT)) => {
                    // a whole axis used as a trigger rests at -1
                    let v = if whole { (v + 1.0) / 2.0 } else { v };
                    s.ax[a as usize] = s.ax[a as usize].max(v);
                }
                Target::Ax(a) => s.ax[a as usize] = v,
            }
        }
        s
    }
}

/// Raw evdev state of one pad.
#[derive(Default)]
pub struct Raw {
    pub keys: HashMap<u16, bool>,
    /// code -> (value, min, max)
    pub absv: HashMap<u16, (i32, i32, i32)>,
}

impl Raw {
    pub fn new(dev: &RawDevice) -> Raw {
        let mut r = Raw::default();
        if let (Some(axes), Ok(info)) = (dev.supported_absolute_axes(), dev.get_abs_state()) {
            for a in axes.iter() {
                let i = &info[a.0 as usize];
                r.absv.insert(a.0, (i.value, i.minimum, i.maximum));
            }
        }
        r
    }
    fn abs(&self, code: u16) -> Option<(i32, i32, i32)> { self.absv.get(&code).copied() }
    pub fn set_abs(&mut self, code: u16, v: i32) {
        if let Some(e) = self.absv.get_mut(&code) { e.0 = v; }
    }
}
