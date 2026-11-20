#!/bin/sh
# Builds azoto-vscreen and installs it for this user:
#   ~/.local/bin/azoto-vscreen
#   ~/.local/share/applications/azoto-vscreen.desktop (KWin's permission for virtual screens)
set -e
cd "$(dirname "$0")"
out=${XDG_CACHE_HOME:-$HOME/.cache}/azoto-vscreen-build
mkdir -p "$out"
wayland-scanner client-header zkde-screencast-unstable-v1.xml "$out/zkde-screencast-unstable-v1-client-protocol.h"
wayland-scanner private-code zkde-screencast-unstable-v1.xml "$out/zkde-screencast-unstable-v1-protocol.c"
cc -O2 -Wall -Wextra -I"$out" -o "$out/azoto-vscreen" vscreen.c "$out/zkde-screencast-unstable-v1-protocol.c" $(pkg-config --cflags --libs wayland-client)

bin=$HOME/.local/bin/azoto-vscreen
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
install -m 755 "$out/azoto-vscreen" "$bin"
cat > "$HOME/.local/share/applications/azoto-vscreen.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=AzotoTV virtual screen
Comment=Makes a virtual screen for streaming games to the TV
Exec=$bin
NoDisplay=true
X-KDE-Wayland-Interfaces=zkde_screencast_unstable_v1
EOF
install -m 755 stream-prep.sh "$HOME/.local/bin/azoto-stream-prep"
echo "installed $bin and azoto-stream-prep"
