// swift-tools-version: 6.0
// Plattformunabhängiger Kern der iOS-App: Modell, GitHub-Client, Normalisierung, Poll- und Merge-Logik.
// Läuft auch unter macOS/Linux, damit `swift test` ohne Simulator geht.

import PackageDescription

let package = Package(
    name: "PRRadarCore",
    defaultLocalization: "de",
    platforms: [.iOS(.v18), .macOS(.v15)],
    products: [
        .library(name: "PRRadarCore", targets: ["PRRadarCore"])
    ],
    targets: [
        .target(name: "PRRadarCore"),
        .testTarget(
            name: "PRRadarCoreTests",
            dependencies: ["PRRadarCore"],
            resources: [.copy("Fixtures")]
        ),
    ]
)
