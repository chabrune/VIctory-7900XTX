//! Card sides page by page (`VICTORY_CARD_PAGES=1`, off by default).
//!
//! What the console has: one memory. The title shows a 64 KiB page of it at
//! several addresses and has buffers at each: a pool's 24 or 50 MB under one
//! buffer, and small buffers over parts of the same pages. A shader that
//! writes through one and one that reads through another meet in the page.
//!
//! What the layer had: a copy for every such buffer, each with its own bytes
//! in the card's memory (a buffer cut from a block of `crate::pool`, or a
//! committed one). They meet only by way of title memory: compared, fetched,
//! put into title memory, found written by the guards of the other view,
//! sent up again (`docs/findings.md`, "Copies of one memory": two blocks in
//! three that shaders write have more than one copy).
//!
//! What is here: a copy's card side as a reserved buffer
//! (`CreateReservedResource`) whose 64 KiB tiles are given from heaps of
//! ours (`UpdateTileMappings`, on the drawing queue, so in order with the
//! lists). Every page has a tile of its own from the start, and the copy is
//! what it was. A page can then be shown the tile of another copy's page
//! ([`Paged::borrow`]) and its own again ([`Paged::restore`]): the owner
//! (`super::owner`) does that for the copies of one page when it takes the
//! page for the card and when it brings it down. (As `crate::tiles` does
//! for the levels of textures; the pool of heaps is of the same make.)

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;

use super::Host;

/// The card's tile.
const TILE: u64 = 64 << 10;

/// Tiles in one heap of the pool (64 MiB).
const HEAP_TILES: u32 = 1024;

/// Since the start: such buffers made; their bytes; alive now; the bytes of
/// the pool's heaps; calls of `UpdateTileMappings`; their microseconds;
/// pages shown another copy's tile; pages given their own again.
static STATS: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];

/// Whether card sides are made this way: the switch, and a card that reads
/// zeros from and drops writes to a page without memory (tiled resources
/// tier 2: a list in the air may name a buffer made after it was sent,
/// through a slot written since, before the mappings behind it on the queue
/// are done).
pub(crate) fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        if !std::env::var_os("VICTORY_CARD_PAGES").is_some_and(|v| v == "1") {
            return false;
        }
        let Some(host) = super::host() else {
            return false;
        };
        let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS::default();
        // SAFETY: a plain query into a struct of its own size.
        let asked = unsafe { host.device.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS, (&raw mut options).cast(), size_of_val(&options) as u32) };
        let tier = options.TiledResourcesTier.0;
        let on = asked.is_ok() && tier >= 2;
        tracing::info!(tier, on, "card sides of title buffers' copies page by page (VICTORY_CARD_PAGES=1)");
        on
    })
}

/// One heap of the pool and its free runs of tiles (first tile, tiles), in
/// order, no two touching.
struct Heap {
    heap: ID3D12Heap,
    free: Vec<(u32, u32)>,
}

// SAFETY: a heap of the device, named only in queue and device calls, which
// any thread may make; its free runs are plain numbers behind `POOL`'s lock.
unsafe impl Send for Heap {}

/// The pool's heaps by place; a place is empty after its heap went back to
/// the card.
static POOL: Mutex<Vec<Option<Heap>>> = Mutex::new(Vec::new());

/// Up to `count` tiles of one heap, the lowest free ones first: the heap's
/// place in the pool and the runs (first tile, tiles). A new heap is made
/// when none has a free tile.
fn take(device: &ID3D12Device, count: u32) -> Option<(usize, Vec<(u32, u32)>)> {
    let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
    let found = pool.iter().position(|h| h.as_ref().is_some_and(|h| !h.free.is_empty()));
    let index = match found {
        Some(index) => index,
        None => {
            let desc = D3D12_HEAP_DESC {
                SizeInBytes: u64::from(HEAP_TILES) * TILE,
                Properties: D3D12_HEAP_PROPERTIES {
                    Type: D3D12_HEAP_TYPE_DEFAULT,
                    ..Default::default()
                },
                Alignment: 0,
                Flags: D3D12_HEAP_FLAG_ALLOW_ONLY_BUFFERS,
            };
            let mut made: Option<ID3D12Heap> = None;
            // SAFETY: plain creation; the out pointer is ours.
            if let Err(e) = unsafe { device.CreateHeap::<ID3D12Heap>(&raw const desc, &raw mut made) } {
                tracing::warn!(error = %e, "no heap for the pages of card sides");
                return None;
            }
            let fresh = Heap {
                heap: made?,
                free: vec![(0, HEAP_TILES)],
            };
            STATS[3].fetch_add(u64::from(HEAP_TILES) * TILE, Relaxed);
            match pool.iter().position(Option::is_none) {
                Some(index) => {
                    pool[index] = Some(fresh);
                    index
                }
                None => {
                    pool.push(Some(fresh));
                    pool.len() - 1
                }
            }
        }
    };
    let h = pool[index].as_mut()?;
    let mut runs = Vec::new();
    let mut left = count;
    while left > 0 {
        let Some(&(start, tiles)) = h.free.first() else {
            break;
        };
        let part = tiles.min(left);
        runs.push((start, part));
        if part == tiles {
            h.free.remove(0);
        } else {
            h.free[0] = (start + part, tiles - part);
        }
        left -= part;
    }
    Some((index, runs))
}

