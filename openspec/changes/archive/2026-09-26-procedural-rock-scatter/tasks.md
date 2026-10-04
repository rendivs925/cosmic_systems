## 1. Procedural Rock Geometry

- [x] 1.1 Add a deterministic procedural rock builder to `MeshAccum` in
  `src/infrastructure/bevy_adapters/terrain/surface/scatter.rs` that starts from
  a closed base shape and displaces vertices along the radial direction with
  seeded fBm (low-frequency asymmetry plus bounded higher-frequency detail).
- [x] 1.2 Add an optional bounded thermal-smoothing pass that relaxes vertices
  above a talus slope threshold, toggled per rock from its deterministic seed.
- [x] 1.3 Apply deterministic per-instance size and non-uniform aspect variation
  drawn from the existing `hash01` chain.
- [x] 1.4 Recompute rock vertex and index counts and update `ROCK_COUNT`,
  `ROCK_MAX_LUMPS`, tessellation constants, and the derived
  `MAX_VEGETATION_MESH_BYTES` reservation in `surface/mod.rs` so the streaming
  bound stays conservative; do not raise the rock count.

## 2. Slope-Weighted Blue-Noise Placement

- [x] 2.1 Replace the per-slot rock candidate hashing with a deterministic
  jittered-grid blue-noise candidate generator over the patch's local UV,
  seeded from patch identity.
- [x] 2.2 Weight candidate acceptance by `slope_deg_at` so steeper ground
  gathers more rock while gentle ground keeps sparse outcrops, and keep
  acceptance capped by `scatter_count_for_level(ROCK_COUNT, patch.level)`.
- [x] 2.3 Confirm placement offsets are seeded per patch so adjacent patches do
  not expose a regular grid seam.

## 3. Grounding And Contact Occlusion

- [x] 3.1 Embed each rock's base below the authoritative `TerrainSource` height
  along the `surface_normal`, scaling embed depth with slope and clamping it so
  steep faces do not clip the rock away.
- [x] 3.2 Darken rock vertex colour toward the embedded base from each vertex's
  normalized axial position, bounded so small rocks are not over-darkened, and
  compose it with the existing moisture/slope tint.
- [x] 3.3 Verify the rock loop still reads height, slope, and normal only from
  the shared `TerrainSource` and writes nothing back to authoritative state.

## 4. Tests And Determinism

- [x] 4.1 Add a test that generating the same patch twice produces identical
  rock geometry and vertex colours.
- [x] 4.2 Add a test that rock silhouettes are non-spherical and that size and
  aspect vary across instances within one patch.
- [x] 4.3 Add a test that steeper ground accepts more rock candidates than
  gentle ground at the same LOD budget.
- [x] 4.4 Add a test that rock base vertices sit below the sampled surface and
  that base vertex colour is darker than crown vertex colour.
- [x] 4.5 Add or update a test that rocks stay merged into one per-patch mesh,
  do not spawn entities, and respect LOD decimation and the per-patch cap.
- [x] 4.6 Update the existing boulder colour test and any scatter budget tests
  affected by the new primitive footprint.

## 5. Validation

- [x] 5.1 Run `cargo fmt --check`, `cargo check`, and `cargo clippy`; resolve
  warnings introduced by the change.
- [x] 5.2 Run `cargo test --features dem` and `cargo check --no-default-features`
  to cover both terrain configurations.
- [x] 5.3 Run bounded startup checks for `cargo run`, `cargo run -- craft`, and
  `cargo run -- rocket`, reporting honestly if a mode cannot be visually
  validated in the environment.
- [x] 5.4 Run `openspec validate procedural-rock-scatter --strict` and confirm
  it passes.
