#!/bin/sh
# Builds AzotoTV and installs it for the user running this, in ~/azototv.
#
#   ./install.sh                 build, install, add "AzotoTV" to the desktop's menu
#   ./install.sh --mii           also fetch the Mii renderer first (FFL.js, AGPL: see README)
#   ./install.sh --android       also build the Android app first (needs the Android SDK)
#   ./install.sh --session       also the login session that boots straight into AzotoTV
#                                (KDE Plasma 6; asks for sudo) and the udev rules for
#                                TV remotes and controller lights
#   ./install.sh --no-turbo      also the service that keeps CPU turbo off outside games
#                                (for a laptop that gets hot under the TV; asks for sudo)
#   ./install.sh --no-build      install what is already built
#
# Run it again to update. It never touches your settings (~/.config/azototv),
# your data (~/.local/share/azototv) or a run-tv.sh you changed.
set -eu
cd "$(dirname "$0")"

MII= ANDROID= SESSION= NOTURBO= BUILD=1
for a in "$@"; do
  case $a in
    --mii) MII=1 ;;
    --android) ANDROID=1 ;;
    --session) SESSION=1 ;;
    --no-turbo) NOTURBO=1 ;;
    --no-build) BUILD= ;;
    -h | --help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $a (see --help)" >&2; exit 2 ;;
  esac
done

