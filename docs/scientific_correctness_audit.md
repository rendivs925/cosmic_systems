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
| SC-03 | Aerodynamic axes, angle-of-attack sign, and center of pressure | **FIXED — body axes defined; coefficients named by axis; lateral slopes equalised; CP comment corrected** |
| SC-04 | Per-thruster RCS allocation | **VERIFIED for nominal ascent/insertion — representative thruster set, allocation, failure, minimum impulse, propellant draw; recovery flight not re-flown** |
| SC-05 | Closed-loop orbit insertion | **FIXED — apoapsis-targeted cutoff, one circularization burn, verified in flight** |
| SC-06 | Falcon 9 data coherence and provenance | **FIXED — public Block 5 masses; source identity explicitly unpinned with a recorded reason** |
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

**Model as found.** Body `+Y` is longitudinal. The X-plane flow angle is
`atan2(−v_x, v_y)`, the Z-plane flow angle is `asin(v_z / |v|)`, drag opposes the
velocity, the normal force is the component of body `+X` perpendicular to the
flow scaled by `1.5·α_x`, and the side force is `lift_dir × v̂` scaled by
`−0.8·β_z`. Each force therefore opposes the lateral flow component in its own
plane, which the mirrored-force test pins.

**Defect 1 — the lateral axes were undefined.** The module comment said the side
force "completes that basis toward body `+Z`" while the sideslip function called
`+Z` "right". Neither `+X` nor `+Z` had a stated relationship to the vehicle or
to the frame built at spawn, so "lift" and "side force" were labels that could
not be checked against the body frame; only the restoring direction was proven.

**Fix 1.** The authoritative body basis now lives in
`rocket_dynamics::orientation_from_up_and_heading` and its module conventions:
`+Y` longitudinal (tail-to-nose, thrust and roll), `+X`/`+Z` the two lateral
axes with `X × Y = Z`; at spawn `+Y` is the pad normal and `+Z` the heading hint.
The airframe is axisymmetric and roll is unanchored in coast, so neither lateral
axis is a physical landmark. The aerodynamic model now names its quantities by
axis: the X-plane flow angle `α_x` drives the `+X` normal force `Cn_x`, and the
Z-plane flow angle `β_z` drives the `+Z` side force `Cn_z`. `angle_of_sideslip`
no longer claims `+Z` is "right". The module comment states the per-axis
convention explicitly.

**Defect 2 — the static-margin comment had the wrong sign.**
`center_of_pressure_m` returns `+0.25·height` along the longitudinal axis —
nose-ward of the geometric centre — while the modelled center of mass is
fuel-weighted aft. That is a **destabilizing** static margin, which is realistic
for a launch vehicle, but the comment claimed it "leaves static margin for the
controller". The comment now says the torque destabilizes the pitch/yaw axes and
the controller must actively stabilize them; a new test pins
`cop.y > com.y` for a fueled vehicle.

**Resolved — equal lateral slopes.** The coefficients used to apply `1.5·α_x` to
the X plane and `−0.8·β_z` to the Z plane. For an axisymmetric body with no fins
that anisotropy was arbitrary: roll about `+Y` is not anchored during coast, so
the same world-relative flow could rotate between the two planes and the model
was not roll-invariant. Both planes now use the common
`LATERAL_NORMAL_FORCE_SLOPE_PER_RAD = 1.5` (only the sign convention differs),
retaining the validated pitch-plane magnitude. A domain test pins equal
coefficient and force magnitudes for the same flow angle rotated 90° about the
longitudinal axis. The three committed baselines are unchanged: the gravity-turn
harness is pitch-plane only, so the yaw slope was unexercised there. The real 3D
Falcon 9 flight is also essentially unchanged (accepted orbit
201.1745 → 201.1783 × 199.9722 → 199.9759 km, final mass 23,858.333 →
23,858.331 kg), which confirms the nominal ascent is pitch-plane dominated; the
value of the change is roll invariance for off-nominal/3D flow, not a
trajectory improvement. No fixture was re-recorded.

