use crate::components::rocket::*;
use crate::domain::services::gravity::gravitational_parameter;
use crate::domain::services::guidance::{
    advance_ascent_phase, advance_descent_phase, attitude_from_direction,
    banked_attitude_from_direction, boostback_guidance, default_surface_landing_target,
    gravity_turn_direction_gated, pitch_axis_from_reference, prograde_attitude, reentry_bank_angle,
    reentry_bank_angle_enhanced, terminal_landing_guidance, transfer_burn_phase, AutopilotMode,
    DescentGuidanceConfig, TransferBurnPhase,
};
use crate::domain::services::physics_orbital::orbital_elements_from_state_in_reference_frame;
use crate::domain::services::reference_frames::{
    planet_equatorial_reference_x_axis, planet_inertial_spin_axis,
};
use crate::domain::services::rocket_propulsion::stage_thrust_body;
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::components::{
    PlanetComponent, RocketAutopilot, RocketCommands,
};
use bevy::ecs::query::QueryData;
use bevy::math::DVec3;
use bevy::prelude::*;

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
        autopilot.target_landing_position_m =
            ship.state.predict_position(target.prediction_horizon_s);
    }
}

/// Bundled read access for the guidance stage: one rocket's mission-relevant
/// state. A derived query keeps the system signature readable and gives every
/// field an explicit name at the use site (composition over positional
/// tuples).
#[derive(QueryData)]
#[query_data(mutable)]
pub struct GuidanceAccess {
    pub binding: &'static RocketPlanetBinding,
    pub dynamics: &'static RocketPhysicsState,
    pub geometry: &'static RocketGeometry,
    pub mass: &'static RocketMass,
    pub mission_state: &'static mut RocketMissionState,
    pub autopilot: &'static mut RocketAutopilot,
    pub propulsion: &'static RocketPropulsion,
    pub conditions: &'static RocketFlightConditions,
    pub collision: &'static TerrainCollisionState,
    pub orbital: &'static OrbitalElements,
    pub commands: &'static mut RocketCommands,
}

