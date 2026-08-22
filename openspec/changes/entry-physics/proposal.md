## Why

The rocket simulator has complete ascent/descent guidance and real DEM terrain, but lacks physically coherent atmospheric entry physics. Without aerothermal heating, ablation, plasma blackout, parachute dynamics, and supersonic retro-propulsion, reentry from orbit is not physically simulated—vehicles simply "land" without thermal protection or realistic deceleration. This change adds the complete entry physics chain from entry interface to touchdown.

## What Changes

- **Convective heating**: Sutton-Graves correlation for stagnation-point heat flux from velocity, density, nose radius.
- **Radiative heating**: Tauber-Sutton approximation for high-velocity entries (lunar return, Mars).
- **Ablation**: Char-layer recession model updating nose radius and vehicle mass from integrated heat load.
- **Plasma blackout**: Electron density model detecting comms loss when plasma frequency exceeds comms frequency.
- **Parachutes**: 3-stage inflation (mortar → reefed → full) with time-varying Cd, forces applied to 6-DOF accumulator.
- **Supersonic retro-propulsion**: Plume-freestream interaction model for engine ignition above Mach 1.
- **EntryPhysicsConfig**: Per-body coefficients, material properties, parachute parameters.

## Capabilities

### New Capabilities

- `entry-physics`: Complete atmospheric entry physics from entry interface to touchdown.

### Modified Capabilities

None — this is a new capability extending the existing physics pipeline.

## Impact

- **Physics pipeline**: New systems in `FixedUpdate` before 6-DOF integration: `compute_heating`, `compute_ablation`, `compute_plasma_blackout`, `compute_parachute_forces`, `compute_retro_propulsion`.
- **Components**: New `ThermalState`, `AblationState`, `ParachuteState` components written by entry physics, read by downstream systems.
- **Propulsion**: Retro-propulsion modifies effective thrust and base pressure during supersonic ignition.
- **Guidance/Comms**: Plasma blackout events signal comms loss to guidance and telemetry.
- **Terrain**: Parachute terminal velocity affects landing detection in collision system.
- **Dependencies**: No new crates required (all pure Rust math); optional `nalgebra` for matrix ops in ablation if needed.