**Residual — telemetry labels.** The presentation layer already names
`roll_rate = ω_y`, `pitch_rate = ω_x`, `yaw_rate = ω_z`, so the pitch-plane flow
is the Y-Z (Z) component. `telemetry.rs` nevertheless fills
`angle_of_attack_deg` from the X-plane function and `sideslip_angle_deg` from the
Z-plane function. That changes only the reported channels, not the force model,
but `simulation_analysis` consumes `angle_of_attack_rad` for its maximum-AoA
summary; correcting the mapping needs a deliberate review rather than a silent
swap.

**Not changed.** The center-of-pressure position itself is unchanged: moving it
would change the aerodynamic torque and the three committed baselines.

## SC-04 — Per-thruster RCS allocation

**Gap as found.** Actuation clamped a commanded body torque to a scalar
`max_rcs_torque_nm` per axis (`clamp_rcs_torque`). There were no RCS thruster
entities, no torque-to-thruster allocation, no propellant draw, and no
minimum-impulse quantization. The controller gave gimbals the pitch/yaw command
and RCS the remainder including roll; grid fins, when fitted, are mixed
separately.

**Root cause of the former authority demand (2026-11-09/10).** Lowering
`max_rcs_torque_nm` on the old code made the ascent fail: 2.0e4 lost control at
~T+145 s, 1.0e5 cut at 100 km and only 1,873 m/s, and 5.0e5 was the first stable
case. Instrumenting the fixed tick showed a 4.07 rad attitude error at t=0 with
the engine still off and a 38 MN·m torque command. Ascent guidance used
`attitude_from_direction`, a shortest-arc rotation from body +Y to the target
direction that leaves **roll arbitrary**. The pad attitude has a specific roll,
so guidance commanded a ~233° roll slew at liftoff. The huge RCS clamp existed
to execute that slew. `roll_preserving_attitude_from_direction` removed it: the
initial attitude error is now ~1e-16 rad and the default authority is 5.0e3 N·m,
a representative cluster scale.

**Implemented model (2026-11-11).** `domain::services::rcs` owns a representative
six-thruster pure-couple cluster, one thruster for each sign of each body torque
axis at a fixed 2 m arm. The catalogue has no public RCS stations or tank
geometry, so this is deliberately a representative set, not a hardware model.
`RcsCluster::allocate(torque_cmd, dt, available_propellant_kg)` is the single
owner of:

- **authority** — each thruster's full-duty torque equals the validated per-axis
  authority; `saturated` reports a command that could not be met because the
  axis is out of authority, a thruster failed, the minimum impulse dropped the
  pulse, or the tank could not supply the mass;
- **failure** — `set_failed` removes that sign's authority only (the opposite
  thruster and the other axes remain usable);
- **minimum impulse** — a pulse below 0.1 N·s is dropped and reported as not
  exact;
- **propellant draw** — mass flow from the allocated duties at the final stage's
  vacuum Isp, scaled down when the tank is the binding constraint.

`RocketRcs` is an optional component. Minimal harnesses without it keep the
validated per-axis clamp path; spawned vehicles get the cluster. `actuation_system`
does not pre-clamp the hardware path, so the authority rule exists in exactly one
place.

**Gimbal/RCS split.** Gimbals receive the primary pitch/yaw command in every
phase; RCS supplies the residual `τ_cmd − τ_gimbal`, which includes roll because
the gimbal allocator solves only the pitch (X) and yaw (Z) equations. This does
not reintroduce double authority: physics sums the gimbal torque produced by the
deflected thrust and the allocated RCS torque, and the two are constructed to add
up to the PID command once. The old ascent path sent the full command to both,
which doubled pitch/yaw authority and, with a real propellant draw, would have
burned RCS mass for torque the gimbals were already producing.

**Propellant accounting.** A single RCS tick draws far below the f32 resolution
of a ~100 t stage tank. `RocketRcs` accumulates the exact f64 draw and drains the
f32 inventory in 0.05 kg chunks; `propellant_used_kg` remains the exact total.
The drain is clamped by the tank, so the inventory can never go negative.

**Measured headless Falcon 9 (Starbase, 5,400 s, 5.0e3 N·m, deterministic).**

