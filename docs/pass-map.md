# The title's passes by the card's time (2026-11-27)

What each of the title's programs costs the card in a frame, by the program's own
name. `docs/findings.md` ("The title's program names, and the map of its passes") has
the names and the static instruction counts and says the per-program cost was not
measured, for want of a frame that replays. No replay is needed for it: a timing run
(`VICTORY_GPU_TIMING=1` with `VICTORY_D3D12_DRED=1`) already gives the card's time of
every command with the hashes of its programs, and `VICTORY_GPU_TIMING_ROWS=4000`
prints all of them instead of the forty heaviest. The join with the names table
(`passes.tsv` and `shader-names.tsv` in the cache folder, made by the scripts
`docs/findings.md` names) is `passmap.py` beside them: it reads the log's largest late
report, takes the pixel or compute program of each command (a mesh or vertex program
where there is no other) and sums by family and by pass. The passes are groups of
words in the names, so a family may sit in the wrong one; the family table is the
measurement, the pass table a reading of it.

Both runs: the user's save inside the house (interior, the character standing), RX
7700 XT, series-s profile, diag build, 70 s, the report of the last 5 s. A timing run
serializes the card (5-10 frames a second), so these are the card's own milliseconds
per command, not shares of a played frame; a played frame here takes 35-38 ms.

## A replayed frame is the measure; the timing run's single commands are not

The timing run's heavy single commands do not stand. The frame of the open world the
main checkout's session recorded (no ray tracing; its `recw/capture-1`), replayed with
`victory replay <capture> --timing detailed --top 4000` (runs rp1 to rp3: 23.4-24.6 ms
on the card, median of 5; 23.7 with list timing only), gives
`PS_CompositeIndirectDrawBlend` 0.01 ms where the timing run gave 2.4,
`CS_GatherRaymarchedLocalLightsClassified16` 0.26 for 1.05 and the probes' temporal
resolve 0.22 for 0.7. Only `CS_ScreenProbes_ActivateRays` is heavy both ways (1.76 ms
for 2.7). The timing run waits for the card at each command, and a command that is
the first to need what earlier ones left pending is charged their time; its sums by
kind of command are of the right order (30 ms against 24), its single rows are not to
be believed without a replay. `replaymap.py` beside `passmap.py` sums a replay's
report the same way:

1393 pipelines, 16.35 ms in 5903 draws and dispatches

| pass | families | commands | ms | share |
|---|---:|---:|---:|---:|
| ray tracing, probes, light cache | 110 | 1093 | 3.61 | 22.1 % |
| geometry and materials, others | 461 | 1277 | 2.89 | 17.7 % |
| the layer's own passes | 4 | 821 | 1.75 | 10.7 % |
| shadows | 42 | 1356 | 1.75 | 10.7 % |
| not named | 40 | 202 | 1.18 | 7.2 % |
| post and picture | 24 | 48 | 1.14 | 7.0 % |
| depth, velocity, masks | 39 | 377 | 1.08 | 6.6 % |
| lighting (deferred) | 54 | 266 | 0.96 | 5.9 % |
| sky, fog, water, wind, particles | 51 | 194 | 0.68 | 4.2 % |
| culling, skinning, upkeep | 56 | 199 | 0.50 | 3.1 % |
| hair | 13 | 51 | 0.42 | 2.6 % |
| composite of the lit picture | 5 | 5 | 0.33 | 2.0 % |
| mirror reflections | 3 | 14 | 0.06 | 0.4 % |

