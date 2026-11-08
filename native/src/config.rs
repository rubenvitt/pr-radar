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
            anyhow::bail!(tr!(
                "Settings are not saved while the broken config.json has not been backed up",
                "Konfiguration wird nicht gespeichert, solange die defekte config.json nicht gesichert ist"
            ));
        }
        save_to(&path(), self)
    }

    /// Nimmt ein Repo (owner/name oder GitHub-URL) auf und speichert. Liefert den Namen.
    /// Scheitert das Speichern, bleibt die Konfiguration unverändert.
    pub fn add_repo(&mut self, input: &str) -> Result<String> {
        self.add_repo_with(input, Config::save)
    }

    /// Entfernt ein Repo samt Filtereintrag und speichert.
    /// Scheitert das Speichern, bleibt die Konfiguration unverändert.
    pub fn remove_repo(&mut self, full: &str) -> Result<()> {
        self.remove_repo_with(full, Config::save)
    }

    fn add_repo_with(
        &mut self,
        input: &str,
        save: impl FnOnce(&Config) -> Result<()>,
    ) -> Result<String> {
        let full = parse_repo(input).ok_or_else(|| {
            anyhow::anyhow!(tr!(
                "Invalid repo – expected owner/name or a GitHub URL.",
                "Ungültiges Repo – erwartet owner/name oder eine GitHub-URL."
            ))
        })?;
        if self.repos.iter().any(|r| r.eq_ignore_ascii_case(&full)) {
            anyhow::bail!(trf!(
                "{full} is already in the list.",
                "{full} ist schon in der Liste."
            ));
        }
        self.commit(|next| next.repos.push(full.clone()), save)?;
        Ok(full)
    }

    fn remove_repo_with(
        &mut self,
        full: &str,
        save: impl FnOnce(&Config) -> Result<()>,
    ) -> Result<()> {
        self.commit(
            |next| {
                next.repos.retain(|r| r != full);
                next.prefs.repo_filter.retain(|r| r != full);
            },
            save,
        )
    }

    /// Ändert eine Kopie, speichert sie und übernimmt sie erst danach. So landet eine
    /// Änderung, deren Speichern gescheitert ist, weder im nächsten Poll noch im
    /// nächsten erfolgreichen Save.
    fn commit(
        &mut self,
        change: impl FnOnce(&mut Config),
        save: impl FnOnce(&Config) -> Result<()>,
    ) -> Result<()> {
        let mut next = self.clone();
        change(&mut next);
        save(&next)?;
        *self = next;
        Ok(())
    }
}

