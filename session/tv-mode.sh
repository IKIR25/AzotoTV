#!/bin/sh
# "AzotoTV" in Plasma's menu: back to the AzotoTV-only session. Plasma is
# logged out (no confirmation) and session.sh, seeing the flag, starts the
# AzotoTV session again, so no desktop is left running behind it. In a
# Plasma not started by session.sh's loop, a logout would leave the TV on the
# login screen: then AzotoTV just opens as a window, as before.
RUN="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
pid=$(cat "$RUN/azototv-session.pid" 2>/dev/null)
if [ -n "$pid" ] && grep -q 'session.sh' "/proc/$pid/cmdline" 2>/dev/null; then
  touch "$RUN/azototv-return"
  exec qdbus6 org.kde.Shutdown /Shutdown org.kde.Shutdown.logout
fi
exec "$(dirname "$(readlink -f "$0")")/run-tv.sh"
