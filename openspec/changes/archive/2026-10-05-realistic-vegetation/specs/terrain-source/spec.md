## ADDED Requirements

### Requirement: Climate vegetation density is the procedural fallback

The terrain source SHALL expose its deterministic climate-derived `vegetation_density` as the documented fallback signal for vegetation when no measured land-cover package is available, computed from climate inputs such as moisture, latitude, and altitude rather than from rendered color.

#### Scenario: Fallback drives placement without a package

- **WHEN** no measured land-cover package is loaded
- **THEN** vegetation placement uses the source's climate-derived density

#### Scenario: Fallback is deterministic and climate-based

- **WHEN** the same coordinate is sampled twice
- **THEN** the fallback density is identical, and it tracks climate rather than the rendered albedo

### Requirement: Measured land cover is a presentation-only overlay

The system SHALL allow a measured land-cover package to override the fallback density and species mix for vegetation presentation only. The terrain source's sampled height, collision surface, and physics authority MUST NOT depend on measured land cover.

#### Scenario: Overlay does not change terrain authority

- **WHEN** a measured land-cover package replaces the fallback for a coordinate
- **THEN** the source's height and collision samples for that coordinate are unchanged

#### Scenario: Fallback remains available

- **WHEN** the measured package is absent or out of coverage
- **THEN** the source continues to supply its climate-derived vegetation density
