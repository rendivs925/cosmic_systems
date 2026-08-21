## 1. Physical scale

- [ ] 1.1 Add `PhysicalScale` resource defining meters-to-display-units and inverse, plus the planet visual scale mapping
- [ ] 1.2 Add unit tests for scale conversions (meters to display units and back)

## 2. Reference-frame module

- [ ] 2.1 Add `domain/services/reference_frames.rs` with solar-inertial, planet-centered, planet body-fixed, local tangent, and rocket-body conversions
- [ ] 2.2 Wire existing `calculate_planet_position`, `calculate_planet_rotation`, `Planet.axial_tilt_deg`, and `LaunchSiteCoordinates` into the conversions
- [ ] 2.3 Add round-trip unit tests for each conversion chain
- [ ] 2.4 Add a KSC lat/lon/alt to body-fixed test consistent with Earth radius/tilt/rotation

## 3. f64 dynamics core

- [ ] 3.1 Add `RocketPhysicalState` with f64 (DVec3) position and velocity
- [ ] 3.2 Add render-boundary conversion from f64 physical state to f32 `Transform` with local-origin rebasing
- [ ] 3.3 Add precision test showing f32 cancellation avoided at large distances

## 4. Integration

- [ ] 4.1 Expose reference frames and physical scale through the shared infrastructure module graph
- [ ] 4.2 Run `cargo check`, `cargo clippy`, `cargo fmt --check`, `cargo test`