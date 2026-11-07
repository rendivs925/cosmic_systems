## ADDED Requirements

### Requirement: Elevation is continuous across cube-face seams and poles

The terrain authority SHALL sample one height per geographic direction. A point
on the shared edge of two cube faces SHALL return the same height through either
face. Longitudes either side of the antimeridian SHALL agree within the local
interpolation tolerance, and every longitude at a pole SHALL collapse to the
same polar sample.

#### Scenario: Adjacent face seam

- **WHEN** the same great-circle point on a shared cube-face edge is sampled
  through either face
- **THEN** both samples return the same stored height

#### Scenario: Antimeridian

- **WHEN** two points separated by a sub-metre step across ±180° are sampled
- **THEN** their heights differ by no more than the interpolation tolerance

#### Scenario: Polar clamp

- **WHEN** the pole is sampled at different longitudes
- **THEN** every longitude returns the same height
