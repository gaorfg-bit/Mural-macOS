import Foundation

/// Persistance des reglages — equivalent de `Config` (`mural/config.py:15-60`).
///
/// Meme discipline que l'original : ecriture atomique (fichier temporaire puis
/// remplacement), permissions privees, et une config illisible est
/// **conservee** sous `reglages.json.corrupt` au lieu d'etre perdue.
///
/// Emplacement macOS plutot que `~/.config/mural/` : Application Support.
public enum ReglagesDepot {
    public static var dossier: URL? {
        FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("Mural", isDirectory: true)
    }

    public static var chemin: URL? {
        dossier?.appendingPathComponent("reglages.json")
    }

    /// Charge les reglages ; toute erreur laisse les valeurs par defaut.
    public static func charger() -> Reglages {
        guard let chemin else { return Reglages() }
        guard let donnees = try? Data(contentsOf: chemin) else { return Reglages() }

        if let brut = try? JSONSerialization.jsonObject(with: donnees) as? [String: Any] {
            return Reglages(dictionnaire: brut)
        }

        // Config illisible : on la met de cote plutot que de l'ecraser
        // (parite avec config.py:44-52).
        let sauvegarde = chemin.appendingPathExtension("corrupt")
        try? FileManager.default.removeItem(at: sauvegarde)
        try? FileManager.default.moveItem(at: chemin, to: sauvegarde)
        Journal.erreur("reglages illisibles, conserves dans \(sauvegarde.lastPathComponent)")
        return Reglages()
    }

    /// Enregistre les reglages. Renvoie faux si l'ecriture a echoue.
    @discardableResult
    public static func sauvegarder(_ reglages: Reglages) -> Bool {
        guard let chemin, let dossier else { return false }
        do {
            try FileManager.default.createDirectory(at: dossier, withIntermediateDirectories: true)
            let donnees = try JSONSerialization.data(
                withJSONObject: reglages.enDictionnaire(),
                options: [.prettyPrinted, .sortedKeys]
            )
            // Ecriture atomique : le fichier final n'est jamais partiel.
            let temporaire = chemin.appendingPathExtension("tmp")
            try donnees.write(to: temporaire, options: .atomic)
            _ = try FileManager.default.replaceItemAt(chemin, withItemAt: temporaire)
            try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: chemin.path)
            return true
        } catch {
            Journal.erreur("enregistrement des reglages impossible : \(error.localizedDescription)")
            return false
        }
    }
}
