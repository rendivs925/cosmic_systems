## Why

Close-range terrain and vegetation still break under zoom, motion, and LOD
transitions, and the working tree measures slower than the clean reference with
no isolated cause. The previous attempt reduced symptoms: it moved the vegetation
cutoff, enlarged distant trees, and blended body-fixed texture coordinates by
weight. Those workarounds introduced new correctness risks (fractional anchor
blending across projection axes, unaccounted coarse-vegetation memory, a frame
cache that can skip a required texture-phase update) and did not establish the
real frame-cost growth.

This change replaces the workarounds with correct, bounded behavior and makes
performance work follow a reproducible measurement instead of speculation.

## What Changes

- The terrain material frame update recomputes whenever any shader-relevant f64
  input changes, including the derived texture anchors, instead of comparing only
  f32 position and rotation.
- Body-fixed texture phase is covered by a test proving it is invariant under
  origin rebase and planetary rotation (the per-axis integer phase cancels under
  texture wrapping), so the anchor approach is verified rather than rewritten.
- Layer/imagery readiness keeps the geographic base continuous: layers modulate
  the base rather than replacing it, and a readiness transition cannot step the
  base color.
- Coarse-vegetation memory is included in the streaming patch estimate.
- Coarse vegetation is physically sized and budget-bounded; the enlargement hack
  and the hard level cutoff are removed, and refinement no longer drops cover to
  zero while a coarse tree representation still exists.
- Performance is re-baselined against a recoverable pre-repair state with fixed
  camera, epoch, resolution, and quality, separating startup from settled
  residency.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `planetary-terrain-material`: projection blending correctness, complete
  material-frame invalidation, and continuity across imagery/layer readiness.
- `vegetation-placement`: stable cross-LOD candidate identity, bounded distant
  representation, and complete streaming memory accounting.

## Impact

- `assets/shaders/terrain_surface.wgsl`: independent triplanar layer sampling.
- `src/infrastructure/bevy_adapters/terrain/render.rs`,
  `terrain/imagery.rs`: frame invalidation and material anchoring.
- `src/infrastructure/bevy_adapters/terrain/streaming.rs`: patch memory estimate.
- `src/infrastructure/bevy_adapters/terrain/surface/{mod,scatter}.rs`,
  `src/domain/services/vegetation.rs`: stable placement and representation.
- `docs/ground_tower_vegetation_repair.md`: measured results and limitations.

No change to authoritative physics, fixed timestep, reference frames, or the
normal/craft/rocket composition.
