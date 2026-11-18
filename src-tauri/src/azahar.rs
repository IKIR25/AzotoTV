//! 3DS games through Azahar (`"type": "3ds"`, `exe` = the game: a decrypted
//! .cci/.3ds/.cxi, e.g. `~/Azahar/Games/Super Mario 3D Land.cci`).
//!
//! Azahar is the flatpak `org.azahar_emu.Azahar`. It has no option for its
//! user folder (and ignores XDG overrides inside the flatpak), so its own
//! `~/.var/app/org.azahar_emu.Azahar` is shared by every player: shader
//! caches, texture packs (`data/azahar-emu/load/textures/<title id>/`),
//! per-game settings. `qt-config.ini` is rewritten at every launch with
//! only what AzotoTV sets; Azahar fills in its defaults for the rest.
//!
//! Each player has their own 3DS: NAND + SD card in `players/<id>/3ds`
//! (Azahar's "custom storage"), so saves, Miis and play coins are theirs.
//! The 3DS's own settings (language, the user's name) live in its NAND's
//! config savefile, which AzotoTV sets to Italian + the profile's name
//! before every launch (a new 3DS gets a copy of `azahar/cfg-template`).
//!
//! The screens: on the TV the top screen, full size; the bottom screen in a
//! window of its own on the laptop panel, which then is a screen of its own
//! (games.rs `split_screens`, kwin.rs places the window), so the phone can
//! show it and touch it (phonescreen.rs). Without that (TV off, or a PC
//! plays through Moonlight) both screens share one: top big, bottom small.
//!
//! Controls: azotopad's virtual Xbox pads, by position like Eden (3DS A =
//! the bottom button), circle pad = left stick, C-stick = right stick,
//! ZL/ZR = the triggers; motion from azotopad's DSU server (Joy-Cons).
//!
//! Notes from the server (2026-11-09): Vulkan on the GTX 1060 crashes
//! Azahar's Wayland window (NVIDIA, "wp_linux_drm_syncobj_surface ... Surface
//! got destroyed"), so it runs as an X11 window (`--socket=x11`, Qt xcb);
//! the flatpak's launcher script shows a "crashed" dialog when the game is
//! killed, so `azahar` is run directly; it ignores SIGTERM (AzotoTV's
//! SIGKILL after 3 s ends it).

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::games::{data_dir, home};

pub(crate) const FLATPAK: &str = "org.azahar_emu.Azahar";

/// Azahar's flatpak is here.
pub fn installed() -> bool {
    crate::games::flatpak_installed(FLATPAK)
}

/// azotopad's virtual Xbox 360 pad as SDL 2.26+ names it: bytes 2-3 are a
/// CRC16 of its name ("Microsoft X-Box 360 pad" -> 81b8). Eden clears them
/// before comparing, Azahar compares the whole string, so here they must be
/// there (with 0000, as in eden.rs, no button reached the game; checked
/// 2026-11-09 with the flatpak's own SDL 2.32 and a copy of the pad).
const PAD: &str = "030081b85e0400008e02000014010000";

fn config_file() -> PathBuf {
    home().join(".var/app").join(FLATPAK).join("config/azahar-emu/qt-config.ini")
}

/// "Large screen" layout: the top screen this many times the bottom one's
/// scale (small_screen_position 2 = the bottom one at its bottom right).
const PROPORTION: u32 = 4;

