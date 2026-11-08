// UI-Smoke: App startet, alle Tabs sind erreichbar, ohne Token führt der leere Zustand zu den Einstellungen.
// Läuft in der CI auf einem frischen Simulator (kein Token). Hat der Simulator schon einen Token,
// prüft der letzte Test stattdessen den hinterlegten Zustand.

import XCTest

final class SmokeTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        // Texte unabhängig von der Simulatorsprache auf Deutsch prüfen
        app.launchArguments += ["-AppleLanguages", "(de)", "-AppleLocale", "de_DE"]
        app.launch()
        return app
    }

    @MainActor
    private func openTab(_ name: String, in app: XCUIApplication) {
        let tabBar = app.tabBars.firstMatch
        XCTAssertTrue(tabBar.waitForExistence(timeout: 15), "Tab-Leiste fehlt")
        tabBar.buttons[name].tap()
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
}
