## Why

CL-05: the deck read as one repeated puff. Coverage, vertical type and wind
streets were already real, but the horizontal cell size came from a single
Perlin-Worley octave, so every cloud in a regime had the same footprint no
matter how the type morph changed its height.

## What Changes

- Add a cell-size spectrum inside `sample_density`: re-weight the base volume's
  Worley octaves (6/12/24 cells across the tile) to move the dominant cell size
  continuously between roughly 3 km and 0.75 km on the Earth deck.
- Select it with a scalar that is regional plus per-cell: the weather map's
  smooth alpha channel biases a whole system, and the coarse Worley channel makes
  neighbouring cells differ.
- Keep the existing carve as the selector-zero end, so the current field and its
  coverage anchor are unchanged and the spectrum only adds finer structure.

## Capabilities

### New Capabilities

- `cloud-deck`: the volumetric deck's density, coverage, morphology and cell
  size.

### Modified Capabilities

None.

## Impact

- `assets/shaders/cloud_density.wgsl`: the size spectrum in `sample_density`.
- `src/domain/services/cloud_noise.rs`: the weather map's alpha channel is now
  documented as the smooth size selector.
- `src/infrastructure/bevy_adapters/terrain/render.rs`: a shader guard.

## Constraints

- No extra texture fetch: the selector is built from channels already in the
  weather and base samples, and the shader still has exactly three
  `textureSampleLevel` calls.
- No coordinate warp. An earlier per-sample shear produced swirls and was
  rejected; this changes the carve amplitude only.
- Seamless: both inputs are periodic and the blend is pointwise.
