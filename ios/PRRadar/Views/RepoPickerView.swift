// Repo-Auswahl: die Repos des GitHub-Kontos mit Suche zum Anhaken – beim ersten Start (Sheet)
// und in den Einstellungen. Häkchen speichern sofort; neu geladen wird einmal kurz danach.

import PRRadarCore
import SwiftUI

struct RepoPickerView: View {
    @Environment(Radar.self) private var radar
    @State private var query = ""
    @State private var error: String?

    var body: some View {
        List {
            if let error {
                Section {
                    Text(error).font(.footnote).foregroundStyle(.red)
                }
            }
            switch radar.repoChoices {
            case .idle, .loading:
                HStack(spacing: 10) {
                    ProgressView()
                    Text("Loading repositories from GitHub…").foregroundStyle(.secondary)
                }
            case let .failed(message):
                Section {
                    Label(message, systemImage: "exclamationmark.triangle.fill")
                        .font(.footnote)
                        .foregroundStyle(.red)
                    Button("Try Again", systemImage: "arrow.clockwise") { radar.loadRepoChoices() }
                }
            case let .loaded(choices):
                let shown = filtered(choices)
                Section {
                    if choices.isEmpty {
                        Text("No repositories found for this account.").foregroundStyle(.secondary)
                    } else if shown.isEmpty {
                        Text("No results for “\(query)”.").foregroundStyle(.secondary)
                    }
                    ForEach(shown) { choice in
                        RepoChoiceRow(choice: choice, watched: radar.isWatched(choice.full)) {
                            toggle(choice.full)
                        }
                    }
                } header: {
                    if !radar.repos.isEmpty {
                        Text("\(radar.repos.count) selected")
                    }
                } footer: {
                    Text(
                        "Org repos only appear once the organization approves PR Radar. Missing a repo? Add it by name in Settings."
                    )
                }
            }
        }
        .searchable(text: $query, prompt: "Search repositories")
        .navigationTitle("Choose Repositories")
        .refreshable { radar.loadRepoChoices() }
        .task { radar.ensureRepoChoices() }
    }

    private func filtered(_ choices: [RepoChoice]) -> [RepoChoice] {
        let q = query.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { return choices }
        return choices.filter {
            $0.full.localizedCaseInsensitiveContains(q) || ($0.description?.localizedCaseInsensitiveContains(q) ?? false)
        }
    }

    private func toggle(_ full: String) {
        do {
            try radar.setWatched(full, !radar.isWatched(full))
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}

private struct RepoChoiceRow: View {
    var choice: RepoChoice
    var watched: Bool
    var toggle: () -> Void

    var body: some View {
        Button(action: toggle) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        RepoName(fullName: choice.full)
                        if choice.isPrivate {
                            Image(systemName: "lock.fill")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .accessibilityLabel("Private")
                        }
                    }
                    if let description = choice.description {
                        Text(description)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                }
                Spacer(minLength: 0)
                Image(systemName: watched ? "checkmark.circle.fill" : "circle")
                    .font(.title3)
                    .foregroundStyle(watched ? AnyShapeStyle(.tint) : AnyShapeStyle(.tertiary))
                    .accessibilityHidden(true)
            }
            .contentShape(Rectangle())
        }
        .foregroundStyle(.primary)
        .accessibilityAddTraits(watched ? .isSelected : [])
        .sensoryFeedback(.selection, trigger: watched)
    }
}

/// Auswahl als Sheet beim ersten Start, mit „Fertig“.
struct RepoPickerSheet: View {
    @Environment(AppState.self) private var app

    var body: some View {
        NavigationStack {
            RepoPickerView()
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { app.showRepoPicker = false }
                    }
                }
        }
    }
}
