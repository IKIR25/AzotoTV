//! Switch games through Eden (`"type": "switch"`, `exe` = the game's .nsp/.xci).
//!
//! Eden is `$AZOTOTV_EDEN` or `~/Eden/Eden.appimage`; its data (keys, firmware,
//! saves, profiles) stays where the desktop Eden keeps it (`~/.local/share/eden`,
//! or `$AZOTOTV_EDEN_DATA` for tests), so both see the same saves.
//!
//! Each AzotoTV player plays as their own Switch profile, like on a console:
//! the profile is remembered in `players/<id>/switch.json`; the first time, a
//! profile with the player's name is used if there is one, else it is created.
//!
//! The config is the desktop one, copied before every launch into AzotoTV's
//! own folder and patched there (players 1-4 = azotopad's virtual Xbox pads,
//! with the motion of their Joy-Cons / Pro Controller from azotopad's DSU
//! server, no profile picker, no "are you sure" on close, no update check),
//! so the desktop Eden's settings are never touched.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};

use crate::games::{data_dir, home};

pub fn appimage() -> PathBuf {
    std::env::var_os("AZOTOTV_EDEN").map(PathBuf::from).unwrap_or_else(|| home().join("Eden/Eden.appimage"))
}

pub(crate) fn eden_data() -> PathBuf {
    std::env::var_os("AZOTOTV_EDEN_DATA").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share/eden"))
}

/// XDG_CONFIG_HOME for Eden runs started by AzotoTV.
fn config_home() -> PathBuf {
    data_dir().join("eden-config")
}

// ---- profiles (nand/.../profiles.dat, Eden's ProfileDataRaw) ---------------------

const PROFILES: &str = "nand/system/save/8000000000000010/su/avators/profiles.dat";
const HEADER: usize = 0x10;
const ENTRY: usize = 0xC8;
const SLOTS: usize = 8;
const NAME_AT: usize = 0x28;
const NAME_LEN: usize = 0x20;
const FILE_LEN: usize = HEADER + ENTRY * SLOTS;

#[derive(Serialize, Deserialize)]
struct Link {
    /// the profile's UUID, as stored in profiles.dat (hex)
    uuid: String,
}

fn slot(raw: &[u8], i: usize) -> &[u8] {
    &raw[HEADER + i * ENTRY..HEADER + (i + 1) * ENTRY]
}

fn uuid_hex(entry: &[u8]) -> String {
    entry[..16].iter().map(|b| format!("{b:02x}")).collect()
}

fn name_of(entry: &[u8]) -> String {
    let n = &entry[NAME_AT..NAME_AT + NAME_LEN];
    String::from_utf8_lossy(&n[..n.iter().position(|&b| b == 0).unwrap_or(NAME_LEN)]).into_owned()
}

fn used(entry: &[u8]) -> bool {
    entry[..16].iter().any(|&b| b != 0)
}

/// Eden lists the profiles in file order, skipping empty slots; `-u` takes
/// that position.
fn index_of(raw: &[u8], uuid: &str) -> Option<usize> {
    (0..SLOTS).map(|i| slot(raw, i)).filter(|e| used(e)).position(|e| uuid_hex(e) == uuid)
}

/// Switch user names are at most 10 characters.
fn switch_name(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for c in name.chars().take(10) {
        let mut b = [0u8; 4];
        let s = c.encode_utf8(&mut b);
        if out.len() + s.len() > NAME_LEN {
            break;
        }
        out.extend_from_slice(s.as_bytes());
    }
    out
}

fn random_uuid() -> Result<[u8; 16], String> {
    let mut u = [0u8; 16];
    use std::io::Read;
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut u)).map_err(|e| e.to_string())?;
    u[6] = (u[6] & 0x0f) | 0x40;
    u[8] = (u[8] & 0x3f) | 0x80;
    Ok(u)
}