| family | pass | pipelines | commands | ms |
|---|---|---:|---:|---:|
| CS_ScreenProbes_ActivateRays | ray tracing, probes, light cache | 1 | 1 | 1.76 |
| victory: argument clamp | the layer's own passes | 1 | 157 | 0.76 |
| CS_TAA2_Resolve | post and picture | 1 | 1 | 0.74 |
| copies to (a texture, in a block) | the layer's own passes | 89 | 177 | 0.64 |
| MS_VS_CascadeShadows_draw | shadows | 16 | 439 | 0.41 |
| PS_draw_screen_space | geometry and materials, others | 1 | 75 | 0.38 |
| PS_DeferredTextured | lighting (deferred) | 18 | 166 | 0.35 |
| victory: card compare | the layer's own passes | 1 | 430 | 0.34 |
| ? compute 82a2ad6d07417d36 |  | 1 | 1 | 0.33 |
| PS_LinearDepth | depth, velocity, masks | 7 | 164 | 0.32 |
| PS_CascadeShadows_texture | shadows | 39 | 332 | 0.30 |
| PS_NoDir_PBR_MATERIAL_STANDARD | geometry and materials, others | 1 | 1 | 0.29 |
| CS_GatherRaymarchedLocalLightsClassified16 | lighting (deferred) | 1 | 1 | 0.26 |
| ? unnamed (DXIL 65c675622927212a) |  | 1 | 1 | 0.26 |
| CS_BatchCullMeshlets_ParaboloidShadows | shadows | 1 | 19 | 0.25 |
| CS_FinalGather_0_1_1_0 | ray tracing, probes, light cache | 1 | 1 | 0.24 |
| PS_LinearDepthOpaque | depth, velocity, masks | 4 | 134 | 0.24 |
| CS_GenerateShaderBinOffsets | ray tracing, probes, light cache | 1 | 7 | 0.22 |
| CS_ScreenProbes_TemporalResolve_ReflectiveCaustics | ray tracing, probes, light cache | 1 | 1 | 0.22 |
| VS_CascadeShadows_drawskinned | shadows | 3 | 16 | 0.21 |
| PS_DeferredPedTexturedNC | lighting (deferred) | 8 | 37 | 0.18 |
| CS_GenerateShadowMasks_Checkerboard_NoHair | hair | 1 | 1 | 0.18 |
| MS_VS_CascadeShadows_drawskinned | shadows | 3 | 105 | 0.17 |
| PS_Composite | composite of the lit picture | 1 | 1 | 0.16 |
| CS_PosOnlyLodSkinning | culling, skinning, upkeep | 1 | 7 | 0.15 |
| CS_Gather_Classified_1 | ray tracing, probes, light cache | 1 | 1 | 0.14 |
| CS_SkyRadianceCache | ray tracing, probes, light cache | 1 | 1 | 0.13 |
| CS_SkyRadianceCache_Camera_Gather | ray tracing, probes, light cache | 1 | 1 | 0.12 |
| VS_CascadeShadows_drawskinnedTexture | shadows | 7 | 80 | 0.11 |
| CS_ScreenProbes_RadianceSpatialFilter | ray tracing, probes, light cache | 1 | 1 | 0.10 |
| ? unnamed (DXIL 844fc95a66e7f23c e1a7a6c8f |  | 1 | 1 | 0.10 |
| CS_ComputeLuminanceBilateralGrid | post and picture | 1 | 1 | 0.10 |
| MS_VS_CascadeShadows_drawTexture | shadows | 9 | 179 | 0.10 |
| CS_GenerateMaterialMasks | depth, velocity, masks | 1 | 1 | 0.09 |
| ? compute 8f9eb68d0c6dd5cf |  | 1 | 11 | 0.08 |
| PS_Textured_Eight | geometry and materials, others | 1 | 6 | 0.08 |
| PS_HairStrandVBuffer | hair | 1 | 13 | 0.08 |
| CS_RayMiss | ray tracing, probes, light cache | 1 | 3 | 0.08 |
| CS_ScreenSpaceBentConesCheckerboard | ray tracing, probes, light cache | 1 | 1 | 0.08 |
| CS_InterpolateStrands | hair | 8 | 21 | 0.08 |

ten heaviest families 6.03 ms; families under 0.05 ms each: 4.29 ms in 841 families

What it says:

- The card has 24 ms of work in this frame of the world, and the title plays it at
  26-29 frames a second (35-38 ms): the card is not what holds the frame. What does is
  the main checkout's question (the thread that prepares, and the card standing idle
  between submissions).
- One title pass is worth a look by itself: `CS_ScreenProbes_ActivateRays`, 1.76 ms
  in one dispatch (the main checkout rests it 15 frames in 16 without ray tracing).
  After it `CS_TAA2_Resolve`, 0.74 ms.
- The layer's own passes are 1.75 ms of the 16.4 (argument clamps 0.76 in 157, copies
  to textures 0.64, compares of card buffers 0.34 in 430), and the unordered-access
  barriers 1.66 ms in 2,550 beside them: as much as any group of the title's passes
  but the probes.
- Nothing else is above 0.45 ms. 841 families are under 0.05 ms each and 4.3 ms
  together: no pass to group or replace by name would give a millisecond.

The two sections below are the timing runs as first read; keep them for the sums and
for which families a scene uses, not for a single command's cost.

