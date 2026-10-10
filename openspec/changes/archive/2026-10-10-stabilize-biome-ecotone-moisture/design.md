## Context

The layered ground material derives a per-patch 32x32 RGBA layer-weight map from
authoritative `TerrainSource` samples (`height_m`, `slope_deg_at`, `moisture`,
`zone_lat`). At level 11 a texel is roughly 150 m. The composed Earth source's
`moisture` blended the base procedural fBm with the detail source's ~100-200 m
drainage statistic at weight 0.65, so the fine drainage term dominated the biome
ecotone. The map point-samples that term; the GPU bilinearly filters it. Adjacent
LODs sample different points of the same fine signal, so their filtered maps
disagree and the grass/sand ecotone steps along shared patch edges.

See proposal.md — Why. `moisture` is presentation-only: no collision, altitude,
streaming, or physics consumer reads it.

## Goals / Non-Goals

**Goals:**
- Remove the straight ecotone step at shared patch edges across LODs.
- Keep biome variation broad and natural (grass, soil, sand all still appear).
- Keep the wetness deterministic, normalized, and continuous on the sphere.

**Non-Goals:**
- Changing terrain height, erosion, or the drainage that shapes geometry.
- Changing the weight-map resolution, budget, or packing.
- Replacing the base procedural moisture signal (it already spans the continent
  scale and was not the aliasing term).

## Decisions

- **Lower the detail moisture frequency rather than raise the weight-map
  resolution.** The biome ecotone is a landscape-scale concept; a ~2 km base
  octave is representable by every LOD's 32x32 map (≈13 texels at level 11),
  whereas matching a ~200 m signal would need far more texels per patch and grow
  residency for every patch. Alternatives considered: a larger weight map (cost),
  nearest sampling (blocky), and forcing mip 0 (no visible change).
- **Sample an fBm field on the unit sphere** (`direction * MOISTURE_NOISE_SCALE`),
  reusing the existing `ValueNoise`, so the field is seamless across cube faces
  and deterministic per seed, matching the rest of the procedural source.
- **Leave geometric drainage untouched.** `drainage_strength_for_direction` still
  drives detail troughs and height; only the presentation wetness consumer changed.
- **Test the filtered reconstruction, not just point samples.** The regression
  builds each parent/child pair at both real launch sites and compares the fine
  texel against the coarse map sampled bilinearly, which is what the shader sees.

## Risks / Trade-offs

- [Ground reads flatter/more uniform green] → Keep the scale high enough for broad
  variation (measured max adjacent-step and moisture span bounded in tests) and
  confirm visually at the fixed prelaunch and orbital cameras.
- [Other moisture consumers change behaviour] → `moisture` is documented
  presentation-only; vegetation fallback density also reads it, so confirm the
  launch-site vegetation still appears in captures/tests.
- [Tests overfit the chosen constant] → The continuity test is mutation-checked:
  restoring the drainage frequency makes it fail, so it guards the mechanism, not
  the value.
