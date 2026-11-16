//! Wii games through Dolphin (`"type": "wii"`, `exe` = the disc: .wbfs/.rvz/.iso).
//!
//! Dolphin is the flatpak `org.DolphinEmu.dolphin-emu` (or `$AZOTOTV_DOLPHIN`,
//! a dolphin-emu binary). AzotoTV gives it its own user folder,
//! `~/.local/share/azototv/dolphin`, shared by every player: texture packs in
//! `Load/Textures/<game id>/` (e.g. RMG = Super Mario Galaxy), per-game
//! settings and patches in `GameSettings/<game id>.ini`, shader caches.
//! The desktop Dolphin's settings are never touched.
//!
//! Each player has their own Wii (NAND: saves, Miis) in `players/<id>/wii`,
//! set up in Italian, 16:9, 60 Hz.
//!
//! `Config/Dolphin.ini`, `GFX.ini` and `WiimoteNew.ini` are rewritten at every
//! launch: players 1-4 = azotopad's virtual Xbox pads, each as a Wii Remote +
//! Nunchuk laid out like the Switch versions of the Galaxy games (see
//! `wiimote`; Dolphin names the vertical axes like XInput: `Y+` is up).
//! With Joy-Cons the motion is real too (azotopad's DSU server, `DSUClient.ini`):
//! the right Joy-Con (or a single one) is the Wii Remote, pointing and
//! shaking, the left one of a pair the Nunchuk; R recentres the pointer. Pads without motion keep
//! the stick and buttons (Dolphin ignores a DSU slot with nothing in it).
//!
//! Games played with the Wii Remote alone, held sideways (Mario Party 9),
//! say `"wiimote": "sideways"` in game.json: see `Layout::Sideways`. Games
//! that also take a Classic Controller (Mario Kart Wii) say `"classic"`:
//! the pad then IS one, see `Layout::Classic`.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::games::data_dir;

const FLATPAK: &str = "org.DolphinEmu.dolphin-emu";

fn user_dir() -> PathBuf {
    data_dir().join("dolphin")
}

/// How a game holds the Wii Remote (game.json `"wiimote"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Layout {
    /// upright, with the Nunchuk (the Galaxy games): the default
    #[default]
    Nunchuk,
    /// alone, sideways like a NES pad (Mario Party 9): no Nunchuk
    Sideways,
    /// a Classic Controller (Mario Kart Wii): the Xbox pad as it is
    Classic,
}

impl Layout {
    pub fn from_manifest(v: Option<&str>) -> Layout {
        match v {
            Some("sideways") => Layout::Sideways,
            Some("classic") => Layout::Classic,
            _ => Layout::Nunchuk,
        }
    }
}

/// Wii Remotes AzotoTV maps (azotopad makes up to 4 virtual pads).
const PLAYERS: usize = 4;

const DOLPHIN_INI: &str = "\
[Analytics]
PermissionAsked = True
Enabled = False
[Core]
GFXBackend = Vulkan
CPUThread = True
DSPHLE = True
EnableCheats = True
WiimoteContinuousScanning = False
WiimoteControllerInterface = False
[Display]
Fullscreen = True
RenderToMain = False
[General]
ISOPaths = 0
[Input]
BackgroundInput = True
[Interface]
ConfirmStop = False
PauseOnFocusLost = False
CursorVisibility = 0
LanguageCode = it
OnScreenDisplayMessages = False
UsePanicHandlers = False
";

/// 3x the Wii's resolution (about 1440p, scaled down to the TV: sharper than
/// 2x, still easy for the GTX 1060), HD texture packs preloaded into RAM so
/// they don't stutter, shaders compiled before the game starts.
const GFX_INI: &str = "\
[Settings]
InternalResolution = 3
Crop = True
HiresTextures = True
CacheHiresTextures = True
ShaderCompilationMode = 2
WaitForShadersBeforeStarting = True
[Enhancements]
MaxAnisotropy = 4
ArbitraryMipmapDetection = True
[Hardware]
VSync = True
";

