# Scientific Correctness Audit

A separate track from the planetary-realism programme. That programme is closed
at 45/45 verified tasks; this document records questions about **physics,
frames, and data provenance** rather than visual quality. Nothing here reopens
a roadmap task.

**Method.** Each item is answered by reading the authoritative implementation
and its tests, then naming what is proven and what is not. A change is made only
when the defect is concrete. A fix may not silently move a committed flight
baseline: either it leaves all three fixtures unchanged, or the affected fixture
is re-recorded through the documented reviewed workflow and the reason is
recorded here. Status follows the project rule: `IMPLEMENTED` means code plus at
least one test, not runtime-verified flight.

| ID | Question | Verdict |
| --- | --- | --- |
| SC-01 | Retro-propulsion effectiveness is applied during a powered ascent | **DEFECT — fixed, IMPLEMENTED** |
| SC-02 | Frame handedness across the ICRF / solar / IAU / terrain boundaries | **AUDITED — no defect; residual risk recorded** |
| SC-03 | Aerodynamic axes, angle-of-attack sign, and center of pressure | **PARTIAL — model self-consistent; two documentation defects** |
| SC-04 | Per-thruster RCS allocation | **GAP — no thruster model; scalar per-axis clamp** |
| SC-05 | Closed-loop orbit insertion | **PARTIAL — acceptance predicate and bang-bang burn only** |
| SC-06 | Falcon 9 data provenance | **DECLARED REPRESENTATIVE — no pinned source byte** |
| SC-07 | WGS-84 geodetic vs geocentric latitude | **DEFECT — fixed for insertion targeting; one residual** |
| SC-08 | Orbital inclination measured from the geometric pole instead of the rotation axis | **DEFECT — fixed; one baseline regenerated** |

---

## SC-01 — Retro-propulsion during ascent

**Defect.** `compute_retro_propulsion` set the thrust multiplier from two
conditions only: Mach ≥ `retro_propulsion_mach_threshold` and a running core
stage. The multiplier is consumed by `propulsion_thrust`, which scales both core
and booster thrust. A vehicle climbing through Mach with its engines running
therefore lost thrust to a base-pressure correlation that only exists when the
exhaust faces the oncoming flow.

With the Earth defaults (threshold 1.2, coefficient 0.1) the loss grows with
Mach: 0.92 at Mach 2, 0.82 at Mach 3, 0.62 at Mach 5, bottoming at 0.50 once the
Mach-excess cap is reached. Every Falcon 9 ascent passes Mach 1.2 with the first
stage still burning, so the committed ascent trajectory was flying with reduced
thrust.

**Fix.** `retro_propulsion_plume_faces_freestream(thrust, relative_velocity)` is
a new pure-domain predicate in `src/domain/services/entry_physics.rs`. The
adapter now builds the thrust in the inertial frame
(`orientation * stage_thrust_body(..)`) and applies the correlation only when
`thrust · v_air_relative < 0`. An ascending burn, a thrusting dive, and a plume
perpendicular to the flow all keep multiplier 1.0; a retro or landing burn
applies the correlation as before.

**Tests.**
- Domain: five cases on the predicate (ascent, retro burn, perpendicular,
  zero thrust, zero velocity).
- ECS: three tests in `rocket/entry.rs` that run the real system — supersonic
  ascent keeps 1.0, a supersonic retro burn gets exactly
  `retro_propulsion_effectiveness`, subsonic flight keeps 1.0.

**Baselines unchanged.** The three committed flight-parameter baselines run a
minimal harness that never inserts `EntryPhysicsConfig` or
`RetroPropulsionEffect`, so neither the old nor the new multiplier was part of
those recordings. Those regression tests still pass without regenerating a
baseline.

**Runtime A/B (headless).** `headless_scenario --vehicle falcon9 --site
starbase --steps 7680` flies the real fixed pipeline for 120 s. The fixed build
reaches 4.817 km/s at 187.4 km; a temporary build with the pre-fix gate reaches
2.807 km/s at 190.7 km. Thrust at Mach 3 is 8,097 kN fixed versus 6,658 kN
pre-fix — exactly the 0.82 correlation factor — and the lost acceleration also
delays Mach growth and staging, so the gap compounds. Restoring the gate
reproduced the fixed artifact bit for bit.

