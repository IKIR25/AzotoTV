#!/bin/sh
# AzotoTV full screen inside the desktop session that is running (what the
# "AzotoTV" entry of the desktop's menu starts when the machine didn't boot
# into AzotoTV's own session: see tv-mode.sh). Change it freely: install.sh
# never overwrites it.
cd "$(dirname "$(readlink -f "$0")")"
export AZOTOTV_FULLSCREEN=1
exec ./azototv "$@"
