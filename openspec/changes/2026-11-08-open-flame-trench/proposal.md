## Why

LF-03 asked for a flame trench that reads as cut into the pad. Reading the
spawned geometry showed the trench was never visible at all: the 36 m deck slab
spanned the whole footprint, and — the part the roadmap entry did not record —
the authoritative terrain surface is level at grade across the whole 110 m pad,
so every trench and deflector vertex sat *behind* the terrain. Opening the deck
alone would have exposed flat ground, not a trench.

The deck is not the only occluder. The terrain grade is.

## What Changes

- Cut a rectangular excavation into the graded pad in the terrain authority, so
  the mesh, collision and radar altitude all see the same trench.
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
  trench and its lining.

## Impact

- `src/domain/services/terrain_source/procedural.rs`: `PadTrenchZone` and the
  shared trench dimensions.
- `src/domain/services/terrain_source/layered.rs`: applies excavations after the
  site grade and extends the declared elevation bounds.
- `src/domain/services/terrain_source/catalog.rs`: one excavation per graded
  Earth pad, from the same site list the grade uses.
- `src/application/rocket_spawning.rs`: apron/deck frames, trench lining,
  deflector, one spawned-scene test.
- Presentation and site grading only. The launch point keeps its grade-level
  footing, so the resting height, launch attitude, deterministic flight
  baselines and the recorded terrain metrics are unchanged.