/// The Wii itself: Italian, 16:9, progressive scan, PAL games at 60 Hz.
const SYSCONF: [&str; 4] = ["SYSCONF.IPL.LNG=5", "SYSCONF.IPL.AR=1", "SYSCONF.IPL.PGS=1", "SYSCONF.IPL.E60=1"];

/// azotopad's two DSU servers, by the names Dolphin's devices get
/// (`DSUClient/<player>/<name>`).
const DSU_MAIN: &str = "AzotoTV";
const DSU_LEFT: &str = "AzotoTV Nunchuk";

fn dsu_ini() -> String {
    use azotopad::motion::{LEFT_PORT, PORT};
    format!("[Server]\nEnabled = True\nEntries = {DSU_MAIN}:127.0.0.1:{PORT};{DSU_LEFT}:127.0.0.1:{LEFT_PORT};\n")
}

/// `<group>/<direction>` = the DSU input of the same name, for the
/// accelerometer (and gyroscope) of player `p`.
fn motion(prefix: &str, p: usize, server: &str, gyro: bool) -> String {
    let dev = format!("DSUClient/{p}/{server}");
    let mut s: String = ["Up", "Down", "Left", "Right", "Forward", "Backward"]
        .iter()
        .map(|d| format!("{prefix}IMUAccelerometer/{d} = `{dev}:Accel {d}`\n"))
        .collect();
    if gyro {
        for d in ["Pitch Up", "Pitch Down", "Roll Left", "Roll Right", "Yaw Left", "Yaw Right"] {
            s += &format!("{prefix}IMUGyroscope/{d} = `{dev}:Gyro {d}`\n");
        }
    }
    s
}

/// Wii Remote `p` (from 0) on azotopad's virtual pad `p`. SDL numbers pads
/// with the same name in the order it finds them.
///
/// Laid out like the Switch versions of the Galaxy games: A/B jump, X/Y
/// (or shaking) spin, ZL = Z (crouch), ZR = B (Star Bits), L = C (camera
/// back). With `motion` (Joy-Cons, Pro Controller) the pointer and tilt are
/// the controller's own, the right stick is the camera (the Wii's D-pad)
/// and R recentres the pointer; a pad without motion points
/// and tilts with the right stick.
fn wiimote(p: usize, connected: bool, motion: bool, layout: Layout) -> String {
    let head = format!("[Wiimote{}]\nSource = {}\nDevice = SDL/{p}/Xbox 360 Controller\n", p + 1, u8::from(connected));
    match layout {
        Layout::Nunchuk => head + &nunchuk(p, motion),
        Layout::Sideways => head + &sideways(p, motion),
        Layout::Classic => head + CLASSIC,
    }
}

/// See `wiimote`: the Galaxy layout.
fn nunchuk(p: usize, motion: bool) -> String {
    let mut s = String::from(
        "\
Buttons/A = `Button S`|`Button E`
Buttons/B = `Trigger R`
Buttons/1 = `Thumb L`
Buttons/2 = `Thumb R`
Buttons/- = Back
Buttons/+ = Start
IR/Auto-Hide = False
Shake/X = `Button W`|`Button N`
Shake/Y = `Button W`|`Button N`
Shake/Z = `Button W`|`Button N`
Extension = Nunchuk
Nunchuk/Buttons/C = `Shoulder L`
Nunchuk/Buttons/Z = `Trigger L`
Nunchuk/Stick/Up = `Left Y+`
Nunchuk/Stick/Down = `Left Y-`
Nunchuk/Stick/Left = `Left X-`
Nunchuk/Stick/Right = `Left X+`
",
    );
    if motion {
        s += "\
D-Pad/Up = `Pad N`|`Right Y+`
D-Pad/Down = `Pad S`|`Right Y-`
D-Pad/Left = `Pad W`|`Right X-`
D-Pad/Right = `Pad E`|`Right X+`
";
        s += &self::motion("", p, DSU_MAIN, true);
        s += "IMUIR/Recenter = `Shoulder R`\n";
        s += &self::motion("Nunchuk/", p, DSU_LEFT, false);
    } else {
        s += "\
D-Pad/Up = `Pad N`
D-Pad/Down = `Pad S`
D-Pad/Left = `Pad W`
D-Pad/Right = `Pad E`
IR/Up = `Right Y+`
IR/Down = `Right Y-`
IR/Left = `Right X-`
IR/Right = `Right X+`
Tilt/Forward = `Right Y+`
Tilt/Backward = `Right Y-`
Tilt/Left = `Right X-`
Tilt/Right = `Right X+`
";
    }
    s
}