/// The Eden profile index for this player, creating the profile if needed.
/// Only call while Eden isn't running (it rewrites the file on exit).
fn profile_index(player: &Path, user_name: &str) -> Result<usize, String> {
    let path = eden_data().join(PROFILES);
    let mut raw = std::fs::read(&path).unwrap_or_default();
    if raw.len() != FILE_LEN {
        if !raw.is_empty() {
            return Err(format!("{}: unexpected format", path.display()));
        }
        raw = vec![0; FILE_LEN];
    }
    let link_path = player.join("switch.json");
    let linked: Option<Link> = std::fs::read_to_string(&link_path).ok().and_then(|t| serde_json::from_str(&t).ok());
    if let Some(i) = linked.as_ref().and_then(|l| index_of(&raw, &l.uuid)) {
        return Ok(i);
    }

    let wanted = String::from_utf8_lossy(&switch_name(user_name)).to_lowercase();
    let uuid = match (0..SLOTS).map(|i| slot(&raw, i)).find(|e| used(e) && name_of(e).to_lowercase() == wanted) {
        Some(e) => uuid_hex(e),
        None => {
            let free = (0..SLOTS).find(|&i| !used(slot(&raw, i))).ok_or("profili Switch esauriti (massimo 8)")?;
            let u = random_uuid()?;
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let e = &mut raw[HEADER + free * ENTRY..HEADER + (free + 1) * ENTRY];
            e.fill(0);
            e[..16].copy_from_slice(&u);
            e[16..32].copy_from_slice(&u);
            e[32..40].copy_from_slice(&now.to_le_bytes());
            let n = switch_name(user_name);
            e[NAME_AT..NAME_AT + n.len()].copy_from_slice(&n);
            std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            let tmp = path.with_extension("dat.azototv");
            std::fs::write(&tmp, &raw).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| e.to_string())?;
            eprintln!("[eden] created Switch profile \"{user_name}\"");
            uuid_hex(&raw[HEADER + free * ENTRY..])
        }
    };
    let link = serde_json::to_string(&Link { uuid: uuid.clone() }).map_err(|e| e.to_string())?;
    std::fs::write(&link_path, link).map_err(|e| e.to_string())?;
    index_of(&raw, &uuid).ok_or_else(|| t!("Switch profile not found"))
}

/// Makes sure the player has their Switch profile (see `profile_index`).
pub(crate) fn profile_for(player: &Path, user_name: &str) -> Result<(), String> {
    profile_index(player, user_name).map(|_| ())
}

/// Where Eden keeps this player's game saves (`<title id>/` inside), if
/// they have a Switch profile: the folder is named after the profile's UUID
/// as two little-endian u64 halves, high one first, in hex.
pub(crate) fn save_dir(player: &Path) -> Option<PathBuf> {
    let link: Link = serde_json::from_str(&std::fs::read_to_string(player.join("switch.json")).ok()?).ok()?;
    let b: Vec<u8> = (0..link.uuid.len() / 2).filter_map(|i| u8::from_str_radix(link.uuid.get(2 * i..2 * i + 2)?, 16).ok()).collect();
    if b.len() != 16 {
        return None;
    }
    let lo = u64::from_le_bytes(b[..8].try_into().ok()?);
    let hi = u64::from_le_bytes(b[8..].try_into().ok()?);
    Some(eden_data().join("nand/user/save/0000000000000000").join(format!("{hi:016X}{lo:016X}")))
}

// ---- config ----------------------------------------------------------------------

/// azotopad's virtual Xbox 360 pad as Eden names it (SDL GUID, name CRC cleared).
const PAD: &str = "030000005e0400008e02000014010000";

/// Switch players AzotoTV maps (azotopad makes up to 4 virtual pads).
const PLAYERS: usize = 4;

/// azotopad's DSU server (127.0.0.1) as Eden names it: the host's IPv4 in a UUID.
const DSU_GUID: &str = "0000000000000000000000007f000001";

