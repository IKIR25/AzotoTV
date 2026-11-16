//! Controllers' player lights, so each controller shows its player number
//! like on a console: Joy-Cons and Pro Controllers (lights 1..n, like the
//! Switch), DualSense (the PS5 pattern and lightbar colour), DualShock 4
//! (lightbar colour), Xbox 360 pads (the ring quadrant). Pads without
//! lights are left alone.
//!
//! The lights are LED class devices of the pad's HID/USB device in sysfs,
//! root's by default: session/71-azototv-pad-lights.rules lets the seat user
//! write them. A Joy-Con pair (joycond's virtual pad) has no lights of its
//! own (joycond ignores its LED events), so its two Joy-Cons are lit: pairs
//! take the held Joy-Cons in the order they appeared, like their motion.

use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

/// Shows `player` (from 1; None = no player: lights off) on the pad whose
/// event node is `node`.
pub fn show(node: &Path, player: Option<usize>) {
    let Some(event) = node.file_name().map(|n| n.to_string_lossy().into_owned()) else { return };
    let input = Path::new("/sys/class/input").join(&event).join("device");
    let read = |f: &str| std::fs::read_to_string(input.join(f)).unwrap_or_default().trim().to_string();
    if read("id/vendor") == "057e" && read("id/product") == "2008" {
        for joycon in pair_joycons(node) {
            show_sys(&joycon, player);
        }
    } else {
        show_sys(&input, player);
    }
}

/// `input`: /sys/class/input/inputN (or eventN/device).
fn show_sys(input: &Path, player: Option<usize>) {
    let Ok(dir) = std::fs::canonicalize(input.join("device/leds")) else { return };
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    let leds: Vec<(String, PathBuf)> =
        entries.flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.path())).collect();
    let n = player.unwrap_or(0).min(4);
    let set = |name_end: &str, value: &str| {
        for (_, path) in leds.iter().filter(|(name, _)| name.ends_with(name_end)) {
            write(&path.join("brightness"), value);
        }
    };
    let has = |part: &str| leds.iter().any(|(name, _)| name.contains(part));

    if has("green:player-") {
        // Nintendo: the first n lights (the Home ring, blue:player-5, is left alone)
        for i in 1..=4 {
            set(&format!("green:player-{i}"), if i <= n { "1" } else { "0" });
        }
    } else if has("white:player-") {
        // DualSense: the PS5's five-light patterns and lightbar colours
        const PATTERN: [[u8; 5]; 5] =
            [[0, 0, 0, 0, 0], [0, 0, 1, 0, 0], [0, 1, 0, 1, 0], [1, 0, 1, 0, 1], [1, 1, 0, 1, 1]];
        for i in 0..5 {
            set(&format!("white:player-{}", i + 1), &PATTERN[n][i].to_string());
        }
        for (_, path) in leds.iter().filter(|(name, _)| name.ends_with(":rgb:indicator")) {
            let (r, g, b) = colour(n);
            write(&path.join("multi_intensity"), &format!("{r} {g} {b}"));
            write(&path.join("brightness"), "255");
        }
    } else if has(":red") && has(":blue") {
        // DualShock 4: lightbar only
        let (r, g, b) = colour(n);
        set(":red", &r.to_string());
        set(":green", &g.to_string());
        set(":blue", &b.to_string());
    } else if let Some((_, path)) = leds.iter().find(|(name, _)| name.starts_with("xpad")) {
        // Xbox 360 ring: 6..9 = quadrant 1..4 on, 0 = off
        write(&path.join("brightness"), &if n > 0 { (5 + n).to_string() } else { "0".into() });
    }
}

/// PlayStation player colours: blue, red, green, pink (none: dim white).
fn colour(n: usize) -> (u8, u8, u8) {
    match n {
        1 => (0, 0, 255),
        2 => (255, 0, 0),
        3 => (0, 255, 0),
        4 => (255, 0, 255),
        _ => (32, 32, 32),
    }
}

fn write(path: &Path, value: &str) {
    // unchanged values aren't rewritten (a DualSense redraws its lights)
    if std::fs::read_to_string(path).is_ok_and(|v| v.trim() == value) {
        return;
    }
    if let Err(e) = std::fs::write(path, value) {
        if std::env::var_os("AZOTOPAD_DEBUG").is_some() {
            eprintln!("[azotopad] light {}: {e}", path.display());
        }
    }
}

/// The two Joy-Cons (/sys/class/input/inputN) behind joycond's pair at
/// `node`: pairs take held Joy-Cons in the order both appeared.
fn pair_joycons(node: &Path) -> Vec<PathBuf> {
    let ctime = |p: &Path| std::fs::metadata(p).map(|m| (m.ctime(), m.ctime_nsec())).unwrap_or((i64::MAX, 0));
    let mut pairs: Vec<PathBuf> = evdev::raw_stream::enumerate()
        .filter(|(_, d)| (d.input_id().vendor(), d.input_id().product()) == (0x057e, 0x2008))
        .map(|(p, _)| p)
        .collect();
    pairs.sort_by_key(|p| ctime(p));
    let Some(k) = pairs.iter().position(|p| p == node) else { return Vec::new() };
    // held_joycons() gives /sys/devices/.../inputN/eventM; the node's ctime is
    // when the Joy-Con connected
    let mut held: Vec<((i64, i64), String, PathBuf)> = crate::session::held_joycons()
        .into_iter()
        .filter_map(|ev| {
            let name = std::fs::read_to_string(ev.join("device/name")).ok()?;
            let when = ctime(&Path::new("/dev/input").join(ev.file_name()?));
            Some((when, name, ev.join("device")))
        })
        .collect();
    held.sort();
    let side = |s: &str| held.iter().filter(|(_, name, _)| name.starts_with(s)).map(|(_, _, p)| p.clone()).nth(k);
    side("Joy-Con (L)").into_iter().chain(side("Joy-Con (R)")).collect()
}
