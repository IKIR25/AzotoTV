#!/bin/sh
# AzotoTV login session (plasmalogin autologin, Session=azototv.desktop).
# KWin runs on its own with AzotoTV as its only client: no Plasma shell, panels,
# lock screen or desktop services. When AzotoTV ends the session ("Esci al
# desktop"), the normal Plasma desktop starts in this same login. The "AzotoTV"
# entry in Plasma's menu (tv-mode.sh) logs out of Plasma with a flag set, and
# then the AzotoTV session comes back; a plain logout ends the login as before.
cd "$(dirname "$(readlink -f "$0")")"

# ~/.config/azototv/session.env (optional, a shell file) sets what this
# machine needs before KWin starts. The case it was made for: a laptop with two
# GPUs, where KWin should composite on the integrated one (the first device in
# KWIN_DRM_DEVICES is the primary one), so the menus are drawn there and the
# other GPU only sends the picture to the TV. With KWin on an NVIDIA GTX 1060,
# every focus move or slide woke it to full clocks (~23 W instead of ~6 W) for
# 10-35 s; films and games still run on it (see gpu_env in games.rs). DRM card
# numbers are not stable, so resolve the cards from their PCI paths:
#   export KWIN_DRM_DEVICES=$(readlink -f /dev/dri/by-path/pci-0000:00:02.0-card):$(readlink -f /dev/dri/by-path/pci-0000:01:00.0-card)
# See examples/session.env. Without the file KWin picks for itself.
ENV="${XDG_CONFIG_HOME:-$HOME/.config}/azototv/session.env"
[ -f "$ENV" ] && . "$ENV"
RUN="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
RETURN="$RUN/azototv-return"
# tv-mode.sh only logs out of Plasma if this loop is there to bring AzotoTV back
echo $$ >"$RUN/azototv-session.pid"
while :; do
  rm -f "$RETURN"
  kwin_wayland_wrapper --xwayland --no-lockscreen --no-global-shortcuts --no-kactivities \
    --exit-with-session "$PWD/kiosk.sh"

  # not exec: when Plasma ends, decide whether AzotoTV comes back
  startplasma-wayland
  [ -f "$RETURN" ] || exit 0
done
