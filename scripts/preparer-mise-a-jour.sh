#!/bin/bash
# Preparation LOCALE seulement. Ne publie rien sur GitHub.
set -euo pipefail
RACINE="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:?Usage: preparer-mise-a-jour.sh <Mural.app> <nouveau-dossier-sortie>}"
SORTIE="${2:?Nouveau dossier requis}"
[[ -d "$APP" ]]
[[ ! -e "$SORTIE" ]] || { echo 'Le dossier de sortie existe deja.' >&2; exit 2; }
VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo 'Version invalide' >&2; exit 2; }
codesign --verify --deep --strict --verbose=2 "$APP"
# Refuser un bundle sans le moteur ou sans la bonne cle de signature.
[[ -d "$APP/Contents/Frameworks/Sparkle.framework" ]]
CLE="$(/usr/libexec/PlistBuddy -c 'Print :SUPublicEDKey' "$APP/Contents/Info.plist")"
[[ "$CLE" == "$("$RACINE/vendor/sparkle/bin/generate_keys" --account com.faux.mural -p)" ]] || { echo 'La cle du bundle ne correspond pas au trousseau Mural.' >&2; exit 2; }
mkdir -p "$SORTIE"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$SORTIE/Mural-$VERSION.zip"
"$RACINE/vendor/sparkle/bin/generate_appcast" --account com.faux.mural \
  --download-url-prefix "https://github.com/gaorfg-bit/homebrew-mural/releases/download/v$VERSION/" \
  --link 'https://github.com/gaorfg-bit/homebrew-mural' --maximum-deltas 0 "$SORTIE"
printf 'Preparation locale terminee : %s (ZIP signe + appcast.xml). Rien de publie.\n' "$SORTIE"
