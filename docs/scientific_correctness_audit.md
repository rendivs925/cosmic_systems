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
| SC-03 | Aerodynamic axes, angle-of-attack sign, and center of pressure | **FIXED — body axes defined; coefficients named by axis; lateral slopes equalised; retrograde 180° normal-force fold; CP comment corrected** |
| SC-04 | Per-thruster RCS allocation | **VERIFIED for nominal ascent/insertion — representative thruster set, allocation, failure, minimum impulse, propellant draw; recovery attitude grid-fin controlled, landing still unverified (energy)** |
| SC-05 | Closed-loop orbit insertion | **FIXED — apoapsis-targeted cutoff, one circularization burn, verified in flight; plane error measured at 0.0014°** |
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
velocity with one constant `Cd = 0.3` for either end of the airframe, the normal
force is the component of body `+X` perpendicular to the flow scaled by
`1.5·α_x`, and the side force is `lift_dir × v̂` scaled by `−0.8·β_z`. Each force
therefore opposes the lateral flow component in its own plane, which the
mirrored-force test pins.

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

**Defect 3 — the small-angle normal-force law was applied at 180° (fixed
2026-11-12).** `angle_of_attack` reports the signed angle from body `+Y`, so an
engines-first booster — the recovery stage's `EntryDescent` attitude — has
`α ≈ ±π`. The coefficient `Cn_x = 1.5·α_x` then commanded a lateral force about
`π` times the small-angle value on a perfectly axial flow, along the arbitrary
body `+X` direction. The Z side force used `lift_dir × v̂`, whose sign also
reverses for retrograde flow. In other words, the recovery entry was not the
zero-lift body the guidance assumed.

The fix folds the X-plane angle to the windward body end
(`folded_normal_flow_angle`): it is identical for `|α| ≤ 90°` and continuous at
±90°, so prograde flight is bit-for-bit unchanged, while a 180° axial flow maps
to zero. `side_force_body` now uses the nose-ward axial unit for its cross
product, so retrograde sideslip is restored instead of augmented. New domain
tests pin zero lateral force at retrograde axial flow, restoring force in both
lateral planes, and the unchanged prograde slope. The three committed baselines
pass unchanged because the gravity-turn ascent never enters the folded branch.

Measured on the committed 5,400 s Starbase profile: the recovery stage now holds
`α ≈ 179.7°` (0.3° from axial) with body rate `≈ 0` through the dynamic-pressure
rise that peaks at 870 kPa, and publishes one crash at T+501.078 s (652.94 m/s
vertical, 379.59 m/s lateral, 26.82° tilt, 710.88 km short, 0 kg propellant).
The previously recorded T+597.062 s crash (325.82 m/s, 745.71 km short) was
cushioned by the spurious `π·slope` lateral force; the corrected model is
steeper and faster. The remaining recovery failure is propellant/entry energy,
not a tumble.

**Defect 4 — the axial drag coefficient ignored the windward end (fixed
2026-11-12).** `drag_force_body` used one constant `Cd = 0.3` regardless of which
end of the airframe faced the flow. A booster entering engines-first does not
present its pointed nose; it presents the blunt base — engine section and, on a
recoverable stage, the entry heat shield. A flat-faced blunt body in the
supersonic regime carries `Cd ≈ 1.0–1.2` on its frontal area (Hoerner-style
flat-face cylinder data), while a streamlined nose is near `0.3`. The model also
made the recovery stage *less* draggy than the generic spent-stage debris path,
which already used `SPENT_STAGE_DRAG_COEFFICIENT = 0.9`.

The fix keeps the nose-first value and blends toward a blunt-base value by the
base-facing axial flow fraction,
`w = max(0, −v_y/|v|)`:
`Cd = Cd_nose·(1 − w) + 1.1·w`. Nose-first and transverse flow return the
original coefficient exactly (`w = 0`), so the ascent, gravity-turn, and vacuum
insertion paths are bit-for-bit unchanged; axial base-first flow returns `1.1`
exactly, and the blend is continuous across ±90°. Nose-bluntness growth applies
only to the nose-first term because a base-first entry presents the heat shield,
not the nose. Four domain tests pin the flow weight, the exact prograde value,
the blunt-base value, and the transverse/continuity behavior.

Measured on the committed 15 t return-to-launch profile (same separation,
insertion, and guidance; base drag is the only change): the crash moves from
652.94 m/s vertical / 379.59 m/s lateral / 26.82° tilt to **186.15 m/s vertical
/ 31.82 m/s lateral / 8.73° tilt** at T+501.078 s, with 0 kg propellant and the
pad still 706.19 km away. The stage now decelerates through the dense
atmosphere instead of arriving at 650 m/s; the residual failure is the RTLS
energy budget, not the entry deceleration. A 25 t downrange variant re-flown with
the same term reaches powered descent at 4.86 km with 3,989 kg and 282 m/s
(versus a 596.83 m/s impact without it), which is the first measured recovery
state with enough altitude for a terminal burn.

**Residual — large-angle crossflow.** The fold fixes the 180° axial case, and the
blunt-base blend fixes the axial coefficient, but the underlying lateral law and
the `lift_direction` basis are calibrated for small angles; near 90° they remain
an analytic approximation, not tabulated crossflow data. The base drag is a
representative flat-face value, not a wind-tunnel measurement of this airframe,
and the model still omits the base-pressure rise a retro plume produces (the
SC-01 thrust side of that interaction is applied separately).

## SC-04 — Per-thruster RCS allocation

**Gap as found.** Actuation clamped a commanded body torque to a scalar
`max_rcs_torque_nm` per axis (`clamp_rcs_torque`). There were no RCS thruster
entities, no torque-to-thruster allocation, no propellant draw, and no
minimum-impulse quantization. The controller gave gimbals the pitch/yaw command
and RCS the remainder including roll. Grid fins were a domain-only mixer at the
time; they are now a separate atmospheric actuator on the recovering stage (see
the grid-fin residual below).

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
SC-04 adds those eight tests; the recovery hardening adds two more (a
landing-gear vehicle must not deploy chutes, and a crashed wreck must publish one
terminal event), the contact-impulse fix adds three domain tests (central,
offset-spin, and separating contact), the pad-target wiring adds one ECS test
(the target follows one hour of Earth rotation and a moving deck is not
overwritten), and the grid-fin/entry work adds nine (six domain allocation
tests, two ECS force/torque tests, one guidance handoff test) while replacing
the three old normalized-mixer tests with one effectiveness test, a net gain of
seven. At the grid-fin commit the tree measured 1,240 passed with `dem` and
1,188 without; the center-engine commit adds two affine-trim tests and raises the
floor to 1,242 and 1,190.

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
4. The recovery flight is now flown (2026-11-11) with a controlled entry, but it
   does not verify a landing. The stage inherits the launch site and guidance
   re-derives the rotating pad target every tick (tested: one hour of Earth
   rotation moves the target by more than 100 km). It also inherits a
   representative `RocketRcs` cluster, and it carries four grid fins that take
   the atmospheric attitude command ahead of RCS. With the 15 t reserve the
   flight enters engines-first, holds under 0.5 rad/s through the dynamic-
   pressure rise, and crashes at T+594.828 s (342.66 m/s descent, 31.13° tilt,
   744.67 km short of the pad, 0 kg propellant). Raising the reserve to 25 t
   preserves insertion and improves the crash to T+553.813 s, 305.62 m/s,
   37.65° tilt, 549.01 km short — still not a landing. 40 t breaks the ascent
   (stage separation at T+135.1 s, Ascent→Reentry at 116.7 km). The committed
   15 t reserve keeps the ascent and baselines unchanged; the remaining blocker
   is recovery energy and burn sequencing, not attitude authority.
