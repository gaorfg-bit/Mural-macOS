#!/bin/bash
set -euo pipefail
RACINE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$RACINE"
TEMP="$(mktemp -d /private/tmp/mural-updates-test.XXXXXX)"
APP="$TEMP/MuralUpdateTest.app"
cargo build --offline --bin recette_mises_a_jour
mkdir -p "$APP/Contents/MacOS"
cp target/debug/recette_mises_a_jour "$APP/Contents/MacOS/mural"
cp Info.plist "$APP/Contents/Info.plist"
# Verifier les vrais choix de distribution avant d'isoler le bundle de recette.
python3 - "$APP/Contents/Info.plist" <<'PY'
import base64, plistlib, sys, uuid
p=sys.argv[1]
with open(p,'rb') as f: info=plistlib.load(f)
assert info['SUEnableAutomaticChecks'] is True
assert info['SUScheduledCheckInterval'] == 86400
assert info['SUAutomaticallyUpdate'] is False
assert info['SUVerifyUpdateBeforeExtraction'] is True
assert info['SUEnableSystemProfiling'] is False
assert len(base64.b64decode(info['SUPublicEDKey'], validate=True)) == 32
info['SUEnableAutomaticChecks']=False
info['CFBundleIdentifier']='com.faux.mural.update-test.'+uuid.uuid4().hex
with open(p,'wb') as f: plistlib.dump(info,f)
PY
./scripts/embarquer-sparkle.sh "$APP" -
codesign --force --sign - "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
MURAL_REGLAGES="$TEMP/reglages.json" MURAL_JOURNAL="$TEMP/journal.log" "$APP/Contents/MacOS/mural"
printf 'Recette conservee : %s\n' "$TEMP"
