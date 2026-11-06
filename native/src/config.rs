//! Konfiguration und Ansichts-Einstellungen in `~/Library/Application Support/pr-radar/config.json`.

use std::path::{Path, PathBuf};

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
    /// Seitenleiste im breiten Fenster ausgeblendet (⌘B)
    pub sidebar_hidden: bool,
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
            sidebar_hidden: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub repos: Vec<String>,
    pub prefs: Prefs,
    /// Die vorhandene Datei war defekt und ließ sich nicht sichern: nichts speichern.
    #[serde(skip)]
    pub read_only: bool,
}

fn path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pr-radar")
        .join("config.json")
}

/// Ergebnis von [`Config::load`]: die Konfiguration und ggf. ein Hinweis für den Nutzer.
pub struct Loaded {
    pub config: Config,
    /// Gesetzt, wenn eine vorhandene `config.json` nicht lesbar war.
    pub warning: Option<String>,
}

impl Config {
    /// Lädt die Konfiguration. Beim ersten Start werden die Repos aus
    /// `REPOS=owner/a,owner/b` übernommen.
    ///
    /// Eine vorhandene, aber nicht lesbare `config.json` wird nie still überschrieben:
    /// Sie wird als `config.json.defekt-<Zeitstempel>` gesichert, und der Aufrufer bekommt
    /// einen Hinweis. Lässt sie sich nicht sichern, startet die App mit Standardwerten,
    /// speichert aber nichts, damit die Datei erhalten bleibt.
    pub fn load() -> Loaded {
        let env_repos = std::env::var("REPOS").unwrap_or_default();
        load_from(&path(), &env_repos, chrono::Local::now())
    }

    pub fn save(&self) -> Result<()> {
        if self.read_only {
            anyhow::bail!(
                "Konfiguration wird nicht gespeichert, solange die defekte config.json nicht gesichert ist"
            );
        }
        save_to(&path(), self)
    }
}

fn save_to(path: &Path, cfg: &Config) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).context("Konfigurationsordner nicht anlegbar")?;
    }
    // Erst in eine Nachbardatei schreiben und dann umbenennen, damit ein Absturz
    // mitten im Schreiben keine halbe config.json hinterlässt.
    // Je Prozess eine eigene Nachbardatei, damit sich zwei Instanzen nicht in die Quere kommen.
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    std::fs::write(&tmp, serde_json::to_string_pretty(cfg)? + "\n")
        .context("Konfiguration nicht speicherbar")?;
    std::fs::rename(&tmp, path).context("Konfiguration nicht speicherbar")
}

fn first_start(env_repos: &str) -> Config {
    Config {
        repos: env_repos.split(',').filter_map(parse_repo).collect(),
        ..Default::default()
    }
}

fn load_from(path: &Path, env_repos: &str, now: chrono::DateTime<chrono::Local>) -> Loaded {
    let (problem, bytes) = match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Config>(&bytes) {
            Ok(config) => {
                return Loaded {
                    config,
                    warning: None,
                };
            }
            Err(e) => (format!("ungültiges JSON ({e})"), Some(bytes)),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let config = first_start(env_repos);
            let _ = save_to(path, &config);
            return Loaded {
                config,
                warning: None,
            };
        }
        Err(e) => (format!("nicht lesbar ({e})"), None),
    };

    match backup(path, now, bytes.as_deref()) {
        Ok(backup) => {
            let config = Config::default();
            let _ = save_to(path, &config);
            Loaded {
                config,
                warning: Some(format!(
                    "Die Konfiguration war {problem}. PR Radar startet mit Standardwerten; \
                     die alte Datei liegt unter {}.",
                    backup.display()
                )),
            }
        }
        Err(e) => Loaded {
            config: Config {
                read_only: true,
                ..Default::default()
            },
            warning: Some(format!(
                "Die Konfiguration unter {} war {problem} und ließ sich nicht sichern ({e}). \
                 PR Radar startet mit Standardwerten und speichert keine Änderungen, \
                 bis die Datei repariert oder entfernt ist.",
                path.display()
            )),
        },
    }
}

