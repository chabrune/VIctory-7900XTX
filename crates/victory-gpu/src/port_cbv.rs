//! Renderer port #5: the title's constant buffers read by the card where the
//! title has them (`VICTORY_PORT_CBV=1`, off by default; read in [`on`]).
//!
//! What the console allows: a constant buffer anywhere on 16 bytes. What
//! D3D12 asks: a constant buffer view on 256. So the layer made every
//! constant buffer a draw's tables name afresh at each submission: a copy of
//! its bytes (out of title memory, on the CPU) into upload memory of its own
//! on 256, and a view of that ([`crate::draw`]'s tables, `defer_constants`):
//! ~23,000 a frame of the city, ~9 MB, and as many steps of the submitting
//! thread. With renderer port 1 (`VICTORY_PORT_CONSTANTS=1`) the memory they
//! are copied out of is the card's to read in place already.
//!
//! What is done here instead: shaders are translated to read constant buffers
//! as raw bytes (`victory_shader::emit::constants_raw`): a table's through
//! the raw twin of its slot, a root one as a root SRV. A raw view starts on 16
//! bytes, a root SRV on 4. So a constant buffer of a table in title memory in
//! place is a raw view of the title's bytes, made in the twin of its slot,
//! and nothing is copied or done at submission; a root one is bound at its
//! address as root SRVs are. Constant buffers anywhere else are copied as
//! before (the twin of their slot is a raw view of the copy, as it was).
//! The twin of a title slot that holds a constant buffer view is a raw view
//! of its bytes too (it was a constant buffer view: a raw read through one
//! on AMD has no end).
//!
//! What it assumes: what the card reads of a constant buffer at a draw is
//! what the title's CPU wrote before it submitted, as for every other title
//! buffer in place (renderer port 1): before, the copy at submission was
//! taken then whatever the title wrote after. The title writes its constants
//! into a ring it waits on its fences for.

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// Whether renderer port #5 is on: the shaders' side
/// (`victory_shader::emit::constants_raw`) and the host's views follow the
/// same switch.
pub(crate) fn on() -> bool {
    victory_shader::emit::constants_raw()
}

/// Whether a table's constant buffer in title memory in place is given to the
/// card where it is (on with the port; `VICTORY_PORT_CBV_PLACE=0`: copied as
/// before, and read through the raw twin all the same). With
/// `VICTORY_AB=<s>,VICTORY_PORT_CBV_PLACE` (diagnostics builds) on and off
/// while the title runs: both sides measured in one run.
pub(crate) fn in_place_on() -> bool {
    on() && crate::toggled!("VICTORY_PORT_CBV_PLACE")
}

/// Whether a table's constant buffer in memory that a host copy shaders
/// write holds is read in that copy, as its other views are (on with the
/// port; `VICTORY_PORT_CBV_COPIES=0`: copied out of title memory at
/// submission, as before; with `VICTORY_AB` by turns).
///
/// Why: the copy at submission reads title memory on the CPU, and what the
/// card writes there is not in title memory while the submissions that
/// write it are in the air. With the title's ray tracing on, one such
/// constant buffer a frame (176 to 240 bytes: in a buffer shaders are
/// declared to write whole, or the destination of a buffer copy the list
/// does at its place) stood the submitting thread still until the card had
/// run everything sent before, 4 to 37 ms a frame (runs rt-trace3,
/// rt-placed, 2026-11-26); the steps on the card refuse it where that copy
/// stands in the same submission. Read in the host copy, the bytes are
/// those the card has at the draw, in the list's order, and the title's own
/// writes go up with the copy's refresh like those of any buffer view.
pub(crate) fn copies_on() -> bool {
    on() && crate::toggled!("VICTORY_PORT_CBV_COPIES")
}

/// Since the last line: tables' constant buffers given in place, their bytes;
/// copied as before because not in place, because not on 16 bytes; title
/// slots' constant buffers whose twin is a raw view; tables' constant buffers
/// read in a host copy shaders write.
static STATS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

/// A table's constant buffer is read in the host copy shaders write.
pub(crate) fn in_copy() {
    STATS[5].fetch_add(1, Relaxed);
}

/// A table's constant buffer of `size` bytes was given in place.
pub(crate) fn in_place(size: u32) {
    STATS[0].fetch_add(1, Relaxed);
    STATS[1].fetch_add(u64::from(size), Relaxed);
}

/// A table's constant buffer was copied as before: `aligned` false when it
/// is in place but not on 16 bytes.
pub(crate) fn copied(aligned: bool) {
    STATS[if aligned { 2 } else { 3 }].fetch_add(1, Relaxed);
}

/// A title slot's constant buffer view got a raw twin.
pub(crate) fn twinned() {
    STATS[4].fetch_add(1, Relaxed);
}

/// Whether a constant buffer at host address `gpu` can be a raw view of
/// those bytes: on 16 bytes.
pub(crate) fn raw_ok(gpu: u64) -> bool {
    gpu.is_multiple_of(16)
}

/// For the frame-rate line.
pub(crate) fn log(frames: f64) {
    if !on() {
        return;
    }
    let s: Vec<f64> = STATS.iter().map(|s| s.swap(0, Relaxed) as f64 / frames.max(1e-9)).collect();
    tracing::info!(
        "renderer port 5 (VICTORY_PORT_CBV=1) per frame: {:.0} tables' constant buffers read by the card in place ({:.1} KB not copied, \
         no step at submission); copied as before: {:.0} not in place, {:.0} not on 16 bytes; {:.0} title slots' constant buffers with a raw twin;          {:.1} read in a host copy shaders write (VICTORY_PORT_CBV_COPIES)",
        s[0],
        s[1] / 1024.0,
        s[2],
        s[3],
        s[4],
        s[5],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_views_need_sixteen_bytes() {
        assert!(raw_ok(0x3500_0010));
        assert!(raw_ok(0x3500_0100));
        assert!(!raw_ok(0x3500_0008));
        assert!(!raw_ok(0x3500_0004));
    }
}
