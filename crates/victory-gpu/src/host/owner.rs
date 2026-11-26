//! Memory with an owner (`VICTORY_CARD_OWNED=1`; off): a title buffer that
//! shaders write is the card's alone from its first such submission on. Its
//! pages in title memory are kept from the CPU; nothing of it is compared,
//! fetched, put into title memory or looked at by a refresh; and the first
//! thing on the CPU that touches it has it brought down whole, once, after
//! which the buffer is as every other (compared and fetched after each
//! submission that writes it).
//!
//! Why: on the console a byte is in one place. Here what shaders wrote to a
//! copy in the card's memory was found by a compare on the card after every
//! submission (8,300 pieces of 64 KiB a frame, 544 MB looked through),
//! fetched (69 MB a frame), merged into title memory, found there as written
//! by the next refresh and told apart from the title's own writes. Measured
//! with all of that off (`VICTORY_CARD_NO_BACK`, an RX 7700 XT, Series S
//! profile, 2026-11-26): the thread that prepares submissions 21.3 ms a frame
//! against 30.3, the card 27.3 ms against 29.0, 34 to 35 fps against 30 to 31;
//! and with it off for copies of a megabyte and more only, 35 fps and the
//! picture as it should be. The title's own calls are the same whatever
//! comes back: sixty megabytes a frame were brought down for nobody. Who
//! does read some of it cannot be known ahead, so it is found out when it
//! happens: the pages are not to be read or written (`PAGE_NOACCESS`), and
//! the fault is where the bytes are fetched.
//!
//! The states of a copy ([`Shadow::owner`]): [`FREE`]; [`CLAIMING`] while the
//! copy is brought up to date whole with its pages read-only (a write of the
//! title's waits for the end of that, then opens the copy); [`OWNED`];
//! [`FETCHING`] while one thread brings it down (the others that touch it
//! wait); [`OPENED`], for good.
//!
//! What is fetched is what the card holds when everything given to the
//! drawing queue so far has run: the fetch is a list of its own on that
//! queue. No submission is planned meanwhile if its lock can be had in
//! 20 ms (one planned while the copy was still the card's has no compare
//! for it).
//!
//! shortcut: the pages are opened before the bytes are in them (private
//! memory has one view): a thread that reads them in the milliseconds the
//! copying takes, without having faulted, reads what was there before. Only
//! memory that is private, committed whole and write-watched is taken, and
//! only copies of a megabyte and more (`VICTORY_CARD_OWNED_KB`).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};

use windows_sys::Win32::System::Memory as mem;

use super::*;
use crate::shadow::Shadow;

/// Not the card's alone (never asked, or turned down).
pub(crate) const FREE: u8 = 0;
/// The card's alone: title memory there is kept from the CPU.
pub(crate) const OWNED: u8 = 1;
/// Being brought down by the thread that touched it first.
pub(crate) const FETCHING: u8 = 2;
/// Touched once: as every other copy from then on.
pub(crate) const OPENED: u8 = 3;
/// Being brought up to date whole, its pages read-only.
pub(crate) const CLAIMING: u8 = 4;
/// Brought up to date: no read of ours begins any more, those that go on
/// are waited for, and then its pages are kept from the CPU.
pub(crate) const SEALING: u8 = 5;

/// Whether shader-written copies become the card's alone
/// (`VICTORY_CARD_OWNED=1`; off).
pub(crate) fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_CARD_OWNED").is_some_and(|v| v == "1"))
}

/// The least bytes of a copy that is taken (`VICTORY_CARD_OWNED_KB`, 1024).
fn least() -> u64 {
    static LEAST: OnceLock<u64> = OnceLock::new();
    *LEAST.get_or_init(|| std::env::var("VICTORY_CARD_OWNED_KB").ok().and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(1024) << 10)
}

/// The copies that are the card's alone, by their first page: where their
/// pages end, and the copy.
static TABLE: Mutex<BTreeMap<usize, (usize, std::sync::Weak<Shadow>)>> = Mutex::new(BTreeMap::new());
/// How many there are (the fault handler asks first).
static COUNT: AtomicU64 = AtomicU64::new(0);
/// Since the start: taken, turned down, opened by the title, opened by code
/// of ours (or let go); and the bytes owned now.
static STATS: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];