/// Mission guidance: computes the target attitude from the mission phase and
/// current state, and advances the ascent/descent phase (Launch → Ascent →
/// Orbit → DeorbitBurn → ReentryCorridor → PoweredDescent/UnpoweredDescent →
/// Landing). Writes only the command interface; never the vehicle's motion
/// (AGENTS.md section 18).
pub fn guidance_system(
    sim_time: Res<SimulationTime>,
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
        let propulsion = access.propulsion;
        let conditions = access.conditions;
        let collision = access.collision;
        let orbital = access.orbital;
        let mass = access.mass;
        let mission_state = &mut *access.mission_state;
        let autopilot = &mut *access.autopilot;
        let commands = &mut *access.commands;

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
        let position_m = rocket.dynamics.position_m;
        let radius = position_m.length();
        if radius < 1.0 {
            continue;
        }
        let up_dir = position_m / radius;
        let altitude_m = (radius - radius_m).max(0.0);
        let radar_altitude_m = if collision.radar_altitude_m.is_finite() {
            collision.radar_altitude_m.max(0.0)
        } else {
            altitude_m
        };
        let velocity = rocket.dynamics.velocity_mps;
        let speed = velocity.length();
        let mu = gravitational_parameter(planet.domain_planet.mass_kg);
        let reference_normal = planet_inertial_spin_axis(&planet.domain_planet);
        let reference_x_axis = planet_equatorial_reference_x_axis(&planet.domain_planet);
        let state_elements = orbital_elements_from_state_in_reference_frame(
            position_m,
            velocity,
            mu,
            reference_normal,
            reference_x_axis,
        );
        let target_orbit_reached = autopilot.target_orbit.matches_state_in_reference_frame(
            position_m,
            velocity,
            mu,
            radius_m,
            reference_normal,
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

        // Check if engines are active for descent phase logic.
        let has_active_engines = propulsion.active_stage < propulsion.vehicle.stages.len()
            && propulsion
                .propellant_remaining_kg
                .get(propulsion.active_stage)
                .map(|m| *m > 0.0)
                .unwrap_or(false);

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
            speed,
            dynamic_pressure_pa,
            has_active_engines,
            &descent_config,
        )
        .into();

        // The mission state owns phase handoffs; without this synchronization a
        // descent can remain in an Off/Reentry mode and coast into the ground.
        match (*mission_state, autopilot.mode) {
            (RocketMissionState::PoweredDescent, AutopilotMode::Off | AutopilotMode::Reentry) => {
                autopilot.mode = AutopilotMode::PoweredDescent;
            }
            (RocketMissionState::Landing, AutopilotMode::PoweredDescent | AutopilotMode::Off) => {
                autopilot.mode = AutopilotMode::Landing;
            }
            _ => {}
        }

        if target_orbit_reached
            && *mission_state == RocketMissionState::Orbit
            && autopilot.mode == AutopilotMode::OrbitInsertion
        {
            autopilot.mode = AutopilotMode::Off;
        }

        // The ascent plane is fixed in the planet-inertial frame; the pitch
        // axis is the horizontal perpendicular to it.
        let pitch_axis = pitch_axis_from_reference(up_dir, DVec3::Z)
            .or_else(|| pitch_axis_from_reference(up_dir, DVec3::X))
            .unwrap_or(DVec3::X);

        // Compute target attitude based on autopilot mode.
        match autopilot.mode {
            AutopilotMode::Ascent => {
                // Gated combined schedule: hold the local vertical until the
                // vehicle clears the pad/tower (altitude AND vertical speed),
                // then follow the altitude/time pitch ramp. Low-thrust
                // vehicles must never start the turn while still near the
                // ground just because the wall clock says so.
                let vertical_speed_mps = velocity.dot(up_dir);
                commands.target_attitude = attitude_from_direction(gravity_turn_direction_gated(
                    &autopilot.ascent_profile,
                    up_dir,
                    pitch_axis,
                    altitude_m,
                    autopilot.time_since_liftoff_s,
                    vertical_speed_mps,
                ));
                commands.throttle_cmd = 1.0;

                // Cut off once the target apoapsis is established; insertion
                // mode coasts to apoapsis before the circularization burn.
                if state_elements.apoapsis_m
                    >= radius_m + autopilot.target_orbit.target_apoapsis_altitude_m
                        - autopilot.target_orbit.altitude_tolerance_m
                {
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
                    commands.target_attitude = prograde_attitude(velocity);
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
                            prograde_attitude(velocity)
                        } else {
                            attitude_from_direction(-velocity / speed.max(1e-6))
                        };
                    }
                    // Coast and arrival circularization belong to the
                    // existing insertion mode.
                    _ => autopilot.mode = AutopilotMode::OrbitInsertion,
                }
            }
            AutopilotMode::OrbitInsertion => {
                if state_elements.apoapsis_m
                    < radius_m + autopilot.target_orbit.target_apoapsis_altitude_m
                        - autopilot.target_orbit.altitude_tolerance_m
                {
                    // Raise the apoapsis until the insertion coast can begin.
                    commands.target_attitude = prograde_attitude(velocity);
                    commands.throttle_cmd = 1.0;
                } else {
                    // Coast to apoapsis, then circularize only near the target
                    // altitude instead of accepting an arbitrary low-e orbit.
                    commands.target_attitude = prograde_attitude(velocity);
                    let near_target_apoapsis = velocity.dot(up_dir).abs() <= 25.0
                        && (altitude_m - autopilot.target_orbit.target_apoapsis_altitude_m).abs()
                            <= autopilot.target_orbit.altitude_tolerance_m;
                    commands.throttle_cmd = if near_target_apoapsis { 1.0 } else { 0.0 };
                }
            }
            AutopilotMode::Deorbit => {
                // Retrograde burn to lower periapsis.
                commands.target_attitude = attitude_from_direction(-velocity / speed.max(1e-6));
                commands.throttle_cmd = 1.0;

                // Check if periapsis is low enough for entry.
                if orbital.periapsis_m < descent_config.entry_interface_altitude_m + radius_m {
                    autopilot.mode = AutopilotMode::Reentry;
                    *mission_state = RocketMissionState::DeorbitBurn;
                }
            }
            AutopilotMode::Reentry => {
                // Enhanced reentry bank-angle management.
                // Compute g-load from acceleration (gravity + aero).
                let g_load = (rocket.dynamics.velocity_mps.length() / 9.81).min(10.0); // Simplified
                let heat_flux = 0.0; // TODO: from ThermalState
                let crossrange = 0.0; // TODO: compute from target
                let downrange = 0.0; // TODO: compute from target

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
                    speed,
                    0.0, // flight path angle
                    dynamic_pressure_pa,
                    heat_flux,
                    g_load,
                    &descent_config,
                    crossrange,
                    downrange,
                    reference_bank,
                );

                // Hold angle of attack then bank about body +Y, the documented
                // longitudinal/roll axis. Control turns this attitude command
                // into RCS/gimbal torque without discarding the bank request.
                commands.target_attitude = banked_attitude_from_direction(up_dir, bank_angle);

                // Transition to powered descent when slow enough.
                if speed < 500.0 && altitude_m < descent_config.powered_descent_altitude_m {
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

                let max_thrust = propulsion
                    .vehicle
                    .stages
                    .get(propulsion.active_stage)
                    .map_or(0.0, |stage| {
                        stage_thrust_body(&stage.engines, 1.0, conditions.ambient_pressure_pa)
                            .0
                            .length()
                    });
                if max_thrust <= 0.0 {
                    *mission_state = RocketMissionState::UnpoweredDescent;
                    autopilot.mode = AutopilotMode::Off;
                    commands.throttle_cmd = 0.0;
                    continue;
                }

                // Estimate gravity at current altitude.
                let mu = gravitational_parameter(planet.domain_planet.mass_kg);
                let gravity_accel = mu / (radius * radius);

                let (thrust_vec, thrust_att) = terminal_landing_guidance(
                    position_m,
                    velocity,
                    autopilot.target_landing_position_m,
                    radar_altitude_m,
                    mass.0,
                    max_thrust,
                    gravity_accel,
                );
                commands.target_attitude = thrust_att;
                commands.throttle_cmd = (thrust_vec.length() / max_thrust).clamp(0.0, 1.0) as f32;

                // Check for terminal guidance transition.
                if radar_altitude_m < descent_config.terminal_descent_altitude_m {
                    autopilot.mode = AutopilotMode::Landing;
                    *mission_state = RocketMissionState::Landing;
                }
            }
            AutopilotMode::Landing => {
                let max_thrust = propulsion
                    .vehicle
                    .stages
                    .get(propulsion.active_stage)
                    .map_or(0.0, |stage| {
                        stage_thrust_body(&stage.engines, 1.0, conditions.ambient_pressure_pa)
                            .0
                            .length()
                    });

                let mu = gravitational_parameter(planet.domain_planet.mass_kg);
                let gravity_accel = mu / (radius * radius);

                let (thrust_vec, thrust_att) = terminal_landing_guidance(
                    position_m,
                    velocity,
                    autopilot.target_landing_position_m,
                    radar_altitude_m,
                    mass.0,
                    max_thrust,
                    gravity_accel,
                );
                commands.target_attitude = thrust_att;
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
                let max_thrust = propulsion.vehicle.stages[propulsion.active_stage]
                    .engines
                    .iter()
                    .map(|e| e.max_thrust_kn as f64 * 1000.0)
                    .sum::<f64>();

                let boostback = boostback_guidance(
                    position_m,
                    velocity,
                    autopilot.target_landing_position_m,
                    mass.0,
                    max_thrust,
                );
                commands.target_attitude = boostback.attitude;
                commands.throttle_cmd = boostback.throttle as f32;

                // Hand off to the landing leg once the pad is roughly below.
                if boostback.complete {
                    autopilot.mode = AutopilotMode::Landing;
                }
            }
            AutopilotMode::StationKeep => {
                // Maintain position relative to target (for orbital station-keeping).
                commands.target_attitude = prograde_attitude(velocity);
                commands.throttle_cmd = 0.0;
            }
            AutopilotMode::Rendezvous => {
                // Future: rendezvous guidance.
                commands.target_attitude = prograde_attitude(velocity);
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

        // For reentry corridor (legacy mission state), compute bank angle command.
        if *mission_state == RocketMissionState::ReentryCorridor
            && autopilot.mode == AutopilotMode::Off
        {
            let g_load = 1.0;
            let heat_flux = 0.0;
            let crossrange = 0.0;
            let bank_angle = reentry_bank_angle(
                altitude_m,
                speed,
                dynamic_pressure_pa,
                heat_flux,
                g_load,
                &descent_config,
                crossrange,
            );
            commands.target_attitude = banked_attitude_from_direction(up_dir, bank_angle);
        }
    }
}
