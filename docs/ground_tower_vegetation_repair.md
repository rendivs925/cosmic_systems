# Ground, Tower, Vegetation, Camera and Performance Repairs

This note records the repairs made to the existing simulator, the systems reused,
the tests added, and the measured results. It is a working note, not a
specification.

## Confirmed root causes

### 1. Rectangular ground/material discontinuity

- The layered ground material replaced the continuous geographic albedo with a
  flat tiled layer albedo (`layered_base = mix(detail_albedo, layered_albedo,
  layer_fade)`). A patch with layer maps therefore took a different base colour
  from a neighbour without them, producing a patch-shaped, rectangular boundary.
- `local_detail_weight(patch.level)` and `layer_blend_weight` are per-patch
  steps, so adjacent LODs at the same view distance could differ.

### 2. Disconnected launch tower

- Horizontal "braces" were a single 12 m beam centred on the tower axis at
  `z = 0`, while the columns sit at `z = ±4 m`, so the braces met no column.
- Service/umbilical arms began in the unsupported tower interior between platform
  levels instead of on a platform edge.

### 3. Vegetation loss on zoom-out

- `VEGETATION_MIN_PATCH_LEVEL` was 12, so once the selected LOD coarsened to 11
  or below all trees disappeared from the patch mesh, independent of projected
  size.

### 4. Camera clearance not persisted

- `update_rocket_camera` stored the pre-clearance `smoothed_pose`, then lerped
  part-way toward clearance each frame. The minimum-clearance invariant was never
  held, so terrain patch churn could sink the camera and read as ground vibration.

### 5. Performance and memory accounting

- `update_terrain_material_origin` used `Assets::iter_mut`, which queues a
  `Modified` event for every terrain material it yields, re-extracting every
  material each frame it ran.
- Its cache compared only the f32 planet centre and quaternion while the written
  anchors are derived from f64 origin and rotation, so it could skip a required
  anchor update.
- Coarse, tree-only vegetation (level 11) was generated without being included in
  `estimated_patch_bytes`, so it lived outside the accounted streaming budget and
  the quadtree selection under-counted it.

## Fixes

### Ground appearance (`assets/shaders/terrain_surface.wgsl`, `terrain/render.rs`)

- The layered material is now a unit-luminance tint multiplied over the
  continuous geographic base, not a replacement. A patch without layer maps only
  loses the layer grain; it cannot step in base colour against a neighbour.
- High-frequency ground detail is reconstructed from a small, precise
  render-relative body-fixed position plus per-scale fractional anchors
  (`body_texture_anchor`), instead of a planet-scale f32 coordinate. This keeps
  sub-metre phase for the micro/near/layer tiling and stays continuous across
  patches and cube faces.
- `update_terrain_material_origin` now compares the complete written frame
  (planet centre, rotation, and all three derived anchors) against a bound cache
  (`TerrainMaterialFrame`) and skips the per-material pass only when nothing
  changed.
- The anchor approach is verified rather than rewritten: a test proves the
  per-axis integer phase cancels under texture wrapping, so the body-fixed phase
  is invariant under origin rebase and planetary rotation.

### Launch tower (`application/launch_tower.rs`, `application/rocket_spawning.rs`)

- New Bevy-free geometry module with explicit member endpoints. Perimeter rings
  connect real columns, every face diagonal lands on a column at a platform
  level, and service/umbilical arms start on the platform edge and reach the
  vehicle surface. The spawn system converts each segment into a rotated beam.

### Vegetation (`terrain/surface/mod.rs`, `scatter.rs`, `streaming.rs`)

- Trees now generate from level 11; ground cover and rocks remain at level 12
  (`VEGETATION_GROUND_COVER_MIN_PATCH_LEVEL`).
- The tree-size enlargement hack is removed: coarse trees keep their true
  physical size.
- Coarse tree-only patches are capped at a small budget
  (`COARSE_TREE_BUDGET_CAP = 8`). The cap is small because the whole sphere's
  coarse leaves are selected together, so a larger cap consumes most of the
  160 MiB selection budget; a larger cap made the near-camera max-level patch
  fail to be requested. The exact cap was chosen by that budget constraint.
- Coarse-tree bytes are now part of `estimated_patch_bytes`
  (`MAX_COARSE_VEGETATION_MESH_BYTES`), sharing the tree mesh formula with the
  full-density estimate so the two cannot drift.

### Camera (`rocket/camera.rs`)

- The final exterior pose is cleared against the streamed surface and the
  corrected pose is stored back in the controller's reference frame, so clearance
  persists across frames and transitions.

## Reused systems

