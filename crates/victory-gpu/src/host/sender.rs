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
//! split, not its gain yet. The next one moves that wait to where the next
//! preparation needs it, and lets the thread that prepares start on what does
//! not depend on the send.

use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, SendError, Sender, channel};

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

struct Job {
    batches: Batches,
    done: Sender<Result<(), String>>,
}

/// The thread, once started ([`started`]).
static JOBS: OnceLock<Option<Sender<Job>>> = OnceLock::new();

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
                    "submissions' lists are given to the queue by a thread of their own (VICTORY_SUBMIT_TWO=1)"
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
    match jobs.send(Job { batches, done }) {
        Ok(()) => Ok(Some(waited.recv().unwrap_or_else(|_| {
            Err("the thread that gives submissions to the queue went".to_owned())
        }))),
        Err(SendError(job)) => Err(job.batches),
    }
}

/// What the thread does, for as long as there is something to send.
fn queue(jobs: &Receiver<Job>) {
    for job in jobs {
        let result = send(&job.batches);
        if job.done.send(result).is_err() {
            break;
        }
    }
}

fn send(batches: &Batches) -> Result<(), String> {
    let host = super::host().ok_or_else(|| "no host device to send a submission to".to_owned())?;
    // SAFETY: closed lists of the submission's; the fence and the value are
    // ours, taken under the submission lock, which the caller holds until
    // this returns.
    unsafe {
        let mut from = 0;
        for to in batches.cuts.iter().copied().chain([batches.lists.len()]) {
            // (No result: the queue says nothing of what it took, as D3D12
            // has it; a mistake shows as the fence never being reached.)
            host.queue.ExecuteCommandLists(&batches.lists[from..to]);
            from = to;
        }
        host.queue.Signal(&host.fence, batches.value).map_err(|e| e.to_string())?;
    }
    Ok(())
}