/// Legt eine Sicherung `config.json.defekt-<Zeitstempel>[-n]` an, ohne je eine vorhandene
/// Datei zu ersetzen: Ein harter Link bzw. `create_new` schlägt fehl, wenn der Name schon
/// vergeben ist (etwa durch eine zweite, gleichzeitig startende Instanz), dann geht es mit
/// dem nächsten Suffix weiter. Das Original bleibt liegen und wird erst danach überschrieben.
fn backup(
    path: &Path,
    now: chrono::DateTime<chrono::Local>,
    bytes: Option<&[u8]>,
) -> std::io::Result<PathBuf> {
    use std::io::{ErrorKind, Write as _};

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.json".into());
    let stamp = now.format("%Y%m%d-%H%M%S");
    for n in 1..=100 {
        let candidate = if n == 1 {
            path.with_file_name(format!("{name}.defekt-{stamp}"))
        } else {
            path.with_file_name(format!("{name}.defekt-{stamp}-{n}"))
        };
        match std::fs::hard_link(path, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            // Kein harter Link möglich (Dateisystem): Inhalt in eine neue Datei kopieren.
            Err(link_err) => {
                let Some(bytes) = bytes else {
                    return Err(link_err);
                };
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&candidate)
                {
                    Ok(mut file) => {
                        file.write_all(bytes)?;
                        file.sync_all()?;
                        return Ok(candidate);
                    }
                    Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e),
                }
            }
        }
    }
    Err(std::io::Error::new(
        ErrorKind::AlreadyExists,
        "kein freier Name für die Sicherung",
    ))
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
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pr-radar-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn now() -> chrono::DateTime<chrono::Local> {
        chrono::DateTime::parse_from_rfc3339("2026-11-06T18:00:00+01:00")
            .unwrap()
            .with_timezone(&chrono::Local)
    }

    #[test]
    fn erster_start_uebernimmt_repos_aus_env_und_speichert() {
        let dir = temp_dir("erster-start");
        let path = dir.join("config.json");
        let loaded = load_from(&path, "a/b, kaputt ,c/d", now());
        assert!(loaded.warning.is_none());
        assert_eq!(loaded.config.repos, ["a/b", "c/d"]);
        assert!(path.exists());
    }

    #[test]
    fn liest_gueltige_konfiguration() {
        let dir = temp_dir("gueltig");
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"repos":["x/y"]}"#).unwrap();
        let loaded = load_from(&path, "a/b", now());
        assert!(loaded.warning.is_none());
        assert_eq!(loaded.config.repos, ["x/y"]);
    }

    #[test]
    fn defekte_konfiguration_wird_gesichert_statt_ueberschrieben() {
        let dir = temp_dir("defekt");
        let path = dir.join("config.json");
        let broken = r#"{"repos":["x/y"],"#;
        std::fs::write(&path, broken).unwrap();

        let loaded = load_from(&path, "a/b", now());

        let warning = loaded.warning.expect("Hinweis erwartet");
        assert!(warning.contains("defekt"), "{warning}");
        assert!(
            loaded.config.repos.is_empty(),
            "keine Env-Repos nach Defekt"
        );
        assert!(!loaded.config.read_only);
        let stamp = now().format("%Y%m%d-%H%M%S").to_string();
        let backup = dir.join(format!("config.json.defekt-{stamp}"));
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), broken);

        // Ein zweiter Defekt in derselben Sekunde überschreibt die erste Sicherung nicht.
        std::fs::write(&path, "kaputt").unwrap();
        load_from(&path, "", now());
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), broken);
        assert_eq!(
            std::fs::read_to_string(dir.join(format!("config.json.defekt-{stamp}-2"))).unwrap(),
            "kaputt"
        );
    }

    #[test]
    fn schreibschutz_verhindert_speichern() {
        let cfg = Config {
            read_only: true,
            ..Default::default()
        };
        assert!(cfg.save().is_err());
    }

    #[test]
    fn speichern_ist_atomar_und_lesbar() {
        let dir = temp_dir("speichern");
        let path = dir.join("config.json");
        let cfg = Config {
            repos: vec!["a/b".into()],
            ..Default::default()
        };
        save_to(&path, &cfg).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        assert_eq!(load_from(&path, "", now()).config, cfg);
    }

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
