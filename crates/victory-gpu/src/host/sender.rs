//! The lists of a submission given to the queue by a thread of its own
//! (`VICTORY_SUBMIT_TWO=1`; off): the first step of the split of what the
//! submitting thread does over two threads in a row (`docs/findings.md`,
//! "The submitting thread, in two stages").
//!
//! What moves: the calls that give the list to the queue and signal the fence
//! behind it. What does not: the preparation (the refreshes, the copies into
//! the card's memory, the recording), the flight in the line, and the waits —
//! those read title memory and the line, and they are the submission's order.
//!
//! In this step the thread that prepares a submission waits for this one to
//! have given the lists to the queue before it goes on, because what is in the
//! air is what the next preparation asks about (`Host::submit`'s
//! `line().written`) and the queues' order is the title's. So this step is
//! expected to cost a hand-over and nothing else: it is the shape of the
//! split, not its gain yet.
//!
//! The next step (`VICTORY_SUBMIT_AFTER=1`; off): the end of a submission
//! sent ahead is this thread's whole ([`After`]): the wait for the heaps'
//! records, the slots that waited for a texture's host copy, the lists given
//! to the queue and the fence signalled behind them. The thread that prepares
//! does not wait for it: it notes the flight in the line and goes on to the
//! next submission's asking and refreshes, which read title memory and the
//! line, not the queue. What has to be in the queue's order waits for this
//! thread first ([`join`]): every call of ours on the drawing queue goes
//! through `Host::queue`, which joins, so the next submission's sends of title
//! bytes, a fetch of what shaders wrote, a tile mapping and the present all
//! come behind the lists handed over before them, as they did.
//!
//! A flight noted in the line before its lists are in the queue is a flight
//! in the air a little longer: its fence value is taken under the submission
//! lock as before, a wait for that value ends when this thread has signalled
//! it and the card has run what is ahead, and nothing is let go under it
//! (objects go by the submissions started and finished, not by the queue).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SendError, Sender, channel};
use std::sync::{Condvar, Mutex, OnceLock};

use windows::Win32::Graphics::Direct3D12::*;

/// The lists of one submission to give to the queue, where its parts begin in
/// them, and the value the fence is signalled with behind them.
pub(super) struct Batches {
    pub(super) lists: Vec<Option<ID3D12CommandList>>,
    pub(super) cuts: Vec<usize>,
    pub(super) value: u64,
}

// SAFETY: D3D12 command lists are free-threaded; those given are closed, and
// nobody records into them or resets the allocator behind them before the
// flight they are part of has landed (which is after this thread's signal).
unsafe impl Send for Batches {}

/// The end of a submission sent ahead, done here while the thread that
/// prepared it goes on ([`after`]).
pub(super) struct After {
    pub(super) batches: Batches,
    /// The round of the heaps' records the submission waits for: dropped
    /// here, which is the wait.
    pub(super) heaps: crate::descriptor::SyncedHeaps,
    /// For the count of that wait (`crate::port_journal::heaps_waited`).
    pub(super) heaps_on: bool,
    /// The preludes' lists that came from the pool, given back once sent.
    pub(super) preludes: Vec<ID3D12GraphicsCommandList>,
    /// For the timeline: when the sending began, and the submission's stamp.
    #[cfg(feature = "diagnostics")]
    pub(super) sending: u64,
    #[cfg(feature = "diagnostics")]
    pub(super) stamp: Option<usize>,
}

// SAFETY: as `Batches`; the preludes' lists are closed and only given back to
// their pool here, and the round is a number.
unsafe impl Send for After {}

enum Job {
    /// Given to the queue while the caller waits for the answer.
    Now(Batches, Sender<Result<(), String>>),
    /// The end of a submission, nobody waiting ([`join`] does, later).
    After(Box<After>),
}

/// The thread, once started ([`started`]).
static JOBS: OnceLock<Option<Sender<Job>>> = OnceLock::new();

/// Ends of submissions handed over and not done yet, and what [`join`] waits
/// on (the count goes down under the lock).
static PENDING: AtomicUsize = AtomicUsize::new(0);
static DONE: (Mutex<()>, Condvar) = (Mutex::new(()), Condvar::new());

/// Since the last frame line: ends handed over, joins that had to wait, and
/// the microseconds they waited.
pub(super) static STATS: [std::sync::atomic::AtomicU64; 3] = [const { std::sync::atomic::AtomicU64::new(0) }; 3];

/// Whether the lists of a submission go to the queue on a thread of their own
/// (`VICTORY_SUBMIT_TWO=1`; off).
pub(super) fn on() -> bool {
    #[cfg(feature = "diagnostics")]
    if let Some(on) = crate::ab::pick("VICTORY_SUBMIT_TWO") {
        return on && started();
    }
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_SUBMIT_TWO").is_some_and(|v| v != "0") && started())
}

/// Whether the end of a submission sent ahead is done by the sending thread
/// while the preparing one goes on (`VICTORY_SUBMIT_AFTER=1`; off).
///
/// What the preparing thread then does not stand for, a frame, with the
/// title's ray tracing at its low setting on an RX 7700 XT: 1.8 to 3.0 ms of
/// waiting for the heaps' records and 0.6 to 1.0 ms of giving lists to the
/// queue (runs g1, s50b).
pub(super) fn after() -> bool {
    #[cfg(feature = "diagnostics")]
    if let Some(on) = crate::ab::pick("VICTORY_SUBMIT_AFTER") {
        return on && started();
    }
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_SUBMIT_AFTER").is_some_and(|v| v != "0") && started())
}

