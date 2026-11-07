## Context

`LayeredTerrainSource::height_m` applies a `SiteGrade`: inside the flat radius
the height is the site anchor, restored over the blend. The appearance pipeline
samples `height_m` for:
- the surface-map elevation grid (geometry and elevation bands),
- the local normal, whose tilt becomes `slope_deg`,
- `slope_deg_at`, a 2 km central-difference probe used by scatter and layers.

The grade therefore changes the classified slope of the footprint. On a site
below the 35° rock threshold the difference is small; on a steep site it turns
rock into grass over the whole grade.

## Goals / Non-Goals

**Goals:**

- Classify material from the natural surface while keeping geometry graded.
- Keep elevation bands tied to the surface actually shown.
- Cover both a gentle and a steep synthetic grade, with a control proving the
  old behaviour would recolour the pad.

**Non-Goals:**

- Change the grade, its footprint, or the collision height.
- Change scatter density policy at launch sites; the shared slope helper now
  sees the natural slope, which is the intended consistency.
- Add a new terrain layer or cache.

## Decisions

### Decision: an appearance-only height on the source trait

`appearance_height_m` defaults to `height_m`, so every existing source is
unchanged. Only `LayeredTerrainSource` overrides it, returning the composition
before `grade`. This keeps the rule with the authority that applies the grade
rather than adding a flag to every caller.

### Decision: slope follows appearance, elevation stays shown

The surface-map grid keeps `h = height_m` for positions and for the shoreline /
snow bands. A second grid `appearance_h` feeds only the normal used for
`slope_deg`. A pad that is graded from +0.8 m to 0 m still reads as wet shore,
because that is the surface drawn; it does not acquire the hillside's rock
classification it no longer physically has.

### Decision: the shared slope helper uses the same rule

`slope_deg_at` is used by scatter and layer weighting. Sampling it from
`appearance_height_m` keeps those consumers consistent with the per-vertex
material and prevents a second classification path from disagreeing.

## Risks / Trade-offs

- **One extra height grid per surface map.** The ungraded path is the same
  composition without the final grade blend, so the cost is close to `height_m`.
  Surface maps are generated off the fixed step on worker tasks.
- **Two height notions.** The trait documents that `appearance_height_m` must
  never drive collision or physics; the name and the doc comment make the
  boundary explicit.
- **Gentle sites look unchanged.** Vandenberg's pad is already continuous; the
  steep synthetic test is the regression that matters.

## Migration Plan

None; presentation-only. Removing the override restores the old classification.

## Open Questions

None.