/// A block of view memory, as the guards have it.
const BLOCK: usize = 64 << 10;
/// Who opened the first copies: the copy's start and bytes, the instruction,
/// 0 a read / 1 a write / 2 no fault (the kernel, a release), whether in a
/// copy of ours, the thread, and the first addresses of our own code on the
/// faulting thread's stack (who called the copy).
static OPENERS: Mutex<Vec<(u64, u64, usize, u8, bool, u32, [usize; 5])>> = Mutex::new(Vec::new());

/// Diagnostics: blocks brought down for a read of ours, by the copy that was
/// being refreshed (start, length; 0: not known) and the copy whose block it
/// was (start): how many, the reader's kind, the owner's length.
static READERS: Mutex<BTreeMap<(u64, u64, u64), (u64, u8, u64)>> = Mutex::new(BTreeMap::new());

/// Who touched a copy: the instruction, read or write, whether in a copy of
/// ours, and where the thread's stack was.
type Who = (usize, u8, bool, usize);

/// The first five values on the stack from `stack` on that lie in our own
/// program: mostly the addresses its calls return to.
fn callers(stack: usize) -> [usize; 5] {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    let mut found = [0usize; 5];
    if stack == 0 {
        return found;
    }
    // SAFETY: the program's own module.
    let base = unsafe { GetModuleHandleW(std::ptr::null()) } as usize;
    // (The program is under 64 MiB.)
    let image = base..base + (64 << 20);
    let mut n = 0;
    for slot in 0..192 {
        // SAFETY: the faulting thread's own stack above where it stood,
        // read while that thread is in the handler.
        let value = unsafe { ((stack + slot * 8) as *const usize).read_volatile() };
        if image.contains(&value) {
            found[n] = value - base;
            n += 1;
            if n == found.len() {
                break;
            }
        }
    }
    found
}

fn table() -> std::sync::MutexGuard<'static, BTreeMap<usize, (usize, std::sync::Weak<Shadow>)>> {
    TABLE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The list, fence and buffer a copy is brought down with.
struct Pager {
    list: Prelude,
    fence: ID3D12Fence,
    event: HANDLE,
    sent: u64,
    /// The buffer, where it is mapped, its bytes.
    buffer: Option<(ID3D12Resource, usize, u64)>,
}

// SAFETY: device objects and a mapping, used by one thread at a time (under
// the lock of [`PAGER`]).
unsafe impl Send for Pager {}

static PAGER: Mutex<Option<Pager>> = Mutex::new(None);

impl Pager {
    fn make(host: &Host) -> Option<Pager> {
        let list = host.make_prelude()?;
        // SAFETY: plain objects of our own.
        unsafe {
            let fence: ID3D12Fence = host.device.CreateFence(0, D3D12_FENCE_FLAG_NONE).ok()?;
            let event = CreateEventW(None, false, false, None).ok()?;
            Some(Pager { list, fence, event, sent: 0, buffer: None })
        }
    }

    /// Room for a copy of `bytes` bytes.
    fn room(&mut self, host: &Host, bytes: u64) -> bool {
        if self.buffer.as_ref().is_none_or(|b| b.2 < bytes) {
            let size = bytes.next_multiple_of(16 << 20);
            self.buffer = host.shared_buffer(size).map(|(buffer, data)| (buffer, data as usize, size));
        }
        self.buffer.is_some()
    }

    /// The bytes of `shadow` as the card holds them once everything on the
    /// drawing queue has run: where they are, until the pager is used again.
    fn fetch(&mut self, host: &Host, shadow: &Shadow) -> Option<*const u8> {
        let (_, len) = shadow.span();
        self.fetch_part(host, shadow, 0, len)
    }

    /// [`Pager::fetch`] for `len` bytes of the copy from offset `at`.
    fn fetch_part(&mut self, host: &Host, shadow: &Shadow, at: u64, len: u64) -> Option<*const u8> {
        let (buffer, data, size) = self.buffer.as_ref()?;
        if *size < len {
            return None;
        }
        let (allocator, list) = &self.list;
        // SAFETY: the list's last run was waited for (below); live buffers
        // that rest in the common state between lists, as the landing's own
        // fetch has them; the fence and the event are the pager's.
        unsafe {
            allocator.Reset().ok()?;
            list.Reset(allocator, None).ok()?;
            list.CopyBufferRegion(buffer, 0, &shadow.buffer, at, len);
            list.Close().ok()?;
            let lists = [Some(list.cast::<ID3D12CommandList>().ok()?)];
            host.queue.ExecuteCommandLists(&lists);
            self.sent += 1;
            host.queue.Signal(&self.fence, self.sent).ok()?;
            self.fence.SetEventOnCompletion(self.sent, self.event).ok()?;
            WaitForSingleObject(self.event, INFINITE);
        }
        Some(*data as *const u8)
    }
}

