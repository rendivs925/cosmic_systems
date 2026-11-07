## Context

Two independent measured products define the coast:

- **Elevation**: ETOPO 2022 at 30 arc-seconds (≈900 m) and regional payloads at
  finer resolution. The water cap uses `height <= sea datum`.
- **Albedo**: the global 8192×4096 overview (≈4.4 km per texel) plus regional
  imagery. Its coastline is where the water colour gives way to land.

A viewer sees the water mesh and the albedo texture together. The acceptance is
therefore about their relative position, not about either dataset alone.

## Goals / Non-Goals

**Goals:**

- Provide one deterministic control transect with a stated method.
- Record both crossings and the one-texel tolerance.
- Document that the albedo product sets the tolerance.

**Non-Goals:**

- Re-georeference or re-resample either dataset.
- Change the water cap, the albedo blend, or the material.
- Force agreement on coasts where one dataset is known to be coarse (rias,
  barrier islands, tidal flats); the control point is deliberately clean.

## Decisions

### Decision: the Great Australian Bight at 32°S

The coast there is a straight cliff line: open water is almost black and inland
ground is bright, so the luminance ramp is unambiguous. Namibia's coast was
rejected as a control because the 30 arc-second DEM smears its rias and shelf,
and the KSC coast has a barrier island and lagoon that make a single crossing
meaningless.

### Decision: luminance midpoint, not a colour threshold

The test takes the median luminance of the first and last twenty samples, uses
their midpoint, and finds the first sample above it. This is objective and
reproducible; a fixed blue threshold was fragile because the overview's coastal
water turns green near the shore.

### Decision: one albedo texel

`360 / 8192 = 0.04395°`. The measured offset is smaller than this, so the test
does not claim sub-pixel agreement between a 4.4 km albedo and a 0.9 km DEM.

## Risks / Trade-offs

- **One control coast.** It proves the pipelines agree where both products are
  clean; it does not certify every coastline. The test is a reconciliation gate,
  not a global survey.
- **Large fixture.** The test reads the 8192×4096 overview and the global DEM,
  like the other Earth tests.

## Migration Plan

None; test and specification only.

## Open Questions

None.
