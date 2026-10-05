## Context

See `proposal.md` for motivation and `specs/terrain-lighting-occlusion/spec.md`
for required behaviour. The relevant existing state:

- Terrain patches are streamed as cube-sphere LOD meshes and rebased into the
  rocket-local flight frame with a per-patch `body_to_inertial_at_spawn` and
  `render_origin_at_spawn` (`terrain/render.rs`). The authoritative height field
  lives in `TerrainSource` and the generated patch geometry; rendering never
  resamples terrain height.
- Rocket direct lighting is a fixed-inertial `DirectionalLight` (`SunLight`)
  oriented from `EphemerisSnapshot`; the rotating planet moves terrain through
  that fixed direction to produce the day/night cycle (`rocket/environment.rs`).
- `assets/shaders/terrain_surface.wgsl` already darkens crevices from the local
  normal (a texture-space heuristic) but has no real occlusion from surrounding
  height.
- Ocean and river water child entities carry `NotShadowCaster` and
  `NotShadowReceiver` (`terrain/render.rs`), so water currently cannot receive
  terrain shadow.
- The scientific-lighting change already owns the Sun, sky, terminator, and
  near-field directional-shadow cascades. This change must not duplicate any of
  that.

## Goals / Non-Goals

**Goals**

- Landscape-scale terrain self-shadow read as depth on hills, valleys, and
  ridges beyond the directional-shadow cascade range.
- Ambient/sky occlusion for crevices, concavities, and ground contact.
- Water participates in shadow receiving for both the shared shadow map and the
  landscape shadow.
- One occlusion owner that reuses the authoritative height field, patch
  identity, shared Sun, and render origin.
- Bounded, measurable, deterministic cost.

**Non-Goals**

- No second terrain source, height field, mesh, or terrain pixel pass.
- No change to the shared Sun, sky, exposure, or shadow-cascade work owned by
  the scientific-lighting change.
- No per-pixel real-time ray-traced or screen-space global illumination.
- No terrain, LOD, collision, streaming, render-origin, physics, guidance, or
  coordinate authority change.
- No new external dependency and no runtime data fetch.

## Decisions

### 1. Bake the self-shadow term rather than run a real-time terrain shadow pass

A heightfield ray-march toward the Sun produces a per-surface visibility term at
bake time. Real-time alternatives were rejected: a second shadow cascade cannot
cover landscape-scale distance at useful resolution within the near-flight
budget, and a per-pixel ray-march repeats the same work every frame for static
geometry. Baking also matches the existing fixed-sun convention: the Sun is
effectively fixed in the inertial frame while the body rotates, so occlusion is
a function of body-fixed terrain and the current Sun direction, not of frame
time.

### 2. Self-shadow scales only the direct-sun contribution

The baked visibility multiplies only the direct-sun term. Bevy's PBR already
separates directional-light and ambient/indirect terms, so the terrain material
path attenuates the directional contribution while leaving sky/ambient fill
intact. This preserves readable shaded slopes and avoids the black-shadow look
of scaling the whole shaded colour. Alternative (multiply final radiance) was
rejected because it also removes fill light and double-darkens with the sky
term.

### 3. Ambient/sky occlusion is a separate term applied to indirect light

Crevices and contacts need occlusion of the sky hemisphere, which is a different
integral from the directional ray toward the Sun. The bake computes a
hemisphere/ambient visibility term (a bounded set of directions or local horizon
estimate from the height field) and applies it to the sky/ambient contribution
only. This replaces the purely texture-derived crevice darkening with physical
occlusion while keeping the direct and indirect controls independent.

### 4. Reuse terrain patch identity, height field, and render origin

Occlusion is keyed by the existing `TerrainPatch` and generated patch geometry;
it is baked in the same body-fixed frame as the mesh, so it survives render-origin
rebasing without a second coordinate path. No new terrain representation is
introduced, and the rendered height field remains the one authoritative source.
The term is attached through the existing terrain material extension and patch
render state, and released with the patch on eviction.

### 5. Bake against the recorded Sun direction; refresh only on material change

The bake records the body-fixed Sun direction it used. Rotation alone does not
trigger a rebake because the Sun direction is fixed in the inertial frame;
regeneration of a patch or a material change in the Sun direction does. This
avoids per-frame bake churn. If a future epoch step changes the Sun direction
beyond a configured tolerance, the resident patches refresh against the new
direction through the existing patch lifecycle.

### 6. Water receives shadow without becoming a caster

Water child entities drop `NotShadowReceiver` so the shared directional shadow
map applies, and the water surface samples the same baked landscape occlusion
term at its position so terrain beyond the cascade range still shades it.
`NotShadowCaster` is retained: water is a thin blended cap and must not cast.
This keeps water presentation-only and unchanged in geometry.

### 7. Budgets and bake cost are evidence-gated

Both the ray-march sample count per bake and the occlusion sampling per fragment
are capped by configuration and reported through the existing
`TerrainPerformanceTelemetry`. Higher quality is opt-in only after profiling
shows headroom; the default stays within the existing streamed-terrain frame
budget. Alternative (always-on high sample counts) was rejected as
unmeasured cost that can stall patch uploads.

### 8. Determinism and precision

The bake is a pure function of patch identity, height-field parameters, seed,
and the recorded Sun direction; it uses the domain's `f64` terrain math and
stores `f32` presentation terms. It does not read camera pose, frame time,
wall-clock time, or unordered iteration, and it never writes simulation state.
This keeps the occlusion layer inside the presentation boundary and
reproducible for the same inputs.

## Risks / Trade-offs

- [Bake cost spikes patch activation] -> cap samples per bake, spread bake work
  through the existing upload/activation budget, and report cost in terrain
  telemetry before raising quality.
- [Occlusion seams across patch/LOD boundaries] -> bake in the shared body-fixed
  frame against the authoritative height field and use continuous sampling
  across patch edges; validate adjacent LODs.
- [Stale occlusion after a large Sun-direction change] -> record the bake Sun
  direction and refresh on material change through the patch lifecycle.
- [Water transparency interacts with received shadow and blending] -> retain
  `NotShadowCaster`, sample the landscape term separately from the shadow map,
  and validate shoreline and open-ocean cases.
- [Regression of the scientific-lighting shadows/sky] -> do not modify the Sun,
  sky, or cascade configuration; only consume the existing direction and shadow
  receiving, and re-run the lighting change's regression checks.

## Migration Plan

1. Land the bake and terrain-surface sampling behind the existing terrain
   material/plugin path with tests; verify static patches render unchanged when
   the term is neutral.
2. Enable water shadow receiving and landscape occlusion sampling, then validate
   shoreline, open ocean, and distant-hill cases.
3. Tune bounded defaults only against measured terrain performance telemetry.
4. Roll back by reverting the phase commit; no persisted or authoritative data
   changes, so no data migration is required.

## Open Questions

- The exact bake representation (vertex attribute versus per-patch texture) and
  ray-march step count can be finalized during implementation against measured
  bake cost and visible quality; both satisfy the same spec behaviour and do
  not change the approach or task breakdown.