/// The Wii Remote alone, held sideways (Dolphin turns the D-pad and the
/// tilt with it): D-pad or left stick = the cross, A/B (Xbox A/B, the two
/// face buttons on the right… bottom and right) = 2, X/Y = 1, RB = A,
/// RT = B (the upright minigames that point), LB/LT shake. With motion the
/// controller's own pointer and tilt, R3 recentres; without, the right
/// stick points and tilts.
fn sideways(p: usize, motion: bool) -> String {
    let mut s = String::from(
        "\
Options/Sideways Wiimote = True
Extension = None
Buttons/2 = `Button S`|`Button E`
Buttons/1 = `Button W`|`Button N`
Buttons/A = `Shoulder R`
Buttons/B = `Trigger R`
Buttons/- = Back
Buttons/+ = Start
D-Pad/Up = `Pad N`|`Left Y+`
D-Pad/Down = `Pad S`|`Left Y-`
D-Pad/Left = `Pad W`|`Left X-`
D-Pad/Right = `Pad E`|`Left X+`
Shake/X = `Shoulder L`|`Trigger L`
Shake/Y = `Shoulder L`|`Trigger L`
Shake/Z = `Shoulder L`|`Trigger L`
IR/Auto-Hide = False
",
    );
    if motion {
        s += &self::motion("", p, DSU_MAIN, true);
        s += "IMUIR/Recenter = `Thumb R`\n";
    } else {
        s += "\
IR/Up = `Right Y+`
IR/Down = `Right Y-`
IR/Left = `Right X-`
IR/Right = `Right X+`
Tilt/Forward = `Right Y+`
Tilt/Backward = `Right Y-`
Tilt/Left = `Right X-`
Tilt/Right = `Right X+`
";
    }
    s
}

/// A Classic Controller in the Wii Remote: the Xbox pad button for button
/// (A = A, B = B, X/Y by position: Xbox Y on top = X), LT/RT = L/R (also
/// analog), LB/RB = ZL/ZR, Back/Start = -/+. Home stays AzotoTV's.
const CLASSIC: &str = "\
Extension = Classic
Classic/Buttons/A = `Button S`
Classic/Buttons/B = `Button E`
Classic/Buttons/X = `Button N`
Classic/Buttons/Y = `Button W`
Classic/Buttons/ZL = `Shoulder L`
Classic/Buttons/ZR = `Shoulder R`
Classic/Buttons/- = Back
Classic/Buttons/+ = Start
Classic/Left Stick/Up = `Left Y+`
Classic/Left Stick/Down = `Left Y-`
Classic/Left Stick/Left = `Left X-`
Classic/Left Stick/Right = `Left X+`
Classic/Right Stick/Up = `Right Y+`
Classic/Right Stick/Down = `Right Y-`
Classic/Right Stick/Left = `Right X-`
Classic/Right Stick/Right = `Right X+`
Classic/Triggers/L = `Trigger L`
Classic/Triggers/R = `Trigger R`
Classic/Triggers/L-Analog = `Trigger L`
Classic/Triggers/R-Analog = `Trigger R`
Classic/D-Pad/Up = `Pad N`
Classic/D-Pad/Down = `Pad S`
Classic/D-Pad/Left = `Pad W`
Classic/D-Pad/Right = `Pad E`
";

fn write(name: &str, text: &str) -> Result<(), String> {
    let dir = user_dir().join("Config");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(name), text).map_err(|e| format!("{name}: {e}"))
}