/// Gives the runs `runs` of the pool's heap at `index` back; runs that touch
/// become one. A heap with every tile free goes back to the card, unless it
/// is the pool's only one.
fn give_back(index: usize, runs: &[(u32, u32)]) {
    let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
    let Some(Some(h)) = pool.get_mut(index) else {
        return;
    };
    h.free.extend_from_slice(runs);
    h.free.sort_unstable();
    let mut joined: Vec<(u32, u32)> = Vec::with_capacity(h.free.len());
    for &(from, tiles) in &h.free {
        match joined.last_mut() {
            Some(last) if last.0 + last.1 == from => last.1 += tiles,
            _ => joined.push((from, tiles)),
        }
    }
    h.free = joined;
    let all_free = h.free.iter().map(|run| run.1).sum::<u32>() == HEAP_TILES;
    if all_free && pool.iter().flatten().count() > 1 {
        pool[index] = None;
        STATS[3].fetch_sub(u64::from(HEAP_TILES) * TILE, Relaxed);
    }
}

/// The heap at `index` of the pool.
fn heap_at(index: usize) -> Option<ID3D12Heap> {
    POOL.lock().unwrap_or_else(|e| e.into_inner()).get(index)?.as_ref().map(|h| h.heap.clone())
}

/// A card side made of pages: the reserved buffer and, per 64 KiB page of
/// it, the tile that is its own (the heap's place in the pool, the tile).
pub(crate) struct Paged {
    buffer: ID3D12Resource,
    own: Vec<(u32, u32)>,
    /// Which pages show another copy's tile now, a bit each.
    lent: Mutex<Vec<u64>>,
}

// SAFETY: a D3D12 resource (free-threaded) and plain numbers; the bits are
// behind their lock.
unsafe impl Send for Paged {}
// SAFETY: as above.
unsafe impl Sync for Paged {}

impl Paged {
    /// A reserved buffer of `size` bytes (a multiple of 64 KiB) shaders may
    /// write, in the common state, every page given a tile of its own; and
    /// what keeps the tiles. `None`: the switch is off, or the host or the
    /// card's memory refused.
    pub(crate) fn make(host: &Host, size: u64) -> Option<(ID3D12Resource, Paged)> {
        if !on() || size == 0 || !size.is_multiple_of(TILE) {
            return None;
        }
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: size,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
            ..Default::default()
        };
        let mut made: Option<ID3D12Resource> = None;
        // SAFETY: plain creation; the out pointer is ours.
        if let Err(e) = unsafe { host.device.CreateReservedResource(&desc, D3D12_RESOURCE_STATE_COMMON, None, &mut made) } {
            tracing::warn!(error = %e, size, "no reserved buffer for a card side page by page");
            return None;
        }
        let buffer = made?;
        let pages = (size / TILE) as u32;
        let mut own: Vec<(u32, u32)> = Vec::with_capacity(pages as usize);
        while (own.len() as u32) < pages {
            let first = own.len() as u32;
            let Some((index, runs)) = take(&host.device, pages - first) else {
                break;
            };
            if runs.is_empty() {
                break;
            }
            let tiles: u32 = runs.iter().map(|run| run.1).sum();
            if let Some(heap) = heap_at(index) {
                // (A buffer no list names yet: its tiles need no place in
                // the order of what the sending thread still holds, and
                // waiting for that thread here, under the lock of the
                // copies' table, stood the title still at the world's
                // start: the sending thread waited for the heaps' records,
                // whose threads waited for that table. One run in five,
                // 2026-11-27, with VICTORY_SUBMIT_AFTER=1.)
                map_runs(&host.raw_queue, &buffer, first, tiles, &heap, &runs);
            }
            for &(start, count) in &runs {
                own.extend((start..start + count).map(|tile| (index as u32, tile)));
            }
        }
        let paged = Paged {
            buffer: buffer.clone(),
            lent: Mutex::new(vec![0; (pages as usize).div_ceil(64)]),
            own,
        };
        if paged.own.len() as u32 != pages {
            // (Dropped: its tiles go back.)
            tracing::warn!(size, "no room in the card's memory for a card side page by page");
            return None;
        }
        STATS[0].fetch_add(1, Relaxed);
        STATS[1].fetch_add(size, Relaxed);
        STATS[2].fetch_add(1, Relaxed);
        Some((buffer, paged))
    }

    /// Page `page` shows the tile that is page `of_page`'s own in `of`:
    /// from the lists sent after this on, what either buffer reads or
    /// writes there is the other's.
    pub(crate) fn borrow(&self, host: &Host, page: u32, of: &Paged, of_page: u32) -> bool {
        let Some(&(index, tile)) = of.own.get(of_page as usize) else {
            return false;
        };
        if page as usize >= self.own.len() {
            return false;
        }
        let Some(heap) = heap_at(index as usize) else {
            return false;
        };
        map_runs(host.queue(), &self.buffer, page, 1, &heap, &[(tile, 1)]);
        let mut lent = self.lent.lock().unwrap_or_else(|e| e.into_inner());
        lent[page as usize / 64] |= 1 << (page % 64);
        STATS[6].fetch_add(1, Relaxed);
        true
    }

    /// Page `page` shows its own tile again, if it showed another's.
    pub(crate) fn restore(&self, host: &Host, page: u32) -> bool {
        {
            let mut lent = self.lent.lock().unwrap_or_else(|e| e.into_inner());
            let Some(word) = lent.get_mut(page as usize / 64) else {
                return false;
            };
            if *word >> (page % 64) & 1 == 0 {
                return false;
            }
            *word &= !(1 << (page % 64));
        }
        let (index, tile) = self.own[page as usize];
        let Some(heap) = heap_at(index as usize) else {
            return false;
        };
        map_runs(host.queue(), &self.buffer, page, 1, &heap, &[(tile, 1)]);
        STATS[7].fetch_add(1, Relaxed);
        true
    }
}

