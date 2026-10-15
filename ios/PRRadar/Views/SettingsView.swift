// Einstellungen: Token, Repositories, Benachrichtigungen, Status.

import PRRadarCore
import SwiftUI

struct SettingsView: View {
    @Environment(AppState.self) private var app
    @Environment(Radar.self) private var radar

    var body: some View {
        NavigationStack {
            Form {
                TokenSection()
                ReposSection()
                NotificationsSection()
                StatusSection()
                Section("Info") {
                    LabeledContent("Version", value: Bundle.main.shortVersion)
                    Link(destination: URL(string: "https://github.com/rubenvitt/pr-radar")!) {
                        Label("Quellcode auf GitHub", systemImage: "chevron.left.forwardslash.chevron.right")
                    }
                    LabeledContent("Lizenz", value: "Apache-2.0")
                }
            }
            .navigationTitle("Einstellungen")
            .toolbar { EditButton() }
        }
    }
}

private struct TokenSection: View {
    @Environment(AppState.self) private var app
    @Environment(Radar.self) private var radar
    @State private var input = ""
    @State private var error: String?
    @FocusState private var focused: Bool

    var body: some View {
        Section {
            if let source = app.tokenSource {
                LabeledContent {
                    Text(source.label)
                } label: {
                    if let viewer = radar.snapshot?.viewer {
                        Label("Angemeldet als \(viewer)", systemImage: "checkmark.seal.fill")
                            .foregroundStyle(.green)
                    } else {
                        Label("Token hinterlegt", systemImage: "key.fill")
                    }
                }
            }
            HStack {
                SecureField(app.hasToken ? "Neuen Token eintragen" : "ghp_… oder github_pat_…", text: $input)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .textContentType(.password)
                    .focused($focused)
                    .onSubmit(save)
                Button("Speichern", action: save)
                    .disabled(input.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            if let error {
                Text(error).font(.footnote).foregroundStyle(.red)
            }
            if app.tokenSource == .keychain {
                Button("Token entfernen", role: .destructive) { app.deleteToken() }
            }
        } header: {
            Text("GitHub-Token")
        } footer: {
            VStack(alignment: .leading, spacing: 6) {
                Text(
                    "Fine-grained PAT auf die gewünschten Repos: Metadata, Contents, Commit statuses, Actions/Checks (read) und Pull requests (**write** für Merge und Auto-Merge). Für Org-Repos ohne fine-grained Tokens einen klassischen PAT mit `repo` nutzen. Der Token liegt nur im Schlüsselbund dieses Geräts."
                )
                Link("Token auf GitHub erstellen", destination: URL(string: "https://github.com/settings/personal-access-tokens/new")!)
            }
        }
    }

    private func save() {
        do {
            try app.saveToken(input)
            input = ""
            error = nil
            focused = false
        } catch {
            self.error = error.localizedDescription
        }
    }
}

private struct ReposSection: View {
    @Environment(Radar.self) private var radar
    @State private var input = ""
    @State private var error: String?

    var body: some View {
        Section {
            HStack {
                TextField("owner/repo oder GitHub-URL", text: $input)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                    .submitLabel(.done)
                    .onSubmit(add)
                Button("Hinzufügen", systemImage: "plus.circle.fill", action: add)
                    .labelStyle(.iconOnly)
                    .disabled(input.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            if let error {
                Text(error).font(.footnote).foregroundStyle(.red)
            }
            if radar.repos.isEmpty {
                Text("Noch keine Repos – füge oben eins hinzu.")
                    .foregroundStyle(.secondary)
            }
            ForEach(radar.repos, id: \.self) { full in
                RepoSettingsRow(full: full, info: radar.snapshot?.repo(full))
            }
            .onDelete { offsets in
                for full in offsets.map({ radar.repos[$0] }) { try? radar.removeRepo(full) }
            }
            .onMove { radar.moveRepos(fromOffsets: $0, toOffset: $1) }
        } header: {
            Text("Repositories")
        } footer: {
            Text("Alle Repos werden gebündelt per GraphQL abgefragt – alle 30 s, solange irgendwo eine Pipeline läuft alle 10 s. Auto-Merge muss im Repo erlaubt sein (Settings → General → Allow auto-merge).")
        }
    }

    private func add() {
        do {
            try radar.addRepo(input)
            input = ""
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}

private struct RepoSettingsRow: View {
    var full: String
    var info: RepoInfo?

    var body: some View {
        HStack(spacing: 12) {
            PipelineIcon(state: info?.defaultBranchPipeline ?? .none, size: 16)
            VStack(alignment: .leading, spacing: 2) {
                RepoName(fullName: full)
                if let error = info?.error {
                    Text(error).font(.caption).foregroundStyle(.red)
                } else if let info {
                    Text("\(info.defaultBranch ?? "–") · Auto-Merge \(info.autoMergeAllowed ? "erlaubt" : "aus")\(info.viewerCanMerge ? "" : " · nur lesend")")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    Text("wird geladen …").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }
}

private struct NotificationsSection: View {
    @Environment(AppState.self) private var app
    @Environment(Radar.self) private var radar

    var body: some View {
        Section {
            Toggle(isOn: Binding(get: { radar.prefs.notify }, set: { setNotify($0) })) {
                Label("Pipelines & Merges melden", systemImage: "bell.badge")
            }
        } header: {
            Text("Benachrichtigungen")
        } footer: {
            Text("Meldet, wenn eine Pipeline rot oder grün wird oder ein PR gemergt wurde, und zeigt rote PRs als Badge. Im Hintergrund aktualisiert iOS nur gelegentlich – live ist die App im Vordergrund.")
        }
    }

    private func setNotify(_ on: Bool) {
        guard on else {
            radar.updatePrefs { $0.notify = false }
            app.updateBadge()
            return
        }
        Task {
            let granted = await Notifier.shared.requestAuthorization()
            radar.updatePrefs { $0.notify = granted }
            app.updateBadge()
            if !granted {
                app.show(.error, "Benachrichtigungen aus", "In den iOS-Einstellungen für PR Radar erlauben.")
            }
        }
    }
}

private struct StatusSection: View {
    @Environment(Radar.self) private var radar

    var body: some View {
        let s = radar.status
        Section("Status") {
            LabeledContent("Aktualisierung", value: "alle \(Int(s.interval)) s")
            LabeledContent("Zuletzt") { AgoText(date: s.lastSuccess) }
            if let rl = s.rateLimit {
                LabeledContent("API-Kontingent", value: "\(rl.remaining) / \(rl.limit)")
            }
            if let error = s.error {
                Text(error).font(.footnote).foregroundStyle(.red)
            }
            Button("Jetzt aktualisieren", systemImage: "arrow.clockwise") { radar.refresh() }
        }
    }
}

extension Bundle {
    var shortVersion: String {
        let v = infoDictionary?["CFBundleShortVersionString"] as? String ?? "–"
        let b = infoDictionary?["CFBundleVersion"] as? String ?? ""
        return b.isEmpty ? v : "\(v) (\(b))"
    }
}
