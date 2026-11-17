# Setting AzotoTV up

`./install.sh` builds AzotoTV and puts it in `~/azototv`. Start it with
`~/azototv/run-tv.sh`; the first start asks for your Jellyfin server and a
user. That is all it needs: films, series, live TV and music from Jellyfin
work from there.

Everything below is optional. Each feature is switched on by a file in
`~/.config/azototv/` (keep them mode 600: they hold API keys), and each has an
example in [`examples/`](../examples). Most of them can be set up from the TV
instead: **START › Services** (shown to Jellyfin administrators) tries the
address and key, writes the file and restarts AzotoTV. By hand, restart
AzotoTV after changing a file (`pkill -x azototv` in the kiosk session, which
starts it again).

## Language and region

AzotoTV speaks the system's language when it has it (English and Italian so
far), English otherwise. START › Language changes it. The region decides what
YouTube shows and how dates are written. To set either by hand:
`settings.json` → `{"language": "en", "region": "US"}`.

A new language is one file: copy `i18n/it.json`, translate the right-hand
side, and add it to `LANGUAGES` in `src-tauri/src/locale.rs` and to
`DICTIONARIES` in `src/lib/i18n.ts`.

## Services

| File | Switches on | Needs |
| --- | --- | --- |
| `arr.json` | "Ready to watch" notifications, download progress on requests | Radarr / Sonarr API keys, a Jellyfin API key; optionally RDT-Client |
| `seerr.json` | Requests (search what isn't in the library and ask for it) | a Seerr API key |
| `immich.json` | Gallery, photo screensaver, photos from phones | an [Immich](https://immich.app) user for the TV and its API key |
| `music.json` | Downloading songs found on YouTube Music into the library | a folder inside a Jellyfin music library, a Jellyfin API key |
| `steamgriddb.json` | Pictures for games added from the TV, console games too | a free [SteamGridDB](https://www.steamgriddb.com) API key |
| `youtube.json` | A profile's own Google account in YouTube | an OAuth client: see `src-tauri/src/ytaccount.rs` |
| `reminders.json` | Programmes announced by themselves (the evening news…) | their channel and name as the guide has them |
| `tv/` | Turning a Hisense / VIDAA TV on and off | see `src-tauri/src/tv.rs` |

Every leaf value in the examples is a placeholder to replace. What each file
means exactly is at the top of the source file that reads it.

## This machine

AzotoTV looks at the machine it runs on and works out the rest:

```sh
~/azototv/azototv --system
```

prints what it found: a laptop's built-in screen (then the TV mirrors it, it
goes off during games and it is what gets streamed), the GPU behind each
screen, how games are sent to the stronger GPU, and whether it may switch the
CPU's turbo. If a guess is wrong, `system.json` overrules it:

| Key | Meaning |
| --- | --- |
| `"panel": "eDP-1"` / `false` | the built-in screen's name, or "this laptop's screen is a screen like any other" |
| `"gpuEnv": {"DRI_PRIME": "1"}` / `false` | the environment that runs a program on the stronger GPU (found by itself for NVIDIA's driver) |
| `"tvSink": "alsa_output…"` | how the TV's sound output's name starts (`pactl list short sinks`) |
| `"turbo": false` | never touch the CPU's turbo |

## Booting straight into AzotoTV

`./install.sh --session` adds an "AzotoTV" session to the login screen (KDE
Plasma 6 on Wayland: it is KWin with AzotoTV as its only window) and the udev
rules that let it read TV remotes and light controllers' player numbers. Make
the display manager log you in automatically with that session and the
machine starts on AzotoTV. "Exit to the desktop" in START opens the normal
Plasma desktop, and "AzotoTV" in its menu comes back.

- `~/.config/azototv/session.env` (see `examples/session.env`): for a laptop
  with two GPUs, which one KWin composites on.
- `./install.sh --no-turbo`: keeps CPU turbo off outside games, for a machine
  that gets hot and loud.
- `session/plymouth/`: a boot splash with the logo
  (`/usr/share/plymouth/themes/azototv`, then `plymouth-set-default-theme -R azototv`).
- `session/wireplumber-hdmi-suspend.conf`: lets HDMI audio sleep sooner.

## Games

Games are added from the TV (Jellyfin administrators):

- **Games › Add a game**: pick the file that starts it (a Windows `.exe`, a
  Linux program, a Switch / Wii / 3DS file), say which Steam game it is, done.
  The game stays where it is; its pictures and description are looked up
  online once.
- **ROM folders** (`~/Games/roms` to begin with): Switch (`.nsp`, `.xci`), Wii
  (`.rvz`, `.wbfs`, `.iso`) and 3DS (`.3ds`, `.cci`) files put there, in
  subfolders too, show up in Games by themselves.
- **Games › Game folders**: which folders hold games and which are searched
  for ROMs, on any disk (`~/.config/azototv/games.json`).
- A game's **Edit** button: its name, players, Steam match, new pictures, or
  taking it out of AzotoTV (its files are never deleted).

Pictures come from Steam for Steam games, from SteamGridDB for everything when
it has a key (START › Services), and from GameTDB for Wii and 3DS boxes;
descriptions from Steam or Wikipedia.

By hand: each game folder holds one folder per game with a `game.json`; its
format is at the top of `src-tauri/src/games.rs` (`library.rs` for the
folders). AzotoTV launches native
games itself, Windows games through Proton (`umu-run`), and hands the rest to
emulators you install: Eden (`eden.rs`), Dolphin (`dolphin.rs`), Azahar
(`azahar.rs`), MAME. No game, ROM, key or firmware comes with AzotoTV.

## Phones

START › Connect a phone shows a QR code: the phone opens the page AzotoTV
serves on port 8686 of the home network (open it in your firewall for the
LAN only). `./install.sh --android` builds the Android app, which the TV then
offers for download from that page.

## Game streaming

With [Sunshine](https://github.com/LizardByte/Sunshine) installed, AzotoTV
follows it (`src-tauri/src/stream.rs`): a PC or a phone can play the TV's
games through Moonlight. `examples/sunshine.conf` and
`examples/sunshine-apps.json` are a starting point.
