# Phase 18: Domain Boundary Audit

Date: 2026-09-06

## Checkpoint

Phase 17 is frozen at `6b8b636 Reorganize simulator module ownership` on
`main`. The worktree was clean before this audit. The checkpoint passed the
DEM-enabled test suite and bounded startup of normal, craft, and rocket modes.

This document is an audit and migration plan only. It does not change domain
behavior, numerical models, RON schemas, or Bevy scheduling.

## Audit Method

- Searched every Rust file under `src/domain` for `bevy::` imports and Bevy ECS
  types.
- Traced direct consumers of the domain types that contain presentation or ECS
  state.
- Confirmed that the installed Bevy 0.17.3 math layer uses `glam` 0.30.10.
- Treated source inspection as authoritative. The existing graph data predates
  Phase 17 module moves and is not current-path evidence.

The audit found 29 domain files with direct Bevy imports. `src/domain/events/`
does not exist: rocket events were already moved to
`src/infrastructure/bevy_adapters/rocket/events.rs` in Phase 17.

## Classification

### Domain Math: REPLACE

The following dependencies represent physical vectors, rotations, and inertia
matrices. They are domain concepts, but their current `bevy::math` import path
is an implementation convenience rather than a requirement for Bevy.

| Current owners | Current Bevy types | Classification | Target |
| --- | --- | --- | --- |
| `entities/rocket.rs` | `Vec3` engine stations, thrust axes, and booster attachments | REPLACE | A domain body-frame vector type backed by direct `glam`.
| `value_objects/launch_site_coordinates.rs` | `Vec3` geographic position and distance helper | REPLACE | A domain vector type; use the authoritative reference-frame conversion rather than retaining a second spherical conversion.
| `value_objects/physical_scale.rs` | `DVec3` display conversion | REPLACE, then MOVE the display scale | Domain meter vectors remain domain math; display-unit conversion leaves the domain.
| `services/actuation.rs`, `aerodynamics.rs`, `atmosphere.rs`, `cube_sphere.rs`, `dem_terrain_source.rs`, `gravity.rs`, `landing_gear.rs`, `long_arc_propagation.rs`, `recovery.rs`, `scientific_validation.rs`, `terrain_collision.rs`, `terrain_source.rs`, and `trajectory.rs` | `DVec3` | REPLACE | Domain f64 vector type.
| `services/body_orientation.rs`, `control.rs`, `ephemeris.rs`, `guidance.rs`, `rocket_dynamics.rs`, and `rocket_propulsion.rs` | `DVec3`, `DQuat`, `DMat3` as applicable | REPLACE | Domain f64 vector, rotation, and matrix types.
| `services/physics_orbital.rs` | `DVec3` | REPLACE | Domain f64 vector type. Its separate f32 display helper is classified below.
| `services/reference_frames.rs` | `DVec3`, `DQuat`, `DMat3` | REPLACE | Domain f64 frame and orientation types. Its f32 display functions are classified below.

The proposed first step is a small `domain::math` facade backed by a direct
`glam` 0.30 dependency compatible with Bevy 0.17.3. It should expose only the
existing vector, rotation, and matrix representations. It must not introduce
new arithmetic, unit conversions, or a second coordinate-system authority.

This is intentionally not a hand-written vector library. `glam` supplies the
same proven numerical implementation without making the domain depend on Bevy.

### Rendering State: MOVE

| Current owner | Current Bevy dependency | Classification | Required destination |
| --- | --- | --- | --- |
| `entities/planet.rs` | `Color` stored on `Planet` | MOVE | Presentation appearance catalog keyed by body identity.
| `value_objects/planet_configs.rs` | `Color` in every `PlanetConfig` | MOVE | Presentation appearance catalog; physical catalog retains only physical and surface data.
| `services/planet_factory.rs` | `Color` passed from config to `PlanetBuilder` | MOVE | Keep the physical factory domain-only; presentation resolves a body's appearance separately.
| `services/rocket_dynamics.rs` | `Transform` returned by `render_transform` | MOVE | `infrastructure/bevy_adapters/rocket/presentation.rs`, where the only production caller already exists.
| `services/physics_orbital.rs` | f32 `Vec3` returned by `transform_orbital_point` | MOVE | `infrastructure/bevy_adapters/rendering/meshes.rs`; its only production consumer builds a render mesh. Preserve the f64 orbital function in the domain.
| `services/reference_frames.rs` | f32 `Vec3` and `PhysicalScale` in solar-display conversion helpers | MOVE | A rendering-boundary adapter. The functions convert to and from display units, not physical frames.
| `value_objects/physical_scale.rs` | `Resource`, display-unit fields, and display conversions | MOVE | Rendering infrastructure or presentation configuration. Keep physical constants such as one AU in a pure domain units module.