impl Drop for Paged {
    fn drop(&mut self) {
        // By heap, in runs. (Nothing on the card reads through the buffer's
        // mappings any more, as for any buffer that is dropped.)
        let mut by: std::collections::BTreeMap<u32, Vec<(u32, u32)>> = std::collections::BTreeMap::new();
        for &(index, tile) in &self.own {
            let runs = by.entry(index).or_default();
            match runs.last_mut() {
                Some(last) if last.0 + last.1 == tile => last.1 += 1,
                _ => runs.push((tile, 1)),
            }
        }
        for (index, runs) in by {
            give_back(index as usize, &runs);
        }
        STATS[2].fetch_sub(1, Relaxed);
    }
}

/// `tiles` tiles of `buffer` from tile `first` on are those of `runs`
/// (first tile, tiles) of `heap`, in order.
fn map_runs(queue: &ID3D12CommandQueue, buffer: &ID3D12Resource, first: u32, tiles: u32, heap: &ID3D12Heap, runs: &[(u32, u32)]) {
    let at = D3D12_TILED_RESOURCE_COORDINATE {
        X: first,
        Y: 0,
        Z: 0,
        Subresource: 0,
    };
    let size = D3D12_TILE_REGION_SIZE {
        NumTiles: tiles,
        ..Default::default()
    };
    let flags = vec![D3D12_TILE_RANGE_FLAG_NONE; runs.len()];
    let starts: Vec<u32> = runs.iter().map(|run| run.0).collect();
    let counts: Vec<u32> = runs.iter().map(|run| run.1).collect();
    let began = std::time::Instant::now();
    // SAFETY: a live reserved buffer and a live heap of ours; one region of
    // `tiles` tiles from tile `first`; `runs.len()` ranges whose counts add
    // up to `tiles`, in three arrays of that length that live through the
    // call.
    unsafe {
        queue.UpdateTileMappings(
            buffer,
            1,
            Some(&raw const at),
            Some(&raw const size),
            heap,
            runs.len() as u32,
            Some(flags.as_ptr()),
            Some(starts.as_ptr()),
            Some(counts.as_ptr()),
            D3D12_TILE_MAPPING_FLAG_NONE,
        );
    }
    STATS[4].fetch_add(1, Relaxed);
    STATS[5].fetch_add(began.elapsed().as_micros() as u64, Relaxed);
}

/// The line of it, while it is on.
pub(super) fn line() {
    if !on() {
        return;
    }
    let s: Vec<u64> = STATS.iter().map(|s| s.load(Relaxed)).collect();
    tracing::info!(
        "card sides page by page (VICTORY_CARD_PAGES=1): {} made since the start ({} MB), {} alive; the pool's heaps {} MB; {} calls of UpdateTileMappings, {} microseconds; pages shown another copy's tile {}, given their own again {}",
        s[0],
        s[1] >> 20,
        s[2],
        s[3] >> 20,
        s[4],
        s[5],
        s[6],
        s[7]
    );
}
