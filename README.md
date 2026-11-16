# AzotoTV

A TV interface for a home server. It turns a Linux machine plugged into the
television into something that works like a console: you pick a profile and
move around with a controller, a TV remote or a phone.

It is a personal project, written for one family and one machine, and it is
published as it is. The interface is in Italian. Expect to adapt things: see
[What is specific to my setup](#what-is-specific-to-my-setup).

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
- **Games**: a library of folders, each with a `game.json`. Native Linux
  games, Windows games through Proton, and games handed to emulators that you
  install yourself (Eden, Dolphin, Azahar, MAME). One virtual controller per
  player (`crates/azotopad`), per-profile saves, optional mods.
- **Phones**: a web page AzotoTV serves on the home network (remote, keyboard,
  "send to the TV", photos, the phone as a controller), and a small Android
  app that wraps it (`android/`).
- **The TV itself**: power and input on Hisense / VIDAA televisions.

## What is not here

No games, ROMs, console keys or firmware, artwork, music or channel lists.
You bring your own, and you are responsible for what you use it with.

Some features need files this repo deliberately does not carry:

| Feature | What you add |
| --- | --- |
| Drawing Miis | `tools/fetch-mii-renderer.sh` (fetches FFL.js, AGPL) and the Wii U's `FFLResHigh.dat` in `~/.local/share/azototv/mii/` |
| Google sign-in for YouTube | `~/.config/azototv/youtube.json` with an OAuth client (see `src-tauri/src/ytaccount.rs`) |
| Hisense TV control | the certificate and pairing described in `src-tauri/src/tv.rs` |
| The Android app | `android/build.sh` (needs the Android SDK) before building AzotoTV |

Without them AzotoTV builds and runs; that feature is simply missing.

## Building

Linux only. You need Rust, Node with pnpm, and what
[Tauri 2 needs on Linux](https://v2.tauri.app/start/prerequisites/)
(WebKitGTK 4.1). At run time: `mpv`, `yt-dlp` and `ffmpeg`.

```sh
pnpm install
tools/fetch-mii-renderer.sh     # optional, see above
android/build.sh                # optional, see above
pnpm tauri build --no-bundle    # -> src-tauri/target/release/azototv
```

`pnpm tauri dev` runs it in a window. Tests: `cargo test` in `src-tauri` and
in `crates/azotopad`, and `pnpm check`.

## Setting it up

On first start AzotoTV asks for the Jellyfin address and a user. Everything
else is optional and switched on by a file in `~/.config/azototv/` (keep them
mode 600: they hold API keys). Each is described at the top of the source file
that reads it:

| File | For | Described in |
| --- | --- | --- |
| `arr.json` | new-arrival notifications, download progress | `src-tauri/src/arr.rs`, `downloads.rs` |
| `seerr.json` | requests | `src-tauri/src/seerr.rs` |
| `immich.json` | photos | `src-tauri/src/immich.rs` |
| `music.json` | downloading songs into the library | `src-tauri/src/songs.rs` |
| `youtube.json` | Google sign-in | `src-tauri/src/ytaccount.rs` |
| `tv/` | Hisense TV control | `src-tauri/src/tv.rs` |

Games live in `~/Games` (or `$AZOTOTV_GAMES`), one folder per game; the format
of `game.json` is at the top of `src-tauri/src/games.rs`.

`session/` holds what makes a machine boot straight into AzotoTV: a login
session that runs KWin with AzotoTV as its only window, a boot splash, udev
rules for remotes and controller lights, and helper scripts.

## What is specific to my setup

These are written for the laptop this runs on, and you will have to change
them for another machine:

- **Two GPUs and two screens.** The code assumes an Intel iGPU that drives the
  laptop panel (`eDP-1`) and an NVIDIA card that drives the TV over HDMI, at
  fixed PCI paths: `session/session.sh`, `session/sunshine.conf`,
  `session/wireplumber-hdmi-suspend.conf`, and constants in
  `src-tauri/src/games.rs`, `stream.rs` and `phonescreen.rs`.
- **CPU turbo** is switched through `intel_pstate`
  (`session/azototv-no-turbo.service`).
- **KDE Plasma 6** on Wayland: the session, window placement (`kwin.rs`) and
  the notification overlay rely on it.
- **Italy**: the interface language, YouTube's region, and automatic reminders
  for the Rai news programmes (`src-tauri/src/notify.rs`).

## Layout

| Path | What |
| --- | --- |
| `src/` | the interface (SvelteKit, Svelte 5) |
| `src-tauri/` | the app (Tauri 2, Rust): players, games, phones, services |
| `src-tauri/phone/` | the page phones open |
| `crates/azotopad/` | the controller layer for games |
| `android/` | the Android app |
| `session/` | boot session, udev rules, helper scripts |
| `tools/` | one-off scripts |

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
