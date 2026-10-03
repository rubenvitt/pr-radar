//! PR Radar – native Desktop-App für Pull Requests, Pipelines, Auto-Merge, Merges und Releases.

mod config;
mod github;
mod model;
mod radar;
mod time;
mod ui;

use std::borrow::Cow;
use std::sync::Arc;

use gpui_kit::component::{Theme, TitleBar};
use gpui_kit::*;

use github::GitHub;
use radar::Radar;
use ui::Workspace;

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
    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        gpui_kit::init(cx);
        Theme::sync_system_appearance(None, cx);

        // Ein Client für GraphQL und Avatare (img(url) braucht einen registrierten Client).
        let http = Arc::new(
            reqwest_client::ReqwestClient::user_agent("pr-radar-native")
                .expect("HTTP-Client nicht initialisierbar"),
        );
        cx.set_http_client(http.clone());
        let github = Arc::new(GitHub::new(http));
        let radar = cx.new(|cx| Radar::new(github, cx));

        ui::init(cx);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1240.), px(820.)), cx)),
            window_min_size: Some(size(px(360.), px(480.))),
            ..TitleBar::window_options()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            window
                .observe_window_appearance(|window, cx| {
                    Theme::sync_system_appearance(Some(window), cx)
                })
                .detach();
            cx.new(|cx| Workspace::new(radar, window, cx))
        })
        .expect("Fenster konnte nicht geöffnet werden");
        cx.activate(true);
    });
}
