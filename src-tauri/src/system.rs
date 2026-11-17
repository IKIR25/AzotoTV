//! What this machine is, found out once at start so nothing else has to
//! assume a particular one: whether it has a built-in screen (a laptop used
//! as the box under the TV), which GPU drives which screen, how games are
//! sent to the stronger GPU, and how the CPU's turbo is switched.
//!
//! Everything is detected from `/sys`; `~/.config/azototv/system.json` only
//! exists to overrule a guess (every key is optional):
//!
//! ```json
//! {
//!   "panel": "eDP-1",
//!   "gpuEnv": {"DRI_PRIME": "1"},
//!   "tvSink": "alsa_output.pci-0000_01_00.1.",
//!   "turbo": false
//! }
//! ```
//! - `panel`: the built-in screen's name as KWin has it, or `false` for
//!   "treat this laptop's screen like any other".
//! - `gpuEnv`: the environment that makes a program run on the stronger GPU,
//!   or `false` for none. Detected: NVIDIA's PRIME offload variables when the
//!   NVIDIA driver is loaded, nothing otherwise.
//! - `tvSink`: how the name of the TV's sound output starts (`pactl list
//!   short sinks`), when the guess (the HDMI sound of the TV's GPU) is wrong.
//! - `turbo`: `false` never touches the CPU's turbo.
//!
//! `azototv --system` prints what was found.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use serde_json::Value;

pub struct System {
    /// A laptop's built-in screen (KWin's name), when there is one. The TV
    /// then mirrors it, it goes off while a game runs, it is what Sunshine
    /// streams, and it holds a two-screen game's second screen.
    pub panel: Option<String>,
    /// What games and videos get in their environment to run on the
    /// stronger GPU (empty: wherever the desktop runs).
    pub gpu_env: Vec<(String, String)>,
    /// The CPU turbo switch: the file, what turns turbo on, what turns it off.
    pub turbo: Option<(PathBuf, &'static str, &'static str)>,
    /// How the TV's sound output's name must start, when it was set by hand.
    pub tv_sink: Option<String>,
}

/// The GPU behind a screen.
#[derive(Debug, PartialEq)]
pub struct Gpu {
    /// its render node (`/dev/dri/renderD128`)
    pub render: PathBuf,
    /// PCI vendor (0x10de NVIDIA, 0x8086 Intel, 0x1002 AMD)
    pub vendor: u32,
    /// its PCI address (`0000:01:00.0`), when it is a PCI device
    pub pci: Option<String>,
    /// where the screen comes among this GPU's connected ones (0 = first)
    pub index: usize,
}

impl Gpu {
    /// The hardware video encoder Sunshine should use on it.
    pub fn encoder(&self) -> &'static str {
        if self.vendor == 0x10de { "nvenc" } else { "vaapi" }
    }

    /// How the name of this GPU's HDMI sound output starts: its audio is
    /// function 1 of the same PCI device (NVIDIA, AMD).
    pub fn sink_prefix(&self) -> Option<String> {
        let pci = self.pci.as_ref()?;
        let (dev, _function) = pci.rsplit_once('.')?;
        Some(format!("alsa_output.pci-{}.1.", dev.replace(':', "_")))
    }
}

const SYS: &str = "/sys";

static SYSTEM: OnceLock<System> = OnceLock::new();

pub fn system() -> &'static System {
    SYSTEM.get_or_init(|| {
        let config = fs::read_to_string(crate::jellyfin::config_dir().join("system.json"))
            .ok()
            .and_then(|t| match serde_json::from_str::<Value>(&t) {
                Ok(v) => Some(v),
                Err(e) => {
                    eprintln!("[system] system.json: {e}");
                    None
                }
            })
            .unwrap_or(Value::Null);
        detect(Path::new(SYS), &config)
    })
}

/// The GPU that drives `output` (KWin's name, which is the kernel's).
pub fn gpu_of(output: &str) -> Option<Gpu> {
    gpu_in(Path::new(SYS), output)
}

