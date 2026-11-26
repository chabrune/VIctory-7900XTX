//! The console's second display plane, drawn over the first at present.
//!
//! A console shows up to two planes its display hardware puts together: the
//! title's picture and, over it, a plane of interface with its own size and
//! alpha. With the Series S profile the title presents both: the picture at
//! 1920x1080 in plane 0 and its interface (menus, subtitles, the HUD) at
//! 3840x2160 in plane 1. The layer showed plane 0 alone: the menus, which
//! are all interface, came out black, and the world had no subtitles. (With
//! the Series X profile the title presents one plane, the interface already
//! in it.)
//!
//! Here both planes are read by one pass that writes the back buffer, each
//! sampled linearly at its size (4K into 1080p takes the mean of each 2x2):
//! `VICTORY_PLANE_ORDER=over` (the default), plane 1 over plane 0 by plane
//! 1's alpha; `under`, plane 0 over plane 1 by plane 0's alpha (wrong for
//! this title: its picture's alpha is not all 1, and the interface shows
//! through in patches); `off`, plane 0 alone, as before. Plane 1 is opaque
//! while the title loads: black with a spinner, the world being made behind
//! it (with plane 0 alone that world showed early, still loading).
//! shortcut: the planes' source rectangles and placement are not read: both
//! cover the whole screen, as the title's do.

use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::s;

use crate::host::Host;

/// How the two planes are put together.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    Off,
    /// Plane 1 over plane 0, by plane 1's alpha.
    Over,
    /// Plane 0 over plane 1, by plane 0's alpha.
    Under,
}

fn order() -> Order {
    static ORDER: OnceLock<Order> = OnceLock::new();
    *ORDER.get_or_init(|| match std::env::var("VICTORY_PLANE_ORDER").as_deref() {
        Ok("off") | Ok("0") => Order::Off,
        Ok("under") => Order::Under,
        _ => Order::Over,
    })
}

/// Whether plane 1 is used at all.
pub(crate) fn on() -> bool {
    order() != Order::Off
}

const SHADERS: &str = r#"
#define RS "DescriptorTable(SRV(t0, numDescriptors = 2), visibility = SHADER_VISIBILITY_PIXEL), RootConstants(num32BitConstants = 1, b0, visibility = SHADER_VISIBILITY_PIXEL), StaticSampler(s0, filter = FILTER_MIN_MAG_MIP_LINEAR, addressU = TEXTURE_ADDRESS_CLAMP, addressV = TEXTURE_ADDRESS_CLAMP, addressW = TEXTURE_ADDRESS_CLAMP, visibility = SHADER_VISIBILITY_PIXEL)"
Texture2D<float4> picture : register(t0);
Texture2D<float4> overlay : register(t1);
cbuffer Order : register(b0) { uint under; };
SamplerState linear_clamp : register(s0);
struct Corner { float4 p : SV_Position; float2 uv : TEXCOORD0; };
Corner corner(uint i : SV_VertexID) {
    Corner c;
    float2 t = float2(float(i & 1) * 2.0, float(i >> 1) * 2.0);
    c.uv = t;
    c.p = float4(t.x * 2.0 - 1.0, 1.0 - t.y * 2.0, 0.0, 1.0);
    return c;
}
[RootSignature(RS)]
float4 both(Corner c) : SV_Target {
    float4 p0 = picture.Sample(linear_clamp, c.uv);
    float4 p1 = overlay.Sample(linear_clamp, c.uv);
    float3 rgb = under != 0 ? lerp(p1.rgb, p0.rgb, saturate(p0.a)) : lerp(p0.rgb, p1.rgb, saturate(p1.a));
    return float4(rgb, 1.0);
}
"#;

/// Pairs of views of the planes, in a shader-visible heap of ours used as
/// a ring (a present's list runs long before the ring comes round).
const VIEWS: u32 = 128;

struct Pass {
    views: ID3D12DescriptorHeap,
    cpu: usize,
    gpu: u64,
    step: u32,
    next: AtomicU32,
    /// Per back buffer format: root signature and pipeline (`None`: refused).
    made: Mutex<Vec<(i32, Option<(ID3D12RootSignature, ID3D12PipelineState)>)>>,
}

