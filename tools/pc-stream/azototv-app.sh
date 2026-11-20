#!/bin/sh
# Sunshine app "AzotoTV" (PC side): this computer's AzotoTV, full screen on the
# virtual screen stream-prep.sh made, played from the TV. Closing it (START ›
# Close AzotoTV) ends the stream and the TV goes back to its own AzotoTV.
bin=${AZOTOTV_BIN:-$HOME/azototv/azototv}

# one AzotoTV at a time (they'd share the phone port and the linked-TV API):
# a window left open on the monitor, now off, closes
pids=$(pgrep -x azototv)
if [ -n "$pids" ]; then
    kill $pids
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        pgrep -x azototv >/dev/null || break
        sleep 0.5
    done
fi

AZOTOTV_STREAMED=1 exec "$bin"
