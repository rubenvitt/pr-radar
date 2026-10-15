// Kleine, wiederverwendete Darstellungsbausteine.

import PRRadarCore
import SwiftUI

extension PipelineState {
    /// Statusfarbe – Bedeutung, nie Dekoration.
    var color: Color {
        switch self {
        case .passed: .green
        case .failed: .red
        case .running: .orange
        case .none: .secondary
        }
    }

    var symbol: String {
        switch self {
        case .passed: "checkmark.circle.fill"
        case .failed: "xmark.circle.fill"
        case .running: "hourglass.circle.fill"
        case .none: "circle.dashed"
        }
    }
}

extension Tone {
    var color: Color {
        switch self {
        case .pass: .green
        case .fail: .red
        case .run: .orange
        case .accent: .accentColor
        case .idle: .secondary
        }
    }
}

/// Pipeline-Symbol: Form und Farbe tragen den Zustand gemeinsam (nicht nur Farbe).
struct PipelineIcon: View {
    var state: PipelineState
    var size: CGFloat = 18

    var body: some View {
        Image(systemName: state.symbol)
            .font(.system(size: size))
            .foregroundStyle(state.color)
            .symbolEffect(.pulse, isActive: state == .running)
            .accessibilityLabel("Pipeline \(state.label)")
    }
}

/// `owner/name` mit gedämpftem Owner.
struct RepoName: View {
    var fullName: String

    var body: some View {
        let parts = fullName.split(separator: "/", maxSplits: 1)
        let owner = parts.count == 2 ? "\(parts[0])/" : ""
        let name = parts.last.map(String.init) ?? fullName
        Text("\(Text(owner).foregroundStyle(.secondary))\(name)")
            .lineLimit(1)
            .truncationMode(.middle)
    }
}

struct AvatarView: View {
    var person: Person?
    var size: CGFloat = 18

    var body: some View {
        AsyncImage(url: url) { image in
            image.resizable().scaledToFill()
        } placeholder: {
            Circle()
                .fill(.quaternary)
                .overlay {
                    Text(person?.login.prefix(1).uppercased() ?? "?")
                        .font(.system(size: size * 0.5, weight: .semibold))
                        .foregroundStyle(.secondary)
                }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        .accessibilityHidden(true)
    }

    private var url: URL? {
        guard let raw = person?.avatarURL, !raw.isEmpty else { return nil }
        let px = Int(size * 3)
        return URL(string: raw + (raw.contains("?") ? "&" : "?") + "s=\(px)")
    }
}

/// Kleines Etikett (Draft, Konflikt, Approved …).
struct Chip: View {
    var text: String
    var symbol: String? = nil
    var color: Color = .secondary
    var filled = false

    var body: some View {
        HStack(spacing: 3) {
            if let symbol { Image(systemName: symbol).imageScale(.small) }
            Text(text)
        }
        .font(.caption2.weight(.semibold))
        .lineLimit(1)
        .padding(.horizontal, 6)
        .padding(.vertical, 2)
        .foregroundStyle(filled ? Color.white : color)
        .background(filled ? AnyShapeStyle(color) : AnyShapeStyle(color.opacity(0.14)), in: Capsule())
    }
}

/// GitHub-Label in seiner eigenen Farbe – die Farbe ist hier Datum, keine Dekoration.
struct LabelChip: View {
    var label: PRLabel
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let base = Color(hex: label.color) ?? .gray
        Text(label.name)
            .font(.caption2.weight(.medium))
            .lineLimit(1)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .foregroundStyle(readable(base))
            .background(base.opacity(0.18), in: Capsule())
            .overlay(Capsule().strokeBorder(base.opacity(0.45), lineWidth: 0.5))
    }

    /// Helle Labelfarben auf hellem Grund (und dunkle auf dunklem) nachdunkeln bzw. aufhellen.
    private func readable(_ color: Color) -> Color {
        let l = luminance(label.color)
        if scheme == .light && l > 0.55 { return color.mix(with: .black, by: 0.55) }
        if scheme == .dark && l < 0.25 { return color.mix(with: .white, by: 0.55) }
        return color
    }

