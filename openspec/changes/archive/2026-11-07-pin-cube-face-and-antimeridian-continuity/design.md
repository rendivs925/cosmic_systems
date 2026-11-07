## Context

The cube sphere maps a direction to the face with the largest component and to
`(u, v)` on that face. Adjacent faces therefore store the same geographic edge
twice. `CubeSphereDem::sample_face_m` bilinearly interpolates the face grid;
if the two face grids disagreed on their shared edge, the same point would jump
where the face changes.

The global DEM is converted offline from one equirectangular raster, so both
faces sample the same source value at the shared edge. The test verifies that
this survives the conversion and the f32 storage.

## Goals / Non-Goals

**Goals:**

- Prove all twelve adjacent face pairs agree at their shared edge.
- Prove the antimeridian and poles are continuous in the runtime sampling path.
- Make the rule explicit so a future bake cannot break it silently.

**Non-Goals:**

- Change the cube-sphere topology or the DEM format.
- Re-test imagery; it already has antimeridian-wrap and pole-clamp tests.
- Prove continuity of the *rendered mesh* at LOD transitions; that is TD-07's
  recorded capture-path observation and a separate streaming audit.

## Decisions

### Decision: sample the DEM face API directly

`height_m(lat, lon)` routes through `face_uv`, which deterministically picks one
face, so it cannot exercise "the same point from the other side". The test
therefore derives `(u, v)` for both faces from the edge direction and calls
`CubeSphereDem::sample_face_m` on each. Across all twelve pairs and 65 steps per
edge, the difference is exactly 0 m.

### Decision: antimeridian tolerance from the probe

At a 1e-6° separation (about 0.1 m on the ground) the largest measured step was
4.8 mm. The test tolerance is 1 cm, which is above interpolation noise and far
below any real terrain feature.

### Decision: poles must collapse to one sample

The pole is a single point. The test samples both poles at every integer
longitude and requires the same height within 1 mm; the measured spread is
1e-12 m.

## Risks / Trade-offs

- **Large fixture.** The test reads the resident global DEM, like the existing
  source tests. A missing asset fails visibly instead of silently passing.
- **Imagery is separate.** The imagery tests already cover the antimeridian and
  latitude clamp; this change does not duplicate them.

## Migration Plan

None; tests and a specification only.

## Open Questions

None.