5. The contact authority now applies a linear-plus-angular contact impulse at a
   crash: `RocketDynamicsState::contact_normal_impulse` and
   `apply_contact_impulse` use the contact-point effective mass, so the same
   impulse changes translation and rotation. Before the fix a 185.25 rad/s wreck
   spin was corrected only at the center of mass and launched the wreck onto a
   739 km arc; after it the wreck stays on the surface. Three domain tests pin
   the central, offset-spin, and separating cases.
6. **Grid-fin fidelity.** The four-fin X installation is now a real force and
   torque actuator: the static geometry comes from the catalog, the normal-force
   slope is tapered by the existing hypersonic effectiveness curve, and control
   gives the fins the gimbal residual before RCS. The area, radial arm, axial
   station, and normal-force slope are representative rather than wind-tunnel
   values; the model omits induced axial drag and has an ideal hinge (no rate
   limit or actuator lag). These are recorded limitations, not hidden physics.
7. A crash over water is treated as a crash on the sea-floor terrain height, not
   at the sea surface. The measured recovery impacts in the Gulf of Mexico and
   finishes on the sea floor at roughly −3.4 km terrain height; there is no
   water-impact, splashdown-at-surface, or buoyancy model.

**Recovery flight measured after the 2026-11-11 grid-fin fixes.** The same
5,400 s Starbase flight spawns the recovery entity at T+144.219 s, gives it the
launch site and a representative RCS cluster, and inserts four grid fins. The
15,000 kg representative reserve is consumed by boostback by T+180 s (40.6 t to
25.6 t) and the stage reaches a 171.7 km ballistic apex. Boostback hands off to
the new engines-first `EntryDescent` mode when the reserve is exhausted; through
the dynamic-pressure rise (T+435 to T+500) the measured body rate stays below
0.5 rad/s and the airframe holds the retrograde axial attitude (α ≈ 179.7°,
0.3° from axial, after the 2026-11-12 fold fix). The wreck publishes one
terminal crash at T+501.078 s: 652.94 m/s descent, 379.59 m/s lateral, 26.82°
tilt, 0.897 rad/s angular rate, 710.88 km short of the pad. Before the fins the
same flight crashed at T+510.234 s with 1,040.52 m/s descent and 208.68 rad/s.
At T+5,400 s it is `Crashed` with 0 kg propellant, ~0 m radar altitude and
409.78 m/s, which is the Earth-rotation surface speed at the crash latitude; the
position is the Gulf sea floor, not the sea surface (residual 7). Before the
aero fold, the 25 t variant crashed at T+553.813 s with 305.62 m/s descent and
549.01 km still to go, so additional propellant alone does not close the return:
the profile needs an entry burn and a landing-burn allocation, or a downrange
recovery zone. The 2026-11-12 aero fold removes the spurious lateral force; the
impact is steeper and faster than the pre-fold 325.82 m/s measurement, and the
stage still has no propellant for a terminal burn.

**2026-11-12 update.** The center-engine commit re-flies the committed profile:
separation and insertion are unchanged, while the crash moves to T+597.062 s
(325.82 m/s vertical, 245.98 m/s lateral, 34.26° tilt, 745.71 km short). The
subsequent aero fold fix (SC-03 defect 3) removes the spurious retrograde
lateral force and re-flies the same profile to a T+501.078 s crash (652.94 m/s
vertical, 379.59 m/s lateral, 710.88 km short) with the stage holding
engines-first at `α ≈ 179.7°` and body rate `≈ 0` through the dynamic-pressure
rise. The pre-fold 25 t and downrange-deck experiments below are therefore
superseded as evidence about *attitude authority*: their tumbles and skips
included the `π·slope` artifact. They still fail after the fold, but the
remaining blocker is entry energy and trajectory shaping, not a fin/torque
authority shortfall.

**Recovery-burn experiment (2026-11-11, not committed).** Before changing the
committed recovery profile, the next options were measured in isolated flights.
None produced a landing, and two exposed hardware/control preconditions. These
flights predate the SC-03 retrograde-fold fix, so their attitude/tumble numbers
are not clean evidence about actuator authority; the hardware findings (no
center engine, single-engine gimbal offset) stand, while the entry-attitude
conclusions must be re-flown before they are re-used:

- A landing burn that lights all nine engines is uncontrollable. The stage burned
  9.2 t while its speed *rose* from 597 to 774 m/s; the thrust direction
  oscillated with a velocity dot product between −0.99 and +0.99 and the stage
  crashed at 1,236 m/s. The terminal controller cannot hold a nine-engine stage
  through a hover-slam.
- A single ring engine cannot trim its own thrust offset. At the 1.2 m ring
  radius, the static torque is about 1.0 MN·m and needs roughly 14° of gimbal to
  cancel; the Merlin installation is limited to 5°. An affine gimbal allocator
  (correctly inverting the nonzero zero-deflection torque) was written and
  tested, but the hardware itself cannot do it. The catalog also had no center
  engine; correcting it to one center plus eight ring engines is a prerequisite
  for a one-engine landing, not a solution by itself.
- Reallocating propellant from boostback to a landing reserve raises the entry
  speed. With a 10 t boostback (2,759 m/s entry) the stage tumbled at 38 rad/s
  despite the grid fins; the committed full 15 t boostback enters at 2,476 m/s
  and stays below 0.5 rad/s. The attitude margin between those two cases is the
  binding constraint.
- A three-engine entry burn from 70 km to 35 km consumed the entry reserve but
  did not stabilise the higher-speed entry. A pre-landing 180° flip with a rate
  limit and a 15° ignition interlock still left the stage tumbling and the
  landing engine unlit.
- After the fold, a three-engine burn from the descent interface (170 km to
  40 km) lights and removes 846 m/s for 16 t, but the stage still re-accelerates
  to 2.18 km/s and peaks near 950 kPa; the 25 t budget cannot both slow the
  stage and leave a terminal reserve. A bare vertical-speed threshold chattered
  at apogee and spent four ignitions in 0.3 s, so the burn needs a latch; the
  latched experiment is recorded but not committed.

The pre-fold lesson was that recovery energy could not be fixed by propellant
allocation alone because the higher entry speed exceeded the entry-attitude
authority. The 2026-11-12 fold fix changes half of that: the booster can now
hold retrograde axial flight at 2,300+ m/s with almost no body rate, so the
attitude authority is no longer the first-order limit. What remains is an energy
and trajectory-shaping problem: the stage still reaches the dense atmosphere at
2,200-2,400 m/s on a −30° to −40° flight path and peaks near 870 kPa dynamic
pressure, leaving either no propellant or no altitude for the terminal burn.
The committed 15 t profile is retained until an entry-burn/lift-management law is
implemented and flown. The experiment is not a committed code path.

