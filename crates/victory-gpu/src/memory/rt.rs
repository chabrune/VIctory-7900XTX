//! The card's own ray tracing behind the title's traversal
//! (`VICTORY_RT_QUERY=1`, `victory_shader::emit::ray_queries`;
//! `docs/hardware-ray-tracing.md`): for the title's bottom-level structures
//! that are unpacked in the arenas, a structure of the host's built from
//! their triangle nodes, its view in the view heaps, and the root's entry in
//! the table translated trace programs look their box nodes up in
//! (`victory_shader::emit::RT_ROOTS`).
//!
//! A structure goes through these steps, each in a later frame than the one
//! before:
//!
//! 1. its header (64 bytes, whole only on the card) is copied to memory the
//!    CPU reads, by the list of a dispatch that walks structures;
//! 2. read: a bottom level that is unpacked, with its triangles one to a
//!    64-byte node in one stretch, all in arena pages. The stretch is copied
//!    into a buffer and the host's structure built from it where it lies
//!    (vertices 4 bytes apart, indices 16 i, 16 i + 3, 16 i + 6), then a top
//!    level of that one instance, again in such a list: the builder's stages
//!    that unpacked the structure ran in lists before it;
//! 3. once that list was submitted and two frames have passed, the root's
//!    entry is written. From then on a ray that comes to the root is the
//!    card's.
//!
//! shortcut: a structure the title replaces by another at the same address,
//! with memory.rs not seeing the first go, keeps its entry (wrong geometry
//! until then). Only structures the module follows as packed ones are taken,
//! and their entry goes when they do.
//! shortcut: the host's structures are not compacted, and never given back
//! while the title runs (their numbers are not used again).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use victory_shader::emit::{RT_FIRST_SLOT, RT_ROOT_THERE, RT_ROOTS, RT_ROOTS_AT, RT_STRUCTURES, rt_root_entry};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::Interface;

use super::{Memory, PAGE};
use crate::descriptor::host::HostHeap;
use crate::host::{HostList, host};

/// Headers being read at a time.
const HEADERS: usize = 256;
/// Headers asked for in a frame, at most.
const ASKS: usize = 24;
/// Triangles whose structures are built in a frame, at most (a build is
/// some 7 ms of the card for 100,000).
const TRIANGLES: u32 = 40_000;
/// The most triangles of a structure (the shared indices).
const MOST: u32 = 1 << 20;
/// Frames after its list was submitted before a structure's entry is
/// written.
const SETTLE: u64 = 2;
/// Frames before a header that was not ready is asked for again.
const AGAIN: u64 = 30;

/// Where a title structure stands.
enum Known {
    /// Header copied in `frame` to place `at` of the read-back memory.
    Asked { frame: u64, at: usize },
    /// Not (yet) one to take: asked again from `frame` on, if ever.
    Later { frame: u64 },
    /// Built in a list (`sent` once that was submitted, in `frame` if so):
    /// the host's structure `number`, and its entry's two key dwords and
    /// first triangle node.
    Built { number: u32, frame: u64, sent: Arc<AtomicBool>, key: [u32; 2], first: u32 },
    /// Its entry is in the table at `entry`.
    There { entry: u32 },
}

#[derive(Default)]
pub(super) struct Rt {
    known: HashMap<u64, Known>,
    /// The headers' read-back memory and its mapping.
    headers: Option<(ID3D12Resource, usize)>,
    next: usize,
    /// The shared indices (32-bit, three a triangle).
    indices: Option<ID3D12Resource>,
    /// The host's structures by number: the top level a view shows, and the
    /// bottom level it holds.
    structures: Vec<(ID3D12Resource, ID3D12Resource)>,
    /// Per view heap: how many structures have their view.
    heaps: HashMap<usize, u32>,
    frame: u64,
    asked: usize,
    built: u32,
    /// Since the start: triangles in the host's structures, bytes of them,
    /// structures turned down, entries whose place another root had.
    stats: [u64; 4],
    said: u64,
}

