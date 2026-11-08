//! Demo-Modus: fester Beispielstand ohne GitHub, Token und Netzwerk.
//!
//! Start mit `cargo run -- --demo` oder `PR_RADAR_DEMO=1`. Der Radar lädt dann statt GitHub
//! den Snapshot aus [`snapshot`], liest und schreibt keine `config.json`, und Merge-Aktionen
//! zeigen nur einen Hinweis. Gedacht für Screenshots (Vorher/Nachher, Produktseite) und zum
//! Ausprobieren ohne Login.
//!
//! Zusätzliche Schalter (nur im Demo-Modus wirksam):
//! - `--dark` / `--light` bzw. `PR_RADAR_APPEARANCE=dark|light`: Erscheinungsbild erzwingen
//! - `--tab open|merged|releases` bzw. `PR_RADAR_TAB`: Startansicht
//! - `--view list|flow` bzw. `PR_RADAR_VIEW`: Liste oder Flow
//! - `--quick all|running|failed|passed|auto|conflict|review|mine` bzw. `PR_RADAR_QUICK`: Schnellfilter

use chrono::{DateTime, Duration, Utc};
use gpui_kit::component::ThemeMode;
use serde::de::DeserializeOwned;

use crate::config::{Config, Prefs};
use crate::model::{
    AutoMerge, Check, CheckState, Label, MergeMethod, Mergeable, MergedPr, OpenPr, Person,
    Pipeline, PipelineState, Release, RepoInfo, ReviewDecision, Snapshot,
};

/// Angemeldeter Nutzer im Demo-Stand („Von dir“, „Dein Review“).
pub const VIEWER: &str = "alex";

/// Startoptionen aus Kommandozeile und Umgebung.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options {
    pub enabled: bool,
    pub appearance: Option<ThemeMode>,
    pub prefs: Prefs,
}

/// Global gesetzt, damit neu geöffnete Fenster ein erzwungenes Erscheinungsbild behalten.
impl gpui_kit::Global for Options {}

impl Options {
    /// Liest die Optionen aus `std::env::args()` und den Umgebungsvariablen.
    pub fn from_env() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        Self::parse(&args, |key| std::env::var(key).ok())
    }

    fn parse(args: &[String], env: impl Fn(&str) -> Option<String>) -> Self {
        // Wert eines Schalters: `--tab merged`, `--tab=merged` oder Umgebungsvariable.
        let value = |flag: &str, var: &str| -> Option<String> {
            let mut iter = args.iter();
            while let Some(a) = iter.next() {
                if a == flag {
                    return iter.next().cloned();
                }
                if let Some(v) = a.strip_prefix(flag).and_then(|r| r.strip_prefix('=')) {
                    return Some(v.to_string());
                }
            }
            env(var).filter(|v| !v.is_empty())
        };
        let has = |flag: &str| args.iter().any(|a| a == flag);

        let enabled = has("--demo") || env("PR_RADAR_DEMO").is_some_and(|v| truthy(&v));
        if !enabled {
            return Self::default();
        }

        let appearance = if has("--dark") {
            Some(ThemeMode::Dark)
        } else if has("--light") {
            Some(ThemeMode::Light)
        } else {
            match value("--appearance", "PR_RADAR_APPEARANCE").as_deref() {
                Some("dark") => Some(ThemeMode::Dark),
                Some("light") => Some(ThemeMode::Light),
                _ => env("PR_RADAR_DARK")
                    .filter(|v| truthy(v))
                    .map(|_| ThemeMode::Dark),
            }
        };

        let mut prefs = Prefs::default();
        if let Some(tab) = value("--tab", "PR_RADAR_TAB").and_then(|v| enum_value(&v)) {
            prefs.tab = tab;
        }
        if let Some(view) = value("--view", "PR_RADAR_VIEW").and_then(|v| enum_value(&v)) {
            prefs.view = view;
        }
        if let Some(quick) = value("--quick", "PR_RADAR_QUICK").and_then(|v| enum_value(&v)) {
            prefs.quick = quick;
        }

        Self {
            enabled,
            appearance,
            prefs,
        }
    }

    /// Konfiguration für den Demo-Stand: Repos aus dem Snapshot, Einstellungen aus den Schaltern.
    /// Sie wird nie gespeichert (`read_only`).
    pub fn config(&self) -> Config {
        Config {
            repos: REPOS.iter().map(|(name, _)| name.to_string()).collect(),
            prefs: self.prefs.clone(),
            read_only: true,
        }
    }
}

