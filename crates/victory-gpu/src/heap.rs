//! The allocator of every crate in the process: the system's heap with
//! blocks kept by the thread that let them go (`VICTORY_HEAP=system`: the
//! system's heap alone, as before).
//!
//! Our own code asks the heap for some 30,000 blocks a frame and makes 7,000
//! of them over at another size (the log's "blocks asked of the heap"; an RX
//! 7700 XT, Series S profile, 2026-11-26): two million calls a second from
//! sixty threads on the one heap of the process. On the thread that prepares
//! submissions 11.7 % of the time it runs was the system's heap (profile p2:
//! 6.6 % making blocks over, 3.7 % making them), 8 % of the thread's that
//! lands flights and of the one that brings up the heaps' records.
//!
//! What is done: a block of up to a megabyte has one of 64 sizes (every 16
//! bytes up to 256, then four to a power of two), and one let go is kept, a
//! few of each size, by one of 256 holders, a thread always using the same.
//! The next block of that size asked for by a thread of that holder is the
//! kept one: no call of the system's, no lock but the holder's own flag,
//! which is never waited for (a holder that is busy is passed by, to the
//! system). A block made over at a size of the same kind stays where it is.
//!
//! Every block of a kept size is asked of the system at that size's bytes
//! and given back at them, whoever holds it meanwhile; a block of another
//! kind (larger, or aligned over 16 bytes) is the system's whole, as before.
//! Which of the two ways it is is asked once, at the first block, and never
//! changes: a block goes back the way it came.
//!
//! What it costs: a block is up to a quarter larger than asked (none up to
//! 256 bytes), and the holders keep at most about 2.5 MB each, what their
//! threads let go.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{
    AtomicBool, AtomicU8, AtomicU64,
    Ordering::{Acquire, Relaxed, Release},
};

/// See the module.
pub(crate) struct Heap;

/// Blocks up to this many bytes are kept; larger ones are the system's.
const MOST: usize = 1 << 20;
/// Sizes up to this come every 16 bytes; above, four to a power of two.
const FINE: usize = 256;
/// The sizes: 16 up to [`FINE`], then four for each of twelve powers of two.
const CLASSES: usize = 16 + 4 * 12;
/// The first size counted against a holder's bytes ([`BIG_MOST`]): over 4 KiB.
const BIG_FROM: usize = 16 + 4 * 4;
/// The most bytes of such sizes one holder keeps.
const BIG_MOST: usize = 2 << 20;
/// The holders.
const HOLDERS: usize = 256;

/// Which of the sizes a block of `size` bytes (1 to [`MOST`]) has.
#[inline]
fn class(size: usize) -> usize {
    if size <= FINE {
        (size.max(1) + 15) / 16 - 1
    } else {
        // 2^k < size <= 2^(k + 1), and k is 8 at least.
        let k = (usize::BITS - 1 - (size - 1).leading_zeros()) as usize;
        let base = 1usize << k;
        16 + 4 * (k - 8) + (size - base - 1) / (base / 4)
    }
}

/// The bytes of a block of size `class`.
#[inline]
fn bytes(class: usize) -> usize {
    if class < 16 {
        (class + 1) * 16
    } else {
        let base = 1usize << (8 + (class - 16) / 4);
        base + base / 4 * ((class - 16) % 4 + 1)
    }
}

/// How many blocks of size `class` one holder keeps.
#[inline]
fn most(class: usize) -> u16 {
    match bytes(class) {
        0..=256 => 32,
        257..=4096 => 16,
        4097..=65536 => 4,
        _ => 2,
    }
}

/// Blocks let go and kept, by size: each holds the next one's address in
/// its first word.
#[repr(align(128))]
struct Holder {
    /// Taken while one thread is at the lists; never waited for.
    busy: AtomicBool,
    first: [Cell<*mut u8>; CLASSES],
    held: [Cell<u16>; CLASSES],
    /// Bytes held of the sizes from [`BIG_FROM`] on.
    big: Cell<usize>,
    /// Blocks asked for that a kept one answered, and that the system did.
    found: AtomicU64,
    asked: AtomicU64,
}

