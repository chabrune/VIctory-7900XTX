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

//!
//! Events are written by a thread of their own (`victory-log`), not by the
//! thread that logs them: a line to a console window takes about a
//! millisecond, and the thread that shows frames logs fifteen of them once a
//! second (a timeline of four frames in the world, 2026-11-26: 16.8 ms of one
//! frame's 69.8 in "the frame meter", which is those lines). Warnings and
//! errors are still written by their own thread, after what was queued
//! before them, so that the last lines before a crash are there.
//! `VICTORY_LOG_SYNC=1`: every event written where it is logged, as before.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};

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
    let file = log_file();
    start_writer(file.as_ref().map(|(_, f)| f.clone()));
    let console = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_writer(Behind { file: false });
    let layers = tracing_subscriber::registry()
        .with(CAP)
        .with(filter)
        .with(console);
    match file {
        Some((path, _)) => {
            let _ = layers
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_target(true)
                        .with_ansi(false)
                        .with_writer(Behind { file: true }),
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

/// What the writing thread is given: an event's text for the console or for
/// the file, or a wish to be told when everything before it is written.
enum Job {
    Console(Vec<u8>),
    File(Vec<u8>),
    Told(SyncSender<()>),
}

/// The file events also go to, if there is one.
static FILE: OnceLock<Arc<Mutex<std::fs::File>>> = OnceLock::new();
/// The line to the writing thread; none with `VICTORY_LOG_SYNC=1`.
static QUEUE: OnceLock<SyncSender<Job>> = OnceLock::new();

/// Events that may wait for the writing thread before a logging thread
/// waits itself (a thread that logs faster than the console takes).
const WAITING: usize = 16384;

fn write_console(bytes: &[u8]) {
    let _ = std::io::stderr().lock().write_all(bytes);
}

fn write_file(bytes: &[u8]) {
    if let Some(file) = FILE.get() {
        let _ = file.lock().unwrap_or_else(|e| e.into_inner()).write_all(bytes);
    }
}

/// Starts the thread that writes events, unless they are to be written where
/// they are logged.
fn start_writer(file: Option<Arc<Mutex<std::fs::File>>>) {
    if let Some(file) = file {
        let _ = FILE.set(file);
    }
    if std::env::var_os("VICTORY_LOG_SYNC").is_some_and(|v| v == "1") || QUEUE.get().is_some() {
        return;
    }
    let (send, receive) = sync_channel::<Job>(WAITING);
    let writer = move || {
        for job in receive {
            match job {
                Job::Console(bytes) => write_console(&bytes),
                Job::File(bytes) => write_file(&bytes),
                Job::Told(done) => {
                    let _ = done.send(());
                }
            }
        }
    };
    if std::thread::Builder::new().name("victory-log".into()).spawn(writer).is_ok() {
        let _ = QUEUE.set(send);
    }
}

/// Waits until every event logged so far is written (before the process
/// ends, and before a warning or an error is written by its own thread).
pub fn flush() {
    let Some(queue) = QUEUE.get() else {
        return;
    };
    let (done, told) = sync_channel(1);
    if queue.send(Job::Told(done)).is_ok() {
        // (Not for ever: a writing thread that is gone tells nobody.)
        let _ = told.recv_timeout(std::time::Duration::from_secs(2));
    }
}

/// Where a layer's events go: the console or the file, through the writing
/// thread.
struct Behind {
    file: bool,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Behind {
    type Writer = Line;

    fn make_writer(&'a self) -> Self::Writer {
        Line { bytes: Vec::new(), file: self.file, now: false }
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        Line { bytes: Vec::new(), file: self.file, now: *meta.level() <= tracing::Level::WARN }
    }
}

/// One event's text, sent on its way when the layer lets go of it.
struct Line {
    bytes: Vec<u8>,
    file: bool,
    /// Written by this thread, after what is queued (warnings and errors).
    now: bool,
}

impl Write for Line {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for Line {
    fn drop(&mut self) {
        if self.bytes.is_empty() {
            return;
        }
        let bytes = std::mem::take(&mut self.bytes);
        let bytes = match (QUEUE.get(), self.now) {
            (Some(queue), false) => {
                let job = if self.file { Job::File(bytes) } else { Job::Console(bytes) };
                match queue.send(job) {
                    Ok(()) => return,
                    // The writing thread is gone: written here.
                    Err(std::sync::mpsc::SendError(Job::File(b) | Job::Console(b))) => b,
                    Err(_) => return,
                }
            }
            (Some(_), true) => {
                flush();
                bytes
            }
            (None, _) => bytes,
        };
        if self.file { write_file(&bytes) } else { write_console(&bytes) }
    }
}

/// Kept for callers that only have a path (`victory run` after it has
/// loaded the configuration): the file location the next [`init`] uses.
pub fn file_path(cfg: &crate::Config) -> Option<&Path> {
    cfg.log_file.as_deref()
}
