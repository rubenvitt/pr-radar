//! Log-Datei, Panic-Hook und Meldung schwerer Startfehler.
//!
//! Das Protokoll liegt unter macOS in `~/Library/Logs/PR Radar/pr-radar.log`
//! (sonst im lokalen Datenordner) und wird beim Start ab 2 MB nach
//! `pr-radar.old.log` verschoben. Lässt sich die Datei dort nicht öffnen,
//! schreibt die App nach `<temp>/pr-radar/pr-radar.log`; `log_path()` nennt
//! dann diesen Pfad. Es enthält keine Tokens oder Inhalte von PRs,
//! nur Fehler und Warnungen der App.

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};

/// Ab dieser Größe wird das Protokoll beim Start rotiert.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

struct FileLogger {
    file: Mutex<Option<File>>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // Eigene Meldungen ab Info, Abhängigkeiten (GPUI, reqwest …) nur Warnungen.
        if metadata.target().starts_with(env!("CARGO_CRATE_NAME")) {
            metadata.level() <= Level::Info
        } else {
            metadata.level() <= Level::Warn
        }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format_line(
            chrono::Local::now(),
            record.level(),
            record.target(),
            &record.args().to_string(),
        );
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        if let Ok(mut guard) = self.file.lock()
            && let Some(file) = guard.as_mut()
        {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock()
            && let Some(file) = guard.as_mut()
        {
            let _ = file.flush();
        }
    }
}

fn format_line(
    now: chrono::DateTime<chrono::Local>,
    level: Level,
    target: &str,
    message: &str,
) -> String {
    format!(
        "{} {level:<5} [{target}] {message}\n",
        now.format("%Y-%m-%d %H:%M:%S%.3f")
    )
}

fn default_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(home) = dirs::home_dir() {
        return home
            .join("Library")
            .join("Logs")
            .join("PR Radar")
            .join("pr-radar.log");
    }
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("pr-radar")
        .join("pr-radar.log")
}

/// Ausweichpfad, falls sich der Hauptpfad nicht öffnen lässt.
fn fallback_path() -> PathBuf {
    std::env::temp_dir().join("pr-radar").join("pr-radar.log")
}

/// Pfad der Log-Datei, in die tatsächlich geschrieben wird. Ließ sich keine
/// Datei öffnen, ist es der Hauptpfad.
pub fn log_path() -> &'static Path {
    LOG_PATH.get_or_init(default_path)
}

/// Ergebnis von [`choose_log_file`].
struct LogFile {
    /// Benutzter Pfad; ließ sich keine Datei öffnen, der Hauptpfad.
    path: PathBuf,
    file: Option<File>,
    /// Hinweis aufs Ausweichen, gehört nach dem Einrichten des Loggers ins Protokoll.
    warning: Option<String>,
}

/// Öffnet das Protokoll am Hauptpfad, sonst am Ausweichpfad.
fn choose_log_file(primary: PathBuf, fallback: PathBuf) -> LogFile {
    let (path, file, warning) = match open_log(&primary) {
        Ok(file) => (primary, Some(file), None),
        Err(primary_err) if fallback != primary => match open_log(&fallback) {
            Ok(file) => {
                let warning = format!(
                    "Protokoll {} nicht beschreibbar ({primary_err}), schreibe nach {}",
                    primary.display(),
                    fallback.display()
                );
                (fallback, Some(file), Some(warning))
            }
            Err(fallback_err) => {
                eprintln!(
                    "Protokoll weder unter {} ({primary_err}) noch unter {} ({fallback_err}) beschreibbar",
                    primary.display(),
                    fallback.display()
                );
                (primary, None, None)
            }
        },
        Err(e) => {
            eprintln!("Protokoll {} nicht beschreibbar: {e}", primary.display());
            (primary, None, None)
        }
    };
    if let Some(warning) = &warning {
        eprintln!("{warning}");
    }
    LogFile {
        path,
        file,
        warning,
    }
}

/// Wie [`choose_log_file`], merkt sich den benutzten Pfad aber für `log_path()`.
fn open_with_fallback(primary: PathBuf, fallback: PathBuf) -> (Option<File>, Option<String>) {
    let LogFile {
        path,
        file,
        warning,
    } = choose_log_file(primary, fallback);
    let _ = LOG_PATH.set(path);
    (file, warning)
}