// SAFETY: the cells are read and written only between `busy` being taken
// and let go, by the one thread that took it.
unsafe impl Sync for Holder {}

impl Holder {
    const fn new() -> Holder {
        Holder {
            busy: AtomicBool::new(false),
            first: [const { Cell::new(std::ptr::null_mut()) }; CLASSES],
            held: [const { Cell::new(0) }; CLASSES],
            big: Cell::new(0),
            found: AtomicU64::new(0),
            asked: AtomicU64::new(0),
        }
    }

    /// A kept block of size `class`, or null (none, or the holder is busy).
    #[inline]
    fn take(&self, class: usize) -> *mut u8 {
        if self.busy.swap(true, Acquire) {
            return std::ptr::null_mut();
        }
        let at = self.first[class].get();
        if !at.is_null() {
            // SAFETY: a kept block's first word is the next one's address
            // (`keep`), and nobody else has the block.
            self.first[class].set(unsafe { at.cast::<*mut u8>().read() });
            self.held[class].set(self.held[class].get() - 1);
            if class >= BIG_FROM {
                self.big.set(self.big.get() - bytes(class));
            }
        }
        self.busy.store(false, Release);
        at
    }

    /// Keeps the block at `at` of size `class`, if there is room for it:
    /// whether it was kept.
    #[inline]
    fn keep(&self, class: usize, at: *mut u8) -> bool {
        if self.busy.swap(true, Acquire) {
            return false;
        }
        let room = self.held[class].get() < most(class) && (class < BIG_FROM || self.big.get() + bytes(class) <= BIG_MOST);
        if room {
            // SAFETY: the block is the caller's to let go, and holds 16
            // bytes at least.
            unsafe { at.cast::<*mut u8>().write(self.first[class].get()) };
            self.first[class].set(at);
            self.held[class].set(self.held[class].get() + 1);
            if class >= BIG_FROM {
                self.big.set(self.big.get() + bytes(class));
            }
        }
        self.busy.store(false, Release);
        room
    }
}

static HOLDERS_ALL: [Holder; HOLDERS] = [const { Holder::new() }; HOLDERS];

/// The calling thread's holder; `None` once the thread's own storage is gone.
#[inline]
fn holder() -> Option<&'static Holder> {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    // (No destructor: the allocator may not have storage of the thread's
    // with one.)
    thread_local!(static MINE: Cell<usize> = const { Cell::new(usize::MAX) });
    MINE.try_with(|mine| {
        if mine.get() == usize::MAX {
            mine.set(NEXT.fetch_add(1, Relaxed) % HOLDERS);
        }
        &HOLDERS_ALL[mine.get()]
    })
    .ok()
}

/// 0: not asked yet; 1: blocks are kept; 2: the system's heap alone.
static WHICH: AtomicU8 = AtomicU8::new(0);

#[inline]
fn keeping() -> bool {
    match WHICH.load(Relaxed) {
        1 => true,
        2 => false,
        _ => ask(),
    }
}

/// Reads `VICTORY_HEAP` without a block of the heap's (this is the heap).
#[cold]
fn ask() -> bool {
    use windows_sys::Win32::System::Environment::GetEnvironmentVariableW;
    const NAME: [u16; 13] = {
        let text = b"VICTORY_HEAP\0";
        let mut wide = [0u16; 13];
        let mut i = 0;
        while i < text.len() {
            wide[i] = text[i] as u16;
            i += 1;
        }
        wide
    };
    let mut value = [0u16; 16];
    // SAFETY: a name that ends in a zero and room for sixteen words.
    let n = unsafe { GetEnvironmentVariableW(NAME.as_ptr(), value.as_mut_ptr(), value.len() as u32) } as usize;
    let system = n == 6 && value[..6].iter().map(|&w| w as u8).eq(*b"system");
    // (Two threads asking at once find the same.)
    let _ = WHICH.compare_exchange(0, if system { 2 } else { 1 }, Relaxed, Relaxed);
    WHICH.load(Relaxed) == 1
}