    private func luminance(_ hex: String) -> Double {
        guard let v = UInt32(hex, radix: 16) else { return 0.5 }
        let r = Double((v >> 16) & 0xFF) / 255, g = Double((v >> 8) & 0xFF) / 255, b = Double(v & 0xFF) / 255
        return 0.2126 * r + 0.7152 * g + 0.0722 * b
    }
}

extension Color {
    init?(hex: String) {
        guard hex.count == 6, let v = UInt32(hex, radix: 16) else { return nil }
        self.init(
            red: Double((v >> 16) & 0xFF) / 255, green: Double((v >> 8) & 0xFF) / 255, blue: Double(v & 0xFF) / 255)
    }
}

/// Zeilenumbrechendes HStack für Etiketten und Metadaten.
struct FlowLayout: Layout {
    var spacing: CGFloat = 6
    var lineSpacing: CGFloat = 4

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(subviews, width: proposal.width ?? .infinity)
        let width = rows.map(\.width).max() ?? 0
        let height = rows.map(\.height).reduce(0, +) + lineSpacing * CGFloat(max(0, rows.count - 1))
        return CGSize(width: proposal.width.map { min($0, width) } ?? width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in arrange(subviews, width: bounds.width) {
            var x = bounds.minX
            for (index, size) in row.items {
                subviews[index].place(
                    at: CGPoint(x: x, y: y + (row.height - size.height) / 2), anchor: .topLeading,
                    proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
            y += row.height + lineSpacing
        }
    }

    private struct Row {
        var items: [(Int, CGSize)] = []
        var width: CGFloat = 0
        var height: CGFloat = 0
    }

    private func arrange(_ subviews: Subviews, width: CGFloat) -> [Row] {
        var rows: [Row] = [Row()]
        for (i, view) in subviews.enumerated() {
            var size = view.sizeThatFits(.unspecified)
            size.width = min(size.width, width)
            let extra = rows[rows.count - 1].items.isEmpty ? size.width : spacing + size.width
            if rows[rows.count - 1].width + extra > width, !rows[rows.count - 1].items.isEmpty {
                rows.append(Row())
            }
            let isFirst = rows[rows.count - 1].items.isEmpty
            rows[rows.count - 1].items.append((i, size))
            rows[rows.count - 1].width += isFirst ? size.width : spacing + size.width
            rows[rows.count - 1].height = max(rows[rows.count - 1].height, size.height)
        }
        return rows.filter { !$0.items.isEmpty }
    }
}

/// Branch-Angabe `head → base` in Monospace.
struct BranchLabel: View {
    var head: String
    var base: String

    var body: some View {
        HStack(spacing: 3) {
            Image(systemName: "arrow.triangle.branch").imageScale(.small)
            Text(head).lineLimit(1).truncationMode(.middle).frame(maxWidth: 160, alignment: .leading)
            Text("→")
            Text(base).lineLimit(1)
        }
        .font(.caption.monospaced())
    }
}

/// `+12 −3`
struct DiffSize: View {
    var additions: Int
    var deletions: Int

    var body: some View {
        HStack(spacing: 3) {
            Text("+\(additions)").foregroundStyle(.green)
            Text("−\(deletions)").foregroundStyle(.red)
        }
        .font(.caption.monospacedDigit())
        .accessibilityLabel("\(additions) Zeilen hinzugefügt, \(deletions) entfernt")
    }
}

/// Live-Zeitangabe („vor 5 Min.“), aktualisiert sich selbst.
struct AgoText: View {
    var date: Date?

    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { context in
            Text(GermanTime.ago(date, now: context.date))
        }
    }
}

/// Repo-Filter als Menü: mehrere Repos an-/abwählbar, leer = alle.
struct RepoFilterMenu: View {
    @Environment(Radar.self) private var radar