**Recovery engine lifecycle at the reserve floor (fixed 2026-11-12).** During
ascent the active core stage deliberately withholds
`recovery_propellant_reserve_kg` from the burnable inventory.
`command_engine_lifecycle` tested only `has_burnable_propellant()` and therefore
called `engine.deplete()` when the burnable tank reached zero, even though the
stage still held its recovery reserve. The separated stage inherits that clone,
so a 25 t recovery profile could not light its boostback or entry burn: the
measured flight showed all nine engines `Depleted` with 24 t still in the tank.
A recoverable stage now shuts its engines down (`Off`, restartable) at the
reserve floor and depletes only when the tank is truly empty; an expendable
stage is unchanged. Two tests pin the recoverable shutdown/restart and the
expendable depletion. The committed 15 t profile is unchanged because its
engines were already `Off` at separation; the 25 t profile can now ignite.

**Downrange entry-burn measurement and target-authority fix (2026-11-12,
experiment not committed).** With the lifecycle fix in place, a post-fold
downrange profile was re-flown: a 25 t reserve, no boostback (recovery starts
in `EntryDescent`), and a three-engine entry burn gated on the descending leg
(70 km to 38 km). The burn lights, consumes 20.0 t in 22 s, and removes about
640 m/s (2,525 to 1,881 m/s) while the SC-01 base-pressure correlation applies
its 0.50 thrust floor at Mach 7–8. The stage still crosses 6.7 km at 1.18 km/s
and 612 kPa, and the 4 t terminal reserve cannot remove the remaining vertical
speed: the measured impact is 596.83 m/s vertical, 357.01 m/s lateral, 22.74°
tilt. Leaving a larger terminal reserve does not close the budget either: a
shorter entry burn arrives faster through the same 6 km of atmosphere. A
landing needs an entry-lift/flight-path law or the missing large-angle
crossflow/base drag (SC-03 residual), not a different propellant split.

The experiment also exposed a target-authority defect. The recovery autopilot
is cloned from the ascent autopilot, so it inherited the pad target even when
no launch site was attached. In the downrange flight that target stayed about
935 km away and terminal guidance spent its reserve steering toward it. The
recovery stage now clears `target_landing_position_m` at separation and takes
its target from the authoritative recovery input: the launch site re-derived
each tick for a return-to-launch flight, or a moving deck. The separation test
inserts a non-zero ascent target and asserts the recovering stage starts at
zero. The committed 15 t return-to-launch flight is unchanged because guidance
re-derives the same pad position on the first tick; the re-flight reproduces
the T+501.078 s crash.

**Terminal divert law (implemented 2026-11-12).** The vertical terminal law's
non-braking branch held a 3–45 m/s descent-rate target, so a stage arriving
at 5 km with a large horizontal component had no way to null it: the law
commanded almost no horizontal acceleration above the braking window.
`terminal_landing_guidance` now has a three-dimensional retrograde divert above
25 m/s. It coasts until a maximum-thrust retro burn can null the velocity within
the remaining distance to the target (`v² / 2(a_max − g·v̂)`), then points the
engine opposite the velocity vector; below 25 m/s the existing tilt-limited
terminal law settles the vehicle. This is the only phase that can remove a large
horizontal component without tilting the main engine far from the deck normal.
It requires a target on the natural ground track — a downrange deck placed where
the stage would otherwise impact — not a pad the stage has already overflown.
Four domain cases pin the prograde/transverse coefficient behavior and the
divert: brake inside the stopping distance, coast beyond it, retrograde attitude,
and the unchanged sub-25 m/s settle. The adapter now builds the terminal target
with `roll_preserving_attitude_from_direction`, so the divert does not command a
roll slew. No baseline changes; the committed RTLS flight still crashes at
T+501.078 s (186.15 m/s vertical, 31.82 m/s lateral, 8.73° tilt, 706.19 km short,
0 kg propellant) because its pad is 706 km away and the stage is out of
propellant.

A downrange attempt with a body-fixed sea-floor site reached the terminal phase
but did not land: the stage crosses a continental-shelf cell whose terrain is
3.3 km above the trench-floor target, so ground contact happens before the
target. That is the missing water-surface/flat-deck input (residual 7), not the
divert law. A deck at a fixed deck plane is the correct next mission input; the
model already has `DroneShipLandingTarget` and deck-relative contact.

**Verdict.** VERIFIED for nominal ascent and insertion: the allocation, failure,
minimum-impulse, and propellant paths are unit- and system-tested and are
exercised by the 5,400 s headless flight. The retrograde aero fold and the
blunt-base drag term are domain-tested and re-flown: the booster now enters
engines-first at `α ≈ 179.7°` with body rate `≈ 0` through the high-dynamic-
pressure rise, and base-first drag cuts the committed RTLS impact from 652.94 to
186.15 m/s. A landing is still not verified; the committed return-to-launch
profile is 706 km short with no propellant. The residuals above bound the claim:
recovery energy sequencing, downrange terminal guidance, entry trajectory
shaping, and station/tank fidelity remain outside it.

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

**Measured** (headless Starbase, 5,400 s of simulated time, deterministic; state
after the per-thruster RCS and equal-slope aero changes at `70bfef5`):

| event | t | state |
| --- | --- | --- |
| stage 1 separation | 144.2 s | 40,600 kg shed, 112,898 kg upper stage |
| ascent cut | 448.203 s | 137.076 km, 7,824.236 m/s, apo 200.164 km, peri 67.966 km |
| circularization burn | 1,719.5–1,720.72 s | 280.6 kg consumed, one ignition |
| insertion accepted | after the trim cutoff | **201.1783 × 199.9759 km, e ≈ 9.1e-5** |
| end of run | 5,400 s | 201.1783 × 199.9759 km, 23,858.331 kg, 34,860.5 kg propellant |

The accepted state satisfies the full predicate: both apsides within ±25 km,
eccentricity ≤ 0.02, and the plane within ±2° of the Starbase geocentric
inclination (about the physical rotation axis, SC-08). The 2026-11-11 diagnostic
measures that plane directly: achieved inclination **25.84445°** against the
**25.84579°** geocentric target, an error of **0.00135° (2.4e-5 rad)**. The ±2°
band is a tolerance, not a consumed error. The apoapsis sits 1.18 km above the
200 km target and the periapsis 0.02 km below it, which is the trim deadband
rather than a plane or energy failure.

**Tests.** Four ECS guidance tests pin the law: a prograde burn at the apsis, the
running-burn latch away from the apsis, no burn before the apsis, and no burn on
a circular orbit. The ascent-predicate test was updated for the new handoff: an
unsafe periapsis now cuts the ascent one tick earlier and is handed to the trim
law, which keeps it under thrust.

**Residual.** The ascent pitch program is still open-loop (schedule plus the
radial-velocity-nulling trim above 80 km). The insertion law trims the apsides
and not the orbital plane, but the measured plane error is 0.00135°, so the
missing plane burn is a capability gap for off-nominal insertion, not a consumed
error on this trajectory. The remaining insertion offset is the altitude
deadband recorded above.

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

