//! The title's calls into its FidelityFX upscaler, seen on their way
//! (`amd_fidelityfx_loader_xbox.dll`, loaded with the Series S profile).
//!
//! The title imports four functions of the loader. Two of them are put
//! through ours, which call the loader's own: `ffxQuery`, where the title
//! asks the upscaler things such as the render size of a quality mode, and
//! `ffxDispatch`, where each frame it hands the upscaler its images, their
//! sizes and the sharpening it wants. The first calls are logged, so that
//! what the title asks for and how its descriptions are laid out on this
//! build can be read; nothing is changed.
//!
//! In the Series S profile the title renders its world at 1280x720 and the
//! upscaler takes it to 1920x1080: the scale of the upscaler's quality mode
//! (1.5), which is how the picture comes to look coarse on a larger screen.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// The loader's DLL, as the title imports it.
const LOADER: &str = "amd_fidelityfx_loader_xbox.dll";

/// The loader's own functions, once the title's imports are bound.
static QUERY: AtomicUsize = AtomicUsize::new(0);
static DISPATCH: AtomicUsize = AtomicUsize::new(0);

/// Calls of each kind logged so far.
static QUERIES_LOGGED: AtomicU32 = AtomicU32::new(0);
static DISPATCHES_LOGGED: AtomicU32 = AtomicU32::new(0);

type Call = extern "C" fn(*mut c_void, *mut u8) -> u32;

/// For the title's import of `dll!symbol`, which binds to `original`: the
/// address of ours to bind instead, if it is one of the two calls put
/// through ours.
pub fn intercept(dll: &str, symbol: &str, original: usize) -> Option<usize> {
    if !dll.eq_ignore_ascii_case(LOADER) || original == 0 {
        return None;
    }
    let (slot, ours) = match symbol {
        "ffxQuery" => (&QUERY, query as *const () as usize),
        "ffxDispatch" => (&DISPATCH, dispatch as *const () as usize),
        _ => return None,
    };
    slot.store(original, Ordering::Release);
    tracing::info!(symbol, "the title's FidelityFX call goes through ours");
    Some(ours)
}

/// The words (u32) of `len` bytes at `at`, read as the title laid them out.
fn words(at: *const u8, len: usize) -> Vec<u32> {
    // SAFETY: the title passes a description at least this long (the
    // header and the fields every description of these kinds has).
    (0..len / 4).map(|i| unsafe { at.cast::<u32>().add(i).read_unaligned() }).collect()
}

/// A description's chain: (type, address) of the description and of each
/// one its header's `pNext` names, as long as the chain is short.
fn chain(desc: *mut u8) -> Vec<(u64, usize)> {
    let mut out = Vec::new();
    let mut at = desc;
    while !at.is_null() && out.len() < 8 {
        // SAFETY: a header is a 64-bit type and a pointer to the next one.
        let (kind, next) = unsafe { (at.cast::<u64>().read_unaligned(), at.add(8).cast::<*mut u8>().read_unaligned()) };
        out.push((kind, at as usize));
        at = next;
    }
    out
}

extern "C" fn query(context: *mut c_void, desc: *mut u8) -> u32 {
    let original = QUERY.load(Ordering::Acquire);
    // SAFETY: the loader's own export, with the title's own arguments.
    let call: Call = unsafe { std::mem::transmute::<usize, Call>(original) };
    let before = (!desc.is_null()).then(|| words(desc, 64));
    let result = call(context, desc);
    if let Some(before) = before
        && QUERIES_LOGGED.fetch_add(1, Ordering::Relaxed) < 32
    {
        let after = words(desc, 64);
        tracing::info!(
            context = format_args!("{context:p}"),
            chain = ?chain(desc).iter().map(|(k, _)| format!("{k:#x}")).collect::<Vec<_>>(),
            result = format_args!("{result:#x}"),
            before = ?before,
            after = ?after,
            "FidelityFX query"
        );
        // A render size asked for: the words that point at the answers are
        // followed, if they look like pointers.
        for pair in after.chunks(2).skip(2).take(6) {
            let pointer = u64::from(pair[0]) | u64::from(pair.get(1).copied().unwrap_or(0)) << 32;
            if pointer > 0x10000 && pointer < 0x8000_0000_0000 {
                let mut value = [0u8; 4];
                // SAFETY: probed through the kernel, never dereferenced blind.
                let read = unsafe { probe(pointer as usize, &mut value) };
                if read {
                    tracing::info!(pointer = format_args!("{pointer:#x}"), value = u32::from_le_bytes(value), "  FidelityFX query: a value it points at");
                }
            }
        }
    }
    result
}