`Planet::color` has exactly two production consumers: solar-system body startup
and rocket moon-proxy presentation. Both are rendering concerns. Moving it must
preserve their current colors through a presentation lookup, not re-infer color
from body class or names at call sites.

### ECS And Runtime Integration: ADAPT Or MOVE

| Current owner | Current Bevy dependency | Classification | Required boundary |
| --- | --- | --- | --- |
| `value_objects/launch_site_coordinates.rs` | `#[derive(Component)]` | ADAPT | Keep `LaunchSiteCoordinates` as a pure domain value. Create an infrastructure component wrapper or an explicit Bevy trait implementation outside the domain module.
| `value_objects/solar_system_params.rs` | `#[derive(Resource)]` | MOVE | Split the current mixed resource into pure simulation/physical settings and presentation settings. `scale_factor`, `show_orbits`, and `planet_scale` are presentation concerns.
| `services/simulation_time.rs` | `Resource`, `Time`, `FixedMain`, `World`, input types, scheduling, and Bevy logging | MOVE | Retain the pure clock state and transition methods in the domain. Move input handling, wall-clock sampling, fixed-time synchronization, bounded runner, resource registration, and logging to an infrastructure time adapter.

`SimulationTime` is the authoritative simulation clock, not Bevy time. The
domain portion must continue to own fixed-step duration, pause, warp backlog,
completed simulation time, and scientific epoch conversion. Bevy only supplies
wall-clock samples and executes the adapter systems.

### KEEP

No direct Bevy dependency in the audited domain has a domain-independent reason
to remain. This does not require replacing `glam` with custom mathematics or
moving pure physics out of the domain.

## Current Boundary Violations

- `RocketDynamicsState::render_transform` creates an f32 `Transform` inside the
  authoritative 6-DOF domain module.
- `LaunchSiteCoordinates` is a domain value object with an ECS `Component`
  derive and a duplicate low-precision spherical coordinate conversion.
- `PhysicalScale` models display units and is registered as a Bevy resource
  despite living in `domain/value_objects`.
- `SolarSystemParameters` combines physical values, time-warp state, and
  presentation controls in one Bevy resource.
- `SimulationTime` contains both the pure clock and Bevy input/schedule runner
  systems.
- Planet physical catalog entries own rendering colors.

## Migration Order

1. Add the direct `glam` dependency and a minimal pure `domain::math` facade.
   Replace only `bevy::math` imports in pure calculations. Keep all concrete
   types and numerical algorithms unchanged.
2. Move render conversions out of `rocket_dynamics`, `physics_orbital`, and
   `reference_frames`. Update presentation and mesh callers to own the final
   `Vec3` and `Transform` conversion.
3. Remove ECS derives from domain values. Add infrastructure adapters for
   launch-site storage and simulation-clock resource access.
4. Split presentation scale/orbit visibility from physical and time state.
   Move `PhysicalScale` and the presentation portion of
   `SolarSystemParameters` out of `domain/value_objects`.
5. Separate planetary appearance data from the physical catalog, then update
   the two rendering consumers through one presentation-owned lookup.
6. Rebuild the project knowledge graph after current-path changes are complete.

Each step is independently reviewable and must preserve the prior step's
validated behavior. Do not combine a numerical precision migration, new units
types, color redesign, or scheduler redesign with this boundary migration.

## Required Regression Coverage

- Preserve the existing fixed-pipeline and determinism baseline tests.
- Add pure domain tests for every moved vector/rotation boundary and for launch
  site coordinate conversion through the authoritative reference-frame API.
- Keep render-boundary tests for f64-to-f32 rebasing, rocket interpolation, and
  orbital mesh generation after adapters move.
- Verify no `bevy::` import remains below `src/domain` after the final slice.
- Run `cargo fmt --check`, `cargo check --features dem`,
  `cargo clippy --features dem -- -D warnings`, `cargo test --features dem`,
  `cargo build --release --features dem`, and bounded normal, craft, and rocket
  startups after the implementation phase.

## Risks

- Direct `glam` and Bevy math must resolve to the same compatible version; a
  duplicate major version would make boundary conversions explicit and increase
  migration risk.
- Moving `SimulationTime` must preserve its bounded fixed-runner ordering and
  the invariant that simulation time advances only after completed fixed ticks.
- Moving planet colors must retain existing visible output without creating a
  second body catalog.
- Replacing launch-site conversion must preserve frame conventions, ellipsoid
  handling, and launch-pad placement. `reference_frames.rs` remains the single
  conversion authority.
