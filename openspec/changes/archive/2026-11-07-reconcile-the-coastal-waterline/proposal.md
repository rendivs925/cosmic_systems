## Why

TM-03 asked whether the water mask, the geometric coastline and the imagery
agree at the same place. The water cap is the DEM-below-datum surface; the
imagery is a separate measured albedo product at its own resolution. A viewer
sees both at once, so a several-kilometre disagreement reads as water painted on
land or a blue strip inland of the shoreline.

The audit found no code defect; the datasets agree within their documented
resolution on a clean control coast. The missing piece was a transect that
records both crossings and the allowable tolerance.

## What Changes

- Add a control transect at the Great Australian Bight: open water is almost
  black and inland ground is bright, so the imagery waterline is the midpoint of
  the luminance ramp rather than a tuned colour threshold.
- Record the DEM crossing (height crosses the sea datum) and the imagery crossing
  and assert they are within one overview texel (0.044°).
- State the rule in `terrain-rendering`: the geometric waterline and the global
  albedo coastline agree within the imagery product's texel on the control coast.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terrain-rendering`: adds a coastal reconciliation requirement between the
  geometric waterline and the measured albedo.

## Impact

- `src/domain/services/terrain_source/catalog.rs`: one data-backed test.
- No production code, data, imagery, water, collision, or baseline change.