/// Whether the pages `[first, first + bytes)` are private memory, committed,
/// to read and write, all of them.
fn private_pages(first: usize, bytes: usize) -> bool {
    let mut at = first;
    while at < first + bytes {
        // SAFETY: plain data filled in by the query.
        let mut info: mem::MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: a valid out pointer of the right size.
        let n = unsafe { mem::VirtualQuery(at as *const _, &mut info, size_of_val(&info)) };
        if n == 0 || info.State != mem::MEM_COMMIT || info.Type != mem::MEM_PRIVATE || info.Protect != mem::PAGE_READWRITE || info.RegionSize == 0 {
            return false;
        }
        at = info.BaseAddress as usize + info.RegionSize;
    }
    true
}

fn protect(first: usize, bytes: usize, how: u32) -> bool {
    let mut old = 0;
    // SAFETY: pages of title memory that hold a live copy's buffer.
    unsafe { mem::VirtualProtect(first as *const _, bytes, how, &mut old) != 0 }
}

/// Asked the first time a submission's shaders are declared to write
/// `shadow`, by the thread that prepares it, before what changed of the
/// copies in the card's memory is gathered: the copy becomes the card's
/// alone if it is of the kind. Whether it did.
pub(super) fn claim(host: &Host, shadow: &Arc<Shadow>) -> bool {
    if !shadow.ownable(least()) {
        return false;
    }
    let (first, bytes) = shadow.pages();
    let (start, len) = shadow.span();
    let down = |why: &str| {
        if STATS[1].fetch_add(1, Relaxed) < 16 {
            tracing::info!(start = format_args!("{start:#x}"), len, why, "a copy shaders write is not made the card's alone (VICTORY_CARD_OWNED)");
        }
        false
    };
    if !matches!(crate::inplace::cover(start as usize, len as usize), crate::inplace::Cover::None) {
        return down("its memory is views of a section");
    }
    if !private_pages(first, bytes) {
        return down("its memory is not private memory committed whole");
    }
    {
        let mut pager = PAGER.lock().unwrap_or_else(|e| e.into_inner());
        if pager.is_none() {
            *pager = Pager::make(host);
        }
        if !pager.as_mut().is_some_and(|pager| pager.room(host, len)) {
            return down("no list or buffer to bring it down with");
        }
    }
    // From here the title's writes wait for the end of this, and its reads
    // read what is there.
    shadow.owner().store(CLAIMING, Release);
    table().insert(first, (first + bytes, Arc::downgrade(shadow)));
    COUNT.fetch_add(1, Relaxed);
    let undo = |why: &str| {
        protect(first, bytes, mem::PAGE_READWRITE);
        table().remove(&first);
        COUNT.fetch_sub(1, Relaxed);
        shadow.owner().store(FREE, Release);
        down(why)
    };
    if !protect(first, bytes, mem::PAGE_READONLY) {
        return undo("its pages could not be protected");
    }
    // All of it as the title has it, and what the title wrote under ranges
    // this submission's shaders are declared to write.
    shadow.update(start as usize, len as usize);
    let mut runs = 4096;
    if shadow.merge_under_writes(start as usize, len as usize, &mut runs) {
        return undo("title bytes under shader writes could not be put in");
    }
    // From here no read of ours begins; those that go on are waited for
    // (a refresh of the copy on another thread).
    shadow.owner().store(SEALING, std::sync::atomic::Ordering::SeqCst);
    let waiting = std::time::Instant::now();
    while shadow.readers() != 0 {
        if waiting.elapsed().as_millis() >= 500 {
            return undo("a read of ours of its memory did not end");
        }
        std::thread::yield_now();
    }
    if !protect(first, bytes, mem::PAGE_NOACCESS) {
        return undo("its pages could not be kept from the CPU");
    }
    // (Only now: a fault before this waits, and finds the pages closed.)
    shadow.owner().store(OWNED, Release);
    STATS[0].fetch_add(1, Relaxed);
    STATS[4].fetch_add(len, Relaxed);
    true
}

