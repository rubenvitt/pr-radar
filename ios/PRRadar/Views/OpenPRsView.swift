// Offene PRs: Schnellfilter, Suche, gruppiert/flach, Liste/Flow, „Alle mergen“.

import PRRadarCore
import SwiftUI

struct OpenPRsView: View {
    @Environment(Radar.self) private var radar
    @State private var query = ""
    @State private var path: [String] = []

    var body: some View {
        NavigationStack(path: $path) {
            Group {
                if let snapshot = radar.snapshot, !radar.repos.isEmpty {
                    content(Derived(snapshot: snapshot, prefs: radar.prefs, query: query))
                } else {
                    SetupStateView()
                }
            }
            .navigationTitle(radar.prefs.quick == .all ? "Pull Requests" : radar.prefs.quick.label)
            .navigationDestination(for: String.self) { PRDetailView(prID: $0) }
            .searchable(text: $query, prompt: "Titel, Repo, Autor, Branch, #Nr.")
            .refreshable { await radar.refreshNow() }
            .toolbar { toolbar }
        }
    }

    @ViewBuilder
    private func content(_ d: Derived) -> some View {
        Group {
            if d.openList.isEmpty {
                ScrollView {
                    StaleBanner().padding()
                    ContentUnavailableView(
                        "Nichts offen", systemImage: "tray",
                        description: Text(query.isEmpty ? "Keine Pull Requests für diesen Filter." : "Keine Treffer für „\(query)“."))
                }
            } else if d.prefs.view == .flow {
                flow(d)
            } else {
                list(d)
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            QuickFilterBar(derived: d)
        }
    }

    // MARK: Liste

    private func list(_ d: Derived) -> some View {
        List {
            if radar.status.error != nil {
                Section { StaleBanner() }
            }
            if d.prefs.grouped {
                ForEach(d.openGroups, id: \.repo) { group in
                    Section {
                        ForEach(group.prs) { pr in row(pr, d.snapshot, showRepo: false) }
                    } header: {
                        RepoHeader(repo: group.repo, count: group.prs.count, info: d.snapshot.repo(group.repo))
                    }
                }
            } else {
                Section {
                    ForEach(d.openList) { pr in row(pr, d.snapshot, showRepo: true) }
                }
            }
            Section { PollFooter() }
                .listRowBackground(Color.clear)
        }
        .listStyle(.insetGrouped)
        .animation(.default, value: d.openList.map(\.id))
    }

    private func row(_ pr: OpenPR, _ snapshot: Snapshot, showRepo: Bool) -> some View {
        NavigationLink(value: pr.id) {
            PRRow(pr: pr, snapshot: snapshot, showRepo: showRepo)
        }
        .prActions(pr, snapshot: snapshot)
        .listRowBackground(RowBackground(pr: pr))
    }

    // MARK: Flow

    private func flow(_ d: Derived) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 10, pinnedViews: []) {
                if radar.status.error != nil {
                    StaleBanner().padding(.horizontal)
                }
                if d.prefs.grouped {
                    ForEach(d.openGroups, id: \.repo) { group in
                        RepoHeader(repo: group.repo, count: group.prs.count, info: d.snapshot.repo(group.repo))
                            .padding(.horizontal, 20)
                            .padding(.top, 10)
                        ForEach(group.prs) { pr in flowCard(pr, d.snapshot, showRepo: false) }
                    }
                } else {
                    ForEach(d.openList) { pr in flowCard(pr, d.snapshot, showRepo: true) }
                }
                PollFooter().padding(.vertical)
            }
            .padding(.vertical, 8)
        }
        .background(Color(.systemGroupedBackground))
    }

    private func flowCard(_ pr: OpenPR, _ snapshot: Snapshot, showRepo: Bool) -> some View {
        FlowCard(pr: pr, snapshot: snapshot, showRepo: showRepo)
            .padding(.vertical, 10)
            .background { RowBackground(pr: pr) }
            .clipShape(RoundedRectangle(cornerRadius: 14))
            .contentShape(.contextMenuPreview, RoundedRectangle(cornerRadius: 14))
            .prActions(pr, snapshot: snapshot)
            .padding(.horizontal, 16)
    }

    // MARK: Werkzeugleiste

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            PollIndicator()
        }
        ToolbarItemGroup(placement: .topBarTrailing) {
            if let snapshot = radar.snapshot {
                MergeAllButton(
                    derived: Derived(snapshot: snapshot, prefs: radar.prefs, query: query))
            }
            Menu {
                Picker("Darstellung", selection: viewBinding) {
                    Label("Liste", systemImage: "list.bullet").tag(ViewMode.list)
                    Label("Flow", systemImage: "point.3.connected.trianglepath.dotted").tag(ViewMode.flow)
                }
                Toggle(isOn: groupedBinding) {
                    Label("Nach Repo gruppieren", systemImage: "square.stack")
                }
                Divider()
                RepoFilterMenu()
            } label: {
                Label("Ansicht", systemImage: radar.prefs.repoFilter.isEmpty ? "ellipsis.circle" : "line.3.horizontal.decrease.circle.fill")
            }
        }
    }

    private var viewBinding: Binding<ViewMode> {
        Binding(get: { radar.prefs.view }, set: { v in radar.updatePrefs { $0.view = v } })
    }

    private var groupedBinding: Binding<Bool> {
        Binding(get: { radar.prefs.grouped }, set: { v in radar.updatePrefs { $0.grouped = v } })
    }
}

