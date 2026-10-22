import Foundation

/// Journal de Mural : fichier + stderr, immediatement vide sur disque.
///
/// Equivalent de la journalisation de `mural/app.py:39-47` (niveau reglable par
/// variable d'environnement, meme format que la console), ecrite dans un
/// fichier pour pouvoir relire ce qui a echoue sans terminal.
public enum Journal {
    private static let verrou = NSLock()
    private static let formateur: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd HH:mm:ss"
        f.locale = Locale(identifier: "fr_FR")
        return f
    }()

    /// `~/Library/Logs/Mural/mural.log`.
    public static var chemin: URL? {
        FileManager.default
            .urls(for: .libraryDirectory, in: .userDomainMask).first?
            .appendingPathComponent("Logs/Mural/mural.log")
    }

    public static func info(_ message: String) { ecrire("info", message) }
    public static func erreur(_ message: String) { ecrire("ERREUR", message) }

    public static func demarrage() {
        info("================ demarrage de Mural ================")
    }

    private static func ecrire(_ niveau: String, _ message: String) {
        let ligne = "\(formateur.string(from: Date())) [\(niveau)] \(message)\n"
        verrou.lock()
        defer { verrou.unlock() }

        if let chemin {
            let dossier = chemin.deletingLastPathComponent()
            try? FileManager.default.createDirectory(at: dossier, withIntermediateDirectories: true)
            if let donnees = ligne.data(using: .utf8) {
                if let poignee = try? FileHandle(forWritingTo: chemin) {
                    defer { try? poignee.close() }
                    _ = try? poignee.seekToEnd()
                    try? poignee.write(contentsOf: donnees)
                } else {
                    try? donnees.write(to: chemin)
                }
            }
        }
        FileHandle.standardError.write(Data(ligne.utf8))
    }
}