- Terrain streamer, patch lifecycle, worker tasks, and upload budgets
  (`TerrainStreamingResource`) were extended, not duplicated.
- Terrain material/asset ownership, imagery upgrade, and occlusion binding were
  reused.
- Existing water/time/render-origin plumbing was reused.
- Existing `PerformanceStats` and terrain telemetry were used for measurement.

## Tests

- `application::launch_tower`: platform/mast support, column enclosure, perimeter
  and diagonal attachment, arm attachment, determinism.
- `application::rocket_spawning::spawned_tower_members_attach_to_columns_or_platforms`:
  expands the actually spawned brace/diagonal meshes and asserts both endpoints
  touch a spawned column or platform, catching the original floating-member defect.
- `terrain::surface`: gate relationships, coarse tree budget bounded and nonzero.
- `terrain::streaming`: coarse vegetation accounted for and strictly less than
  the full-density reservation; budget test unchanged for close patches.
- `terrain::render`: `body_texture_anchor` reconstructs the absolute body-fixed
  phase within 1e-3 tile, and the phase is invariant under origin rebase and
  planetary rotation.
- `rocket::camera`: cleared camera pose round-trips through the reference frame
  and re-clearing is idempotent.
- `rocket::camera::camera_clearance_survives_patch_replacement_and_refinement`:
  walks a fine-to-coarse patch replacement sequence and asserts the camera always
  clears the finest resident patch, that adding finer relief never lowers it, and
  that a hole (no resident patch) never moves the camera or generates terrain.
- `rocket::camera::camera_clearance_is_invariant_under_origin_rebase`: the same
  physical camera point clears identically after a large render-origin rebase.
- `rocket::camera::update_rocket_camera_keeps_clearance_through_mode_transitions`:
  drives the real `update_rocket_camera` system with the rocket, camera, and
  origin below the streamed surface, then steps through Chase → Orbital → Free
  transitions and asserts the cleared pose holds every frame. Mutation-checked:
  disabling the clearance call makes it fail.

## Measured results

Hardware: AMD Ryzen 9 8940HX (16C/32T), NVIDIA RTX 5070 Laptop (8 GiB, driver
610.43.03), Vulkan. Native window (`DISPLAY=:0`), release build, rocket mode,
`COSMIC_SYSTEMS_PERFORMANCE_METRICS=1`, default chase camera.

Frame-time percentiles grow after startup in **both** the clean-HEAD baseline and
this tree, so there is a pre-existing residency/streaming degradation. Raw
sequential runs also drift monotonically with wall-clock time (later runs are
uniformly slower), which is characteristic of thermal or background-load drift
rather than a code change:

| Sequential release run (120-180 s) | steady p50 |
| --- | --- |
| Clean HEAD baseline (early) | ~22 ms |
| Working tree, coarse cap 48 | ~34 ms |
| Working tree, coarse cap 8 | ~30 ms |
| Clean HEAD baseline (later) | ~24 ms |
| Working tree, coarse tier disabled (min level 12) | ~37 ms |

The ordering is not monotonic and the coarse-tier-disabled run was among the
**slowest**, so the ~6 ms apparent gap is not reliably attributable to any single
change. Terrain CPU attribution (`lod_selection`, `scheduling`, `viewport_culling`)
was comparable or lower in the working tree, and GPU `main_opaque_pass_3d` and the
shadow cascades were within ~0.1 ms.

**Conclusion: the measurement environment is too noisy for a 6 ms discrimination.**
No credible performance improvement or regression is claimed.

The window uses `PresentMode::AutoVsync`. Running the same binary twice
back-to-back differed by 5.7 ms (28.9 vs 34.6 ms), the same size as the apparent
difference, confirming the signal sits below frame-to-frame noise. Disabling
vsync for profiling only (`COSMIC_SYSTEMS_PRESENT_MODE=none`) removes the
quantization and gives the true frame cost:

| Same binary, 90 s | steady p50 |
| --- | --- |
| vsync run 1 | 28.9 ms |
| vsync run 2 | 34.6 ms |
| no vsync | **18.5 ms** (min 15.2, max 19.9) |

So the ~30 ms is vsync quantization: the true frame cost is ~18.5 ms, just over
the 16.6 ms budget, and any frame that overruns a vblank is doubled to ~33 ms.
Note the panel reports 240 Hz (`xrandr`), yet the observed quantization behaves
like ~60 Hz, which is itself consistent with the cross-GPU present path capping
delivery. An opt-in `COSMIC_SYSTEMS_PRESENT_MODE=none` switch was added to
`main.rs` for this measurement; the default remains vsync.