fn truthy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "ja" | "on"
    )
}

/// Wert eines Einstellungs-Enums über dieselben Namen wie in `config.json`.
fn enum_value<T: DeserializeOwned>(v: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(v.trim().to_ascii_lowercase())).ok()
}

/// Beobachtete Repos; `Some` = Fehlermeldung (Repo nicht erreichbar).
const REPOS: [(&str, Option<&str>); 5] = [
    ("acme/web", None),
    ("acme/api", None),
    ("acme/mobile", None),
    ("acme/infra", None),
    (
        "acme/legacy-billing",
        Some("Repository nicht gefunden oder kein Zugriff mit diesem Token."),
    ),
];

/// Hinweis, wenn im Demo-Modus eine Aktion ausgelöst wird, die GitHub ändern würde.
pub fn action_notice(what: &str) -> String {
    format!("{what} – im Demo-Modus passiert nichts. Starte PR Radar ohne --demo für echte Daten.")
}

fn person(login: &str) -> Person {
    // Ohne Avatar-URL zeigt die App Initialen – kein Abruf von avatars.githubusercontent.com.
    Person {
        login: login.into(),
        avatar_url: String::new(),
    }
}

fn label(name: &str, color: &str) -> Label {
    Label {
        name: name.into(),
        color: color.into(),
    }
}

fn check(name: &str, group: &str, state: CheckState) -> Check {
    Check {
        name: name.into(),
        state,
        url: None,
        group: Some(group.into()),
    }
}

fn pipeline(checks: Vec<Check>) -> Pipeline {
    let count = |s: CheckState| checks.iter().filter(|c| c.state == s).count();
    let (passed, failed, running) = (
        count(CheckState::Passed),
        count(CheckState::Failed),
        count(CheckState::Running),
    );
    let state = if failed > 0 {
        PipelineState::Failed
    } else if running > 0 {
        PipelineState::Running
    } else if passed > 0 {
        PipelineState::Passed
    } else {
        PipelineState::None
    };
    Pipeline {
        state,
        total: checks.len(),
        passed,
        failed,
        running,
        checks,
    }
}

fn ci(lint: CheckState, test: CheckState, build: CheckState) -> Pipeline {
    pipeline(vec![
        check("lint", "CI", lint),
        check("test (ubuntu-latest)", "CI", test),
        check("build", "CI", build),
        check("CodeQL", "Security", CheckState::Passed),
    ])
}

struct Pr<'a> {
    repo: &'a str,
    number: u64,
    title: &'a str,
    author: &'a str,
    head_ref: &'a str,
    minutes_ago: i64,
    diff: (u64, u64),
    pipeline: Pipeline,
}

impl Pr<'_> {
    fn build(self, now: DateTime<Utc>) -> OpenPr {
        OpenPr {
            id: format!("demo-{}-{}", self.repo, self.number),
            repo: self.repo.into(),
            number: self.number,
            title: self.title.into(),
            url: format!("https://github.com/{}/pull/{}", self.repo, self.number),
            author: Some(person(self.author)),
            is_draft: false,
            updated_at: now - Duration::minutes(self.minutes_ago),
            head_ref: self.head_ref.into(),
            head_oid: format!("{:040x}", self.number),
            base_ref: "main".into(),
            additions: self.diff.0,
            deletions: self.diff.1,
            review_decision: None,
            mergeable: Mergeable::Mergeable,
            merge_state_status: "BLOCKED".into(),
            auto_merge: None,
            labels: Vec::new(),
            review_requests: Vec::new(),
            pipeline: self.pipeline,
            merge_methods: vec![MergeMethod::Squash, MergeMethod::Merge],
        }
    }
}