| quantity | pre-per-thruster (976f998) | post (this change) |
| --- | --- | --- |
| stage separation | T+144.219 s | T+144.219 s |
| ascent cut | T+448.219 s, 137.0897 km, 7,824.610 m/s | T+448.203 s, 137.0761 km, 7,824.236 m/s |
| circularization | T+1,727.672–1,728.875 s | T+1,719.453–1,720.672 s |
| accepted orbit | 200.847 × 199.238 km | 201.175 × 199.972 km |
| RCS propellant | not modelled | 42.488 kg |
| final mass at T+5,400 s | 23,901.016 kg | 23,858.333 kg |

The ±0.7 km apsis shift and the 8.2 s earlier circularization follow from an
ascent cut one tick earlier and 0.37 m/s lower, which is the expected effect of
carrying 42.5 kg less mass and splitting the pitch/yaw torque instead of doubling
it. The 42.683 kg final-mass difference is 42.488 kg of RCS propellant plus
0.195 kg more main-engine propellant in the slightly different trim burn. The
orbit remains inside every acceptance condition; this is attribution, not drift
that was rounded away.

**Tests.** Six domain tests pin the pure-couple geometry, allocation, saturation,
failure, minimum impulse, and tank-limited scaling. Two ECS tests run the real
`actuation_system`: a commanded X torque appears in `TorqueAccumulator` and
decreases the final-stage tank, and a failed +X thruster leaves no +X authority.
SC-04 adds those eight tests; with the two later SC-03/SC-06 guards the tree
measures 1,227 passed with `dem` and 1,175 without.

**Baselines.** `ascent-gravity-turn` was re-recorded through
`scripts/regression/save_baseline.sh` because the gimbal/RCS split changes the
pitch/yaw torque allocation in the recorded window: the old path applied the
full command to both actuators, so the earlier fixture contains small RCS-driven
rates that no longer exist. `ascent` (first 4 s) and `leo-insertion-safe` (a
vacuum insertion state) are unchanged. The baseline README records the reason.

**Residuals — implemented scope, not hardware or fully flown scope.**

1. The cluster is a representative pure-couple six-thruster set at a fixed 2 m
   arm. It is not a catalogued station layout, and the arm does not move with the
   center of mass.
2. There are no catalogued RCS tanks. Propellant is drawn from the final serial
   stage's main tank at that stage's vacuum Isp, which is a documented
   simplification of a separate RCS propellant system.
3. Thruster failure is representable in the model and tested, but no flight
   scenario exercises a failed thruster.
4. A complete booster-recovery flight was **not** re-flown at the new authority.
   The landing/recovery tests exercise the envelope, but recovery remains
   IMPLEMENTED-tested, not flight-verified at this allocation.

**Verdict.** VERIFIED for nominal ascent and insertion: the allocation, failure,
minimum-impulse, and propellant paths are unit- and system-tested and are
exercised by the 5,400 s headless flight. The four residuals above bound the
claim; recovery and station/tank fidelity remain outside it.

## SC-05 — Closed-loop orbit insertion

**As found.** Ascent is an open-loop schedule: full throttle plus an
altitude/time gravity-turn ramp gated on clearing the tower. Insertion is a
bang-bang state machine: raise apoapsis with a prograde burn, coast, then
circularize when within 25 km and |ṙ| ≤ 25 m/s of the target apoapsis. The
completion test is `LowEarthOrbitTarget::matches_state_in_reference_frame`
(±25 km on both apsides, eccentricity ≤ 0.02, inclination within ±2°).

**Verdict.** FIXED. The ascent cuts on the target apoapsis, one circularization
burn at that apsis raises the periapsis to the same radius, and the accepted
state lies inside every acceptance condition.

**Defect 1 — the cutoff targeted the wrong quantity.** The first fix cut the
ascent when the osculating semi-major axis reached the target orbital radius.
Energy is reached before the vehicle is high enough: the burn ended at 137.8 km
with the apoapsis already at 292 km, so the insertion phase inherited altitude
it had to remove, and a retro trim fired away from periapsis deorbited the
vehicle. The cut now targets the osculating **apoapsis** — the classical
profile. The ascent only has to raise one apsis, the coast reaches it, and the
circularization burn is a small prograde burn at that point.

