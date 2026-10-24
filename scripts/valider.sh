#!/bin/bash
# Validation locale : aucune application de fond, installation ou suppression.
set -euo pipefail
projet="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$projet"
preuves="$(mktemp -d /private/tmp/mural-validation.XXXXXX)"
export CARGO_TARGET_DIR="$projet/target"
export MURAL_REGLAGES="$preuves/bibliotheque/reglages.json"
export MURAL_CACHE="$preuves/vignettes"
export MURAL_AVIF_CACHE="$preuves/menus/avif"
mkdir -p "$preuves/bibliotheque" "$preuves/menus" "$preuves/panneau"
terminer() {
    resultat=$?
    printf '%s\n' "$resultat" > "$preuves/code-sortie.txt"
    printf '\nCode de sortie : %s. Preuves conservees : %s\n' "$resultat" "$preuves"
}
trap terminer EXIT
printf 'Preuves : %s\n' "$preuves"
printf '\n1/7 — Tests Cargo hors ligne\n'
cargo test --offline --locked --verbose 2>&1 | tee "$preuves/cargo-test.log"
printf '\n2/7 — Compilation de la recette\n'
cargo build --offline --locked --verbose --bin recette_bibliotheque --bin recette_interface 2>&1 | tee "$preuves/cargo-build.log"
printf '\n3/7 — Bibliotheque puis restauration dans un second processus\n'
"$projet/target/debug/recette_bibliotheque" 2>&1 | tee "$preuves/bibliotheque.log"
"$projet/target/debug/recette_bibliotheque" --restauration 2>&1 | tee "$preuves/restauration.log"
printf '\n4/7 — Menus et conversion AVIF en cache temporaire\n'
MURAL_REGLAGES="$preuves/menus/reglages.json" "$projet/target/debug/recette_bibliotheque" --menus 2>&1 | tee "$preuves/menus-avif.log"
if rg -q 'NON VERIFIE : conversion du panneau' "$preuves/menus-avif.log"; then
    printf 'ECHEC : conversion omise, verifier le perimetre du cache AVIF.\n' >&2
    exit 1
fi
printf '\n5/7 — Panneau, progression, apercu et cycle de vie\n'
MURAL_REGLAGES="$preuves/panneau/reglages.json" "$projet/target/debug/recette_bibliotheque" --panneau 2>&1 | tee "$preuves/panneau.log"
printf '\n6/7 — Composition epuree et apparences natives\n'
MURAL_REGLAGES="$preuves/interface/reglages.json" "$projet/target/debug/recette_interface" 2>&1 | tee "$preuves/interface.log"
printf '\n7/7 — Archive source GPL et empreinte\n'
"$projet/scripts/test-source-archive.sh" 2>&1 | tee "$preuves/source-archive.log"
printf '\nValidation terminee. Les NON VERIFIE eventuels restent des limites explicites.\n'