/// A buffer of the card's own memory in `state`.
fn card_buffer(size: u64, state: D3D12_RESOURCE_STATES) -> Option<ID3D12Resource> {
    let heap = D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_DEFAULT, ..Default::default() };
    let desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: size.max(1),
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
        ..Default::default()
    };
    let mut buffer: Option<ID3D12Resource> = None;
    // SAFETY: plain creation of a device-local buffer.
    unsafe { host()?.device.CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &desc, state, None, &mut buffer) }.ok()?;
    buffer
}

fn turn(buffer: &ID3D12Resource, before: D3D12_RESOURCE_STATES, after: D3D12_RESOURCE_STATES) -> D3D12_RESOURCE_BARRIER {
    crate::host::transition(buffer, before, after)
}

fn written(buffer: &ID3D12Resource) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            // SAFETY: a borrowed pointer; the barrier never releases it.
            UAV: std::mem::ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER { pResource: unsafe { std::mem::transmute_copy(buffer) } }),
        },
    }
}

/// The slots a heap's run needs beside the memory copies' views.
pub(super) fn slots() -> u32 {
    if victory_shader::emit::ray_queries() { RT_STRUCTURES } else { 0 }
}

/// The bytes the directory's memory needs behind its entries.
pub(super) fn table_bytes() -> u64 {
    if victory_shader::emit::ray_queries() { u64::from(RT_ROOTS) * 16 } else { 0 }
}

impl Memory {
    /// The card buffer and the offset in it of the arena page that holds
    /// title address `address`.
    fn rt_at(&self, address: u64) -> Option<(&ID3D12Resource, u64)> {
        let held = self.pages.get(&(address / PAGE))?;
        let shadow = &self.copies[held.copy];
        Some((&shadow.buffer, held.at as u64 - shadow.span().0 + address % PAGE))
    }

    /// The table's entry `entry`: its four dwords in the directory's memory.
    fn rt_entry(&self, entry: u32) -> *mut u32 {
        // SAFETY: inside the directory's memory, which has the table behind
        // its entries when the switch is on (`table_bytes`).
        unsafe { self.directory.0.add((RT_ROOTS_AT / 4 + entry * 4) as usize) }
    }

    /// The packed structure at `address` is gone: so is its root's entry.
    pub(super) fn rt_gone(&mut self, address: u64) {
        if let Some(Known::There { entry }) = self.rt.known.remove(&address) {
            let at = self.rt_entry(entry);
            for n in 0..4 {
                // SAFETY: an entry of the table, under the mutex.
                unsafe { at.add(n).write_volatile(0) };
            }
        }
    }