**Defect 2 — the upper stage never relit for the trim.** `command_engine_lifecycle`
was not at fault: the engine was `Off` with a full ignition budget. The trim
law was. The first version started the circularization burn within 5 km of
apoapsis, one tick of thrust pushed the apsis outside that band, the next tick
commanded zero throttle and shut the engine down, and the following tick
re-ignited it. Four ticks consumed all three ignitions and left the engine
`Depleted`. Two changes close this:

- The burn latches on the authoritative running-engine state, so it continues
  until the periapsis reaches its own 1 km deadband instead of stopping on a
  band test.
- Acceptance no longer ends a trim burn that is in progress; the ±25 km
  predicate would otherwise cut the circularization burn at its lower edge
  (175.7 km periapsis in the measured run).

**Defect 3 — the burn started before the apsis.** A distance band at apoapsis
fires while the vehicle is still climbing, and a prograde burn then makes the
apoapsis recede; the periapsis can only chase the current radius and the burn
never terminates. A 200 m band still drifted the final apoapsis to 223 km. The
start is now the **radial-velocity crossing** (|ṙ| ≤ 0.5 m/s within 1 km of the
apsis), which places the ignition within a few metres of the true apsis. The
same test guards the retro overshoot at periapsis.

**Measured** (headless Starbase, 5,400 s of simulated time, deterministic):

| event | t | state |
| --- | --- | --- |
| stage 1 separation | 144.2 s | 40,600 kg shed, 112,898 kg upper stage |
| ascent cut | 448.2 s | 137.09 km, 7,824.6 m/s, apo 200.82 km, peri 68.60 km |
| circularization burn | 1,727.7–1,728.9 s | 276 kg consumed, one ignition |
| insertion accepted | 1,728.9 s | **200.85 × 199.24 km, e = 0.00012** |
| end of run | 5,400 s | 200.87 × 199.22 km, 23,901 kg |

The accepted state satisfies the full predicate: both apsides within ±25 km,
eccentricity ≤ 0.02, and the plane within ±2° of the Starbase geocentric
inclination (about the physical rotation axis, SC-08). The upper stage finishes
with about 18.0 t of propellant and one ignition still available. The slightly
later burn and 0.8 km apsis spread, compared with the pre-SC-04 5e7 run, are the
cost of the now-physical 5e3 N·m RCS authority; SC-05 still closes.

**Tests.** Four ECS guidance tests pin the law: a prograde burn at the apsis, the
running-burn latch away from the apsis, no burn before the apsis, and no burn on
a circular orbit. The ascent-predicate test was updated for the new handoff: an
unsafe periapsis now cuts the ascent one tick earlier and is handed to the trim
law, which keeps it under thrust.

**Residual.** The ascent pitch program is still open-loop (schedule plus the
radial-velocity-nulling trim above 80 km); the insertion law trims the apsides
but not the orbital plane. A plane error is only observed by the acceptance
predicate, never corrected by a burn.

## SC-06 — Falcon 9 data

**Defect.** The shipped Falcon 9 combined the Block 5 *thrust* (7,607 kN) with
the legacy fixture's masses (22.2 t dry, 120 t propellant). Liftoff mass was
143,950 kg — **5.4 g** — while the attitude PID gains are documented as
normalized to a ~549 t reference. At 5.4 g the 160 s gravity-turn program cannot
turn the vehicle horizontal: it reached 7.4 km/s on an e = 0.31 arc with a
negative periapsis, then burned to depletion.

**Fix.** Public Block 5 figures, applied to the catalog and the hardcoded fixture
together (the equivalence test enforces this, and the aggregate pins in the
tests follow): stage 1 25.6 t dry + 411 t propellant, stage 2 4.0 t dry + 107 t
propellant, fairing 1.9 t, 549.5 t liftoff; Merlin Vacuum 981 kN at 348 s. The
upper stage ignition budget is 3 so an insertion relight is at least nominally
possible. The entry remains `Representative`: public figures, no pinned source
byte.

**Measured.** Headless Starbase ascent: 549,500 kg at liftoff, T/W 1.41, stage
separation at T+144.2 s shedding 40,600 kg and leaving 112,898 kg of upper
stage. The three committed flight baselines use the Electron-like fixture, not
the catalog, so they are unchanged.

