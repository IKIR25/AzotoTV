//! KWin scripts AzotoTV loads while a game runs (through KWin's D-Bus
//! `/Scripting`): for now the two-screen games (3DS, Wii U), whose second
//! screen is a window of its own that goes on the laptop panel (a screen of
//! its own then, games.rs `split_screens`), borderless and filling it, while
//! the game's main window stays fullscreen on the TV.
//!
//! KWin opens a new window on the active screen, which may be the panel
//! (AzotoTV's own window was there), so the script places both, also the
//! ones that open later or change their title (Azahar names its second
//! window "<title> | Finestra secondaria" once the game runs: its UI
//! language, so in English "| Secondary Window").

use std::process::{Command, Stdio};

use crate::games::data_dir;

const NAME: &str = "azototv-screens";

/// The emulator windows to place: their class (X11 WM_CLASS / Wayland app
/// id) and what the second-screen window's title contains.
pub struct Windows {
    pub classes: &'static [&'static str],
    pub second: &'static [&'static str],
}

fn script(w: &Windows, tv: &str, panel: &str) -> String {
    let list = |v: &[&str]| serde_json::to_string(v).unwrap_or_else(|_| "[]".into());
    format!(
        r#"// written by AzotoTV (kwin.rs) for a two-screen game
const TV = {tv}, PANEL = {panel};
const CLASSES = {classes}, SECOND = {second};
function output(name) {{ return workspace.screens.find(o => o.name == name); }}
function place(w) {{
  if (!w || !CLASSES.includes(w.resourceClass)) return;
  const second = SECOND.some(s => w.caption.includes(s));
  const o = output(second ? PANEL : TV);
  if (!o) return;
  if (second) {{
    // a plain window (it can't go fullscreen): no frame, the whole panel, on top
    w.fullScreen = false;
    w.noBorder = true;
    w.keepAbove = true;
    w.skipTaskbar = true;
    if (w.output != o) workspace.sendClientToScreen(w, o);
    const g = o.geometry;
    w.frameGeometry = {{ x: g.x, y: g.y, width: g.width, height: g.height }};
  }} else if (w.normalWindow && w.output != o) {{
    // fullscreen again on the TV (KWin keeps a fullscreen window on its screen)
    w.fullScreen = false;
    workspace.sendClientToScreen(w, o);
    w.fullScreen = true;
  }}
}}
function watch(w) {{
  place(w);
  if (w) w.captionChanged.connect(() => place(w));
}}
workspace.windowAdded.connect(watch);
for (const w of workspace.windowList()) watch(w);
"#,
        tv = serde_json::to_string(tv).unwrap_or_default(),
        panel = serde_json::to_string(panel).unwrap_or_default(),
        classes = list(w.classes),
        second = list(w.second),
    )
}

fn busctl(args: &[&str]) -> Result<String, String> {
    let o = Command::new("busctl")
        .arg("--user")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("busctl: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

/// Keeps `w`'s windows placed: the main one on `tv`, the second screen on
/// `panel`, until [`stop`].
pub fn place_screens(w: &Windows, tv: &str, panel: &str) -> Result<(), String> {
    stop();
    let path = data_dir().join(format!("{NAME}.js"));
    std::fs::write(&path, script(w, tv, panel)).map_err(|e| format!("{}: {e}", path.display()))?;
    let path = path.to_string_lossy().into_owned();
    // "i 3"
    let out = busctl(&["call", "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", "loadScript", "ss", &path, NAME])?;
    let id: i64 = out.split_whitespace().nth(1).and_then(|n| n.parse().ok()).ok_or_else(|| format!("loadScript: {out}"))?;
    if id < 0 {
        return Err(t!("KWin didn't load the script"));
    }
    busctl(&["call", "org.kde.KWin", &format!("/Scripting/Script{id}"), "org.kde.kwin.Script", "run"])?;
    eprintln!("[kwin] placing {:?} on {tv} + {panel}", w.classes);
    Ok(())
}

/// The script gone (the game ended, or AzotoTV starting).
pub fn stop() {
    let loaded = busctl(&["call", "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", "isScriptLoaded", "s", NAME]);
    if loaded.is_ok_and(|out| out.trim() == "b true") {
        let _ = busctl(&["call", "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", "unloadScript", "s", NAME]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_names_the_screens_and_windows() {
        let w = Windows { classes: &["Azahar"], second: &["| Secondary Window"] };
        let s = script(&w, "HDMI-A-1", "eDP-1");
        assert!(s.contains("const TV = \"HDMI-A-1\", PANEL = \"eDP-1\";"));
        assert!(s.contains("const CLASSES = [\"Azahar\"], SECOND = [\"| Secondary Window\"];"));
    }
}