/// The size of a block of `layout` if it is of a kept kind.
#[inline]
fn kept(layout: &Layout) -> Option<usize> {
    (layout.size() <= MOST && layout.align() <= 16 && keeping()).then(|| class(layout.size()))
}

/// What the system is asked for a block of size `class`.
#[inline]
fn whole(class: usize) -> Layout {
    // SAFETY: a size of at most a megabyte and a power of two to align to.
    unsafe { Layout::from_size_align_unchecked(bytes(class), 16) }
}

/// For the log: whether blocks are kept, and of the blocks of kept sizes
/// asked for since the last call, how many a kept one answered and how many
/// the system did.
pub(crate) fn line() -> (bool, u64, u64) {
    let (mut found, mut asked) = (0, 0);
    for holder in &HOLDERS_ALL {
        found += holder.found.swap(0, Relaxed);
        asked += holder.asked.swap(0, Relaxed);
    }
    (keeping(), found, asked)
}

// SAFETY: a block of a kept kind is always asked of the system, and given
// back to it, at its size's whole bytes and an alignment of 16, and is held
// by one owner at a time (the caller, or one holder's list); any other block
// goes to the system unchanged.
unsafe impl GlobalAlloc for Heap {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some(class) = kept(&layout) else {
            // SAFETY: the caller's contract, passed on.
            return unsafe { System.alloc(layout) };
        };
        if let Some(holder) = holder() {
            let at = holder.take(class);
            if !at.is_null() {
                holder.found.fetch_add(1, Relaxed);
                return at;
            }
            holder.asked.fetch_add(1, Relaxed);
        }
        // SAFETY: a layout of a size that is not zero.
        unsafe { System.alloc(whole(class)) }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let Some(class) = kept(&layout) else {
            // SAFETY: the caller's contract, passed on.
            return unsafe { System.alloc_zeroed(layout) };
        };
        if let Some(holder) = holder() {
            let at = holder.take(class);
            if !at.is_null() {
                holder.found.fetch_add(1, Relaxed);
                // SAFETY: the block holds its size's bytes, which are no
                // fewer than the layout's.
                unsafe { at.write_bytes(0, layout.size()) };
                return at;
            }
            holder.asked.fetch_add(1, Relaxed);
        }
        // SAFETY: a layout of a size that is not zero.
        unsafe { System.alloc_zeroed(whole(class)) }
    }

    #[inline]
    unsafe fn dealloc(&self, at: *mut u8, layout: Layout) {
        let Some(class) = kept(&layout) else {
            // SAFETY: the caller's contract, passed on.
            return unsafe { System.dealloc(at, layout) };
        };
        if holder().is_some_and(|holder| holder.keep(class, at)) {
            return;
        }
        // SAFETY: made by `System.alloc(whole(class))`, here or by whoever
        // kept it before.
        unsafe { System.dealloc(at, whole(class)) }
    }

    #[inline]
    unsafe fn realloc(&self, at: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the caller's contract: `size` with the same alignment is a
        // layout.
        let new = unsafe { Layout::from_size_align_unchecked(size, layout.align()) };
        match (kept(&layout), kept(&new)) {
            // The block holds its size's whole bytes.
            (Some(was), Some(will)) if was == will => at,
            // SAFETY: the system's block, made over by the system.
            (None, None) => unsafe { System.realloc(at, layout, size) },
            _ => {
                // SAFETY: the caller's contract for the new layout; the old
                // block holds `layout.size()` bytes and the new one `size`.
                unsafe {
                    let to = self.alloc(new);
                    if !to.is_null() {
                        std::ptr::copy_nonoverlapping(at, to, layout.size().min(size));
                        self.dealloc(at, layout);
                    }
                    to
                }
            }
        }
    }
}

