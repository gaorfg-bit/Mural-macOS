#!/bin/bash
set -euo pipefail

RACINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NOM="$(basename "$RACINE")"
VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$RACINE/Info.plist")"
TEMP="$(mktemp -d /private/tmp/mural-source-archive-test.XXXXXX)"
trap 'rm -rf "$TEMP"' EXIT

"$RACINE/scripts/archiver-source.sh" "$TEMP"
ARCHIVE="$TEMP/Mural-$VERSION-source.tar.gz"
[[ -f "$ARCHIVE" ]]
[[ -f "$ARCHIVE.sha256" ]]
read -r empreinte nom_archive < "$ARCHIVE.sha256"
if [[ "$nom_archive" != "Mural-$VERSION-source.tar.gz" ]]; then
  printf 'Empreinte non portable : %s\n' "$nom_archive" >&2
  exit 1
fi

LISTE="$TEMP/contenu.txt"
tar -tzf "$ARCHIVE" > "$LISTE"
python3 - "$ARCHIVE" <<'PYARCHIVE'
import sys, tarfile
with tarfile.open(sys.argv[1]) as archive:
    for entree in archive.getmembers():
        nom = entree.name.rsplit('/', 1)[-1]
        assert not nom.startswith('._') and nom != '.DS_Store', \
            f'Archive source contient des metadonnees Finder : {entree.name}'
PYARCHIVE
for chemin in \
  "$NOM/Cargo.toml" \
  "$NOM/Cargo.lock" \
  "$NOM/Info.plist" \
  "$NOM/LICENSE" \
  "$NOM/NOTICE.md" \
  "$NOM/PRIVACY.md" \
  "$NOM/Makefile" \
  "$NOM/assets/logo.png" \
  "$NOM/assets/Mural.png" \
  "$NOM/docs/MISES-A-JOUR.md" \
  "$NOM/src/" \
  "$NOM/tests/avif.rs" \
  "$NOM/tests/avif_conversion.rs" \
  "$NOM/Ressources/mural-wordmark.png" \
  "$NOM/Ressources/icones/mural.icns"; do
  rg -Fqx "$chemin" "$LISTE" || {
    printf 'Archive source incomplete : %s\n' "$chemin" >&2
    exit 1
  }
done

if rg -q "^${NOM}/(target|\\.git|\\.codex)/" "$LISTE"; then
  printf '%s\n' 'Archive source contient des fichiers locaux ou de construction.' >&2
  exit 1
fi

mkdir "$TEMP/extrait"
tar -xzf "$ARCHIVE" -C "$TEMP/extrait"
(cd "$TEMP" && shasum -a 256 -c "$(basename "$ARCHIVE.sha256")")
cargo metadata --offline --locked --no-deps --format-version 1 --manifest-path "$TEMP/extrait/$NOM/Cargo.toml" > "$TEMP/metadata.json"
python3 - "$TEMP/metadata.json" <<'PYMETA'
import json, sys
with open(sys.argv[1]) as f: paquet=json.load(f)['packages'][0]
for nom in ['avif', 'avif_conversion']:
    assert any(t['name']==nom and 'test' in t['kind'] for t in paquet['targets']), nom
PYMETA

printf 'OK : archive source %s (%s entrees)\n' "$ARCHIVE" "$(wc -l < "$LISTE" | tr -d ' ')"