/// Brings `shadow` down and opens its pages; the caller made its state
/// [`FETCHING`].
fn open(shadow: &Shadow, who: Option<Who>) {
    let host = super::host();
    // No submission planned meanwhile, if that can be had.
    let _planning = host.and_then(|host| {
        let until = std::time::Instant::now() + std::time::Duration::from_millis(20);
        loop {
            match host.submitted.try_lock() {
                Ok(held) => break Some(held),
                Err(std::sync::TryLockError::Poisoned(e)) => break Some(e.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) => {
                    if std::time::Instant::now() >= until {
                        break None;
                    }
                    std::thread::yield_now();
                }
            }
        }
    });
    let (first, bytes) = shadow.pages();
    let (start, len) = shadow.span();
    {
        let mut pager = PAGER.lock().unwrap_or_else(|e| e.into_inner());
        let fetched = host.zip(pager.as_mut()).and_then(|(host, pager)| pager.fetch(host, shadow));
        protect(first, bytes, mem::PAGE_READWRITE);
        match fetched {
            // SAFETY: the copy's bytes, in the pager's buffer until the
            // pager is let go; the pages are open.
            Some(from) => unsafe { shadow.opened_with(from) },
            None => {
                STATS[1].fetch_add(1, Relaxed);
            }
        }
    }
    shadow.owner().store(OPENED, Release);
    table().remove(&first);
    COUNT.fetch_sub(1, Relaxed);
    STATS[4].fetch_sub(len, Relaxed);
    let (site, kind, ours, stack) = who.unwrap_or((0, 2, true, 0));
    STATS[if ours { 3 } else { 2 }].fetch_add(1, Relaxed);
    let mut openers = OPENERS.lock().unwrap_or_else(|e| e.into_inner());
    if openers.len() < 24 {
        // SAFETY: no arguments.
        let thread = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
        openers.push((start, len, site, kind, ours, thread, if ours { callers(stack) } else { [0; 5] }));
    }
}

/// Waits for a copy that is being taken or brought down, or brings it down:
/// whether the access that led here can be tried again.
fn settle(shadow: &Shadow, who: Option<Who>) -> bool {
    let state = shadow.owner();
    let began = std::time::Instant::now();
    loop {
        match state.load(Acquire) {
            CLAIMING | SEALING | FETCHING => {
                if began.elapsed().as_secs() >= 10 {
                    return false;
                }
                std::thread::yield_now();
            }
            OWNED => {
                if state.compare_exchange(OWNED, FETCHING, AcqRel, Acquire).is_ok() {
                    open(shadow, who);
                    return true;
                }
            }
            _ => return true,
        }
    }
}

/// A read or write of `at` faulted: if it lies in a copy that is the card's
/// alone, the copy is brought down and opened (or waited for), and the
/// access can be tried again. Called from the fault handler.
pub(crate) fn fault(at: usize, kind: u8, site: usize, ours: bool, stack: usize) -> bool {
    if COUNT.load(Relaxed) == 0 {
        return false;
    }
    // Another view of the pages of a block that is the card's alone: the
    // block the copy has.
    let at = if crate::watch::card_block(at) { crate::watch::card_primary(at / BLOCK) * BLOCK + at % BLOCK } else { at };
    let began = std::time::Instant::now();
    loop {
        let found = {
            let table = table();
            match table.range(..=at).next_back() {
                Some((_, (end, shadow))) if at < *end => Some(shadow.upgrade()),
                _ => None,
            }
        };
        match found {
            // (A block of the card's whose copy is gone: given back as it is.)
            None => return crate::watch::card_block(at) && crate::watch::card_give(at / BLOCK),
            Some(Some(shadow)) => {
                if crate::watch::card_block(at) {
                    return open_block(&shadow, at, Some((site, kind, ours, stack)));
                }
                return settle(&shadow, Some((site, kind, ours, stack)));
            }
            Some(None) if crate::watch::card_block(at) => return crate::watch::card_give(at / BLOCK),
            // Being released: whoever does that opens it ([`dropped`]).
            Some(None) => {
                if began.elapsed().as_secs() >= 10 {
                    return false;
                }
                std::thread::yield_now();
            }
        }
    }
}