/// Where the bottom screen is in the 16:9 picture the phone gets of the
/// game's screen (x, y, width, height, 0-1; phone.rs `/api/app/screen`).
/// `separate`: its own window, 4:3 in the middle of the panel; otherwise the
/// small one of the "large screen" layout (a PC playing through Moonlight, or
/// the TV alone): top 400x240 scaled by PROPORTION, then the bottom 320x240
/// at its bottom right, together centred and fitted to the picture.
pub fn bottom_screen(separate: bool) -> [f64; 4] {
    let (pw, ph) = (16.0, 9.0);
    if separate {
        let w = ph * 4.0 / 3.0 / pw;
        return [(1.0 - w) / 2.0, 0.0, w, 1.0];
    }
    let k = PROPORTION as f64;
    let (lw, lh) = (400.0 * k + 320.0, 240.0 * k);
    let s = (pw / lw).min(ph / lh);
    let (x0, y0) = ((pw - lw * s) / 2.0, (ph - lh * s) / 2.0);
    [(x0 + 400.0 * k * s) / pw, (y0 + (lh - 240.0) * s) / ph, 320.0 * s / pw, 240.0 * s / ph]
}

/// The NAND's config savefile (CFG), from a fresh 3DS (Azahar makes one at
/// first boot, in English).
fn cfg_template() -> PathBuf {
    data_dir().join("azahar/cfg-template")
}

/// Where a player's 3DS keeps that file.
fn cfg_file(nand: &Path) -> PathBuf {
    nand.join("data/00000000000000000000000000000000/sysdata/00010017/00000000/config")
}

/// An SDL button of the virtual pad (any of them: one-player games get all
/// pads merged into player 1 by azotopad).
fn button(b: u8) -> String {
    format!("\"api:controller,button:{b},engine:sdl,guid:{PAD},maptype:guid,port:0\"")
}

fn trigger(axis: u8) -> String {
    format!("\"api:controller,axis:{axis},direction:+,engine:sdl,guid:{PAD},maptype:guid,port:0,threshold:0.5\"")
}

fn stick(x: u8, y: u8) -> String {
    format!("\"api:controller,axis_x:{x},axis_y:{y},deadzone:0.150000,engine:sdl,guid:{PAD},maptype:guid,port:0\"")
}

/// `qt-config.ini`. `separate`: the bottom screen in its own window.
fn qt_config(player: &Path, separate: bool) -> String {
    // SDL_GameController numbers: A B X Y = 0-3, Back 4, Start 6, LB 9, RB 10,
    // D-pad 11-14; axes: left 0/1, right 2/3, triggers 4/5
    let controls = [
        ("button_a", button(0)),
        ("button_b", button(1)),
        ("button_x", button(2)),
        ("button_y", button(3)),
        ("button_up", button(11)),
        ("button_down", button(12)),
        ("button_left", button(13)),
        ("button_right", button(14)),
        ("button_l", button(9)),
        ("button_r", button(10)),
        ("button_start", button(6)),
        ("button_select", button(4)),
        ("button_zl", trigger(4)),
        ("button_zr", trigger(5)),
        // Home (Guide) stays AzotoTV's: held, it closes the game
        ("button_home", "\"\"".into()),
        ("button_debug", "\"\"".into()),
        ("button_gpio14", "\"\"".into()),
        ("button_power", "\"\"".into()),
        ("circle_pad", stick(0, 1)),
        ("c_stick", stick(2, 3)),
        ("motion_device", "\"engine:cemuhookudp\"".into()),
        ("touch_device", "\"engine:emu_window\"".into()),
        ("udp_input_address", "127.0.0.1".into()),
        ("udp_input_port", azotopad::motion::PORT.to_string()),
        ("udp_pad_index", "0".into()),
        ("use_touch_from_button", "false".into()),
        ("input_maptype", "1".into()),
    ];
    let mut s = String::from("[Controls]\nprofile=0\nprofiles\\size=1\nprofiles\\1\\name=AzotoTV\n");
    for (k, v) in controls {
        s += &format!("profiles\\1\\{k}={v}\n");
    }
    let storage = player.join("3ds");
    // 2 = large top screen + small bottom one, 4 = separate windows
    let layout = if separate { 4 } else { 2 };
    s += &format!(
        "\n[Data%20Storage]\nuse_virtual_sd=true\nuse_custom_storage=true\n\
         nand_directory={}/\nsdmc_directory={}/\n\
         \n[Layout]\nlayout_option={layout}\nswap_screen=false\nupright_screen=false\n\
         large_screen_proportion={PROPORTION}\nsmall_screen_position=2\nfilter_mode=true\n\
         \n[Renderer]\ngraphics_api=2\nphysical_device=0\nasync_shader_compilation=true\n\
         use_disk_shader_cache=true\nresolution_factor=4\nuse_vsync=true\nframe_limit=100\n\
         \n[System]\nregion_value=-1\nis_new_3ds=true\n\
         \n[UI]\nfullscreen=true\nconfirmClose=false\nfirstStart=false\nsingleWindowMode=true\n\
         displayTitleBars=false\nshowStatusBar=false\nshowFilterBar=false\nhideInactiveMouse=true\n\
         pauseWhenInBackground=false\nmuteWhenInBackground=false\nenable_discord_presence=false\n\
         saveStateWarning=false\nPaths\\language=it\n",
        storage.join("nand").display(),
        storage.join("sdmc").display(),
    );
    s
}

