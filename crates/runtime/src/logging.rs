use std::fs::OpenOptions;
use std::sync::Mutex;

/// Sends the log to the platform's log file; without one, nothing is logged.
pub fn init_logging() {
    let Some(path) = kanaemi_config::log_file() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    // Debug logs record every key typed, so only a development build writes
    // them; a release build compiles them out (release_max_level_info).
    let level = if cfg!(debug_assertions) {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    tracing_subscriber::fmt()
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .with_max_level(level)
        .try_init()
        .ok();
}
