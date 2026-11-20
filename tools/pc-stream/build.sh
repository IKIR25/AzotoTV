#!/bin/sh
# PC side of "play from the PC on the TV" (Sunshine on this computer, Moonlight
# on the TV's AzotoTV: see play.sh). Builds and installs, for this user:
#   ~/.local/bin/azoto-vscreen        virtual screen at the TV's size (vscreen.c)
#   ~/.local/bin/azoto-stream-prep    Sunshine prep command: that screen on, monitors off
#   ~/.local/bin/azototv-stream-app   Sunshine app: this AzotoTV, full screen
#   ~/.local/share/applications/azoto-vscreen.desktop  (KWin's permission for virtual screens)
# and with --sunshine, the Sunshine settings that use them (sunshine.conf,
# apps.json; the old ones are kept as *.before-azototv). Sunshine then runs
# from when AzotoTV opens on this computer, not from login.
set -e
cd "$(dirname "$0")"
out=${XDG_CACHE_HOME:-$HOME/.cache}/azoto-vscreen-build
mkdir -p "$out"
wayland-scanner client-header zkde-screencast-unstable-v1.xml "$out/zkde-screencast-unstable-v1-client-protocol.h"
wayland-scanner private-code zkde-screencast-unstable-v1.xml "$out/zkde-screencast-unstable-v1-protocol.c"
cc -O2 -Wall -Wextra -I"$out" -o "$out/azoto-vscreen" vscreen.c "$out/zkde-screencast-unstable-v1-protocol.c" $(pkg-config --cflags --libs wayland-client)

bin=$HOME/.local/bin
mkdir -p "$bin" "$HOME/.local/share/applications"
install -m 755 "$out/azoto-vscreen" "$bin/azoto-vscreen"
install -m 755 stream-prep.sh "$bin/azoto-stream-prep"
install -m 755 azototv-app.sh "$bin/azototv-stream-app"
cat > "$HOME/.local/share/applications/azoto-vscreen.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=AzotoTV virtual screen
Comment=Makes a virtual screen for streaming games to the TV
Exec=$bin/azoto-vscreen
NoDisplay=true
X-KDE-Wayland-Interfaces=zkde_screencast_unstable_v1
EOF
echo "installed azoto-vscreen, azoto-stream-prep, azototv-stream-app in $bin"

[ "$1" = "--sunshine" ] || exit 0
conf=$HOME/.config/sunshine
mkdir -p "$conf"
for f in sunshine.conf apps.json; do
    if [ -f "$conf/$f" ] && [ ! -f "$conf/$f.before-azototv" ]; then cp -p "$conf/$f" "$conf/$f.before-azototv"; fi
done
# sunshine.conf: our keys replace the same keys, the rest stays
touch "$conf/sunshine.conf"
sed -i -E '/^(capture|output_name|global_prep_cmd) *=/d; /^# (AzotoTV|PC -> TV|\(AzotoTV|size; without it)/d' "$conf/sunshine.conf"
cat >> "$conf/sunshine.conf" <<EOF
# AzotoTV (tools/pc-stream): capture the virtual screen KWin makes at the TV's
# size; without it (no stream) Sunshine takes the first monitor
capture = kwin
output_name = Virtual-AzotoTV
global_prep_cmd = [{"do":"$bin/azoto-stream-prep do","undo":"$bin/azoto-stream-prep undo","elevated":false}]
EOF
# apps.json: an "AzotoTV" app (replaced if there)
python3 - "$conf/apps.json" "$bin/azototv-stream-app" <<'EOF'
import json, os, sys
path, cmd = sys.argv[1], sys.argv[2]
try:
    with open(path) as f:
        data = json.load(f)
except (OSError, ValueError):
    data = {"env": {}, "apps": [{"name": "Desktop", "image-path": "desktop.png"}]}
data["apps"] = [a for a in data.get("apps", []) if a.get("name") != "AzotoTV"]
data["apps"].insert(0, {"name": "AzotoTV", "cmd": cmd, "auto-detach": "false", "image-path": "desktop.png"})
tmp = path + ".tmp"
with open(tmp, "w") as f:
    json.dump(data, f, indent=2)
os.replace(tmp, path)
EOF
# not from login: AzotoTV starts Sunshine when it opens (stream.rs start_pc_host)
systemctl --user disable app-dev.lizardbyte.app.Sunshine.service 2>/dev/null || true
echo "Sunshine set up: restart it (systemctl --user restart app-dev.lizardbyte.app.Sunshine.service);"
echo "from now on AzotoTV starts it when it opens, not the login"