**Residuals.**
- No runtime flight measurement is possible in this session (rocket mode leaves
  the pad only on a Space press and there is no input injection), so this is
  `IMPLEMENTED`, not `VERIFIED`. The unit and system tests are the evidence.
- `MIN_RETRO_EFFECTIVENESS = 0.1` is unreachable with the default
  `base_pressure_coefficient = 0.1` and `MAX_RETRO_MACH_EXCESS = 5.0`; the
  lowest value is 0.5. The clamp comment ("never loses more than 90 %") only
  holds for a coefficient of at least 0.18. Left as is: recalibration is a
  data/config decision, not a code change.

## SC-02 — Frame handedness

**Design as found.** Three improper maps are deliberate and tested:
`icrf_j2000_to_solar_inertial` has determinant −1 and maps `x × y = −z`
(`icrf_to_solar_inertial_is_an_improper_transform`); the terrain↔IAU axis swap
`(x, z, y)` is an odd permutation, hence also improper (its round trip is
tested); the IAU orientation between them is proper. The composition is
therefore a proper rotation, which the comment on
`body_fixed_to_planet_inertial_rotation` states explicitly.

Pseudovectors that cross a reflection are negated at the crossing:
`surface_velocity_in_planet_inertial` negates the ICRF angular velocity before
the solar mapping. A consequence of the reflected IAU boundary is that the
terrain-frame angular velocity points along **−north**; `planet_inertial_spin_axis`
returns the *geometric* pole instead, which is why
`planet_inertial_enu_basis` builds east as `up × spin` and says so in its doc
comment. Orbital elements are evaluated in the composed planet-inertial frame,
where cross products and the standard right-hand rule apply.

**Verdict.** No defect found. Every consumer of angular velocity in the solar
frame goes through the negating helper.

**Residual risk.** The simulated universe is a mirror image of the physical
one. All internal physics is consistent, but any future import of a signed
pseudovector (a magnetic-field vector, a published angular-momentum direction, a
mirrored texture) must cross the same reflection or it will have the wrong sign.
No test asserts `det(body_fixed → planet_inertial) = +1`, and the only external
azimuth check is the pinned SPICE launch-site case.

**The risk materialized.** Orbital inclination was measured about the geometric
pole, which is antiparallel to the rotation axis in this frame. See SC-08.

## SC-03 — Aerodynamic axes and center of pressure

**Model as found.** Body `+Y` is longitudinal. Angle of attack is
`atan2(−v_x, v_y)`, sideslip is `asin(v_z / |v|)`, drag opposes the velocity,
and the normal force is the component of body `+X` perpendicular to the flow
scaled by `1.5·α`. The force therefore always opposes the lateral flow
component, which the mirrored-force test pins.

**Two documentation defects.**
1. The module comment says the side force "completes that basis toward body
   `+Z`" while the sideslip function calls `+Z` "right". The frame never defines
   whether `+X` is dorsal or lateral, so "lift" and "side force" are labels that
   cannot be validated against a real vehicle; only the restoring direction is
   proven.
2. `center_of_pressure_m` returns `+0.25·height` along the longitudinal axis —
   nose-ward of the geometric centre — while the center of mass is fuel-weighted
   aft. That is a **destabilizing** static margin, which is realistic for a
   launch vehicle, but the function comment claims it "leaves static margin for
   the controller". The comment describes the opposite sign of the modelled
   physics.

**Not changed.** Correcting the comment would be cosmetic; correcting the
center of pressure would change the aerodynamic torque and therefore the three
committed baselines. It stays a modeling decision for a dedicated change.

## SC-04 — Per-thruster RCS allocation

**Gap as found.** Actuation clamps a commanded body torque to a scalar
`max_rcs_torque_nm` per axis (`clamp_rcs_torque`). There are no RCS thruster
entities, no torque-to-thruster allocation, no propellant draw, and no
minimum-impulse quantization. The controller gives gimbals the pitch/yaw
command and RCS the remainder including roll; grid fins, when fitted, are mixed
separately.

**Verdict.** Documented, honest limitation — the vehicle has torque authority on
three axes but not a thruster-level model. Landing and coast burns are therefore
not sensitive to thruster placement, and an asymmetric or failed thruster
cannot be represented.

