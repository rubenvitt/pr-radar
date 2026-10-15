// Tabs (auf dem iPad als Seitenleiste), Hinweis-Banner und Tastaturkürzel.

import PRRadarCore
import SwiftUI

struct RootView: View {
    @Environment(AppState.self) private var app
    @Environment(Radar.self) private var radar

    var body: some View {
        TabView(selection: Binding(get: { radar.prefs.tab }, set: { t in radar.updatePrefs { $0.tab = t } })) {
            SwiftUI.Tab("Offen", systemImage: "arrow.triangle.pull", value: RadarTab.open) {
                OpenPRsView()
            }
            .badge(failedCount)

            SwiftUI.Tab("Gemergt", systemImage: "arrow.triangle.merge", value: RadarTab.merged) {
                MergedView()
            }

            SwiftUI.Tab("Releases", systemImage: "tag", value: RadarTab.releases) {
                ReleasesView()
            }

            SwiftUI.Tab("Einstellungen", systemImage: "gearshape", value: RadarTab.settings) {
                SettingsView()
            }
        }
        .tabViewStyle(.sidebarAdaptable)
        .overlay(alignment: .top) { ToastOverlay() }
        .background { KeyboardShortcuts() }
    }

    /// Rote PRs als Badge am Tab
    private var failedCount: Int {
        radar.snapshot?.open.filter { $0.pipeline.state == .failed && (radar.prefs.repoFilter.isEmpty || radar.prefs.repoFilter.contains($0.repo)) }.count ?? 0
    }
}

/// Banner oben für fehlgeschlagene Aktionen und „Alle mergen“ – mit Haptik.
private struct ToastOverlay: View {
    @Environment(AppState.self) private var app

    var body: some View {
        VStack {
            if let toast = app.toast {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: symbol(toast.kind))
                        .foregroundStyle(color(toast.kind))
                        .font(.title3)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(toast.title).font(.subheadline.weight(.semibold))
                        if !toast.message.isEmpty {
                            Text(toast.message).font(.footnote).foregroundStyle(.secondary).lineLimit(6)
                        }
                    }
                    Spacer(minLength: 0)
                }
                .padding(14)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
                .shadow(color: .black.opacity(0.15), radius: 12, y: 4)
                .padding(.horizontal)
                .frame(maxWidth: 520)
                .transition(.move(edge: .top).combined(with: .opacity))
                .onTapGesture { app.toast = nil }
                .task(id: toast.id) {
                    try? await Task.sleep(for: .seconds(toast.kind == .error ? 6 : 3.5))
                    if app.toast?.id == toast.id { app.toast = nil }
                }
                .accessibilityAddTraits(.isStaticText)
            }
        }
        .animation(.spring(duration: 0.35), value: app.toast)
        .sensoryFeedback(trigger: app.toast) { _, new in
            switch new?.kind {
            case .error: .error
            case .success: .success
            case .info: .impact
            case nil: nil
            }
        }
    }

    private func symbol(_ kind: Toast.Kind) -> String {
        switch kind {
        case .success: "checkmark.circle.fill"
        case .error: "exclamationmark.octagon.fill"
        case .info: "info.circle.fill"
        }
    }

    private func color(_ kind: Toast.Kind) -> Color {
        switch kind {
        case .success: .green
        case .error: .red
        case .info: .accentColor
        }
    }
}

/// Tastatur (iPad): ⌘R neu laden, ⌘1–4 Tabs, ⌘L Liste/Flow, ⌘G gruppieren.
private struct KeyboardShortcuts: View {
    @Environment(Radar.self) private var radar

    var body: some View {
        Group {
            Button("Neu laden") { radar.refresh() }.keyboardShortcut("r")
            Button("Offen") { radar.updatePrefs { $0.tab = .open } }.keyboardShortcut("1")
            Button("Gemergt") { radar.updatePrefs { $0.tab = .merged } }.keyboardShortcut("2")
            Button("Releases") { radar.updatePrefs { $0.tab = .releases } }.keyboardShortcut("3")
            Button("Einstellungen") { radar.updatePrefs { $0.tab = .settings } }.keyboardShortcut("4")
            Button("Liste/Flow") { radar.updatePrefs { $0.view = $0.view == .list ? .flow : .list } }.keyboardShortcut("l")
            Button("Gruppieren") { radar.updatePrefs { $0.grouped.toggle() } }.keyboardShortcut("g")
        }
        .opacity(0)
        .frame(width: 0, height: 0)
        .accessibilityHidden(true)
    }
}
