# Terrain & World Realism Roadmap

Status: active plan. Source requirements:
`docs/technical_modules/1..6.md`,
`docs/procedural_3d_tree_engine_technical_specification.md`,
`docs/technical_specification_photorealistic_procedural_3d_cloud_engine.md`.

This plan adapts those specifications to the repository's Bevy/wgpu
architecture. The specifications target OpenGL compute and Three.js; their
algorithmic requirements are authoritative, their embedded GLSL/JS bodies are
reference only.

## 1. Objective And Scope

Make the visible world realistic — ground, hills, mountains, lakes, rivers,
trees, vegetation, clouds, atmosphere, and the existing fire/entry effects —
while preserving the authoritative simulation.

In scope: the eight documents above.

Out of scope: the tree document's standalone Three.js application prompt
(`docs/plans/attitude-regression-and-simulator-quality-plan.md:262-263`), and
OpenSpec artifacts (this roadmap is a plan document, not a change).

## 2. Architecture And Non-Negotiables

- **Hybrid rendering.** All domain fields — elevation, erosion, hydrology,
  vegetation placement, and the Module 3 density field — are pure CPU Rust,
  seeded and testable. GPU work is fragment-shader presentation (ray-marching,
  parallax occlusion, wave shading) through the existing custom
  `Material`/`ExtendedMaterial` pattern. No custom compute render-graph nodes
  are introduced by this plan.
- **One authority per quantity.** The base surface stays
  `TerrainSource::height_m` and is authoritative for collision, radar altitude,
  guidance, and physics. Erosion, hydrology, the Module 3 density field,
  appearance, and scatter are deterministic functions *of* that authority, not
  competing sources. This is documented at each extension point.
- **Precision.** `f64`/`DVec3` in the domain and body-fixed coordinates; `f32`
  meshes rebased through the existing `RenderOrigin`. No second floating origin,
  no new coordinate system.
- **No fake physics.** Effects and materials never mutate physical state.
- **Evidence-gated.** Every phase starts with captures and profiling and ends
  with the shared validation matrix. No phase is complete on `cargo check`
  alone.
- **Reuse before invention.** Extend the existing owners listed in section 3;
  do not create parallel terrain, erosion, scatter, cloud, or atmosphere systems.
- **Adapt, don't copy.** GLSL bodies in the specs are recipes, not code to port
  verbatim; keep the repository's engine patterns and units.

## 3. Doc To Code Reuse Map

| Doc | Existing capability (reuse) | Gap to close |
|---|---|---|
| Module 1 — Macro landform | `terrain_source/procedural.rs`: continental mask, domain warp, ridged mountains, craters | Spline continental remap, self-attenuating ridge, octave tuning; applies to procedural/detail layers (Earth base is measured) |
| Module 2 — Hydrology/thermal | `erosion/simulate.rs`: thermal + hydraulic droplets + D8 flow + river carve; `erosion/store.rs`; bake tool | Substrate hardness `K_hard`, Eulerian shallow-water fill for endorheic basins (lakes), exposed flow/sediment/hardness fields |
| Module 3 — Volumetric geology | none; `cube_sphere/mesh.rs` is heightmap geometry | 3D density field + Dual Contouring + Transvoxel-style LOD stitching (presentation-first) |
| Module 4 — Ground micro-detail | `surface/layers.rs` (5 layers, `height_aware_weights`, layer height array), `surface/surface_maps.rs`, `terrain_surface.wgsl` | Gravel/Mud layers, flow/sediment-driven weights, parallax occlusion + self-shadow |
| Module 5 — Ecological scatter | `vegetation.rs` (lattice/species/clump/budgets), `surface/scatter.rs`, `land_cover.rs` | Land-cover class to species, per-species spacing, multi-tier canopy, SSS/wind, alpha-tested shadows, impostors |
| Tree spec | `scatter.rs` continuous ring-extruded branches, canopy normal bending | Buttress roots, volumetric cluster lobes, translucent cutout foliage, GPU wind, procedural bark |
| Cloud spec | `rendering/materials.rs` `CloudExtension`, `clouds.wgsl` shell, `rocket/planet.rs` flight shell | Volumetric ray-march, Perlin-Worley 3D noise, weather map, multi-scatter/powder/triple-HG, curl wind |
| Module 6 — Atmosphere/water/lighting | `sky.wgsl` single-scatter, `atmospheric_optics.rs`, `environment.rs` aerial fog, `water.wgsl`/`water.rs`, Bevy clustered forward+ | Bruneton LUTs, volumetric fog, unified terrain in-scatter, FFT/JONSWAP water |
| Fire/entry (no doc) | `rocket/effects.rs`, `atmospheric_effects.rs`, `lifecycle_effects.rs`, `ground_presentation.rs` | Noise/textured plume, smoke/contrail, shock diamonds, RCS, velocity-aligned plasma sheath/trail, ablation trail |

## 4. Phased Roadmap

Each phase lists its doc source, reuse, changes, tests, acceptance, and risk.

### Phase 1 — Ground micro-detail (Module 4) — first

