// Flow-Ansicht: PR → Checks je Workflow → Review → Merge → Ziel-Branch als Knotengraph.
// Auf schmalen Bildschirmen scrollt der Graph horizontal.

import PRRadarCore
import SwiftUI

private enum Metrics {
    static let prWidth: CGFloat = 210
    static let checkWidth: CGFloat = 160
    static let stageWidth: CGFloat = 156
    static let baseWidth: CGFloat = 136
    static let checkHeight: CGFloat = 46
    static let checkGap: CGFloat = 8
    static let columnGap: CGFloat = 28
    static let rowPadding: CGFloat = 16
    static let minHeight: CGFloat = 112

    static var totalWidth: CGFloat {
        prWidth + checkWidth + 2 * stageWidth + baseWidth + 4 * columnGap
    }
}

/// Eine Kante zwischen zwei Anschlusspunkten.
struct FlowEdge: Sendable {
    var from: CGPoint
    var to: CGPoint
    var tone: Tone

    func point(at t: CGFloat) -> CGPoint {
        // Kubische Bezier-Kurve mit waagrechten Tangenten an beiden Enden
        let mx = (from.x + to.x) / 2
        let p0 = from, p1 = CGPoint(x: mx, y: from.y), p2 = CGPoint(x: mx, y: to.y), p3 = to
        let u = 1 - t
        let a = u * u * u, b = 3 * u * u * t, c = 3 * u * t * t, d = t * t * t
        return CGPoint(
            x: a * p0.x + b * p1.x + c * p2.x + d * p3.x,
            y: a * p0.y + b * p1.y + c * p2.y + d * p3.y)
    }

    var path: Path {
        var p = Path()
        let mx = (from.x + to.x) / 2
        p.move(to: from)
        p.addCurve(to: to, control1: CGPoint(x: mx, y: from.y), control2: CGPoint(x: mx, y: to.y))
        return p
    }
}

struct FlowCard: View {
    var pr: OpenPR
    var snapshot: Snapshot
    var showRepo: Bool
    /// In der Detailansicht selbst führt der PR-Knoten nirgendwohin.
    var linksToDetail = true
    @Environment(Radar.self) private var radar
    @Environment(\.openURL) private var openURL
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let checks = checkNodes(pr)
        let review = reviewStage(pr, viewer: snapshot.viewer)
        let merge = mergeStage(pr)
        let n = CGFloat(checks.count)
        let checksHeight = n * Metrics.checkHeight + (n - 1) * Metrics.checkGap
        let height = max(Metrics.minHeight, checksHeight + Metrics.rowPadding)
        let edges = makeEdges(checks: checks, review: review, merge: merge, height: height, checksHeight: checksHeight)

        ScrollView(.horizontal, showsIndicators: false) {
            ZStack(alignment: .topLeading) {
                FlowEdges(edges: edges, beat: radar.status.lastSuccess, animate: !reduceMotion)
                    .frame(width: Metrics.totalWidth, height: height)

                HStack(alignment: .center, spacing: Metrics.columnGap) {
                    Group {
                        if linksToDetail {
                            NavigationLink(value: pr.id) { prNode }
                                .buttonStyle(.plain)
                        } else {
                            prNode
                        }
                    }
                    .frame(width: Metrics.prWidth)

                    VStack(spacing: Metrics.checkGap) {
                        ForEach(checks) { c in checkNode(c) }
                    }
                    .frame(width: Metrics.checkWidth)

                    FlowNode(tone: review.tone, title: review.title, sub: review.sub) {
                        Image(systemName: review.symbol)
                    }
                    .frame(width: Metrics.stageWidth)

                    FlowNode(tone: merge.tone, title: merge.title, sub: merge.sub) {
                        Image(systemName: merge.symbol)
                    } accessory: {
                        MergeControl(pr: pr, snapshot: snapshot)
                    }
                    .frame(width: Metrics.stageWidth)

                    let base = baseState(pr, in: snapshot)
                    FlowNode(
                        tone: Tone(base), title: pr.baseRef,
                        sub: snapshot.repo(pr.repo)?.defaultBranch == pr.baseRef
                            ? "Default · \(base.label)" : "Ziel-Branch",
                        monospacedTitle: true
                    ) {
                        Image(systemName: "arrow.triangle.branch")
                    }
                    .frame(width: Metrics.baseWidth)
                }
                .frame(width: Metrics.totalWidth, height: height)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 4)
        }
        .scrollClipDisabled()
    }

    private var prNode: some View {
        FlowNode(
            tone: pr.isBy(snapshot.viewer) ? .accent : .idle,
            title: pr.title,
            sub: [showRepo ? pr.repo : nil, "#\(pr.number)", pr.headRef, GermanTime.ago(pr.updatedAt)]
                .compactMap { $0 }.joined(separator: " · "),
            titleLines: 2,
            dimmed: pr.isDraft
        ) {
            AvatarView(person: pr.author, size: 26)
        }
        .contentShape(Rectangle())
        .accessibilityHint(linksToDetail ? "Details öffnen" : "")
    }

    @ViewBuilder
    private func checkNode(_ c: CheckNode) -> some View {
        let node = FlowNode(tone: Tone(c.state), title: c.label, sub: c.sub) {
            PipelineIcon(state: c.state, size: 15)
        }
        .frame(height: Metrics.checkHeight)
        if let raw = c.url, let url = URL(string: raw) {
            Button { openURL(url) } label: { node }
                .buttonStyle(.plain)
                .accessibilityHint("Check auf GitHub öffnen")
        } else {
            node
        }
    }

    private func makeEdges(checks: [CheckNode], review: Stage, merge: Stage, height: CGFloat, checksHeight: CGFloat)
        -> [FlowEdge]
    {
        let mid = height / 2
        let top = (height - checksHeight) / 2
        let xChecks = Metrics.prWidth + Metrics.columnGap
        let xReview = xChecks + Metrics.checkWidth + Metrics.columnGap
        let xMerge = xReview + Metrics.stageWidth + Metrics.columnGap
        let xBase = xMerge + Metrics.stageWidth + Metrics.columnGap
        var edges: [FlowEdge] = []
        for (i, c) in checks.enumerated() {
            let y = top + CGFloat(i) * (Metrics.checkHeight + Metrics.checkGap) + Metrics.checkHeight / 2
            let tone = Tone(c.state)
            edges.append(FlowEdge(from: CGPoint(x: Metrics.prWidth, y: mid), to: CGPoint(x: xChecks, y: y), tone: tone))
            edges.append(
                FlowEdge(
                    from: CGPoint(x: xChecks + Metrics.checkWidth, y: y), to: CGPoint(x: xReview, y: mid), tone: tone))
        }
        edges.append(
            FlowEdge(
                from: CGPoint(x: xReview + Metrics.stageWidth, y: mid), to: CGPoint(x: xMerge, y: mid),
                tone: review.tone))
        edges.append(
            FlowEdge(
                from: CGPoint(x: xMerge + Metrics.stageWidth, y: mid), to: CGPoint(x: xBase, y: mid), tone: merge.tone))
        return edges
    }
}