    /// At a dispatch that walks structures, recorded into `list` with the
    /// view heap `heap` whose run of slots starts at `base`: the next steps
    /// of the structures on their way (the module's text), within a frame's
    /// share.
    pub(super) fn rt_ready(&mut self, list: &HostList, heap: &HostHeap, base: u32) {
        if !victory_shader::emit::ray_queries() {
            return;
        }
        let Some(host) = host() else { return };
        if self.rt.frame != self.frame {
            self.rt.frame = self.frame;
            self.rt.asked = 0;
            self.rt.built = 0;
            self.rt_settle();
        }
        // The views of the structures made since this heap was last here.
        let key = std::ptr::from_ref(heap) as usize;
        let shown = self.rt.heaps.get(&key).copied().unwrap_or(0);
        for (n, (top, _)) in self.rt.structures.iter().enumerate().skip(shown as usize) {
            heap.structure_at(base + RT_FIRST_SLOT + n as u32, top);
        }
        self.rt.heaps.insert(key, self.rt.structures.len() as u32);
        // Once a list: the copies and builds below are a frame's, not a
        // dispatch's.
        if !list.first_use(usize::MAX - 0x7274) {
            return;
        }
        if self.rt.headers.is_none() {
            self.rt.headers = host.shared_buffer((HEADERS * 64) as u64).map(|(buffer, data)| (buffer, data as usize));
        }
        let Some((headers, mapped)) = self.rt.headers.clone() else { return };
        let frame = self.frame;
        // Headers that came.
        let came: Vec<(u64, usize)> = self
            .rt
            .known
            .iter()
            .filter_map(|(&address, known)| match known {
                Known::Asked { frame: asked, at } if asked + SETTLE <= frame => Some((address, *at)),
                _ => None,
            })
            .collect();
        for (address, at) in came {
            // SAFETY: inside the read-back mapping; the copy ran frames ago.
            let h = unsafe { std::slice::from_raw_parts((mapped + at * 64) as *const u32, 16) }.to_vec();
            let (version, marks) = (h[0] & 0xFFFF, h[0] >> 16);
            let (triangles, first, root) = (h[3], h[13], h[12]);
            // A bottom level, unpacked, its triangles in one stretch of
            // whole nodes, its root a box node.
            let fit = version == 6
                && marks & 0x8102 == 0x8002
                && (1..=MOST).contains(&triangles)
                && first.is_multiple_of(64)
                && matches!(root & 7, 4 | 5);
            if !fit {
                // (Still packed, or being built: looked at again; anything
                // else too, at the same pace.)
                self.rt.known.insert(address, Known::Later { frame: frame + AGAIN });
                continue;
            }
            if self.rt.built + triangles > TRIANGLES && self.rt.built != 0 {
                // (Next frame: the header is asked for again then.)
                self.rt.known.insert(address, Known::Later { frame: frame + 1 });
                continue;
            }
            match self.rt_build(list, address, triangles, first, root) {
                Some(known) => {
                    self.rt.built += triangles;
                    self.rt.known.insert(address, known);
                }
                None => {
                    self.rt.stats[2] += 1;
                    self.rt.known.insert(address, Known::Later { frame: u64::MAX });
                }
            }
        }
        // Headers to ask for: the packed structures not looked at yet, and
        // those due again.
        let due: Vec<u64> = self
            .packed
            .keys()
            .copied()
            .filter(|address| match self.rt.known.get(address) {
                None => true,
                Some(Known::Later { frame: again }) => *again <= frame,
                _ => false,
            })
            .take(ASKS.saturating_sub(self.rt.asked))
            .collect();
        for address in due {
            let Some((buffer, offset)) = self.rt_at(address).map(|(b, o)| (b.clone(), o)) else {
                continue;
            };
            let at = self.rt.next % HEADERS;
            // (A place whose header was not read yet is not taken.)
            if self.rt.known.values().any(|k| matches!(k, Known::Asked { at: taken, .. } if *taken == at)) {
                break;
            }
            self.rt.next += 1;
            self.rt.asked += 1;
            // SAFETY: an open list and live buffers; 64 bytes inside each.
            unsafe {
                list.list.ResourceBarrier(&[turn(&buffer, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
                list.list.CopyBufferRegion(&headers, (at * 64) as u64, &buffer, offset, 64);
                list.list.ResourceBarrier(&[turn(&buffer, D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_COMMON)]);
            }
            self.rt.known.insert(address, Known::Asked { frame, at });
        }
        list.keep(&headers);
        if frame >= self.rt.said + 600 {
            self.rt.said = frame;
            let there = self.rt.known.values().filter(|k| matches!(k, Known::There { .. })).count();
            tracing::info!(
                "the card's own ray tracing (VICTORY_RT_QUERY=1): {} structures of the host's, {} triangles, {} MB; {} roots in the table, {} title structures known, {} turned down, {} roots without a place in the table",
                self.rt.structures.len(),
                self.rt.stats[0],
                self.rt.stats[1] >> 20,
                there,
                self.rt.known.len(),
                self.rt.stats[2],
                self.rt.stats[3]
            );
        }
    }

    /// The entries of structures whose build was submitted [`SETTLE`]
    /// frames ago are written.
    fn rt_settle(&mut self) {
        let frame = self.frame;
        let ready: Vec<u64> = self
            .rt
            .known
            .iter()
            .filter_map(|(&address, known)| match known {
                Known::Built { frame: built, sent, .. } if sent.load(Ordering::Relaxed) && built + SETTLE <= frame => Some(address),
                _ => None,
            })
            .collect();
        for address in ready {
            let Some(Known::Built { number, key, first, .. }) = self.rt.known.remove(&address) else { continue };
            let entry = rt_root_entry(key[0]);
            let at = self.rt_entry(entry);
            // SAFETY: an entry of the table, under the mutex.
            if unsafe { at.add(1).read_volatile() } != 0 {
                self.rt.stats[3] += 1;
                self.rt.known.insert(address, Known::Later { frame: u64::MAX });
                continue;
            }
            // SAFETY: as above. The key's second dword last: the entry
            // counts from then on.
            unsafe {
                at.write_volatile(key[0]);
                at.add(2).write_volatile(number);
                at.add(3).write_volatile(first);
                at.add(1).write_volatile(key[1]);
            }
            self.rt.known.insert(address, Known::There { entry });
        }
    }

    /// Records into `list` the host's structure for the title's bottom level
    /// at `address` (`triangles` nodes from byte `first`, root pointer
    /// `root`), and its top level of one instance. `None` if it cannot be
    /// made.
    fn rt_build(&mut self, list: &HostList, address: u64, triangles: u32, first: u32, root: u32) -> Option<Known> {
        let host = host()?;
        let number = self.rt.structures.len() as u32;
        if number >= RT_STRUCTURES {
            return None;
        }
        let device: ID3D12Device5 = host.device.cast().ok()?;
        let list4: ID3D12GraphicsCommandList4 = list.list.cast().ok()?;
        // The stretch of leaves, page by page: every page in an arena.
        let (from, bytes) = (address + u64::from(first), u64::from(triangles) * 64);
        let mut pieces = Vec::new();
        let mut at = from;
        while at < from + bytes {
            let n = (PAGE - at % PAGE).min(from + bytes - at);
            let (buffer, offset) = self.rt_at(at)?;
            pieces.push((buffer.clone(), offset, at - from, n));
            at += n;
        }
        if self.rt.indices.is_none() {
            let (buffer, data) = host.upload_buffer(u64::from(MOST) * 12)?;
            // SAFETY: a fresh mapping of that many dwords.
            let all = unsafe { std::slice::from_raw_parts_mut(data.cast::<u32>(), MOST as usize * 3) };
            for (i, t) in all.chunks_exact_mut(3).enumerate() {
                let node = 16 * i as u32;
                t.copy_from_slice(&[node, node + 3, node + 6]);
            }
            self.rt.indices = Some(buffer);
        }
        let indices = self.rt.indices.clone()?;
        let leaves = card_buffer(bytes, D3D12_RESOURCE_STATE_COMMON)?;
        // SAFETY: plain queries and creation; an open list; live buffers,
        // each range inside its buffer; what the builds read is kept by the
        // list until it has run.
        unsafe {
            let geometry = D3D12_RAYTRACING_GEOMETRY_DESC {
                Type: D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES,
                // (Every triangle counts: the title's own test decides.)
                Flags: D3D12_RAYTRACING_GEOMETRY_FLAG_OPAQUE,
                Anonymous: D3D12_RAYTRACING_GEOMETRY_DESC_0 {
                    Triangles: D3D12_RAYTRACING_GEOMETRY_TRIANGLES_DESC {
                        Transform3x4: 0,
                        IndexFormat: DXGI_FORMAT_R32_UINT,
                        VertexFormat: DXGI_FORMAT_R32G32B32_FLOAT,
                        IndexCount: triangles * 3,
                        VertexCount: triangles * 16,
                        IndexBuffer: indices.GetGPUVirtualAddress(),
                        VertexBuffer: D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE { StartAddress: leaves.GetGPUVirtualAddress(), StrideInBytes: 4 },
                    },
                },
            };
            let bottom = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
                Type: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_BOTTOM_LEVEL,
                Flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE,
                NumDescs: 1,
                DescsLayout: D3D12_ELEMENTS_LAYOUT_ARRAY,
                Anonymous: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0 { pGeometryDescs: &geometry },
            };
            let mut sizes = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO::default();
            device.GetRaytracingAccelerationStructurePrebuildInfo(&bottom, &mut sizes);
            if sizes.ResultDataMaxSizeInBytes == 0 {
                return None;
            }
            let state = D3D12_RESOURCE_STATE_RAYTRACING_ACCELERATION_STRUCTURE;
            let blas = card_buffer(sizes.ResultDataMaxSizeInBytes, state)?;
            let scratch = card_buffer(sizes.ScratchDataSizeInBytes, D3D12_RESOURCE_STATE_COMMON)?;
            let (instances, instance) = host.upload_buffer(64)?;
            let mut desc = D3D12_RAYTRACING_INSTANCE_DESC::default();
            (desc.Transform[0], desc.Transform[5], desc.Transform[10]) = (1.0, 1.0, 1.0);
            desc._bitfield1 = 0xFF << 24;
            desc.AccelerationStructure = blas.GetGPUVirtualAddress();
            instance.cast::<D3D12_RAYTRACING_INSTANCE_DESC>().write_unaligned(desc);
            let top = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
                Type: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL,
                Flags: D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PREFER_FAST_TRACE,
                NumDescs: 1,
                DescsLayout: D3D12_ELEMENTS_LAYOUT_ARRAY,
                Anonymous: D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS_0 { InstanceDescs: instances.GetGPUVirtualAddress() },
            };
            let mut top_sizes = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO::default();
            device.GetRaytracingAccelerationStructurePrebuildInfo(&top, &mut top_sizes);
            let tlas = card_buffer(top_sizes.ResultDataMaxSizeInBytes, state)?;
            let top_scratch = card_buffer(top_sizes.ScratchDataSizeInBytes, D3D12_RESOURCE_STATE_COMMON)?;
            // The leaves, as the card has them now.
            let mut sources: Vec<ID3D12Resource> = Vec::new();
            for (buffer, ..) in &pieces {
                if !sources.contains(buffer) {
                    sources.push(buffer.clone());
                }
            }
            let barriers = |before, after| sources.iter().map(|b| turn(b, before, after)).collect::<Vec<_>>();
            list.list.ResourceBarrier(&barriers(D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_SOURCE));
            for (buffer, offset, to, n) in &pieces {
                list.list.CopyBufferRegion(&leaves, *to, buffer, *offset, *n);
            }
            list.list.ResourceBarrier(&barriers(D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_COMMON));
            list.list.ResourceBarrier(&[turn(&leaves, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE)]);
            list4.BuildRaytracingAccelerationStructure(
                &D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
                    DestAccelerationStructureData: blas.GetGPUVirtualAddress(),
                    Inputs: bottom,
                    SourceAccelerationStructureData: 0,
                    ScratchAccelerationStructureData: scratch.GetGPUVirtualAddress(),
                },
                None,
            );
            list.list.ResourceBarrier(&[written(&blas)]);
            list4.BuildRaytracingAccelerationStructure(
                &D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
                    DestAccelerationStructureData: tlas.GetGPUVirtualAddress(),
                    Inputs: top,
                    SourceAccelerationStructureData: 0,
                    ScratchAccelerationStructureData: top_scratch.GetGPUVirtualAddress(),
                },
                None,
            );
            list.list.ResourceBarrier(&[written(&tlas)]);
            for kept in [&leaves, &scratch, &top_scratch, &instances] {
                list.keep(kept);
            }
            self.rt.stats[0] += u64::from(triangles);
            self.rt.stats[1] += sizes.ResultDataMaxSizeInBytes + top_sizes.ResultDataMaxSizeInBytes;
            self.rt.structures.push((tlas, blas));
        }
        // Told when the list is submitted: its structure exists from the
        // frames after that on.
        let sent = Arc::new(AtomicBool::new(false));
        let told = sent.clone();
        list.defer_host(move || told.store(true, Ordering::Relaxed));
        // The root's address in 64-byte units, as the trace programs look
        // it up.
        let node = (address + ((u64::from(root) >> 3) << 6)) >> 6;
        Some(Known::Built { number, frame: self.frame, sent, key: [node as u32, (node >> 32) as u32 | RT_ROOT_THERE], first: first / 64 })
    }
}
