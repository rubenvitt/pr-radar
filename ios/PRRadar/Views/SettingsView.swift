// Einstellungen: GitHub-Konto, Token (Erweitert), Repositories, Benachrichtigungen, Status.

import PRRadarCore
import SwiftUI
import UIKit

struct SettingsView: View {
    @Environment(AppState.self) private var app
    @Environment(Radar.self) private var radar

    var body: some View {
        NavigationStack {
            Form {
                AccountSection()
                TokenSection()
                ReposSection()
                    // Die Demo-Repos sind fest; eigene Repos gibt es erst nach dem Beenden.
                    .disabled(app.isDemo)
                NotificationsSection()
                StatusSection()
                InfoSection()
            }
            .navigationTitle("Settings")
            .toolbar { EditButton() }
        }
    }
}

private struct AccountSection: View {
    @Environment(AppState.self) private var app
    @Environment(Radar.self) private var radar
    @Environment(\.openURL) private var openURL

    var body: some View {
        Section {
            switch app.login {
            case .starting:
                HStack(spacing: 10) {
                    ProgressView()
                    Text("Connecting to GitHub…")
                }
                Button("Cancel", role: .cancel) { app.cancelLogin() }
            case let .waiting(code):
                VStack(alignment: .leading, spacing: 8) {
                    Text("Enter this code on GitHub (it’s on the clipboard):")
                        .font(.subheadline)
                    Text(code.userCode)
                        .font(.system(.title, design: .monospaced).weight(.semibold))
                        .textSelection(.enabled)
                    HStack(spacing: 8) {
                        ProgressView()
                        Text("Waiting for confirmation on GitHub…")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 4)
                Button("Copy Code and Open GitHub", systemImage: "safari") {
                    UIPasteboard.general.string = code.userCode
                    openURL(code.verificationURI)
                }
                Button("Cancel", role: .cancel) { app.cancelLogin() }
            case .idle, .failed:
                if let source = app.tokenSource {
                    LabeledContent {
                        Text(source.label)
                    } label: {
                        if let viewer = radar.snapshot?.viewer {
                            Label("Signed in as \(viewer)", systemImage: "checkmark.seal.fill")
                                .foregroundStyle(.green)
                        } else {
                            Label("Token Saved", systemImage: "key.fill")
                        }
                    }
                }
                if app.tokenSource != .login {
                    Button("Sign In with GitHub", systemImage: "person.badge.key") { app.startLogin() }
                        .disabled(!app.canLogin)
                }
                if case let .failed(message) = app.login {
                    Text(message).font(.footnote).foregroundStyle(.red)
                }
                if app.tokenSource == .login {
                    Button("Sign Out", role: .destructive) { app.deleteToken() }
                }
            }
        } header: {
            Text("GitHub Account")
        } footer: {
            if app.canLogin {
                Text(
                    "Signing in opens GitHub in the browser. PR Radar gets access to your repos (scope “repo”) to read PRs, merge them and toggle auto-merge. Org repos only appear once the organization approves PR Radar. The token is stored only in this device’s keychain."
                )
            } else {
                Text("Sign-in is not set up in this build. Enter a personal access token under “Advanced”.")
            }
        }
    }
}

/// Erweitert: Personal Access Token von Hand, z. B. für feinere Rechte als bei der Anmeldung.
private struct TokenSection: View {
    @Environment(AppState.self) private var app
    @State private var input = ""
    @State private var error: String?
    @FocusState private var focused: Bool

