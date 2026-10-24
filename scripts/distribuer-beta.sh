#!/bin/bash
set -euo pipefail

RACINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IDENTITE="${MURAL_DEVELOPER_ID_APPLICATION:-}"
PROFIL="${MURAL_NOTARY_PROFILE:-}"

if [[ -z "$IDENTITE" || -z "$PROFIL" ]]; then
  printf '%s\n' "Prerequis manquants : definir MURAL_DEVELOPER_ID_APPLICATION et MURAL_NOTARY_PROFILE." >&2
  exit 2
fi

if ! /usr/bin/security find-identity -v -p codesigning 2>/dev/null | /usr/bin/grep -Fq "$IDENTITE"; then
  printf '%s\n' "Identite de signature Developer ID introuvable dans le trousseau." >&2
  exit 2
fi

if [[ "$IDENTITE" != Developer\ ID\ Application:* ]]; then
  printf '%s\n' "L'identite doit etre un certificat Developer ID Application." >&2
  exit 2
fi

VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$RACINE/Info.plist")"
NUMERO="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$RACINE/Info.plist")"
SORTIE="$RACINE/target/distribution"
LIVRAISON_FINALE="$SORTIE/Mural-$VERSION"
LIVRAISON="$SORTIE/.Mural-$VERSION.incomplete.$$"

if [[ -e "$LIVRAISON_FINALE" ]]; then
  printf 'La livraison existe deja : %s\n' "$LIVRAISON_FINALE" >&2
  exit 2
fi

mkdir -p "$SORTIE"
"$RACINE/scripts/valider.sh"
cargo build --offline --release --manifest-path "$RACINE/Cargo.toml"
mkdir -p "$LIVRAISON/Mural.app/Contents/MacOS" \
  "$LIVRAISON/Mural.app/Contents/Resources" \
  "$LIVRAISON/DMG"
"$RACINE/scripts/archiver-source.sh" "$LIVRAISON"

cp "$RACINE/Info.plist" "$LIVRAISON/Mural.app/Contents/Info.plist"
cp "$RACINE/target/release/mural" "$LIVRAISON/Mural.app/Contents/MacOS/mural"
cp "$RACINE/Ressources/icones/mural.icns" "$LIVRAISON/Mural.app/Contents/Resources/mural.icns"
cp "$RACINE/LICENSE" "$RACINE/NOTICE.md" "$LIVRAISON/Mural.app/Contents/Resources/"
cp "$RACINE/PRIVACY.md" "$LIVRAISON/PRIVACY.md"

"$RACINE/scripts/embarquer-sparkle.sh" "$LIVRAISON/Mural.app" "$IDENTITE"

/usr/bin/codesign --force --options runtime --timestamp --sign "$IDENTITE" \
  "$LIVRAISON/Mural.app/Contents/MacOS/mural"
/usr/bin/codesign --force --options runtime --timestamp --sign "$IDENTITE" \
  "$LIVRAISON/Mural.app"
/usr/bin/codesign --verify --deep --strict --verbose=2 "$LIVRAISON/Mural.app"

cat > "$LIVRAISON/INSTALL.txt" <<EOF
Mural $VERSION (build $NUMERO) — beta

Installation : faites glisser Mural.app vers Applications.
Premiere ouverture : ouvrez Mural depuis Applications.
Retour arriere : quittez Mural, puis deplacez Mural.app hors du dossier Applications.
Les reglages et le cache restent conserves pour une prochaine installation.
Source correspondante GPL-3.0 : archive Mural-$VERSION-source.tar.gz et son fichier SHA-256, inclus dans ce DMG et a cote.
Confidentialite : lisez PRIVACY.md avant l'installation ; les journaux peuvent contenir des chemins de fichiers et des termes recherches.
Soutien facultatif : https://ko-fi.com/M4M51RKN7V
Pour signaler un probleme, repondez au message de beta-test avec le modele de retour fourni.

Cette beta ne promet pas les fonds d'ecran de l'ecran verrouille ni les Spaces.
EOF

ditto -c -k --sequesterRsrc --keepParent \
  "$LIVRAISON/Mural.app" "$LIVRAISON/Mural-$VERSION.zip"
xcrun notarytool submit "$LIVRAISON/Mural-$VERSION.zip" \
  --keychain-profile "$PROFIL" --wait
xcrun stapler staple "$LIVRAISON/Mural.app"
xcrun stapler validate "$LIVRAISON/Mural.app"
/usr/bin/spctl --assess --type execute --verbose=2 "$LIVRAISON/Mural.app"

cp -R "$LIVRAISON/Mural.app" "$LIVRAISON/DMG/Mural.app"
ln -s /Applications "$LIVRAISON/DMG/Applications"
cp "$LIVRAISON/INSTALL.txt" "$LIVRAISON/DMG/INSTALL.txt"
cp "$LIVRAISON/PRIVACY.md" "$LIVRAISON/DMG/PRIVACY.md"
cp "$LIVRAISON/Mural-$VERSION-source.tar.gz" "$LIVRAISON/Mural-$VERSION-source.tar.gz.sha256" "$LIVRAISON/DMG/"
hdiutil create -volname "Mural $VERSION" -srcfolder "$LIVRAISON/DMG" \
  -format UDZO "$LIVRAISON/Mural-$VERSION.dmg"
xcrun notarytool submit "$LIVRAISON/Mural-$VERSION.dmg" \
  --keychain-profile "$PROFIL" --wait
xcrun stapler staple "$LIVRAISON/Mural-$VERSION.dmg"
xcrun stapler validate "$LIVRAISON/Mural-$VERSION.dmg"
shasum -a 256 "$LIVRAISON/Mural-$VERSION.dmg" > "$LIVRAISON/Mural-$VERSION.dmg.sha256"

mv "$LIVRAISON" "$LIVRAISON_FINALE"
printf '\nLivraison locale prete : %s\n' "$LIVRAISON_FINALE"
