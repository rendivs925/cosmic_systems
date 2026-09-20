## 1. Sunlight Correctness

- [x] 1.1 Derive rocket directional-light illuminance from the ephemeris
  planet–Sun distance with inverse-square scaling, keeping the existing Earth
  calibration value and updating it when the epoch changes.
- [x] 1.2 Size the rendered Sun disc from the actual planet–Sun distance and
  hide it below the local horizon with a bounded refraction allowance.
- [x] 1.3 Rewrite the non-direct daylight factor as solar-altitude twilight
  bands so the direct terminator stays geometric.
- [x] 1.4 Remove flat albedo emissive from non-Sun bodies; keep the Sun
  emissive and Earth's real night-emission texture.
- [x] 1.5 Add pure unit tests for direction sign, inverse-square scaling, disc
  angular radius, horizon visibility, twilight bands, and night-side emission.

## 2. Rocket-Flight Shadows

- [x] 2.1 Enable directional shadows with an explicit cascade configuration and
  shadow-map resource sized to near-flight scale.
- [x] 2.2 Mark the far-field planet shell and cloud shell as
  `NotShadowCaster`/`NotShadowReceiver` so they cannot darken the scene.
- [ ] 2.3 Confirm terrain, vehicle, and pad cast/receive and add regression
  tests for shadow participation and enclosing-globe exclusion.

  Conclusive Xvfb capture shows the vehicle casting a correctly directed pad
  shadow from the ephemeris Sun; the enclosing-shell exclusion is enforced by
  the inserted markers. A direct ECS regression test for the markers was not
  added because spawning the spawn paths requires asset-server setup, so this
  task remains open pending a lighter-weight assertion.
- [x] 2.4 Visually validate shadow direction near the pad against the
  ephemeris Sun via a headless startup check (or Xvfb screenshot).

## 3. Physically Based Sky

- [x] 3.1 Add a Bevy-free `AtmosphericOptics` value object with per-body
  Rayleigh/Mie/ozone parameters and deterministic unit tests.
- [x] 3.2 Add the custom `sky.wgsl` single-scattering shader and a
  camera-anchored sky dome material/adapter that consumes true planet centre,
  local vertical, Sun direction, and the optics.
- [x] 3.3 Update the sky material each frame from render origin, camera,
  ephemeris, and flight conditions, and register it in rocket-mode composition.
- [x] 3.4 Replace the flat clear-colour/fog sky with the scattering dome and
  derive ambient sky terms from the same model.
- [x] 3.5 Add aerial perspective to the terrain surface extension from the same
  optics and remove the independent distance-fog colour.

  Implemented through Bevy's per-channel `FogFalloff::Atmospheric`, whose
  extinction and in-scattering are derived per frame from the bound body's
  `AtmosphericOptics` at the observer altitude, with a warm Sun lobe for Mie
  forward scattering. This is a single homogeneous-layer approximation of the
  vertical atmosphere, not a per-fragment vertical integral; it removes the
  independent fixed fog colour and uses the authoritative optics.
- [x] 3.6 Add tests for optics determinism and sky-material uniform derivation;
  validate day, twilight, night, and vacuum transitions.

  Determinism, extinction/scattering falloff, ozone-band, aerial-perspective,
  and uniform-derivation tests pass. Day, night, and the solar-disc horizon
  occlusion were validated under Xvfb software rendering; twilight and vacuum
  were validated only analytically by the pure functions. The sky radiance is
  explicitly calibrated (`SKY_RADIANCE_SCALE`) so a clear daytime sky does not
  clip to white; final aesthetic tuning needs a real display.

## 4. Exposure, Calibration, And Validation

- [x] 4.1 Add HDR and an explicit exposure calibrated to physical solar
  illuminance, and threshold bloom from the calibrated solar luminance.
- [x] 4.2 Verify daylight, night lights, and the solar disc map stably in the
  shared camera without an unrelated ambient floor.
- [x] 4.3 Run `cargo fmt --check`, `cargo check/test --features dem`,
  `cargo check/test --no-default-features`, and
  `cargo test --release --features dem`.
- [x] 4.4 Run bounded startup checks for `cargo run`, `cargo run -- craft`, and
  `cargo run -- rocket`, plus `openspec validate --all --strict`.
- [x] 4.5 Record any visual limitation honestly when no usable display is
  available.

  All visual evidence in this change came from Xvfb software Vulkan at
  1920x1080. No native-GPU display validation was performed, and the normal
  solar-map view renders empty in this environment (identical before and after
  the change), so only bounded no-panic startup was asserted for it.