**Source identity (2026-11-11) — deliberately unpinned, with the reason
recorded.** The entry stays `Representative` and claims no `sha256`. The cited
URL is SpaceX's mutable vehicle page: HTML whose content changes without a
versioned, immutable edition. This offline workspace holds no retrieved snapshot,
so hashing the live page would name an artifact a later reader cannot retrieve.
Rather than fabricate a citation or a checksum, `falcon9.ron` now states that
reason in its header and in `provenance.representative_rationale`, and the test
`falcon9_records_that_its_web_source_cannot_be_byte_pinned` asserts the
`Representative` basis, the absent hash, and the recorded reason. This is the
explicit "byte pinning is not possible for this public web source" outcome, not
a silent gap.

Reproducibility is pinned at a different layer: the loader hashes the
configuration file into `configuration_sha256`, and every simulation run identity
records it. That pins the *vehicle definition bytes* the trajectory was flown
with; it does not make the external source `SourceVerified`.

**Remaining limitation.** The representative trajectory means every number above
is conditional on the assumptions listed in the file header. A future dated,
immutable source artifact could promote the entry, but that requires a real
archived document, not a new hash over the same mutable page.

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
reference for `planet_inertial_enu_basis` and the launch-heading formula. That
combination is not a convention accident: the new test
`launch_heading_achieves_the_target_inclination_about_the_rotation_axis` proves
that a horizontal launch along the returned heading produces an orbit whose
angular momentum is the requested angle from the **rotation** axis, for targets
from the site latitude through polar and retrograde, even though the local frame
is built from the geometric pole.

**Tests.**
- `rotation_axis_matches_the_surface_velocity_sense`: the axis is exactly
  antiparallel to the pole and reproduces `surface_velocity = |ω|·(axis × r)`.
- `inclination_is_measured_about_the_rotation_axis`: a circular equatorial
  prograde state reads 0° about the axis and 180° about the pole.
- `launch_heading_achieves_the_target_inclination_about_the_rotation_axis`:
  targets 28.5°, 45°, 97° and 135° from a 28.5° site all reach the requested
  rotation-axis inclination.
- The two ascent baselines are unchanged; the system-level insertion test still
  reaches `Orbit` and cuts thrust.

**Baseline change.** `tests/baselines/leo-insertion-safe.ron` was regenerated
with the documented `REGRESSION_RECORD=1` workflow. The initial position is
unchanged; the velocity flips to the prograde direction (the old recording flew
the mirror orbit) and the recording still ends in `Orbit` with thrust cut. Only
that fixture changed; `ascent` and `ascent-gravity-turn` match their committed
files.

**Follow-up.** With that fix the Falcon 9 ascent could reach `Orbit` for the
first time, but it over-burned to a 1,468 km, e=0.31 arc because the ascent had
no cutoff law. That was SC-05, now closed; the final orbit is 200.7 × 200.0 km.

---

## Pole convention checklist

Every consumer of a planet pole must state which one it needs. The authorities
are in `reference_frames`; this table is the review checklist for new code.

| consumer | reference | reason |
| --- | --- | --- |
| `planet_inertial_enu_basis`, `prograde_ascending_node_launch_heading` | geometric pole (`planet_inertial_spin_axis`) | defines east `= up × pole` and north toward the pole; the returned azimuth still reaches the requested inclination about the rotation axis (tested) |
| `surface_velocity_in_planet_inertial` | rotation sense from the negated transformed ICRF ω | eastward surface velocity |
| `orbital_elements_from_state_in_reference_frame`, insertion acceptance, plane-change Δv | physical rotation axis (`planet_inertial_rotation_axis`) | inclination is defined about the angular-momentum axis by the right-hand rule |
| `planet_equatorial_reference_x_axis` | body-fixed +X transformed at the current orientation | in-plane phase origin for node/periapsis; it is a phase convention, not an inclination reference |
| J2 acceleration, zonal harmonics | geometric pole | J2 is symmetric under axis reversal, so the sign is irrelevant; the full spherical-harmonic field is evaluated in the body-fixed frame, not from a pole |
| sky/planet presentation, surface maps | body-fixed rotation | visual state only; never feeds orbital elements or guidance |

