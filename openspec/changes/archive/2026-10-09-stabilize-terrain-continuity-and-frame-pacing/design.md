## Context

See `proposal.md - Why`. The terrain material extension already carries a small,
precise render-relative body-fixed position and per-scale fractional anchors
computed in f64 (`body_texture_anchor`). Vegetation is generated inside
`super::surface` scatter code from patch-seeded candidates in
`domain/services/vegetation.rs`, and the streaming estimate lives in
`streaming.rs::estimated_patch_bytes`. The authoritative rotation and origin are
available from `EphemerisSnapshot` and `RenderOrigin`.

## Goals / Non-Goals

**Goals:**
- Make body-fixed ground sampling correct under blending, rebase, and rotation.
- Make material refresh complete and idempotent.
- Make vegetation identity independent of terrain LOD and bounded in cost.
- Account for every generated representation in the streaming budget.
- Produce a reproducible performance baseline.

**Non-Goals:**
- Changing terrain height, collision, or simulation authority.
- Adding a new coordinate system, profiler, task pool, or cache owner.
- Fixing shadow unproven causes without captures.

## Decisions

### Keep coordinate blending, verify phase invariance

`body_layer_uv` weights axis-plane coordinates (including fractional anchors)
into one UV. Analysis shows this is already rebase- and rotation-invariant: the
anchored coordinate `local_i * scale + frac(body_origin_i * scale)` equals the
true absolute body-fixed coordinate modulo the integer part, which texture
wrapping discards, and the render-relative `local_i` and the anchor origin cancel
under a rebase. A full independent-axis blend would sample the layer arrays up to
three times per fragment for no proven correctness gain.
- Rationale: remove the suspected defect only after proving it exists.
- Alternative rejected: sample-blend the axes. Costly (layer height + albedo +
  normal samples tripled) without a demonstrated phase error.
- Alternative rejected: drop anchors and use absolute f32 positions. Reintroduces
  planet-scale precision loss, which is the real defect.

### Invalidate on the full derived frame

`update_terrain_material_origin` will build the complete frame value (f64 origin
phase plus rotation) and compare the exact derived anchors, not an f32 reduction.
- Rationale: equal f32 position/rotation does not imply equal f64 anchor phase.
- Alternative rejected: hash the f64 origin/rotation. More code for no benefit;
  the anchor comparison is already the minimal shader-relevant value.

### Physically sized coarse vegetation within a bounded budget

Remove `coarse_vegetation_scale` enlargement. Coarse tree-only patches place
trees at their true species size up to a fixed budget, so distant geometry is
cheaper instead of larger. A true cross-LOD stable candidate identity would need
a single global lattice; iterating that lattice is not affordable, so identity
stays per-patch and the limitation is documented rather than faked.
- Rationale: removes the arbitrary enlargement and cutoff while keeping the
  existing deterministic per-patch contract.
- Alternative rejected: a global body-fixed candidate lattice. Correct identity,
  but iterating it at fine patch resolution is unbounded work.
- Alternative rejected: cache a parent's generated vegetation and reuse on
  children. Ties identity to cache lifetime and complicates eviction.

### Residency accounting follows generation

`estimated_patch_bytes` will include the coarse tree-only representation by
computing the same budget the scatter path uses, so estimate and generation agree.

## Risks / Trade-offs

- [Triplanar layers cost up to 3x layer samples] -> Profile; early-out when the
  layer weight is zero; keep a single-sample path when one axis dominates.
- [Stable-cell selection changes existing forests] -> Accept; determinism and
  cross-LOD identity are the requirement. Keep the same seed and ecological rules.
- [Projected-size hysteresis can flicker] -> Use a size band and require the
  change to persist before switching representation.
- [Frame invalidation every frame reintroduces churn] -> Keep the direct
  comparison; only true changes refresh, and anchors change only on rebase or
  rotation, not on a stationary 1x scene.

## Migration Plan

1. Land projection/frame fixes behind existing material code; verify with the
   existing tests plus new rebase/rotation tests.
2. Land stable-cell placement; update scatter and streaming together so the
   estimate always matches generation.
3. Remove the enlargement/cutoff workaround last, after coverage tests pass.
4. Re-baseline and optimize only measured bottlenecks.

Rollback is per-step: each step is independently revertible and leaves the app
building and running in all three modes.
