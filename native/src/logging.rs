//! Log-Datei, Panic-Hook und Meldung schwerer Startfehler.
//!
//! Das Protokoll liegt unter macOS in `~/Library/Logs/PR Radar/pr-radar.log`
//! (sonst im lokalen Datenordner) und wird beim Start ab 2 MB nach
//! `pr-radar.old.log` verschoben. Es enthält keine Tokens oder Inhalte von PRs,
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

/// Pfad der Log-Datei (auch wenn sie sich nicht öffnen ließ).
pub fn log_path() -> &'static Path {
    LOG_PATH.get_or_init(default_path)
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

/// Richtet Logger und Panic-Hook ein. Schlägt das Öffnen der Datei fehl,
/// läuft die App ohne Datei weiter (Meldungen dann nur auf stderr im Debug-Build).
pub fn init() {
    let path = log_path();
    let file = match open_log(path) {
        Ok(file) => Some(file),
        Err(e) => {
            eprintln!("Protokoll {} nicht beschreibbar: {e}", path.display());
            None
        }
    };
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
    fn panic_nachricht_aus_str_und_string() {
        assert_eq!(panic_message(&"kaputt"), "kaputt");
        assert_eq!(panic_message(&String::from("auch kaputt")), "auch kaputt");
        assert_eq!(panic_message(&42), "unbekannter Fehler");
    }
}
