//! What host lists and their allocators cost in the process's private memory
//! on the driver at hand (a measurement, run by name:
//! `cargo test -p victory-gpu --lib listmem -- --ignored --nocapture`).

use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;

/// The process's private bytes.
fn private_bytes() -> u64 {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut c = PROCESS_MEMORY_COUNTERS { cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
    // SAFETY: our own process and a sized structure.
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    c.PagefileUsage as u64
}

#[test]
#[ignore = "a measurement, not a check"]
fn what_lists_and_allocators_cost() {
    let Some(host) = crate::host::host() else { return };
    let device = &host.device;
    let device4: ID3D12Device4 = device.cast().unwrap();
    const N: usize = 64;
    let kb = |from: u64, to: u64| (to as i64 - from as i64) / 1024 / N as i64;
    // SAFETY: plain object creation and recording of lists of the test's own;
    // nothing is executed.
    unsafe {
        // (The first of each makes the driver's own pools.)
        let _first: ID3D12CommandAllocator = device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
        let _first_list: ID3D12GraphicsCommandList =
            device4.CreateCommandList1(0, D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_LIST_FLAG_NONE).unwrap();
        let t0 = private_bytes();
        let allocators: Vec<ID3D12CommandAllocator> =
            (0..N).map(|_| device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap()).collect();
        let t1 = private_bytes();
        let lists: Vec<ID3D12GraphicsCommandList> = (0..N)
            .map(|_| device4.CreateCommandList1(0, D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_LIST_FLAG_NONE).unwrap())
            .collect();
        let t2 = private_bytes();
        // Each list recorded once on an allocator of its own, with nothing in it.
        for (list, allocator) in lists.iter().zip(&allocators) {
            list.Reset(allocator, None).unwrap();
            list.Close().unwrap();
        }
        let t3 = private_bytes();
        // N more lists, all recorded on one allocator, one after the other
        // (a closed list leaves its allocator free for the next).
        let shared: ID3D12CommandAllocator = device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
        let more: Vec<ID3D12GraphicsCommandList> = (0..N)
            .map(|_| device4.CreateCommandList1(0, D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_LIST_FLAG_NONE).unwrap())
            .collect();
        let t4 = private_bytes();
        for list in &more {
            list.Reset(&shared, None).unwrap();
            list.Close().unwrap();
        }
        let t5 = private_bytes();
        // One list recorded N times on the allocators of the first (a
        // prelude's list taken from a pool, on its list's allocator).
        let one = &more[0];
        for allocator in &allocators {
            one.Reset(allocator, None).unwrap();
            one.Close().unwrap();
        }
        let t6 = private_bytes();
        // As the host makes them now: an allocator and an open list, N times.
        let pairs: Vec<(ID3D12CommandAllocator, ID3D12GraphicsCommandList)> = (0..N)
            .map(|_| {
                let allocator: ID3D12CommandAllocator = device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
                let list: ID3D12GraphicsCommandList =
                    device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None).unwrap();
                list.Close().unwrap();
                (allocator, list)
            })
            .collect();
        let t7 = private_bytes();
        // Allocators reset: whether anything comes back.
        for allocator in &allocators {
            allocator.Reset().unwrap();
        }
        let t8 = private_bytes();
        drop(pairs);
        let t9 = private_bytes();
        println!(
            "per object, KiB of private memory ({N} of each): an allocator {}, a list made closed {}, its first recording on an allocator of its own {}; \
             a list made closed (again) {}, its recording on one shared allocator {}; one list recorded on each of {N} used allocators {} per recording; \
             an allocator with an open list, as the host makes them, {}; resetting {N} allocators {}; releasing the pairs {}",
            kb(t0, t1),
            kb(t1, t2),
            kb(t2, t3),
            kb(t3, t4),
            kb(t4, t5),
            kb(t5, t6),
            kb(t6, t7),
            kb(t7, t8),
            kb(t8, t9),
        );
    }
}
