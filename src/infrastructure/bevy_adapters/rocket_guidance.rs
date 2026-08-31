use crate::components::rocket::*;
use crate::domain::services::guidance::{
    advance_ascent_phase, advance_descent_phase, attitude_from_direction,
    banked_attitude_from_direction, boostback_guidance, default_surface_landing_target,
    gravity_turn_direction_gated, pitch_axis_from_reference,
    prograde_ascending_node_launch_heading, prograde_attitude, reentry_bank_angle,
    reentry_bank_angle_enhanced, target_surface_range_errors_m, terminal_landing_guidance,
    transfer_burn_phase, AutopilotMode, DescentGuidanceConfig, TransferBurnPhase,
};
use crate::domain::services::physics_orbital::orbital_elements_from_state_in_reference_frame;
use crate::domain::services::reference_frames::{
    planet_equatorial_reference_x_axis, planet_inertial_spin_axis,
};
use crate::domain::services::rocket_propulsion::stage_available_thrust_body;
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::components::{
    PlanetComponent, RocketAutopilot, RocketCommands,
};
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
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
        let radar_altitude_m = if collision.radar_altitude_m.is_finite() {
            collision.radar_altitude_m.max(0.0)
        } else {
            altitude_m
        };
        let inertial_velocity_mps = rocket.dynamics.velocity_mps;
        let surface_relative_velocity_mps = conditions.atmosphere_relative_velocity_mps;
        let surface_relative_speed_mps = conditions.airspeed_mps;
        let mu = mu_m3_s2;
        let reference_normal = planet_inertial_spin_axis(orientation);
        let reference_x_axis = planet_equatorial_reference_x_axis(orientation);
        let state_elements = orbital_elements_from_state_in_reference_frame(
            position_m,
            inertial_velocity_mps,
            mu,
            reference_normal,
            reference_x_axis,
        );
        let target_orbit_reached = autopilot.target_orbit.matches_state_in_reference_frame(
            position_m,
            inertial_velocity_mps,
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
            surface_relative_speed_mps,
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

        // Compute target attitude based on autopilot mode.
        match autopilot.mode {
            AutopilotMode::Ascent => {
                // Gated combined schedule: hold the local vertical until the
                // vehicle clears the pad/tower (altitude AND vertical speed),
                // then follow the altitude/time pitch ramp. Low-thrust
                // vehicles must never start the turn while still near the
                // ground just because the wall clock says so.
                let vertical_speed_mps = inertial_velocity_mps.dot(up_dir);
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
                        attitude_from_direction(gravity_turn_direction_gated(
                            &autopilot.ascent_profile,
                            up_dir,
                            pitch_axis,
                            altitude_m,
                            autopilot.time_since_liftoff_s,
                            vertical_speed_mps,
                        ))
                    }
                    // A polar site or unreachable inclination has no safe
                    // launch azimuth. Hold vertical rather than substitute a
                    // world axis and silently enter a wrong ascent plane.
                    Err(_) => attitude_from_direction(up_dir),
                };
                commands.throttle_cmd = 1.0;

                // Do not coast merely because apoapsis is high enough. A
                // suborbital state can satisfy that condition while retaining
                // an Earth-intersecting periapsis, leaving the upper stage in
                // Ascent with zero throttle until it impacts. The shared orbit
                // predicate validates both apsides and the target plane.
                if target_orbit_reached {
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
                if state_elements.apoapsis_m
                    < radius_m + autopilot.target_orbit.target_apoapsis_altitude_m
                        - autopilot.target_orbit.altitude_tolerance_m
                {
                    // Raise the apoapsis until the insertion coast can begin.
                    commands.target_attitude = prograde_attitude(inertial_velocity_mps);
                    commands.throttle_cmd = 1.0;
                } else {
                    // Coast to apoapsis, then circularize only near the target
                    // altitude instead of accepting an arbitrary low-e orbit.
                    commands.target_attitude = prograde_attitude(inertial_velocity_mps);
                    let near_target_apoapsis = inertial_velocity_mps.dot(up_dir).abs() <= 25.0
                        && (altitude_m - autopilot.target_orbit.target_apoapsis_altitude_m).abs()
                            <= autopilot.target_orbit.altitude_tolerance_m;
                    commands.throttle_cmd = if near_target_apoapsis { 1.0 } else { 0.0 };
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
                            aero.force_body.length() / rocket.dynamics.mass_kg.max(1.0) / 9.80665
                        })
                    },
                    |specific_force| specific_force.value.length() / 9.80665,
                );
                let heat_flux = thermal.map_or(0.0, |state| state.total_heat_flux_w_m2);
                let (crossrange, downrange) = target_surface_range_errors_m(
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
                    crossrange,
                    downrange,
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

                let max_thrust = propulsion
                    .vehicle
                    .stages
                    .get(propulsion.active_stage)
                    .map_or(0.0, |stage| {
                        stage_available_thrust_body(
                            &stage.engines,
                            1.0,
                            conditions.ambient_pressure_pa,
                        )
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
                let gravity_accel = mu_m3_s2 / (radius * radius);

                let (thrust_vec, thrust_att) = terminal_landing_guidance(
                    position_m,
                    surface_relative_velocity_mps,
                    autopilot.target_landing_position_m,
                    radar_altitude_m,
                    rocket.dynamics.mass_kg,
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
                        stage_available_thrust_body(
                            &stage.engines,
                            1.0,
                            conditions.ambient_pressure_pa,
                        )
                        .0
                        .length()
                    });

                let gravity_accel = mu_m3_s2 / (radius * radius);

                let (thrust_vec, thrust_att) = terminal_landing_guidance(
                    position_m,
                    surface_relative_velocity_mps,
                    autopilot.target_landing_position_m,
                    radar_altitude_m,
                    rocket.dynamics.mass_kg,
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
                let max_thrust = stage_available_thrust_body(
                    &propulsion.vehicle.stages[propulsion.active_stage].engines,
                    1.0,
                    conditions.ambient_pressure_pa,
                )
                .0
                .length();

                let boostback = boostback_guidance(
                    position_m,
                    inertial_velocity_mps,
                    autopilot.target_landing_position_m,
                    rocket.dynamics.mass_kg,
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

        // Reentry-corridor mission state receives a bank-angle command.
        if *mission_state == RocketMissionState::ReentryCorridor
            && autopilot.mode == AutopilotMode::Off
        {
            let g_load = specific_force.map_or_else(
                || {
                    aerodynamic_forces.map_or(0.0, |aero| {
                        aero.force_body.length() / rocket.dynamics.mass_kg.max(1.0) / 9.80665
                    })
                },
                |specific_force| specific_force.value.length() / 9.80665,
            );
            let heat_flux = thermal.map_or(0.0, |state| state.total_heat_flux_w_m2);
            let (crossrange, _) = target_surface_range_errors_m(
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
                crossrange,
            );
            commands.target_attitude = banked_attitude_from_direction(up_dir, bank_angle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::rocket::Rocket;
    use crate::domain::services::body_orientation::BodyOrientation;
    use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
    use crate::domain::services::gravity::gravitational_parameter;
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::reference_frames::{
        body_fixed_to_planet_inertial, geodetic_to_body_fixed, surface_velocity_in_planet_inertial,
    };
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
    use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
    use crate::infrastructure::bevy_adapters::components::PlanetComponent;
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
        let mut conditions = RocketFlightConditions::default();
        conditions.0.atmosphere_relative_velocity_mps = DVec3::ZERO;
        conditions.0.airspeed_mps = 0.0;

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
                    booster_propellant_remaining_kg: Vec::new(),
                    boosters_attached: false,
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
}