**Engine layout correction (2026-11-12).** The nine first-stage engines were all
on the 1.2 m ring, so there was no center engine for a one-engine landing. The
catalog and the fixture now carry one center Merlin plus eight ring engines in
opposite pairs, and the first-stage ignition budget is five (ascent, boostback,
entry, landing, margin). The layout is symmetric, so the all-engine ascent force
and torque are unchanged; the catalog/fixture equivalence test pins it. The
three deterministic baselines use the Electron-like fixture and the full-stage
gimbal allocation is preserved bit-for-bit, so **no baseline was re-recorded**.
A recovery stage now carries an `EngineSelection` component; the committed
profile still selects all nine engines, so this is plumbing for a future
one-engine landing, not a recovery-profile change.

**Measured.** Headless Starbase ascent: 549,500 kg at liftoff, T/W 1.41, stage
separation at T+144.2 s shedding 40,600 kg and leaving 112,898 kg of upper
stage. Insertion still closes at 201.1783 × 199.9759 km. The three committed
flight baselines use the Electron-like fixture, not the catalog, so they are
unchanged. The center-engine layout changes the recovery stage's gimbal
allocation slightly; the committed recovery flight now crashes at T+597.062 s
(325.82 m/s, 34.26° tilt, 745.71 km short) instead of T+594.828 s (342.66 m/s,
31.13° tilt, 744.67 km short). That is a comparable controlled entry, not a
landing. The later 2026-11-12 retrograde aero fold (SC-03 defect 3) re-flies the
same profile to a T+501.078 s crash (652.94 m/s, 26.82° tilt, 710.88 km short);
insertion and separation remain unchanged.

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

1. **Insertion trims apsides only, and the plane needs no trim.** The
   circularization law raises the periapsis to the target radius; it never
   commands a plane change. The 2026-11-11 flight measured the achieved
   inclination at acceptance as 25.84445° against the 25.84579° geocentric
   target, an error of 0.00135° (2.4e-5 rad), so the ±2° acceptance band is
   tolerance rather than observed error. The trim leaves a +1.18 km / −0.02 km
   apsis offset from the 200 km target, which is a control deadband. A
   plane-change burn remains a capability gap for off-nominal insertion, not a
   correction this trajectory needs.
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
   booster-recovery case are the SC-04 residuals above. The committed profile now
   enters engines-first under grid-fin control (α ≈ 179.7°, body rate below
   0.5 rad/s through the dynamic-pressure rise), but it does not land: boostback
   consumes the 15 t reserve, the stage is still 710.88 km short, and the impact
   at T+501.078 s is a crash at 652.94 m/s. A 25 t reserve improves the crash to
   549.01 km short and 305.62 m/s, but still does not land; 40 t breaks the
   ascent. The 2026-11-12 follow-up, using the center engine and
   `EngineSelection` plumbing, flew two variants before the aero fold:
   - **25 t RTLS.** Insertion is preserved (200.526 × 199.353 km). The stage
     reaches powered descent with about 4.2 t left, but crashes with 195.85 m/s
     vertical, 222.57 m/s lateral and 39.05° tilt, 520.54 km short.
   - **Downrange deck.** No boostback; a three-engine entry burn from 60 to 25 km
     consumes 25.4 → 5.07 t and slows 2537 → 1890 m/s, then the stage crashes
     158–622 km from the deck.

   Re-flown after the fold, the downrange entry holds retrograde (α ≈ 180°,
   body rate ≈ 0) but still reaches 2.2 km/s at 15 km and peaks near 900 kPa
   dynamic pressure; the three-engine entry burn is efficient once aligned but
   the 25 t budget cannot both slow the stage and leave a terminal reserve. The
   binding constraint is entry energy and flight-path shaping, not propellant
   mass, engine layout, or fin authority. The center-engine correction, subset
   plumbing, and aero fold are committed; the experimental burn profiles are
   not. Recovery energy sequencing remains IMPLEMENTED-tested, not VERIFIED
   hardware or flight.

5. **Water impact.** A crash over water resolves against the sea-floor terrain
   height, not the sea surface. The measured recovery finishes on the Gulf sea
   floor at about −3.4 km terrain height. There is no water-impact, surface
   splashdown, buoyancy, or sinking model; a crash over water is not a water
   landing.

## Next steps, in order of physical impact

1. **Recovery entry-energy and flight-path shaping.** The 2026-11-12 aero fold
   removed the spurious retrograde lateral force, and the booster now holds
   engines-first (`α ≈ 179.7°`, body rate `≈ 0`) through the dynamic-pressure
   rise. It still does not land: with 25 t it reaches the dense atmosphere at
   2.2 km/s on a −30° to −40° path and peaks near 900 kPa, so the three-engine
   entry burn either stops at the landing reserve or leaves no terminal altitude.
   The next step is an entry-lift/flight-path law (bank/AoA modulation) plus a
   deliberate entry-burn/landing split, and a downrange recovery zone sized to
   the actual ballistic range. The center-engine layout and aero fold are
   committed; the three deterministic baselines are Electron-only and the
   prograde aero path is preserved bit-for-bit, so no baseline was re-recorded.
2. **Water impact.** Resolve a water contact at the sea surface and distinguish a
   survivable splashdown from a crash, instead of always using the sea-floor
   height. This affects any coastal or offshore landing.
3. **RCS station/tank fidelity.** Replace the representative single-arm cluster
   and final-stage tank draw with a catalogued layout. The current model remains a
   documented simplification; the recovering stage now uses the same cluster path
   as the primary.
4. **SC-06 source promotion.** Requires a dated, immutable Falcon 9 document or an
   archived snapshot with a checksum; the current mutable vendor page cannot be
   pinned honestly.
5. **Insertion deadband (optional, low impact).** The plane error is measured at
   0.00135°, so a plane-change burn is not justified by this flight. The visible
   insertion offset is the altitude deadband (+1.18 km apoapsis). Tightening it
   is a control change with a baseline review; treat it as polish, not physics.
   A plane-change law would still be needed for off-nominal or plane-corrected
   insertion, but nothing in the current flight asks for one.

## Recovery deck and downrange entry profile

**Historical status at the deck introduction: IMPLEMENTED and measured.** A
specific 15 t deck landing is now VERIFIED below under **Deck touchdown
verification — 2026-11-14**. The
recovery target can now be a body-fixed sea-surface deck supplied with
`--recovery-deck <latitude>,<longitude>`. The deck is a simulation asset, not a
presentation prop:

- The input is **terrain-radial** latitude/longitude on the catalog mean radius,
  the same convention as the terrain source, capture poses and impact logs. It
  is deliberately not WGS-84 geodetic: at mid-latitudes the two differ by about
  0.15° (17 km), which is larger than the deck itself.
- The authoritative deck state is refreshed from the shared ephemeris every
  recovery tick, so the platform rotates with the Earth. It is anchored, not
  station-kept in an inertial frame; an inertial target would drift hundreds of
  kilometres over a return flight.
- Guidance measures deck altitude against the deck, not the terrain radar
  altitude. Over the ocean the terrain datum is the sea floor about 3.2 km
  below, which previously delayed the terminal ignition.
