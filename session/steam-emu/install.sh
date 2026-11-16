#!/bin/bash
# Lets a game that needs Steam run without it: swaps its steam_api library for
# the gbe_fork emulator's (the original is kept as .orig) and sets the emulator up.
#
#   install.sh <library entry, e.g. ~/Games/plague-inc> <steam appid> [achievements dir]
#
# The achievements dir is what schema2gbe.py writes (achievements.json,
# stats.json, images/; dlcs.py adds configs.app.ini, the DLCs owned). Everything goes into <entry>/steam_settings; the game
# gets a steam_settings symlink to it next to each steam_api library, and
# AzotoTV (steamemu.rs) sees the folder and knows the game uses the emulator.
# The emulator's libraries come from $EMU (default ~/azototv/steam-emu, see fetch.sh).
# Undo: move each .orig back and delete the steam_settings links and folder.
set -euo pipefail
entry=$(realpath "$1") appid=$2 ach=${3:-}
EMU=${EMU:-$HOME/azototv/steam-emu}
[ -d "$entry/game" ] || { echo "no game/ in $entry"; exit 1; }
for f in linux/x64/libsteam_api.so linux/x86/libsteam_api.so win/x64/steam_api64.dll win/x86/steam_api.dll; do
    [ -f "$EMU/$f" ] || { echo "missing $EMU/$f (run fetch.sh)"; exit 1; }
done

s=$entry/steam_settings
mkdir -p "$s"
echo "$appid" > "$s/steam_appid.txt"
# the player's name and Steam id are written by AzotoTV into each player's own saves folder
cat > "$s/configs.user.ini" <<EOF
[user::general]
language=italian
ip_country=IT
EOF
cat > "$s/configs.main.ini" <<EOF
[main::connectivity]
# no LAN broadcasts: one TV, nobody to find on the network
disable_networking=1
EOF
[ -n "$ach" ] && cp -r "$ach"/. "$s/"

n=0
while IFS= read -r -d '' lib; do
    case $(basename "$lib") in
        steam_api.dll) src=win/x86/steam_api.dll ;;
        steam_api64.dll) src=win/x64/steam_api64.dll ;;
        libsteam_api.so)
            orig=$lib; [ -e "$lib.orig" ] && orig=$lib.orig
            if file -L "$orig" | grep -q 'ELF 32-bit'; then src=linux/x86/libsteam_api.so; else src=linux/x64/libsteam_api.so; fi ;;
    esac
    [ -e "$lib.orig" ] || mv "$lib" "$lib.orig"
    cp "$EMU/$src" "$lib"
    d=$(dirname "$lib")
    rm -rf "${d:?}/steam_settings"
    ln -s "$(realpath --relative-to="$d" "$s")" "$d/steam_settings"
    echo "emulator: ${lib#$entry/} ($src)"
    n=$((n + 1))
done < <(find "$entry/game" \( -name steam_api.dll -o -name steam_api64.dll -o -name libsteam_api.so \) -print0)
[ $n -gt 0 ] || { echo "no steam_api library found"; exit 1; }
echo "steam_settings: $(find "$s" -name '*.json' | wc -l) json, $(find "$s/images" -type f 2>/dev/null | wc -l) icons"
