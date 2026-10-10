## 1. Shared tree lattice

- [x] 1.1 Add `TREE_SITE_LEVEL`, `TREE_CANDIDATE_SIDE`, and
      `TREE_CANDIDATE_SIDE_COARSE` and the `tree_site_candidates` lattice in
      `vegetation.rs`.
- [x] 1.2 Add `tree_candidate_side` so a level maps to a nested lattice side.
- [x] 1.3 Add `tree_site_gate` (level-independent acceptance) and
      `thin_coarse_sites` (position-ranked coarse trimming) in `scatter.rs`.
- [x] 1.4 Generate trees from the lattice; remove the patch-local spacing filter.

## 2. Budget

- [x] 2.1 Set `FULL_TREE_BUDGET_CAP` (100) and `COARSE_TREE_BUDGET_CAP` (24).
- [x] 2.2 Feed both into the streaming reservations; add the const assert that
      the full lattice stays within the configured tree budget.
- [x] 2.3 Confirm the near-camera LOD selection test still requests level 14.

## 3. Tests

- [x] 3.1 Domain: `tree_site_lattice_nests_across_lods` (positions and a
      level-independent gate).
- [x] 3.2 Domain: `tree_site_lattice_is_deterministic_and_bounded`.
- [x] 3.3 DEM: `launch_site_trees_survive_lod_refinement` with the real Earth
      source.
- [x] 3.4 Update the coarse-lattice budget test.

## 4. Validation

- [x] 4.1 `cargo fmt --check`, `cargo clippy --features dem --all-targets`.
- [x] 4.2 `cargo test --features dem`, `cargo test --no-default-features --lib`.
- [x] 4.3 Release build and smoke-run solar, craft, and rocket modes.
- [x] 4.4 `openspec validate --all --strict`.