- Deck contact resolves the touchdown verdict in the deck frame. A deck crash
  now records the scorecard and emits one terminal crash event, like a ground
  crash.
- The downrange profile suppresses boostback, starts in `EntryDescent`, lights
  a latched three-engine entry burn from 70 km to 55 km, then keeps three
  engines for the high-speed landing burn and drops to the center engine only
  when both speed and altitude are low. Available thrust is evaluated for the
  selected engine subset; the return-to-launch profile still selects the full
  stage, so its measured crash is unchanged.

**Measured flight (25 t experimental reserve, Starbase, deck at
25.57427, −89.31169).** Separation and insertion are unchanged. The entry burn
lights at 70.0 km at 2,496 m/s with 24,521 kg aboard and cuts off at 55.0 km at
2,273 m/s with 14,008 kg. Powered descent begins at 4.994 km and 1,065 m/s; the
deck contact occurs at **242.35 m/s descent, 25.13 m/s lateral and 134.1° tilt**
with 14,008 kg still aboard. This is a deck crash, not a landing. The remaining
blocker is entry energy and flight-path shaping: the stage crosses the terminal
gate at about 1.0 km/s, and a three-engine retro burn from 5 km cannot remove
that energy. This is the same lift/crossflow gap recorded in residual 7 and the
SC-03 residuals, not a tuning problem. The committed vehicle configuration stays
at the 15 t reserve; 25 t is an experiment and is not committed.

## RCS telemetry attribution — 2026-11-14

**DEFECT — fixed, VERIFIED.** The headless summary reported one
`propellant_used_kg` from whichever `RocketRcs` entity the query returned first.
The recovering booster stays in the normal rocket pipeline and carries its own
RCS cluster, so the printed value could silently be either vehicle. It was the
booster's: 15.887217 kg.

Reporting is now per vehicle, labelled by role and entity:

| vehicle | entity | RCS propellant used |
| --- | --- | ---: |
| primary (Falcon 9 upper stage) | `1v0` | 42.482986 kg |
| recovering booster | `2v0` | 15.887217 kg |

The primary value reproduces the 42.483 kg recorded for the per-thruster split
work, which the single-number summary had made unverifiable. The two figures are
different vehicles drawing from different tanks; neither is a correction of the
other. A unit test spawns one primary and one recovering vehicle and asserts both
rows carry the right role and their own draw.

This is a reporting change only. No allocation, torque, mass flow or propellant
accounting changed, and the flight's final state is identical.

## Touchdown acceptance rejects tumbling bodies and water landings — 2026-11-14

**DEFECT — fixed, IMPLEMENTED.** Two acceptance holes surfaced while measuring the
capture envelope:

1. **A tumbling body could be "landed".** `evaluate_touchdown` judged only the
   sampled contact-point speeds, slope and tilt. A wreck rotating at ~5 rad/s can
   present a point that is momentarily moving at 0.15 m/s while its centre falls
   at ~19 m/s, which passed every limit and recorded a *Landed* splashdown after
   the propellant had already run out. `TouchdownCriteria` now carries
   `max_angular_rate_radps` (default 0.5 rad/s): a controlled touchdown is not
   rotating, so a high body rate is a crash whatever the sampled point says.
2. **Water arrival could be a landing.** A booster cannot land on water. A
   water contact is now always a crash, and the splashdown event is emitted from
   the crash path. The verdict and the scorecard stop calling a lost stage
   "Landed (water)".

Both call sites pass the authoritative body angular rate. Deck landings are
unaffected because a settled deck contact is already below 0.5 rad/s; the three
verified decks below are unchanged to every printed digit.

A 5 × 5 deck grid then measured the capture envelope: roughly 50 m up-track and
50 m cross-track, with 127–670 kg of margin depending on direction, and every
out-of-envelope case exhausting propellant. The table is in **Deck touchdown
verification — 2026-11-14**. This is the concrete form of the reserve limit: it
is a placement tolerance, not a landing failure.

Tests: a domain test asserts a 5 rad/s contact crashes while the same speeds at
0 rad/s land, and a `dem`-gated pipeline test places the recovery stage just
above open Gulf water and asserts the verdict is `Crash` with
`over_water = true`, never `Landed`.

## Reserve trade measured — 2026-11-14

**Experiment; the committed configuration is unchanged.** The open reserve
question was whether 25 t simply widens the 15 t capture window. It does not.
Both variants were flown through the same 25-point deck grid, with the only
difference a temporary local edit to `falcon9.ron`
(`recovery_propellant_reserve_kg: 25_000.0`). That edit was reverted; the
committed asset and its configuration hash are verified below.

| measured | 15 t (committed) | 25 t (experiment) |
| --- | --- | --- |
| vehicle configuration SHA-256 | `c245abf9…a596` | `79f7a6a8…b651` |
| 5 × 5 grid around 25.53904, −88.40209 | 9 landings, 127–670 kg left | 0 landings, all crash |
| contact at the best grid cell | 1.34 m/s vertical, 2.03 m/s lateral, 626 kg left | grid never reached the deck |
| where the stage actually comes down | at the deck | 25.57579, −89.33067 |
| a deck placed on that track | — | 1.63 m/s vertical, **8.67 m/s lateral**, 2,779.65 kg left → crash |

Three results matter:

1. **Reserve changes the trajectory, not just the margin.** With 25 t the stage
   enters heavier and its ballistic track moves about 92 km downrange. At the
   committed deck the ignition gate is never satisfied: the stage passes
   overhead and crashes at 271.8 m/s with 14,008 kg still aboard — the same
   post-entry-burn mass the older experiment reported.
2. **On its own track, 25 t fails on lateral speed, not fuel.** It reaches the
   deck plane at 1.63 m/s vertical, comfortably inside the 5 m/s limit, while
   still drifting 8.67 m/s sideways against a 3 m/s limit, with 2.78 t of
   propellant unused. Extra propellant cannot buy that back with the current
   terminal gains: the heavier vehicle responds more slowly to the same
   attitude commands.
3. **So "more reserve" is a mission-design change, not a free widening.** A
   reserve change invalidates the deck placement, the entry track and the
   terminal tuning together.

The committed 15 t configuration stays. Restoring the asset reproduces the
committed hash `c245abf9615e…a596` and the deck at 25.53904, −88.40209 lands
again with 625.552795 kg left, identical to the verified run.

This is also why the reserve is recorded as a **GAP with a measured shape**
rather than a defect: the 15 t envelope is ~50 m of placement tolerance, and the
25 t alternative trades that for a shifted track and a lateral-speed failure
that the current fixed terminal gains cannot absorb.

## Water-surface impact — 2026-11-14

**DEFECT — fixed, IMPLEMENTED.** Collision used `TerrainSource::height_m` directly.
That field is signed bathymetry over an ocean, so a stage that missed its deck
fell **through** the rendered sea surface and was stopped at the sea floor
3.3–3.4 km below. Radar altitude, impact speed and the crash location were all
measured against the seabed.

### What changed

- `terrain_collision::contact_surface_height_m` returns the source's sea-level
  datum where the surface is open water, and the terrain height otherwise.
