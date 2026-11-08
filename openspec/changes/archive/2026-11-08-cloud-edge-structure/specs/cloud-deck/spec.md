## ADDED Requirements

### Requirement: Cloud edges are a resolved ramp

The coverage band SHALL transition from clear air to full coverage through a
smoothstep whose lower quarter is the soft edge, so the boundary is a resolved
ramp rather than a linear cut through sub-pixel rank noise.

The endpoints and the mean of the band SHALL be preserved, so the measured
fraction still selects the same share of the sky and the clear-air threshold does
not move.

#### Scenario: Boundary at a coarse view

- **WHEN** the deck is drawn at 400-1,200 km
- **THEN** its boundary is a continuous ramp, not per-pixel notches

### Requirement: The march filters to the coarser of step and pixel footprint

The volumetric march SHALL sample the density field at the coarser of the level
its ray step can resolve and the level the pixel footprint covers. The footprint
SHALL be derived from the view projection, not from a derivative inside the
divergent march, and the filter SHALL cost no additional texture fetch.

The footprint level SHALL be zero while one pixel covers less than one volume
texel, so the near deck is not blurred.

#### Scenario: Distant nadir deck

- **WHEN** one pixel covers several volume texels
- **THEN** the field is sampled from a coarser mip that averages them

#### Scenario: Near deck

- **WHEN** one pixel covers less than one texel
- **THEN** the footprint level is zero and the finest mip is used
