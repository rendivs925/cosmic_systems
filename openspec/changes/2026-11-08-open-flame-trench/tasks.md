## 1. Terrain excavation

- [x] 1.1 Add `PadTrenchZone` and the shared trench dimensions to the terrain
      source domain.
- [x] 1.2 Apply excavations in `LayeredTerrainSource::grade` after the site grade
      and extend the declared elevation bounds.
- [x] 1.3 Build one excavation per graded Earth pad from the same site list the
      grade uses.

## 2. Rendered mesh

- [x] 2.1 Add `TerrainMeshCutout` and a cutout-aware patch builder that drops
      only cells lying entirely inside the cap.
- [x] 2.2 Register the hardstand's cap through a `TerrainMeshCutouts` resource and
      pass it into every terrain bake request.
- [x] 2.3 Size the hardstand and the cap so the guaranteed opening (cap minus half
      a cell diagonal) still covers the trench's far edge.

## 3. Facility geometry

- [x] 3.1 Split the apron and deck into frames around the trench mouth, with the
      frames overlapping instead of sharing edges.
- [x] 3.2 Line the excavation with a floor and four walls whose rim meets the pad
      plane, each part straddling its opening edge.
- [x] 3.3 Place the steel deflector on the trench floor below the pad plane.

## 4. Verification

- [x] 4.1 `pad_trench_cut_is_full_inside_and_blends_to_grade` — full depth
      inside, zero beyond the blend, never at the launch point.
- [x] 4.2 `site_trench_cuts_the_pad_but_not_the_launch_point` — the composed
      surface drops only inside the rectangle and the declared bounds include the
      excavation.
- [x] 4.3 `spawned_flame_trench_is_open_and_grounded` — no slab bridges the
      mouth, four walls and one floor span the excavation, the deflector rests on
      the floor below the pad plane, the rim clears grade, the mouth clears the
      hold-down clamps and the hardstand spans the cutout.
- [x] 4.4 `mesh_cutout_omits_only_cells_inside_the_cap` — removed geometry stays
      inside the cap and vertices are untouched.
- [x] 4.5 `mesh_cutout_leaves_coarse_patches_whole` — a distant view cannot punch
      a hole.
- [x] 4.6 `launch_site_mesh_is_cut_under_the_hardstand` — the real Starbase patch
      loses geometry, and the guaranteed opening still clears the trench.
- [x] 4.7 Terrain metrics baseline re-asserted unchanged.
- [x] 4.8 Flight baselines untouched.

## 5. Evidence and records

- [x] 5.1 Two ground frames from different headings.
- [x] 5.2 Update the five progress documents.