fn started() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let (jobs, waiting) = channel::<Job>();
        let worker = move || {
            crate::cores::keep_on_fast_cores("victory-send");
            queue(&waiting);
        };
        match std::thread::Builder::new().name("victory-send".into()).spawn(worker) {
            Ok(_) => {
                if JOBS.set(Some(jobs)).is_err() {
                    return false;
                }
                tracing::info!(
                    "submissions' lists are given to the queue by a thread of their own (VICTORY_SUBMIT_TWO=1, VICTORY_SUBMIT_AFTER=1)"
                );
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "no thread to send submissions: the thread that prepares them does");
                false
            }
        }
    })
}

/// Hands `batches` to the thread, and comes back with what the queue said of
/// them; `Err` with them back if there is no thread to take them (or it went),
/// so that the caller sends them itself.
#[allow(clippy::type_complexity)]
pub(super) fn hand(batches: Batches) -> Result<Option<Result<(), String>>, Batches> {
    let Some(jobs) = JOBS.get().and_then(Option::as_ref) else {
        return Err(batches);
    };
    let (done, waited) = channel();
    match jobs.send(Job::Now(batches, done)) {
        Ok(()) => Ok(Some(waited.recv().unwrap_or_else(|_| {
            Err("the thread that gives submissions to the queue went".to_owned())
        }))),
        Err(SendError(Job::Now(batches, _))) => Err(batches),
        Err(SendError(Job::After(_))) => unreachable!("a job of another kind came back"),
    }
}

/// Hands the end of a submission to the thread and comes back at once; `Err`
/// with it back if there is no thread to take it, so that the caller does it
/// itself.
pub(super) fn hand_after(end: After) -> Result<(), Box<After>> {
    let end = Box::new(end);
    let Some(jobs) = JOBS.get().and_then(Option::as_ref) else {
        return Err(end);
    };
    PENDING.fetch_add(1, Ordering::AcqRel);
    match jobs.send(Job::After(end)) {
        Ok(()) => {
            STATS[0].fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        Err(SendError(job)) => {
            finished();
            match job {
                Job::After(end) => Err(end),
                Job::Now(..) => unreachable!("a job of another kind came back"),
            }
        }
    }
}

/// One end handed over is done (or was never taken).
fn finished() {
    let held = DONE.0.lock().unwrap_or_else(|e| e.into_inner());
    PENDING.fetch_sub(1, Ordering::AcqRel);
    drop(held);
    DONE.1.notify_all();
}

/// Waits until every end of a submission handed over ([`hand_after`]) is
/// done: its lists are in the queue and its fence is signalled behind them.
/// Nothing to wait for unless `VICTORY_SUBMIT_AFTER` is on. Never called by
/// the sending thread itself.
pub(crate) fn join() {
    if PENDING.load(Ordering::Acquire) == 0 {
        return;
    }
    let began = std::time::Instant::now();
    let mut held = DONE.0.lock().unwrap_or_else(|e| e.into_inner());
    while PENDING.load(Ordering::Acquire) != 0 {
        held = DONE.1.wait(held).unwrap_or_else(|e| e.into_inner());
    }
    drop(held);
    STATS[1].fetch_add(1, Ordering::Relaxed);
    STATS[2].fetch_add(began.elapsed().as_micros() as u64, Ordering::Relaxed);
}

/// What the thread does, for as long as there is something to send.
fn queue(jobs: &Receiver<Job>) {
    for job in jobs {
        match job {
            Job::Now(batches, done) => {
                let result = send(&batches);
                if done.send(result).is_err() {
                    break;
                }
            }
            Job::After(end) => {
                after_here(*end);
                finished();
            }
        }
    }
}

/// The end of a submission sent ahead, in the order `Host::submit` has it:
/// the heaps' records, the slots that waited, the lists and the signal, and
/// what follows a send.
fn after_here(end: After) {
    let _ending = crate::timeline::span("the end of a submission, on the sending thread");
    let began = std::time::Instant::now();
    drop(end.heaps);
    crate::port_journal::heaps_waited(began.elapsed().as_nanos() as u64, end.heaps_on);
    // The slots that waited for a texture's host copy, before the lists
    // that may read them run.
    crate::descriptor::make_waiting();
    let sent = send(&end.batches);
    super::give_preludes(end.preludes);
    if let Some(host) = super::host() {
        super::awake::sent(&host.device);
        #[cfg(feature = "diagnostics")]
        crate::timeline::sent(end.sending, end.batches.value, end.stamp);
        host.card_sent();
    }
    if let Err(e) = sent {
        super::fail(format_args!("a submission sent ahead could not be followed by a signal ({e})"));
    }
}

fn send(batches: &Batches) -> Result<(), String> {
    let host = super::host().ok_or_else(|| "no host device to send a submission to".to_owned())?;
    // SAFETY: closed lists of the submission's; the fence and the value are
    // ours, taken under the submission lock. The caller holds that lock until
    // this returns, or has handed the end of the submission over, in which
    // case every other call on the queue waits for this thread ([`join`]).
    unsafe {
        let mut from = 0;
        for to in batches.cuts.iter().copied().chain([batches.lists.len()]) {
            // (No result: the queue says nothing of what it took, as D3D12
            // has it; a mistake shows as the fence never being reached.)
            host.raw_queue.ExecuteCommandLists(&batches.lists[from..to]);
            from = to;
        }
        host.raw_queue.Signal(&host.fence, batches.value).map_err(|e| e.to_string())?;
    }
    Ok(())
}