extern "C" fn dispatch(context: *mut c_void, desc: *mut u8) -> u32 {
    let n = DISPATCHES_LOGGED.fetch_add(1, Ordering::Relaxed);
    if !desc.is_null() && (n < 3 || n == 300) {
        let w = words(desc, 512);
        // Where a 1280x720 render size and a 1920x1080 output size lie,
        // word by word, whatever the layout.
        let sizes: Vec<usize> = (0..w.len().saturating_sub(1))
            .filter(|&i| matches!((w[i], w[i + 1]), (1280, 720) | (1920, 1080) | (960, 540) | (1600, 900)))
            .collect();
        tracing::info!(
            n,
            chain = ?chain(desc).iter().map(|(k, _)| format!("{k:#x}")).collect::<Vec<_>>(),
            sizes_at_word = ?sizes,
            words = ?w,
            "FidelityFX dispatch"
        );
    }
    let original = DISPATCH.load(Ordering::Acquire);
    // SAFETY: the loader's own export, with the title's own arguments.
    let call: Call = unsafe { std::mem::transmute::<usize, Call>(original) };
    if let Some(sharpness) = sharpness()
        && let Some(mut copy) = sharpened(desc, sharpness)
    {
        // The title's description stays as it is; the upscaler reads ours.
        return call(context, copy.as_mut_ptr().cast());
    }
    call(context, desc)
}

/// The upscale dispatch's description, as the title lays it out on this
/// build (the public FidelityFX API's, 432 bytes): its type, the render and
/// output sizes (words 94 to 97), the sharpening switch (byte 392) and its
/// strength (a float at byte 396). The title leaves sharpening off.
const DISPATCH_UPSCALE: u64 = 0x0001_0001;
const DISPATCH_BYTES: usize = 432;
const SHARPEN_AT: usize = 392;
const SHARPNESS_AT: usize = 396;

/// The upscaler's own sharpening (RCAS) at this strength, 0 to 1
/// (`VICTORY_FSR_SHARPNESS`; unset or 0: as the title asks, off).
fn sharpness() -> Option<f32> {
    static VALUE: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *VALUE.get_or_init(|| {
        let value = std::env::var("VICTORY_FSR_SHARPNESS").ok()?.trim().parse::<f32>().ok()?;
        let value = value.clamp(0.0, 1.0);
        (value > 0.0).then(|| {
            tracing::info!(sharpness = value, "the upscaler sharpens what it upscales (VICTORY_FSR_SHARPNESS)");
            value
        })
    })
}

