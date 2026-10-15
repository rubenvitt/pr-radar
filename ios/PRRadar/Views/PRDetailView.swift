// Detail eines offenen PRs: Flow, Merge-Steuerung, Review und alle Checks.

import PRRadarCore
import SwiftUI

struct PRDetailView: View {
    var prID: String
    @Environment(Radar.self) private var radar
    @Environment(\.openURL) private var openURL

    var body: some View {
        if let snapshot = radar.snapshot, let pr = snapshot.openPR(prID) {
            detail(pr, snapshot)
        } else {
            ContentUnavailableView(
                "Nicht mehr offen", systemImage: "arrow.triangle.merge",
                description: Text("Der Pull Request wurde gemergt oder geschlossen."))
        }
    }

    private func detail(_ pr: OpenPR, _ snapshot: Snapshot) -> some View {
        List {
            Section {
                VStack(alignment: .leading, spacing: 8) {
                    RepoName(fullName: pr.repo).font(.subheadline)
                    Text(pr.title).font(.title3.weight(.semibold))
                    PRTags(pr: pr, viewer: snapshot.viewer)
                    FlowLayout(spacing: 10, lineSpacing: 4) {
                        HStack(spacing: 4) {
                            AvatarView(person: pr.author, size: 18)
                            Text(pr.author?.login ?? "ghost")
                        }
                        BranchLabel(head: pr.headRef, base: pr.baseRef)
                        AgoText(date: pr.updatedAt)
                        DiffSize(additions: pr.additions, deletions: pr.deletions)
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                .padding(.vertical, 4)
                .listRowBackground(RowBackground(pr: pr))
            }

            Section("Flow") {
                FlowCard(pr: pr, snapshot: snapshot, showRepo: false, linksToDetail: false)
                    .listRowInsets(EdgeInsets(top: 10, leading: 0, bottom: 10, trailing: 0))
            }

            mergeSection(pr, snapshot)

            Section("Review") {
                let stage = reviewStage(pr, viewer: snapshot.viewer)
                Label {
                    VStack(alignment: .leading) {
                        Text(stage.title)
                        Text(stage.sub).font(.caption).foregroundStyle(.secondary)
                    }
                } icon: {
                    Image(systemName: stage.symbol).foregroundStyle(stage.tone.color)
                }
            }

            Section {
                if pr.pipeline.checks.isEmpty {
                    Text("Keine Checks für den letzten Commit.").foregroundStyle(.secondary)
                }
                ForEach(Array(pr.pipeline.checks.enumerated()), id: \.offset) { _, check in
                    CheckRow(check: check)
                }
            } header: {
                HStack {
                    Text("Checks")
                    Spacer()
                    if pr.pipeline.total > 0 {
                        Text("\(pr.pipeline.passed)/\(pr.pipeline.total) grün")
                            .foregroundStyle(pr.pipeline.state.color)
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
        .navigationTitle("#\(pr.number)")
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { await radar.refreshNow() }
        .toolbar {
            if let url = URL(string: pr.url) {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    ShareLink(item: url)
                    Button("Auf GitHub öffnen", systemImage: "safari") { openURL(url) }
                }
            }
        }
    }

    private func mergeSection(_ pr: OpenPR, _ snapshot: Snapshot) -> some View {
        let stage = mergeStage(pr)
        let action = MergeAction.of(pr, in: snapshot)
        return Section {
            HStack(spacing: 12) {
                Image(systemName: stage.symbol)
                    .foregroundStyle(stage.tone.color)
                    .frame(width: 24)
                VStack(alignment: .leading) {
                    Text(stage.title)
                    Text(stage.sub).font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                MergeControl(pr: pr, snapshot: snapshot, prominent: true)
            }
            switch action {
            case let .unavailable(reason):
                Text(reason).font(.footnote).foregroundStyle(.secondary)
            case let .disable(method, by):
                Text("Auto-Merge aktiv (\(method.label)\(by.map { ", von \($0)" } ?? "")) – mergt, sobald alles grün ist.")
                    .font(.footnote).foregroundStyle(.secondary)
            default:
                EmptyView()
            }
            if radar.isRetrying(pr.id) {
                Label("GitHub lässt den Merge noch nicht zu – neuer Versuch gleich …", systemImage: "clock.arrow.circlepath")
                    .font(.footnote)
                    .foregroundStyle(.orange)
            }
        } header: {
            Text("Merge")
        } footer: {
            if !pr.mergeMethods.isEmpty {
                Text("Erlaubt: \(pr.mergeMethods.map(\.label).joined(separator: ", ")) – Repo-Einstellungen und Rulesets von \(pr.baseRef).")
            }
        }
    }
}

private struct CheckRow: View {
    var check: Check
    @Environment(\.openURL) private var openURL

    var body: some View {
        let content = HStack(spacing: 10) {
            switch check.state {
            case .skipped, .neutral:
                Image(systemName: "minus.circle").foregroundStyle(.secondary)
            default:
                PipelineIcon(state: check.state.pipeline, size: 16)
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(check.name).lineLimit(2)
                let detail = [check.group, check.state == .skipped ? "übersprungen" : nil].compactMap { $0 }
                if !detail.isEmpty {
                    Text(detail.joined(separator: " · ")).font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
            if check.url != nil {
                Image(systemName: "arrow.up.right.square").foregroundStyle(.tertiary)
            }
        }
        if let raw = check.url, let url = URL(string: raw) {
            Button { openURL(url) } label: { content }
                .foregroundStyle(.primary)
        } else {
            content
        }
    }
}
