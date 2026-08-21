## Why

The solar system is rendered at a heavily scaled, display-oriented world scale (`SolarSystemParameters::for_visualization`: 1 AU = 75,000 units, `planet_scale = 80`, `time_scale = 3000`). A rocket needs physically coherent, meter-based dynamics relative to a body, and must not suffer catastrophic f32 precision loss when a small vehicle is ~1.5e8 units from the Sun. Today there is no flight reference-frame or physical-scale layer at all. Without it, every later rocket subsystem (gravity, 6-DOF, aero, landing) would inherit an inconsistent, unscaled world.

## What Changes

- Introduce a physical-scale layer that decouples flight dynamics (real meters) from the visualization world scale.
- Introduce an explicit reference-frame module with conversions between solar-inertial, planet-centered, planet body-fixed, local tangent (lat/lon/alt), and rocket-body frames.
- Run rocket dynamics in f64 (`DVec3`) internally while rendering stays in `Vec3` via a local origin / origin-rebasing boundary.
- Reuse existing `Planet.mass_kg` (f64), `calculate_planet_position`, `calculate_planet_rotation`, and `LaunchSiteCoordinates` rather than duplicating coordinate math.
- Keep a single authoritative implementation of each conversion (AGENTS.md sections 14 and 51).

## Capabilities

### New Capabilities

- `reference-frames`: Conversions between solar-inertial, planet-centered, planet body-fixed, local tangent, and rocket-body frames, and the physical scale mapping between meters and display units.

### Modified Capabilities

<!-- None - no existing openspec/specs exist yet. -->