/// A copy of the title's upscale dispatch with sharpening on at
/// `sharpness`, if the description is laid out as expected; `None` leaves
/// the title's own.
fn sharpened(desc: *mut u8, sharpness: f32) -> Option<[u32; DISPATCH_BYTES / 4]> {
    if desc.is_null() {
        return None;
    }
    let mut copy = [0u32; DISPATCH_BYTES / 4];
    // SAFETY: an upscale dispatch description is this long (checked by its
    // type below before anything of the copy is used).
    unsafe { std::ptr::copy_nonoverlapping(desc, copy.as_mut_ptr().cast::<u8>(), DISPATCH_BYTES) };
    let kind = u64::from(copy[0]) | u64::from(copy[1]) << 32;
    let next = u64::from(copy[2]) | u64::from(copy[3]) << 32;
    let (render, output) = ((copy[94], copy[95]), (copy[96], copy[97]));
    // The layout is the one seen: a lone description, sizes that make
    // sense, the switch a bool.
    let laid_out = kind == DISPATCH_UPSCALE
        && next == 0
        && render.0 > 0
        && render.1 > 0
        && output.0 >= render.0
        && output.1 >= render.1
        && output.0 <= 7680
        && copy[SHARPEN_AT / 4] <= 1;
    if !laid_out {
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| tracing::warn!(kind = format_args!("{kind:#x}"), ?render, ?output, "an upscale dispatch laid out otherwise: not sharpened"));
        return None;
    }
    copy[SHARPEN_AT / 4] = 1;
    copy[SHARPNESS_AT / 4] = sharpness.to_bits();
    Some(copy)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An upscale dispatch as the title lays it out (words seen in a run):
    /// render 1280x720, output 1920x1080, sharpening off.
    fn title_dispatch() -> [u32; DISPATCH_BYTES / 4] {
        let mut w = [0u32; DISPATCH_BYTES / 4];
        w[0] = 0x0001_0001;
        (w[94], w[95], w[96], w[97]) = (1280, 720, 1920, 1080);
        w[101] = 1.0f32.to_bits();
        w
    }

    #[test]
    fn sharpening_is_switched_on_in_a_copy_only() {
        let mut title = title_dispatch();
        let copy = sharpened(title.as_mut_ptr().cast(), 0.5).expect("laid out as seen");
        assert_eq!(copy[SHARPEN_AT / 4], 1);
        assert_eq!(f32::from_bits(copy[SHARPNESS_AT / 4]), 0.5);
        // Everything else as the title had it, and the title's own untouched.
        assert_eq!(&copy[..98], &title[..98]);
        assert_eq!(&copy[100..], &title[100..]);
        assert_eq!(title[SHARPEN_AT / 4], 0);
    }

    #[test]
    fn a_description_laid_out_otherwise_is_left_alone() {
        // Another kind of dispatch.
        let mut other = title_dispatch();
        other[0] = 0x0002_0001;
        assert!(sharpened(other.as_mut_ptr().cast(), 0.5).is_none());
        // A chained description.
        let mut chained = title_dispatch();
        chained[2] = 0x1234;
        assert!(sharpened(chained.as_mut_ptr().cast(), 0.5).is_none());
        // Sizes where they are not.
        let mut shifted = title_dispatch();
        (shifted[94], shifted[96]) = (1920, 1280);
        assert!(sharpened(shifted.as_mut_ptr().cast(), 0.5).is_none());
        assert!(sharpened(std::ptr::null_mut(), 0.5).is_none());
    }

    #[test]
    fn only_the_two_loader_calls_are_taken() {
        assert!(intercept("amd_fidelityfx_loader_xbox.dll", "ffxCreateContext", 0x1000).is_none());
        assert!(intercept("other.dll", "ffxQuery", 0x1000).is_none());
        assert!(intercept("AMD_FidelityFX_Loader_Xbox.dll", "ffxQuery", 0).is_none());
        assert!(intercept("AMD_FidelityFX_Loader_Xbox.dll", "ffxDispatch", 0x2000).is_some());
    }
}

/// Reads 4 bytes at `at` if they are readable, through the kernel.
unsafe fn probe(at: usize, out: &mut [u8; 4]) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let mut read = 0usize;
        // SAFETY: the kernel checks the source; the destination is ours.
        let ok = unsafe { ReadProcessMemory(GetCurrentProcess(), at as *const c_void, out.as_mut_ptr().cast(), 4, &mut read) };
        ok != 0 && read == 4
    }
    #[cfg(not(windows))]
    {
        let _ = (at, out);
        false
    }
}