/// Verschiebt ein zu großes Protokoll nach `<name>.old.log` und öffnet es zum Anhängen.
fn open_log(path: &Path) -> std::io::Result<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(path, path.with_extension("old.log"));
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// Richtet Logger und Panic-Hook ein. Lässt sich weder der Haupt- noch der
/// Ausweichpfad öffnen, läuft die App ohne Datei weiter (Meldungen dann nur
/// auf stderr im Debug-Build).
pub fn init() {
    let (file, warning) = open_with_fallback(default_path(), fallback_path());
    let logger = Box::leak(Box::new(FileLogger {
        file: Mutex::new(file),
    }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
    install_panic_hook();
    log::info!(
        "PR Radar {} gestartet ({} {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    if let Some(warning) = warning {
        log::warn!("{warning}");
    }
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = panic_message(info.payload());
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unbekannt".into());
        let thread = std::thread::current();
        let backtrace = std::backtrace::Backtrace::force_capture();
        log::error!(
            "Absturz im Thread {:?} bei {location}: {message}\n{backtrace}",
            thread.name().unwrap_or("unbenannt")
        );
        log::logger().flush();
        previous(info);
        show_alert(
            "PR Radar ist abgestürzt",
            &format!(
                "{message}\n\nDetails stehen im Protokoll:\n{}",
                log_path().display()
            ),
        );
    }));
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unbekannter Fehler".into())
}

/// Meldet einen Fehler, bei dem die App nicht weiterlaufen kann, und beendet sie.
pub fn fatal(title: &str, error: impl std::fmt::Display) -> ! {
    log::error!("{title}: {error:#}");
    log::logger().flush();
    eprintln!("{title}: {error:#}");
    show_alert(
        title,
        &format!(
            "{error:#}\n\nDetails stehen im Protokoll:\n{}",
            log_path().display()
        ),
    );
    std::process::exit(1);
}

/// Zeigt einen Systemdialog, der auch ohne Fenster und nach dem Ende der App sichtbar bleibt.
fn show_alert(title: &str, message: &str) {
    #[cfg(target_os = "macos")]
    {
        // Text als Argumente übergeben, damit Anführungszeichen nichts kaputt machen.
        let _ = std::process::Command::new("/usr/bin/osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "display alert (item 1 of argv) message (item 2 of argv) as critical",
                "-e",
                "end run",
                title,
                message,
            ])
            .spawn();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (title, message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeile_hat_zeit_stufe_und_ziel() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-06T18:00:00.5+00:00")
            .unwrap()
            .with_timezone(&chrono::Local);
        let line = format_line(now, Level::Warn, "pr_radar::radar", "Hallo");
        assert!(line.ends_with(" WARN  [pr_radar::radar] Hallo\n"), "{line}");
        assert!(line.contains(".500 "), "{line}");
    }

    #[test]
    fn grosses_protokoll_wird_rotiert() {
        let dir = std::env::temp_dir().join(format!("pr-radar-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("pr-radar.log");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, vec![b'x'; (MAX_BYTES + 1) as usize]).unwrap();

        let mut file = open_log(&path).unwrap();
        file.write_all(b"neu\n").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "neu\n");
        assert_eq!(
            std::fs::metadata(dir.join("pr-radar.old.log"))
                .unwrap()
                .len(),
            MAX_BYTES + 1
        );
    }

    #[test]
    fn hauptpfad_bleibt_wenn_beschreibbar() {
        let dir = std::env::temp_dir().join(format!("pr-radar-primary-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let primary = dir.join("haupt").join("pr-radar.log");
        let fallback = dir.join("ausweich").join("pr-radar.log");

        let chosen = choose_log_file(primary.clone(), fallback.clone());

        assert_eq!(chosen.path, primary);
        assert!(chosen.file.is_some());
        assert!(chosen.warning.is_none());
        assert!(!fallback.exists());
    }

    #[test]
    fn ausweichpfad_wenn_hauptpfad_nicht_beschreibbar() {
        let dir = std::env::temp_dir().join(format!("pr-radar-fallback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Eine Datei an Stelle des Ordners blockiert den Hauptpfad, auch für root.
        let blocker = dir.join("blockiert");
        std::fs::write(&blocker, b"").unwrap();
        let primary = blocker.join("pr-radar.log");
        let fallback = dir.join("ausweich").join("pr-radar.log");

        let (file, warning) = open_with_fallback(primary.clone(), fallback.clone());
        file.unwrap().write_all(b"neu\n").unwrap();

        // Einziger Test, der `LOG_PATH` setzt; andere Tests rufen `log_path()` nicht auf.
        assert_eq!(log_path(), fallback.as_path());
        assert_eq!(std::fs::read_to_string(&fallback).unwrap(), "neu\n");
        let warning = warning.unwrap();
        assert!(
            warning.contains(&primary.display().to_string()),
            "{warning}"
        );
        assert!(
            warning.contains(&fallback.display().to_string()),
            "{warning}"
        );
    }

    #[test]
    fn panic_nachricht_aus_str_und_string() {
        assert_eq!(panic_message(&"kaputt"), "kaputt");
        assert_eq!(panic_message(&String::from("auch kaputt")), "auch kaputt");
        assert_eq!(panic_message(&42), "unbekannter Fehler");
    }
}
