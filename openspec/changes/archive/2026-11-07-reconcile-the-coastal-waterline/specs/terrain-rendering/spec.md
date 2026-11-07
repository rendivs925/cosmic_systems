## ADDED Requirements

### Requirement: The geometric waterline agrees with the albedo coast

The rendered geometric waterline (terrain at or below the sea datum) and the
measured albedo coastline SHALL be reconciled at a fixed coastal control
transect: their crossings SHALL lie within one texel of the albedo product. The
comparison SHALL use a stated, non-tuned method rather than a colour threshold
chosen per frame.

#### Scenario: Control transect

- **WHEN** a transect crosses a clean coast from open water to inland ground
- **THEN** the DEM sea-datum crossing and the albedo luminance-ramp midpoint
  agree within one overview texel

#### Scenario: Datum consistency

- **WHEN** the three coastal authorities (DEM classification, water cap, albedo)
  are compared
- **THEN** they use the same sea datum and do not reclassify measured land

#### Scenario: Resolution limit

- **WHEN** the albedo product is coarser than the elevation source
- **THEN** the tolerance is the albedo texel, not sub-pixel agreement, and the
  limitation is documented rather than hidden
