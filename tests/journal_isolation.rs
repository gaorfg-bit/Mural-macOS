use std::{fs, process::Command};
#[test]
fn journal_respecte_le_chemin_isole_du_processus() {
    if std::env::var_os("MURAL_TEST_JOURNAL_ENFANT").is_some() {
        mural::journal::info("recette-journal-isole");
        return;
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let chemin = format!(
        "/private/tmp/mural-journal-test-{}-{unique}/journal.log",
        std::process::id()
    );
    let resultat = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "journal_respecte_le_chemin_isole_du_processus",
            "--nocapture",
        ])
        .env("MURAL_JOURNAL", &chemin)
        .env("MURAL_TEST_JOURNAL_ENFANT", "1")
        .output()
        .unwrap();
    assert!(
        resultat.status.success(),
        "{}",
        String::from_utf8_lossy(&resultat.stderr)
    );
    let texte = fs::read_to_string(&chemin).expect("journal temporaire absent");
    assert!(texte.contains("[info] recette-journal-isole"));
}
