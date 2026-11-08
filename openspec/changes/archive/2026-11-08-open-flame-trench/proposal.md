## Why

LF-03 asked for a flame trench that reads as cut into the pad. Reading the
spawned geometry showed the trench was never visible at all: the 36 m deck slab
spanned the whole footprint, and — the part the roadmap entry did not record —
the authoritative terrain surface is level at grade across the whole 110 m pad,
so every trench and deflector vertex sat *behind* the terrain. Opening the deck
alone would have exposed flat ground, not a trench.

The deck is not the only occluder. The terrain grade is — and once that was
fixed, a third occluder appeared: **the streamed mesh cannot resolve the
excavation at all.** The finest Earth patch is 610.8 m across at 33 samples, i.e.
19.09 m between vertices, so a 14 m trench is sampled by at most one vertex and
the mesh interpolates straight back to grade. Measured at the real Starbase
patch: level 13 lands **0** vertices inside the trench, level 14 lands exactly
**1**. A below-grade feature can only be seen once the *mesh* stops spanning it.

## What Changes

- Cut a rectangular excavation into the graded pad in the terrain authority, so
  collision and radar altitude see the trench.
- Add a terrain-mesh cutout capability: a structure declares the cap it covers,
  and the mesh drops only the cells that lie entirely inside it. The launch
  hardstand registers one, so the rendered ground stops at the structure and the
  excavation is visible. The height field is untouched.
- Split the apron and deck into frames around the trench mouth.
- Line the excavation with a floor and four walls, and place the steel deflector
  on the trench floor, below the pad plane.
- Keep the near edge clear of the launch point so the terrain normal under the
  vehicle is not measured across the excavation edge.
- Add domain tests for the cut and the composition, and a spawned-scene test for
  the opening and the lining.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `launch-site-presentation`: adds an open-excavation requirement for the flame
  trench and its lining, and a covering-structure requirement for the mesh
  cutout.

## Impact

- `src/domain/services/terrain_source/procedural.rs`: `PadTrenchZone` and the
  shared trench dimensions.
- `src/domain/services/terrain_source/layered.rs`: applies excavations after the
  site grade and extends the declared elevation bounds.
- `src/domain/services/terrain_source/catalog.rs`: one excavation per graded
  Earth pad, from the same site list the grade uses.
- `src/domain/services/cube_sphere/{mesh,mod}.rs`: `TerrainMeshCutout` and the
  cutout-aware patch builder.
- `src/infrastructure/bevy_adapters/terrain/{cutouts,streaming,render}.rs`: the
  cutout resource and its passage into the bake request.
- `src/application/rocket_spawning.rs`: 110 m hardstand with overlapping frames,
  trench lining, deflector, cutout registration, one spawned-scene test.
- Presentation and site grading only. The launch point keeps its grade-level
  footing, so the resting height, launch attitude, deterministic flight
  baselines and the recorded terrain metrics are unchanged.
