// Listenzeile eines offenen PRs: Status, Titel, Etiketten, Metadaten, Merge-Steuerung.

import PRRadarCore
import SwiftUI

struct PRRow: View {
    var pr: OpenPR
    var snapshot: Snapshot
    var showRepo: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            PipelineIcon(state: pr.pipeline.state)
                .padding(.top, 1)

            VStack(alignment: .leading, spacing: 6) {
                if showRepo {
                    RepoName(fullName: pr.repo).font(.caption)
                }
                Text("\(Text(pr.title).foregroundStyle(pr.isDraft ? Color.secondary : Color.primary)) \(Text("#\(pr.number)").foregroundStyle(.secondary))")
                    .font(.body.weight(.medium))
                    .lineLimit(3)

                PRTags(pr: pr, viewer: snapshot.viewer)

                FlowLayout(spacing: 8, lineSpacing: 3) {
                    HStack(spacing: 4) {
                        AvatarView(person: pr.author, size: 16)
                        Text(pr.author?.login ?? "ghost")
                    }
                    BranchLabel(head: pr.headRef, base: pr.baseRef)
                    AgoText(date: pr.updatedAt)
                    DiffSize(additions: pr.additions, deletions: pr.deletions)
                }
                .font(.caption)
                .foregroundStyle(.secondary)

                HStack(spacing: 8) {
                    if pr.pipeline.total > 0 {
                        Label("\(pr.pipeline.passed)/\(pr.pipeline.total)", systemImage: "checklist")
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(pr.pipeline.state.color)
                            .accessibilityLabel("\(pr.pipeline.passed) von \(pr.pipeline.total) Checks grün")
                    }
                    Spacer(minLength: 0)
                    MergeControl(pr: pr, snapshot: snapshot)
                }
            }
        }
        .padding(.vertical, 4)
    }
}

/// Draft, Konflikt, Review-Status und GitHub-Labels.
struct PRTags: View {
    var pr: OpenPR
    var viewer: String?

    var body: some View {
        let hasAny = pr.isDraft || pr.hasConflict || pr.requestsReview(from: viewer) || pr.reviewDecision == .approved
            || pr.reviewDecision == .changesRequested || pr.autoMerge != nil || !pr.labels.isEmpty
        if hasAny {
            FlowLayout(spacing: 5, lineSpacing: 4) {
                if pr.isDraft { Chip(text: "Draft", symbol: "pencil") }
                if pr.hasConflict { Chip(text: "Konflikt", symbol: "exclamationmark.triangle.fill", color: .red, filled: true) }
                if pr.requestsReview(from: viewer) { Chip(text: "Dein Review", symbol: "person.badge.clock", color: .accentColor) }
                switch pr.reviewDecision {
                case .approved: Chip(text: "Approved", symbol: "checkmark.shield", color: .green)
                case .changesRequested: Chip(text: "Änderungen", symbol: "exclamationmark.bubble", color: .red)
                default: EmptyView()
                }
                if let am = pr.autoMerge { Chip(text: "Auto · \(am.method.label)", symbol: "bolt.fill", color: .accentColor) }
                ForEach(pr.labels, id: \.name) { LabelChip(label: $0) }
            }
        }
    }
}

/// Kopf einer Repo-Gruppe: Name, Anzahl, Pipeline des Default-Branches.
struct RepoHeader: View {
    var repo: String
    var count: Int
    var info: RepoInfo?
    @Environment(\.openURL) private var openURL

    var body: some View {
        HStack(spacing: 8) {
            Button {
                if let url = URL(string: info?.url ?? "https://github.com/\(repo)") { openURL(url) }
            } label: {
                RepoName(fullName: repo).font(.subheadline.weight(.semibold))
            }
            .buttonStyle(.plain)
            .foregroundStyle(.primary)
            Text("\(count)")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
            Spacer()
            if let branch = info?.defaultBranch {
                HStack(spacing: 4) {
                    PipelineIcon(state: info?.defaultBranchPipeline ?? .none, size: 12)
                    Text(branch).font(.caption.monospaced())
                }
                .foregroundStyle(.secondary)
                .accessibilityElement(children: .combine)
            }
        }
        .textCase(nil)
    }
}