    var body: some View {
        let placeholder: LocalizedStringKey = app.hasToken ? "Enter new token" : "ghp_… or github_pat_…"
        Section {
            HStack {
                SecureField(placeholder, text: $input)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .textContentType(.password)
                    .focused($focused)
                    .onSubmit(save)
                Button("Save", action: save)
                    .disabled(input.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            if let error {
                Text(error).font(.footnote).foregroundStyle(.red)
            }
            if app.tokenSource == .keychain {
                Button("Remove Token", role: .destructive) { app.deleteToken() }
            }
        } header: {
            Text("Advanced: Personal Access Token")
        } footer: {
            VStack(alignment: .leading, spacing: 6) {
                Text(
                    "Instead of signing in: a fine-grained PAT for the repos you want with Metadata, Contents, Commit statuses, Actions/Checks (read) and Pull requests (**write** for merge and auto-merge). For org repos without fine-grained tokens, use a classic PAT with `repo`. Replaces an existing sign-in."
                )
                Link("Create Token on GitHub", destination: URL(string: "https://github.com/settings/personal-access-tokens/new")!)
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
    @Environment(AppState.self) private var app
    @Environment(Radar.self) private var radar
    @State private var input = ""
    @State private var error: String?

    var body: some View {
        Section {
            if app.hasToken {
                NavigationLink {
                    RepoPickerView()
                } label: {
                    Label("Choose from GitHub Account", systemImage: "checklist")
                }
            }
            HStack {
                TextField("owner/repo or GitHub URL", text: $input)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                    .submitLabel(.done)
                    .onSubmit(add)
                Button("Add", systemImage: "plus.circle.fill", action: add)
                    .labelStyle(.iconOnly)
                    .disabled(input.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            if let error {
                Text(error).font(.footnote).foregroundStyle(.red)
            }
            if radar.repos.isEmpty {
                Text("No repos yet – choose them from your account or add one by name.")
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
            Text("All repos are fetched together via GraphQL – every 30 s, or every 10 s while any pipeline is running. Auto-merge must be allowed in the repo (Settings → General → Allow auto-merge).")
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
                    let autoMerge =
                        info.autoMergeAllowed ? String(localized: "Auto-merge allowed") : String(localized: "Auto-merge off")
                    let parts: [String] = [info.defaultBranch ?? "–", autoMerge] + (info.viewerCanMerge ? [] : [String(localized: "read-only")])
                    Text(parts.joined(separator: " · "))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    Text("Loading…").font(.caption).foregroundStyle(.secondary)
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
                Label("Notify About Pipelines & Merges", systemImage: "bell.badge")
            }
        } header: {
            Text("Notifications")
        } footer: {
            Text("Notifies you when a pipeline turns red or green or a PR is merged, and shows red PRs as a badge. In the background iOS only refreshes occasionally – the app is live in the foreground.")
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
                app.show(
                    .error, String(localized: "Notifications Off"),
                    String(localized: "Allow them for PR Radar in iOS Settings."))
            }
        }
    }
}

private struct StatusSection: View {
    @Environment(Radar.self) private var radar

    var body: some View {
        let s = radar.status
        Section("Status") {
            LabeledContent("Refresh Interval", value: String(localized: "every \(intervalLabel(s.interval))"))
            LabeledContent("Last Updated") { AgoText(date: s.lastSuccess) }
            if let rl = s.rateLimit {
                LabeledContent("API Quota", value: "\(rl.remaining) / \(rl.limit)")
            }
            if let error = s.error {
                Text(error).font(.footnote).foregroundStyle(.red)
            }
            Button("Refresh Now", systemImage: "arrow.clockwise") { radar.refresh() }
        }
    }
}

/// Version, Produktseite, Support (GitHub Issues) und Datenschutz (Pflicht für den App Store).
private struct InfoSection: View {
    var body: some View {
        Section("About") {
            LabeledContent("Version", value: Bundle.main.shortVersion)
            Link(destination: Site.url(.home)) {
                Label("PR Radar Website", systemImage: "globe")
            }
            Link(destination: Site.url(.support)) {
                Label("Support", systemImage: "questionmark.circle")
            }
            Link(destination: Site.reportIssueURL) {
                Label("Report a Problem", systemImage: "exclamationmark.bubble")
            }
            Link(destination: Site.url(.privacy)) {
                Label("Privacy Policy", systemImage: "hand.raised")
            }
            Link(destination: URL(string: "https://github.com/rubenvitt/pr-radar")!) {
                Label("Source Code on GitHub", systemImage: "chevron.left.forwardslash.chevron.right")
            }
            LabeledContent("Source Code License", value: "FSL-1.1-ALv2")
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
