## Why

Rocket mode currently renders sampled apsis markers and transitions to `Orbit` from a speed threshold, even when the predicted periapsis intersects Earth. Guidance and control also compete for throttle authority, so a valid insertion burn cannot complete reliably. This prevents the simulator from producing a physically valid Earth orbit or a trustworthy orbital display.

## What Changes

- Compute apoapsis and periapsis markers analytically from f64 two-body state vectors, while retaining patched-conics propagation for the visible trajectory.
- Define a target low-Earth orbit and use bound-energy, safe-periapsis, radius, eccentricity, and inclination criteria for insertion success.
- Make guidance the sole owner of throttle targets; control remains responsible for attitude torque and actuator allocation.
- Drive ascent toward an Earth-fixed target orbital plane and perform a target-aware circularization burn instead of stopping at a speed threshold.
- Tighten powered-flight numerical handling and add deterministic full-ascent regression coverage.

## Capabilities

### New Capabilities
- `precision-orbit-insertion`: Targets and validates a safe Earth orbit from authoritative f64 state vectors.
- `analytic-orbit-markers`: Renders physically defined apsis markers from f64 orbital geometry.

### Modified Capabilities
- `rocket-guidance-control`: Guidance owns trajectory throttle targets and declares insertion only after a valid orbital solution.
- `rocket-dynamics`: Powered-flight integration remains bounded and deterministic through the ascent and insertion sequence.

## Impact

- Updates orbital-domain calculations, rocket guidance/control systems, flight telemetry/prediction rendering, and the Falcon 9 flight configuration where required for a viable insertion margin.
- Adds deterministic unit and integration regressions for apsis geometry, insertion criteria, and a complete Earth-orbit ascent.
- Does not add a second coordinate system, gravity model, or propulsion implementation.
