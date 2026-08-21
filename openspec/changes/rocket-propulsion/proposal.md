## Why

The rocket entity already stores real Falcon-9 propulsion parameters (dry mass, fuel mass, `max_thrust_kn`, sea-level and vacuum ISP, `gimbal_range_deg`, plus `mass_flow_rate_kg_s`), but the runtime physics hardcodes a simplified `100 kg/s` fuel burn and an upward-only `thrust.y = 100000`. There is no throttle, no staged engine set, no staging, no gimbal torque, and no proper propellant depletion feeding the 6-DOF dynamics. Propulsion must become physically authoritative and feed the 6-DOF system.

## What Changes

- Extend the `Rocket` domain entity into a full vehicle definition: stages, per-stage engines, engine positions and thrust vectors, dry mass, propellant, ISP (sea level/vacuum), throttle, gimbal range.
- Add propulsion systems: thrust calculation (`T = m_dot * Isp * g0`), throttle control, mass flow and propellant depletion, engine startup/shutdown, staging transitions, and gimbal torque.
- Replace the hardcoded thrust/fuel logic in `rocket_systems.rs`.
- Feed thrust force and gimbal torque into the 6-DOF accumulator pipeline.
- Add unit tests: rocket equation Δv, mass loss = m_dot·t, staging mass shed, thrust from ISP.

## Capabilities

### New Capabilities

- `rocket-propulsion`: Physically consistent propulsion for the rocket: thrust from ISP and mass flow, throttle, propellant consumption, engine startup/shutdown, staging, and engine gimbal torque.

### Modified Capabilities

<!-- None - no existing openspec/specs exist yet. -->