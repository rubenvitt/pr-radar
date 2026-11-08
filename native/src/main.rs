//! PR Radar – native Desktop-App für Pull Requests, Pipelines, Auto-Merge, Merges und Releases.

mod config;
mod github;
mod logging;
mod model;
mod radar;
mod time;
mod ui;

use std::borrow::Cow;
use std::sync::Arc;

use gpui_kit::component::Theme;
use gpui_kit::*;

use github::GitHub;
use radar::Radar;

// Nur die zusätzlich genutzten Lucide-Icons einbetten, nicht den ganzen Katalog.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        BellOff,
        BookMarked,
        CircleDashed,
        Eye,
        GitBranch,
        GitMerge,
        GitPullRequest,
        GitPullRequestDraft,
        Hourglass,
        LayoutDashboard,
        Layers,
        List,
        LoaderCircle,
        MessageSquareWarning,
        ShieldCheck,
        Tag,
        Trash,
        UserCheck,
        Workflow,
        Zap
    ]
);

/// Handgezeichnete Icons (icons8 „Claude Hand Drawn“, nur Kontur). Sie liegen unter den
/// Lucide-Pfaden und ersetzen so auch die Icons der eingebauten Komponenten.
#[derive(rust_embed::RustEmbed)]
#[folder = "assets/"]
#[include = "icons/*.svg"]
struct HandDrawn;

struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(file) = HandDrawn::get(path) {
            return Ok(Some(file.data));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.extend(
            HandDrawn::iter()
                .filter(|p| p.starts_with(path))
                .map(|p| SharedString::from(p.to_string())),
        );
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

fn main() {
    logging::init();
    let app = gpui_kit::application().with_assets(AppAssets);
    // Klick aufs Dock-Symbol, nachdem das Fenster mit ⌘W geschlossen wurde.
    app.on_reopen(ui::show_main_window);
    app.run(|cx| {
        gpui_kit::init(cx);
        Theme::sync_system_appearance(None, cx);

        // Ein Client für GraphQL und Avatare (img(url) braucht einen registrierten Client).
        let http = match reqwest_client::ReqwestClient::user_agent("pr-radar-native") {
            Ok(client) => Arc::new(client),
            Err(e) => logging::fatal("Netzwerkzugriff nicht möglich", e),
        };
        cx.set_http_client(http.clone());
        let github = Arc::new(GitHub::new(http));
        let radar = cx.new(|cx| Radar::new(github, cx));

        ui::init(radar.clone(), cx);
        if let Err(e) = ui::open_main_window(radar, cx) {
            logging::fatal("Fenster konnte nicht geöffnet werden", e);
        }
        cx.activate(true);
    });
}
