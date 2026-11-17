#!/bin/sh
# The only client of the AzotoTV KWin session (see session.sh): AzotoTV full
# screen, started again if it crashes or is killed (a redeploy is just
# `pkill -x azototv`). A clean exit ("Esci al desktop") ends the session, and so
# does a crash loop, so the TV falls back to the Plasma desktop either way.
cd "$(dirname "$(readlink -f "$0")")"
dbus-update-activation-environment --systemd WAYLAND_DISPLAY DISPLAY XDG_SESSION_TYPE XDG_CURRENT_DESKTOP
# game streaming (stream.rs), where Sunshine is installed: its unit wants
# graphical-session.target, which this session never reaches. Config:
# examples/sunshine.conf, examples/sunshine-apps.json.
for unit in app-dev.lizardbyte.app.Sunshine.service sunshine.service; do
  systemctl --user start "$unit" 2>/dev/null && break
done

export AZOTOTV_FULLSCREEN=1
fails=0
while :; do
  [ -f run.log ] && mv -f run.log run.prev.log
  start=$(date +%s)
  ./azototv >run.log 2>&1 && exit 0
  if [ $(($(date +%s) - start)) -lt 30 ]; then fails=$((fails + 1)); else fails=0; fi
  [ "$fails" -ge 3 ] && exit 1
  sleep 1
done
