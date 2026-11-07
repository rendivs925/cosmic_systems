## 1. Terrain excavation

- [x] 1.1 Add `PadTrenchZone` and the shared trench dimensions to the terrain
      source domain.
- [x] 1.2 Apply excavations in `LayeredTerrainSource::grade` after the site grade
      and extend the declared elevation bounds.
- [x] 1.3 Build one excavation per graded Earth pad from the same site list the
      grade uses.

## 2. Facility geometry

- [x] 2.1 Split the apron and deck into frames around the trench mouth.
- [x] 2.2 Line the excavation with a floor and four walls.
- [x] 2.3 Place the steel deflector on the trench floor below the pad plane.

## 3. Verification

- [x] 3.1 `pad_trench_cut_is_full_inside_and_blends_to_grade` — full depth
      inside, zero beyond the blend, never at the launch point.
- [x] 3.2 `site_trench_cuts_the_pad_but_not_the_launch_point` — the composed
      surface drops only inside the rectangle and the declared bounds include
      the excavation.
- [x] 3.3 `spawned_flame_trench_is_open_and_grounded` — no slab bridges the
      mouth, four walls and one floor span the excavation, the deflector rests
      on the floor below the pad plane, and the mouth clears the hold-down
      clamps.
- [x] 3.4 Terrain metrics baseline re-asserted unchanged.
- [x] 3.5 Flight baselines untouched.

## 4. Evidence and records

- [x] 4.1 Two ground frames from different headings.
- [x] 4.2 Update the five progress documents.
