// swift-tools-version:5.9
// Mural pour macOS — portage du gestionnaire de fonds d'écran GNOME.
// Aucune dépendance : SwiftPM + frameworks Apple uniquement.
import PackageDescription

let package = Package(
    name: "Mural",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "MuralKit", targets: ["MuralKit"]),
        .executable(name: "Mural", targets: ["Mural"]),
        .executable(name: "sonde", targets: ["Sonde"]),
    ],
    targets: [
        // Coeur testable : modeles + services (AppKit autorise, pas d'UI).
        .target(name: "MuralKit", path: "Sources/MuralKit"),
        // Application AppKit.
        .executableTarget(name: "Mural", dependencies: ["MuralKit"], path: "Sources/Mural"),
        // Outil de mesure, sans interface (equivalent du script de verification).
        .executableTarget(name: "Sonde", dependencies: ["MuralKit"], path: "Sources/Sonde"),
        .testTarget(name: "MuralKitTests", dependencies: ["MuralKit"], path: "Tests/MuralKitTests"),
    ]
)