- The datum is `TerrainSource::sea_level_height_m`, **not zero**: on Earth it is
  the local EGM2008 undulation. Comparing against zero is wrong by tens of metres
  at the sites this simulator launches from.
- Water is decided by the source's own `SurfaceClass`, not a height comparison.
  A graded coastal pad can sit within a metre of the datum and is still a built
  platform; a height test would have moved the launch pad by that margin.
- Over water the contact normal is the sea-level radial direction and the slope
  is zero, because a sea surface is a geoid, not the sloped seabed.
- `contact_altitude_m` applies the same rule to spent-stage debris despawn, so
  debris stops at the surface rather than 3.3 km below it.

Bathymetry is untouched: it remains terrain data for depth, materials and
rendering. It is simply not the surface a vehicle can strike.

### Measured effect

Both recovered vehicles are over the Gulf. The deck landings below are unchanged
to every printed digit, because they contact the deck, not the sea.

| flight | before (sea floor) | after (sea surface) |
| --- | --- | --- |
| RTLS stage, no deck | 186.155 m/s vertical, 31.822 m/s lateral, 8.726° tilt, 706,186.117 m miss | 205.075 m/s vertical, 77.242 m/s lateral, 16.407° tilt, 705,303.064 m miss |
| deck 25.53897, −88.40277 (out of fuel) | 168.434 m/s vertical, 34.346 m/s lateral | 22.670 m/s vertical, 3.555 m/s lateral, 1.316° tilt |

The RTLS impact is now faster and more tilted because it is arrested 3.4 km
higher, earlier in the same fall. That is the honest measurement; the previous
figures described a seabed impact. The primary vehicle is unaffected: mass
23,858.331 kg, the same final position and velocity, 201.178314 × 199.975904 km.
The RTLS recovery remains a crash and is still not a landing.

### A test artifact this exposed

`safe_leo_insertion_app` constructs a 200 km orbital state on top of the ascent
fixture, which leaves `GroundRest` latched. The contact system therefore damped
this orbiting vehicle's velocity along the local surface normal every tick, and
the committed baseline encoded that damping — including the old seabed normal.
The fixture now clears `GroundRest`, and `leo-insertion-safe` was **deliberately
re-recorded** with an `intended_baseline_divergence: Expected` audit entry naming
the artifact. Tick 2 now records the undamped 7788.52 m/s circular state instead
of a 0.829× damped value. No physics, tolerance or guidance changed to make the
test pass, and the other two baselines remain bit-identical.

The pad-holding and recovery fixtures now pin their launch pad through the
existing `PadPinnedTerrain`, declaring it land. Without the `dem` payload pyramid
the procedural fallback puts Kennedy Space Center ~991 m below sea level, which
is a data-availability artifact, not a pad; the fixtures previously inherited it.
That keeps both feature configurations testing the same geometry.

### Validation

- `cargo fmt --check`, both checks, `cargo clippy --all-targets --features dem --
  -D warnings`: PASS.
- `cargo test --features dem`: **1,274 library tests passed**, two ignored.
- `cargo test --no-default-features`: **1,221 library tests passed**, two ignored.
- `openspec validate --all --strict`: 43 items pass.
- `cargo build --release --features dem`: PASS.
- Three-mode release smoke, 15 s each: exit 124, no ERROR/panic matches.
- Headless: three deck landings (all `Landed`, identical to the pre-change
  figures), one out-of-fuel deck crash, and the RTLS re-flight above.

### Measured capture envelope

The three verified decks are not an isolated coincidence: a 5 × 5 grid of
placements around 25.53904, −88.40209 (25 m spacing, 25 flights) gives a coherent
envelope. `Δalong` is positive down-track; `Δcross` positive to the east.

| Δalong (m) | Δcross −50 | −25 | 0 | +25 | +50 |
| ---: | --- | --- | --- | --- | --- |
| −50 | 262 kg | 568 kg | 670 kg | crash | crash |
| −25 | 130 kg | 459 kg | 629 kg | crash | crash |
| 0 | 127 kg | 455 kg | 626 kg | crash | crash |
| +25 | crash | crash | crash | crash | crash |
| +50 | crash | crash | crash | crash | crash |

Reading it honestly:

- The capture window is roughly **50 m up-track and 50 m cross-track**, not a
  radius around a point, and it is strongly asymmetric. Every landing is at
  Δalong ≤ 0 and Δcross ≤ 0.
- The margin depends on direction, not just distance: 670 kg when the deck sits
  50 m up-track, but only 127 kg when it sits 50 m to one side at the same
  distance.
- Every failure at +25 m or beyond is propellant exhaustion (0 kg left), not a
  control limit. The stage reaches the deck plane with nothing left to arrest the
  last metres and falls into the sea, which is now correctly a crash.
- One earlier placement, 25.53897, −88.40277, is outside this grid and crashes
  the same way.

That makes the remaining limit concrete: with the committed 15 t reserve the
stage can absorb about a 50 m deck-placement error in one favourable direction,
and essentially none in the other. Widening it needs either more entry-burn
reserve or less entry Δv loss (SC-01 / lift), not more terminal gain.

The grid is a coarse sample: the true boundary lies between nodes, so these are
bracketing measurements, not a fitted contour.

Water is still not modelled as a fluid: there is no splashdown, buoyancy, or
wetting. A water contact is a surface arrest and a crash verdict, which is the
honest limit of this change.

## Deck touchdown verification — 2026-11-14

**VERIFIED for three specified headless deck placements, not the general
recovery envelope.** Each placement was flown twice, with independent processes
and one run of each pair using the debug trace; every repeated recovery summary
matches its pair to all printed digits and remains `Landed` through T+656.25 s.

### Defects corrected

1. Terminal gimbal allocation used requested throttle while actuation applied
   slew-limited throttle. During the retrograde-to-terminal handover the request
   was ~0.10 while physical throttle was ~0.34. Allocation now shares the exact
   upcoming throttle calculation with actuation, including the selected engine
   envelope. A fixed-system test without RCS verifies delivered gimbal torque
   against requested torque in both increasing and decreasing throttle slews.
   Other phases retain their existing allocation policy; actuation itself is
   unchanged. The flight still missed with this fix alone.
2. Deck guidance used cylinder-center altitude, but contact uses the rotated
   lower extent. A nearby deck-placement trial crossed the real deck at
   10.42 m/s before the 15 m flare had begun, with 937 kg still aboard. Guidance
   now projects the **same lower-extent point onto the same deck normal** as the
   contact solver. A regression compares pad and deck flare commands at equal
   clearance, including a tilted vehicle. This fixes the geometric delay rather
   than retuning a flare constant or moving the contact plane.
3. The terminal lateral loop ran at ~0.35 rad/s while the booster needs roughly
   1 s to slew 10° of thrust vector. It therefore reversed its command faster
   than the airframe could follow: the measured failure held a 12° target with a
   24° attitude error and reached 17.1° tilt at contact. The position/velocity
   gain pair is now a second-order loop at 0.15 rad/s with damping ratio 1.5
   (0.0225 /s² and 0.45 /s), deliberately overdamped for the inner-loop lag.
   Two pure tests guard this: one rejects an outer loop closer than 3× to the
   observed ~0.5 rad/s attitude rate, and one integrates the loop with a 0.35 s
   attitude lag and requires it to stay inside the 12° envelope. The first test
   fails for the previous 0.12 / 0.9 pair, so it guards the actual regression.