/// Knoten: Symbol-Kachel, Titel, Untertitel, Anschlusspunkte links/rechts.
struct FlowNode<Icon: View, Accessory: View>: View {
    var tone: Tone
    var title: String
    var sub: String
    var titleLines = 1
    var dimmed = false
    var monospacedTitle = false
    @ViewBuilder var icon: Icon
    @ViewBuilder var accessory: Accessory

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                icon
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(tone.color)
                    .frame(width: 28, height: 28)
                    .background(tone.color.opacity(0.14), in: RoundedRectangle(cornerRadius: 7))
                VStack(alignment: .leading, spacing: 1) {
                    Text(title)
                        .font(monospacedTitle ? Font.caption.weight(.semibold).monospaced() : Font.caption.weight(.semibold))
                        .foregroundStyle(dimmed ? Color.secondary : Color.primary)
                        .lineLimit(titleLines)
                    Text(sub)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer(minLength: 0)
            }
            accessory
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(.tertiarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .strokeBorder(tone.color.opacity(tone == .idle ? 0.25 : 0.6), lineWidth: 1)
        )
        .overlay(alignment: .leading) { port }
        .overlay(alignment: .trailing) { port }
        .accessibilityElement(children: .combine)
    }

    private var port: some View {
        Circle()
            .fill(tone.color)
            .frame(width: 6, height: 6)
            .padding(.horizontal, -3)
            .accessibilityHidden(true)
    }
}

extension FlowNode where Accessory == EmptyView {
    init(
        tone: Tone, title: String, sub: String, titleLines: Int = 1, dimmed: Bool = false,
        monospacedTitle: Bool = false, @ViewBuilder icon: () -> Icon
    ) {
        self.init(
            tone: tone, title: title, sub: sub, titleLines: titleLines, dimmed: dimmed,
            monospacedTitle: monospacedTitle, icon: icon, accessory: { EmptyView() })
    }
}

/// Kanten als Bezier-Kurven; bei jedem Poll (`beat`) wandert einmal ein Punkt über laufende
/// Kanten – „hier fließt gerade etwas“. Kein Dauer-Animieren, das kostet Akku.
private struct FlowEdges: View {
    var edges: [FlowEdge]
    var beat: Date?
    var animate: Bool
    @State private var phase: CGFloat = 1

    var body: some View {
        Canvas { ctx, _ in
            for e in edges {
                let dashed = e.tone == .idle
                ctx.stroke(
                    e.path, with: .color(e.tone.color.opacity(dashed ? 0.45 : 0.75)),
                    style: StrokeStyle(lineWidth: 1.5, lineCap: .round, dash: dashed ? [4, 4] : []))
            }
        }
        .overlay {
            if animate {
                RunningDots(edges: edges.filter { $0.tone == .run }, phase: phase)
                    .fill(Color.orange)
            }
        }
        .onChange(of: beat) {
            guard animate, edges.contains(where: { $0.tone == .run }) else { return }
            var reset = Transaction()
            reset.disablesAnimations = true
            withTransaction(reset) { phase = 0 }
            Task { @MainActor in
                withAnimation(.easeInOut(duration: 1.4)) { phase = 1 }
            }
        }
        .accessibilityHidden(true)
    }
}

/// Punkte auf laufenden Kanten; `phase` wird von SwiftUI interpoliert (0 → 1).
private struct RunningDots: Shape {
    var edges: [FlowEdge]
    var phase: CGFloat

    var animatableData: CGFloat {
        get { phase }
        set { phase = newValue }
    }

    func path(in rect: CGRect) -> Path {
        var path = Path()
        guard phase > 0.001, phase < 0.999 else { return path }
        for e in edges {
            let p = e.point(at: phase)
            path.addEllipse(in: CGRect(x: p.x - 3.5, y: p.y - 3.5, width: 7, height: 7))
        }
        return path
    }
}
