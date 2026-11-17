#!/bin/sh
# Builds AzotoTV.apk with the SDK's command-line tools alone (no Gradle):
# aapt2 -> javac -> d8 -> zipalign -> apksigner. The result is copied next to
# the phone page (src-tauri/phone/AzotoTV.apk): AzotoTV serves it, so the
# family downloads it from the TV itself.
#
#   BUILD_TOOLS  default /opt/android-sdk/build-tools/37.0.0
#   ANDROID_JAR  default ~/.local/share/android-sdk/platforms/android-36/android.jar
#   KEYSTORE     default ~/.android/debug.keystore (password "android"); keep using
#                the same one, or phones must uninstall the app before an update
#   DEBUG=1      a debuggable build (its web view can be inspected over USB); not for phones
#                that keep it: the page is copied next to the phone page only for release builds
set -eu
cd "$(dirname "$0")"

BT=${BUILD_TOOLS:-/opt/android-sdk/build-tools/37.0.0}
JAR=${ANDROID_JAR:-$HOME/.local/share/android-sdk/platforms/android-36/android.jar}
KS=${KEYSTORE:-$HOME/.android/debug.keystore}
VERSION_CODE=6
VERSION_NAME=1.5

rm -rf build
mkdir -p build/gen build/classes build/dex

"$BT/aapt2" compile --dir res -o build/res.zip
"$BT/aapt2" link -o build/base.apk -I "$JAR" --manifest AndroidManifest.xml -R build/res.zip --auto-add-overlay \
  --java build/gen --min-sdk-version 24 --target-sdk-version 36 \
  --version-code "$VERSION_CODE" --version-name "$VERSION_NAME" ${DEBUG:+--debug-mode}
javac --release 17 -nowarn -classpath "$JAR" -d build/classes $(find src build/gen -name '*.java')
"$BT/d8" --release --min-api 24 --lib "$JAR" --output build/dex $(find build/classes -name '*.class')
python3 -c 'import sys, zipfile; zipfile.ZipFile(sys.argv[1], "a", zipfile.ZIP_DEFLATED).write(sys.argv[2], "classes.dex")' build/base.apk build/dex/classes.dex
"$BT/zipalign" -f -p 4 build/base.apk build/aligned.apk
"$BT/apksigner" sign --ks "$KS" --ks-pass pass:android --key-pass pass:android --out build/AzotoTV.apk build/aligned.apk
"$BT/apksigner" verify build/AzotoTV.apk
if [ -n "${DEBUG:-}" ]; then ls -l build/AzotoTV.apk; exit; fi
cp build/AzotoTV.apk ../src-tauri/phone/AzotoTV.apk
ls -l ../src-tauri/phone/AzotoTV.apk