## SC-05 — Closed-loop orbit insertion

**As found.** Ascent is an open-loop schedule: full throttle plus an
altitude/time gravity-turn ramp gated on clearing the tower. Insertion is a
bang-bang state machine: raise apoapsis with a prograde burn, coast, then
circularize when within 25 km and |ṙ| ≤ 25 m/s of the target apoapsis. The
completion test is `LowEarthOrbitTarget::matches_state_in_reference_frame`
(±25 km on both apsides, eccentricity ≤ 0.02, inclination within ±2°).

**Verdict.** PARTIAL. The loop closes on **acceptance**, not on the burn: there
is no throttle cut targeted at the apoapsis, no eccentricity or plane-error
correction law, and no post-insertion trim. A state that satisfies the tolerance
is accepted even if it was reached by overshoot.

## SC-06 — Falcon 9 provenance

**As found.** Every catalog entry is `DataBasis::Representative`, and validation
requires a non-blank `representative_rationale`. `falcon9.ron` states that the
numbers derive from `Rocket::falcon9_test_fixture()`, are "rough public
figures", and that "the dated source byte is not yet pinned, so no checksum is
claimed". The recorded source URL is the vehicle marketing page rather than a
dated document.

**Verdict.** Correctly declared as representative; not a scientific authority.

**Minor defect.** The file header lists the vacuum engine as "ISP 311 s (SL) /
348 s (vac)". 311 s is the *sea-level-equivalent catalog value* for that engine,
not a measured sea-level figure, and the label invites the wrong reading.

## SC-07 — WGS-84 datum audit

**Site conversion is correct.** Every site-anchoring path converts WGS-84
geodetic latitude to the terrain (geocentric/radial) latitude through the single
`geodetic_to_terrain_lat_lon` function: spawn, contact resolution, terrain layer
sampling and the regional DEM bake. Starbase converts 25.9971° → 25.8458°.

**Defect found — insertion targeting.** `RocketAutopilot::for_launch_site` was
fed the raw WGS-84 geodetic latitude, and its comment justified the result with
the spherical relation "a due-east launch reaches inclination |φ|". On an
ellipsoid the orbit plane contains the radial position vector, so the
inclination a due-east ascent can reach is the **geocentric** latitude. At
Starbase the target was 25.9971° for a plane the ascent can reach only at
25.8458°: a 0.151° error, about 17 km of ground track, hidden by the ±2°
acceptance tolerance.

**Fix.** `LaunchSetup` now carries the autopilot, built by
`autopilot_for_site(site, planet)`, which converts through
`geodetic_to_terrain_lat_lon` and then sets the inclination. A regression test
asserts the autopilot targets exactly the geocentric latitude, that Starbase's
two latitudes differ by more than 0.1°, and that the geocentric value is the
poleward one. `for_launch_site` keeps its arithmetic and its tests; only its
contract is now stated as geocentric input.

**Residual — ENU "up".** `enu_basis` builds `up` as the radial direction at the
*geodetic* latitude, not the WGS-84 ellipsoid normal. So
`body_fixed_to_local_tangent` reports heights along a line tilted by up to
≈0.19° from a true geodetic up. The flight-critical paths do not use it (the
launch stand uses the terrain surface normal), and changing it would move every
local-tangent consumer, so it is recorded rather than changed.

**Runtime evidence.** The same headless ascent that measured SC-01 was analyzed
independently: the ascent plane converges to 25.81° inclination relative to the
rotation axis by t=120 s (geocentric Starbase 25.8458°, geodetic 25.9971°), with
a launch azimuth of 89.8° east of north. The target the autopilot now stores is
therefore the plane the vehicle actually flies.

## SC-08 — Orbital inclination reference axis

**Defect.** Inclination was computed as `acos(h · k / |h|)` with `k` =
`planet_inertial_spin_axis`, the **geometric north pole**. In the reflected solar
frame the body's rotation axis is `−k` (that is why
`surface_velocity_in_planet_inertial` negates the ICRF angular velocity, and why
the local eastward direction is `up × k`). A physically correct eastward ascent
therefore has `h` antiparallel to `k` and the sim reported **180° − i**.