These are representative control settings, not validated Falcon 9 flight
software. No thrust, drag, lift, mass, engine-limit or touchdown criterion was
changed.

An opt-in `rocket_landing` DEBUG trace samples authoritative clearance, lateral
error/speed, actual/commanded tilt, attitude error, throttle and fuel once per
simulation second with an entity identifier. Enable with
`RUST_LOG=info,rocket_landing=debug`. It adds no force or guidance state.

### Flight evidence and limits

```sh
cargo build --release --features dem
RUST_LOG=info target/release/headless_scenario --site starbase \
  --recovery-deck <lat>,<lon> --steps 42000 --scenario <id>
```

All three placements below are `Landed` in two independent runs each, with
recovery summaries identical to every printed digit. Nothing differs between
the repeats, so the landing is deterministic for a fixed mission input.

| deck (terrain-radial) | vertical | lateral | tilt | fuel left | centre offset |
| --- | ---: | ---: | ---: | ---: | ---: |
| 25.53904, −88.40209 | 1.338214 m/s | 2.029191 m/s | 0.863950° | 625.553 kg | 20.711 m |
| 25.538992, −88.402547 | 1.329556 m/s | 0.633947 m/s | 0.371857° | 267.465 kg | 13.036 m |
| 25.53902, −88.40232 | 1.328258 m/s | 1.276469 m/s | 0.577652° | 455.929 kg | 12.234 m |

The third column of placements is the range over which this guidance converges
without retuning. The middle placement is the one that crashed at 17.1° tilt
before the bandwidth change; it now lands at 0.37° tilt with 267 kg left, which
is the smallest measured margin and the closest to the fuel limit.

`centre offset` is the scorecard distance from the deck centre to the vehicle
**centre**, not the footprint test. Contact latches on the rotated lower extent,
so a tilted vehicle can show a larger centre offset than its contact point; all
three passed the unchanged 25 m bounded deck check.

The existing deck extent, touchdown criteria and landing-gear energy-capacity
check accept these contacts. No thresholds were relaxed. The final inertial speed
is ~419.19 m/s because the landed vehicle co-rotates with Earth; it is not a
deck-relative landing speed. Guidance and contact continue to use physical state.

Deck position is a mission input, not a fitted physics coefficient. A fourth
placement, 25.53897, −88.40277, still **crashes**: it exhausts propellant before
the deck and falls to the sea floor at 168.43 m/s vertical, 34.35 m/s lateral and
11.49° tilt with 0 kg left. That is a fuel-margin limit rather than a control
limit — the stage was inside the footprint laterally but had nothing left to
arrest the fall. The verified envelope is therefore bounded: convergence works
for a deck near the natural track, and the committed 15 t reserve leaves little
margin when the required diversion grows. No universal landing capability is
claimed.

The committed reserve remains 15 t. SC-01 retro-thrust, drag, lift, engine limits,
landing criteria and vehicle configuration are unchanged. Measured margins of
626 kg / 267 kg / 456 kg are for these scenarios, not a certified operational
reserve, and the 267 kg case shows how thin the margin can get.
Water-surface impact modelling, entry-lift/flight-path fidelity, RCS station/tank
fidelity (the cluster remains representative) and SC-06 immutable-source
promotion remain open. RCS *reporting* attribution is fixed.

RTLS was deliberately re-flown for 345,600 ticks with no deck. Its impact figures
changed once water became the collidable surface, and are recorded under
**Water-surface impact — 2026-11-14**. The primary vehicle is unchanged:
23,858.331 kg, the same final state, 201.178314 × 199.975904 km. Vehicle, terrain
and kernel hashes are the same, and the committed regression artifacts pass.

### Validation for the touchdown correction

- `cargo fmt --check`, both checks (all-targets with `dem`, no-default-features),
  and `cargo clippy --all-targets --features dem -- -D warnings`: PASS.
- `cargo test --features dem`: **1,270 library tests passed**, two ignored.
- `cargo test --no-default-features`: **1,218 library tests passed**, two ignored.
- `openspec validate --all --strict`: 43 items pass.
- `cargo build --release --features dem`: PASS.
- Release solar/craft/rocket smoke, 15 s each: exit 124, no ERROR/panic matches.
- Debug `cargo run`, `cargo run -- craft`, `cargo run -- rocket`, 15 s each:
  each creates its mode window, exits 124 and has no ERROR/panic matches.
- Headless: three verified deck landings, each run twice; one fuel-limited deck
  crash; RTLS re-flight. Measurements above distinguish a landing from completed
  execution.

## Earlier terminal recovery revalidation — 2026-11-14

**Status: IMPLEMENTED and regression-tested; successful full-flight recovery is
still a GAP.** This section supersedes the earlier deck-contact interpretation
and records a deliberate re-flight of the RTLS reference, rather than assuming
the changed terminal law preserves it.

- One terminal law serves both pad and deck recovery: 25% ignition margin,
  10 m/s minimum descent before the 15 m flare. The lateral gains recorded here
  (0.12 /s² and 0.9 /s) were later superseded by the 0.15 rad/s overdamped loop;
  see **Deck touchdown verification — 2026-11-14**.
- `LandingBurnSequence` is retained in `RocketAutopilot`, which replay already
  captures. Coast → retrograde → terminal is one-way. The previous boolean
  reset at 25 m/s: the first re-flight shut down again at ~65 m when speed rose
  above that threshold and crashed with 1,064.40 kg still aboard. The sequence
  now holds ignition and prevents terminal descent from returning to coast.
  It applies to pad recovery too, without requiring `EntryBurnState` or RCS.
- Three landing engines remain selected until both speed ≤50 m/s and altitude
  ≤100 m; the center engine then owns the low, slow settle.
- Deck contact requires lateral distance within 25 m and contact-point altitude
  in the fixed-step closing band below the plane. Previously a missed deck
  could capture a sea-floor impact ~3.35 km below it. The earlier 25 t “deck
  crash” is therefore historical output from an invalid contact gate, not
  evidence that the stage crossed the deck. The radial lateral check uses a
  circular footprint inscribed in the deck, conservatively excluding corners.
- Temporary recovery, entry, landing and plane-crossing diagnostics and their
  plugin registration have been removed.

### Reproducible final flights

Built with `cargo build --release --features dem`; both commands exited 0:

```sh
RUST_LOG=info target/release/headless_scenario --site starbase --recovery-deck 25.538992,-88.402547 --steps 42000 --scenario recovery-deck-final
RUST_LOG=info target/release/headless_scenario --site starbase --steps 345600 --scenario recovery-rtls-final
```

| measured output | deck, 656.25 s | RTLS, 5,400 s |
| --- | ---: | ---: |
| outcome | sea-floor crash; deck missed | crash |
| contact vertical speed | 166.696370 m/s | 186.154696 m/s |
| contact lateral speed | 41.601202 m/s | 31.822342 m/s |
| contact tilt | 15.450160° | 8.726413° |
| scorecard distance to target | 86.511053 m | 706,186.117161 m |
| final recovery propellant | 0 kg | 0 kg |

