## Context

See proposal.md - Why. Water presentation already exists as a single shared
`WaterMaterial` (an `ExtendedMaterial<StandardMaterial, WaterExtension>`) backed
by `assets/shaders/water.wgsl`, spawned from
`src/infrastructure/bevy_adapters/terrain/render.rs`:
`water_mesh_for_patch` builds a flat sea-level sphere cap whose red vertex-colour
channel carries normalized depth, and the water child entities are currently
marked `NotShadowCaster` and `NotShadowReceiver`. Rivers are built by
`build_river_mesh` in
`src/infrastructure/bevy_adapters/terrain/surface/scatter.rs` as flat ribbons
that sample `TerrainSource::river_strength` per core grid vertex. The
authoritative hydrology/flow signal lives behind the `TerrainSource` boundary
(`surface_sample`, `river_strength`, and the erosion `flow_accumulation` bake).
The render-origin/body-to-inertial conversion, patch streaming lifecycle, and
material sharing are established and must be reused. Presentation must remain
separate from simulation (see the `planetary-rendering-presentation` skill) and
native-first with a WASM fallback.

## Goals / Non-Goals

**Goals:**
- Displaced ocean surface with analytic normals, physically plausible water
  shading, foam, shadow reception, and optional refraction.
- Flow-directed river channels aligned to the authoritative drainage network
  with discharge-scaled width and banks.
- Deterministic, presentation-only water that reuses the existing material,
  shader, mesh spawn, and render-origin paths.

**Non-Goals:**
- No water collision, buoyancy, radar altitude, drag, or physics coupling.
- No new hydrology/erosion computation in the water subsystem; the terrain
  authority stays the single source of drainage and discharge.
- No second water system, plugin, material family, or floating origin.
- No FFT/Tessendorf spectral ocean, compute pipeline, or GPU-driven tessellation
  in the first implementation.

## Decisions

### Decision: Gerstner displacement sum instead of FFT/Tessendorf cascades

Sum a bounded set of Gerstner waves in the vertex shader and derive the analytic
normal from the same sum. This is deterministic without a compute pipeline, works
on the existing native-first/WASM-fallback targets, and matches the existing
small-shader architecture. Alternative considered: Tessendorf FFT ocean
cascades, which give richer spectra but require GPU compute, large displacement
textures, and backend-dependent numeric behaviour; rejected for the first
implementation and left as a future option if evidence shows the bounded sum is
insufficient.

### Decision: Extend the existing WaterExtension / water.wgsl and spawn path

Add wave, absorption, scattering, foam, refraction, and shadow parameters to
`WaterParams`/`WaterExtension` and extend `water.wgsl`; keep one shared ocean
material and one shared river material. Extend `water_mesh_for_patch` to emit
displacement-capable subdivision rather than adding a parallel water renderer.
Alternative considered: a dedicated displaced-ocean plugin/material; rejected
because it duplicates the existing owner and violates reuse-first rules.

### Decision: Rivers consume the authoritative hydrology signal

River geometry reads drainage alignment and discharge from the existing
`TerrainSource` boundary (`surface_sample`/`river_strength`, and the baked
erosion flow accumulation) and derives channel width from that signal. Water
does not compute its own flow accumulation or drainage. `build_river_mesh` is
evolved to emit a channel with banks and a flow-directed surface, using the
authoritative per-vertex signal already sampled into the vertex colour channel.
Alternative considered: computing a render-time drainage field; rejected because
it would create a second, disagreing authority.

### Decision: Vertex-displacement subdivision and foam budgets are evidence-gated

Ocean and channel subdivision, Gerstner wave count, and foam coverage are
configuration values with conservative defaults. They are only raised when
profiling shows they are the bottleneck, per the project's measure-before-
optimizing rule. Screen-space refraction is configuration-gated and off by
default, with depth-based blending fallback on targets without the required
buffers.

### Decision: Water receives shadows but still does not cast them

Remove `NotShadowReceiver` from water entities so terrain shadows darken the
surface, while keeping `NotShadowCaster` so translucent blended water never
produces shadow artifacts. This is the minimum change that satisfies "receive
terrain shadows" without a new shadow pass.

## Risks / Trade-offs

- [Large flat caps need subdivision for visible displacement] → Drive
  subdivision from patch LOD/distance and a configurable budget; keep amplitude
  bounded so the undisplaced silhouette stays sealed against the coast.
- [WASM or restricted targets lack screen-space refraction buffers] → Gate
  refraction off by default and fall back to depth-based blending; never require
  it for correct output.
- [Blended water receiving shadows can show self-shadow artifacts] → Keep
  `NotShadowCaster`, receive only external (terrain) shadows, and validate with
  render tests.
- [Trigonometric wave sums can vary slightly across backends] → Keep wave
  frequencies/amplitudes as fixed constants, avoid accumulated cross-frame state,
  and drive phase only from the deterministic presentation clock.
- [Per-vertex displacement raises vertex cost on far patches] → Budget-gate
  subdivision and wave count; only spend geometry where displacement is visible.
- [Duplicating drainage logic would create a second authority] → Restrict the
  water subsystem to reading `TerrainSource`; no hydrology code in water.

## Migration Plan

1. Extend `WaterParams`/`WaterExtension` and `water.wgsl` in place, keeping
   existing defaults valid so `WaterExtension::default()` still compiles and
   renders.
2. Update `water_mesh_for_patch` to emit displacement-capable geometry and
   preserve the depth channel.
3. Update `build_river_mesh` to flow-directed channels; keep the existing
   `None`-when-dry contract and patch lifecycle.
4. Flip water entities to receive terrain shadows.
5. Add optional refraction behind configuration.
6. Rollback strategy: the change is presentation-only and per-feature
   configurable, so reverting the shader/material parameters restores the flat
   surface without touching simulation or streaming state.

## Open Questions

- Default Gerstner wave count/amplitudes and foam thresholds can be tuned once
  the shader is visible; they are configuration values and do not change the
  specs or task breakdown.
- Whether screen-space refraction ships enabled for this iteration or stays a
  follow-up is a configuration decision deferred to profiling.
- Subdivision budget thresholds depend on measured vertex cost and are deferred
  to profiling evidence.
