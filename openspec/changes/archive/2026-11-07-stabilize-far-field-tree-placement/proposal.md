## Why

Far-field trees are both unstable and far too sparse. Tree candidates are phased
and jittered from the *patch* identity, so when a patch refines its whole tree set
is re-rolled: trees teleport and pop. Separately, the coarse, tree-only level
(level 11) is capped at 8 trees over a ~4.9 km patch (~0.33 trees/km²) while level
12 already reaches ~21 trees/km², so zooming out leaves the landscape essentially
bare.

## What Changes

- Tree sites come from one shared lattice keyed only by the world direction
  (`tree_site_candidates`), anchored to fine cells at level 14. A patch selects
  every `2^(14 - level)`-th cell, so the sites of a coarse patch are a strict
  subset of those of the finer patches that replace it.
- The ecological gate (`tree_site_gate`) reads only level-independent signals
  (`height_m`, slope, moisture, cover, clumping), never a level-specific mesh
  height, so the accepted set nests across LODs. Refining a patch adds trees but
  never moves or removes one.
- Coarse patches use a reduced lattice and thin their sites to a bounded budget by
  a position-deterministic rank. Because finer levels never thin, the retained
  coarse sites are still a subset of the finer accepted set, so thinning never
  drops a tree that a finer patch would keep.
- The full lattice stays within the configured tree budget
  (`TREE_CANDIDATE_SIDE = 10`, 100 sites <= 128) and the coarse reservation stays
  bounded (`COARSE_TREE_BUDGET_CAP = 24`), so the per-patch streaming reservation
  does not grow and near-camera LOD selection is preserved.
- The per-patch in-species spacing filter is removed: the lattice already
  guarantees spacing, and a patch-local filter would break cross-LOD stability.

## Capabilities

### New Capabilities

<!-- none -->

### Modified Capabilities

- `vegetation-placement`: adds a "Tree sites are stable across LOD refinement"
  requirement, so placement must derive positions and acceptance only from the
  world direction and coarse thinning must nest inside the finer set.

## Impact

- `src/domain/services/vegetation.rs`: lattice, gate, and thinning helpers.
- `src/infrastructure/bevy_adapters/terrain/surface/scatter.rs`: tree generation.
- `src/infrastructure/bevy_adapters/terrain/surface/mod.rs`: byte reservations.
- Tests: domain lattice nesting/determinism; a DEM launch-site cross-LOD test.
