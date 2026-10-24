#!/bin/bash
# Signature de l'interieur vers l'exterieur, sans --deep a la signature.
set -euo pipefail
RACINE="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:?Usage: embarquer-sparkle.sh <app> <identite>}"
IDENTITE="${2:?Identite requise}"
SOURCE="$RACINE/vendor/sparkle/Sparkle.framework"
[[ -d "$SOURCE" ]] || { echo 'Sparkle absent : lancer make sparkle (reseau explicite).' >&2; exit 2; }
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$SOURCE/Resources/Info.plist")" == 2.10.0 ]]
codesign --verify --deep --strict --verbose=2 "$SOURCE"
mkdir -p "$APP/Contents/Frameworks" "$APP/Contents/Resources"
ditto "$SOURCE" "$APP/Contents/Frameworks/Sparkle.framework"
cp "$RACINE/vendor/sparkle/LICENSE" "$APP/Contents/Resources/Sparkle-LICENSE.txt"
FRAMEWORK="$APP/Contents/Frameworks/Sparkle.framework"
OPTIONS=(--timestamp=none)
if [[ "$IDENTITE" == Developer\ ID\ Application:* ]]; then
  OPTIONS=(--options runtime --timestamp)
fi
for ELEMENT in \
  "$FRAMEWORK/Versions/B/XPCServices/Downloader.xpc" \
  "$FRAMEWORK/Versions/B/XPCServices/Installer.xpc" \
  "$FRAMEWORK/Versions/B/Autoupdate" \
  "$FRAMEWORK/Versions/B/Updater.app" \
  "$FRAMEWORK"; do
  codesign --force "${OPTIONS[@]}" --sign "$IDENTITE" "$ELEMENT"
done
codesign --verify --deep --strict --verbose=2 "$FRAMEWORK"
