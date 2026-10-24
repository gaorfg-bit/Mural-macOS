#!/bin/bash
set -euo pipefail

RACINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESTINATION="${1:?Usage: archiver-source.sh <dossier-sortie>}"
VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$RACINE/Info.plist")"
NOM="$(basename "$RACINE")"
PARENT="$(dirname "$RACINE")"
ARCHIVE="$DESTINATION/Mural-$VERSION-source.tar.gz"
EMPREINTE="$ARCHIVE.sha256"
TEMP="$DESTINATION/.Mural-$VERSION-source.$$.tar.gz"

if [[ -e "$ARCHIVE" || -e "$EMPREINTE" ]]; then
  printf 'Archive source deja presente dans %s\n' "$DESTINATION" >&2
  exit 2
fi

mkdir -p "$DESTINATION"
trap 'rm -f "$TEMP"' EXIT

ENTREES=(
  "$NOM/Cargo.toml"
  "$NOM/Cargo.lock"
  "$NOM/Info.plist"
  "$NOM/LICENSE"
  "$NOM/NOTICE.md"
  "$NOM/PRIVACY.md"
  "$NOM/Makefile"
  "$NOM/README.md"
  "$NOM/src"
  "$NOM/Ressources"
  "$NOM/scripts/valider.sh"
  "$NOM/scripts/distribuer-beta.sh"
  "$NOM/scripts/test-mises-a-jour.sh"
  "$NOM/scripts/preparer-sparkle.sh"
  "$NOM/scripts/embarquer-sparkle.sh"
  "$NOM/scripts/preparer-mise-a-jour.sh"
  "$NOM/scripts/archiver-source.sh"
  "$NOM/scripts/test-source-archive.sh"
)

tar -czf "$TEMP" -C "$PARENT" "${ENTREES[@]}"
mv "$TEMP" "$ARCHIVE"
shasum -a 256 "$ARCHIVE" > "$EMPREINTE"
printf 'Archive source prete : %s\n' "$ARCHIVE"