/// The 3DS's own settings in its config savefile: language Italian, the
/// user's name = the profile's (first 10 characters). The file: a header
/// (u16 entries, u16 data offset), then 12-byte entries (u32 block id,
/// u32 offset or the data itself when it fits in 4 bytes, u16 size, u16 flags).
fn set_cfg(cfg: &mut [u8], name: &str) -> Result<(), String> {
    const LANGUAGE: u32 = 0x000A_0002;
    const USERNAME: u32 = 0x000A_0000;
    const ITALIAN: u8 = 4;
    let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]) as usize;
    if cfg.len() < 4 {
        return Err("config 3DS troppo corto".into());
    }
    let entries = u16_at(cfg, 0);
    let (mut lang, mut user) = (false, false);
    for e in 0..entries {
        let at = 4 + e * 12;
        if at + 12 > cfg.len() {
            break;
        }
        let id = u32::from_le_bytes(cfg[at..at + 4].try_into().unwrap());
        let size = u16_at(cfg, at + 8);
        match id {
            LANGUAGE if size == 1 => {
                cfg[at + 4] = ITALIAN;
                lang = true;
            }
            USERNAME if size == 0x1C => {
                let off = u32::from_le_bytes(cfg[at + 4..at + 8].try_into().unwrap()) as usize;
                let Some(block) = cfg.get_mut(off..off + 0x1C) else { continue };
                block[..20].fill(0);
                for (i, unit) in name.encode_utf16().take(10).enumerate() {
                    block[i * 2..i * 2 + 2].copy_from_slice(&unit.to_le_bytes());
                }
                user = true;
            }
            _ => {}
        }
    }
    if lang && user { Ok(()) } else { Err("config 3DS senza lingua o nome".into()) }
}