/// Player `p` (from 0) on azotopad's virtual pad `p`, by label (A = A): the
/// desktop mapping, with the virtual pad's GUID. (SDL numbers pads with the
/// same GUID as ports, in the order it finds them.)
fn pad_keys(p: usize) -> Vec<(String, String)> {
    let b = |n: u32| format!("\"engine:sdl,guid:{PAD},port:{p},pad:0,button:{n}\"");
    let hat = |d: &str| format!("\"engine:sdl,guid:{PAD},hat:0,port:{p},pad:0,direction:{d}\"");
    let trig = |a: u32| format!("\"engine:sdl,guid:{PAD},threshold:0.500000,port:{p},pad:0,axis:{a}\"");
    // the player's controller motion: DSU slot = player (both entries, as a
    // Pro Controller only reads the left one)
    let motion = format!("\"engine:cemuhookudp,guid:{DSU_GUID},port:{},pad:{p},motion:0\"", azotopad::motion::PORT);
    let stick = |x: u32, y: u32| {
        format!("\"engine:sdl,guid:{PAD},axis_x:{x},offset_x:-0.000000,invert_y:+,deadzone:0.150000,port:{p},axis_y:{y},offset_y:0.000000,invert_x:+\"")
    };
    [
        ("type", "0".to_string()),
        ("button_a", b(0)),
        ("button_b", b(1)),
        ("button_x", b(2)),
        ("button_y", b(3)),
        ("button_l", b(4)),
        ("button_r", b(5)),
        ("button_minus", b(6)),
        ("button_plus", b(7)),
        ("button_home", b(8)),
        ("button_lstick", b(9)),
        ("button_rstick", b(10)),
        ("button_slleft", b(4)),
        ("button_srleft", b(5)),
        ("button_slright", b(4)),
        ("button_srright", b(5)),
        ("button_zl", trig(2)),
        ("button_zr", trig(5)),
        ("button_dup", hat("up")),
        ("button_ddown", hat("down")),
        ("button_dleft", hat("left")),
        ("button_dright", hat("right")),
        ("lstick", stick(0, 1)),
        ("rstick", stick(3, 4)),
        ("motionleft", motion.clone()),
        ("motionright", motion),
    ]
    .into_iter()
    .map(|(k, v)| (format!("player_{p}_{k}"), v))
    .collect()
}

/// Sets `key=value` (and `key\default=false`) in `[group]` of a Qt ini file.
fn ini_set(lines: &mut Vec<String>, group: &str, key: &str, value: &str) {
    let header = format!("[{group}]");
    let start = match lines.iter().position(|l| l.trim() == header) {
        Some(i) => i,
        None => {
            lines.push(header);
            lines.len() - 1
        }
    };
    let mut end = lines[start + 1..].iter().position(|l| l.starts_with('[')).map_or(lines.len(), |i| start + 1 + i);
    for (k, v) in [(format!("{key}\\default"), "false".to_string()), (key.to_string(), value.to_string())] {
        let prefix = format!("{k}=");
        match lines[start + 1..end].iter().position(|l| l.starts_with(&prefix)) {
            Some(i) => lines[start + 1 + i] = format!("{prefix}{v}"),
            None => {
                lines.insert(end, format!("{prefix}{v}"));
                end += 1;
            }
        }
    }
}

/// Adds `dir` to Eden's external content folders (updates, DLC), unless
/// it's there (`[UI]` `Paths\external_content_dirs\<n>\path`).
fn add_external_dir(lines: &mut Vec<String>, dir: &str) {
    let get = |lines: &[String], k: &str| lines.iter().find_map(|l| l.strip_prefix(&format!("Paths\\{k}=")).map(str::to_string));
    let n: usize = get(lines, "external_content_dirs\\size").and_then(|v| v.parse().ok()).unwrap_or(0);
    if (1..=n).any(|i| get(lines, &format!("external_content_dirs\\{i}\\path")).as_deref() == Some(dir)) {
        return;
    }
    ini_set(lines, "UI", &format!("Paths\\external_content_dirs\\{}\\path", n + 1), dir);
    ini_set(lines, "UI", "Paths\\external_content_dirs\\size", &(n + 1).to_string());
}

