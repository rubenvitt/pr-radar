//! Konfiguration und Ansichts-Einstellungen in `~/Library/Application Support/pr-radar/config.json`.

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tab {
    #[default]
    Open,
    Merged,
    Releases,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quick {
    #[default]
    All,
    Running,
    Failed,
    Passed,
    Auto,
    Conflict,
    Review,
    Mine,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum View {
    #[default]
    List,
    Flow,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub tab: Tab,
    pub quick: Quick,
    pub view: View,
    pub grouped: bool,
    pub repo_filter: Vec<String>,
    pub notify: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            tab: Tab::Open,
            quick: Quick::All,
            view: View::List,
            grouped: true,
            repo_filter: Vec::new(),
            notify: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub repos: Vec<String>,
    pub prefs: Prefs,
}

fn path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pr-radar")
        .join("config.json")
}

impl Config {
    /// Lädt die Konfiguration. Beim ersten Start werden die Repos der Web-App
    /// (`data/config.json`) bzw. `REPOS=owner/a,owner/b` übernommen.
    pub fn load() -> Self {
        if let Some(cfg) = std::fs::read_to_string(path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
        {
            return cfg;
        }
        let legacy = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/config.json");
        let mut cfg = std::fs::read_to_string(legacy)
            .ok()
            .and_then(|s| serde_json::from_str::<Config>(&s).ok())
            .unwrap_or_default();
        if cfg.repos.is_empty() {
            cfg.repos = std::env::var("REPOS")
                .unwrap_or_default()
                .split(',')
                .filter_map(parse_repo)
                .collect();
        }
        let _ = cfg.save();
        cfg
    }

    pub fn save(&self) -> Result<()> {
        let path = path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).context("Konfigurationsordner nicht anlegbar")?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(self)? + "\n")
            .context("Konfiguration nicht speicherbar")
    }
}

fn valid_part(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Akzeptiert "owner/name", "https://github.com/owner/name(.git)(/...)" und "git@github.com:owner/name.git".
pub fn parse_repo(input: &str) -> Option<String> {
    let s = input.trim();
    let s = s.strip_prefix("git@github.com:").unwrap_or(s);
    let s = [
        "https://www.github.com/",
        "http://www.github.com/",
        "https://github.com/",
        "http://github.com/",
    ]
    .iter()
    .find_map(|p| s.strip_prefix(p))
    .unwrap_or(s);
    let mut parts = s.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    let name = name.strip_suffix(".git").unwrap_or(name);
    (valid_part(owner) && valid_part(name)).then(|| format!("{owner}/{name}"))
}

#[cfg(test)]
mod tests {
    use super::parse_repo;

    #[test]
    fn parst_repo_angaben() {
        let cases = [
            ("rubeen/pr-radar", Some("rubeen/pr-radar")),
            (
                "https://github.com/rubeen/pr-radar",
                Some("rubeen/pr-radar"),
            ),
            (
                "https://github.com/rubeen/pr-radar/pulls",
                Some("rubeen/pr-radar"),
            ),
            (
                "git@github.com:rubeen/pr-radar.git",
                Some("rubeen/pr-radar"),
            ),
            ("kaputt", None),
            ("a/b c", None),
        ];
        for (input, out) in cases {
            assert_eq!(parse_repo(input).as_deref(), out, "{input}");
        }
    }
}