## Without ray tracing (run pm1): 30.3 ms a frame in 1,767 kinds of command

| pass | families | commands a frame | ms a frame | share |
|---|---:|---:|---:|---:|
| ray tracing, probes, light cache | 120 | 1098 | 7.10 | 23.4 % |
| geometry and materials, others | 583 | 2047 | 6.75 | 22.3 % |
| lighting (deferred) | 72 | 1752 | 4.05 | 13.4 % |
| composite of the lit picture | 5 | 5 | 2.86 | 9.4 % |
| culling, streaming, upkeep | 57 | 215 | 2.19 | 7.2 % |
| shadows | 43 | 1631 | 2.18 | 7.2 % |
| depth, velocity, masks | 42 | 322 | 1.70 | 5.6 % |
| sky, fog, water, particles | 40 | 110 | 1.12 | 3.7 % |
| not named | 33 | 133 | 0.90 | 3.0 % |
| hair | 14 | 50 | 0.60 | 2.0 % |
| post and picture | 19 | 42 | 0.47 | 1.6 % |
| mirror reflections | 7 | 27 | 0.24 | 0.8 % |
| no program | 2 | 16 | 0.12 | 0.4 % |

| family | pass | programs | commands a frame | ms a frame |
|---|---|---:|---:|---:|
| CS_ScreenProbes_ActivateRays | ray tracing, probes, light cache | 1 | 1.0 | 2.66 |
| PS_CompositeIndirectDrawBlend | composite of the lit picture | 1 | 1.0 | 2.43 |
| PS_DeferredTextured | lighting (deferred) | 96 | 1041.1 | 1.43 |
| CS_GatherRaymarchedLocalLightsClassified16 | lighting (deferred) | 1 | 1.0 | 1.05 |
| CS_ScreenProbes_TemporalResolve_ReflectiveCaustics | ray tracing, probes, light cache | 1 | 1.0 | 0.72 |
| CS_BatchCullInitArgs | culling, streaming, upkeep | 1 | 50.3 | 0.71 |
| PS_draw_screen_space | geometry and materials, others | 1 | 74.1 | 0.62 |
| MS_VS_CascadeShadows_draw | shadows | 18 | 548.7 | 0.46 |
| PS_CascadeShadows_texture | shadows | 26 | 447.4 | 0.46 |
| PS_Textured_Eight | geometry and materials, others | 11 | 70.5 | 0.45 |
| CS_Gather_Classified_3 | ray tracing, probes, light cache | 1 | 1.0 | 0.45 |
| CS_FinalGather_0_1_1_0 | ray tracing, probes, light cache | 1 | 1.0 | 0.44 |
| PS_AlphaPrepass | depth, velocity, masks | 14 | 41.5 | 0.35 |
| CS_DistanceWindowEmissivesCulling | culling, streaming, upkeep | 1 | 5.3 | 0.30 |
| CS_WriteOutDeepOceanTexture | sky, fog, water, particles | 1 | 3.0 | 0.29 |
| PS_PropFoliageDeferredTo_GBuffer | lighting (deferred) | 4 | 101.9 | 0.28 |
| CS_BatchCullMeshlets_ParaboloidShadows | shadows | 1 | 12.6 | 0.27 |
| CS_Prop_BatchCull | culling, streaming, upkeep | 1 | 2.9 | 0.26 |
| PS_DeferredPedTexturedNC | lighting (deferred) | 11 | 38.3 | 0.26 |
| MS_VS_CascadeShadows_drawTexture | shadows | 11 | 249.2 | 0.24 |
| CS_GenerateShaderBinOffsets | ray tracing, probes, light cache | 1 | 7.0 | 0.23 |
| PS_Composite | composite of the lit picture | 1 | 1.0 | 0.23 |
| CS_GenerateShadowMasks_Checkerboard_NoHair | hair | 1 | 1.0 | 0.22 |
| VS_CascadeShadows_drawskinnedTexture | shadows | 7 | 124.8 | 0.22 |
| PS_LinearDepth | depth, velocity, masks | 28 | 95.2 | 0.22 |
| PS_TexturedDeferred | lighting (deferred) | 9 | 107.6 | 0.22 |
| PS_PBR_MATERIAL_STANDARD | geometry and materials, others | 1 | 1.0 | 0.21 |
| ? 48426bae580a080c |  | 1 | 0.9 | 0.21 |
| CS_ScreenProbes_RadianceSpatialFilter | ray tracing, probes, light cache | 1 | 1.0 | 0.21 |
| PS_NoDir_PBR_MATERIAL_STANDARD | geometry and materials, others | 1 | 1.0 | 0.21 |
| PS_LinearDepthOpaque | depth, velocity, masks | 19 | 112.7 | 0.19 |
| CS_SkyRadianceCache_Camera_Gather | ray tracing, probes, light cache | 1 | 1.0 | 0.17 |
| PS_DeferredTexturedStdNC | lighting (deferred) | 6 | 107.3 | 0.16 |
| CS_BatchCullMeshlets | culling, streaming, upkeep | 1 | 20.1 | 0.16 |
| MS_VS_Transform_PositionOnly | geometry and materials, others | 16 | 197.6 | 0.15 |
| PS_DeferredTextured_AlphaClip | lighting (deferred) | 20 | 140.4 | 0.15 |
| MS_VS_CascadeShadows_drawskinned | shadows | 5 | 85.8 | 0.15 |
| CS_ComputeLuminanceBilateralGrid | geometry and materials, others | 1 | 1.0 | 0.14 |
| ? 4518151afe8c6c09 |  | 1 | 21.0 | 0.13 |
| CS_RayMiss | ray tracing, probes, light cache | 1 | 3.0 | 0.13 |
| CS_GenerateMaterialMasks | depth, velocity, masks | 1 | 1.0 | 0.13 |
| CS_SkyRadianceCache | ray tracing, probes, light cache | 1 | 1.0 | 0.13 |
| CS_ApplyCombinedDeferred | composite of the lit picture | 1 | 1.0 | 0.13 |
| ? 8f9eb68d0c6dd5cf |  | 1 | 11.3 | 0.13 |
| (copy, no program) |  | 0 | 14.0 | 0.12 |
| CS_TemporalResolve2_Opaque_FrameStatistics | geometry and materials, others | 1 | 1.0 | 0.12 |
| ? 5ad3a82b1c3cc63a |  | 1 | 2.7 | 0.12 |
| CS_GenerateRayBinOffsets | ray tracing, probes, light cache | 2 | 4.0 | 0.11 |
| CS_CloudMultiScatteringCache_TimeSliced | sky, fog, water, particles | 1 | 1.0 | 0.11 |
| CS_Tree_BatchCull | culling, streaming, upkeep | 1 | 2.9 | 0.11 |
| CS_ScreenSpaceBentConesCheckerboard | ray tracing, probes, light cache | 1 | 1.0 | 0.10 |
| CS_CacheScreenSpaceRadiance | ray tracing, probes, light cache | 1 | 1.0 | 0.10 |
| CS_InterpolateStrands | hair | 10 | 21.0 | 0.10 |
| CS_UpdateParticles | sky, fog, water, particles | 1 | 30.0 | 0.10 |
| CS_InverseFFT_X | geometry and materials, others | 1 | 7.5 | 0.10 |
| PS_HairStrandVBuffer | hair | 1 | 12.0 | 0.10 |
| CS_ReconstructSurfaces | ray tracing, probes, light cache | 1 | 1.0 | 0.10 |
| CS_RaytracingResolve | ray tracing, probes, light cache | 275 | 933.2 | 0.10 |
| CS_ScreenProbe_RayMiss | ray tracing, probes, light cache | 1 | 1.0 | 0.09 |
| CS_InverseFFT_Y | geometry and materials, others | 1 | 7.5 | 0.09 |