/// `[at, at + len)` of title memory is about to be written by the kernel for
/// the title, or to be let go: copies there that are the card's alone are
/// brought down first.
pub(crate) fn opening(at: usize, len: usize) {
    // Blocks of view memory that are the card's alone, each brought down.
    for block in crate::watch::card_blocks_in(at, at.saturating_add(len)) {
        let block = crate::watch::card_primary(block);
        match copy_at(block * BLOCK) {
            Some(shadow) => {
                open_block(&shadow, block * BLOCK, None);
            }
            None => {
                crate::watch::card_give(block);
            }
        }
    }
    if COUNT.load(Relaxed) == 0 {
        return;
    }
    let there: Vec<Arc<Shadow>> = {
        let table = table();
        table.iter().filter(|(first, (end, _))| **first < at.saturating_add(len) && at < *end).filter_map(|(_, (_, shadow))| shadow.upgrade()).collect()
    };
    for shadow in there {
        settle(&shadow, None);
    }
}

/// A copy is released while it is the card's alone: brought down first (the
/// title's memory may be read after its buffer is gone).
pub(crate) fn dropped(shadow: &Shadow) {
    if shadow.owner().compare_exchange(OWNED, FETCHING, AcqRel, Acquire).is_ok() {
        open(shadow, None);
    }
}

/// Blocks asked about for the card, and those of them brought up to date
/// with nothing left to settle; since the start.
static ASKED: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];

/// The copy in the table whose pages hold `at`, if it is alive.
fn copy_at(at: usize) -> Option<Arc<Shadow>> {
    let table = table();
    match table.range(..=at).next_back() {
        Some((_, (end, shadow))) if at < *end => shadow.upgrade(),
        _ => None,
    }
}

/// In view memory, block by block: asked for the ranges a submission's
/// shaders are declared to write of `shadow`, by the thread that prepares it.
/// The 64 KiB blocks that lie whole in such a range, were never asked about,
/// are shown at no other address and that nothing wrote since the copy took
/// them become the card's alone, 64 a submission at most. A buffer the title
/// writes in part keeps the blocks it writes: the first write of one opens
/// it, and it is not asked about again.
pub(super) fn claim_blocks(host: &Host, shadow: &Arc<Shadow>, ranges: &[(usize, usize)]) {
    if !shadow.ownable_by_block(least()) {
        return;
    }
    let blocks = shadow.unasked_blocks(ranges, 64);
    if blocks.is_empty() {
        return;
    }
    ASKED[0].fetch_add(blocks.len() as u64, Relaxed);
    {
        let mut pager = PAGER.lock().unwrap_or_else(|e| e.into_inner());
        if pager.is_none() {
            *pager = Pager::make(host);
        }
        if !pager.as_mut().is_some_and(|pager| pager.room(host, BLOCK as u64)) {
            return;
        }
    }
    // Each as the title has it: the refresh leaves it read-only and clean by
    // the guards' hand, and what the title wrote under shader writes is put in.
    let mut ready = Vec::with_capacity(blocks.len());
    for &block in &blocks {
        shadow.update(block * BLOCK, BLOCK);
        let mut runs = 4096;
        if !shadow.merge_under_writes(block * BLOCK, BLOCK, &mut runs) {
            ready.push(block);
        }
    }
    ASKED[1].fetch_add(ready.len() as u64, Relaxed);
    // No read of ours of the copy's memory goes on while they are closed.
    shadow.seal(true);
    let waiting = std::time::Instant::now();
    while shadow.readers() != 0 && waiting.elapsed().as_millis() < 200 {
        std::thread::yield_now();
    }
    #[cfg(feature = "diagnostics")]
    let refused_before: Vec<u64> = crate::watch::CARD_REFUSED.iter().map(|r| r.load(Relaxed)).collect();
    let mut taken = 0;
    if shadow.readers() == 0 {
        for &block in &ready {
            taken += u32::from(crate::watch::card_take(block));
        }
    }
    // Diagnostics: the first askings that took less than they asked for.
    #[cfg(feature = "diagnostics")]
    {
        static SAID: AtomicU64 = AtomicU64::new(0);
        if (taken as usize) < blocks.len() && SAID.fetch_add(1, Relaxed) < 80 {
            let refused: Vec<u64> = crate::watch::CARD_REFUSED.iter().zip(&refused_before).map(|(r, b)| r.load(Relaxed) - b).collect();
            let (start, len) = shadow.span();
            tracing::info!(
                "blocks asked for the card of {start:#x}+{len:#x}: {} asked, {} up to date, {taken} taken (readers {}); refused now: {} not read-only by the guards' hand, {} not clean, {} with a view that could not be closed, {} left alone, {} shown at more than eight addresses or none; first block {:#x}",
                blocks.len(),
                ready.len(),
                shadow.readers(),
                refused[0],
                refused[1],
                refused[2],
                refused[3],
                refused[4],
                blocks[0] * BLOCK
            );
        }
    }
    if taken > 0 {
        shadow.blocks_taken(taken);
        let (first, bytes) = shadow.pages();
        let mut table = table();
        if !table.contains_key(&first) {
            table.insert(first, (first + bytes, Arc::downgrade(shadow)));
            COUNT.fetch_add(1, Relaxed);
        }
        STATS[5].fetch_add(u64::from(taken), Relaxed);
    }
    shadow.seal(false);
}

