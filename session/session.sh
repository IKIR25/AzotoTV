#!/bin/sh
# AzotoTV login session (plasmalogin autologin, Session=azototv.desktop).
# KWin runs on its own with AzotoTV as its only client: no Plasma shell, panels,
# lock screen or desktop services. When AzotoTV ends the session ("Esci al
# desktop"), the normal Plasma desktop starts in this same login. The "AzotoTV"
# entry in Plasma's menu (tv-mode.sh) logs out of Plasma with a flag set, and
# then the AzotoTV session comes back; a plain logout ends the login as before.
cd "$(dirname "$(readlink -f "$0")")"

# KWin composites on the Intel iGPU (the first device in KWIN_DRM_DEVICES is the
# primary one), so AzotoTV's menus are drawn there too and the GTX 1060 only
# sends the picture to HDMI-A-1 (the TV): with KWin on the NVIDIA, every focus
# move or slide woke it to full clocks (~23 W instead of ~6 W) for 10-35 s.
# Films and games still run on the NVIDIA (PRIME offload variables, see
# ON_NVIDIA in games.rs); measured: same cost for both either way.
# kwinoutputconfig.json (shared with Plasma) mirrors the TV and the laptop panel
# (eDP-1). DRM card numbers are not stable, so the cards are resolved from their PCI paths.
RUN="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
RETURN="$RUN/azototv-return"
# tv-mode.sh only logs out of Plasma if this loop is there to bring AzotoTV back
echo $$ >"$RUN/azototv-session.pid"
while :; do
  rm -f "$RETURN"
  KWIN_DRM_DEVICES=$(readlink -f /dev/dri/by-path/pci-0000:00:02.0-card):$(readlink -f /dev/dri/by-path/pci-0000:01:00.0-card) \
    kwin_wayland_wrapper --xwayland --no-lockscreen --no-global-shortcuts --no-kactivities \
    --exit-with-session "$PWD/kiosk.sh"

  # not exec: when Plasma ends, decide whether AzotoTV comes back
  startplasma-wayland
  [ -f "$RETURN" ] || exit 0
done