/// Builds without the counting allocator of the diagnostics (`copystat`).
#[cfg(not(feature = "diagnostics"))]
#[global_allocator]
static HEAP: Heap = Heap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_size_has_a_class_that_holds_it_and_no_smaller_one_does() {
        assert_eq!(class(MOST), CLASSES - 1);
        assert_eq!(bytes(CLASSES - 1), MOST);
        let mut last = 0;
        for size in 1..=MOST {
            let c = class(size);
            assert!(c < CLASSES && c >= last, "size {size} class {c}");
            assert!(bytes(c) >= size, "size {size} in class {c} of {} bytes", bytes(c));
            assert!(c == 0 || bytes(c - 1) < size, "size {size} fits the class before {c}");
            // No more than a quarter over, and nothing up to FINE but the
            // rounding to 16.
            assert!(bytes(c) - size < if size <= FINE { 16 } else { size / 4 + 1 });
            last = c;
        }
    }

    /// A block of `size` bytes filled with a pattern that says where it is.
    unsafe fn filled(size: usize) -> *mut u8 {
        // SAFETY: a layout of a size that is not zero; the block holds it.
        unsafe {
            let at = Heap.alloc(Layout::from_size_align(size, 8).unwrap());
            assert!(!at.is_null());
            for i in 0..size {
                at.add(i).write((i as u8) ^ (size as u8));
            }
            at
        }
    }

    #[test]
    fn blocks_made_over_keep_their_bytes_whatever_the_sizes() {
        let sizes = [1usize, 15, 16, 17, 250, 256, 257, 300, 4000, 5000, 70_000, 1 << 20, (1 << 20) + 1, 3 << 20, 100, 2 << 20, 9];
        // SAFETY: blocks of the test's own, each made over with the layout
        // it has and let go with the last.
        unsafe {
            let mut size = sizes[0];
            let mut at = filled(size);
            for &next in &sizes[1..] {
                let first = size;
                at = Heap.realloc(at, Layout::from_size_align(size, 8).unwrap(), next);
                assert!(!at.is_null());
                for i in 0..size.min(next) {
                    assert_eq!(at.add(i).read(), (i as u8) ^ (first as u8), "byte {i} from {size} to {next}");
                }
                size = next;
                for i in 0..size {
                    at.add(i).write((i as u8) ^ (size as u8));
                }
            }
            Heap.dealloc(at, Layout::from_size_align(size, 8).unwrap());
        }
    }

    #[test]
    fn a_block_let_go_is_the_next_one_of_its_size_and_zeroed_if_asked() {
        if !keeping() {
            return;
        }
        // SAFETY: blocks of the test's own. (A thread of its own: nothing
        // else uses its holder meanwhile, unless 256 threads run.)
        std::thread::spawn(|| unsafe {
            let layout = Layout::from_size_align(3000, 16).unwrap();
            let a = Heap.alloc(layout);
            a.write_bytes(0xa5, 3000);
            Heap.dealloc(a, layout);
            // 3001 to 3072 bytes are of the same size as 3000.
            let again = Layout::from_size_align(3072, 8).unwrap();
            let b = Heap.alloc_zeroed(again);
            assert_eq!(a, b);
            assert!(std::slice::from_raw_parts(b, 3072).iter().all(|&byte| byte == 0));
            Heap.dealloc(b, again);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn blocks_made_by_one_thread_and_let_go_by_others_come_to_no_harm() {
        let (send, receive) = std::sync::mpsc::channel::<(usize, usize)>();
        let receive = std::sync::Arc::new(std::sync::Mutex::new(receive));
        let takers: Vec<_> = (0..4)
            .map(|_| {
                let receive = receive.clone();
                std::thread::spawn(move || {
                    loop {
                        let Ok((at, size)) = receive.lock().unwrap().recv() else { return };
                        let at = at as *mut u8;
                        // SAFETY: the block the maker sent, whole and ours now.
                        unsafe {
                            for i in (0..size).step_by(97) {
                                assert_eq!(at.add(i).read(), (i as u8) ^ (size as u8));
                            }
                            Heap.dealloc(at, Layout::from_size_align(size, 8).unwrap());
                        }
                    }
                })
            })
            .collect();
        let mut size = 1usize;
        for n in 0..20_000usize {
            size = (size * 31 + n) % 200_000 + 1;
            // SAFETY: a block of the test's own, given to a taker whole.
            send.send((unsafe { filled(size) } as usize, size)).unwrap();
        }
        drop(send);
        for taker in takers {
            taker.join().unwrap();
        }
    }
}