What it says:

- No pass is the frame. The two heaviest single commands are one dispatch of
  `CS_ScreenProbes_ActivateRays` (2.7 ms) and one draw of
  `PS_CompositeIndirectDrawBlend` (2.4 ms); with
  `CS_GatherRaymarchedLocalLightsClassified16` (1.05 ms) and the screen probes'
  temporal resolve (0.7 ms) they are 6.9 ms in four commands. Those are the ones
  worth a look one by one (why one dispatch costs 2.7 ms here; whether it can rest
  some frames, which main does for the first since this run).
- The rest is a tail: the ten heaviest families are about 11 ms of the 30, and some
  1,750 kinds of command share the other 19, most under 0.1 ms. Grouping or
  replacing passes by name gains little there; what the tail costs is per command
  (5,280 indirect and 1,247 dispatches a frame), not per program.
- The screen probes and the light cache run without ray tracing too (5-7 ms).

## With ray tracing in every frame (run pm2, `VICTORY_TRACE_EVERY=1`): 52.5 ms

Not the same moment of the save (3,197 deferred draws against 1,041), so the
difference is not all the tracing's.

| pass | families | commands a frame | ms a frame | share |
|---|---:|---:|---:|---:|
| ray tracing, probes, light cache | 120 | 943 | 16.79 | 32.0 % |
| lighting (deferred) | 70 | 4815 | 10.51 | 20.0 % |
| geometry and materials, others | 582 | 2669 | 8.66 | 16.5 % |
| not named | 68 | 745 | 3.43 | 6.5 % |
| shadows | 43 | 2687 | 3.34 | 6.4 % |
| composite of the lit picture | 5 | 5 | 2.95 | 5.6 % |
| culling, streaming, upkeep | 56 | 206 | 2.36 | 4.5 % |
| depth, velocity, masks | 42 | 400 | 2.31 | 4.4 % |
| sky, fog, water, particles | 40 | 78 | 1.18 | 2.2 % |
| post and picture | 19 | 41 | 0.43 | 0.8 % |
| hair | 14 | 21 | 0.28 | 0.5 % |
| mirror reflections | 7 | 18 | 0.12 | 0.2 % |
| no program | 2 | 16 | 0.11 | 0.2 % |

