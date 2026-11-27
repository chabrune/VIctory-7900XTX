//! What the card's own ray tracing does (measurements, run by name:
//! `cargo test -p victory-gpu --lib hwrt -- --ignored --nocapture`): rays a
//! second through an acceleration structure of the host's, with inline ray
//! queries from a compute shader, as a translated trace program would use
//! them in place of the title's traversal with an emulated node test.
//!
//! The shader is compiled by the Windows SDK's `dxc.exe` if one is
//! installed (shader model 6.5); the tests say so and end if not.

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::Interface;

/// Rays in a thread group.
const GROUP: u32 = 64;

const SOURCE: &str = r#"
#define ROOT "SRV(t0), UAV(u0), RootConstants(num32BitConstants = 4, b0)"
RaytracingAccelerationStructure scene : register(t0);
RWByteAddressBuffer result : register(u0);
cbuffer Ask : register(b0) { uint rays; uint seed; float side; uint pad; };

uint hash(uint x) {
    x ^= x >> 16; x *= 0x7feb352du; x ^= x >> 15; x *= 0x846ca68bu; x ^= x >> 16;
    return x;
}
float unit(uint x) { return (x & 0xffffff) / 16777216.0; }

[RootSignature(ROOT)]
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) {
    if (id.x >= rays) return;
    uint h = hash(id.x * 4 + seed);
    // From a point in the scene's box, in any direction: as bounced light is asked for.
    float3 origin = float3(unit(h), unit(hash(h + 1)), unit(hash(h + 2))) * side;
    float3 d = float3(unit(hash(h + 3)), unit(hash(h + 4)), unit(hash(h + 5))) * 2 - 1;
    RayDesc ray;
    ray.Origin = origin;
    ray.Direction = normalize(d + 1e-4);
    ray.TMin = 0.0;
    ray.TMax = side;
    RayQuery<RAY_FLAG_FORCE_OPAQUE> query;
    query.TraceRayInline(scene, RAY_FLAG_NONE, 0xff, ray);
    query.Proceed();
    if (query.CommittedStatus() == COMMITTED_TRIANGLE_HIT) {
        uint was;
        result.InterlockedAdd(0, 1, was);
        result.InterlockedAdd(4, query.CommittedPrimitiveIndex() & 0xff, was);
    }
}
"#;

