## Context

The launch pad is a presentation facility anchored on the authoritative terrain
surface. That surface is graded level across a 110 m radius at every Earth
launch site, so local `y = 0` is grade for the whole facility. The previous
trench was a solid 9 × 5 × 16 m box centred at `y = −2.55`: its top face sat
0.05 m *below* grade and the rest was inside the ground. The deflector's top
cleared the deck by 0.04 m, which is what the "buried box" looked like.

Three constraints shape the solution.

1. A height field cannot be cut by hiding geometry behind it. Any below-grade
   part is occluded by the terrain surface unless the terrain itself is lowered.
2. The vehicle rests on the terrain, and launch facilities carry no collision
   surface. Excavating under the launch point would drop the vehicle and move
   every flight baseline.
3. Lowering the *height field* is not enough, because the rendered mesh samples
   it. At the finest streamed LOD (610.8 m patches, 33 samples, 19.09 m between
   vertices) a 14 m trench receives at most one vertex, so the mesh spans it.
   Lowering alone produced a flat pad with a five-metre needle at the far corner.

## Goals

- The trench reads as an excavation in the pad from a chase camera.
- Every trench part rests on the excavation floor or spans floor to pad plane.
- The launch point keeps a grade-level footing and an undisturbed surface
  normal.
- One authority: the excavation is terrain, the lining and deflector are
  presentation.

## Non-Goals

- Modelling exhaust flow, deluge flow or flame impingement.
- A trench running under the vehicle. That needs a physical launch-mount
  surface, which is a separate change.
- Any change to collision, resting height, launch attitude or flight baselines.

## Decisions

### The rendered mesh needs a declared cutout

`TerrainSource` stays the single authority for height: collision, radar altitude
and physics sample the excavation exactly as before. Only the *mesh* is cut, and
it is cut where a structure says it covers the ground. `TerrainMeshCutout` is a
body-fixed spherical cap; the patch builder drops a cell only when all four of its
corners lie inside one. Two properties follow:

- the hole is a **subset** of the cap, so a structure that spans the cap always
  closes it;
- the hole is **smaller** than the cap by up to half a cell diagonal, so the
  structure must also guarantee that the reduced disc still exposes what it
  covers. At Earth's finest LOD that is `50 - 13.5 = 36.5 m`, comfortably past the
  trench's 28 m far edge. The measured hole at Starbase before this margin was
  fixed stopped at 27.5 m and left half a metre of grass in the opening.

Coarse patches are never cut, because no cell of a coarse patch fits inside a
50 m cap. A distant view therefore cannot punch a see-through hole in the planet.

The cutout is registered by the facility that fills it, not by the terrain
catalogue: a site with no hardstand keeps its ground.

### The excavation lives in the terrain authority

`PadTrenchZone` is applied in `LayeredTerrainSource::grade`, after the site
grade, so the cut is always the same depth below the levelled platform and the
rendered mesh, collision and radar altitude cannot disagree. Declared elevation
bounds subtract the deepest cut, so culling still contains the excavation.

### Rectangular, in the pad's own frame

The zone is a rectangle in the pad's local tangent plane (`+X` east, `+Z`
north), which is exactly the frame the facility meshes are built in. A circular
`PadFlatZone` could not express a trench, and expressing it in a different frame
would let the opening and the terrain drift apart.

### The mouth clears the launch point

`surface_normal` measures relief over a ±5 m stencil. With the 2.5 m blend, a
near edge closer than ~7.5 m makes the normal under the vehicle a slope measured
across the excavation edge. The near edge is therefore 9 m: it clears the
stencil with 1.5 m of margin, clears the hold-down clamps at 3.6 m, and leaves
the resting height and launch attitude untouched.

## Risks

- The excavation edge is a steep step in the height field. It is ~9 m from the
  vehicle, inside the finest streamed patch level, and faced with concrete
  lining; the visual evidence checks it.
- The lining rim used to top out at exactly grade, coplanar with the graded
  ground and the sea cap, so it z-fought and let slivers of grass through. It now
  meets the apron's top plane, and every rim part straddles its opening edge so
  no two faces are coplanar.
- Because the cut exposes the sea cap at one trench corner, a small water sliver
  remains there (measured at about 0.4 m in a 34 m nadir close-up). It is a site
  datum artefact of cutting ground that sits close to sea level, not a hole
  through the planet — the height diagnostic shows solid ground.
- A future pad whose vehicle or clamps extend past the mouth would need the near
  edge moved outward, and the spawned-scene test would fail first.
- The trench is a bounded pit rather than a duct to the pad edge. Recorded as a
  limit, not hidden.

## Migration

None. The change is additive: no saved state, no artifact schema and no
`SimulationRunIdentity` field moves.

## Open Questions

- Whether a later slice should give the launch mount a physical surface so the
  trench can pass under the vehicle. That is a collision-authority change and is
  deliberately out of scope here.