- Reuse: `surface/layers.rs`, `surface/surface_maps.rs`, `terrain_surface.wgsl`,
  `TerrainSurfaceExtension`, baked terrain occlusion.
- Changes:
  - Add **Gravel** and **Mud** ground layers (7 total) with descriptors and
    procedural texture entries.
  - Expose erosion flow/sediment/hardness from `HeightRaster` through
    `TerrainSurfaceSample`; drive `layer_weights` from elevation, slope,
    moisture, flow, sediment, and latitude (Module 4 section 2).
  - Add **parallax occlusion mapping with tangent-space self-shadow** over the
    layer height array, plus POM uniforms (height scale, min/max steps,
    contrast).
  - Cover the close-range displacement role (the spec's hardware tessellation)
    with near-focus resolution / near-detail overlay, since Bevy 0.17 has no
    tessellation pipeline.
- Tests: pure `layer_weights` unit tests including gravel/mud; a POM
  UV-invariance regression guarding the earlier triplanar-anchor flicker class;
  existing layer tests unchanged.
- Acceptance: no material popping; believable mud/gravel/rock transitions at
  close range; `render/main_opaque_pass_3d` GPU within budget; all suites and
  the three modes green.
- Risk: POM cost at grazing angles — adaptive step count and quality gating.

### Phase 2 — Macro landform and hydrology (Modules 1-2)

- Reuse: `terrain_source/procedural.rs`, `terrain_source/layered.rs`,
  `erosion/simulate.rs`, `erosion/store.rs`, `erosion/mod.rs`, `terrain/water.rs`.
- Changes: Module 1 spline continental remap and self-attenuating ridge in the
  procedural/detail source; Module 2 substrate hardness and an Eulerian
  shallow-water pass that fills endorheic basins into **lakes**; expose a
  lake/water-level signal on `TerrainSource`; extend water rendering to inland
  water and add river width attributes.
- Tests: deterministic hydrology tests (basin fill, conservation); elevation
  continuity; lake detection on synthetic basins.
- Acceptance: lakes and rivers visible in the launch region; no terrain/collision
  disagreement; no streaming spikes.
- Risk: procedural changes must not perturb Earth's measured base; lakes are a
  new water surface path.

### Phase 3 — Volumetric geology (Module 3): full 3D density + Dual Contouring

- Architecture (presentation-first, authority-preserving):
  - Pure domain field `D(p) = H_eroded(x,y) - z + Omega(p)` with slope mask,
    sedimentary strata/hardness, Voronoi jointing, and 3D domain warp;
    deterministic and seeded, a function of the authoritative heightmap.
  - Extracted per cube-sphere patch neighborhood via **Dual Contouring** (QEF),
    streamed like terrain; LOD transitions via Transvoxel-style transition
    cells (skirt fallback).
  - Collision stays on the base heightmap in this phase (documented limitation,
    same class as current visual grounding). Overhang collision is a follow-up.
  - Rendering reuses terrain streaming, budgets, and `RenderOrigin`.
- Tests: field determinism and isovalue continuity; sharp-edge preservation vs
  marching cubes on synthetic cells; crack-free transition-cell assertions;
  bounded generation cost.
- Acceptance: cliff overhangs, undercuts, and arches render and stream at LOD;
  no cracks at transitions; base collision unchanged.
- Risk: highest-risk phase (new geometry path, LOD stitching, budget pressure);
  sequenced after Phases 1-2 with its own validation.
- Status: **implemented, gated off by default** (`COSMIC_SYSTEMS_VOLUMETRIC_GEOLOGY=1`).
  `domain/services/volumetric_geology/` owns the deterministic density field
  (`D(p)=H(x,y)-z+Ω(p)` with slope mask, tilted strata/hardness, domain-warped
  overhang noise, Voronoi jointing) and Dual Contouring (QEF). The streaming
  layer builds a bounded rock shell for steep patches at or above a configured
  LOD level and the render layer spawns it as a presentation-only child of the
  heightmap patch; collision, altitude, guidance, and physics keep sampling
  `heightmap`. Shell activity is reported through `TerrainPerformanceTelemetry`
  (`volumetric_shells`, `volumetric_shell_triangles`).
  Documented limitations (next phase work): shells only trigger on high-LOD
  (small, near) patches, which at the launch region only occur in flight and
  require steep terrain; the shell is an *overlapping* surface (a small
  `surface_bias_m` avoids coplanar z-fighting) rather than a replacement, so
  patch seams and LOD-transition cells (Transvoxel) are not yet handled; a
  real-cliff capture is still pending (the render path was validated
  synthetically).
  Resolution: Module 3 overhang cavities are only a few metres deep, so the
  shell footprint is decoupled from the patch size (`footprint_m`, default
  90 m) and sampled at `cell_m` (default 1.5 m); covering a whole LOD-14 patch
  at metre resolution would exceed any sample budget. The 3D perturbation fades
  to zero over the outer footprint (`edge_taper_fraction`, default 0.35) so the
  shell meets the surrounding heightmap without a hard boundary wall.
  Regression tests assert the field produces overhang cavities
  (`default_field_produces_overhang_cavities_on_a_cliff`), that the extracted
  mesh contains genuine downward-facing (overhanging) normals at streaming
  resolution (`shell_at_streaming_resolution_contains_overhangs`), and that the
  edge taper reaches zero at the footprint boundary
  (`edge_taper_fades_omega_to_zero_at_the_footprint_boundary`).

### Phase 4 — Trees and vegetation (Module 5 + tree spec)

- Reuse: `vegetation.rs`, `scatter.rs`, `land_cover.rs`, merged-mesh material.
- Changes: land-cover class to species; enforce per-species spacing; buttress
  roots; volumetric cluster-lobe normals; a dedicated vegetation material with
  subsurface translucency and GPU vertex wind; alpha-tested shadow depth;
  procedural bark/foliage textures; far-field impostor LOD; fix the
  shadow-marker inconsistency on the vegetation child entity.
- Tests: species-from-class selection; spacing enforcement; wind deforms
  identically in shadow/depth passes; deterministic generation; conservative
  animated bounds.
- Acceptance: no disconnected near-field branches; believable canopy volume; no
  LOD popping; bounded patch batching preserved.
- Status: started. Land-cover class now decides the vegetation *form* in
  `select_species` (water/snow/built-up host nothing; open classes never host
  trees; a measured tree-cover class selects the best tree for the climate and
  is not downgraded to ground cover by the climate density threshold).
  Per-species `min_spacing_m` needs no placement filter: the tree lattice cell
  (~61 m at `TREE_SITE_LEVEL` on Earth) already exceeds the largest minimum, and
  a patch-local filter would break the cross-LOD nesting guarantee; a regression
  test pins this (`lattice_spacing_exceeds_per_species_minimum`). Remaining:
  buttress roots, canopy-lobe normals, the dedicated material (translucency,
  GPU wind, alpha-tested shadows), procedural bark/foliage, impostor LOD, and
  the vegetation shadow-marker fix.

### Phase 5 — Volumetric clouds (cloud spec)

- Reuse: `rendering/materials.rs` `CloudExtension`/`CloudMaterial`,
  `rocket/planet.rs` flight shell, `environment.rs`.
- Changes: fragment ray-marched volumetric clouds; startup-generated
  Perlin-Worley 3D noise `Image` assets (CPU, seeded); 2D weather map;
  triple-lobe Henyey-Greenstein with multi-scatter and powder; blue-noise
  jitter and adaptive/empty-space steps; curl-noise wind advection.
- Tests: noise determinism; density/height-profile units; weather-map decode.
- Acceptance: natural multilayer structure from ground to orbit; no whole-field
  per-frame regeneration; GPU within budget (quarter-res/temporal reprojection
  only if profiling demands it).

### Phase 6 — Atmosphere and water (Module 6)

- Reuse: `sky.wgsl`, `rocket/sky.rs`, `environment.rs`, `water.wgsl`,
  `water.rs`, `atmospheric_optics.rs`, Bevy clustered forward+.
- Changes: Bruneton transmittance and sky-view LUTs; unified terrain aerial
  perspective (feed optics uniforms into `terrain_surface.wgsl`, replacing the
  `DistanceFog` approximation); volumetric fog and light shafts; FFT/JONSWAP
  water (or extended multi-component Gerstner) with Snell refraction and depth.
- Tests: LUT vs analytic single-scatter; continuity at 10/30/70 km and orbit.
- Acceptance: continuous appearance with no blue plate or oversized shell;
  water visually physical within budget.

### Phase 7 — Fire and entry effects (existing only)

- Reuse: `rocket/effects.rs`, `atmospheric_effects.rs`, `lifecycle_effects.rs`,
  `ground_presentation.rs`.
- Changes: noise/texture-driven plume with smoke/contrail and shock diamonds;
  RCS VFX; a velocity-aligned plasma sheath/trail plus ablation trail and shock
  light. No new dependency.
- Acceptance: smooth sea-level-to-vacuum plume evolution; correct nozzle
  attachment through staging; no physics mutation or unbounded resource growth.

## 5. Cross-Cutting Requirements

- **Validation matrix** per phase: `cargo fmt --check`,
  `cargo check/clippy/test` for `dem` and `--no-default-features`, a release
  build, native flight plus solar/craft/rocket smoke runs, and
  `openspec validate --all --strict`. Report matched before/after p50/p95/p99,
  GPU passes, and any unavailable metric explicitly.
- **Assets.** All new textures and noise volumes are generated deterministically
  at startup or by a small offline binary following the existing erosion-bake
  pattern. No asset paths scattered through systems.
- **Instrumentation.** Extend `RenderDiagnosticsPlugin` and terrain telemetry
  before optimizing; no speculative optimization.
- **Commits.** One validated commit per completed phase, staging only intended
  files.

## 6. Open Risks

- Module 3 is a genuine new geometry path with budget and complexity risk;
  isolated to its own phase.
- Bevy lacks hardware tessellation; Module 4 uses near-field subdivision and
  parallax occlusion instead (documented deviation from the spec).
- Volumetric clouds and fog are GPU-heavy against the current un-attributed
  ~13-15 ms frame gap; each is quality-gated and profiled before adoption.
