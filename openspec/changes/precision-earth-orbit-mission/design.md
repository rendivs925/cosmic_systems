## Context

The rocket already keeps authoritative state in f64 planet-centered inertial coordinates, uses centralized gravity and propulsion, and presents prediction through a render-origin adapter. The current ascent reaches an `Orbit` phase from a speed fraction, while control replaces guidance throttle targets. Apsis markers are extrema of coarse propagated samples rather than extrema of the osculating conic.

## Goals / Non-Goals

**Goals:**
- Preserve one authoritative f64 state, gravity model, propulsion system, and reference-frame implementation.
- Make displayed apsides exact for the current two-body osculating state.
- Make the standard Earth mission insert only into a physically safe target orbit.
- Keep numerical error bounded and regression-tested during powered flight.

**Non-Goals:**
- Claim perfect real-world flight fidelity without validation data and models for winds, engine performance, navigation error, Earth harmonics, and operational constraints.
- Replace the current 6-DOF integrator, terrain model, or patched-conics visual propagator.
- Add multi-body precision propagation or satellite payload dynamics in this change.

## Decisions

### Analytic apsides supplement the visual propagator

Use the eccentricity vector and specific orbital energy from the f64 state vector to derive exact apoapsis/periapsis positions for bound, non-circular conics. The existing RK4 patched-conics path remains the visible trajectory and impact detector. This eliminates sample-step marker displacement without duplicating trajectory propagation. Circular and impact-truncated arcs omit ambiguous or false markers.

### Target orbit is a value object evaluated from state vectors

Add a compact target-orbit configuration with explicit altitude, eccentricity, inclination, and safety tolerances. A pure predicate evaluates the current f64 state with the existing orbital-element machinery. This replaces the raw speed-fraction phase transition and gives guidance one observable completion condition.

### Guidance owns throttle targets

Guidance writes throttle and attitude targets. Control only computes torque/gimbal/RCS responses to the attitude target, and actuation continues to apply physical limits. This restores the documented guidance → control → actuation boundary and permits insertion/deorbit/descent burn logic to function.

### Powered physics uses bounded substeps

Keep an individual authoritative physics substep at or below the configured fixed duration. Time acceleration advances simulation through multiple steps rather than multiplying a burn integrator duration. This favors deterministic, stable force and propellant integration over arbitrary warp throughput.

## Risks / Trade-offs

- More fixed substeps at high time warp increase CPU work → cap warp or process bounded steps over multiple render frames.
- A real Falcon 9 mass/performance calibration changes existing deterministic baselines → record a justified new baseline only after insertion behavior is verified.
- Two-body target criteria omit J2, drag uncertainty, winds, and navigation dispersions → document this as engineering-grade nominal-flight scope, not a real launch certification.

## Migration Plan

1. Add pure apsis and target-orbit tests before changing rendering or phase transitions.
2. Route throttle ownership through guidance and tighten insertion completion.
3. Add bounded-step regression coverage and update baselines only with an explicit audit.
4. Retain the current velocity-threshold behavior only until the target predicate is active; no save-data migration is required.