    var body: some View {
        let filter = radar.prefs.repoFilter
        Menu {
            Button {
                radar.updatePrefs { $0.repoFilter = [] }
            } label: {
                if filter.isEmpty { Label("Alle Repos", systemImage: "checkmark") } else { Text("Alle Repos") }
            }
            Divider()
            ForEach(radar.repos, id: \.self) { repo in
                Button {
                    radar.updatePrefs { p in
                        if let i = p.repoFilter.firstIndex(of: repo) { p.repoFilter.remove(at: i) } else { p.repoFilter.append(repo) }
                    }
                } label: {
                    if filter.contains(repo) { Label(repo, systemImage: "checkmark") } else { Text(repo) }
                }
            }
        } label: {
            Label(
                "Repos", systemImage: filter.isEmpty
                    ? "line.3.horizontal.decrease.circle" : "line.3.horizontal.decrease.circle.fill")
        }
        .menuActionDismissBehavior(.disabled)
    }
}

/// Zustand „noch nichts zu zeigen“: kein Token, keine Repos, lädt, Fehler.
struct SetupStateView: View {
    @Environment(AppState.self) private var app
    @Environment(Radar.self) private var radar

    var body: some View {
        if !app.hasToken {
            ContentUnavailableView {
                Label("Kein GitHub-Token", systemImage: "key")
            } description: {
                Text("PR Radar spricht GitHub direkt an. Hinterlege einen Personal Access Token.")
            } actions: {
                Button("Token eintragen") { radar.updatePrefs { $0.tab = .settings } }
                    .buttonStyle(.borderedProminent)
            }
        } else if radar.repos.isEmpty {
            ContentUnavailableView {
                Label("Noch keine Repositories", systemImage: "arrow.triangle.pull")
            } description: {
                Text("Füge Repos als owner/name oder GitHub-URL hinzu.")
            } actions: {
                Button("Repositories hinzufügen") { radar.updatePrefs { $0.tab = .settings } }
                    .buttonStyle(.borderedProminent)
            }
        } else if let error = radar.status.error {
            ContentUnavailableView {
                Label("GitHub nicht erreichbar", systemImage: "wifi.exclamationmark")
            } description: {
                Text(error)
            } actions: {
                Button("Erneut versuchen") { radar.refresh() }
            }
        } else {
            ProgressView("Lade Daten von GitHub …")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

/// Fehlerhinweis über der Liste, wenn der letzte Abruf scheiterte, aber alte Daten da sind.
struct StaleBanner: View {
    @Environment(Radar.self) private var radar

    var body: some View {
        if let error = radar.status.error {
            Label {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Aktualisierung fehlgeschlagen").font(.subheadline.weight(.semibold))
                    Text(error).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                }
            } icon: {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
            }
        }
    }
}

/// Fußzeile: wann zuletzt aktualisiert, wie oft, API-Kontingent.
struct PollFooter: View {
    @Environment(Radar.self) private var radar

    var body: some View {
        let s = radar.status
        TimelineView(.periodic(from: .now, by: 10)) { ctx in
            VStack(spacing: 2) {
                Text("Aktualisiert \(GermanTime.ago(s.lastSuccess, now: ctx.date)) · alle \(Int(s.interval)) s")
                if let rl = s.rateLimit {
                    Text("API-Kontingent \(rl.remaining) / \(rl.limit)")
                }
            }
            .font(.caption2)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity)
        }
    }
}

/// Punkt in der Navigationsleiste: grün = aktuell, gelb pulsierend = lädt, rot = Fehler.
struct PollIndicator: View {
    @Environment(Radar.self) private var radar

    var body: some View {
        let state = radar.status.state
        Button {
            radar.refresh()
        } label: {
            Image(systemName: state == .error ? "exclamationmark.circle.fill" : "dot.radiowaves.left.and.right")
                .foregroundStyle(state == .error ? Color.red : state == .polling ? Color.orange : Color.green)
                .symbolEffect(.variableColor.iterative, isActive: state == .polling)
        }
        .accessibilityLabel(state == .error ? "Fehler – neu laden" : state == .polling ? "Lädt" : "Aktuell – neu laden")
    }
}
