## Context

Rendering uses `SolarSystemParameters::for_visualization()` (`scale_factor=75000`, `planet_scale=80`, `time_scale=3000`). Planet positions come from `physics::calculate_planet_position` and rotations from `calculate_planet_rotation`. `LaunchSiteCoordinates::to_planet_relative_position` already maps lat/lon/alt to a sphere (documented as spherical-only). `Planet.mass_kg` is f64 but unused. All positions are f32 `Vec3`. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- Establish a single authoritative reference-frame module.
- Decouple flight meters from display scale.
- Provide f64 dynamics with a Vec3 render boundary and local-origin rebasing.

**Non-Goals:**
- Changing the solar-system renderer or its scale (world visuals stay as-is for now).
- Implementing gravity or 6-DOF (separate changes consume these frames).

## Decisions

**Decision: `reference_frames.rs` as the single conversion authority.**
Frames: solar-inertial (existing `calculate_planet_position` space), planet-centered (subtract planet origin), planet body-fixed (apply `calculate_planet_rotation` + axial tilt from `Planet.axial_tilt_deg`), local tangent (extend `LaunchSiteCoordinates`), rocket-body (rocket orientation `Quat`).
- Alternative: per-subsystem converters. Rejected: violates AGENTS.md section 14 (one authoritative conversion each) and section 51 (no duplicate domain logic).

**Decision: f64 dynamics core + f32 render boundary.**
`RocketPhysicalState` stores `DVec3` position/velocity. A `render_transform(state, local_origin)` maps to `Vec3`/`Transform` for Bevy. Local-origin rebasing keeps numbers small near the rocket.
- Alternative: keep f32 everywhere (Bevy native). Rejected: f32 cancellation at ~1e8-unit distances (AGENTS.md section 13).
- Alternative: move the entire solar world to f64. Rejected: large, risky rewrite not needed to fly a rocket.

**Decision: Central `PhysicalScale` resource.**
One resource defines `meters_to_display_units`, `display_units_to_meters`, and the planet visual scale mapping, reused by rocket and terrain.
- Rationale: prevents scattered magic-number scale factors (AGENTS.md sections 15 and 39).

## Risks / Trade-offs

- [Tilt/rotation sign conventions] → Document axis conventions in the module and add round-trip tests; the existing `calculate_planet_rotation` is the authoritative rotation source.
- [Performance of f64] → f64 math is confined to the few rocket dynamics systems; rendering stays f32. Measure before optimizing (AGENTS.md section 41).
- [Double mapping mismatch] → All mapping flows through `PhysicalScale`; no subsystem defines its own factor.

## Migration Plan

1. Add `reference_frames.rs` and `PhysicalScale` in domain/services.
2. Add round-trip and scale unit tests (pure Rust, no Bevy).
3. Convert `LaunchSiteCoordinates` usage to the new module (keep the old struct as a thin facade).
4. No changes to solar rendering or craft mode.

## Open Questions

None.