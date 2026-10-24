#!/bin/bash
# Acquisition explicite, version et empreinte epinglees. Aucun reseau au build Cargo.
set -euo pipefail
RACINE="$(cd "$(dirname "$0")/.." && pwd)"
VERSION=2.10.0
SHA=c2bf58aa8387266ac179357b1415d6f2635f044da8be41042af32425dae6da0c
DEST="$RACINE/vendor/sparkle"
if [[ -e "$DEST/Sparkle.framework" ]]; then
  [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$DEST/Sparkle.framework/Resources/Info.plist")" == "$VERSION" ]]
  codesign --verify --deep --strict --verbose=2 "$DEST/Sparkle.framework"
  exit 0
fi
TEMP="$(mktemp -d /private/tmp/mural-sparkle.XXXXXX)"
curl --fail --location --proto '=https' --tlsv1.2 \
  "https://github.com/sparkle-project/Sparkle/releases/download/$VERSION/Sparkle-$VERSION.tar.xz" -o "$TEMP/Sparkle.tar.xz"
printf '%s  %s\n' "$SHA" "$TEMP/Sparkle.tar.xz" | shasum -a 256 -c -
mkdir -p "$DEST"
tar -xf "$TEMP/Sparkle.tar.xz" -C "$DEST"
codesign --verify --deep --strict --verbose=2 "$DEST/Sparkle.framework"
