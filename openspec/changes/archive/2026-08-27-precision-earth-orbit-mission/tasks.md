## 1. Exact Orbital Geometry

- [x] 1.1 Add f64 analytic apsis endpoints for bound, non-circular state vectors in the existing orbital-domain module.
- [x] 1.2 Use analytic endpoints for rocket prediction markers and omit ambiguous or impact-truncated markers.
- [x] 1.3 Add oriented, long-period, circular, and impact-truncation orbital-marker regressions.

## 2. Safe Earth Insertion

- [x] 2.1 Add a pure low-Earth target-orbit configuration and success predicate using bound energy, apsides, eccentricity, and inclination.
- [x] 2.2 Replace speed-fraction orbit completion with target-aware ascent and circularization guidance.
- [x] 2.3 Make guidance throttle authoritative through control and preserve actuator limiting.

## 3. Numerical Flight Validation

- [x] 3.1 Bound powered-flight integration timesteps under time acceleration without changing the authoritative coordinate frame.
- [x] 3.2 Add deterministic ascent/insertion regressions, including an unsafe-periapsis rejection and a safe circular-Earth-orbit acceptance.
- [x] 3.3 Run formatting, linting, full tests, and rocket-mode startup validation; record any remaining nominal-flight model limitations.
