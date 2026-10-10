## MODIFIED Requirements

### Requirement: Ground layers blend continuously from authoritative inputs

The system SHALL blend a bounded set of ground layers (grass, soil, rock, sand,
snow) using continuous weights derived only from authoritative terrain samples:
elevation, slope, moisture, and latitude zone. The blend MUST be continuous across
layer transitions and MUST NOT use hard biome bands. The wetness signal that drives
the biome ecotone MUST be representable at the resolution of the per-patch
layer-weight map, so that neighbouring patches at different LOD levels reconstruct
the same wetness at shared world directions under the map's filtering, not only at
coincident point samples.

#### Scenario: Slope exposes rock

- **WHEN** a terrain point's slope increases through the configured rock range
- **THEN** its blended material transitions smoothly from vegetation and soil
  toward rock without a visible hard edge

#### Scenario: Snow line

- **WHEN** a terrain point's elevation crosses the configured snow band
- **THEN** the blended material transitions continuously toward snow and returns to
  the lower-elevation layers below the band

#### Scenario: Adjacent LOD agreement

- **WHEN** two adjacent patches at different LOD levels share a boundary
- **THEN** their layer weights agree at shared source samples so the seam shows no
  material discontinuity

#### Scenario: Filtered LOD edge agreement

- **WHEN** two adjacent patches at different LOD levels share a boundary
- **THEN** the coarse patch's layer-weight map, reconstructed by bilinear filtering
  the way the GPU samples it, agrees with the fine patch's own weights at the same
  world direction, so the ecotone does not step where the LODs meet

#### Scenario: Deterministic weights

- **WHEN** the same source, seed, patch, and configuration are evaluated twice
- **THEN** the resulting layer weights are identical and independent of frame rate,
  spawn order, and cache history