| family | pass | programs | commands a frame | ms a frame |
|---|---|---:|---:|---:|
| CS_ScreenProbes_TraceRays_RayBinning_BVH_Mixed_ShaderBinning_WithFar | ray tracing, probes, light cache | 1 | 1.0 | 6.03 |
| PS_DeferredTextured | lighting (deferred) | 97 | 3197.4 | 5.25 |
| CS_ScreenProbes_ActivateRays | ray tracing, probes, light cache | 1 | 1.0 | 3.30 |
| PS_CompositeIndirectDrawBlend | composite of the lit picture | 1 | 1.0 | 2.48 |
| CS_RaytraceVBuffer_RayBinning_ShaderBinning_Mixed_WithFar | ray tracing, probes, light cache | 1 | 3.0 | 2.39 |
| ? caa78b8ca62bc3d6 |  | 1 | 155.2 | 1.24 |
| PS_PropFoliageDeferredTo_GBuffer | lighting (deferred) | 4 | 355.0 | 1.23 |
| PS_Textured_Eight | geometry and materials, others | 11 | 273.3 | 1.05 |
| CS_RaytracingResolve | ray tracing, probes, light cache | 271 | 778.6 | 0.97 |
| PS_AlphaPrepass | depth, velocity, masks | 15 | 141.4 | 0.95 |
| PS_CascadeShadows_texture | shadows | 24 | 641.7 | 0.87 |
| CS_Gather_Classified_19 | ray tracing, probes, light cache | 1 | 1.0 | 0.82 |
| CS_BatchCullInitArgs | culling, streaming, upkeep | 1 | 48.8 | 0.76 |
| PS_DeferredTextured_AlphaClip | lighting (deferred) | 19 | 397.6 | 0.70 |
| PS_TexturedDeferred | lighting (deferred) | 9 | 139.2 | 0.67 |
| PS_draw_screen_space | geometry and materials, others | 1 | 80.3 | 0.67 |
| PS_VehicleLiveryOffDetailMapsOffDeferred_NoDeform | lighting (deferred) | 4 | 206.0 | 0.63 |
| CS_GatherRaymarchedLocalLightsClassified16 | lighting (deferred) | 1 | 1.0 | 0.62 |
| MS_VS_CascadeShadows_draw | shadows | 18 | 765.5 | 0.61 |
| PS_DeferredTexturedStdNC | lighting (deferred) | 6 | 179.0 | 0.57 |
| MS_VS_CascadeShadows_drawTexture | shadows | 11 | 467.7 | 0.39 |
| ? 28918b7150a7aad5 |  | 1 | 65.8 | 0.38 |
| CS_Primary_GatherVBuffer_WithFar | geometry and materials, others | 1 | 1.0 | 0.37 |
| VS_CascadeShadows_drawskinnedTexture | shadows | 7 | 218.4 | 0.36 |
| ? 5552dac6a2b7a1fd |  | 1 | 10.7 | 0.31 |
| CS_Tree_BatchCull | culling, streaming, upkeep | 1 | 2.6 | 0.30 |
| MS_VS_CascadeShadows_drawskinned | shadows | 6 | 374.7 | 0.29 |
| CS_WriteOutDeepOceanTexture | sky, fog, water, particles | 1 | 3.0 | 0.29 |
| MS_VS_Transform_PositionOnly | geometry and materials, others | 16 | 370.7 | 0.29 |
| CS_FinalGather_0_1_1_0 | ray tracing, probes, light cache | 1 | 1.0 | 0.28 |
| ? ffd93c8a029ff016 |  | 1 | 1.9 | 0.28 |
| CS_BatchCullMeshlets_ParaboloidShadows | shadows | 1 | 14.1 | 0.26 |
| CS_Prop_BatchCull | culling, streaming, upkeep | 1 | 2.6 | 0.25 |
| PS_Composite | composite of the lit picture | 1 | 1.0 | 0.24 |
| PS_PBR_MATERIAL_STANDARD | geometry and materials, others | 1 | 1.0 | 0.24 |
| CS_GenerateShaderBinOffsets | ray tracing, probes, light cache | 1 | 7.0 | 0.24 |
| CS_ScreenProbes_RadianceSpatialFilter | ray tracing, probes, light cache | 1 | 1.0 | 0.21 |
| PS_LinearDepthOpaque | depth, velocity, masks | 19 | 135.2 | 0.21 |
| CS_GatherAlphaVBuffer_WithFar | geometry and materials, others | 1 | 1.0 | 0.20 |
| CS_IrradianceCache_GatherVBuffer_WithFar | ray tracing, probes, light cache | 1 | 1.0 | 0.20 |
| CS_ScreenProbes_TemporalResolve_ReflectiveCaustics | ray tracing, probes, light cache | 1 | 1.0 | 0.19 |
| CS_FinalGather_1_1_1_0 | ray tracing, probes, light cache | 1 | 1.0 | 0.18 |
| CS_GenerateMaterialMasks | depth, velocity, masks | 1 | 1.0 | 0.17 |
| PS_LinearDepth | depth, velocity, masks | 28 | 62.2 | 0.17 |
| CS_BatchCullMeshlets | culling, streaming, upkeep | 1 | 18.1 | 0.16 |
| PS_OctaVsplit | geometry and materials, others | 1 | 123.2 | 0.16 |
| PS_Octa | geometry and materials, others | 1 | 255.5 | 0.16 |
| ? 5ad3a82b1c3cc63a |  | 1 | 3.6 | 0.15 |
| ? 5279ae36796c31fc |  | 1 | 49.2 | 0.15 |
| CS_DistanceWindowEmissivesCulling | culling, streaming, upkeep | 1 | 3.1 | 0.15 |
| CS_ApplyCombinedDeferred | composite of the lit picture | 1 | 1.0 | 0.15 |
| PS_PuddleDetect | geometry and materials, others | 16 | 34.2 | 0.14 |
| CS_SkyRadianceCache | ray tracing, probes, light cache | 1 | 1.0 | 0.13 |
| CS_ComputeLuminanceBilateralGrid | geometry and materials, others | 1 | 1.0 | 0.13 |
| CS_Gather_Classified_3 | ray tracing, probes, light cache | 1 | 1.0 | 0.13 |
| ? 4518151afe8c6c09 |  | 1 | 16.0 | 0.13 |
| CS_Primary_Shade | geometry and materials, others | 1 | 2.0 | 0.13 |
| PS_VehicleLiveryOnDetailMapsOffDeferred_NoDeform | lighting (deferred) | 4 | 98.1 | 0.13 |
| PS_DeferredPedTexturedNC | lighting (deferred) | 11 | 18.0 | 0.13 |
| PS_Cable_Deferred | lighting (deferred) | 1 | 112.6 | 0.12 |

What it says: the trace programs proper are
`CS_ScreenProbes_TraceRays_RayBinning_BVH_Mixed_ShaderBinning_WithFar` (6.0 ms, one
dispatch) and `CS_RaytraceVBuffer_RayBinning_ShaderBinning_Mixed_WithFar` (2.4 ms in
three): 8.4 ms of the card in every frame, which the spaced profile
(`VICTORY_TRACE_EVERY=3-6`) spreads to 1.4-2.8. The played runs sv6 and sv7 showed 26
frames a second both ways in this room, so the card was not what held the frame
there; where it is (outdoors, driving), every frame will cost those 8 ms.