fn save_to(path: &Path, cfg: &Config) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).context(tr!(
            "Could not create the settings folder",
            "Konfigurationsordner nicht anlegbar"
        ))?;
    }
    // Erst in eine Nachbardatei schreiben und dann umbenennen, damit ein Absturz
    // mitten im Schreiben keine halbe config.json hinterlässt.
    // Je Prozess eine eigene Nachbardatei, damit sich zwei Instanzen nicht in die Quere kommen.
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    std::fs::write(&tmp, serde_json::to_string_pretty(cfg)? + "\n").context(tr!(
        "Could not save the settings",
        "Konfiguration nicht speicherbar"
    ))?;
    std::fs::rename(&tmp, path).context(tr!(
        "Could not save the settings",
        "Konfiguration nicht speicherbar"
    ))
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
            Err(e) => (
                trf!("invalid JSON ({e})", "ungültiges JSON ({e})"),
                Some(bytes),
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let config = first_start(env_repos);
            let _ = save_to(path, &config);
            return Loaded {
                config,
                warning: None,
            };
        }
        Err(e) => (trf!("unreadable ({e})", "nicht lesbar ({e})"), None),
    };

    match backup(path, now, bytes.as_deref()) {
        Ok(backup) => {
            let config = Config::default();
            // Scheitert das Speichern, gelten die Standardwerte nur im Speicher;
            // spätere Saves bleiben möglich und dürfen es erneut versuchen.
            let unsaved = match save_to(path, &config) {
                Ok(()) => String::new(),
                Err(e) => trf!(
                    " The defaults could not be saved ({e:#}) and only apply until you quit.",
                    " Die Standardwerte ließen sich nicht speichern ({e:#}), \
                     sie gelten nur bis zum Beenden."
                ),
            };
            Loaded {
                config,
                warning: Some(trf!(
                    "The settings file was {problem}. PR Radar starts with defaults; \
                     the old file is at {}.{unsaved}",
                    "Die Konfiguration war {problem}. PR Radar startet mit Standardwerten; \
                     die alte Datei liegt unter {}.{unsaved}",
                    backup.display()
                )),
            }
        }
        Err(e) => Loaded {
            config: Config {
                read_only: true,
                ..Default::default()
            },
            warning: Some(trf!(
                "The settings file at {} was {problem} and could not be backed up ({e}). \
                 PR Radar starts with defaults and saves no changes \
                 until the file is repaired or removed.",
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
        tr!(
            "no free name for the backup",
            "kein freier Name für die Sicherung"
        ),
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
    fn fehlgeschlagenes_speichern_nach_sicherung_wird_gemeldet() {
        let dir = temp_dir("speichern-scheitert");
        let path = dir.join("config.json");
        std::fs::write(&path, "kaputt").unwrap();
        // Ein Ordner an der Stelle der Zwischendatei lässt das Speichern scheitern.
        std::fs::create_dir(path.with_extension(format!("json.tmp-{}", std::process::id())))
            .unwrap();

        let loaded = load_from(&path, "", now());

        let warning = loaded.warning.expect("Hinweis erwartet");
        assert!(warning.contains("could not be saved"), "{warning}");
        let warning = crate::l10n::with_lang(crate::l10n::Lang::De, || {
            load_from(&path, "", now())
                .warning
                .expect("Hinweis erwartet")
        });
        assert!(warning.contains("nicht speichern"), "{warning}");
        assert!(!loaded.config.read_only, "spätere Saves bleiben möglich");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "kaputt");
    }

    #[test]
    fn schreibschutz_verhindert_speichern() {
        let cfg = Config {
            read_only: true,
            ..Default::default()
        };
        assert!(cfg.save().is_err());
    }

    fn mit_repo(repo: &str) -> Config {
        let mut cfg = Config {
            repos: vec![repo.into()],
            ..Default::default()
        };
        cfg.prefs.repo_filter = vec![repo.into()];
        cfg
    }

    #[test]
    fn repo_aendern_bei_schreibschutz_laesst_konfiguration_unveraendert() {
        let mut cfg = Config {
            read_only: true,
            ..mit_repo("a/b")
        };
        let before = cfg.clone();

        assert!(cfg.add_repo("c/d").is_err());
        assert_eq!(cfg, before, "add_repo");
        // Ein erneuter Versuch meldet nicht „schon in der Liste“, sondern wieder den Schreibschutz.
        let err = cfg.add_repo("c/d").unwrap_err().to_string();
        assert!(!err.contains("already in the list"), "{err}");

        assert!(cfg.remove_repo("a/b").is_err());
        assert_eq!(cfg, before, "remove_repo");
    }

    #[test]
    fn repo_aendern_bei_schreibfehler_laesst_konfiguration_unveraendert() {
        let dir = temp_dir("repo-schreibfehler");
        let path = dir.join("config.json");
        // Ein Ordner an der Stelle der Zwischendatei lässt das Speichern scheitern.
        std::fs::create_dir(path.with_extension(format!("json.tmp-{}", std::process::id())))
            .unwrap();
        let mut cfg = mit_repo("a/b");
        let before = cfg.clone();

        assert!(cfg.add_repo_with("c/d", |c| save_to(&path, c)).is_err());
        assert_eq!(cfg, before);
        assert!(cfg.remove_repo_with("a/b", |c| save_to(&path, c)).is_err());
        assert_eq!(cfg, before);
        assert!(!path.exists());
    }

    #[test]
    fn repo_aendern_uebernimmt_nach_erfolgreichem_speichern() {
        let dir = temp_dir("repo-aendern");
        let path = dir.join("config.json");
        let mut cfg = mit_repo("a/b");

        let added = cfg
            .add_repo_with("https://github.com/c/d", |c| save_to(&path, c))
            .unwrap();
        assert_eq!(added, "c/d");
        assert_eq!(cfg.repos, ["a/b", "c/d"]);
        assert_eq!(load_from(&path, "", now()).config, cfg);
        let err = cfg.add_repo_with("C/D", |_| unreachable!()).unwrap_err();
        assert!(err.to_string().contains("already in the list"), "{err}");

        cfg.remove_repo_with("a/b", |c| save_to(&path, c)).unwrap();
        assert_eq!(cfg.repos, ["c/d"]);
        assert!(cfg.prefs.repo_filter.is_empty());
        assert_eq!(load_from(&path, "", now()).config, cfg);
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