/// The block's bytes put into its pages through a view of our own of them,
/// so that the title reads the block whole or not at all. Whether that was
/// done (else the caller copies them once the block is open).
fn fill_view(address: usize, from: *const u8) -> bool {
    let Some((section, offset)) = crate::inplace::section_of(address) else {
        return false;
    };
    // SAFETY: a section of the runtime's, alive while its views are told of;
    // a view of one block of it of our own, written whole and let go.
    unsafe {
        let view = mem::MapViewOfFile(section as _, mem::FILE_MAP_WRITE, (offset >> 32) as u32, offset as u32, BLOCK);
        if view.Value.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(from, view.Value.cast::<u8>(), BLOCK);
        mem::UnmapViewOfFile(view);
    }
    true
}

/// Brings down the block of `shadow` that holds `at` and opens it; nothing
/// to do if it is open by now. The access that led here can be tried again.
fn open_block(shadow: &Shadow, at: usize, who: Option<Who>) -> bool {
    let block = at / BLOCK;
    let host = super::host();
    // No submission planned meanwhile, if that can be had.
    let _planning = host.and_then(|host| {
        let until = std::time::Instant::now() + std::time::Duration::from_millis(20);
        loop {
            match host.submitted.try_lock() {
                Ok(held) => break Some(held),
                Err(std::sync::TryLockError::Poisoned(e)) => break Some(e.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) => {
                    if std::time::Instant::now() >= until {
                        break None;
                    }
                    std::thread::yield_now();
                }
            }
        }
    });
    let mut pager = PAGER.lock().unwrap_or_else(|e| e.into_inner());
    if !crate::watch::card_block(at) {
        return true;
    }
    let (start, len) = shadow.span();
    let (start, end) = (start as usize, (start + len) as usize);
    let address = block * BLOCK;
    if address < start || address + BLOCK > end {
        // Not a block of this copy's: given back as it is.
        return crate::watch::card_give(block);
    }
    let fetched = host.zip(pager.as_mut()).and_then(|(host, pager)| pager.fetch_part(host, shadow, (address - start) as u64, BLOCK as u64));
    let through = fetched.is_some_and(|from| fill_view(address, from));
    crate::watch::card_give(block);
    if let Some(from) = fetched {
        // SAFETY: a block's bytes in the pager's buffer, held until the
        // pager is let go; the block lies in the copy and is open.
        unsafe { shadow.block_opened(address - start, from, BLOCK, !through) };
    }
    shadow.block_given();
    STATS[6].fetch_add(1, Relaxed);
    let (site, kind, ours, stack) = who.unwrap_or((0, 2, true, 0));
    let mut openers = OPENERS.lock().unwrap_or_else(|e| e.into_inner());
    if openers.len() < 24 {
        // SAFETY: no arguments.
        let thread = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
        openers.push((address as u64, BLOCK as u64, site, kind, ours, thread, if ours { callers(stack) } else { [0; 5] }));
    }
    // Diagnostics: by the copy whose refresh read the block (a copy of ours).
    if ours {
        let reader = crate::shadow::reader_now().unwrap_or((0, 0, 0));
        let (cs, cl) = shadow.span();
        let mut by = READERS.lock().unwrap_or_else(|e| e.into_inner());
        let e = by.entry((reader.0, reader.1, cs)).or_insert((0, reader.2, cl));
        e.0 += 1;
    }
    true
}