Measured on a real headless ascent (Falcon 9, Starbase, 900 s): azimuth 89.8°
east of north, inclination 154.19° about the geometric pole, 25.81° about the
rotation axis. Consequences:

1. `LowEarthOrbitTarget::matches_state_in_reference_frame` compared 154.19° with
   a 25.85° ± 2° target, so the plane condition could never be satisfied and the
   mission could not reach `Orbit` through insertion.
2. Orbital telemetry reported the supplement of the true inclination.
3. The safe-LEO regression fixture was itself built about the geometric pole, so
   its recorded "validated inclined circular orbit" was physically retrograde
   relative to the planet's rotation. It could never have caught this.

**Fix.** `planet_inertial_rotation_axis(orientation)` returns
`−normalize(icrf_j2000_to_solar_inertial(ω))` — the physical rotation axis — and
falls back to the geometric pole when a body has no usable angular velocity. It
is now the reference for `orbital_elements_from_state_in_reference_frame`
(telemetry, guidance and insertion checks). The geometric pole stays the
reference for `planet_inertial_enu_basis` and the launch-heading formula, which
empirically produce the correct prograde plane.

**Tests.**
- `rotation_axis_matches_the_surface_velocity_sense`: the axis is exactly
  antiparallel to the pole and reproduces `surface_velocity = |ω|·(axis × r)`.
- `inclination_is_measured_about_the_rotation_axis`: a circular equatorial
  prograde state reads 0° about the axis and 180° about the pole.
- The two ascent baselines are unchanged; the system-level insertion test still
  reaches `Orbit` and cuts thrust.

**Baseline change.** `tests/baselines/leo-insertion-safe.ron` was regenerated
with the documented `REGRESSION_RECORD=1` workflow. The initial position is
unchanged; the velocity flips to the prograde direction (the old recording flew
the mirror orbit) and the recording still ends in `Orbit` with thrust cut. Only
that fixture changed; `ascent` and `ascent-gravity-turn` match their committed
files.

**Remaining blocker.** Insertion still does not complete for the Falcon 9 ascent
in 900 s, because the apsis/eccentricity condition never matches — the ascent
over-burns to a 1,468 km, e=0.31 arc. That is SC-05 (no throttle cutoff law),
not a frame issue.

---

## Next steps, in order of physical impact

1. **SC-04 — RCS allocation.** A torque-level model cannot represent thruster
   placement, failure, or propellant cost. This is the largest remaining gap in
   the attitude loop.
2. **SC-05 — insertion law.** It is now the only reason a Falcon 9 ascent does
   not reach `Orbit`: the vehicle over-burns to a 1,468 km, e=0.31 arc. Replace
   bang-bang apoapsis chasing with a cutoff targeted at the apoapsis and a
   post-insertion eccentricity/plane trim; the acceptance predicate can then
   shrink below ±25 km and ±2°.
3. **SC-06 — pin the source byte.** Publish a dated Falcon 9 document (or an
   explicitly dated snapshot) with a checksum, then promote the entry to
   `SourceVerified`, or state that the vehicle is permanently representative.
4. **SC-03 — define the body lateral axes** in one place, then name the
   aerodynamic coefficients from that definition and correct the static-margin
   comment.
5. **Inclination convention checklist.** Every new consumer of a planet pole
   must state whether it needs the geometric pole (local east/north, J2
   symmetry) or the rotation axis (orbital elements, angular momentum). SC-08
   was exactly that confusion.

## Validation

The gate for this track is the project gate. Counts are the post-change values;
the previous floor was 1,202 with `dem` and 1,150 without.

| check | result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo check --all-targets --features dem` | PASS |
| `cargo check --no-default-features` | PASS |
| `cargo clippy --all-targets --features dem -- -D warnings` | PASS |
| `cargo test --features dem --lib` | 1,209 passed, 2 ignored |
| `cargo test --no-default-features --lib` | 1,157 passed, 2 ignored |
| three-mode 15 s smoke after the change | `solar`, `craft`, `rocket` all exit 124, zero hard-error matches |
| headless Falcon 9 ascent | 120 s run reproduced bit-identically; plane 25.81° about the rotation axis; insertion still blocked by SC-05 |