// SAFETY: D3D12 objects are free-threaded.
unsafe impl Send for Pass {}
// SAFETY: as above.
unsafe impl Sync for Pass {}

fn pass(host: &Host) -> Option<&'static Pass> {
    static PASS: OnceLock<Option<Pass>> = OnceLock::new();
    PASS.get_or_init(|| {
        let kind = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
        // SAFETY: plain heap creation and queries.
        unsafe {
            let views: ID3D12DescriptorHeap = host
                .device
                .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: kind,
                    NumDescriptors: VIEWS,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                    NodeMask: 0,
                })
                .ok()?;
            Some(Pass {
                cpu: views.GetCPUDescriptorHandleForHeapStart().ptr,
                gpu: views.GetGPUDescriptorHandleForHeapStart().ptr,
                step: host.device.GetDescriptorHandleIncrementSize(kind),
                views,
                next: AtomicU32::new(0),
                made: Mutex::default(),
            })
        }
    })
    .as_ref()
}

impl Pass {
    fn pipeline(&self, host: &Host, format: DXGI_FORMAT) -> Option<(ID3D12RootSignature, ID3D12PipelineState)> {
        let mut made = self.made.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, p)) = made.iter().find(|m| m.0 == format.0) {
            return p.clone();
        }
        let p = match make(host, format) {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!(error = %e, format = format.0, "no pipeline to draw the interface plane: plane 0 alone");
                None
            }
        };
        made.push((format.0, p.clone()));
        p
    }
}

fn make(host: &Host, format: DXGI_FORMAT) -> windows::core::Result<(ID3D12RootSignature, ID3D12PipelineState)> {
    let vs = crate::host::compile(SHADERS, s!("corner"), s!("vs_5_1"))?;
    let ps = crate::host::compile(SHADERS, s!("both"), s!("ps_5_1"))?;
    let code = |bytes: &[u8]| D3D12_SHADER_BYTECODE {
        pShaderBytecode: bytes.as_ptr().cast(),
        BytecodeLength: bytes.len(),
    };
    let mut formats = [DXGI_FORMAT_UNKNOWN; 8];
    formats[0] = format;
    // SAFETY: the root signature is inside the pixel shader; the description
    // points at live data and never releases its pointer.
    unsafe {
        let root: ID3D12RootSignature = host.device.CreateRootSignature(0, &ps[..])?;
        let desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::transmute_copy(&root),
            VS: code(&vs[..]),
            PS: code(&ps[..]),
            BlendState: D3D12_BLEND_DESC {
                AlphaToCoverageEnable: false.into(),
                IndependentBlendEnable: false.into(),
                RenderTarget: [D3D12_RENDER_TARGET_BLEND_DESC {
                    BlendEnable: false.into(),
                    LogicOpEnable: false.into(),
                    SrcBlend: D3D12_BLEND_ONE,
                    DestBlend: D3D12_BLEND_ZERO,
                    BlendOp: D3D12_BLEND_OP_ADD,
                    SrcBlendAlpha: D3D12_BLEND_ONE,
                    DestBlendAlpha: D3D12_BLEND_ZERO,
                    BlendOpAlpha: D3D12_BLEND_OP_ADD,
                    LogicOp: D3D12_LOGIC_OP_NOOP,
                    RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
                }; 8],
            },
            SampleMask: u32::MAX,
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: D3D12_CULL_MODE_NONE,
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            RTVFormats: formats,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            ..Default::default()
        };
        let pipeline: ID3D12PipelineState = host.device.CreateGraphicsPipelineState(&desc)?;
        Ok((root, pipeline))
    }
}

/// The format plane 1 is read as: its own, a typeless one as its plain
/// sibling; `None` for what the pass does not read.
fn view_format(format: DXGI_FORMAT) -> Option<DXGI_FORMAT> {
    Some(match format.0 {
        27 => DXGI_FORMAT_R8G8B8A8_UNORM,
        28 | 29 => format,
        90 => DXGI_FORMAT_B8G8R8A8_UNORM,
        87 | 91 => format,
        23 => DXGI_FORMAT_R10G10B10A2_UNORM,
        24 => format,
        9 => DXGI_FORMAT_R16G16B16A16_FLOAT,
        10 => format,
        _ => return None,
    })
}

