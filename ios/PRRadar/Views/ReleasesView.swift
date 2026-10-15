// Releases aller Repos – das neueste je Repo mit aufgeklappten Release Notes.

import PRRadarCore
import SwiftUI
import UIKit

struct ReleasesView: View {
    @Environment(Radar.self) private var radar
    @State private var query = ""
    /// Releases, deren Zustand vom Standard abweicht (auf- bzw. zugeklappt)
    @State private var toggled: Set<String> = []

    var body: some View {
        NavigationStack {
            Group {
                if let snapshot = radar.snapshot, !radar.repos.isEmpty {
                    let d = Derived(snapshot: snapshot, prefs: radar.prefs, query: query)
                    if d.releases.isEmpty {
                        ContentUnavailableView("Keine Releases", systemImage: "tag")
                    } else {
                        let defaults = d.releasesOpenByDefault
                        List {
                            if radar.status.error != nil { Section { StaleBanner() } }
                            ForEach(d.releases) { r in
                                Section {
                                    ReleaseCard(
                                        release: r,
                                        isOpen: Binding(
                                            get: { defaults.contains(r.id) != toggled.contains(r.id) },
                                            set: { open in
                                                if open != defaults.contains(r.id) { toggled.insert(r.id) } else { toggled.remove(r.id) }
                                            }))
                                }
                            }
                            Section { PollFooter() }.listRowBackground(Color.clear)
                        }
                        .listStyle(.insetGrouped)
                        .listSectionSpacing(.compact)
                    }
                } else {
                    SetupStateView()
                }
            }
            .navigationTitle("Releases")
            .searchable(text: $query, prompt: "Name, Tag, Repo")
            .refreshable { await radar.refreshNow() }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { RepoFilterMenu() }
            }
        }
    }
}

private struct ReleaseCard: View {
    var release: Release
    @Binding var isOpen: Bool
    @Environment(\.openURL) private var openURL

    var body: some View {
        let hasNotes = !release.descriptionHTML.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "tag.fill")
                    .foregroundStyle(Color.accentColor)
                    .padding(.top, 2)
                VStack(alignment: .leading, spacing: 5) {
                    FlowLayout(spacing: 6, lineSpacing: 4) {
                        Text(release.name).font(.body.weight(.semibold))
                        if release.name != release.tagName {
                            Text(release.tagName).font(.caption.monospaced()).foregroundStyle(.secondary)
                        }
                        if release.isLatest { Chip(text: "Latest", color: .green) }
                        if release.isPrerelease { Chip(text: "Pre-Release", color: .orange) }
                    }
                    FlowLayout(spacing: 8, lineSpacing: 3) {
                        RepoName(fullName: release.repo)
                        if let author = release.author { Text("von \(author)") }
                        AgoText(date: release.publishedAt)
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                if hasNotes {
                    Image(systemName: "chevron.down")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.tertiary)
                        .rotationEffect(.degrees(isOpen ? 180 : 0))
                }
            }
            .contentShape(Rectangle())
            .onTapGesture {
                guard hasNotes else { return }
                withAnimation(.snappy) { isOpen.toggle() }
            }
            .accessibilityAddTraits(hasNotes ? .isButton : [])
            .accessibilityHint(hasNotes ? (isOpen ? "Release Notes zuklappen" : "Release Notes aufklappen") : "")

            if isOpen && hasNotes {
                Divider()
                ReleaseNotes(id: release.id, html: release.descriptionHTML)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .padding(.vertical, 4)
        .contextMenu {
            if let url = URL(string: release.url) {
                Button("Auf GitHub öffnen", systemImage: "safari") { openURL(url) }
                ShareLink(item: url)
            }
            if let date = release.publishedAt {
                Text("Veröffentlicht \(GermanTime.full(date))")
            }
        }
        .swipeActions(edge: .trailing) {
            if let url = URL(string: release.url) {
                Button("GitHub", systemImage: "safari") { openURL(url) }.tint(.blue)
            }
        }
    }
}

/// Release Notes: `descriptionHTML` ist von GitHub gerendertes, bereinigtes HTML.
/// Farben werden entfernt, damit der Text in Hell und Dunkel lesbar bleibt.
/// Der HTML-Import läuft außerhalb des SwiftUI-Updates (in `.task`), weil er intern die
/// Runloop dreht – mitten in `updateUIView` kann das hängen.
private struct ReleaseNotes: View {
    var id: String
    var html: String
    @State private var text: NSAttributedString?

    @MainActor private static var cache: [String: NSAttributedString] = [:]

    var body: some View {
        Group {
            if let text {
                NotesTextView(text: text)
            } else {
                ProgressView().frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .task(id: "\(id):\(html.hashValue)") {
            // erst nach dem aktuellen Update rendern
            await Task.yield()
            text = Self.render(id: id, html: html)
        }
    }

    @MainActor private static func render(id: String, html: String) -> NSAttributedString {
        let key = "\(id):\(html.hashValue)"
        if let cached = cache[key] { return cached }
        let css = """
            <style>
            body { font-family: -apple-system, sans-serif; font-size: 15px; line-height: 1.35; }
            h1 { font-size: 20px; } h2 { font-size: 18px; } h3, h4 { font-size: 16px; }
            code, pre { font-family: ui-monospace, Menlo, monospace; font-size: 13px; }
            </style>
            """
        // Der HTML-Import lädt Bilder synchron auf dem Main Thread – deshalb vorher entfernen.
        let stripped = html.replacingOccurrences(
            of: "<(img|video|iframe|picture|source)\\b[^>]*>", with: "", options: [.regularExpression, .caseInsensitive])
        let out: NSMutableAttributedString
        if let parsed = try? NSMutableAttributedString(
            data: Data((css + stripped).utf8),
            options: [
                .documentType: NSAttributedString.DocumentType.html,
                .characterEncoding: String.Encoding.utf8.rawValue,
            ],
            documentAttributes: nil)
        {
            out = parsed
        } else {
            out = NSMutableAttributedString(string: html)
        }
        let all = NSRange(location: 0, length: out.length)
        out.removeAttribute(.backgroundColor, range: all)
        out.addAttribute(.foregroundColor, value: UIColor.label, range: all)
        // Abschließende Leerzeilen aus dem HTML-Import entfernen
        while out.length > 0, out.string.last?.isNewline == true {
            out.deleteCharacters(in: NSRange(location: out.length - 1, length: 1))
        }
        cache[key] = out
        return out
    }
}

private struct NotesTextView: UIViewRepresentable {
    var text: NSAttributedString

    func makeUIView(context: Context) -> UITextView {
        let view = UITextView()
        view.isEditable = false
        view.isScrollEnabled = false
        view.backgroundColor = .clear
        view.textContainerInset = .zero
        view.textContainer.lineFragmentPadding = 0
        view.dataDetectorTypes = []
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        return view
    }

    func updateUIView(_ view: UITextView, context: Context) {
        if view.attributedText != text { view.attributedText = text }
        view.linkTextAttributes = [.foregroundColor: UIColor.tintColor]
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: UITextView, context: Context) -> CGSize? {
        let width = proposal.width ?? 320
        let size = uiView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
        return CGSize(width: width, height: ceil(size.height))
    }
}
