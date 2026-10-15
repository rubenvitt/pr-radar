// Merge-Steuerung: Auto-Merge an/aus oder direkt mergen – nur mit erlaubten Methoden.
// Direktes Mergen ist nicht umkehrbar und braucht deshalb auf dem Touchscreen immer einen
// zweiten Tipp (Menü bzw. Bestätigung) – anders als die Desktop-App mit Mausklick.

import PRRadarCore
import SwiftUI
import UIKit

struct MergeControl: View {
    var pr: OpenPR
    var snapshot: Snapshot
    var prominent = false
    @Environment(Radar.self) private var radar

    var body: some View {
        let busy = radar.isPending(pr.id)
        let action = MergeAction.of(pr, in: snapshot)
        Group {
            switch action {
            case let .disable(method, _):
                Button {
                    radar.setAutoMerge(pr.id, method: nil)
                } label: {
                    ControlLabel(symbol: "bolt.fill", text: "Auto · \(method.label)", busy: busy, busyText: "Ändert …")
                }
                .tint(.accentColor)
                .accessibilityHint("Auto-Merge deaktivieren")

            case let .mergeNow(methods):
                Menu {
                    Section("Jetzt mergen") {
                        ForEach(methods) { m in
                            Button(m.label, systemImage: "arrow.triangle.merge") { radar.merge(pr.id, method: m) }
                        }
                    }
                } label: {
                    ControlLabel(
                        symbol: "arrow.triangle.merge", text: "Mergen", busy: busy,
                        busyText: radar.isRetrying(pr.id) ? "Neuer Versuch …" : "Mergt …")
                }
                .tint(.green)

            case let .enable(methods):
                if let only = methods.first, methods.count == 1 {
                    Button {
                        radar.setAutoMerge(pr.id, method: only)
                    } label: {
                        ControlLabel(symbol: "bolt", text: "Auto-Merge", busy: busy, busyText: "Aktiviert …")
                    }
                    .accessibilityHint("Auto-Merge mit \(only.label) aktivieren")
                } else {
                    Menu {
                        Section("Auto-Merge mit") {
                            ForEach(methods) { m in
                                Button(m.label) { radar.setAutoMerge(pr.id, method: m) }
                            }
                        }
                    } label: {
                        ControlLabel(symbol: "bolt", text: "Auto-Merge", busy: busy, busyText: "Aktiviert …")
                    }
                }

            case let .unavailable(reason):
                // Tippen zeigt den Grund – statt eines Tooltips wie auf dem Desktop.
                Menu {
                    Text(reason)
                } label: {
                    ControlLabel(symbol: "bolt.slash", text: "Auto-Merge", busy: false, busyText: "")
                }
                .tint(.secondary)
            }
        }
        .buttonStyle(.bordered)
        .buttonBorderShape(.capsule)
        .controlSize(prominent ? .regular : .small)
        .disabled(busy)
        .fixedSize()
    }
}

private struct ControlLabel: View {
    var symbol: String
    var text: String
    var busy: Bool
    var busyText: String

    var body: some View {
        if busy {
            HStack(spacing: 6) {
                ProgressView().controlSize(.mini)
                Text(busyText)
            }
            .font(.caption.weight(.semibold))
        } else {
            Label(text, systemImage: symbol)
                .font(.caption.weight(.semibold))
        }
    }
}

/// Wischgesten und Kontextmenü für eine PR-Zeile.
struct PRActions: ViewModifier {
    var pr: OpenPR
    var snapshot: Snapshot
    @Environment(Radar.self) private var radar
    @Environment(\.openURL) private var openURL
    @State private var confirmMerge = false

    func body(content: Content) -> some View {
        let action = MergeAction.of(pr, in: snapshot)
        let busy = radar.isPending(pr.id)
        content
            .swipeActions(edge: .leading, allowsFullSwipe: true) {
                if !busy {
                    switch action {
                    case .mergeNow:
                        Button("Mergen", systemImage: "arrow.triangle.merge") { confirmMerge = true }
                            .tint(.green)
                    case let .enable(methods):
                        Button("Auto-Merge", systemImage: "bolt") {
                            if methods.count == 1 { radar.setAutoMerge(pr.id, method: methods[0]) } else { confirmMerge = true }
                        }
                        .tint(.accentColor)
                    case .disable:
                        Button("Auto aus", systemImage: "bolt.slash") { radar.setAutoMerge(pr.id, method: nil) }
                            .tint(.gray)
                    case .unavailable:
                        EmptyView()
                    }
                }
            }
            .swipeActions(edge: .trailing) {
                if let url = URL(string: pr.url) {
                    Button("GitHub", systemImage: "safari") { openURL(url) }
                        .tint(.blue)
                }
            }
            .contextMenu {
                if !busy { mergeButtons(action) }
                if let url = URL(string: pr.url) {
                    Section {
                        Button("Auf GitHub öffnen", systemImage: "safari") { openURL(url) }
                        Button("Link kopieren", systemImage: "doc.on.doc") { UIPasteboard.general.url = url }
                        ShareLink(item: url) { Label("Teilen …", systemImage: "square.and.arrow.up") }
                    }
                }
            }
            .confirmationDialog(dialogTitle(action), isPresented: $confirmMerge, titleVisibility: .visible) {
                mergeButtons(action)
            }
    }

    @ViewBuilder
    private func mergeButtons(_ action: MergeAction) -> some View {
        switch action {
        case let .mergeNow(methods):
            Section("Jetzt mergen") {
                ForEach(methods) { m in
                    Button("\(m.label)", systemImage: "arrow.triangle.merge") { radar.merge(pr.id, method: m) }
                }
            }
        case let .enable(methods):
            Section("Auto-Merge mit") {
                ForEach(methods) { m in
                    Button("\(m.label)", systemImage: "bolt") { radar.setAutoMerge(pr.id, method: m) }
                }
            }
        case .disable:
            Button("Auto-Merge deaktivieren", systemImage: "bolt.slash") { radar.setAutoMerge(pr.id, method: nil) }
        case .unavailable:
            EmptyView()
        }
    }

    private func dialogTitle(_ action: MergeAction) -> String {
        if case .mergeNow = action { return "\(pr.repo)#\(pr.number) jetzt mergen?" }
        return "Auto-Merge für #\(pr.number)"
    }
}

extension View {
    func prActions(_ pr: OpenPR, snapshot: Snapshot) -> some View {
        modifier(PRActions(pr: pr, snapshot: snapshot))
    }
}