say() { printf '\n== %s\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }

# ---- what is needed ---------------------------------------------------------
missing= old=
need() { have "$1" || missing="$missing $1"; }
# at least version $2 of a "x.y" version number $1
atleast() { [ "$(printf '%s\n%s\n' "$2" "$1" | sort -t. -k1,1n -k2,2n | head -n1)" = "$2" ]; }
if [ -n "$BUILD" ]; then
  need cargo
  need node
  need pnpm
  have pkg-config || missing="$missing pkg-config"
  have pkg-config && { pkg-config --exists webkit2gtk-4.1 2>/dev/null || missing="$missing webkit2gtk-4.1(dev)"; }
  # Tauri needs Rust 1.90 (src-tauri/Cargo.toml), Vite Node 20.19 or 22.12
  if have rustc; then
    v=$(rustc --version | sed -n 's/^rustc \([0-9]*\.[0-9]*\).*/\1/p')
    atleast "$v" 1.90 || old="$old rust($v, needs 1.90)"
  fi
  if have node; then
    v=$(node --version | sed -n 's/^v\([0-9]*\.[0-9]*\).*/\1/p')
    { atleast "$v" 22.12 || { [ "${v%%.*}" = 20 ] && atleast "$v" 20.19; }; } || old="$old node($v, needs 20.19 or 22.12)"
  fi
fi
need mpv
if [ -n "$missing$old" ]; then
  [ -n "$missing" ] && echo "Missing:$missing"
  [ -n "$old" ] && echo "Too old:$old"
  echo "  Arch:    sudo pacman -S --needed rust nodejs pnpm webkit2gtk-4.1 base-devel mpv yt-dlp ffmpeg"
  echo "  Fedora:  sudo dnf install cargo nodejs pnpm webkit2gtk4.1-devel openssl-devel alsa-lib-devel systemd-devel mpv yt-dlp ffmpeg"
  echo "  Debian / Ubuntu:  sudo apt install nodejs libwebkit2gtk-4.1-dev build-essential pkg-config curl libssl-dev libasound2-dev libudev-dev mpv yt-dlp ffmpeg"
  echo "           and Rust from https://rustup.rs: the distribution's cargo is too old"
  echo "  pnpm, where the distribution has none: curl -fsSL https://get.pnpm.io/install.sh | sh -   (then open a new terminal)"
  echo "  The full list for building: https://v2.tauri.app/start/prerequisites/"
  exit 1
fi
for tool in yt-dlp ffmpeg kscreen-doctor; do
  have "$tool" || echo "note: $tool not found ($(case $tool in yt-dlp) echo YouTube ;; ffmpeg) echo "Mii music" ;; *) echo "screens off, two-screen games: KDE only" ;; esac) won't work without it)"
done

# ---- build ------------------------------------------------------------------
if [ -n "$MII" ]; then
  say "Mii renderer (FFL.js, AGPL-3.0)"
  tools/fetch-mii-renderer.sh
fi
if [ -n "$ANDROID" ]; then
  say "Android app"
  android/build.sh
fi
if [ -n "$BUILD" ]; then
  say "Building (the first time takes a while)"
  pnpm install --frozen-lockfile
  pnpm tauri build --no-bundle
fi
BIN=src-tauri/target/release/azototv
[ -x "$BIN" ] || BIN=${CARGO_TARGET_DIR:-src-tauri/target}/release/azototv
[ -x "$BIN" ] || { echo "no built azototv found: run without --no-build" >&2; exit 1; }

# ---- install ----------------------------------------------------------------
DEST=$HOME/azototv
say "Installing in $DEST"
mkdir -p "$DEST" "$HOME/.config/azototv" "$HOME/.local/share/applications"
# a running AzotoTV keeps its old file; the kiosk session starts the new one when it is restarted
[ -f "$DEST/azototv" ] && cp -p "$DEST/azototv" "$DEST/azototv.prev"
install -m 755 "$BIN" "$DEST/azototv.new" && mv -f "$DEST/azototv.new" "$DEST/azototv"
install -m 755 session/session.sh session/kiosk.sh session/tv-mode.sh session/joycon-pair.sh session/joycon-rate.py "$DEST/"
[ -e "$DEST/run-tv.sh" ] || install -m 755 session/run-tv.sh "$DEST/"
grep -v '^#' session/azototv-menu.desktop | sed "s|@HOME@|$HOME|g" >"$HOME/.local/share/applications/azototv.desktop"

if [ -n "$SESSION" ]; then
  say "Login session and udev rules (sudo)"
  run=
  for c in /usr/lib/plasma-dbus-run-session-if-needed /usr/libexec/plasma-dbus-run-session-if-needed /usr/lib/*/libexec/plasma-dbus-run-session-if-needed; do
    [ -x "$c" ] && run=$c && break
  done
  [ -n "$run" ] || { have dbus-run-session && run=$(command -v dbus-run-session); }
  [ -n "$run" ] || { echo "neither Plasma's D-Bus session starter nor dbus-run-session found" >&2; exit 1; }
  have kwin_wayland_wrapper || echo "note: kwin_wayland_wrapper not found: the session needs KDE Plasma 6 (KWin)"
  grep -v '^#' session/azototv.desktop | sed "s|@HOME@|$HOME|g; s|@DBUS_RUN@|$run|g" | sudo tee /usr/share/wayland-sessions/azototv.desktop >/dev/null
  sudo install -m 644 session/70-azototv-remote.rules session/71-azototv-pad-lights.rules /etc/udev/rules.d/
  sudo udevadm control --reload
  echo "\"AzotoTV\" is now a session on the login screen. To boot straight into it, make your"
  echo "display manager log $USER in automatically with the session azototv (SDDM: [Autologin]"
  echo "User=$USER, Session=azototv in /etc/sddm.conf.d/autologin.conf)."
fi

if [ -n "$NOTURBO" ]; then
  say "CPU turbo off outside games (sudo)"
  sed "s|@USER@|$USER|g" session/azototv-no-turbo.service | sudo tee /etc/systemd/system/azototv-no-turbo.service >/dev/null
  sudo systemctl daemon-reload
  sudo systemctl enable --now azototv-no-turbo.service
fi

say "Done"
"$DEST/azototv" --system 2>/dev/null | sed 's/^/  /' || true
cat <<EOT

Start it:   $DEST/run-tv.sh   (or "AzotoTV" in the desktop's menu)
First start asks for your Jellyfin server and a user.
Everything else is optional and switched on by a file in ~/.config/azototv/:
the examples are in examples/, what each does is in docs/SETUP.md.
EOT
