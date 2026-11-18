// UI-Smoke: App startet, alle Tabs sind erreichbar, ohne Token führt der leere Zustand zu den Einstellungen.
// Läuft in der CI auf einem frischen Simulator (kein Token). Hat der Simulator schon einen Token,
// prüft der letzte Test stattdessen den hinterlegten Zustand. Der Demo-Test zeigt Beispieldaten und
// prüft, dass auch die Texte aus PRRadarCore (eigenes Bundle) deutsch erscheinen.

import XCTest

final class SmokeTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func launch(_ extra: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        // Texte unabhängig von der Simulatorsprache auf Deutsch prüfen
        app.launchArguments += ["-AppleLanguages", "(de)", "-AppleLocale", "de_DE"] + extra
        app.launch()
        return app
    }

    /// Knopf eines Tabs. Ab iOS 26 (Liquid Glass) steht die Tab-Leiste je nach Version nicht mehr als
    /// `TabBar` im Accessibility-Baum, deshalb über die Beschriftung suchen und die Leiste nur bevorzugen.
    /// Fehlt der Knopf, landet der Baum im Log, damit die CI zeigt, was stattdessen da ist.
    @MainActor
    private func tabButton(_ name: String, in app: XCUIApplication) -> XCUIElement? {
        let inBar = app.tabBars.buttons[name]
        if inBar.exists { return inBar }
        // Gleich beschriftete Knöpfe gibt es mehrfach (z. B. die verborgene Seitenleiste von
        // `.sidebarAdaptable`), deshalb den nehmen, der auf dem Bildschirm liegt.
        let matches = app.buttons.matching(NSPredicate(format: "label == %@", name))
        guard matches.firstMatch.waitForExistence(timeout: 15) else {
            XCTFail("Tab „\(name)“ fehlt\n\(app.debugDescription)")
            return nil
        }
        if let visible = matches.allElementsBoundByIndex.first(where: { $0.isHittable }) { return visible }
        XCTFail("Tab „\(name)“ ist \(matches.count)× da, aber nicht sichtbar\n\(app.debugDescription)")
        return nil
    }

    @MainActor
    private func openTab(_ name: String, in app: XCUIApplication) {
        tabButton(name, in: app)?.tap()
    }

    @MainActor
    func testAlleTabsErreichbar() {
        let app = launch()
        let tabs = [
            ("Gemergt", "Gemergt"),
            ("Releases", "Releases"),
            ("Einstellungen", "Einstellungen"),
            ("Offen", "Pull Requests"),
        ]
        for (tab, title) in tabs {
            openTab(tab, in: app)
            XCTAssertTrue(app.navigationBars[title].waitForExistence(timeout: 5), "Titel „\(title)“ fehlt")
        }
    }

    @MainActor
    func testEinstellungenZeigenTokenUndRepos() {
        let app = launch()
        openTab("Einstellungen", in: app)
        XCTAssertTrue(app.navigationBars["Einstellungen"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Speichern"].exists, "Token-Bereich fehlt")
        XCTAssertFalse(app.buttons["Speichern"].isEnabled, "ohne Eingabe nichts zu speichern")
        XCTAssertTrue(app.textFields["owner/repo oder GitHub-URL"].exists, "Repo-Eingabe fehlt")
    }

    @MainActor
    func testOhneTokenZumEintragen() {
        let app = launch()
        openTab("Offen", in: app)
        if app.staticTexts["Nicht bei GitHub angemeldet"].waitForExistence(timeout: 5) {
            // Ohne Client-ID der OAuth App (CI) bleibt nur der Weg über den Token.
            if app.buttons["Mit GitHub anmelden"].exists { return }
            app.buttons["Token eintragen"].tap()
            XCTAssertTrue(app.navigationBars["Einstellungen"].waitForExistence(timeout: 5))
            XCTAssertTrue(app.secureTextFields["ghp_… oder github_pat_…"].exists)
        } else {
            // Simulator mit hinterlegtem Token
            openTab("Einstellungen", in: app)
            XCTAssertTrue(app.secureTextFields["Neuen Token eintragen"].waitForExistence(timeout: 5))
        }
    }

    /// Kern-Texte kommen über `NSLocalizedString(…, bundle: .module)` aus dem Bundle von
    /// PRRadarCore, nicht aus dem der App. Fehlt dort die deutsche Lokalisierung zur Laufzeit,
    /// stünden die Schnellfilter englisch in einer sonst deutschen App.
    @MainActor
    func testDemoDeutschInklusiveKernTexte() {
        let app = launch(["--demo"])

        // App-Text (Catalog der App)
        XCTAssertNotNil(tabButton("Einstellungen", in: app), "App-Text „Einstellungen“ fehlt")

        // Kern-Text: Schnellfilter `Quick.failed` heißt deutsch „Rot“, englisch „Failed“
        let rot = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Rot")).firstMatch
        let failed = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "Failed")).firstMatch
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "All")).firstMatch
            .waitForExistence(timeout: 10), "Schnellfilter „Alle“/„All“ fehlen (Demo nicht gestartet?)")
        XCTAssertFalse(failed.exists, "Schnellfilter englisch („Failed“) statt deutsch")
        XCTAssertTrue(rot.exists, "Schnellfilter „Rot“ fehlt")

        // Gewählter Schnellfilter wird zum Titel, ebenfalls aus dem Kern
        rot.tap()
        XCTAssertTrue(app.navigationBars["Rot"].waitForExistence(timeout: 5), "Titel „Rot“ fehlt")
    }
}
