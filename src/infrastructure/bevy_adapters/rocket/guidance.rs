use super::components::*;
use crate::domain::services::guidance::{
    advance_ascent_phase, advance_descent_phase, ascent_pitch_gate_engaged,
    attitude_from_direction, banked_attitude_from_direction, boostback_guidance,
    default_surface_landing_target, gravity_turn_direction_engaged, pitch_axis_from_reference,
    prograde_ascending_node_launch_heading, prograde_attitude, reentry_bank_angle,
    reentry_bank_angle_enhanced, roll_preserving_attitude_from_direction,
    target_surface_range_errors_m, terminal_landing_guidance, transfer_burn_phase, AutopilotMode,
    DescentGuidanceConfig, EntryBurnCommand, TransferBurnPhase,
};
use crate::domain::services::physics_orbital::orbital_elements_from_state_in_reference_frame;
use crate::domain::services::reference_frames::{
    body_fixed_to_planet_inertial, geodetic_to_body_fixed, planet_equatorial_reference_x_axis,
    planet_inertial_rotation_axis, planet_inertial_spin_axis,
};
use crate::domain::services::rocket_propulsion::{
    engine_subset, stage_available_thrust_body, STANDARD_GRAVITY_MPS2,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
use crate::infrastructure::bevy_adapters::entity_components::PlanetComponent;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use bevy::ecs::query::QueryData;
use bevy::log::{debug, info};
use bevy::math::DVec3;
use bevy::prelude::*;

/// Insertion burns stop 1 km short of the target apsis. The acceptance
/// predicate keeps its wider 25 km band; this deadband is what the burn law
/// aims at, so a trimmed orbit satisfies the predicate rather than a burn that
/// coasted to a stop 24 km below the target.
const INSERTION_DEADBAND_M: f64 = 1_000.0;

/// Minimum altitude for the ascent cutoff. The cutoff is targeted at the
/// osculating apoapsis, which would otherwise be a meaningless few kilometres
/// on the pad; 100 km also keeps the cutoff above the sensible atmosphere.
const INSERTION_MIN_ALTITUDE_M: f64 = 100_000.0;

/// Altitude above which ascent replaces the open-loop pitch ramp with a
/// radial-velocity-nulling tilt about the local horizontal.
const RADIAL_NULL_ALTITUDE_M: f64 = 80_000.0;

/// Time constant used to turn the remaining altitude into a radial-velocity
/// setpoint above [`RADIAL_NULL_ALTITUDE_M`], seconds.
const RADIAL_ARRIVAL_TIME_S: f64 = 30.0;

/// Bounds on that radial-velocity setpoint, m/s. Negative values are allowed so
/// an overshooting ascent is steered back down instead of only coasting.
const RADIAL_RATE_MIN_MPS: f64 = -50.0;
const RADIAL_RATE_MAX_MPS: f64 = 250.0;

/// Thrust tilt per m/s of radial-velocity error, and its cap (≈19°). The cap
/// keeps the command a trim on a mostly horizontal burn, not a pitch-over.
const RADIAL_NULL_GAIN: f64 = 1.0 / 1_500.0;
const RADIAL_TILT_LIMIT: f64 = 0.35;

/// Radial-speed tolerance for "the vehicle is at an apsis", m/s. The osculating
/// apsis is where the radial speed crosses zero, and near it the radial rate
/// changes by only ~0.1 m/s per second, so this captures the crossing within a
/// few metres instead of several kilometres.
const APSIS_RADIAL_SPEED_TOLERANCE_MPS: f64 = 0.5;

/// Distance-to-apsis sanity bound for the radial-speed test, meters. It keeps
/// the apoapsis crossing from also matching a periapsis of a nearly circular
/// orbit.
const APSIS_CAPTURE_DISTANCE_M: f64 = 1_000.0;

/// Guidance can command an engine start, so it uses the stage's available
/// thrust envelope rather than the currently running engine set. The active
/// core capability still enforces synchronized inventory and recovery reserve.
/// A selected engine subset must be reflected here: using the full nine-engine
/// envelope for a one-engine landing tells the terminal law it can decelerate
/// at nine times the real rate and it ignites far too late.
fn available_core_thrust_n(
    propulsion: &RocketPropulsion,
    ambient_pressure_pa: f64,
    selected_engines: Option<u32>,
) -> f64 {
    let Some(active_core_stage) = propulsion.active_core_stage() else {
        return 0.0;
    };
    if !active_core_stage.has_burnable_propellant() {
        return 0.0;
    }
    let engines = engine_subset(&active_core_stage.stage().engines, selected_engines);
    stage_available_thrust_body(engines, 1.0, ambient_pressure_pa)
        .0
        .length()
}

/// Burnable propellant on the active core stage, for transition logging.
fn available_propellant_kg(propulsion: &RocketPropulsion) -> f64 {
    propulsion
        .active_core_stage()
        .map_or(0.0, |stage| f64::from(stage.burnable_propellant_kg()))
}

/// Engine subset for the terminal landing burn. While either the speed or the
/// altitude is above its switch value the stage keeps the high-thrust subset;
/// only the final low, slow settle drops to the center engine. Absent when no
/// downrange entry plan exists, leaving the existing return-to-launch
/// full-stage behaviour untouched.
fn landing_burn_engine_count(
    entry_burn: Option<&EntryBurnState>,
    speed_mps: f64,
    altitude_m: f64,
) -> Option<u32> {
    entry_burn.map(|burn| {
        if speed_mps > burn.plan.landing_burn_switch_speed_mps
            || altitude_m > burn.plan.landing_burn_switch_altitude_m
        {
            burn.plan.landing_burn_engines
        } else {
            burn.plan.landing_engines
        }
    })
}

/// Feed a moving deck's domain prediction into the existing recovery guidance
/// laws. This adapter does not introduce new guidance mathematics or mutate a
/// render transform: boostback and terminal guidance continue to consume the
/// normal `RocketAutopilot::target_landing_position_m` command input.
pub fn update_drone_ship_landing_targets(
    ships: Query<&DroneShip>,
    mut rockets: Query<(&DroneShipLandingTarget, &mut RocketAutopilot)>,
) {
    for (target, mut autopilot) in &mut rockets {
        let Ok(ship) = ships.get(target.drone_ship) else {
            continue;
        };
        if !target.prediction_horizon_s.is_finite() || target.prediction_horizon_s < 0.0 {
            continue;
        }
        let first_assignment = autopilot.target_landing_position_m.length_squared() <= 1.0;
        autopilot.target_landing_position_m =
            ship.state.predict_position(target.prediction_horizon_s);
        if first_assignment {
            info!(
                "Recovery deck target acquired: deck=({:.1}, {:.1}, {:.1}) m, horizon={:.1} s",
                ship.state.position_m.x,
                ship.state.position_m.y,
                ship.state.position_m.z,
                target.prediction_horizon_s
            );
        }
    }
}

/// Bundled read access for the guidance stage: one rocket's mission-relevant
/// state. A derived query keeps the system signature readable and gives every
/// field an explicit name at the use site (composition over positional
/// tuples).
#[derive(QueryData)]
#[query_data(mutable)]
pub struct GuidanceAccess {
    pub entity: Entity,
    pub binding: &'static RocketPlanetBinding,
    pub dynamics: &'static RocketPhysicsState,
    pub geometry: &'static RocketGeometry,
    pub mission_state: &'static mut RocketMissionState,
    pub autopilot: &'static mut RocketAutopilot,
    pub propulsion: &'static RocketPropulsion,
    /// Refreshed by `RocketSet::Atmosphere` before guidance. Descent guidance
    /// uses this shared atmosphere/surface-relative sample rather than deriving
    /// another rotating-frame velocity.
    pub conditions: &'static RocketFlightConditions,
    /// Present on production rockets. Optional to keep isolated fixed-pipeline
    /// fixtures focused on the state they are testing.
    pub aerodynamic_forces: Option<&'static AerodynamicForces>,
    /// Fixed-tick proper acceleration from every non-gravitational force.
    pub specific_force: Option<&'static SpecificForceAcceleration>,
    pub thermal: Option<&'static ThermalState>,
    pub collision: &'static TerrainCollisionState,
    pub orbital: &'static OrbitalElements,
    pub commands: &'static mut RocketCommands,
    /// Ground pad for a launch/return vehicle. Optional so isolated fixtures
    /// that only exercise attitude laws do not need the component.
    pub launch_site: Option<&'static LaunchSiteCoordinates>,
    /// Moving-deck target. When present it owns the landing target; the
    /// ground-pad update must not overwrite it.
    pub drone_ship_target: Option<&'static DroneShipLandingTarget>,
    /// Downrange entry-burn sequencer. Absent on a return-to-launch profile.
    pub entry_burn: Option<&'static mut EntryBurnState>,
    /// Engine subset selected for the current recovery phase.
    pub engine_selection: Option<&'static mut EngineSelection>,
}

/// Mission guidance: computes the target attitude from the mission phase and
/// current state, and advances the ascent/descent phase (Launch → Ascent →
/// Orbit → DeorbitBurn → ReentryCorridor → PoweredDescent/UnpoweredDescent →
/// Landing). Writes only the command interface; never the vehicle's motion
/// (AGENTS.md section 18).
pub fn guidance_system(
    sim_time: Res<SimulationTime>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<GuidanceAccess>,
) {
    let dt = sim_time.fixed_timestep();
    for mut access in rocket_query.iter_mut() {
        // Rebind the bundled fields to the names the guidance body reads;
        // mutable fields deref out of Bevy's change-detection wrappers.
        // Read-only view of the bundled dynamics.
        let rocket = &access.dynamics;
        let binding = access.binding;
        let launch_site = access.launch_site;
        let drone_ship_target_present = access.drone_ship_target.is_some();
        let propulsion = access.propulsion;
        let conditions = access.conditions;
        let aerodynamic_forces = access.aerodynamic_forces;
        let specific_force = access.specific_force;
        let thermal = access.thermal;
        let collision = access.collision;
        let orbital = access.orbital;
        let mission_state = &mut *access.mission_state;
        let autopilot = &mut *access.autopilot;
        let commands = &mut *access.commands;
        let mut entry_burn = access.entry_burn.as_deref_mut();
        let mut engine_selection = access.engine_selection.as_deref_mut();
        let selected_engine_count = engine_selection.as_deref().map(|selection| {
            // Read before the entry-burn block may write the subset.
            selection.active_count
        });
        let previous_autopilot_mode = autopilot.mode;

        // Guidance owns throttle targets. Preserve the established phase
        // defaults for modes that do not need a specialized burn law.
        commands.throttle_cmd = match *mission_state {
            RocketMissionState::Launch | RocketMissionState::Ascent => 1.0,
            RocketMissionState::PoweredDescent => 0.7,
            RocketMissionState::Landing => 0.5,
            RocketMissionState::PreLaunch
            | RocketMissionState::Orbit
            | RocketMissionState::DeorbitBurn
            | RocketMissionState::ReentryCorridor
            | RocketMissionState::UnpoweredDescent
            | RocketMissionState::Landed
            | RocketMissionState::Crashed => 0.0,
        };

        let Some(planet) = planet_query
            .iter()
            .find(|planet| planet.matches_body(&binding.planet_name))
        else {
            continue;
        };
        let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
        let Some(orientation) =
            ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
        else {
            continue;
        };
        let Some(mu_m3_s2) =
            ephemeris_snapshot.gravitational_parameter_for_catalog_body(&planet.domain_planet.name)
        else {
            continue;
        };
        let position_m = rocket.dynamics.position_m;
        let radius = position_m.length();
        if radius < 1.0 {
            continue;
        }
        let up_dir = position_m / radius;
        let altitude_m = (radius - radius_m).max(0.0);
        let radar_altitude_m = if drone_ship_target_present
            && autopilot.target_landing_position_m.length_squared() > 1.0
        {
            // A deck is the landing surface. Terrain radar altitude over the
            // ocean measures the sea floor several kilometres below, which
            // ignites the terminal burn far too late. Deck-relative altitude is
            // the same frame and lower-extent point deck contact resolves in.
            // Center altitude would delay the flare by half a booster length.
            let contact_position_m =
                position_m + rocket.dynamics.orientation * access.geometry.lower_extent_body_m();
            (contact_position_m - autopilot.target_landing_position_m)
                .dot(autopilot.target_landing_position_m.normalize())
                .max(0.0)
        } else if collision.radar_altitude_m.is_finite() {
            collision.radar_altitude_m.max(0.0)
        } else {
            altitude_m
        };
        // A returning stage re-derives its ground-pad target from the current
        // body orientation every tick. A target captured once at separation
        // would miss the rotating pad by hundreds of kilometres over a return
        // flight. A moving-deck target owns the command input when present.
        if !drone_ship_target_present {
            if let Some(launch_site) = launch_site {
                let pad_body_fixed = geodetic_to_body_fixed(launch_site, &planet.domain_planet)
                    .normalize()
                    * (radius_m + f64::from(launch_site.altitude_m));
                autopilot.target_landing_position_m =
                    body_fixed_to_planet_inertial(pad_body_fixed, orientation);
            }
        }
        let inertial_velocity_mps = rocket.dynamics.velocity_mps;
        let surface_relative_velocity_mps = conditions.atmosphere_relative_velocity_mps;
        let surface_relative_speed_mps = conditions.airspeed_mps;
        let mu = mu_m3_s2;
        // Two distinct references: the geometric pole defines the local
        // east/north basis used by the launch heading, while inclination is
        // measured about the physical rotation axis. For a prograde body in the
        // reflected solar frame those are antiparallel; using the pole for
        // elements reports 180°-i and the insertion target can never match.
        let reference_normal = planet_inertial_spin_axis(orientation);
        let rotation_axis = planet_inertial_rotation_axis(orientation);
        let reference_x_axis = planet_equatorial_reference_x_axis(orientation);
        let state_elements = orbital_elements_from_state_in_reference_frame(
            position_m,
            inertial_velocity_mps,
            mu,
            rotation_axis,
            reference_x_axis,
        );
        let target_orbit_reached = autopilot.target_orbit.matches_state_in_reference_frame(
            position_m,
            inertial_velocity_mps,
            mu,
            radius_m,
            rotation_axis,
            reference_x_axis,
        );

        // Update time since liftoff for time-based ascent guidance.
        if *mission_state != RocketMissionState::PreLaunch {
            autopilot.time_since_liftoff_s += dt;
        }

        // Pre-launch hold: wait for user input (Space) via handle_rocket_launch_input.
        // Do NOT auto-transition to Launch; physics keeps the vehicle on the pad
        // via GroundRest until thrust exceeds weight.

        // If mission state is Launch but autopilot is still Off (e.g., test setup
        // or user just pressed Space), arm the ascent autopilot.
        if *mission_state == RocketMissionState::Launch
            && autopilot.mode == crate::domain::services::guidance::AutopilotMode::Off
        {
            autopilot.mode = crate::domain::services::guidance::AutopilotMode::Ascent;
        }

        // Get descent guidance config for this body.
        let descent_config = DescentGuidanceConfig::for_body(&planet.domain_planet.name);

        // Descent phase selection must use the same propellant reserve and
        // engine lifecycle constraints as actuation. A nonzero tank that is
        // wholly reserved or has terminally depleted engines is not thrust
        // availability.
        let has_active_engines = available_core_thrust_n(
            propulsion,
            conditions.ambient_pressure_pa,
            selected_engine_count,
        ) > 0.0;

        // Guidance constraints use the shared fixed-tick flight conditions.
        let dynamic_pressure_pa = conditions.dynamic_pressure_pa;

        // Advance the mission phase from the authoritative f64 target predicate.
        *mission_state = advance_ascent_phase(
            (*mission_state).into(),
            altitude_m,
            autopilot.ascent_profile.ascent_start_altitude_m,
            target_orbit_reached,
        )
        .into();
        // Also advance descent phases.
        *mission_state = advance_descent_phase(
            (*mission_state).into(),
            radar_altitude_m,
            surface_relative_speed_mps,
            dynamic_pressure_pa,
            surface_relative_velocity_mps.dot(up_dir) < -1.0,
            has_active_engines,
            &descent_config,
        )
        .into();

        // The mission state owns phase handoffs; without this synchronization a
        // descent can remain in an Off/Reentry mode and coast into the ground.
        match (*mission_state, autopilot.mode) {
            (
                RocketMissionState::PoweredDescent,
                AutopilotMode::Off | AutopilotMode::Reentry | AutopilotMode::Ascent,
            ) => {
                autopilot.mode = AutopilotMode::PoweredDescent;
            }
            (RocketMissionState::ReentryCorridor, AutopilotMode::Off | AutopilotMode::Ascent) => {
                autopilot.mode = AutopilotMode::Reentry;
            }
            (RocketMissionState::UnpoweredDescent, _) => {
                autopilot.mode = AutopilotMode::Off;
            }
            (RocketMissionState::Landing, AutopilotMode::PoweredDescent | AutopilotMode::Off) => {
                autopilot.mode = AutopilotMode::Landing;
            }
            _ => {}
        }

        // A trim burn already in progress owns the throttle until its own
        // tighter deadband stops it. The acceptance predicate has a ±25 km
        // band, so accepting in the middle of the circularization burn would
        // end the burn tens of kilometres short of the target.
        let core_burning = propulsion
            .active_core_stage()
            .is_some_and(|stage| stage.has_running_engines());
        if target_orbit_reached
            && *mission_state == RocketMissionState::Orbit
            && autopilot.mode == AutopilotMode::OrbitInsertion
            && !core_burning
        {
            autopilot.mode = AutopilotMode::Off;
        }

        // Compute target attitude based on autopilot mode.
        match autopilot.mode {
            AutopilotMode::Ascent => {
                // Gated combined schedule: hold the local vertical until the
                // vehicle clears the pad/tower (altitude AND vertical speed),
                // then follow the altitude/time pitch ramp. Low-thrust
                // vehicles must never start the turn while still near the
                // ground just because the wall clock says so. The gate is
                // latched: once engaged, a later reduction in vertical speed
                // (e.g. coasting through high-altitude apogee) must not command
                // the vehicle back to a vertical attitude.
                let vertical_speed_mps = inertial_velocity_mps.dot(up_dir);
                autopilot.ascent_pitch_over_engaged = ascent_pitch_gate_engaged(
                    autopilot.ascent_pitch_over_engaged,
                    &autopilot.ascent_profile,
                    altitude_m,
                    vertical_speed_mps,
                );
                commands.target_attitude = match prograde_ascending_node_launch_heading(
                    position_m,
                    reference_normal,
                    autopilot.target_orbit.target_inclination_rad,
                ) {
                    Ok(heading) => {
                        // The heading is horizontal by construction, so its
                        // perpendicular pitch axis is always defined.
                        let pitch_axis = pitch_axis_from_reference(up_dir, heading.direction_pci)
                            .expect("a horizontal ascent heading has a pitch axis");
                        roll_preserving_attitude_from_direction(
                            gravity_turn_direction_engaged(
                                &autopilot.ascent_profile,
                                up_dir,
                                pitch_axis,
                                altitude_m,
                                autopilot.time_since_liftoff_s,
                                autopilot.ascent_pitch_over_engaged,
                            ),
                            rocket.dynamics.orientation,
                        )
                    }
                    // A polar site or unreachable inclination has no safe
                    // launch azimuth. Hold vertical rather than substitute a
                    // world axis and silently enter a wrong ascent plane.
                    Err(_) => {
                        roll_preserving_attitude_from_direction(up_dir, rocket.dynamics.orientation)
                    }
                };
                commands.throttle_cmd = 1.0;

                // Above the sensible atmosphere the fixed pitch ramp keeps
                // adding radial velocity after the vehicle is already near
                // orbital altitude, so the apoapsis runs away while the
                // periapsis stays low. Command a bounded tilt about the
                // horizontal that tracks a radial-velocity setpoint: positive
                // while still climbing to the target altitude, zero at the
                // target, negative when the vehicle is above or falling.
                // Below the gate altitude the schedule is untouched, so the
                // tower-clearance and gravity-turn baselines are unaffected.
                if autopilot.ascent_pitch_over_engaged && altitude_m >= RADIAL_NULL_ALTITUDE_M {
                    let radial_mps = vertical_speed_mps;
                    let horizontal_mps = (inertial_velocity_mps - up_dir * radial_mps).length();
                    if inertial_velocity_mps.length() > 1.0 && horizontal_mps > 1.0 {
                        let target_radial_mps =
                            ((autopilot.target_orbit.target_apoapsis_altitude_m - altitude_m)
                                / RADIAL_ARRIVAL_TIME_S)
                                .clamp(RADIAL_RATE_MIN_MPS, RADIAL_RATE_MAX_MPS);
                        let tilt = ((target_radial_mps - radial_mps) * RADIAL_NULL_GAIN)
                            .clamp(-RADIAL_TILT_LIMIT, RADIAL_TILT_LIMIT);
                        let horizontal_dir =
                            (inertial_velocity_mps - up_dir * radial_mps).normalize();
                        commands.target_attitude = roll_preserving_attitude_from_direction(
                            (horizontal_dir + up_dir * tilt).normalize(),
                            rocket.dynamics.orientation,
                        );
                    }
                }

                // Cut when the osculating apoapsis reaches the target altitude,
                // then coast up to it. An energy cutoff ends the burn at a
                // lower altitude with the apoapsis already above the target:
                // the target semi-major axis is reached while the trajectory
                // still has a large radial component, so the coast overshoots
                // and the insertion trim has to remove altitude afterwards.
                // Apoapsis-targeted cutoff is the classical ascent profile:
                // the burn only has to raise one apsis, and circularization at
                // apoapsis is a small prograde burn.
                let target_apoapsis_radius =
                    radius_m + autopilot.target_orbit.target_apoapsis_altitude_m;
                let apoapsis_reached = state_elements.apoapsis_m.is_finite()
                    && state_elements.apoapsis_m >= target_apoapsis_radius;
                let above_atmosphere = altitude_m >= INSERTION_MIN_ALTITUDE_M;
                if target_orbit_reached || (apoapsis_reached && above_atmosphere) {
                    autopilot.mode = AutopilotMode::OrbitInsertion;
                    commands.throttle_cmd = 0.0;
                }
            }
            AutopilotMode::Transfer => {
                // Two-impulse transfer (Phase 15): this branch executes the
                // departure burn only — once the transfer apoapsis reaches
                // the target, OrbitInsertion machinery owns the coast and
                // circularization burn (burn-coast-burn composition).
                let target_radius_m = autopilot.transfer_target_radius_m;
                if target_radius_m <= radius_m {
                    // No target configured: hold and hand back.
                    autopilot.mode = AutopilotMode::OrbitInsertion;
                    commands.target_attitude = prograde_attitude(inertial_velocity_mps);
                    commands.throttle_cmd = 0.0;
                    continue;
                }
                match transfer_burn_phase(
                    radius,
                    target_radius_m,
                    orbital.apoapsis_m,
                    orbital.eccentricity,
                ) {
                    TransferBurnPhase::Departure => {
                        // Burn along the velocity vector when raising,
                        // against it when lowering.
                        let raising = target_radius_m > radius;
                        commands.throttle_cmd = 1.0;
                        commands.target_attitude = if raising {
                            prograde_attitude(inertial_velocity_mps)
                        } else {
                            attitude_from_direction(
                                -inertial_velocity_mps / inertial_velocity_mps.length().max(1e-6),
                            )
                        };
                    }
                    // Coast and arrival circularization belong to the
                    // existing insertion mode.
                    _ => autopilot.mode = AutopilotMode::OrbitInsertion,
                }
            }
            AutopilotMode::OrbitInsertion => {
                let target_apoapsis_m =
                    radius_m + autopilot.target_orbit.target_apoapsis_altitude_m;
                let target_periapsis_m =
                    radius_m + autopilot.target_orbit.target_periapsis_altitude_m;
                // Two burns, one axis at a time: first give the ellipse the
                // right apoapsis, then raise the periapsis to the target. Both
                // cut on a 1 km deadband rather than on the 25 km acceptance
                // tolerance, so the predicate confirms a trimmed orbit instead
                // of accepting a burn that stopped early.
                // Periapsis can only be raised efficiently near apoapsis and
                // apoapsis lowered near periapsis, so each burn waits for its
                // own apsis. Firing the retro trim anywhere would lower the
                // periapsis into the atmosphere and deorbit the vehicle.
                // An apsis is a radial-velocity crossing, not a distance band.
                // Starting the circularization burn a few hundred metres
                // before apoapsis makes the apoapsis recede ahead of the
                // vehicle: the periapsis can then only chase the current
                // radius while the apoapsis climbs, which drifted the result
                // from 200 km to 223 km.
                let at_apsis =
                    inertial_velocity_mps.dot(up_dir).abs() <= APSIS_RADIAL_SPEED_TOLERANCE_MPS;
                let near_apoapsis =
                    at_apsis && state_elements.apoapsis_m - radius <= APSIS_CAPTURE_DISTANCE_M;
                let near_periapsis =
                    at_apsis && radius - state_elements.periapsis_m <= APSIS_CAPTURE_DISTANCE_M;
                // Latch the circularization burn once it starts. The first
                // tick of thrust changes the osculating apsis enough to leave
                // the apsis start band even though the periapsis is still far
                // below target; stopping there and re-lighting a tick later
                // burned through the upper stage's whole ignition budget in
                // four ticks and left the engine Depleted. A running engine is
                // the authoritative "this burn is in progress" flag, so no new
                // autopilot state is needed.
                let raise_apoapsis =
                    state_elements.apoapsis_m < target_apoapsis_m - INSERTION_DEADBAND_M;
                let raise_periapsis = (near_apoapsis || core_burning)
                    && state_elements.periapsis_m < target_periapsis_m - INSERTION_DEADBAND_M;
                if raise_apoapsis || raise_periapsis {
                    commands.target_attitude = prograde_attitude(inertial_velocity_mps);
                    commands.throttle_cmd = 1.0;
                } else if near_periapsis
                    && state_elements.apoapsis_m
                        > target_apoapsis_m + autopilot.target_orbit.altitude_tolerance_m
                {
                    // Overshoot trim: burn retrograde at periapsis until the
                    // apoapsis is back inside the tolerance band.
                    commands.target_attitude = attitude_from_direction(
                        -inertial_velocity_mps / inertial_velocity_mps.length().max(1e-6),
                    );
                    commands.throttle_cmd = 1.0;
                } else {
                    commands.target_attitude = prograde_attitude(inertial_velocity_mps);
                    commands.throttle_cmd = 0.0;
                }
            }
            AutopilotMode::Deorbit => {
                // Retrograde burn to lower periapsis.
                commands.target_attitude = attitude_from_direction(
                    -inertial_velocity_mps / inertial_velocity_mps.length().max(1e-6),
                );
                commands.throttle_cmd = 1.0;

                // Check if periapsis is low enough for entry.
                if orbital.periapsis_m < descent_config.entry_interface_altitude_m + radius_m {
                    autopilot.mode = AutopilotMode::Reentry;
                    *mission_state = RocketMissionState::DeorbitBurn;
                }
            }
            AutopilotMode::Reentry => {
                // Enhanced reentry bank-angle management.
                // Guidance observes the completed prior tick. Proper acceleration
                // excludes free-fall gravity and includes all applied vehicle forces.
                let g_load = specific_force.map_or_else(
                    || {
                        aerodynamic_forces.map_or(0.0, |aero| {
                            aero.force_body.length()
                                / rocket.dynamics.mass_kg.max(1.0)
                                / STANDARD_GRAVITY_MPS2
                        })
                    },
                    |specific_force| specific_force.value.length() / STANDARD_GRAVITY_MPS2,
                );
                let heat_flux = thermal.map_or(0.0, |state| state.total_heat_flux_w_m2);
                let range_errors = target_surface_range_errors_m(
                    position_m,
                    surface_relative_velocity_mps,
                    autopilot.target_landing_position_m,
                    radius_m,
                );

                // Reference bank angle from precomputed profile (simplified).
                let reference_bank = if altitude_m > 80_000.0 {
                    30.0_f64.to_radians()
                } else if altitude_m > 40_000.0 {
                    50.0_f64.to_radians()
                } else {
                    70.0_f64.to_radians()
                };

                let bank_angle = reentry_bank_angle_enhanced(
                    altitude_m,
                    surface_relative_speed_mps,
                    dynamic_pressure_pa,
                    heat_flux,
                    g_load,
                    &descent_config,
                    range_errors.crossrange_m,
                    range_errors.downrange_m,
                    reference_bank,
                );

                // Hold angle of attack then bank about body +Y, the documented
                // longitudinal/roll axis. Control turns this attitude command
                // into RCS/gimbal torque without discarding the bank request.
                commands.target_attitude = banked_attitude_from_direction(up_dir, bank_angle);

                // Transition to powered descent when slow enough.
                if surface_relative_speed_mps < 500.0
                    && altitude_m < descent_config.powered_descent_altitude_m
                {
                    autopilot.mode = AutopilotMode::PoweredDescent;
                    *mission_state = RocketMissionState::PoweredDescent;
                }
            }
            AutopilotMode::PoweredDescent => {
                let target_pos = autopilot.target_landing_position_m;
                if target_pos.length() < 1.0 {
                    // Default to point below current position.
                    autopilot.target_landing_position_m =
                        default_surface_landing_target(position_m, radius_m);
                }

                let max_thrust = {
                    let landing_engines = landing_burn_engine_count(
                        entry_burn.as_deref(),
                        surface_relative_speed_mps,
                        radar_altitude_m,
                    );
                    if let (Some(count), Some(selection)) =
                        (landing_engines, engine_selection.as_deref_mut())
                    {
                        selection.active_count = count;
                    }
                    available_core_thrust_n(
                        propulsion,
                        conditions.ambient_pressure_pa,
                        landing_engines.or(selected_engine_count),
                    )
                };
                if max_thrust <= 0.0 {
                    *mission_state = RocketMissionState::UnpoweredDescent;
                    autopilot.mode = AutopilotMode::Off;
                    commands.throttle_cmd = 0.0;
                    continue;
                }

                // Estimate gravity at current altitude.
                let gravity_accel = mu_m3_s2 / (radius * radius);

                let (thrust_vec, thrust_att) = terminal_landing_guidance(
                    position_m,
                    surface_relative_velocity_mps,
                    autopilot.target_landing_position_m,
                    radar_altitude_m,
                    rocket.dynamics.mass_kg,
                    max_thrust,
                    gravity_accel,
                    &mut autopilot.landing_burn_sequence,
                );
                commands.target_attitude = roll_preserving_attitude_from_direction(
                    thrust_att * DVec3::Y,
                    rocket.dynamics.orientation,
                );
                commands.throttle_cmd = (thrust_vec.length() / max_thrust).clamp(0.0, 1.0) as f32;

                // Check for terminal guidance transition.
                if radar_altitude_m < descent_config.terminal_descent_altitude_m {
                    autopilot.mode = AutopilotMode::Landing;
                    *mission_state = RocketMissionState::Landing;
                }
            }
            AutopilotMode::Landing => {
                let max_thrust = {
                    let landing_engines = landing_burn_engine_count(
                        entry_burn.as_deref(),
                        surface_relative_speed_mps,
                        radar_altitude_m,
                    );
                    if let (Some(count), Some(selection)) =
                        (landing_engines, engine_selection.as_deref_mut())
                    {
                        selection.active_count = count;
                    }
                    available_core_thrust_n(
                        propulsion,
                        conditions.ambient_pressure_pa,
                        landing_engines.or(selected_engine_count),
                    )
                };

                let gravity_accel = mu_m3_s2 / (radius * radius);

                let (thrust_vec, thrust_att) = terminal_landing_guidance(
                    position_m,
                    surface_relative_velocity_mps,
                    autopilot.target_landing_position_m,
                    radar_altitude_m,
                    rocket.dynamics.mass_kg,
                    max_thrust,
                    gravity_accel,
                    &mut autopilot.landing_burn_sequence,
                );
                commands.target_attitude = roll_preserving_attitude_from_direction(
                    thrust_att * DVec3::Y,
                    rocket.dynamics.orientation,
                );
                commands.throttle_cmd = if max_thrust > 0.0 {
                    (thrust_vec.length() / max_thrust).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                };
                // Ground contact alone owns the Landed verdict.
            }
            AutopilotMode::Boostback => {
                // Booster flyback (RTLS): downrange-zeroing retrograde burn.
                // The landing target doubles as the launch-site pad position;
                // default to the sub-vehicle surface point when unset.
                if autopilot.target_landing_position_m.length() < 1.0 {
                    autopilot.target_landing_position_m = up_dir * radius_m;
                }
                let max_thrust = available_core_thrust_n(
                    propulsion,
                    conditions.ambient_pressure_pa,
                    selected_engine_count,
                );

                let boostback = boostback_guidance(
                    position_m,
                    inertial_velocity_mps,
                    autopilot.target_landing_position_m,
                    rocket.dynamics.mass_kg,
                    max_thrust,
                );
                commands.target_attitude = boostback.attitude;
                commands.throttle_cmd = boostback.throttle as f32;

                // The boostback law ends when the stage is over the pad or
                // when no burn is available. Either way, an altitude below the
                // terminal gate goes straight to the landing leg; otherwise
                // the stage must coast through the atmosphere engines-first
                // before terminal guidance is meaningful.
                let can_burn = available_core_thrust_n(
                    propulsion,
                    conditions.ambient_pressure_pa,
                    selected_engine_count,
                ) > 0.0;
                if boostback.complete || !can_burn {
                    autopilot.mode =
                        if radar_altitude_m < descent_config.terminal_descent_altitude_m {
                            AutopilotMode::Landing
                        } else {
                            AutopilotMode::EntryDescent
                        };
                }
            }
            AutopilotMode::EntryDescent => {
                // Unpowered atmospheric flight. Hold the stage engines-first
                // along the air-relative velocity so the airframe presents its
                // smallest angle of attack; grid fins trim the residual. This
                // is the attitude the fins are sized for, not a broadside
                // hold. Without propellant the stage cannot land, but it must
                // still enter in a controlled attitude.
                let air_velocity_mps = conditions.atmosphere_relative_velocity_mps;
                let direction = if air_velocity_mps.length() > 1.0 {
                    -air_velocity_mps.normalize()
                } else if inertial_velocity_mps.length() > 1.0 {
                    -inertial_velocity_mps.normalize()
                } else {
                    up_dir
                };
                commands.target_attitude =
                    roll_preserving_attitude_from_direction(direction, rocket.dynamics.orientation);
                commands.throttle_cmd = 0.0;

                // A downrange recovery latches an entry burn at a descending
                // altitude gate. The attitude law above already points the
                // engines into the flow, so the burn only needs throttle and
                // the correct engine subset. The latch lives in the domain
                // sequence, not in a velocity threshold, so it cannot chatter
                // through an apogee or re-light after cutoff.
                if let (Some(burn), Some(selection)) =
                    (entry_burn.as_mut(), engine_selection.as_mut())
                {
                    let was_started = burn.sequence.started;
                    let was_complete = burn.sequence.complete;
                    let command = burn.sequence.advance(
                        altitude_m,
                        surface_relative_velocity_mps.dot(up_dir),
                        surface_relative_speed_mps,
                        &burn.plan,
                    );
                    if !was_started && burn.sequence.started {
                        info!(
                            "Entry burn ignition at {:.1} km, {:.0} m/s: {} engines, {:.0} kg propellant",
                            altitude_m / 1_000.0,
                            surface_relative_speed_mps,
                            burn.plan.engines,
                            available_propellant_kg(propulsion)
                        );
                    }
                    if !was_complete && burn.sequence.complete {
                        info!(
                            "Entry burn cutoff at {:.1} km, {:.0} m/s: {:.0} kg propellant",
                            altitude_m / 1_000.0,
                            surface_relative_speed_mps,
                            available_propellant_kg(propulsion)
                        );
                    }
                    match command {
                        EntryBurnCommand::Burn => {
                            commands.throttle_cmd = 1.0;
                            selection.active_count = burn.plan.engines;
                        }
                        EntryBurnCommand::Coast => {
                            selection.active_count = burn.plan.engines;
                        }
                        EntryBurnCommand::Complete => {
                            selection.active_count = burn.plan.landing_engines;
                        }
                    }
                }

                if radar_altitude_m < descent_config.powered_descent_altitude_m {
                    if available_core_thrust_n(
                        propulsion,
                        conditions.ambient_pressure_pa,
                        selected_engine_count,
                    ) > 0.0
                    {
                        autopilot.mode = AutopilotMode::PoweredDescent;
                        *mission_state = RocketMissionState::PoweredDescent;
                    } else if radar_altitude_m < descent_config.terminal_descent_altitude_m {
                        autopilot.mode = AutopilotMode::Landing;
                    }
                }
            }
            AutopilotMode::StationKeep => {
                // Maintain position relative to target (for orbital station-keeping).
                commands.target_attitude = prograde_attitude(inertial_velocity_mps);
                commands.throttle_cmd = 0.0;
            }
            AutopilotMode::Rendezvous => {
                // Future: rendezvous guidance.
                commands.target_attitude = prograde_attitude(inertial_velocity_mps);
                commands.throttle_cmd = 0.0;
            }
            AutopilotMode::Off => {
                // Entering an unpowered coast must not retain the ascent
                // integral or a prograde target that keeps the vehicle turning.
                // The controller will still damp any residual body rates via RCS.
                autopilot.integral = DVec3::ZERO;
                commands.target_attitude = rocket.dynamics.orientation;
                commands.throttle_cmd = 0.0;
            }
        }

        // Opt-in, simulation-time sampled guidance evidence. This observes the
        // authoritative state and commands without affecting the controller.
        if matches!(
            autopilot.mode,
            AutopilotMode::PoweredDescent | AutopilotMode::Landing
        ) && !matches!(
            *mission_state,
            RocketMissionState::Landed | RocketMissionState::Crashed
        ) && sim_time.sim_time_s.rem_euclid(1.0) < dt
        {
            debug!(
                target: "rocket_landing",
                entity = ?access.entity,
                t_s = sim_time.sim_time_s,
                sequence = ?autopilot.landing_burn_sequence,
                altitude_m = radar_altitude_m,
                vertical_speed_mps = surface_relative_velocity_mps.dot(up_dir),
                lateral_speed_mps = surface_relative_velocity_mps.cross(up_dir).length(),
                lateral_error_m = (autopilot.target_landing_position_m - position_m).cross(up_dir).length(),
                actual_tilt_deg = (rocket.dynamics.orientation * DVec3::Y).angle_between(up_dir).to_degrees(),
                target_tilt_deg = (commands.target_attitude * DVec3::Y).angle_between(up_dir).to_degrees(),
                attitude_error_deg = (rocket.dynamics.orientation * DVec3::Y).angle_between(commands.target_attitude * DVec3::Y).to_degrees(),
                angular_speed_radps = rocket.dynamics.angular_velocity_radps.length(),
                throttle_cmd = commands.throttle_cmd,
                throttle = propulsion.throttle,
                propellant_kg = available_propellant_kg(propulsion),
                "terminal guidance sample"
            );
        }

        // Entry observability: the landing limits trace back to arrival energy,
        // so the entry phase is sampled with the same 1 Hz, opt-in trace. It
        // reports the shared atmosphere sample and the aero force the physics
        // already computed; it computes nothing for the simulation.
        if matches!(
            autopilot.mode,
            AutopilotMode::EntryDescent | AutopilotMode::Reentry
        ) && sim_time.sim_time_s.rem_euclid(1.0) < dt
        {
            let nose = rocket.dynamics.orientation * DVec3::Y;
            let air = conditions.atmosphere_relative_velocity_mps;
            debug!(
                target: "rocket_entry",
                entity = ?access.entity,
                t_s = sim_time.sim_time_s,
                altitude_km = altitude_m / 1_000.0,
                airspeed_mps = surface_relative_speed_mps,
                mach = conditions.mach_number,
                q_kpa = conditions.dynamic_pressure_pa / 1_000.0,
                alpha_deg = nose.angle_between(air).to_degrees(),
                aero_accel_mps2 = aerodynamic_forces
                    .map_or(0.0, |aero| aero.force_body.length() / rocket.dynamics.mass_kg),
                g_load = specific_force.map_or_else(
                    || {
                        aerodynamic_forces.map_or(0.0, |aero| {
                            aero.force_body.length()
                                / rocket.dynamics.mass_kg.max(1.0)
                                / STANDARD_GRAVITY_MPS2
                        })
                    },
                    |force| force.value.length() / STANDARD_GRAVITY_MPS2,
                ),
                heat_flux_w_m2 = thermal.map_or(0.0, |state| state.total_heat_flux_w_m2),
                vertical_speed_mps = surface_relative_velocity_mps.dot(up_dir),
                throttle = propulsion.throttle,
                propellant_kg = available_propellant_kg(propulsion),
                "entry sample"
            );
        }

        // Reentry-corridor mission state receives a bank-angle command.
        if *mission_state == RocketMissionState::ReentryCorridor
            && autopilot.mode == AutopilotMode::Off
        {
            let g_load = specific_force.map_or_else(
                || {
                    aerodynamic_forces.map_or(0.0, |aero| {
                        aero.force_body.length()
                            / rocket.dynamics.mass_kg.max(1.0)
                            / STANDARD_GRAVITY_MPS2
                    })
                },
                |specific_force| specific_force.value.length() / STANDARD_GRAVITY_MPS2,
            );
            let heat_flux = thermal.map_or(0.0, |state| state.total_heat_flux_w_m2);
            let range_errors = target_surface_range_errors_m(
                position_m,
                surface_relative_velocity_mps,
                autopilot.target_landing_position_m,
                radius_m,
            );
            let bank_angle = reentry_bank_angle(
                altitude_m,
                surface_relative_speed_mps,
                dynamic_pressure_pa,
                heat_flux,
                g_load,
                &descent_config,
                range_errors.crossrange_m,
            );
            commands.target_attitude = banked_attitude_from_direction(up_dir, bank_angle);
        }

        // Autopilot-mode edges are what insertion diagnosis needs: the mission
        // state alone does not say whether the burn was cut by the periapsis
        // criterion, by acceptance, or by fuel exhaustion.
        if autopilot.mode != previous_autopilot_mode {
            info!(
                target: "rocket_guidance",
                from = ?previous_autopilot_mode,
                to = ?autopilot.mode,
                altitude_km = altitude_m / 1000.0,
                speed_mps = inertial_velocity_mps.length(),
                apoapsis_km = (state_elements.apoapsis_m - radius_m) / 1000.0,
                periapsis_km = (state_elements.periapsis_m - radius_m) / 1000.0,
                inclination_deg = state_elements.inclination_rad.to_degrees(),
                target_inclination_deg =
                    autopilot.target_orbit.target_inclination_rad.to_degrees(),
                throttle = commands.throttle_cmd,
                "autopilot mode change"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::rocket::{EngineState, Rocket};
    use crate::domain::services::atmosphere::FlightConditions;
    use crate::domain::services::body_orientation::BodyOrientation;
    use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
    use crate::domain::services::gravity::gravitational_parameter;
    use crate::domain::services::guidance::{EntryBurnPlan, EntryBurnSequence};
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::reference_frames::{
        body_fixed_to_planet_inertial, geodetic_to_body_fixed, surface_velocity_in_planet_inertial,
    };
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
    use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
    use crate::infrastructure::bevy_adapters::entity_components::PlanetComponent;
    use bevy::math::{DMat3, DQuat};

    fn earth_orientation() -> BodyOrientation {
        BodyOrientation::from_kernel(
            NaifBodyId::EARTH,
            TdbEpoch::j2000(),
            "guidance-test-orientation".to_owned(),
            DQuat::IDENTITY,
            DVec3::Z * (std::f64::consts::TAU / (23.934 * 3_600.0)),
        )
    }

    fn earth_snapshot(orientation: BodyOrientation) -> EphemerisSnapshot {
        let earth_mass_kg = PlanetFactory::create_by_name("Earth")
            .expect("Earth exists")
            .mass_kg;
        EphemerisSnapshot::from_states_orientations_and_gravitational_parameters(
            Vec::new(),
            vec![orientation],
            vec![(NaifBodyId::EARTH, gravitational_parameter(earth_mass_kg))],
        )
    }

    fn spawn_descent_rocket(
        app: &mut App,
        position_m: DVec3,
        inertial_velocity_mps: DVec3,
        target_position_m: DVec3,
        mission_state: RocketMissionState,
        autopilot_mode: AutopilotMode,
        radar_altitude_m: f64,
    ) -> Entity {
        let vehicle = Rocket::falcon9_test_fixture();
        let propellant_remaining_kg = vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        let conditions = RocketFlightConditions::from_sample(FlightConditions {
            atmosphere_relative_velocity_mps: DVec3::ZERO,
            airspeed_mps: 0.0,
            ..default()
        });

        app.world_mut()
            .spawn((
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        position_m,
                        inertial_velocity_mps,
                        DQuat::from_rotation_arc(DVec3::Y, position_m.normalize()),
                        100_000.0,
                        DMat3::IDENTITY,
                        DVec3::ZERO,
                    ),
                },
                RocketGeometry {
                    radius_m: 1.85,
                    height_m: 70.0,
                    lower_extent_y_m: -35.0,
                },
                mission_state,
                RocketAutopilot {
                    mode: autopilot_mode,
                    target_landing_position_m: target_position_m,
                    ..Default::default()
                },
                RocketPropulsion {
                    vehicle,
                    active_stage: 0,
                    propellant_remaining_kg,
                    booster_attachment: BoosterAttachmentState::Detached,
                    throttle: 0.0,
                    gimbal_pitch_rad: 0.0,
                    gimbal_yaw_rad: 0.0,
                    time_since_separation_s: 0.0,
                    ullage_settle_time_s: 0.0,
                    separations_count: 0,
                    attached_payload_kg: 0.0,
                },
                conditions,
                TerrainCollisionState {
                    radar_altitude_m,
                    ..Default::default()
                },
                OrbitalElements::default(),
                RocketCommands::default(),
                RocketPlanetBinding {
                    planet_name: CelestialBodyId::earth(),
                },
            ))
            .id()
    }

    /// Spawn a coasting upper stage in the insertion phase, with the engine
    /// either off or already running to represent a burn in progress.
    fn spawn_insertion_rocket(
        app: &mut App,
        position_m: DVec3,
        inertial_velocity_mps: DVec3,
        engines_running: bool,
    ) -> Entity {
        let mut vehicle = Rocket::falcon9_test_fixture();
        let propellant_remaining_kg: Vec<f32> = vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        for engine in &mut vehicle.stages[1].engines {
            if engines_running {
                engine.state = EngineState::Running;
                engine.ignition_count = 1;
            } else {
                engine.reset_lifecycle();
            }
        }
        app.world_mut()
            .spawn((
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        position_m,
                        inertial_velocity_mps,
                        DQuat::from_rotation_arc(DVec3::Y, position_m.normalize()),
                        100_000.0,
                        DMat3::IDENTITY,
                        DVec3::ZERO,
                    ),
                },
                RocketGeometry {
                    radius_m: 1.85,
                    height_m: 70.0,
                    lower_extent_y_m: -35.0,
                },
                RocketMissionState::Orbit,
                RocketAutopilot {
                    mode: AutopilotMode::OrbitInsertion,
                    ..Default::default()
                },
                RocketPropulsion {
                    vehicle,
                    active_stage: 1,
                    propellant_remaining_kg,
                    booster_attachment: BoosterAttachmentState::Detached,
                    throttle: 0.9,
                    gimbal_pitch_rad: 0.0,
                    gimbal_yaw_rad: 0.0,
                    time_since_separation_s: 10.0,
                    ullage_settle_time_s: 2.0,
                    separations_count: 1,
                    attached_payload_kg: 0.0,
                },
                RocketFlightConditions::from_sample(FlightConditions {
                    atmosphere_relative_velocity_mps: DVec3::ZERO,
                    airspeed_mps: 0.0,
                    ..default()
                }),
                TerrainCollisionState::default(),
                OrbitalElements::default(),
                RocketCommands::default(),
                RocketPlanetBinding {
                    planet_name: CelestialBodyId::earth(),
                },
            ))
            .id()
    }

    fn insertion_app() -> App {
        let orientation = earth_orientation();
        let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
        let mut app = App::new();
        app.insert_resource(SimulationTime::new(1.0 / 64.0));
        app.insert_resource(earth_snapshot(orientation));
        app.world_mut().spawn(PlanetComponent {
            domain_planet: earth,
            material: Handle::default(),
            has_texture: false,
            base_reflectance: 1.0,
            base_roughness: 1.0,
        });
        app.add_systems(Update, guidance_system);
        app
    }

    #[test]
    fn deck_flare_uses_lower_extent_clearance_like_terrain() {
        for tilt_rad in [0.0_f64, 0.4] {
            let mut app = insertion_app();
            let deck = app.world_mut().spawn_empty().id();
            let target = DVec3::X * 6_371_000.0;
            let attitude = roll_preserving_attitude_from_direction(
                DVec3::new(tilt_rad.cos(), tilt_rad.sin(), 0.0),
                DQuat::IDENTITY,
            );
            let position = target + DVec3::X * (5.0 + 35.0 * tilt_rad.cos());
            let mut stages = Vec::new();
            for on_deck in [false, true] {
                let stage = spawn_descent_rocket(
                    &mut app,
                    position,
                    -DVec3::X * 10.0,
                    target,
                    RocketMissionState::Landing,
                    AutopilotMode::Landing,
                    5.0,
                );
                app.world_mut()
                    .get_mut::<RocketPhysicsState>(stage)
                    .unwrap()
                    .dynamics
                    .orientation = attitude;
                app.world_mut()
                    .entity_mut(stage)
                    .insert(RocketFlightConditions::from_sample(FlightConditions {
                        atmosphere_relative_velocity_mps: -DVec3::X * 10.0,
                        airspeed_mps: 10.0,
                        ..default()
                    }));
                if on_deck {
                    app.world_mut()
                        .entity_mut(stage)
                        .insert(DroneShipLandingTarget {
                            drone_ship: deck,
                            prediction_horizon_s: 0.0,
                            deck_contact: false,
                        });
                }
                stages.push(stage);
            }
            app.update();
            let pad = app.world().get::<RocketCommands>(stages[0]).unwrap();
            let deck = app.world().get::<RocketCommands>(stages[1]).unwrap();
            assert!(
                (pad.throttle_cmd - deck.throttle_cmd).abs() < 1e-6,
                "the same 5 m contact clearance must command the same flare"
            );
        }
    }

    /// Build a state exactly on the orbit with the given apsides. The radial
    /// speed is derived from the orbit rather than chosen, so the apsides the
    /// guidance computes match the arguments.
    fn insertion_state(
        radius_m: f64,
        apoapsis_m: f64,
        periapsis_m: f64,
        radial_sign: f64,
    ) -> (DVec3, DVec3) {
        let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
        let mu = gravitational_parameter(earth.mass_kg);
        let semi_major_m = 0.5 * (apoapsis_m + periapsis_m);
        let tangential_speed_mps = (mu * apoapsis_m * periapsis_m / semi_major_m).sqrt() / radius_m;
        let speed_squared_m2_s2 = mu * (2.0 / radius_m - 1.0 / semi_major_m);
        let radial_speed_mps = (speed_squared_m2_s2 - tangential_speed_mps.powi(2))
            .max(0.0)
            .sqrt()
            * radial_sign;
        (
            DVec3::X * radius_m,
            DVec3::Y * tangential_speed_mps + DVec3::X * radial_speed_mps,
        )
    }

    #[test]
    fn exhausted_boostback_hands_off_to_engines_first_entry_descent() {
        let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
        let radius_m = earth.radius_km as f64 * 1000.0;
        let up = DVec3::X;
        let position_m = up * (radius_m + 10_000.0);
        let velocity_mps = -up * 300.0 + DVec3::Y * 40.0;
        let mut app = insertion_app();
        let entity = spawn_descent_rocket(
            &mut app,
            position_m,
            velocity_mps,
            position_m,
            RocketMissionState::Landing,
            AutopilotMode::Boostback,
            10_000.0,
        );
        {
            let world = app.world_mut();
            let mut propulsion = world
                .get_mut::<RocketPropulsion>(entity)
                .expect("propulsion");
            propulsion.propellant_remaining_kg =
                vec![0.0; propulsion.propellant_remaining_kg.len()];
        }

        // The boostback burn has nothing left to burn: the stage must not stay
        // in the boostback attitude law for the entry.
        app.update();
        assert_eq!(
            app.world()
                .get::<RocketAutopilot>(entity)
                .expect("autopilot")
                .mode,
            AutopilotMode::EntryDescent
        );

        app.update();
        let forward = app
            .world()
            .get::<RocketPhysicsState>(entity)
            .expect("physics")
            .dynamics
            .orientation
            * DVec3::Y;
        let expected = -velocity_mps.normalize();
        assert!(
            forward.dot(expected) > 0.99,
            "entry attitude must point the engines into the flow: forward {forward:?}, expected {expected:?}"
        );
        assert_eq!(
            app.world()
                .get::<RocketCommands>(entity)
                .expect("commands")
                .throttle_cmd,
            0.0
        );
    }

    #[test]
    fn ground_pad_target_follows_the_rotating_body() {
        let site = LaunchSiteCoordinates::default();
        let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
        let radius_m = earth.radius_km as f64 * 1000.0;
        let orientation = earth_orientation();
        let pad_body_fixed = geodetic_to_body_fixed(&site, &earth).normalize()
            * (radius_m + f64::from(site.altitude_m));
        let pad_j2000 = body_fixed_to_planet_inertial(pad_body_fixed, &orientation);

        let mut app = insertion_app();
        let entity = spawn_descent_rocket(
            &mut app,
            pad_j2000 + pad_j2000.normalize() * 1_000.0,
            DVec3::ZERO,
            DVec3::ZERO,
            RocketMissionState::Landing,
            AutopilotMode::Boostback,
            1_000.0,
        );
        app.world_mut().entity_mut(entity).insert(site.clone());
        app.update();
        let target_j2000 = app
            .world()
            .get::<RocketAutopilot>(entity)
            .expect("autopilot")
            .target_landing_position_m;
        assert!(
            (target_j2000 - pad_j2000).length() < 1.0,
            "ground target must be the pad, got {}",
            (target_j2000 - pad_j2000).length()
        );

        let later_orientation = BodyOrientation::from_kernel(
            NaifBodyId::EARTH,
            TdbEpoch::from_seconds_since_j2000(3_600.0).expect("one hour"),
            "guidance-test-orientation-later".to_owned(),
            DQuat::from_rotation_z(std::f64::consts::TAU / 24.0),
            DVec3::Z * (std::f64::consts::TAU / (23.934 * 3_600.0)),
        );
        let pad_later = body_fixed_to_planet_inertial(pad_body_fixed, &later_orientation);
        app.insert_resource(earth_snapshot(later_orientation));
        app.update();
        let target_later = app
            .world()
            .get::<RocketAutopilot>(entity)
            .expect("autopilot")
            .target_landing_position_m;
        assert!(
            (target_later - pad_later).length() < 1.0,
            "target must follow the rotated pad, got {}",
            (target_later - pad_later).length()
        );
        assert!(
            (target_later - target_j2000).length() > 100_000.0,
            "one hour of Earth rotation must move the pad target"
        );

        // A moving-deck target owns the command input.
        let sentinel = DVec3::new(1.0, 2.0, 3.0);
        app.world_mut()
            .get_mut::<RocketAutopilot>(entity)
            .expect("autopilot")
            .target_landing_position_m = sentinel;
        app.world_mut()
            .entity_mut(entity)
            .insert(DroneShipLandingTarget {
                drone_ship: Entity::PLACEHOLDER,
                prediction_horizon_s: 1.0,
                deck_contact: false,
            });
        app.update();
        assert_eq!(
            app.world()
                .get::<RocketAutopilot>(entity)
                .expect("autopilot")
                .target_landing_position_m,
            sentinel,
            "the pad update must not overwrite a moving-deck target"
        );
    }

    /// At an apsis crossing with the periapsis below target, insertion must
    /// command a prograde circularization burn.
    #[test]
    fn insertion_starts_a_prograde_burn_at_the_apoapsis() {
        let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
        let surface_m = earth.radius_km as f64 * 1_000.0;
        let (position_m, velocity_mps) = insertion_state(
            surface_m + 200_000.0,
            surface_m + 200_000.0,
            surface_m + 68_000.0,
            0.0,
        );
        let mut app = insertion_app();
        let rocket = spawn_insertion_rocket(&mut app, position_m, velocity_mps, false);
        app.update();

        let commands = app.world().get::<RocketCommands>(rocket).unwrap();
        let fired = commands.target_attitude * DVec3::Y;
        assert_eq!(commands.throttle_cmd, 1.0, "apoapsis trim must light");
        assert!(
            fired.dot(velocity_mps.normalize()) > 0.99,
            "the circularization burn must be prograde"
        );
    }

    /// A burn that has started must continue even after the apsis test stops
    /// matching; otherwise the engine cycles on/off and burns its ignition
    /// budget without circularizing.
    #[test]
    fn insertion_burn_latches_until_the_periapsis_target() {
        let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
        let surface_m = earth.radius_km as f64 * 1_000.0;
        // Fifty metres past apoapsis: outside the radial-speed crossing test,
        // still far from the periapsis target.
        let (position_m, velocity_mps) = insertion_state(
            surface_m + 200_000.0 - 50.0,
            surface_m + 200_000.0,
            surface_m + 68_000.0,
            -1.0,
        );
        let mut app = insertion_app();
        let rocket = spawn_insertion_rocket(&mut app, position_m, velocity_mps, true);
        app.update();

        let commands = app.world().get::<RocketCommands>(rocket).unwrap();
        assert_eq!(
            commands.throttle_cmd, 1.0,
            "a running circularization burn must not be cut by the apsis test"
        );
    }

    /// No burn before the apsis: thrust applied while still climbing makes the
    /// apoapsis recede and the periapsis target unreachable.
    #[test]
    fn insertion_does_not_start_before_the_apoapsis() {
        let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
        let surface_m = earth.radius_km as f64 * 1_000.0;
        let (position_m, velocity_mps) = insertion_state(
            surface_m + 195_000.0,
            surface_m + 200_000.0,
            surface_m + 68_000.0,
            1.0,
        );
        let mut app = insertion_app();
        let rocket = spawn_insertion_rocket(&mut app, position_m, velocity_mps, false);
        app.update();

        let commands = app.world().get::<RocketCommands>(rocket).unwrap();
        assert_eq!(
            commands.throttle_cmd, 0.0,
            "the circularization burn must wait for the apoapsis"
        );
        assert_eq!(
            app.world().get::<RocketAutopilot>(rocket).unwrap().mode,
            AutopilotMode::OrbitInsertion,
            "waiting is not a phase change"
        );
    }

    /// A trimmed orbit must coast instead of continuing to burn.
    #[test]
    fn insertion_stops_when_the_orbit_is_circular() {
        let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
        let surface_m = earth.radius_km as f64 * 1_000.0;
        let (position_m, velocity_mps) = insertion_state(
            surface_m + 200_000.0,
            surface_m + 200_000.0,
            surface_m + 200_000.0,
            0.0,
        );
        let mut app = insertion_app();
        let rocket = spawn_insertion_rocket(&mut app, position_m, velocity_mps, false);
        app.update();

        let commands = app.world().get::<RocketCommands>(rocket).unwrap();
        assert_eq!(commands.throttle_cmd, 0.0, "a circular orbit needs no trim");
    }

    #[test]
    fn ksc_surface_velocity_does_not_trigger_descent_lateral_correction() {
        let orientation = earth_orientation();
        let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
        let ksc_surface_position_m = body_fixed_to_planet_inertial(
            geodetic_to_body_fixed(&LaunchSiteCoordinates::default(), &earth),
            &orientation,
        );
        let up_dir = ksc_surface_position_m.normalize();
        let position_m = ksc_surface_position_m + up_dir * 1_000.0;
        let inertial_velocity_mps = surface_velocity_in_planet_inertial(position_m, &orientation);
        let target_position_m = up_dir * (earth.radius_km as f64 * 1_000.0);

        assert!(
            (400.0..420.0).contains(&inertial_velocity_mps.length()),
            "KSC's rotating-surface speed must expose the inertial-velocity regression"
        );

        let mut app = App::new();
        app.insert_resource(SimulationTime::new(1.0 / 64.0));
        app.insert_resource(earth_snapshot(orientation));
        app.world_mut().spawn(PlanetComponent {
            domain_planet: earth,
            material: Handle::default(),
            has_texture: false,
            base_reflectance: 1.0,
            base_roughness: 1.0,
        });

        let reentry = spawn_descent_rocket(
            &mut app,
            position_m,
            inertial_velocity_mps,
            target_position_m,
            RocketMissionState::ReentryCorridor,
            AutopilotMode::Reentry,
            1_000.0,
        );
        let powered_descent = spawn_descent_rocket(
            &mut app,
            position_m,
            inertial_velocity_mps,
            target_position_m,
            RocketMissionState::PoweredDescent,
            AutopilotMode::PoweredDescent,
            1_000.0,
        );
        let landing = spawn_descent_rocket(
            &mut app,
            position_m,
            inertial_velocity_mps,
            target_position_m,
            RocketMissionState::Landing,
            AutopilotMode::Landing,
            50.0,
        );
        app.add_systems(Update, guidance_system);
        app.update();

        let world = app.world();
        assert_eq!(
            *world.get::<RocketMissionState>(reentry).unwrap(),
            RocketMissionState::PoweredDescent,
            "the descent handoff must use the zero shared surface-relative speed"
        );
        assert_eq!(
            world.get::<RocketAutopilot>(reentry).unwrap().mode,
            AutopilotMode::PoweredDescent
        );
        for entity in [reentry, powered_descent, landing] {
            let commanded_up =
                world.get::<RocketCommands>(entity).unwrap().target_attitude * DVec3::Y;
            assert!(
                commanded_up.dot(up_dir) > 1.0 - 1e-12,
                "co-moving vehicle received a false lateral correction: {commanded_up}"
            );
        }
    }

    #[test]
    fn configured_orbit_inclinations_produce_distinct_ascent_headings() {
        let orientation = earth_orientation();
        let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
        let radius_m = earth.radius_km as f64 * 1_000.0;
        let position_m = DVec3::X * (radius_m + 50_000.0);
        let up_dir = position_m.normalize();
        let inertial_velocity_mps = up_dir * 300.0;

        let mut app = App::new();
        app.insert_resource(SimulationTime::new(1.0 / 64.0));
        app.insert_resource(earth_snapshot(orientation));
        app.world_mut().spawn(PlanetComponent {
            domain_planet: earth,
            material: Handle::default(),
            has_texture: false,
            base_reflectance: 1.0,
            base_roughness: 1.0,
        });
        let low_inclination = spawn_descent_rocket(
            &mut app,
            position_m,
            inertial_velocity_mps,
            DVec3::ZERO,
            RocketMissionState::Ascent,
            AutopilotMode::Ascent,
            50_000.0,
        );
        let high_inclination = spawn_descent_rocket(
            &mut app,
            position_m,
            inertial_velocity_mps,
            DVec3::ZERO,
            RocketMissionState::Ascent,
            AutopilotMode::Ascent,
            50_000.0,
        );
        for (entity, inclination_rad) in [
            (low_inclination, 28.5_f64.to_radians()),
            (high_inclination, 60.0_f64.to_radians()),
        ] {
            let mut autopilot = app.world_mut().get_mut::<RocketAutopilot>(entity).unwrap();
            autopilot.target_orbit.target_inclination_rad = inclination_rad;
            // Past the existing gate and smoothing schedule so this asserts the
            // horizontal heading selected by the real guidance adapter.
            autopilot.time_since_liftoff_s = 200.0;
        }
        app.add_systems(Update, guidance_system);
        app.update();

        let heading = |entity| {
            let direction = app
                .world()
                .get::<RocketCommands>(entity)
                .unwrap()
                .target_attitude
                * DVec3::Y;
            (direction - up_dir * direction.dot(up_dir)).normalize()
        };
        let low_heading = heading(low_inclination);
        let high_heading = heading(high_inclination);

        assert!(
            low_heading.dot(high_heading) < 0.9,
            "distinct inclination targets must not share an ascent heading"
        );
    }

    #[test]
    fn engaged_ascent_guidance_keeps_pitch_over_at_apogee() {
        // Regression for the high-altitude "almost upright" defect. At apogee
        // the vertical component of velocity falls through zero, below the
        // tower-clearance gate. Because the gate is latched (as it is set by the
        // real guidance adapter), the commanded attitude must remain the
        // gravity-turn pitch, not snap back to local vertical.
        let orientation = earth_orientation();
        let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
        let radius_m = earth.radius_km as f64 * 1_000.0;
        let up_dir = DVec3::X;
        let position_m = up_dir * (radius_m + 298_000.0);
        // Nearly horizontal with a downwards vertical component (< the 30 m/s
        // gate), as happens coasting through apogee.
        let inertial_velocity_mps = DVec3::Z * 3_000.0 - up_dir * 40.0;

        let mut app = App::new();
        app.insert_resource(SimulationTime::new(1.0 / 64.0));
        app.insert_resource(earth_snapshot(orientation));
        app.world_mut().spawn(PlanetComponent {
            domain_planet: earth,
            material: Handle::default(),
            has_texture: false,
            base_reflectance: 1.0,
            base_roughness: 1.0,
        });
        let engaged = spawn_descent_rocket(
            &mut app,
            position_m,
            inertial_velocity_mps,
            DVec3::ZERO,
            RocketMissionState::Ascent,
            AutopilotMode::Ascent,
            298_000.0,
        );
        let disengaged = spawn_descent_rocket(
            &mut app,
            position_m,
            inertial_velocity_mps,
            DVec3::ZERO,
            RocketMissionState::Ascent,
            AutopilotMode::Ascent,
            298_000.0,
        );
        for (entity, latched) in [(engaged, true), (disengaged, false)] {
            let mut autopilot = app.world_mut().get_mut::<RocketAutopilot>(entity).unwrap();
            autopilot.time_since_liftoff_s = 200.0;
            autopilot.ascent_pitch_over_engaged = latched;
        }
        app.add_systems(Update, guidance_system);
        app.update();

        let target_up = |entity| {
            (app.world()
                .get::<RocketCommands>(entity)
                .unwrap()
                .target_attitude
                * DVec3::Y)
                .dot(up_dir)
        };
        assert!(
            target_up(disengaged) > 0.999,
            "a vehicle that has not cleared the gate must hold local vertical"
        );
        assert!(
            target_up(engaged) < 0.5,
            "an engaged pitch-over must stay pitched over through apogee, got {}",
            target_up(engaged)
        );
    }

    #[test]
    fn landing_burn_keeps_the_high_thrust_subset_until_slow_and_low() {
        let state = EntryBurnState {
            sequence: EntryBurnSequence::default(),
            plan: EntryBurnPlan::FALCON9_DOWNRANGE,
        };
        assert_eq!(
            landing_burn_engine_count(Some(&state), 300.0, 5_000.0),
            Some(3),
            "high speed keeps three engines"
        );
        assert_eq!(
            landing_burn_engine_count(Some(&state), 30.0, 5_000.0),
            Some(3),
            "high altitude keeps three engines even when slow"
        );
        assert_eq!(
            landing_burn_engine_count(Some(&state), 80.0, 200.0),
            Some(3),
            "a 80 m/s stage still needs the three-engine subset"
        );
        assert_eq!(
            landing_burn_engine_count(Some(&state), 30.0, 80.0),
            Some(1),
            "only the final low, slow settle drops to the center engine"
        );
        assert_eq!(
            landing_burn_engine_count(None, 80.0, 120.0),
            None,
            "a return-to-launch profile keeps its existing full-stage selection"
        );
    }
}
