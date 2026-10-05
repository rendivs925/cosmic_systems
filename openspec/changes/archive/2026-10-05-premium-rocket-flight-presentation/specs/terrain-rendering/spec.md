## ADDED Requirements

### Requirement: Surface microdetail remains presentation-only and source-derived
Terrain rendering SHALL derive microdetail appearance from the authoritative terrain source and prepared surface data while keeping geometry, streaming, collision, and LOD authority unchanged.

#### Scenario: Near terrain material detail
- **WHEN** a sufficiently detailed terrain patch is rendered near the flight camera
- **THEN** its material can provide bounded local color, normal, and roughness variation derived from prepared source data

#### Scenario: Patch eviction
- **WHEN** a terrain patch is evicted from the streaming cache
- **THEN** all presentation-only surface assets associated with that patch are released with the patch and no terrain authority data is changed

### Requirement: Papua surface presentation remains offline and deterministic
The Papua launch-region terrain presentation SHALL derive its tropical biome, vegetation, and material variation from existing offline terrain samples and deterministic patch inputs. It MUST NOT fetch terrain, imagery, land-cover, or vegetation data at runtime.

#### Scenario: Reproducible Papua patch presentation
- **WHEN** a Papua-region patch is prepared repeatedly with the same terrain source and configuration
- **THEN** its presentation data is equivalent regardless of preparation order or runtime network availability