fn prepare_config() -> Result<PathBuf, String> {
    let dir = config_home().join("eden");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let desktop = home().join(".config/eden");
    if desktop.join("qt-config.ini").exists() {
        std::fs::copy(desktop.join("qt-config.ini"), dir.join("qt-config.ini")).map_err(|e| e.to_string())?;
        let custom = desktop.join("custom");
        if custom.is_dir() {
            let _ = std::fs::remove_dir_all(dir.join("custom"));
            let st = Command::new("cp").arg("-a").arg(&custom).arg(dir.join("custom")).status();
            if !st.is_ok_and(|s| s.success()) {
                return Err(t!("couldn't copy Eden's configuration"));
            }
        }
    }
    let ini = dir.join("qt-config.ini");
    let mut lines: Vec<String> = std::fs::read_to_string(&ini).unwrap_or_default().lines().map(String::from).collect();
    for (k, v) in [
        ("fullscreen", "true"),
        ("select_user_on_boot", "false"),
        ("confirmStop", "2"), // Ask_Never: closing (Home held) must not wait for a dialog
        ("check_for_updates", "false"),
        ("pauseWhenInBackground", "false"),
        ("hideInactiveMouse", "true"),
        ("firstStart", "false"),
    ] {
        ini_set(&mut lines, "UI", k, v);
    }
    // updates and DLC brought by another AzotoTV (peer.rs) land in ~/Eden/Extra
    let extra = home().join("Eden/Extra");
    if extra.is_dir() {
        add_external_dir(&mut lines, &extra.to_string_lossy());
    }
    ini_set(&mut lines, "Controls", "motion_enabled", "true");
    ini_set(&mut lines, "Controls", "udp_input_servers", &format!("127.0.0.1:{}", azotopad::motion::PORT));
    for p in 0..PLAYERS {
        for (k, v) in pad_keys(p) {
            ini_set(&mut lines, "Controls", &k, &v);
        }
    }
    std::fs::write(&ini, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    Ok(config_home())
}

/// Connects the first `n` players (one per pad switched on) and disconnects
/// the rest, so a game doesn't see controllers nobody holds. Call after
/// `command`, before starting Eden.
pub fn connect_players(n: usize) -> Result<(), String> {
    let ini = config_home().join("eden/qt-config.ini");
    let mut lines: Vec<String> = std::fs::read_to_string(&ini).map_err(|e| e.to_string())?.lines().map(String::from).collect();
    for p in 0..PLAYERS {
        ini_set(&mut lines, "Controls", &format!("player_{p}_connected"), if p < n.max(1) { "true" } else { "false" });
    }
    std::fs::write(&ini, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

// ---- system applets (Tools > Launch Applet) ---------------------------------------

/// A system applet Eden can open (only from its menu; see `games::open_applet`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applet {
    /// the Mii editor: the Switch's Miis, shared by every profile like on
    /// the console
    Mii,
}

/// Keys for Eden's menu: some pressed together, some one after the other.
pub enum Keys {
    Combo(Vec<azotopad::KeyCode>),
    Tap(Vec<azotopad::KeyCode>),
}

impl Applet {
    pub fn parse(s: &str) -> Result<Applet, String> {
        match s {
            "mii" => Ok(Applet::Mii),
            _ => Err(format!("unknown applet: {s}")),
        }
    }

    /// The program it runs (as Eden's log names it).
    fn title_id(self) -> &'static str {
        match self {
            Applet::Mii => "0100000000001009",
        }
    }

    /// Alt+T (Tools), A (Launch Applet), then the applet's own letter.
    pub fn menu_keys(self) -> Vec<Keys> {
        use azotopad::KeyCode as Key;
        let last = match self {
            Applet::Mii => Key::KEY_M, // "&Mii Editor"
        };
        vec![Keys::Combo(vec![Key::KEY_LEFTALT, Key::KEY_T]), Keys::Tap(vec![Key::KEY_A]), Keys::Tap(vec![last])]
    }
}

/// This run's log (Eden rewrites it at every start).
fn log() -> String {
    std::fs::read_to_string(eden_data().join("log/eden_log.txt")).unwrap_or_default()
}

/// Whether Eden has loaded `applet`.
pub fn applet_running(applet: Applet) -> bool {
    log().contains(&format!("title_id={}", applet.title_id()))
}

/// Whether the emulation has ended (the applet closed, or Stop).
pub fn emulation_stopped() -> bool {
    let l = log();
    l.contains("Force stopping EmuThread") || l.contains("ShutdownMainProcess")
}

/// Eden without a game, as this player's profile: `games::open_applet` then
/// picks the applet from the menu.
pub fn applet_command(player: &Path, user_name: &str) -> Result<Command, String> {
    let eden = appimage();
    if !eden.exists() {
        return Err(t!("Eden not found: {a}", a = eden.display()));
    }
    let user = profile_index(player, user_name)?;
    let config = prepare_config()?;
    // a log from an earlier run must not look like the applet already ran
    let _ = std::fs::remove_file(eden_data().join("log/eden_log.txt"));
    let mut c = Command::new(eden);
    c.arg("-u").arg(user.to_string());
    c.env("XDG_CONFIG_HOME", config).env("DISABLE_AUTO_UPDATES", "1");
    if let Some(d) = std::env::var_os("AZOTOTV_EDEN_DATA") {
        if let Some(parent) = Path::new(&d).parent() {
            c.env("XDG_DATA_HOME", parent);
        }
    }
    Ok(c)
}

/// The command that boots `game` fullscreen as this player's Switch profile.
pub fn command(player: &Path, user_name: &str, game: &Path) -> Result<Command, String> {
    let eden = appimage();
    if !eden.exists() {
        return Err(t!("Eden not found: {a}", a = eden.display()));
    }
    let user = profile_index(player, user_name)?;
    let config = prepare_config()?;
    let mut c = Command::new(eden);
    c.arg("-f").arg("-u").arg(user.to_string()).arg("-g").arg(game);
    c.env("XDG_CONFIG_HOME", config)
        // the AppImage's own updater asks with a dialog at every start
        .env("DISABLE_AUTO_UPDATES", "1");
    if let Some(d) = std::env::var_os("AZOTOTV_EDEN_DATA") {
        if let Some(parent) = Path::new(&d).parent() {
            c.env("XDG_DATA_HOME", parent);
        }
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_folder_added_once() {
        let mut l: Vec<String> = ["[UI]", "Paths\\external_content_dirs\\1\\path=/a", "Paths\\external_content_dirs\\size=1"].map(String::from).to_vec();
        add_external_dir(&mut l, "/a");
        assert_eq!(l.len(), 3);
        add_external_dir(&mut l, "/b");
        assert!(l.contains(&"Paths\\external_content_dirs\\2\\path=/b".to_string()));
        assert!(l.contains(&"Paths\\external_content_dirs\\size=2".to_string()));
        let mut empty = Vec::new();
        add_external_dir(&mut empty, "/b");
        assert!(empty.contains(&"Paths\\external_content_dirs\\1\\path=/b".to_string()));
    }

    #[test]
    fn ini_set_replaces_and_inserts() {
        let mut l: Vec<String> =
            ["[UI]", "confirmStop\\default=true", "confirmStop=1", "[System]", "x=1"].iter().map(|s| s.to_string()).collect();
        ini_set(&mut l, "UI", "confirmStop", "2");
        ini_set(&mut l, "UI", "fullscreen", "true");
        ini_set(&mut l, "Controls", "player_0_type", "0");
        assert_eq!(
            l,
            [
                "[UI]",
                "confirmStop\\default=false",
                "confirmStop=2",
                "fullscreen\\default=false",
                "fullscreen=true",
                "[System]",
                "x=1",
                "[Controls]",
                "player_0_type\\default=false",
                "player_0_type=0"
            ]
        );
    }

    #[test]
    fn each_player_uses_its_own_virtual_pad() {
        let keys = pad_keys(2);
        assert!(keys.iter().all(|(k, _)| k.starts_with("player_2_")));
        let a = &keys.iter().find(|(k, _)| k == "player_2_button_a").unwrap().1;
        assert_eq!(a, &format!("\"engine:sdl,guid:{PAD},port:2,pad:0,button:0\""));
        let m = &keys.iter().find(|(k, _)| k == "player_2_motionleft").unwrap().1;
        assert_eq!(m, "\"engine:cemuhookudp,guid:0000000000000000000000007f000001,port:26760,pad:2,motion:0\"");
    }

    #[test]
    fn names_are_cut_to_ten_chars() {
        assert_eq!(switch_name("PLAYER1"), b"PLAYER1");
        assert_eq!(switch_name("Giovanni Bianchi"), b"Giovanni B");
        assert_eq!(switch_name("èèèèèèèèèèèè").len(), 20);
    }

    #[test]
    fn index_skips_empty_slots() {
        let mut raw = vec![0u8; FILE_LEN];
        for (i, b) in [(0usize, 1u8), (2, 3)] {
            raw[HEADER + i * ENTRY] = b;
        }
        let u3 = uuid_hex(slot(&raw, 2));
        assert_eq!(index_of(&raw, &u3), Some(1));
    }
}
