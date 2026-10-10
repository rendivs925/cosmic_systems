## Why

A straight grass/sand boundary appeared on the launch-site ground. The biome
wetness that drives the layered material was the ~100-200 m drainage statistic,
which the 32x32 per-patch weight map point-samples. At level 11 a texel is ~150 m,
so the map undersamples that signal and neighbouring LODs filter the alias
differently, stepping the ecotone in a straight line across shared patch edges.
Point-sample agreement at shared edges does not catch this, because the GPU
bilinearly filters each map.

## What Changes

- The detail source's presentation wetness becomes a low-frequency fractal field
  sampled on the unit sphere (`MOISTURE_NOISE_SCALE = 3000.0`, base octave ~2 km on
  Earth) instead of the ~200 m drainage statistic, so every LOD's weight map can
  represent it and neighbouring LODs reconstruct the same value at a shared world
  direction.
- Geometric drainage is unchanged: it still shapes terrain height and detail
  troughs. Only the presentation biome signal changes, so collision and physics are
  unaffected.
- Regression tests build the composed Earth weight maps at both real launch sites
  (Papua and KSC), assert no single coarse texel step is a hard band, and compare
  each parent/child pair against the bilinearly reconstructed coarse map.

## Capabilities

### New Capabilities

<!-- none -->

### Modified Capabilities

- `terrain-material-splat`: the "Ground layers blend continuously from authoritative
  inputs" requirement gains a filtered-LOD-edge scenario, so agreeing at shared
  point samples is no longer sufficient — the wetness source must be representable
  at the weight-map resolution so bilinear reconstruction agrees across LODs.

## Impact

- `src/domain/services/terrain_source/procedural.rs`
  (`ProceduralDetailSource::moisture`, `MOISTURE_NOISE_SCALE`).
- Presentation-only: layered material weight maps and their consumers. Terrain
  height, erosion, collision, and streaming budgets are unchanged.
- `src/infrastructure/bevy_adapters/terrain/surface/layers.rs` regression tests.