/// Fester Beispielstand relativ zu `now`: 7 offene PRs in 4 Repos (Konflikt, Auto-Merge,
/// Draft, Review-Anfrage, rot/läuft/grün), ein nicht erreichbares Repo, 7 gemergte PRs
/// über drei Tage und 2 Releases.
pub fn snapshot(now: DateTime<Utc>) -> Snapshot {
    use CheckState::{Failed as F, Passed as P, Running as R, Skipped as S};

    let repos = REPOS
        .iter()
        .map(|(name, error)| match error {
            Some(e) => RepoInfo::unavailable(name, e.to_string()),
            None => RepoInfo {
                full_name: name.to_string(),
                url: format!("https://github.com/{name}"),
                default_branch: Some("main".into()),
                default_branch_pipeline: if *name == "acme/api" {
                    PipelineState::Running
                } else {
                    PipelineState::Passed
                },
                auto_merge_allowed: true,
                merge_methods: vec![MergeMethod::Squash, MergeMethod::Merge],
                viewer_can_merge: true,
                error: None,
            },
        })
        .collect();

    // Bereit zum Mergen, Review erteilt.
    let mut ready = Pr {
        repo: "acme/web",
        number: 412,
        title: "Checkout: Adressformular validiert Postleitzahlen",
        author: "kim",
        head_ref: "feat/plz-validierung",
        minutes_ago: 12,
        diff: (184, 37),
        pipeline: ci(P, P, P),
    }
    .build(now);
    ready.review_decision = Some(ReviewDecision::Approved);
    ready.merge_state_status = "CLEAN".into();
    ready.labels = vec![label("feature", "a2eeef"), label("frontend", "1d76db")];

    // Rote Pipeline, eigener PR.
    let mut failing = Pr {
        repo: "acme/web",
        number: 409,
        title: "Navigation auf neue Routing-API umstellen",
        author: VIEWER,
        head_ref: "refactor/router",
        minutes_ago: 47,
        diff: (623, 410),
        pipeline: ci(P, F, P),
    }
    .build(now);
    failing.review_decision = Some(ReviewDecision::ChangesRequested);
    failing.labels = vec![label("refactor", "fbca04")];

    // Pipeline läuft, Auto-Merge an.
    let mut auto = Pr {
        repo: "acme/api",
        number: 1288,
        title: "Rate-Limit pro API-Schlüssel statt pro IP",
        author: "jo",
        head_ref: "feat/rate-limit-key",
        minutes_ago: 3,
        diff: (241, 58),
        pipeline: pipeline(vec![
            check("lint", "CI", P),
            check("test (postgres-16)", "CI", R),
            check("test (postgres-17)", "CI", R),
            check("build", "CI", P),
            check("CodeQL", "Security", P),
        ]),
    }
    .build(now);
    auto.review_decision = Some(ReviewDecision::Approved);
    auto.auto_merge = Some(AutoMerge {
        method: MergeMethod::Squash,
        enabled_by: Some(VIEWER.into()),
    });
    auto.labels = vec![label("backend", "5319e7")];

    // Review vom Nutzer angefragt.
    let mut review = Pr {
        repo: "acme/api",
        number: 1291,
        title: "Webhook-Zustellung mit Wiederholung und Dead-Letter-Queue",
        author: "sam",
        head_ref: "feat/webhook-retry",
        minutes_ago: 95,
        diff: (512, 76),
        pipeline: ci(P, P, P),
    }
    .build(now);
    review.review_decision = Some(ReviewDecision::ReviewRequired);
    review.review_requests = vec![VIEWER.into(), "jo".into()];
    review.labels = vec![label("backend", "5319e7"), label("needs-review", "d93f0b")];

    // Merge-Konflikt.
    let mut conflict = Pr {
        repo: "acme/mobile",
        number: 77,
        title: "Push-Benachrichtigungen: Einstellungen pro Kanal",
        author: "lee",
        head_ref: "feat/push-kanaele",
        minutes_ago: 60 * 26,
        diff: (330, 122),
        pipeline: ci(P, P, S),
    }
    .build(now);
    conflict.mergeable = Mergeable::Conflicting;
    conflict.merge_state_status = "DIRTY".into();
    conflict.review_decision = Some(ReviewDecision::Approved);

    // Draft.
    let mut draft = Pr {
        repo: "acme/mobile",
        number: 79,
        title: "Offline-Modus für die Auftragsliste (Entwurf)",
        author: VIEWER,
        head_ref: "spike/offline",
        minutes_ago: 60 * 5,
        diff: (1094, 12),
        pipeline: pipeline(vec![
            check("lint", "CI", P),
            check("test (ios)", "CI", P),
            check("test (android)", "CI", R),
        ]),
    }
    .build(now);
    draft.is_draft = true;
    draft.merge_state_status = "DRAFT".into();

    // Infrastruktur, grün, ohne Review-Pflicht, nur Squash erlaubt.
    let mut infra = Pr {
        repo: "acme/infra",
        number: 58,
        title: "Terraform: Postgres auf 17 heben",
        author: "dependabot[bot]",
        head_ref: "dependabot/terraform/postgres-17",
        minutes_ago: 60 * 3,
        diff: (4, 4),
        pipeline: pipeline(vec![
            check("terraform fmt", "Terraform", P),
            check("terraform plan", "Terraform", P),
        ]),
    }
    .build(now);
    infra.merge_state_status = "CLEAN".into();
    infra.merge_methods = vec![MergeMethod::Squash];
    infra.labels = vec![label("dependencies", "0366d6")];

    let merged = [
        (
            "acme/web",
            410,
            "Footer: Links auf neue Hilfeseiten",
            "kim",
            25,
        ),
        (
            "acme/api",
            1285,
            "Logging: Request-ID in allen Antworten",
            "jo",
            80,
        ),
        (
            "acme/api",
            1284,
            "Abhängigkeiten aktualisieren",
            "dependabot[bot]",
            190,
        ),
        (
            "acme/mobile",
            75,
            "Login: Face ID als Option",
            "lee",
            60 * 22,
        ),
        (
            "acme/web",
            405,
            "Dunkelmodus für das Dashboard",
            VIEWER,
            60 * 27,
        ),
        (
            "acme/infra",
            56,
            "CDN-Cache für Bilder auf 7 Tage",
            "sam",
            60 * 30,
        ),
        (
            "acme/api",
            1279,
            "Pagination für /v2/orders",
            "sam",
            60 * 50,
        ),
    ]
    .into_iter()
    .map(|(repo, number, title, author, minutes_ago)| MergedPr {
        id: format!("demo-merged-{repo}-{number}"),
        repo: repo.into(),
        number,
        title: title.into(),
        url: format!("https://github.com/{repo}/pull/{number}"),
        author: Some(person(author)),
        merged_at: now - Duration::minutes(minutes_ago),
        merged_by: Some(
            if author.ends_with("[bot]") {
                VIEWER
            } else {
                author
            }
            .into(),
        ),
        base_ref: "main".into(),
    })
    .collect();

    let releases = vec![
        Release {
            id: "demo-release-api-2.14.0".into(),
            repo: "acme/api".into(),
            name: "v2.14.0".into(),
            tag_name: "v2.14.0".into(),
            url: "https://github.com/acme/api/releases/tag/v2.14.0".into(),
            published_at: Some(now - Duration::hours(20)),
            is_prerelease: false,
            is_latest: true,
            author: Some("jo".into()),
            description_html: "<h2>Neu</h2><ul><li>Pagination für <code>/v2/orders</code></li>\
                <li>Request-ID in allen Antworten</li></ul>\
                <h2>Behoben</h2><ul><li>Zeitzonen bei Exporten</li></ul>"
                .into(),
        },
        Release {
            id: "demo-release-mobile-3.0.0-beta.2".into(),
            repo: "acme/mobile".into(),
            name: "3.0 Beta 2".into(),
            tag_name: "v3.0.0-beta.2".into(),
            url: "https://github.com/acme/mobile/releases/tag/v3.0.0-beta.2".into(),
            published_at: Some(now - Duration::days(3)),
            is_prerelease: true,
            is_latest: false,
            author: Some("lee".into()),
            description_html: "<p>Zweite Beta mit Face ID und überarbeiteter Auftragsliste.</p>\
                <h3>Bekannte Probleme</h3><ul><li>Push-Einstellungen werden noch nicht synchronisiert.</li></ul>"
                .into(),
        },
    ];

    Snapshot {
        viewer: Some(VIEWER.into()),
        repos,
        open: vec![failing, auto, review, ready, conflict, draft, infra],
        merged,
        releases,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Quick, Tab, View};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ohne_schalter_kein_demo() {
        let o = Options::parse(&args(&["--dark", "--tab", "merged"]), |_| None);
        assert_eq!(o, Options::default());
        let o = Options::parse(&[], |k| (k == "PR_RADAR_DEMO").then(|| "0".into()));
        assert!(!o.enabled);
    }

    #[test]
    fn demo_per_schalter_oder_umgebung() {
        assert!(Options::parse(&args(&["--demo"]), |_| None).enabled);
        assert!(Options::parse(&[], |k| (k == "PR_RADAR_DEMO").then(|| "1".into())).enabled);
        assert!(Options::parse(&[], |k| (k == "PR_RADAR_DEMO").then(|| "true".into())).enabled);
    }

    #[test]
    fn startansicht_und_erscheinungsbild() {
        let o = Options::parse(
            &args(&[
                "--demo",
                "--dark",
                "--tab",
                "releases",
                "--view=flow",
                "--quick",
                "failed",
            ]),
            |_| None,
        );
        assert_eq!(o.appearance, Some(ThemeMode::Dark));
        assert_eq!(o.prefs.tab, Tab::Releases);
        assert_eq!(o.prefs.view, View::Flow);
        assert_eq!(o.prefs.quick, Quick::Failed);

        let env = |k: &str| match k {
            "PR_RADAR_DEMO" => Some("1".to_string()),
            "PR_RADAR_DARK" => Some("1".to_string()),
            "PR_RADAR_TAB" => Some("Merged".to_string()),
            _ => None,
        };
        let o = Options::parse(&[], env);
        assert_eq!(o.appearance, Some(ThemeMode::Dark));
        assert_eq!(o.prefs.tab, Tab::Merged);

        // Unbekannte Werte fallen auf den Standard zurück.
        let o = Options::parse(&args(&["--demo", "--tab", "quatsch"]), |_| None);
        assert_eq!(o.prefs.tab, Tab::Open);
        assert_eq!(o.appearance, None);
    }

    #[test]
    fn konfiguration_wird_nie_gespeichert() {
        let o = Options::parse(&args(&["--demo"]), |_| None);
        let config = o.config();
        assert!(config.read_only);
        assert!(config.save().is_err());
        assert_eq!(config.repos.len(), REPOS.len());
    }

    #[test]
    fn beispielstand_deckt_alle_zustaende_ab() {
        let s = snapshot(Utc::now());
        assert_eq!(s.open.len(), 7);
        assert_eq!(s.merged.len(), 7);
        assert_eq!(s.releases.len(), 2);
        let repos: std::collections::HashSet<_> = s.open.iter().map(|p| &p.repo).collect();
        assert_eq!(repos.len(), 4);

        let viewer = s.viewer.as_deref();
        assert!(s.open.iter().any(|p| p.has_conflict()));
        assert!(s.open.iter().any(|p| p.auto_merge.is_some()));
        assert!(s.open.iter().any(|p| p.is_draft));
        assert!(s.open.iter().any(|p| p.ready_to_merge()));
        assert!(s.open.iter().any(|p| p.requests_review_from(viewer)));
        assert!(s.open.iter().any(|p| p.is_by(viewer)));
        assert!(s.failed().count() >= 1);
        assert!(s.any_running());
        assert!(s.repos.iter().any(|r| r.error.is_some()));
        assert!(s.releases.iter().any(|r| r.is_prerelease));

        // Jeder offene PR gehört zu einem erreichbaren Repo, IDs sind eindeutig.
        for pr in &s.open {
            assert!(s.repo(&pr.repo).is_some_and(|r| r.error.is_none()));
        }
        let ids: std::collections::HashSet<_> = s.open.iter().map(|p| &p.id).collect();
        assert_eq!(ids.len(), s.open.len());
        // Kein Avatar wird aus dem Netz geladen.
        assert!(
            s.open
                .iter()
                .filter_map(|p| p.author.as_ref())
                .all(|a| a.avatar_url.is_empty())
        );
    }
}
