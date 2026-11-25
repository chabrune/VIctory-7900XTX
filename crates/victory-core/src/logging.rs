//! One tracing setup for every binary.
//!
//! Filter with `VICTORY_LOG` (falls back to `RUST_LOG`), e.g.
//! `VICTORY_LOG=victory_loader=trace,info`. Debug and trace output exist
//! only in builds with the `diagnostics` feature; a plain release build
//! logs info and above whatever the filter asks for.
//!
//! Every event also goes to a file — by default
//! `%LOCALAPPDATA%\victory\cache\logs\victory.log` (the cache's `logs`
//! folder), so a crash or a bad run can be read afterwards. `[log] file`
//! in `victory.toml` or `VICTORY_LOG_FILE` moves it; `off` turns it off.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// The most verbose level this build can log.
const CAP: LevelFilter = if cfg!(feature = "diagnostics") {
    LevelFilter::TRACE
} else {
    LevelFilter::INFO
};

/// Past this size the file starts again, so it never grows without bound.
const BIG: u64 = 8 << 20;

pub fn init(default_level: &str) {
    let filter = std::env::var("VICTORY_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .ok()
        .and_then(|v| EnvFilter::try_new(v).ok())
        .unwrap_or_else(|| EnvFilter::new(default_level));
    let console = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_writer(std::io::stderr);
    let layers = tracing_subscriber::registry()
        .with(CAP)
        .with(filter)
        .with(console);
    match log_file() {
        Some((path, file)) => {
            let _ = layers
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_target(true)
                        .with_ansi(false)
                        .with_writer(FileWriter(file)),
                )
                .try_init();
            eprintln!("log file       {}", path.display());
        }
        None => {
            let _ = layers.try_init();
        }
    }
}

/// Where the file goes and the open handle, if there is one.
fn log_file() -> Option<(PathBuf, Arc<Mutex<std::fs::File>>)> {
    let path = crate::Config::load()
        .ok()
        .and_then(|cfg| cfg.log_file)?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // A fresh file past the limit; otherwise append, so a relaunch keeps
    // the previous run at the top.
    let big = std::fs::metadata(&path).is_ok_and(|m| m.len() >= BIG);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(!big)
        .truncate(big)
        .open(&path)
        .ok()?;
    Some((path, Arc::new(Mutex::new(file))))
}

/// Writes each event to the shared file, from whichever thread logs.
struct FileWriter(Arc<Mutex<std::fs::File>>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for FileWriter {
    type Writer = FileGuard<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        FileGuard(self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

struct FileGuard<'a>(std::sync::MutexGuard<'a, std::fs::File>);

impl Write for FileGuard<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// Kept for callers that only have a path (`victory run` after it has
/// loaded the configuration): the file location the next [`init`] uses.
pub fn file_path(cfg: &crate::Config) -> Option<&Path> {
    cfg.log_file.as_deref()
}
