## Why

There is no atmosphere model in the codebase. Atmospheric flight (drag, lift, dynamic pressure, Max Q, reentry) is impossible without one, and the rocket currently has no aerodynamic forces at all. The architecture must centralize atmospheric calculations and aerodynamic forces (AGENTS.md section 19) so multiple planets can have different atmospheres and no subsystem scatters its own density formulas.

## What Changes

- Introduce an atmosphere model abstraction (`AtmosphereSource`) returning temperature, pressure, density, and speed of sound by altitude for a given planet.
- Implement an Earth atmosphere model (exponential/ISA-style reference) with an extensible design for other planets (Mars, Venus, Moon/vacuum).
- Add aerodynamics: dynamic pressure `q = ½ρv²`, Mach number, angle of attack, drag, lift, side force, aerodynamic coefficients, center of pressure, and aerodynamic torque about the center of mass.
- Add Max Q detection (peak dynamic pressure during ascent).
- Centralize so each planet provides its own atmosphere and aero calculations reuse it.

## Capabilities

### New Capabilities

- `atmosphere`: Per-planet atmosphere models producing temperature, pressure, density, and speed of sound by altitude.
- `aerodynamics`: Aerodynamic forces (drag, lift, side force), dynamic pressure, Mach, angle of attack, aerodynamic torque, and Max Q derived from atmosphere and rocket geometry.

### Modified Capabilities

<!-- None - no existing openspec/specs exist yet. -->