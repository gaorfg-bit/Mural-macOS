#!/bin/bash
# Validation locale : aucune application de fond ni installation du bundle principal.
set -euo pipefail
projet="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$projet"
preuves="$(mktemp -d /private/tmp/mural-validation.XXXXXX)"
export CARGO_TARGET_DIR="$projet/target"
export MURAL_REGLAGES="$preuves/bibliotheque/reglages.json"
export MURAL_CACHE="$preuves/vignettes"
export MURAL_AVIF_CACHE="$preuves/menus/avif"
export MURAL_JOURNAL="$preuves/journal.log"
mkdir -p "$preuves/bibliotheque" "$preuves/menus" "$preuves/panneau" "$preuves/memoire"
terminer() {
    resultat=$?
    printf '%s\n' "$resultat" > "$preuves/code-sortie.txt"
    printf '\nCode de sortie : %s. Preuves conservees : %s\n' "$resultat" "$preuves"
}
trap terminer EXIT
printf 'Preuves : %s\n' "$preuves"
printf '\n1/10 — Tests Cargo hors ligne, omissions visibles\n'
cargo test --offline --locked -- --nocapture 2>&1 | tee "$preuves/cargo-test.log"
if rg 'TEST NON EXECUTE' "$preuves/cargo-test.log" > "$preuves/omissions-avif.txt"; then
    printf 'NON VERIFIE : certains scenarios AVIF ne sont pas executes sur ce systeme (voir omissions-avif.txt).\n'
fi
printf '\n2/10 — Compilation des recettes\n'
cargo build --offline --locked --bin recette_bibliotheque --bin recette_interface --bin recette_memoire --bin recette_activation --bin recette_reentrance 2>&1 | tee "$preuves/cargo-build.log"
printf '\n3/10 — Bibliotheque et restauration dans un second processus\n'
"$projet/target/debug/recette_bibliotheque" 2>&1 | tee "$preuves/bibliotheque.log"
"$projet/target/debug/recette_bibliotheque" --restauration 2>&1 | tee "$preuves/restauration.log"
printf '\n4/10 — Menus et conversion AVIF en cache temporaire\n'
MURAL_REGLAGES="$preuves/menus/reglages.json" "$projet/target/debug/recette_bibliotheque" --menus 2>&1 | tee "$preuves/menus-avif.log"
if rg -q 'NON VERIFIE : conversion du panneau' "$preuves/menus-avif.log"; then
    printf 'ECHEC : conversion omise, verifier le perimetre du cache AVIF.\n' >&2
    exit 1
fi
printf '\n5/10 — Panneau, progression, apercu et cycle de vie\n'
MURAL_REGLAGES="$preuves/panneau/reglages.json" "$projet/target/debug/recette_bibliotheque" --panneau 2>&1 | tee "$preuves/panneau.log"
printf '\n6/10 — Composition et galerie de 500 images\n'
MURAL_REGLAGES="$preuves/interface/reglages.json" "$projet/target/debug/recette_interface" "$preuves/bibliotheque/Paysages" 2>&1 | tee "$preuves/interface.log"
MURAL_REGLAGES="$preuves/memoire/reglages.json" "$projet/target/debug/recette_memoire" 2>&1 | tee "$preuves/memoire.log"
printf '\n7/10 — Activation ancienne et moderne\n'
for mode in ancien moderne; do
    MURAL_REGLAGES="$preuves/activation-$mode/reglages.json" "$projet/target/debug/recette_activation" "$mode" 2>&1 | tee "$preuves/activation-$mode.log"
done
printf '\n8/10 — Notifications imbriquees et protection des reglages\n'
for mode in grille panneau panneau-initial preferences; do
    MURAL_REGLAGES="$preuves/reentrance-$mode/reglages.json" "$projet/target/debug/recette_reentrance" "--$mode" 2>&1 | tee "$preuves/reentrance-$mode.log"
done
printf '\n9/10 — Moteur Sparkle dans un bundle temporaire\n'
"$projet/scripts/test-mises-a-jour.sh" 2>&1 | tee "$preuves/mises-a-jour.log"
printf '\n10/10 — Archive source GPL, empreinte et tests extraits\n'
"$projet/scripts/test-source-archive.sh" 2>&1 | tee "$preuves/source-archive.log"
printf '\nValidation terminee. Les NON VERIFIE eventuels restent des limites explicites.\n'