/// Zeilenhintergrund: Konflikte leicht rot, geänderte PRs kurz in ihrer Statusfarbe.
struct RowBackground: View {
    var pr: OpenPR
    @Environment(Radar.self) private var radar

    var body: some View {
        let flashing = radar.flashes[pr.id] != nil
        ZStack {
            Color(.secondarySystemGroupedBackground)
            if pr.hasConflict { Color.red.opacity(0.07) }
            pr.pipeline.state.color.opacity(flashing ? 0.22 : 0)
                .animation(.easeOut(duration: flashing ? 0.15 : flashDuration), value: flashing)
        }
    }
}

/// Schnellfilter als Chips mit Zählern.
struct QuickFilterBar: View {
    var derived: Derived
    @Environment(Radar.self) private var radar

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(Quick.allCases) { q in
                    let count = derived.count(q)
                    let selected = radar.prefs.quick == q
                    Button {
                        radar.updatePrefs { $0.quick = q }
                    } label: {
                        HStack(spacing: 5) {
                            Image(systemName: q.symbol).imageScale(.small)
                            Text(q.label)
                            Text("\(count)")
                                .monospacedDigit()
                                .foregroundStyle(selected ? Color.white.opacity(0.8) : Color.secondary)
                        }
                        .font(.footnote.weight(.medium))
                        .padding(.horizontal, 11)
                        .padding(.vertical, 6)
                        .foregroundStyle(selected ? Color.white : (count == 0 ? Color.secondary : Color.primary))
                        .background(
                            selected ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.thinMaterial), in: Capsule())
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(selected ? .isSelected : [])
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
        }
        .background(.bar)
        .sensoryFeedback(.selection, trigger: radar.prefs.quick)
    }
}

/// „Alle mergen (n)“ für alle sichtbaren, sofort mergebaren PRs – mit Bestätigung.
struct MergeAllButton: View {
    var derived: Derived
    @Environment(Radar.self) private var radar
    @State private var confirm = false

    var body: some View {
        if let plan = MergeAllPlan(prs: derived.openList, snapshot: derived.snapshot, isPending: { radar.isPending($0) }) {
            Button {
                confirm = true
            } label: {
                Label("Alle mergen (\(plan.count))", systemImage: "arrow.triangle.merge")
                    .labelStyle(.titleAndIcon)
            }
            .tint(.green)
            .confirmationDialog(
                "\(plan.count) PRs jetzt mergen?", isPresented: $confirm, titleVisibility: .visible
            ) {
                if plan.needsChoice {
                    ForEach(plan.offered) { m in
                        Button("Mit \(m.label) mergen") { radar.mergeAll(plan.items(preferred: m)) }
                    }
                } else {
                    Button("Mergen") { radar.mergeAll(plan.items(preferred: nil)) }
                }
            } message: {
                Text("Je Repo und Ziel-Branch nacheinander. Lehnt GitHub vorübergehend ab, wird erneut versucht bzw. Auto-Merge aktiviert.")
            }
        }
    }
}