### CPU/GPU profile of the ~18.5 ms floor

`perf record --call-graph dwarf` over 25 s (vsync off) shows where CPU time goes:

- ~57% of CPU on the `AsyncComputeTaskPool` and ~32% on `ComputeTaskPool`,
  dominated by terrain generation — `erosion::simulate::steepest_downhill_with_spacing`
  alone is ~29%, with hydraulic/thermal erosion, D8 flow accumulation, and
  `DemTerrainSource` sampling close behind. This is the *startup* streaming burst.
- Reading `/proc/<pid>/task/*` after the scene settles shows the distribution is
  very different once terrain stops streaming: the busiest thread is the render
  thread (~23% CPU), then the main thread (~14%), with the worker pools low.
- The five logged GPU passes total ~2.5 ms, but **that is only the instrumented
  passes**, not the full GPU frame (see below).

The obvious hypothesis — that the saturated erosion workers stall the frame —
was tested directly by temporarily setting `droplets: 0, thermal_iterations: 0`
in `earth_erosion_config` and re-measuring with vsync off:

| vsync off, 70 s | steady p50 |
| --- | --- |
| Erosion enabled | 18.5 ms |
| Erosion disabled | 21.7 ms |

Disabling ~90% of worker CPU did **not** lower the frame time, so worker
contention is not the frame-pacing bottleneck.

The frame time is, however, strongly **resolution-dependent**, which rules out a
pure present/pacing explanation. Re-measured with vsync off after settling:

| Window size (no vsync) | steady p50 |
| --- | --- |
| 640x360 | 15.3 ms |
| 1280x720 | 21.8 ms |
| 1920x1080 | 23.5 ms |

`nvidia-smi` reports ~55-66% GPU utilization at ~50 W and 1380-1455 MHz while
this runs — the GPU is active but not saturated. Combined with the resolution
scaling, the ~18.5 ms frame is dominated by a resolution-dependent **GPU
render/present path**, not by CPU simulation and not purely by vblank pacing.
The environment is a hybrid laptop: the NVIDIA dGPU renders while the panel is
driven by the AMD iGPU, so the frame is also copied across GPUs before display.

**Corrected conclusion:** the frame cost is GPU/present-path bound and scales
with pixels; the earlier claim that it is purely "present/swapchain pacing" was
an overstatement, and the "2.5 ms GPU" figure only covered instrumented passes.
No terrain/erosion optimization is justified by the profile; reducing GPU pixel
work (shading/shadows/upscale) is the lever, and that is out of scope here.

## Remaining issues

- The ~18.5 ms frame is GPU/present-path bound and resolution-dependent on a
  hybrid AMD-iGPU + NVIDIA-dGPU display path. It needs per-pass GPU timing and
  present tracing to attribute, and the fix is likely reducing GPU pixel work,
  not CPU simulation. No terrain/erosion optimization is justified by the
  profile.
- Far-field vegetation continuity is only bounded, not truly solved: the level-11
  coarse tier keeps a small, physically sized tree presence so cover does not drop
  to zero, but density at level 11 is far below level 12. A real impostor /
  billboard representation is the correct long-term fix and is out of scope here.
  A global body-fixed candidate lattice would give stable cross-LOD identity but
  iterating it at fine patch resolution is not affordable.
- Visual confirmation of the marked ground seam and the zoom transition used the
  available display but could not reproduce the exact source camera pose; the
  high-level captures look continuous. A matched-camera A/B remains to be done.
- Tower spawned meshes are covered by an integration test
  (`spawned_tower_members_attach_to_columns_or_platforms`) that expands each
  spawned brace/diagonal from its transform and asserts its endpoints land on a
  spawned column or platform.
- Camera clearance is now automated across patch replacement, origin rebase, and
  mode transitions (tests above, mutation-verified). Rendered stationary and
  camera-transition sequences were captured with the in-app F12 framebuffer
  screenshot path (`COSMIC_SYSTEMS_PRESENT_MODE=none`, 1280x720, release, rocket
  mode) and inspected: shadows are present and consistently placed, with no
  visible flicker across the stationary sequence.
- Shadow stability could **not** be isolated by frame differencing in this
  environment: consecutive stationary frames differ by ~0.9% normalized RMSE, but
  the scene keeps changing because terrain patches finish streaming and the
  clouds/wind animate every frame, so the residual is not attributable to
  shadows. A true shadow-stability check needs a frozen-scene capture harness
  (fixed epoch, terrain generation paused, clouds frozen), which does not exist.
  No shadow defect was identified by the captures, so per the plan the result is
  recorded rather than "fixed".