/// A copy that had blocks asked about is released: those still the card's
/// are brought down first, and the table forgets the copy.
pub(crate) fn dropped_blocks(shadow: &Shadow) {
    let (start, len) = shadow.span();
    for block in crate::watch::card_blocks_in(start as usize, (start + len) as usize) {
        open_block(shadow, block * BLOCK, None);
    }
    let (first, _) = shadow.pages();
    if shadow.owner().load(Relaxed) == FREE && table().remove(&first).is_some() {
        COUNT.fetch_sub(1, Relaxed);
    }
}

/// The runtime mapped a view at `[at, at + len)`: a block that is the card's
/// alone at another view of the same pages is brought down, since they can
/// be read through this one from now on.
pub(crate) fn view_mapped(at: usize, len: usize) {
    if crate::watch::card_none() {
        return;
    }
    for k in 0..len.div_ceil(BLOCK) {
        let page = at + k * BLOCK;
        for other in crate::inplace::pages_of(page) {
            if other != page && crate::watch::card_block(other) {
                opening(other, BLOCK);
            }
        }
    }
}

/// `[at, at + len)` of title memory is gone: blocks there that were the
/// card's alone are nobody's, with nothing brought down.
pub(crate) fn forgetting(at: usize, len: usize) {
    for block in crate::watch::card_blocks_in(at, at.saturating_add(len)) {
        if crate::watch::card_give(block) {
            if let Some(shadow) = copy_at(block * BLOCK) {
                shadow.block_given();
            }
            STATS[6].fetch_add(1, Relaxed);
        }
    }
}

/// Diagnostics: the copies a compare was planned for since the last line,
/// by the copy: what it is to the owner, the pieces compared and in how
/// many ranges.
#[cfg(feature = "diagnostics")]
static COMPARED: Mutex<BTreeMap<usize, (String, u64, u64)>> = Mutex::new(BTreeMap::new());

/// Diagnostics: a compare of `pieces` pieces of `shadow` was planned.
#[cfg(feature = "diagnostics")]
pub(super) fn compared(shadow: &Arc<Shadow>, pieces: u32) {
    if !on() {
        return;
    }
    let mut all = COMPARED.lock().unwrap_or_else(|e| e.into_inner());
    let entry = all.entry(Arc::as_ptr(shadow) as usize).or_insert_with(|| (String::new(), 0, 0));
    if entry.2 == 0 {
        entry.0 = shadow.owner_says();
    }
    entry.1 += u64::from(pieces);
    entry.2 += 1;
}

/// A landing left `pieces` pieces of 64 KiB on the card: in blocks that are
/// the card's alone.
pub(super) fn left(pieces: u64) {
    STATS[7].fetch_add(pieces, Relaxed);
}