/// Records into `list` the pass that writes `back` (a back buffer of
/// `format`, `size`, in the copy destination state and left in it) from
/// `picture` (plane 0, in the copy source state and left in it) and
/// `overlay` (plane 1, in host state `state` and left in it), by
/// [`order`]. False if nothing was drawn (the copy of plane 0 stands).
#[allow(clippy::too_many_arguments)]
pub(crate) fn compose(
    host: &Host,
    list: &ID3D12GraphicsCommandList,
    back: &ID3D12Resource,
    format: DXGI_FORMAT,
    (width, height): (u32, u32),
    picture: &ID3D12Resource,
    overlay: &ID3D12Resource,
    state: D3D12_RESOURCE_STATES,
) -> bool {
    if !on() {
        return false;
    }
    // SAFETY: live resources.
    let (desc0, desc1) = unsafe { (picture.GetDesc(), overlay.GetDesc()) };
    let (Some(format0), Some(format1)) = (view_format(desc0.Format), view_format(desc1.Format)) else {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            tracing::warn!(picture = desc0.Format.0, interface = desc1.Format.0, "planes of a format not drawn: plane 0 alone")
        });
        return false;
    };
    let Some(pass) = pass(host) else { return false };
    let Some((root, pipeline)) = pass.pipeline(host, format) else { return false };
    let slot = (pass.next.fetch_add(1, Ordering::Relaxed) % (VIEWS / 2)) * 2;
    let cpu = |i: u32| D3D12_CPU_DESCRIPTOR_HANDLE { ptr: pass.cpu + ((slot + i) * pass.step) as usize };
    let gpu = D3D12_GPU_DESCRIPTOR_HANDLE { ptr: pass.gpu + u64::from(slot * pass.step) };
    let view = |format: DXGI_FORMAT| D3D12_SHADER_RESOURCE_VIEW_DESC {
        Format: format,
        ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
        Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
        Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
            Texture2D: D3D12_TEX2D_SRV { MostDetailedMip: 0, MipLevels: 1, PlaneSlice: 0, ResourceMinLODClamp: 0.0 },
        },
    };
    const READ: D3D12_RESOURCE_STATES = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
    let rtv = host.rtv(Some(back), None);
    let under = u32::from(order() == Order::Under);
    // SAFETY: an open list of ours; live resources; slots of our heap.
    unsafe {
        host.device.CreateShaderResourceView(picture, Some(&view(format0)), cpu(0));
        host.device.CreateShaderResourceView(overlay, Some(&view(format1)), cpu(1));
        let mut into = vec![
            crate::host::transition(back, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_RENDER_TARGET),
            crate::host::transition(picture, D3D12_RESOURCE_STATE_COPY_SOURCE, READ),
        ];
        if state != READ {
            into.push(crate::host::transition(overlay, state, READ));
        }
        list.ResourceBarrier(&into);
        list.SetDescriptorHeaps(&[Some(pass.views.clone())]);
        list.SetGraphicsRootSignature(&root);
        list.SetPipelineState(&pipeline);
        list.SetGraphicsRootDescriptorTable(0, gpu);
        list.SetGraphicsRoot32BitConstant(1, under, 0);
        list.OMSetRenderTargets(1, Some(&rtv), false, None);
        list.RSSetViewports(&[D3D12_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: width as f32,
            Height: height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        }]);
        list.RSSetScissorRects(&[RECT { left: 0, top: 0, right: width as i32, bottom: height as i32 }]);
        list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        list.DrawInstanced(3, 1, 0, 0);
        let mut out = vec![
            crate::host::transition(back, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_COPY_DEST),
            crate::host::transition(picture, READ, D3D12_RESOURCE_STATE_COPY_SOURCE),
        ];
        if state != READ {
            out.push(crate::host::transition(overlay, READ, state));
        }
        list.ResourceBarrier(&out);
    }
    static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !SHOWN.swap(true, Ordering::Relaxed) {
        tracing::info!(
            width = desc1.Width,
            height = desc1.Height,
            format = desc1.Format.0,
            under = under != 0,
            "the title's two display planes are put together"
        );
    }
    true
}