fn detect(sys: &Path, config: &Value) -> System {
    let panel = match &config["panel"] {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Bool(false) => None,
        _ => panel_in(sys),
    };
    let gpu_env = match &config["gpuEnv"] {
        Value::Object(m) => m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect(),
        Value::Bool(false) => Vec::new(),
        _ => gpu_env_in(sys),
    };
    let turbo = if config["turbo"] == Value::Bool(false) { None } else { turbo_in(sys) };
    let tv_sink = config["tvSink"].as_str().filter(|s| !s.is_empty()).map(str::to_string);
    System { panel, gpu_env, turbo, tv_sink }
}

/// The connectors the kernel knows: (card, connector name, connected).
fn connectors(sys: &Path) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    for e in fs::read_dir(sys.join("class/drm")).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        // "card1-HDMI-A-1"
        let Some((card, connector)) = name.split_once('-') else { continue };
        if !card.starts_with("card") {
            continue;
        }
        let connected = fs::read_to_string(e.path().join("status")).is_ok_and(|s| s.trim() == "connected");
        out.push((card.to_string(), connector.to_string(), connected));
    }
    out.sort();
    out
}

/// A built-in screen: an embedded DisplayPort, LVDS or DSI connector with a
/// panel on it.
fn panel_in(sys: &Path) -> Option<String> {
    connectors(sys)
        .into_iter()
        .find(|(_, c, connected)| *connected && ["eDP-", "LVDS-", "DSI-"].iter().any(|p| c.starts_with(p)))
        .map(|(_, c, _)| c)
}