The rule for new consumers: if the quantity is an orbital element or an
angular-momentum comparison, use the rotation axis; if it is a local
east/north/up frame or an axisymmetric gravity term, use the geometric pole.
Record the choice in the consumer's doc comment.

---

## Recorded residuals (deliberately not hidden)

These are known remaining gaps. None is called VERIFIED; each stays IMPLEMENTED
or GAP in the scope where it is untested or unflown.

1. **Insertion trims apsides only.** The circularization law raises the
   periapsis to the target radius; it never commands a plane change. A
   plane/inclination error is observed by the acceptance predicate (~±2° at
   Starbase) and is accepted, not corrected. Fixing it needs a plane-change burn
   and a new baseline review.
2. **SC-07 ENU residual.** `enu_basis` builds `up` from the radial at the
   *geodetic* latitude, not the WGS-84 ellipsoid normal, so local-tangent heights
   are tilted by up to ≈0.19°. Flight-critical paths do not use it (the launch
   stand uses the terrain surface normal). Changing it would move every
   local-tangent consumer, so it remains recorded.
3. **Solar captures.** Offscreen solar screenshots are black. Windowed F12 frames
   are the current evidence path; this is a presentation/capture limitation, not
   a simulation result. It must not be cited as visual verification of anything
   else.
4. **RCS fidelity and recovery.** The representative station/tank model and the
   unflown booster-recovery case are the SC-04 residuals above. They remain
   implemented/tested scope, not VERIFIED hardware or flight.

## Next steps, in order of physical impact

1. **Trim the orbital plane.** The insertion law now closes both apsides; the
   remaining orbital-element error is inclination. A bounded plane-change burn
   (or an ascent-azimuth correction) would close it, but it changes every
   insertion baseline and needs the same measured A/B used above.
2. **RCS station/tank fidelity and a recovery flight.** Replace the representative
   single-arm cluster and final-stage tank draw with a catalogued layout, then fly
   the booster recovery at the new authority. Until then SC-04's recovery scope
   is IMPLEMENTED-tested, not flight-verified.
3. **SC-06 source promotion.** Requires a dated, immutable Falcon 9 document or an
   archived snapshot with a checksum; the current mutable vendor page cannot be
   pinned honestly.

## Validation

The gate for this track is the project gate, run on `70bfef5`, the last code
commit of the three changes (the documentation commit that records it changes no
code). Counts are the post-change values; the previous floor was 1,217 with
`dem` and 1,165 without.

| check | result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo check --all-targets --features dem` | PASS |
| `cargo check --no-default-features` | PASS (4 pre-existing unused-import/dead-code warnings) |
| `cargo clippy --all-targets --features dem -- -D warnings` | PASS |
| `cargo test --features dem --lib` | 1,227 passed, 0 failed, 2 ignored |
| `cargo test --no-default-features --lib` | 1,175 passed, 0 failed, 2 ignored |
| `openspec validate --all --strict` | 43 passed, 0 failed |
| `cargo build --release --features dem` | PASS |
| three-mode 15 s smoke after the change | `solar`, `craft`, `rocket` all exit 124, zero `ERROR` or panic matches |
| headless Falcon 9 flight (Starbase, 345,600 ticks = 5,400 s) | ascent cut at T+448.203 s (137.076 km, 7,824.236 m/s); one circularization burn T+1719.5–1720.72 s; accepted 201.1783 x 199.9759 km (e ≈ 9.1e-5 from the apsides); 42.483 kg RCS propellant; 23,858.331 kg at T+5,400 s |
| RCS authority A/B | roll-slew fix (`0e333b0`): 5.0e7 / 5.0e6 / 1.0e6 / 5.0e5 N·m all hand off near 137.7 km, 1.0e5 cuts at 100 km/1,873 m/s, 2.0e4 loses control at ~T+145 s; after it, 5.0e3 and 2.0e4 circularize and 100 N·m still reaches the ascent cut. Per-thruster split (`70bfef5` vs `976f998`): accepted 201.1783 x 199.9759 vs 200.847 x 199.238 km, RCS 42.483 vs 0 kg, final mass 23,858.331 vs 23,901.016 kg |
