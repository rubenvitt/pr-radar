// „Zuletzt gemergt“ als Timeline nach Tagen.

import PRRadarCore
import SwiftUI

struct MergedView: View {
    @Environment(Radar.self) private var radar
    @State private var query = ""

    var body: some View {
        NavigationStack {
            Group {
                if let snapshot = radar.snapshot, !radar.repos.isEmpty {
                    let d = Derived(snapshot: snapshot, prefs: radar.prefs, query: query)
                    if d.merged.isEmpty {
                        ContentUnavailableView("Noch nichts gemergt", systemImage: "arrow.triangle.merge")
                    } else {
                        List {
                            if radar.status.error != nil { Section { StaleBanner() } }
                            ForEach(d.mergedByDay(), id: \.day) { group in
                                Section(group.day) {
                                    ForEach(group.prs) { MergedRow(pr: $0) }
                                }
                            }
                            Section { PollFooter() }.listRowBackground(Color.clear)
                        }
                        .listStyle(.insetGrouped)
                    }
                } else {
                    SetupStateView()
                }
            }
            .navigationTitle("Gemergt")
            .searchable(text: $query, prompt: "Titel, Repo, #Nr.")
            .refreshable { await radar.refreshNow() }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { RepoFilterMenu() }
            }
        }
    }
}

private struct MergedRow: View {
    var pr: MergedPR
    @Environment(\.openURL) private var openURL

    var body: some View {
        Button {
            if let url = URL(string: pr.url) { openURL(url) }
        } label: {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "arrow.triangle.merge")
                    .foregroundStyle(Color.accentColor)
                    .padding(.top, 2)
                VStack(alignment: .leading, spacing: 5) {
                    Text("\(Text(pr.title)) \(Text("#\(pr.number)").foregroundStyle(.secondary))")
                        .font(.body.weight(.medium))
                        .lineLimit(3)
                    FlowLayout(spacing: 8, lineSpacing: 3) {
                        RepoName(fullName: pr.repo)
                        HStack(spacing: 4) {
                            AvatarView(person: pr.author, size: 16)
                            Text(pr.author?.login ?? "ghost")
                        }
                        if let by = pr.mergedBy, by != pr.author?.login {
                            Text("gemergt von \(by)")
                        }
                        Text("→ \(pr.baseRef)").font(.caption.monospaced())
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                VStack(alignment: .trailing, spacing: 2) {
                    Text(GermanTime.clock(pr.mergedAt)).monospacedDigit()
                    AgoText(date: pr.mergedAt)
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
        }
        .foregroundStyle(.primary)
        .contextMenu {
            if let url = URL(string: pr.url) {
                Button("Auf GitHub öffnen", systemImage: "safari") { openURL(url) }
                ShareLink(item: url)
            }
        }
    }
}