/// This player's 3DS ready (Italian, their name).
fn prepare_3ds(player: &Path, user_name: &str) -> Result<(), String> {
    let storage = player.join("3ds");
    for d in ["nand", "sdmc"] {
        std::fs::create_dir_all(storage.join(d)).map_err(|e| e.to_string())?;
    }
    let cfg = cfg_file(&storage.join("nand"));
    if !cfg.exists() {
        let template = cfg_template();
        if !template.exists() {
            // Azahar makes its own (English) at first boot; Italian from the next one
            eprintln!("[azahar] no {}: the first boot will be in English", template.display());
            return Ok(());
        }
        std::fs::create_dir_all(cfg.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::copy(&template, &cfg).map_err(|e| format!("{}: {e}", cfg.display()))?;
    }
    let mut data = std::fs::read(&cfg).map_err(|e| format!("{}: {e}", cfg.display()))?;
    match set_cfg(&mut data, user_name) {
        Ok(()) => std::fs::write(&cfg, data).map_err(|e| format!("{}: {e}", cfg.display())),
        Err(e) => {
            eprintln!("[azahar] {}: {e}", cfg.display());
            Ok(())
        }
    }
}

/// The command that boots `game` fullscreen on this player's own 3DS.
/// `separate`: the bottom screen gets its own window (for the panel).
pub fn command(player: &Path, user_name: &str, game: &Path, separate: bool) -> Result<Command, String> {
    let config = config_file();
    std::fs::create_dir_all(config.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&config, qt_config(player, separate)).map_err(|e| format!("{}: {e}", config.display()))?;
    prepare_3ds(player, user_name)?;
    let mut c = Command::new("flatpak");
    c.args(["run", "--socket=x11", "--command=azahar"])
        // the player's 3DS (the rest of home is read-only in the sandbox)
        .arg(format!("--filesystem={}", player.join("3ds").display()))
        .arg(FLATPAK)
        .arg("-f")
        .arg(game);
    c.env("QT_QPA_PLATFORM", "xcb");
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config savefile with the two blocks AzotoTV changes, laid out like
    /// Azahar's (language inline, the name at an offset).
    fn cfg() -> Vec<u8> {
        let mut b = vec![0u8; 0x8000];
        b[0..2].copy_from_slice(&2u16.to_le_bytes());
        b[2..4].copy_from_slice(&0x455Cu16.to_le_bytes());
        // language: English, inline
        b[4..8].copy_from_slice(&0x000A_0002u32.to_le_bytes());
        b[8] = 1;
        b[12..14].copy_from_slice(&1u16.to_le_bytes());
        // name: "CITRA" at 0x455C
        b[16..20].copy_from_slice(&0x000A_0000u32.to_le_bytes());
        b[20..24].copy_from_slice(&0x455Cu32.to_le_bytes());
        b[24..26].copy_from_slice(&0x1Cu16.to_le_bytes());
        for (i, u) in "CITRA".encode_utf16().enumerate() {
            b[0x455C + i * 2..0x455C + i * 2 + 2].copy_from_slice(&u.to_le_bytes());
        }
        b
    }

    #[test]
    fn italian_with_the_profiles_name() {
        let mut b = cfg();
        set_cfg(&mut b, "Giovanni Bianchi").unwrap();
        assert_eq!(b[8], 4);
        let name: Vec<u16> = (0..10).map(|i| u16::from_le_bytes([b[0x455C + i * 2], b[0x455D + i * 2]])).collect();
        assert_eq!(String::from_utf16(&name).unwrap(), "Giovanni B");
        let mut b = cfg();
        set_cfg(&mut b, "Ada").unwrap();
        assert_eq!(&b[0x455C..0x455C + 8], &[b'A', 0, b'd', 0, b'a', 0, 0, 0]);
    }

    #[test]
    fn config_maps_the_pad_and_the_players_3ds() {
        let c = qt_config(Path::new("/p/players/abc"), true);
        assert!(c.contains("profiles\\1\\button_a=\"api:controller,button:0,engine:sdl,guid:030081b85e0400008e02000014010000,maptype:guid,port:0\"\n"));
        assert!(c.contains("profiles\\1\\button_zr=\"api:controller,axis:5,direction:+,"));
        assert!(c.contains("nand_directory=/p/players/abc/3ds/nand/\n"));
        assert!(c.contains("layout_option=4\n"));
        assert!(qt_config(Path::new("/p"), false).contains("layout_option=2\n"));
        assert!(c.contains("large_screen_proportion=4\n"));
    }

    #[test]
    fn bottom_screen_in_the_picture() {
        let near = |a: [f64; 4], b: [f64; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9);
        assert!(near(bottom_screen(true), [0.125, 0.0, 0.75, 1.0]));
        // 1920x1080: top 1600x960 at (0, 60), bottom 320x240 at (1600, 780)
        assert!(near(bottom_screen(false), [1600.0 / 1920.0, 780.0 / 1080.0, 320.0 / 1920.0, 240.0 / 1080.0]));
    }
}
