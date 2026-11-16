#!/bin/bash
# Puts the gbe_fork emulator's steam_api libraries in $EMU (default ~/azototv/steam-emu)
# for install.sh. Pinned release; bump RELEASE to update (then re-run install.sh per game).
set -euo pipefail
RELEASE=release-2026_09_27
EMU=${EMU:-$HOME/azototv/steam-emu}
url=https://github.com/Detanup01/gbe_fork/releases/download/$RELEASE
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
curl -fsSL -o "$tmp/linux.tar.bz2" "$url/emu-linux-release.tar.bz2"
curl -fsSL -o "$tmp/win.7z" "$url/emu-win-release-vs22.7z"
mkdir -p "$tmp/l" "$tmp/w"
tar -xjf "$tmp/linux.tar.bz2" -C "$tmp/l"
7z x -o"$tmp/w" "$tmp/win.7z" >/dev/null
mkdir -p "$EMU"/linux/{x64,x86} "$EMU"/win/{x64,x86}
cp "$tmp/l/release/regular/x64/libsteam_api.so" "$EMU/linux/x64/"
cp "$tmp/l/release/regular/x86/libsteam_api.so" "$EMU/linux/x86/"
cp "$tmp/w/release/regular/x64/steam_api64.dll" "$EMU/win/x64/"
cp "$tmp/w/release/regular/x86/steam_api.dll" "$EMU/win/x86/"
echo "$RELEASE" > "$EMU/RELEASE"
echo "gbe_fork $RELEASE -> $EMU"