/// NVIDIA's PRIME render offload (what `prime-run` sets), when its driver is
/// loaded: on a laptop whose desktop runs on the integrated GPU it sends the
/// program to the NVIDIA, and where the NVIDIA is the only GPU it changes
/// nothing. Other pairs of GPUs need `gpuEnv` in system.json.
fn gpu_env_in(sys: &Path) -> Vec<(String, String)> {
    if !sys.join("module/nvidia").exists() {
        return Vec::new();
    }
    [("__NV_PRIME_RENDER_OFFLOAD", "1"), ("__VK_LAYER_NV_optimus", "NVIDIA_only"), ("__GLX_VENDOR_LIBRARY_NAME", "nvidia")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// The turbo switch this CPU has, if this user may write it (the optional
/// azototv-no-turbo.service makes it so): intel_pstate's, or cpufreq's boost.
fn turbo_in(sys: &Path) -> Option<(PathBuf, &'static str, &'static str)> {
    let cpu = sys.join("devices/system/cpu");
    [(cpu.join("intel_pstate/no_turbo"), "0", "1"), (cpu.join("cpufreq/boost"), "1", "0")]
        .into_iter()
        .find(|(f, _, _)| fs::OpenOptions::new().write(true).open(f).is_ok())
}

fn gpu_in(sys: &Path, output: &str) -> Option<Gpu> {
    let all = connectors(sys);
    let (card, _, _) = all.iter().find(|(_, c, _)| c == output)?;
    let device = sys.join("class/drm").join(card).join("device");
    let vendor = fs::read_to_string(device.join("vendor")).ok().and_then(|v| u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok())?;
    let render = fs::read_dir(device.join("drm"))
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.starts_with("renderD"))?;
    // ".../0000:00:01.0/0000:01:00.0"
    let pci = fs::canonicalize(&device)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .filter(|n| n.split(':').count() == 3 && n.contains('.'));
    let index = all.iter().filter(|(c, _, connected)| c == card && *connected).position(|(_, c, _)| c == output).unwrap_or(0);
    Some(Gpu { render: Path::new("/dev/dri").join(render), vendor, pci, index })
}

/// `azototv --system`: what was found, for setting a machine up.
pub fn print() {
    let s = system();
    let sys = Path::new(SYS);
    println!("built-in screen: {}", s.panel.as_deref().unwrap_or("none"));
    for (card, connector, connected) in connectors(sys) {
        if !connected {
            continue;
        }
        match gpu_in(sys, &connector) {
            Some(g) => println!(
                "screen {connector}: {card}, vendor {:#06x}, {}, encoder {}, HDMI sound {}",
                g.vendor,
                g.render.display(),
                g.encoder(),
                g.sink_prefix().as_deref().unwrap_or("?")
            ),
            None => println!("screen {connector}: {card}"),
        }
    }
    if s.gpu_env.is_empty() {
        println!("games and videos: on the desktop's GPU (no offload environment)");
    } else {
        println!("games and videos: {}", s.gpu_env.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" "));
    }
    match &s.turbo {
        Some((f, ..)) => println!("CPU turbo: off in the menus, on in games ({})", f.display()),
        None => println!("CPU turbo: left alone (no switch this user may write)"),
    }
    println!("TV sound output: {}", s.tv_sink.as_deref().unwrap_or("found from the TV's GPU"));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake /sys: `gpus` = (card, PCI address, vendor, render node, connectors (name, connected)).
    fn fake(name: &str, gpus: &[(&str, &str, &str, &str, &[(&str, bool)])], nvidia_driver: bool) -> PathBuf {
        let root = std::env::temp_dir().join(format!("azototv-sys-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (card, pci, vendor, render, outs) in gpus {
            let dev = root.join("devices/pci0000:00").join(pci);
            fs::create_dir_all(dev.join("drm").join(render)).unwrap();
            fs::write(dev.join("vendor"), format!("{vendor}\n")).unwrap();
            let c = root.join("class/drm").join(card);
            fs::create_dir_all(&c).unwrap();
            std::os::unix::fs::symlink(&dev, c.join("device")).unwrap();
            for (out, connected) in *outs {
                let o = root.join("class/drm").join(format!("{card}-{out}"));
                fs::create_dir_all(&o).unwrap();
                fs::write(o.join("status"), if *connected { "connected\n" } else { "disconnected\n" }).unwrap();
            }
        }
        if nvidia_driver {
            fs::create_dir_all(root.join("module/nvidia")).unwrap();
        }
        root
    }

    #[test]
    fn laptop_with_two_gpus() {
        let sys = fake(
            "laptop",
            &[
                ("card0", "0000:01:00.0", "0x10de", "renderD129", &[("HDMI-A-1", true), ("DP-1", false)]),
                ("card1", "0000:00:02.0", "0x8086", "renderD128", &[("eDP-1", true)]),
            ],
            true,
        );
        let s = detect(&sys, &Value::Null);
        assert_eq!(s.panel.as_deref(), Some("eDP-1"));
        assert_eq!(s.gpu_env.len(), 3);
        assert!(s.turbo.is_none() && s.tv_sink.is_none());
        let tv = gpu_in(&sys, "HDMI-A-1").unwrap();
        assert_eq!((tv.encoder(), tv.render.to_str().unwrap(), tv.index), ("nvenc", "/dev/dri/renderD129", 0));
        assert_eq!(tv.sink_prefix().as_deref(), Some("alsa_output.pci-0000_01_00.1."));
        let panel = gpu_in(&sys, "eDP-1").unwrap();
        assert_eq!((panel.encoder(), panel.render.to_str().unwrap()), ("vaapi", "/dev/dri/renderD128"));
        assert!(gpu_in(&sys, "HDMI-A-2").is_none());
        let _ = fs::remove_dir_all(sys);
    }

    #[test]
    fn small_pc_with_one_gpu() {
        let sys = fake("minipc", &[("card0", "0000:00:02.0", "0x8086", "renderD128", &[("DP-1", true), ("HDMI-A-1", true)])], false);
        let s = detect(&sys, &Value::Null);
        assert!(s.panel.is_none() && s.gpu_env.is_empty());
        let tv = gpu_in(&sys, "HDMI-A-1").unwrap();
        assert_eq!((tv.encoder(), tv.index), ("vaapi", 1));
        let _ = fs::remove_dir_all(sys);
    }

    #[test]
    fn system_json_overrules() {
        let sys = fake("config", &[("card0", "0000:00:02.0", "0x8086", "renderD128", &[("eDP-1", true)])], true);
        let s = detect(&sys, &serde_json::json!({ "panel": false, "gpuEnv": { "DRI_PRIME": "1" }, "tvSink": "alsa_output.usb", "turbo": false }));
        assert!(s.panel.is_none() && s.turbo.is_none());
        assert_eq!(s.gpu_env, vec![("DRI_PRIME".to_string(), "1".to_string())]);
        assert_eq!(s.tv_sink.as_deref(), Some("alsa_output.usb"));
        let s = detect(&sys, &serde_json::json!({ "panel": "DSI-1", "gpuEnv": false }));
        assert_eq!(s.panel.as_deref(), Some("DSI-1"));
        assert!(s.gpu_env.is_empty());
        let _ = fs::remove_dir_all(sys);
    }
}
