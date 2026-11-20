#!/bin/sh
# TV side: a "game" that streams a computer's AzotoTV (Sunshine app "AzotoTV",
# see azototv-app.sh) to the TV with Moonlight. Its game.json:
#   {"name": "…", "type": "native", "exe": "play.sh", "players": 4,
#    "pad": {"mode": "stream"}, "env": {"AZOTO_STREAM_HOST": "<the computer's address>"}}
# AzotoTV gives Moonlight the pads as for any game; "stream" passes Home on, so
# holding it closes the game on the computer, not the stream. The stream ends
# when the computer's AzotoTV closes, or with Moonlight's Back+Start+LB+RB.
host=${AZOTO_STREAM_HOST:?AZOTO_STREAM_HOST: the computer to stream from}
app=${AZOTO_STREAM_APP:-AzotoTV}
size=${AZOTO_STREAM_SIZE:-1920x1080}
bitrate=${AZOTO_STREAM_KBPS:-20000}
moonlight=com.moonlight_stream.Moonlight

# a game's HOME is the player's folder: Moonlight's pairing is the real user's
HOME=$(getent passwd "$(id -u)" | cut -d: -f6)
export HOME
unset XDG_CONFIG_HOME XDG_DATA_HOME XDG_CACHE_HOME
# games are sent to the NVIDIA; decoding a stream is a job for the Intel GPU (cool, quiet)
unset __NV_PRIME_RENDER_OFFLOAD __VK_LAYER_NV_optimus __GLX_VENDOR_LIBRARY_NAME DRI_PRIME \
    __EGL_VENDOR_LIBRARY_FILENAMES VK_ICD_FILENAMES VK_DRIVER_FILES

# Sunshine keeps a session when its client leaves (waiting for a resume): end it.
# Own session, as AzotoTV kills this script's process group when it ends.
quit_app() {
    setsid flatpak run "$moonlight" quit "$host" </dev/null >/dev/null 2>&1 &
}
# Home / AzotoTV closing the game: killing `flatpak run` leaves Moonlight running
stop() {
    flatpak kill "$moonlight" 2>/dev/null
    quit_app
    exit 0
}
trap stop TERM INT HUP

flatpak run "$moonlight" stream "$host" "$app" \
    --resolution "$size" --fps 60 --bitrate "$bitrate" \
    --display-mode fullscreen --video-codec HEVC --video-decoder hardware \
    --quit-after &
wait $!
quit_app
