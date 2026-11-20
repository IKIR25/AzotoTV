#!/bin/sh
# Sunshine global prep command (PC side of "Play from PC").
#   stream-prep.sh do    virtual screen at the client's size, real monitors off
#   stream-prep.sh undo  real monitors back, virtual screen gone
#   (do also starts "stream-prep.sh watch": closes the session if the client
#   left more than 2 minutes ago without coming back)
# Sunshine passes SUNSHINE_CLIENT_WIDTH / SUNSHINE_CLIENT_HEIGHT.
# Real monitors go off so every window (games included) opens on the virtual
# screen; their names are saved so undo turns the same ones back on.
name=AzotoTV
saved=${XDG_RUNTIME_DIR:-/tmp}/azoto-vscreen-monitors
vscreen=$HOME/.local/bin/azoto-vscreen

outputs() { # "name enabled|disabled" for each connected output except ours
    kscreen-doctor -o 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | awk -v v="Virtual-$name" '
        /^Output:/ { if (n != "") print n, s; n = $3; s = "disabled"; next }
        /^\tenabled$/ { s = "enabled" }
        END { if (n != "") print n, s }' | grep -v "^Virtual-$name "
}

watchdog=${XDG_RUNTIME_DIR:-/tmp}/azoto-vscreen-watchdog.pid
log=$HOME/.config/sunshine/sunshine.log
grace=120

# A client that leaves doesn't end Sunshine's session (it waits for a resume),
# so undo would never run and the monitor would stay off. If the last client
# event is a disconnect older than $grace seconds, close the app ourselves.
watch_disconnect() {
    while sleep 5; do
        last=$(tail -n 400 "$log" 2>/dev/null | grep -E "CLIENT (CONNECTED|DISCONNECTED)" | tail -n 1)
        case "$last" in
        *DISCONNECTED*)
            at=$(date -d "$(printf '%s' "$last" | sed 's/^\[\([^]]*\)\].*/\1/')" +%s 2>/dev/null) || continue
            if [ $(( $(date +%s) - at )) -ge $grace ]; then
                curl -sk -m 10 -u "azototv:$(cat "$HOME/.config/sunshine/webui-pass")" \
                    -X POST https://localhost:47990/api/apps/close >/dev/null
                exit 0
            fi
            ;;
        esac
    done
}

stop_watchdog() {
    [ -s "$watchdog" ] || return 0
    pid=$(cat "$watchdog")
    [ "$pid" != "$$" ] && grep -q "azoto-stream-prep" "/proc/$pid/cmdline" 2>/dev/null && kill "$pid"
    rm -f "$watchdog"
}

case "$1" in
watch)
    echo $$ > "$watchdog"
    watch_disconnect
    ;;
do)
    w=${SUNSHINE_CLIENT_WIDTH:-1920}
    h=${SUNSHINE_CLIENT_HEIGHT:-1080}
    "$vscreen" start "${w}x${h}" "$name" || exit 1
    on=$(outputs | awk '$2 == "enabled" { print $1 }')
    [ -s "$saved" ] || printf '%s\n' $on > "$saved"
    args=""
    for o in $on; do args="$args output.$o.disable"; done
    [ -n "$args" ] && kscreen-doctor $args >/dev/null 2>&1
    stop_watchdog
    setsid "$0" watch </dev/null >/dev/null 2>&1 &
    exit 0
    ;;
undo)
    stop_watchdog
    args=""
    if [ -s "$saved" ]; then
        for o in $(cat "$saved"); do args="$args output.$o.enable"; done
    else
        # nothing saved: turn on everything that's connected
        for o in $(outputs | awk '{ print $1 }'); do args="$args output.$o.enable"; done
    fi
    [ -n "$args" ] && kscreen-doctor $args >/dev/null 2>&1
    rm -f "$saved"
    "$vscreen" stop "$name"
    exit 0
    ;;
*)
    echo "usage: $0 do|undo" >&2
    exit 2
    ;;
esac
