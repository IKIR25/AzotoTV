//! Steam games, on a machine with Steam (a computer): every installed one
//! becomes a game of the library by itself (`"type": "steam"`, `"added":
//! "steam"`, found by `library::scan`), so AzotoTV lists all of them, not
//! only the DRM-free ones. They start through Steam (it has to be signed in),
//! which also keeps their saves (Steam Cloud, the Steam account's, not the
//! profile's) and runs the controllers with Steam Input. They stay on this
//! machine: another AzotoTV never gets them to download.
//!
//! A game uninstalled from Steam disappears from the library; one taken out
//! of AzotoTV is remembered (`games.json` `hidden`: `steam:<appid>`).

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::games::home;

/// Steam's own pieces, not games.
const NOT_GAMES: [&str; 5] = ["Proton", "Steam Linux Runtime", "Steamworks Common", "Legacy Steam Runtime", "SteamVR"];

#[derive(Debug, PartialEq)]
pub struct App {
    pub appid: u32,
    pub name: String,
}

/// Where Steam keeps itself: the usual install, then the Flatpak one.
fn root() -> Option<PathBuf> {
    [".local/share/Steam", ".steam/steam", ".var/app/com.valvesoftware.Steam/.local/share/Steam"]
        .iter()
        .map(|p| home().join(p))
        .find(|p| p.join("steamapps").is_dir())
}

fn flatpak() -> bool {
    root().is_some_and(|r| r.starts_with(home().join(".var/app")))
}

/// `"key"		"value"` lines of a VDF/ACF file, in order.
fn pairs(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|l| {
        let mut q = l.split('"');
        let (_, k, _, v) = (q.next()?, q.next()?, q.next()?, q.next()?);
        Some((k, v))
    })
}

/// Every Steam library (`libraryfolders.vdf`), Steam's own first.
fn libraries(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.to_path_buf()];
    if let Ok(t) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
        for (k, v) in pairs(&t) {
            let p = PathBuf::from(v.replace("\\\\", "\\"));
            if k == "path" && !out.iter().any(|o| fs::canonicalize(o).ok() == fs::canonicalize(&p).ok()) {
                out.push(p);
            }
        }
    }
    out
}

/// An app manifest: the app, when it is a fully installed game.
fn parse_acf(text: &str) -> Option<App> {
    let (mut appid, mut name, mut flags) = (None, None, 0u32);
    for (k, v) in pairs(text) {
        match k {
            "appid" if appid.is_none() => appid = v.parse().ok(),
            "name" if name.is_none() => name = Some(v.to_string()),
            "StateFlags" => flags = v.parse().unwrap_or(0),
            _ => {}
        }
    }
    let name = name?;
    // 4: fully installed (an update pending still has it)
    (flags & 4 != 0 && !NOT_GAMES.iter().any(|n| name.starts_with(n)) && appid != Some(228980)).then_some(App { appid: appid?, name })
}

/// The installed Steam games, by name.
pub fn installed() -> Vec<App> {
    let Some(root) = root() else { return Vec::new() };
    let mut out: Vec<App> = libraries(&root)
        .iter()
        .flat_map(|l| fs::read_dir(l.join("steamapps")).into_iter().flatten().flatten())
        .filter(|e| e.file_name().to_string_lossy().starts_with("appmanifest_"))
        .filter_map(|e| parse_acf(&fs::read_to_string(e.path()).ok()?))
        .collect();
    out.sort_by_key(|a| a.name.to_lowercase());
    out.dedup_by_key(|a| a.appid);
    out
}

pub fn is_installed(appid: u32) -> bool {
    let Some(root) = root() else { return false };
    libraries(&root).iter().any(|l| {
        fs::read_to_string(l.join(format!("steamapps/appmanifest_{appid}.acf"))).ok().and_then(|t| parse_acf(&t)).is_some()
    })
}

/// Steam's own pictures of a game (its library cache) into `art/`, so a new
/// game has them at once, without the network. What's missing is looked up later.
pub fn copy_art(appid: u32, dir: &Path) {
    let Some(root) = root() else { return };
    let cache = root.join("appcache/librarycache").join(appid.to_string());
    let art = dir.join("art");
    for (from, to) in [("library_600x900.jpg", "cover.jpg"), ("library_hero.jpg", "hero.jpg"), ("logo.png", "logo.png"), ("header.jpg", "header.jpg")] {
        // newer Steam keeps some of them one folder down
        let found = std::iter::once(cache.join(from))
            .chain(fs::read_dir(&cache).into_iter().flatten().flatten().map(|e| e.path().join(from)))
            .find(|p| p.is_file());
        if let Some(p) = found {
            let _ = fs::create_dir_all(&art);
            let _ = fs::copy(p, art.join(to));
        }
    }
}

/// Starts the game through Steam and lasts as long as it does, so it can be
/// waited on like any other game. Steam itself is started outside the game's
/// process group (closing the game must not close Steam); stopping this
/// (SIGTERM) closes the game: whatever Steam's launcher (`reaper`) runs for it.
pub fn command(appid: u32) -> Command {
    const SCRIPT: &str = r#"
id=$1; shift
setsid -f "$@" "steam://rungameid/$id" >/dev/null 2>&1 </dev/null
reaper() { pgrep -f "/reaper SteamLaunch AppId=$id( |$)" | head -n1; }
kids() { for c in $(pgrep -P "$1"); do kids "$c"; echo "$c"; done; }
trap 'r=$(reaper); [ -n "$r" ] && kill $(kids "$r") 2>/dev/null; exit 0' TERM INT
# Steam may have to start (and sign in, and update the game) first
i=0
until [ -n "$(reaper)" ]; do
  i=$((i + 1)); [ "$i" -gt 600 ] && exit 1
  sleep 1 & wait $!
done
while [ -n "$(reaper)" ]; do sleep 2 & wait $!; done
"#;
    let mut c = Command::new("sh");
    c.args(["-c", SCRIPT, "sh", &appid.to_string()]);
    if flatpak() {
        c.args(["flatpak", "run", "com.valvesoftware.Steam"]);
    } else {
        c.arg("steam");
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests() {
        let game = "\"AppState\"\n{\n\t\"appid\"\t\t\"105600\"\n\t\"name\"\t\t\"Terraria\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"UserConfig\"\n\t{\n\t\t\"name\"\t\t\"x\"\n\t}\n}";
        assert_eq!(parse_acf(game), Some(App { appid: 105600, name: "Terraria".into() }));
        let tool = game.replace("Terraria", "Proton 9.0");
        assert_eq!(parse_acf(&tool), None);
        let downloading = game.replace("\"4\"", "\"1026\"");
        assert_eq!(parse_acf(&downloading), None);
        let updating = game.replace("\"4\"", "\"6\"");
        assert!(parse_acf(&updating).is_some());
    }
}