/// The line of it, while it is on.
pub(super) fn line() {
    if !on() {
        return;
    }
    {
        let why: Vec<u64> = crate::watch::CARD_WHY.iter().map(|w| w.load(Relaxed)).collect();
        tracing::info!(
            "blocks not taken for the card, since the start, finer: not read-only: {} resting after a write, {} left alone, {} not a view's, {} absent, {} others; views of the pages: {} none known, {} more than eight, {} its own not among them",
            why[0], why[1], why[2], why[3], why[4], why[5], why[6], why[7]
        );
        let views: Vec<u64> = crate::watch::CARD_VIEWS_OF.iter().map(|w| w.load(Relaxed)).collect();
        tracing::info!(
            "blocks not taken for the card by how many addresses show their pages (0, 1, 2-8, 9-16, 17-64, 65-256, 257-1024, more): not read-only {:?}; many views {:?}",
            &views[..8],
            &views[8..]
        );
    }
    #[cfg(feature = "diagnostics")]
    {
        let mut all: Vec<(String, u64, u64)> = std::mem::take(&mut *COMPARED.lock().unwrap_or_else(|e| e.into_inner())).into_values().collect();
        all.sort_by_key(|c| std::cmp::Reverse(c.1));
        let pieces: u64 = all.iter().map(|c| c.1).sum();
        let lines: Vec<String> = all.iter().take(14).map(|(what, pieces, ranges)| format!("{pieces} pieces in {ranges} ranges of {what}")).collect();
        tracing::info!(
            "copies still compared on the card since the last line (VICTORY_CARD_OWNED=1): {pieces} pieces of {} copies; the most: {}",
            all.len(),
            lines.join("; ")
        );
    }
    tracing::info!(
        "blocks of 64 KiB of view memory that are the card's alone (VICTORY_CARD_OWNED=1): since the start {} asked about, {} of them up to date, {} taken, {} brought down again; {} pieces left on the card and not fetched; not taken: {} not read-only by the guards' hand, {} not clean, {} with a view that could not be closed, {} left alone, {} shown at more than eight addresses or none",
        ASKED[0].load(Relaxed),
        ASKED[1].load(Relaxed),
        STATS[5].load(Relaxed),
        STATS[6].load(Relaxed),
        STATS[7].load(Relaxed),
        crate::watch::CARD_REFUSED[0].load(Relaxed),
        crate::watch::CARD_REFUSED[1].load(Relaxed),
        crate::watch::CARD_REFUSED[2].load(Relaxed),
        crate::watch::CARD_REFUSED[3].load(Relaxed),
        crate::watch::CARD_REFUSED[4].load(Relaxed)
    );
    let s: Vec<u64> = STATS.iter().map(|s| s.load(Relaxed)).collect();
    let named = |site: usize| -> String {
        use windows_sys::Win32::System::LibraryLoader::{GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW, GetModuleHandleExW};
        if site == 0 {
            return "no fault".to_owned();
        }
        let mut module = std::ptr::null_mut();
        // SAFETY: an address to ask about and a place for the answer.
        let found = unsafe {
            GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, site as *const u16, &mut module)
        } != 0;
        if !found {
            return format!("{site:#x}");
        }
        let mut name = [0u16; 260];
        // SAFETY: a module of the process and room for its name.
        let n = unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) } as usize;
        let path = String::from_utf16_lossy(&name[..n.min(name.len())]);
        format!("{}+{:#x}", path.rsplit(['\\', '/']).next().unwrap_or(""), site - module as usize)
    };
    let openers: Vec<String> = OPENERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|&(start, len, site, kind, ours, thread, callers)| {
            let from: Vec<String> = callers.iter().filter(|&&c| c != 0).map(|c| format!("+{c:#x}")).collect();
            format!(
                "{start:#x}+{len:#x} {} by thread {thread} at {}{}",
                ["read", "written", "opened"][usize::from(kind.min(2))],
                named(site),
                if ours && kind < 2 { format!(" (a copy of ours, called from {})", from.join(" ")) } else { String::new() }
            )
        })
        .collect();
    {
        let by = READERS.lock().unwrap_or_else(|e| e.into_inner());
        let mut all: Vec<_> = by.iter().collect();
        all.sort_by_key(|(_, v)| std::cmp::Reverse(v.0));
        let lines: Vec<String> = all
            .iter()
            .take(16)
            .map(|((rs, rl, os), (n, kind, ol))| {
                format!(
                    "{n} blocks of {os:#x}+{ol:#x} for the refresh of {rs:#x}+{rl:#x}{}{}",
                    if kind & 1 != 0 { " (a window)" } else { "" },
                    if kind & 2 != 0 { " (on the card)" } else { "" }
                )
            })
            .collect();
        tracing::info!(
            "blocks that were the card's alone brought down for reads of ours, since the start, by the copy refreshed: {} pairs; {}",
            all.len(),
            lines.join("; ")
        );
    }
    tracing::info!(
        "copies that are the card's alone (VICTORY_CARD_OWNED=1): {} now, {} MB; since the start {} taken, {} turned down, {} opened by the title, {} by code of ours or let go; the first opened: {}",
        COUNT.load(Relaxed),
        s[4] >> 20,
        s[0],
        s[1],
        s[2],
        s[3],
        if openers.is_empty() { "none".to_owned() } else { openers.join("; ") }
    );
}