The scorecard distance is recorded at terrain contact; it is **not** deck-plane
crossing error. The final deck output exactly matches the diagnostic sequence
flight's recovery summary after diagnostic removal. No deck location sweep or
physics coefficient tuning was used to manufacture a successful contact.

Both columns record a **seabed** contact, which the later water-surface fix
replaced; the RTLS column is superseded by **Water-surface impact — 2026-11-14**.
The deck column is also superseded: that placement now lands (see the deck
touchdown table below).

RTLS revalidation confirms the previous rounded impact figures and the primary
vehicle's accepted 201.178314 × 199.975904 km orbit and final mass 23,858.331 kg.
The anticipated RTLS change did not appear in these measured outcomes: the
reserve is already exhausted. All committed regression artifacts and tolerances
remain unchanged and pass. The `rcs_summary` line originally printed a single
number from the first `RocketRcs` entity it found, without saying which vehicle
that was; see **RCS telemetry attribution — 2026-11-14**.

Run provenance printed by the final RTLS flight:

- vehicle SHA-256: `c245abf9615edf07948e82f9b464b15a4400f9399f20a7da88b367f2acb5a596`
- terrain: `earth-csdem-sha256:2e30bb5498ce9ec8a12020cefde669208f0dba09d5a6b66861fa6e9ee11d33b6`
- ephemeris: `naif-de440-egm2008-primary-v1:b3916ccb53d43a1b55a9ef8d50959ec65d696abfa2935f97973759031a0ea960`

**Reserve decision:** retain the committed 15 t. The measured entry burn consumes
about 10.25 t (14,502 → 4,247 kg, 70 →55 km). SC-01 supplies roughly half nominal
retro-thrust at Mach ~9; this correlation was not tuned. The remaining landing
reserve does not achieve recovery in this flight. Lateral convergence, entry
energy/flight-path shaping and fuel margin remain GAPs; 25 t remains experimental.
Water-surface impact, RCS station/tank fidelity and SC-06 immutable source
promotion remain the previously recorded limitations.

### Current validation

Four additional library regressions cover ignition margin, persistent ignition
and terminal handover, terminal speed excursions, and rejection of deep/outside
deck contacts. Existing tests were retained; counts rise from 1,262/1,210 to
**1,266/1,214**, with two ignored in each configuration.

| check | result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo check --all-targets --features dem` | PASS |
| `cargo check --no-default-features` | PASS; existing feature-disabled warnings |
| `cargo clippy --all-targets --features dem -- -D warnings` | PASS |
| `cargo test --features dem` | PASS; library 1,266 passed, 2 ignored |
| `cargo test --no-default-features` | PASS; library 1,214 passed, 2 ignored |
| `openspec validate --all --strict` | PASS, 43 items |
| `cargo build --release --features dem` | PASS |
| release binary smoke, solar/craft/rocket, 15 s each | all exit 124; no ERROR/panic matches; startup evidence only |
| `cargo run`, `cargo run -- craft`, `cargo run -- rocket`, 15 s each after debug build | all exit 124; each creates its mode window; no ERROR/panic matches |
| headless deck and RTLS flights | completed; crashes recorded above, landing GAP |

Tests emit logger-already-installed and missing-image-loader messages in minimal
Bevy fixtures despite passing assertions. The three release smoke logs do not.

## Historical validation — recovery-deck commit

The gate for this track is the project gate, run on the recovery-deck commit. It
adds eight tests (three entry-burn sequencing, two body-fixed deck, one deck
input parse, two deck-coordinate validation) and one landing-engine-subset test,
so the floor rises to 1,262 with `dem` and 1,210 without; the previous floor was
1,253 and 1,201.

| check | result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo check --all-targets --features dem` | PASS |
| `cargo check --no-default-features` | PASS (5 pre-existing unused-import/dead-code warnings) |
| `cargo clippy --all-targets --features dem -- -D warnings` | PASS |
| `cargo test --features dem --lib` | 1,262 passed, 0 failed, 2 ignored |
| `cargo test --no-default-features --lib` | 1,210 passed, 0 failed, 2 ignored |
| `openspec validate --all --strict` | 43 passed, 0 failed |
| `cargo build --release --features dem` | PASS |
| three-mode 15 s smoke after the change | `solar`, `craft`, `rocket` all exit 124, zero `ERROR` or panic matches |
| headless Falcon 9 flight (Starbase, 345,600 ticks = 5,400 s) | ascent cut at T+448.203 s (137.076 km, 7,824.236 m/s); one circularization burn T+1719.5–1720.72 s; accepted 201.1783 x 199.9759 km (e ≈ 9.1e-5 from the apsides); 42.483 kg RCS propellant; 23,858.331 kg at T+5,400 s |
| recovery stage in the same flight | unchanged by the deck work: boostback → engines-first `EntryDescent`; holds α ≈ 179.7° with body rate below 0.5 rad/s through the T+435–500 dynamic-pressure rise; one crash at T+501.078 s (186.15 m/s vertical, 31.82 m/s lateral, 8.73° tilt, 706.19 km short); 0 kg propellant, `Crashed` at T+5,400 s |
| downrange deck flight (25 t experiment, 42,000 ticks) | entry burn 70→55 km (24,521→14,008 kg, 2,496→2,273 m/s); powered descent at 4.994 km / 1,065 m/s; deck contact 242.35 m/s vertical, 25.13 m/s lateral, 134.1° tilt, 14,008 kg remaining; deck crash, not a landing |
| reserve trade-off with grid fins (pre-center-engine) | 25 t preserves insertion (200.526 × 199.353 km) and improves the crash to T+553.813 s, 305.62 m/s, 37.65° tilt, 549.01 km short — still a crash; 40 t breaks the ascent (separation T+135.1 s, Ascent→Reentry at 116.7 km); the committed config stays at 15 t |
| 25 t + center engine + one-engine subset (experiment, not committed) | insertion 200.526 × 199.353 km; RTLS reaches powered descent with 4.186 t left but crashes with 195.85 m/s vertical, 222.57 m/s lateral, 39.05° tilt, 520.54 km short. A downrange deck profile with a three-engine entry burn (25.4 → 5.07 t, 2537 → 1890 m/s) crashes 158–622 km from the deck; re-flown after the fold it holds retrograde but still reaches 2.2 km/s at 15 km and peaks near 900 kPa |
| RCS authority A/B | roll-slew fix (`0e333b0`): 5.0e7 / 5.0e6 / 1.0e6 / 5.0e5 N·m all hand off near 137.7 km, 1.0e5 cuts at 100 km/1,873 m/s, 2.0e4 loses control at ~T+145 s; after it, 5.0e3 and 2.0e4 circularize and 100 N·m still reaches the ascent cut. Per-thruster split (`70bfef5` vs `976f998`): accepted 201.1783 x 199.9759 vs 200.847 x 199.238 km, RCS 42.483 vs 0 kg, final mass 23,858.331 vs 23,901.016 kg |
