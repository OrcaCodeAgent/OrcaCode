use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::safety::redact::redact_secrets;

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init() -> PathBuf {
    let directory = log_directory();
    let _ = create_dir_all(&directory);
    let path = directory.join("orca.log");
    if let Ok(mut slot) = LOG_PATH.lock() {
        *slot = Some(path.clone());
    }
    log_line("info", "Orca logger started");
    path
}

pub fn log_directory() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            return home.join("Library/Logs/Orca Code");
        }
    }
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Orca Code")
        .join("logs")
}

pub fn log_line(level: &str, message: &str) {
    let clean = redact_secrets(message);
    let stamp = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown-time".into());
    let line = format!("{stamp} {level} {clean}\n");
    eprintln!("{line}");
    let path = LOG_PATH.lock().ok().and_then(|slot| slot.clone());
    if let Some(path) = path {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(line.as_bytes());
        }
    }
}
