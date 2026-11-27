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
