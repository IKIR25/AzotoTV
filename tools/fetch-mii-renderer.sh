#!/bin/sh
# Puts the Mii renderer in place (it is not in the repo): run once before
# building, and again after changing the versions below.
#
# - FFL.js by Arian Kordi (AGPL-3.0-only): draws Miis with the Wii U's own Mii
#   library (FFL, compiled to WebAssembly). Only the files the app uses go to
#   src/lib/vendor/ffl/, the WebAssembly module to static/mii/ffl-emscripten.js.
#   A build that includes it has to follow the AGPL.
# - The Mii bodies miijs ships (ISC), to static/mii/.
#
# Without them the app builds all the same, with a Mii app that can't draw.
# Drawing also needs the Wii U's FFLResHigh.dat in the data folder (see
# src-tauri/src/mii.rs), which this script does not fetch.
set -eu
FFL_COMMIT=3a1236753eba69c9260bfc46f840079cff75019c # FFL.js 2.2.0, the one miijs 3.1.0 pins
MIIJS=3.1.0

root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

git init -q "$tmp/ffl"
git -C "$tmp/ffl" fetch -q --depth 1 https://github.com/ariankordi/FFL.js "$FFL_COMMIT"
git -C "$tmp/ffl" -c advice.detachedHead=false checkout -q FETCH_HEAD

dst=$root/src/lib/vendor/ffl
mkdir -p "$dst/helpers" "$dst/materials" "$root/static/mii"
cp "$tmp/ffl/LICENSE" "$dst/LICENSE"
for f in ffl.js helpers/BodyUtilities.js helpers/ModelScaleDesc.js helpers/SkeletonScalingExtensions.js \
  materials/FFLShaderMaterial.js materials/SampleShaderMaterial.js; do
  # the app's type check doesn't cover someone else's JavaScript
  { echo '// @ts-nocheck'; grep -v -x '// @ts-check' "$tmp/ffl/$f"; } >"$dst/$f"
done
cp "$tmp/ffl/examples/ffl-emscripten-single-file.cjs" "$root/static/mii/ffl-emscripten.js"

curl -fsSL -o "$tmp/miijs.tgz" "https://registry.npmjs.org/miijs/-/miijs-$MIIJS.tgz"
tar -xzf "$tmp/miijs.tgz" -C "$tmp" package/miiMaleBody.glb package/miiFemaleBody.glb
cp "$tmp/package/miiMaleBody.glb" "$tmp/package/miiFemaleBody.glb" "$root/static/mii/"

echo "FFL.js $FFL_COMMIT -> src/lib/vendor/ffl, static/mii"
