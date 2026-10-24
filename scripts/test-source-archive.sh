#!/bin/bash
set -euo pipefail

RACINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$RACINE/Info.plist")"
TEMP="$(mktemp -d /private/tmp/mural-source-archive-test.XXXXXX)"
trap 'rm -rf "$TEMP"' EXIT

"$RACINE/scripts/archiver-source.sh" "$TEMP"
ARCHIVE="$TEMP/Mural-$VERSION-source.tar.gz"
[[ -f "$ARCHIVE" ]]
[[ -f "$ARCHIVE.sha256" ]]

LISTE="$TEMP/contenu.txt"
tar -tzf "$ARCHIVE" > "$LISTE"
for chemin in \
  "Mural/Cargo.toml" \
  "Mural/Cargo.lock" \
  "Mural/Info.plist" \
  "Mural/LICENSE" \
  "Mural/NOTICE.md" \
  "Mural/PRIVACY.md" \
  "Mural/Makefile" \
  "Mural/src/" \
  "Mural/Ressources/mural-wordmark.png" \
  "Mural/Ressources/icones/mural.icns"; do
  rg -Fqx "$chemin" "$LISTE" || {
    printf 'Archive source incomplete : %s\n' "$chemin" >&2
    exit 1
  }
done

if rg -q '^Mural/(target|\.git|docs|\.codex)/' "$LISTE"; then
  printf '%s\n' 'Archive source contient des fichiers locaux ou de construction.' >&2
  exit 1
fi

printf 'OK : archive source %s (%s entrees)\n' "$ARCHIVE" "$(wc -l < "$LISTE" | tr -d ' ')"