/// `source` compiled as a compute shader of model 6.5 by the newest
/// `dxc.exe` of the Windows SDK; `None` if there is none or it refuses.
fn compiled(source: &str) -> Option<Vec<u8>> {
    let kits = std::path::Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut found: Vec<std::path::PathBuf> =
        std::fs::read_dir(kits).ok()?.flatten().map(|e| e.path().join(r"x64\dxc.exe")).filter(|p| p.exists()).collect();
    found.sort();
    let dxc = found.pop()?;
    let dir = std::env::temp_dir().join(format!("victory-hwrt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let (from, to) = (dir.join("trace.hlsl"), dir.join("trace.cso"));
    std::fs::write(&from, source).ok()?;
    let ran = std::process::Command::new(&dxc).args(["-T", "cs_6_5", "-E", "main", "-Fo"]).arg(&to).arg(&from).output().ok()?;
    if !ran.status.success() {
        println!("dxc: {}", String::from_utf8_lossy(&ran.stderr));
        return None;
    }
    let code = std::fs::read(&to).ok();
    let _ = std::fs::remove_dir_all(&dir);
    code
}

/// A buffer of the card's own memory in `state`, for acceleration
/// structures and their scratch.
unsafe fn card_buffer(device: &ID3D12Device, size: u64, state: D3D12_RESOURCE_STATES) -> ID3D12Resource {
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
    // SAFETY: plain creation.
    unsafe { device.CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &desc, state, None, &mut buffer) }.expect("a buffer on the card");
    buffer.unwrap()
}

/// How many rays a second the card traces through a structure of
/// `triangles` small triangles spread in a cube, each ray from a point in
/// the cube in any direction, to the first hit.
#[test]
#[ignore = "a measurement on the card: run it by name with --ignored --nocapture"]
fn rays_a_second_through_the_cards_own_structures() {
    let Some(host) = crate::host::host() else { return };
    let device = &host.device;
    let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS5::default();
    // SAFETY: writes `options` of the given size.
    let asked = unsafe { device.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS5, (&raw mut options).cast(), size_of_val(&options) as u32) };
    println!("the card's ray tracing: asked {:?}, tier {}", asked.is_ok(), options.RaytracingTier.0);
    if asked.is_err() || options.RaytracingTier.0 < D3D12_RAYTRACING_TIER_1_1.0 {
        return;
    }
    let Some(code) = compiled(SOURCE) else {
        println!("no dxc.exe of the Windows SDK compiled the shader: nothing measured");
        return;
    };
    let device5: ID3D12Device5 = device.cast().unwrap();
    // `nodes`: the triangles lie as the title's leaves do, one in the first
    // 36 bytes of each 64-byte node, and the structure is built from them
    // where they are: vertices 4 bytes apart (the stride may be any
    // multiple of a component), and indices 16 i, 16 i + 3, 16 i + 6.
    for (triangles, nodes) in [(100_000u32, false), (100_000, true), (1_000_000, false), (1_000_000, true), (4_000_000, false)] {
        // SAFETY: plain creation, recording and execution of objects of the
        // test's own; nothing is read before the fence is reached.
        unsafe {
            const SIDE: f32 = 100.0;
            // Triangles of about `edge` a side, at random in the cube.
            let edge = SIDE / (triangles as f32).cbrt() * 1.5;
            let each = if nodes { 64 } else { 36 };
            let (vertices, data) = host.upload_buffer(u64::from(triangles) * each as u64).unwrap();
            let mut x = 0x1234_5678u32;
            let mut next = || {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x & 0xff_ffff) as f32 / 16_777_216.0
            };
            let floats = std::slice::from_raw_parts_mut(data.cast::<f32>(), triangles as usize * each / 4);
            for t in floats.chunks_exact_mut(each / 4) {
                // (What a node holds past its triangle is not the structure's.)
                t[9..].fill(f32::NAN);
                let at = [next() * SIDE, next() * SIDE, next() * SIDE];
                for v in 0..3 {
                    for c in 0..3 {
                        t[3 * v + c] = at[c] + (next() - 0.5) * edge;
                    }
                }
            }
            let (indices, index_data) = host.upload_buffer(u64::from(triangles) * 12).unwrap();
            if nodes {
                let all = std::slice::from_raw_parts_mut(index_data.cast::<u32>(), triangles as usize * 3);
                for (i, t) in all.chunks_exact_mut(3).enumerate() {
                    t.copy_from_slice(&[16 * i as u32, 16 * i as u32 + 3, 16 * i as u32 + 6]);
                }
            }
            let geometry = D3D12_RAYTRACING_GEOMETRY_DESC {
                Type: D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES,
                Flags: D3D12_RAYTRACING_GEOMETRY_FLAG_OPAQUE,
                Anonymous: D3D12_RAYTRACING_GEOMETRY_DESC_0 {
                    Triangles: D3D12_RAYTRACING_GEOMETRY_TRIANGLES_DESC {
                        Transform3x4: 0,
                        IndexFormat: if nodes { DXGI_FORMAT_R32_UINT } else { DXGI_FORMAT_UNKNOWN },
                        VertexFormat: DXGI_FORMAT_R32G32B32_FLOAT,
                        IndexCount: if nodes { triangles * 3 } else { 0 },
                        VertexCount: if nodes { triangles * 16 } else { triangles * 3 },
                        IndexBuffer: if nodes { indices.GetGPUVirtualAddress() } else { 0 },
                        VertexBuffer: D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE {
                            StartAddress: vertices.GetGPUVirtualAddress(),
                            StrideInBytes: if nodes { 4 } else { 12 },
                        },
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
            device5.GetRaytracingAccelerationStructurePrebuildInfo(&bottom, &mut sizes);
            let state = D3D12_RESOURCE_STATE_RAYTRACING_ACCELERATION_STRUCTURE;
            let blas = card_buffer(device, sizes.ResultDataMaxSizeInBytes, state);
            let scratch = card_buffer(device, sizes.ScratchDataSizeInBytes, D3D12_RESOURCE_STATE_COMMON);
            let blas_bytes = sizes.ResultDataMaxSizeInBytes;
            // One instance of it, where it is.
            let (instances, instance) = host.upload_buffer(64).unwrap();
            let mut desc = D3D12_RAYTRACING_INSTANCE_DESC::default();
            desc.Transform[0] = 1.0;
            desc.Transform[5] = 1.0;
            desc.Transform[10] = 1.0;
            desc._bitfield1 = 0xff << 24;
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
            device5.GetRaytracingAccelerationStructurePrebuildInfo(&top, &mut top_sizes);
            let tlas = card_buffer(device, top_sizes.ResultDataMaxSizeInBytes, state);
            let top_scratch = card_buffer(device, top_sizes.ScratchDataSizeInBytes, D3D12_RESOURCE_STATE_COMMON);

            let root: ID3D12RootSignature = device.CreateRootSignature(0, &code).unwrap();
            let pipeline: ID3D12PipelineState = device
                .CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                    pRootSignature: std::mem::transmute_copy(&root),
                    CS: D3D12_SHADER_BYTECODE { pShaderBytecode: code.as_ptr().cast(), BytecodeLength: code.len() },
                    ..Default::default()
                })
                .unwrap();
            let (result, back) = host.shared_buffer(4096).unwrap();
            let queue: ID3D12CommandQueue =
                device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC { Type: D3D12_COMMAND_LIST_TYPE_DIRECT, ..Default::default() }).unwrap();
            let allocator: ID3D12CommandAllocator = device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
            let list: ID3D12GraphicsCommandList4 = device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None).unwrap();
            let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
            let mut value = 0;
            let mut run = |list: &ID3D12GraphicsCommandList4| {
                list.Close().unwrap();
                let began = std::time::Instant::now();
                queue.ExecuteCommandLists(&[Some(list.cast().unwrap())]);
                value += 1;
                queue.Signal(&fence, value).unwrap();
                // No event: returns when the fence is there.
                fence.SetEventOnCompletion(value, windows::Win32::Foundation::HANDLE::default()).unwrap();
                began.elapsed()
            };
            let uav = |buffer: &ID3D12Resource| D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
                Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    UAV: std::mem::ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER { pResource: std::mem::transmute_copy(buffer) }),
                },
            };
            list.BuildRaytracingAccelerationStructure(
                &D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
                    DestAccelerationStructureData: blas.GetGPUVirtualAddress(),
                    Inputs: bottom,
                    SourceAccelerationStructureData: 0,
                    ScratchAccelerationStructureData: scratch.GetGPUVirtualAddress(),
                },
                None,
            );
            list.ResourceBarrier(&[uav(&blas)]);
            list.BuildRaytracingAccelerationStructure(
                &D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
                    DestAccelerationStructureData: tlas.GetGPUVirtualAddress(),
                    Inputs: top,
                    SourceAccelerationStructureData: 0,
                    ScratchAccelerationStructureData: top_scratch.GetGPUVirtualAddress(),
                },
                None,
            );
            list.ResourceBarrier(&[uav(&tlas)]);
            let built = run(&list);
            device.GetDeviceRemovedReason().ok().expect("the device after the builds");
            for rays in [1u32 << 20, 1 << 22, 1 << 24] {
                let mut best = std::time::Duration::MAX;
                let (mut hits, mut sum) = (0, 0);
                for turn in 0..4u32 {
                    back.cast::<u32>().write_volatile(0);
                    back.cast::<u32>().add(1).write_volatile(0);
                    allocator.Reset().unwrap();
                    list.Reset(&allocator, None).unwrap();
                    list.SetComputeRootSignature(&root);
                    list.SetPipelineState(&pipeline);
                    list.SetComputeRootShaderResourceView(0, tlas.GetGPUVirtualAddress());
                    list.SetComputeRootUnorderedAccessView(1, result.GetGPUVirtualAddress());
                    list.SetComputeRoot32BitConstants(2, 4, [rays, turn * 7919, SIDE.to_bits(), 0].as_ptr().cast(), 0);
                    list.Dispatch(rays.div_ceil(GROUP), 1, 1);
                    best = best.min(run(&list));
                    hits = back.cast::<u32>().read_volatile();
                    sum = back.cast::<u32>().add(1).read_volatile();
                }
                println!(
                    "{}{triangles} triangles, hits' sum {sum} (structure {} MB, built in {:.1} ms): {rays} rays in {:.2} ms, {:.0} million a second, {:.0} % hit",
                    if nodes { "in 64-byte nodes: " } else { "" },
                    blas_bytes >> 20,
                    built.as_secs_f64() * 1e3,
                    best.as_secs_f64() * 1e3,
                    f64::from(rays) / best.as_secs_f64() / 1e6,
                    f64::from(hits) * 100.0 / f64::from(rays)
                );
            }
            device.GetDeviceRemovedReason().ok().expect("the device after the traces");
            drop((vertices, indices, scratch, top_scratch, instances));
        }
    }
}
