# AzotoTV

A TV interface for a home server. It turns a Linux machine plugged into the
television into something that works like a console: you pick a profile and
move around with a controller, a TV remote or a phone.

It started as one family's project and it still is, so some corners are
rougher than others, but it is meant to run on your machine too: it looks at
the hardware it finds, speaks English or Italian, and needs only a Jellyfin
server to begin with.

The name: *azoto* is Italian for nitrogen. The project started on an Acer
Nitro laptop.

## What it does

- **Films and series** from [Jellyfin](https://jellyfin.org), played with mpv
  under AzotoTV's own on-screen controls. Jellyfin users are the profiles
  (each can have a PIN).
- **Live TV** with a programme guide, through Jellyfin's live TV.
- **Requests** through Seerr, with download progress from Radarr / Sonarr, and
  a notification when something arrives in the library.
- **YouTube** with a local, per-profile feed, and optionally a profile's own
  Google account.
- **Music** from Jellyfin, with per-profile playlists and YouTube Music search.
- **Photos** from [Immich](https://immich.app), also as a screensaver.
- **Games**: native Linux games, Windows games through Proton, and games
  handed to emulators that you install yourself (Eden, Dolphin, Azahar, MAME).
  Added from the TV (pick the game's file, pictures and description are found
  online) or by dropping Switch / Wii / 3DS files in a ROM folder. One virtual
  controller per player (`crates/azotopad`), per-profile saves, optional mods.
- **Phones**: a web page AzotoTV serves on the home network: sign in with a
  profile's PIN or a code from the TV, then films, series, live TV and music
  playing on the phone (resume points shared with the TV, "watch on the TV" /
  "continue here"), the photo gallery, requests, a remote with typing, "send
  to the TV" and the phone as a controller. A small Android app wraps it
  (`android/`) and adds the share menu, the volume keys and music with the
  screen off. What differs between the browser, the Android app and the
  iPhone app (a separate project for now) is listed in
  [docs/SETUP.md](docs/SETUP.md#phones).
- **The TV itself**: power and input on Hisense / VIDAA televisions.

## Quick start

On Linux, with Rust, Node with pnpm, [what Tauri 2 needs](https://v2.tauri.app/start/prerequisites/)
(WebKitGTK 4.1) and `mpv` installed:

```sh
git clone https://github.com/IKIR25/AzotoTV && cd AzotoTV
./install.sh
~/azototv/run-tv.sh
```

`install.sh` says what is missing, builds, and installs into `~/azototv`. The
first start asks for your Jellyfin server and a user, and films, series, live
TV and music work from there. `yt-dlp` adds YouTube, `ffmpeg` the Mii app's
music.

Requests, photos and notifications are switched on from the TV, in
**START › Services**. Everything else is optional too and described in
**[docs/SETUP.md](docs/SETUP.md)**:
requests, photos, notifications, the language, booting straight into AzotoTV
(`./install.sh --session`), games, phones and game streaming.

## What it expects

- **Linux**, on Wayland or X11. AzotoTV itself is one window and runs under
  any desktop.
- **KDE Plasma 6** for what goes beyond that window: the boot-into-AzotoTV
  session, turning screens off, two-screen games and the notification overlay
  use KWin and `kscreen-doctor`.
- The hardware is detected (`~/azototv/azototv --system` prints it): one GPU
  or two, a TV alone or a laptop with its own screen under the TV. The laptop
  case is the one it was built and tested on, so it is the best trodden.

## What is not here

No games, ROMs, console keys or firmware, artwork, music or channel lists.
You bring your own, and you are responsible for what you use it with.

Some features need files this repo deliberately does not carry:

| Feature | What you add |
| --- | --- |
| Drawing Miis | `./install.sh --mii` (fetches FFL.js, AGPL) and the Wii U's `FFLResHigh.dat` in `~/.local/share/azototv/mii/` |
| Google sign-in for YouTube | `~/.config/azototv/youtube.json` with an OAuth client (see `src-tauri/src/ytaccount.rs`) |
| Hisense TV control | the certificate and pairing described in `src-tauri/src/tv.rs` |
| The Android app | `./install.sh --android` (needs the Android SDK) |

Without them AzotoTV builds and runs; that feature is simply missing.

## Developing

```sh
pnpm install
pnpm tauri dev                  # AzotoTV in a window, reloading as you edit
pnpm check                      # type check
cargo test                      # in src-tauri and in crates/azotopad
```

| Path | What |
| --- | --- |
| `src/` | the interface (SvelteKit, Svelte 5) |
| `src-tauri/` | the app (Tauri 2, Rust): players, games, phones, services |
| `src-tauri/phone/` | the page phones open |
| `crates/azotopad/` | the controller layer for games |
| `android/` | the Android app |
| `i18n/` | translations: one file per language, from the English text to it |
| `session/` | boot session, udev rules, helper scripts |
| `examples/` | example configuration files |
| `tools/` | one-off scripts |

Text is written in English in the code (`t('Who is watching?')` in the
interface, `t!("…")` in Rust) and translated by `i18n/<language>.json`. Adding
a language is described in [docs/SETUP.md](docs/SETUP.md#language-and-region).

## Licence

AzotoTV is released under the [MIT licence](LICENSE).

Third-party code in this repo keeps its own licence:

- `src/lib/vendor/miijs/`: [miijs](https://github.com/Stewared/MiiJS) by
  Stewared, ISC.
- `crates/azotopad/gamecontrollerdb.txt`: from
  [SDL_GameControllerDB](https://github.com/mdqinc/SDL_GameControllerDB), zlib.

[FFL.js](https://github.com/ariankordi/FFL.js) is **not** in this repo.
`tools/fetch-mii-renderer.sh` downloads it, and it is AGPL-3.0-only: if you
distribute a build that includes it, that build has to follow the AGPL.

AzotoTV is not affiliated with or endorsed by Nintendo, Google, Jellyfin,
Hisense, Valve or anyone else whose products it works with. All trademarks
belong to their owners.