/// Connects the first `n` Wii Remotes (one per pad switched on) and
/// disconnects the rest; `motion[p]`: player `p`'s controller has motion.
/// Call after `command`, before starting Dolphin.
pub fn connect_players(n: usize, motion: &[bool], layout: Layout) -> Result<(), String> {
    let ini: String = (0..PLAYERS)
        .map(|p| wiimote(p, p < n.max(1), motion.get(p).copied().unwrap_or(false), layout))
        .collect();
    write("WiimoteNew.ini", &ini)
}

/// The command that boots `game` fullscreen on this player's own Wii.
pub fn command(player: &Path, game: &Path, layout: Layout) -> Result<Command, String> {
    write("Dolphin.ini", DOLPHIN_INI)?;
    write("GFX.ini", GFX_INI)?;
    write("DSUClient.ini", &dsu_ini())?;
    connect_players(1, &[], layout)?;
    let nand = player.join("wii");
    std::fs::create_dir_all(&nand).map_err(|e| e.to_string())?;
    let mut c = match std::env::var_os("AZOTOTV_DOLPHIN") {
        Some(bin) => Command::new(bin),
        None => {
            let mut c = Command::new("flatpak");
            c.args(["run", FLATPAK]);
            c
        }
    };
    c.arg("-u").arg(user_dir()).arg("-b").arg("-e").arg(game);
    c.arg("-C").arg(format!("Dolphin.General.NANDRootPath={}", nand.display()));
    // the same Wii at every launch: Miis keep this console's ID (mii.rs)
    let user = player.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mac = crate::mii::wii_mac(&user).map(|b| format!("{b:02x}")).join(":");
    c.arg("-C").arg(format!("Dolphin.General.WirelessMac={mac}"));
    for s in SYSCONF {
        c.arg("-C").arg(s);
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_wiimote_uses_its_own_virtual_pad() {
        let w = wiimote(2, false, true, Layout::Nunchuk);
        assert!(w.starts_with("[Wiimote3]\nSource = 0\nDevice = SDL/2/Xbox 360 Controller\n"));
        assert!(w.contains("Extension = Nunchuk\n"));
        assert!(w.contains("\nIMUGyroscope/Yaw Left = `DSUClient/2/AzotoTV:Gyro Yaw Left`\n"));
        assert!(w.contains("\nNunchuk/IMUAccelerometer/Up = `DSUClient/2/AzotoTV Nunchuk:Accel Up`\n"));
        assert!(!w.contains("Nunchuk/IMUGyroscope"));
    }

    #[test]
    fn right_stick_is_camera_with_motion_pointer_without() {
        let with = wiimote(0, true, true, Layout::Nunchuk);
        assert!(with.contains("\nD-Pad/Up = `Pad N`|`Right Y+`\n") && !with.contains("\nIR/Up"));
        let without = wiimote(0, true, false, Layout::Nunchuk);
        assert!(without.contains("\nIR/Up = `Right Y+`\n") && !without.contains("IMU"));
    }

    #[test]
    fn sideways_has_no_nunchuk() {
        let w = wiimote(1, true, false, Layout::Sideways);
        assert!(w.starts_with("[Wiimote2]\nSource = 1\n"));
        assert!(w.contains("\nOptions/Sideways Wiimote = True\nExtension = None\n"));
        assert!(w.contains("\nD-Pad/Up = `Pad N`|`Left Y+`\n") && !w.contains("Nunchuk/"));
        let m = wiimote(0, true, true, Layout::Sideways);
        assert!(m.contains("IMUGyroscope/Yaw Left") && !m.contains("AzotoTV Nunchuk") && !m.contains("\nIR/Up"));
        assert_eq!(Layout::from_manifest(Some("sideways")), Layout::Sideways);
        assert_eq!(Layout::from_manifest(None), Layout::Nunchuk);
    }

    #[test]
    fn classic_is_the_pad_as_it_is() {
        let w = wiimote(3, true, true, Layout::Classic);
        assert!(w.starts_with("[Wiimote4]\nSource = 1\nDevice = SDL/3/Xbox 360 Controller\nExtension = Classic\n"));
        assert!(w.contains("\nClassic/Left Stick/Up = `Left Y+`\n") && !w.contains("IMU") && !w.contains("IR/"));
        assert_eq!(Layout::from_manifest(Some("classic")), Layout::Classic);
    }
}
