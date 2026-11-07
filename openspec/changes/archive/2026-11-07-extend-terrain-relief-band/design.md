## Context

`LayeredTerrainSource::mesh_height_m` samples the base, the optional macro layer,
and one bounded detail octave weighted by `DetailLodFade::new(11, 14)`. The detail
source is ~250 m ridges; at LOD 10 a patch is ~10 km wide with ~300 m vertex
spacing, so those ridges alias. The fade removes them, leaving a flat mesh from
roughly 5 km outward.

The measured ETOPO base still carries continental relief, but a coastal plain —
the launch-site case — has almost none. TM-02 therefore adds a presentation band
whose feature size matches the coarse LOD, not more high-frequency detail.

## Goals / Non-Goals

**Goals:**

- Restore visible relief at 5–50 km on low-relief ground.
- Keep collision, radar, launch anchors and physics bit-identical.
- Keep the coastline identical at every LOD.
- Keep patch count, memory and batch cost inside the existing budget.

**Non-Goals:**

- Replace the measured base elevation or the erosion field.
- Add per-patch materials, normals or shadow changes.
- Change the fine detail octave or its fade.

## Decisions

### Decision: mesh-only, not physical

The fine detail is part of `height_m`; adding a 4 km ±30 m field there would move
the launch-site anchor and therefore the physical launch altitude. The far band
is added only in `mesh_height_m`. Collision is only sampled near the vehicle,
where patches are fine and the band is zero, so the rendered/collision difference
is confined to distant ground that physics never queries.

### Decision: band-pass, not a one-sided fade

A plain `1 - fade(8, 11)` would also apply at LOD 0–4, where a 4 km feature is
sub-pixel and would alias the whole face. Two fades multiply:
`rise(5, 8) × (1 - fall(9, 11))`: absent at planet scale, full at LOD 8, absent
again once the fine octave is full.

### Decision: land-only weighting

The band is multiplied by the same sea-guard smoothstep used for the detail
(full above 60 m over the datum, zero at the datum). This keeps the water cap and
the coastline independent of LOD: a coarse patch cannot raise a shallow seabed
above the datum or pull a coast below it.

### Decision: reuse the launch-site attenuation

`ProceduralFarDetailSource` takes the same `PadFlatZone` list as the fine detail,
so the band fades out over prepared platforms before the site grade is applied.

## Risks / Trade-offs

- **Rendered/collision difference in the far field.** Up to ±30 m at LOD 5–9.
  Physics never samples there; the coastline is protected separately.
- **One extra sampler per vertex.** Only on patches in the band; the streaming
  batch time and residency budget are measured in the evidence.
- **Synthetic relief.** It is bounded presentation relief and is labelled as
  such; it does not claim to be measured terrain.

## Migration Plan

Removing `.with_far_detail(...)` restores the previous mesh. No data changes.

## Open Questions

None.
