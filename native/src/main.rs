//! PR Radar – native Desktop-App für Pull Requests, Pipelines, Auto-Merge, Merges und Releases.

#[macro_use]
mod l10n;
mod config;
mod demo;
mod github;
mod logging;
#[cfg(target_os = "macos")]
mod macos;
mod menubar;
mod model;
mod notices;
mod radar;
mod site;
mod time;
mod ui;

use std::borrow::Cow;
use std::sync::Arc;

use gpui_kit::component::Theme;
use gpui_kit::*;

use github::GitHub;
use radar::Radar;

/// Eigenes Icon-Set: icons8 „Material Rounded“ (24er-Raster, gefüllt). Die Dateien liegen unter
/// den Lucide-Pfaden (`icons/<name>.svg`) und ersetzen so auch die Icons der eingebauten
/// Komponenten. Jedes Icon, das die App zeigt, braucht hier eine Datei, sonst fiele es auf den
/// Lucide-Stil von gpui-kit zurück bzw. bliebe leer (Test `every_used_icon_has_a_file`).
#[derive(rust_embed::RustEmbed)]
#[folder = "assets/"]
#[include = "icons/*.svg"]
struct AppIcons;

struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(file) = AppIcons::get(path) {
            return Ok(Some(file.data));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(
            AppIcons::iter()
                .filter(|p| p.starts_with(path))
                .map(|p| SharedString::from(p.to_string())),
        );
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

fn main() {
    l10n::init();
    logging::init();
    let demo = demo::Options::from_env();
    if demo.enabled {
        log::info!("Demo-Modus: Beispieldaten, kein GitHub-Zugriff");
    }
    let app = gpui_kit::application().with_assets(AppAssets);
    // Klick aufs Dock-Symbol, nachdem das Fenster mit ⌘W geschlossen wurde.
    app.on_reopen(ui::show_main_window);
    app.run(|cx| {
        gpui_kit::init(cx);
        ui::theme::init(cx);
        // Danach die passende Hälfte des eigenen Schemas laden: erzwungen (Demo) oder vom System.
        match demo.appearance {
            Some(mode) => Theme::change(mode, None, cx),
            None => Theme::sync_system_appearance(None, cx),
        }
        // macOS-Maß statt der 16 px von gpui-component; Hell/Dunkel-Wechsel lassen das stehen,
        // weil die mitgelieferten Themes keine Schriftgröße setzen.
        Theme::update(cx, |theme| theme.font_size = ui::typography::BASE);

        // Ein Client für GraphQL und Avatare (img(url) braucht einen registrierten Client).
        let http = match reqwest_client::ReqwestClient::user_agent("pr-radar-native") {
            Ok(client) => Arc::new(client),
            Err(e) => logging::fatal(
                tr!(
                    "Network access unavailable",
                    "Netzwerkzugriff nicht möglich"
                ),
                e,
            ),
        };
        cx.set_http_client(http.clone());
        let github = Arc::new(GitHub::new(http));
        let radar = cx.new(|cx| Radar::new(github, &demo, cx));
        cx.set_global(demo);

        ui::init(radar.clone(), cx);
        #[cfg(target_os = "macos")]
        menubar::init(radar.clone(), cx);

        // Beim Anmelden still starten: Der Radar pollt, das Menüleisten-Symbol zeigt
        // die Zahl. Menüleiste und Dock-Klick öffnen das Fenster (`show_main_window`).
        #[cfg(target_os = "macos")]
        if macos::launched_as_login_item() {
            log::info!("Als Anmeldeobjekt gestartet, ohne Fenster");
            return;
        }
        if let Err(e) = ui::open_main_window(radar, cx) {
            logging::fatal(
                tr!(
                    "Could not open the window",
                    "Fenster konnte nicht geöffnet werden"
                ),
                e,
            );
        }
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::AppIcons;
    use std::collections::BTreeSet;
    use std::path::Path;

    /// Icons, die gpui-kit-Komponenten intern zeichnen (Benachrichtigung, Sheet, Menü,
    /// Spinner, Eingabefeld). Sie stehen nicht im App-Code, sind aber im Fenster zu sehen.
    const COMPONENT_ICONS: &[&str] = &[
        "Check",
        "ChevronRight",
        "CircleCheck",
        "CircleX",
        "Close",
        "Info",
        "Loader",
        "TriangleAlert",
    ];

    /// `GitPullRequestDraft` → `git-pull-request-draft`, `Settings2` → `settings-2`.
    fn kebab(name: &str) -> String {
        let mut out = String::new();
        let mut prev_digit = false;
        for (i, c) in name.chars().enumerate() {
            let digit = c.is_ascii_digit();
            if i > 0 && (c.is_ascii_uppercase() || (digit && !prev_digit)) {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
            prev_digit = digit;
        }
        out
    }

    fn collect(dir: &Path, names: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(&path, names);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let src = std::fs::read_to_string(&path).unwrap();
                for marker in ["Lucide::", "IconName::"] {
                    for (pos, _) in src.match_indices(marker) {
                        let ident: String = src[pos + marker.len()..]
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric())
                            .collect();
                        if ident.starts_with(|c: char| c.is_ascii_uppercase()) {
                            names.insert(ident);
                        }
                    }
                }
            }
        }
    }

    /// Pfade aller Icons, die im Fenster erscheinen können (App-Code und Komponenten).
    fn used_icon_paths() -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        collect(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut names,
        );
        names.extend(COMPONENT_ICONS.iter().map(|s| s.to_string()));
        assert!(names.len() > 20, "zu wenige Icons gefunden: {names:?}");
        names
            .iter()
            .map(|n| format!("icons/{}.svg", kebab(n)))
            .collect()
    }

    #[test]
    fn kebab_matches_lucide_paths() {
        assert_eq!(kebab("GitPullRequestDraft"), "git-pull-request-draft");
        assert_eq!(kebab("Settings2"), "settings-2");
        assert_eq!(kebab("Close"), "close");
    }

    /// Nur ein Icon-Stil im Fenster: Jedes genutzte Icon kommt aus `assets/icons`.
    #[test]
    fn every_used_icon_has_a_file() {
        let missing: Vec<String> = used_icon_paths()
            .into_iter()
            .filter(|p| AppIcons::get(p).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "Icons ohne Datei in assets/icons: {missing:?}"
        );
    }

    /// Keine Datei ohne Nutzer, damit das Set nicht wieder ausfranst.
    #[test]
    fn no_unused_icon_files() {
        let used = used_icon_paths();
        let unused: Vec<String> = AppIcons::iter()
            .map(|p| p.to_string())
            .filter(|p| !used.contains(p))
            .collect();
        assert!(unused.is_empty(), "Icon-Dateien ohne Nutzung: {unused:?}");
    }
}
