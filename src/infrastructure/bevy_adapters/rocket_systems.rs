use crate::components::rocket::*;
use crate::domain::events::{CommsBlackoutEvent, SplashdownDetectedEvent};
use crate::domain::services::actuation::{clamp_deflection, clamp_rcs_torque, limit_throttle_slew};
use crate::domain::services::aerodynamics::{
    aerodynamic_coefficients_with_nose_bluntness, aerodynamic_torque_body, angle_of_attack,
    angle_of_sideslip, center_of_pressure_m, drag_force_body, dynamic_pressure_q, lift_force_body,
    side_force_body, update_max_q,
};
use crate::domain::services::control::control_torque_body;
use crate::domain::services::entry_physics::{
    comms_blackout_active, electron_density_m3, retro_propulsion_effectiveness,
};
use crate::domain::services::gravity::{
    circular_orbit_speed_mps, gravitational_acceleration, gravitational_parameter,
};
use crate::domain::services::guidance::{
    advance_ascent_phase, advance_descent_phase, attitude_from_direction,
    gravity_turn_direction_combined, hover_slam_guidance, pitch_axis_from_reference,
    powered_descent_guidance_convex, prograde_attitude, reentry_bank_angle,
    reentry_bank_angle_enhanced, suicide_burn_guidance, AutopilotMode, DescentGuidanceConfig,
};
use crate::domain::services::physics_orbital::orbital_elements_from_state;
use crate::domain::services::rocket_propulsion::{
    active_vehicle_inertia, active_vehicle_mass, allocate_gimbal_deflections, clamp_gimbal,
    consume_propellant, gimbal_torque_body, shed_stage, stage_thrust_body,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_collision::{
    detect_ground_contact, lat_lon_from_direction, radar_altitude_m, sample_surface, GroundContact,
};
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::components::{
    AerodynamicForces, AtmosphereState, EntryPhysicsConfig, MaxQTracker, PlanetAtmosphere,
    PlanetComponent, PlanetTerrain, RocketAutopilot, RocketCommands, TerrainCollisionState,
};
use bevy::math::DVec3;
use bevy::prelude::*;

/// Fraction of the circular orbital speed at which ascent guidance declares
/// orbit insertion.
const ORBIT_SPEED_FRACTION: f64 = 0.98;

/// Compute authoritative gravitational acceleration for each rocket from its
/// dominant body (see [`RocketPlanetBinding`]) and store it for the force
/// accumulation stage. Gravity uses the rocket's f64 planet-centered inertial
/// position directly and the single gravity implementation in
/// `domain::services::gravity`.
pub fn update_rocket_gravity(
    planet_query: Query<(&PlanetComponent, &Transform)>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &mut GravityAcceleration,
    )>,
) {
    for (binding, rocket, mut gravity) in rocket_query.iter_mut() {
        let Some((planet, _)) = planet_query
            .iter()
            .find(|(planet, _)| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        gravity.value = gravitational_acceleration(
            planet.domain_planet.mass_kg,
            rocket.dynamics.position_m,
            DVec3::ZERO,
        );
    }
}

/// Compute orbital elements from rocket state vectors for telemetry and guidance.
/// Runs in FixedUpdate after gravity to use the current planet-centered inertial state.
pub fn update_orbital_elements(
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &mut OrbitalElements,
    )>,
) {
    for (binding, rocket, mut elements) in rocket_query.iter_mut() {
        let Some(planet) = planet_query
            .iter()
            .find(|planet| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        let mu = gravitational_parameter(planet.domain_planet.mass_kg);
        let state_elements = orbital_elements_from_state(
            rocket.dynamics.position_m,
            rocket.dynamics.velocity_mps,
            mu,
        );

        elements.semi_major_axis_m = state_elements.semi_major_axis_m;
        elements.eccentricity = state_elements.eccentricity;
        elements.inclination_rad = state_elements.inclination_rad;
        elements.longitude_ascending_node_rad = state_elements.longitude_ascending_node_rad;
        elements.argument_of_periapsis_rad = state_elements.argument_of_periapsis_rad;
        elements.true_anomaly_rad = state_elements.true_anomaly_rad;
        elements.mean_anomaly_rad = state_elements.mean_anomaly_rad;
        elements.orbital_period_s = state_elements.orbital_period_s;
        elements.apoapsis_m = state_elements.apoapsis_m;
        elements.periapsis_m = state_elements.periapsis_m;
    }
}

/// Accumulate the gravitational force acting on each rocket. Forces are in the
/// planet-centered inertial meter frame. Thrust is added by the propulsion
/// thrust system.
pub fn accumulate_forces(
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketMass,
        &GravityAcceleration,
        &mut ForceAccumulator,
    )>,
) {
    for (rocket, mass, gravity, mut force_accum) in rocket_query.iter_mut() {
        let gravity_force = gravity.value * mass.0;
        force_accum.0 = gravity_force;
    }
}

/// Integrate the authoritative 6-DOF dynamics (semi-implicit Euler in f64)
/// from the accumulated force/torque, then reset the accumulators. Propellant
/// depletion and staging are handled by the propulsion systems.
/// Uses SimulationTime fixed timestep for deterministic physics.
pub fn integrate_6dof(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &mut RocketPhysicsState,
        &mut RocketMass,
        &mut ForceAccumulator,
        &mut TorqueAccumulator,
    )>,
) {
    let dt = sim_time.fixed_timestep();

    for (mut rocket, mut mass, mut force_accum, mut torque_accum) in rocket_query.iter_mut() {
        let force = force_accum.0;
        let torque = torque_accum.0;
        rocket.dynamics.integrate_translation(force, dt);
        rocket.dynamics.integrate_rotation(torque, dt);
        mass.0 = rocket.dynamics.mass_kg;

        force_accum.0 = DVec3::ZERO;
        torque_accum.0 = DVec3::ZERO;
    }
}

/// Cache the atmosphere state (altitude → temperature/pressure/density/speed
/// of sound) at each rocket's position, using the shared per-planet atmosphere
/// model. Aero and propulsion systems consume this rather than recomputing
/// planet lookups or scattering their own formulas.
pub fn atmosphere_properties(
    planet_query: Query<(&PlanetComponent, &PlanetAtmosphere)>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &mut AtmosphereState,
    )>,
) {
    for (binding, rocket, mut atmosphere) in rocket_query.iter_mut() {
        let Some((planet, planet_atmosphere)) = planet_query
            .iter()
            .find(|(planet, _)| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };
        let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
        let altitude_m = (rocket.dynamics.position_m.length() - radius_m).max(0.0);
        let props = planet_atmosphere.source.properties(altitude_m);
        atmosphere.altitude_m = altitude_m;
        atmosphere.temperature_k = props.temperature_k;
        atmosphere.pressure_pa = props.pressure_pa;
        atmosphere.density_kg_m3 = props.density_kg_m3;
        atmosphere.speed_of_sound_mps = props.speed_of_sound_mps;
    }
}

/// Compute aerodynamic forces (drag, lift, side) from the atmosphere and
/// vehicle orientation, add them to the translational accumulator, track Max Q,
/// and store the body-frame force for the torque system. Never writes the
/// transform. Drag couples to ablation: a blunted (recessed) nose raises Cd
/// via `aerodynamic_coefficients_with_nose_bluntness`.
pub fn aerodynamic_forces(
    config: Res<EntryPhysicsConfig>,
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketGeometry,
        &AtmosphereState,
        &AblationState,
        &mut AerodynamicForces,
        &mut MaxQTracker,
        &mut ForceAccumulator,
    )>,
) {
    for (rocket, geometry, atmosphere, ablation, mut aero, mut max_q, mut force_accum) in
        rocket_query.iter_mut()
    {
        let velocity = rocket.dynamics.velocity_mps;
        let speed = velocity.length();
        aero.center_of_pressure_body = center_of_pressure_m(geometry.height_m as f64);
        if speed < 1.0 || atmosphere.density_kg_m3 <= 0.0 {
            aero.force_body = DVec3::ZERO;
            continue;
        }

        let q = dynamic_pressure_q(atmosphere.density_kg_m3, speed);
        max_q.max_q_pa = update_max_q(q, max_q.max_q_pa);

        let reference_area_m2 = std::f64::consts::PI * (geometry.radius_m as f64).powi(2);
        let body_velocity = rocket.dynamics.orientation.inverse() * velocity;
        // Ablation blunts the nose: ratio of current to initial nose radius.
        // Zero (or pre-heating) ablation keeps the baseline coefficients.
        let nose_radius_ratio =
            if ablation.nose_radius_m > 0.0 && config.nose_radius_initial_m > 0.0 {
                ablation.nose_radius_m / config.nose_radius_initial_m
            } else {
                1.0
            };
        let (cd, cl, cy) = aerodynamic_coefficients_with_nose_bluntness(
            angle_of_attack(body_velocity),
            angle_of_sideslip(body_velocity),
            nose_radius_ratio,
        );

        let total_body = drag_force_body(q, cd, reference_area_m2, body_velocity)
            + lift_force_body(q, cl, reference_area_m2, body_velocity)
            + side_force_body(q, cy, reference_area_m2, body_velocity);
        aero.force_body = total_body;
        let orientation = rocket.dynamics.orientation;
        force_accum.0 += orientation * total_body;
    }
}

/// Apply the aerodynamic force at the center of pressure to produce a torque
/// about the center of mass, added to the rotational accumulator (body frame).
pub fn aerodynamic_torque(
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketGeometry,
        &AerodynamicForces,
        &mut TorqueAccumulator,
    )>,
) {
    for (rocket, geometry, aero, mut torque_accum) in rocket_query.iter_mut() {
        if aero.force_body.length_squared() == 0.0 {
            continue;
        }
        let center_of_mass_m = rocket.dynamics.center_of_mass_m;
        torque_accum.0 += aerodynamic_torque_body(
            aero.force_body,
            aero.center_of_pressure_body,
            center_of_mass_m,
        );
    }
}

/// Compute thrust from the active stage's engines (T = m_dot · Isp · g0, with
/// density-selected ISP) and add it to the translational accumulator in the
/// planet-inertial frame. The supersonic retro-propulsion multiplier scales
/// the effective thrust (single thrust writer; no double counting). Never
/// writes the transform.
pub fn propulsion_thrust(
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &AtmosphereState,
        &RocketPropulsion,
        &RetroPropulsionEffect,
        &mut ForceAccumulator,
    )>,
) {
    for (rocket, atmosphere, propulsion, retro, mut force_accum) in rocket_query.iter_mut() {
        let Some(stage) = propulsion.vehicle.stages.get(propulsion.active_stage) else {
            continue;
        };
        let remaining = propulsion
            .propellant_remaining_kg
            .get(propulsion.active_stage)
            .copied()
            .unwrap_or(0.0);
        let throttle = propulsion.throttle.clamp(0.0, 1.0);
        if throttle <= 0.0 || remaining <= 0.0 {
            continue;
        }
        let (thrust_body, _) =
            stage_thrust_body(&stage.engines, throttle, atmosphere.density_kg_m3);
        let thrust_world = rocket.dynamics.orientation * thrust_body;
        force_accum.0 += thrust_world * retro.thrust_multiplier;
    }
}

/// Deplete the active stage's propellant at the engine mass flow and update the
/// vehicle mass, inertia tensor, and center of mass. Mass always derives from
/// the vehicle state (single source). Uses SimulationTime fixed timestep.
pub fn propulsion_consumption(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &mut RocketPhysicsState,
        &RocketGeometry,
        &AtmosphereState,
        &mut RocketPropulsion,
        &mut RocketMass,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (mut rocket, geometry, atmosphere, mut propulsion, mut mass) in rocket_query.iter_mut() {
        let Some(stage) = propulsion.vehicle.stages.get(propulsion.active_stage) else {
            continue;
        };
        let remaining = propulsion
            .propellant_remaining_kg
            .get(propulsion.active_stage)
            .copied()
            .unwrap_or(0.0);
        let throttle = propulsion.throttle.clamp(0.0, 1.0);
        if throttle <= 0.0 || remaining <= 0.0 {
            continue;
        }
        let (_, mass_flow) = stage_thrust_body(&stage.engines, throttle, atmosphere.density_kg_m3);
        let (remaining_new, _consumed) = consume_propellant(remaining, mass_flow, dt);
        let active_stage = propulsion.active_stage;
        propulsion.propellant_remaining_kg[active_stage] = remaining_new;

        let new_mass = active_vehicle_mass(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
        );
        mass.0 = new_mass;
        rocket.dynamics.mass_kg = new_mass;
        let (inertia, com) = active_vehicle_inertia(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
            geometry.radius_m as f64,
            geometry.height_m as f64,
        );
        rocket.dynamics.inertia_body = inertia;
        rocket.dynamics.center_of_mass_m = com;
    }
}

/// Separate the spent stage when its propellant is exhausted and the vehicle is
/// still thrusting. The shed stage's dry and residual mass is removed and the
/// vehicle mass/inertia are recomputed.
pub fn propulsion_staging(
    mut rocket_query: Query<(
        &mut RocketPhysicsState,
        &RocketGeometry,
        &mut RocketMass,
        &mut RocketPropulsion,
    )>,
) {
    for (mut rocket, geometry, mut mass, mut propulsion) in rocket_query.iter_mut() {
        let remaining = propulsion
            .propellant_remaining_kg
            .get(propulsion.active_stage)
            .copied()
            .unwrap_or(0.0);
        let thrusting = propulsion.throttle.clamp(0.0, 1.0) > 0.0;
        if remaining > 0.0 || !thrusting {
            continue;
        }
        let Some((next, _shed)) = shed_stage(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
        ) else {
            continue;
        };
        propulsion.active_stage = next;

        let new_mass = active_vehicle_mass(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
        );
        mass.0 = new_mass;
        rocket.dynamics.mass_kg = new_mass;
        let (inertia, com) = active_vehicle_inertia(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
            geometry.radius_m as f64,
            geometry.height_m as f64,
        );
        rocket.dynamics.inertia_body = inertia;
        rocket.dynamics.center_of_mass_m = com;
    }
}

/// Apply engine gimbal deflection to produce torque about the rocket's center
/// of mass, added to the rotational accumulator (body frame).
pub fn propulsion_gimbal(
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketGeometry,
        &mut RocketPropulsion,
        &mut TorqueAccumulator,
    )>,
) {
    for (rocket, geometry, mut propulsion, mut torque_accum) in rocket_query.iter_mut() {
        let Some(stage) = propulsion.vehicle.stages.get(propulsion.active_stage) else {
            continue;
        };
        let remaining = propulsion
            .propellant_remaining_kg
            .get(propulsion.active_stage)
            .copied()
            .unwrap_or(0.0);
        let throttle = propulsion.throttle.clamp(0.0, 1.0);
        if throttle <= 0.0 || remaining <= 0.0 {
            continue;
        }
        let com = rocket.dynamics.center_of_mass_m;
        for engine in &stage.engines {
            let pitch = clamp_gimbal(propulsion.gimbal_pitch_rad, engine.gimbal_range_deg) as f64;
            let yaw = clamp_gimbal(propulsion.gimbal_yaw_rad, engine.gimbal_range_deg) as f64;
            let thrust = engine.max_thrust_kn as f64 * 1000.0 * throttle as f64;
            torque_accum.0 += gimbal_torque_body(
                engine.position_m.as_dvec3(),
                com,
                engine.thrust_axis.as_dvec3(),
                thrust,
                pitch,
                yaw,
            );
        }
    }
}

/// Sync the rocket's rendered [`Transform`] and the f32 facade fields from the
/// authoritative f64 dynamics state. This is the only system that writes the
/// rocket's `Transform`.
pub fn sync_render_transform(
    planet_query: Query<(&PlanetComponent, &Transform)>,
    physical_scale: Res<PhysicalScale>,
    mut rocket_query: Query<
        (
            &RocketPlanetBinding,
            &RocketPhysicsState,
            &mut RocketFacade,
            &mut Transform,
        ),
        Without<PlanetComponent>,
    >,
) {
    for (binding, rocket, mut facade, mut transform) in rocket_query.iter_mut() {
        let Some((_, planet_transform)) = planet_query
            .iter()
            .find(|(planet, _)| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        let solar_display = planet_transform.translation.as_dvec3()
            + DVec3::new(
                physical_scale.solar_meters_to_units(rocket.dynamics.position_m.x),
                physical_scale.solar_meters_to_units(rocket.dynamics.position_m.y),
                physical_scale.solar_meters_to_units(rocket.dynamics.position_m.z),
            );

        transform.translation = solar_display.as_vec3();
        transform.rotation = rocket.dynamics.orientation.as_quat();

        // Refresh the compatible facade fields from the authoritative state.
        facade.position = transform.translation;
        facade.velocity = rocket.dynamics.velocity_mps.as_vec3();
        facade.orientation = rocket.dynamics.orientation.as_quat();
        facade.angular_velocity = rocket.dynamics.angular_velocity_radps.as_vec3();
        facade.mass = rocket.dynamics.mass_kg as f32;
    }
}

/// Mission guidance: computes the target attitude from the mission phase and
/// current state, and advances the ascent/descent phase (Launch → Ascent →
/// Orbit → DeorbitBurn → ReentryCorridor → PoweredDescent/UnpoweredDescent →
/// Landing). Writes only the command interface; never the vehicle's motion
/// (AGENTS.md section 18).
pub fn guidance_system(
    sim_time: Res<SimulationTime>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketGeometry,
        &RocketMass,
        &mut RocketMissionState,
        &mut RocketAutopilot,
        &RocketPropulsion,
        &OrbitalElements,
        &mut RocketCommands,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (
        binding,
        rocket,
        geometry,
        mass,
        mut mission_state,
        mut autopilot,
        propulsion,
        orbital,
        mut commands,
    ) in rocket_query.iter_mut()
    {
        let Some(planet) = planet_query
            .iter()
            .find(|planet| planet.domain_planet.name == binding.planet_name)
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
        let velocity = rocket.dynamics.velocity_mps;
        let speed = velocity.length();

        // Update time since liftoff for time-based ascent guidance.
        if *mission_state != RocketMissionState::PreLaunch {
            autopilot.time_since_liftoff_s += dt;
        }

        // Auto-launch: begin the ascent flight on the first guidance pass.
        if *mission_state == RocketMissionState::PreLaunch {
            *mission_state = RocketMissionState::Launch;
            autopilot.mode = AutopilotMode::Ascent;
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

        // Read dynamic pressure from cached atmosphere state.
        // Note: we'd need to add AtmosphereState to the query; for now approximate.
        let dynamic_pressure_pa = 0.0; // TODO: read from AtmosphereState

        // Advance the mission phase (ascent + descent).
        let circular_speed = circular_orbit_speed_mps(planet.domain_planet.mass_kg, radius);
        *mission_state = advance_ascent_phase(
            (*mission_state).into(),
            altitude_m,
            speed,
            circular_speed,
            autopilot.ascent_profile.ascent_start_altitude_m,
            ORBIT_SPEED_FRACTION,
        )
        .into();
        // Also advance descent phases.
        *mission_state = advance_descent_phase(
            (*mission_state).into(),
            altitude_m,
            speed,
            dynamic_pressure_pa,
            has_active_engines,
            &descent_config,
        )
        .into();

        // The ascent plane is fixed in the planet-inertial frame; the pitch
        // axis is the horizontal perpendicular to it.
        let pitch_axis = pitch_axis_from_reference(up_dir, DVec3::Z)
            .or_else(|| pitch_axis_from_reference(up_dir, DVec3::X))
            .unwrap_or(DVec3::X);

        // Compute target attitude based on autopilot mode.
        match autopilot.mode {
            AutopilotMode::Ascent => {
                // Use combined altitude/time pitch schedule for gravity turn.
                commands.target_attitude =
                    attitude_from_direction(gravity_turn_direction_combined(
                        &autopilot.ascent_profile,
                        up_dir,
                        pitch_axis,
                        altitude_m,
                        autopilot.time_since_liftoff_s,
                    ));

                // Auto-transition to OrbitInsertion when near orbital speed.
                if speed >= circular_speed * 0.95 && altitude_m > 150_000.0 {
                    autopilot.mode = AutopilotMode::OrbitInsertion;
                }
            }
            AutopilotMode::OrbitInsertion => {
                // Prograde burn to circularize at apoapsis.
                if orbital.apoapsis_m > radius_m + 100_000.0 {
                    // Not at apoapsis yet - coast.
                    commands.target_attitude = prograde_attitude(velocity);
                    commands.throttle_cmd = 0.0;
                } else {
                    // At apoapsis - circularize burn.
                    commands.target_attitude = prograde_attitude(velocity);
                    commands.throttle_cmd = 1.0;

                    // Check if orbit is circularized (eccentricity < 0.01).
                    if orbital.eccentricity < 0.01 {
                        autopilot.mode = AutopilotMode::Off;
                        *mission_state = RocketMissionState::Orbit;
                    }
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

                // Apply bank angle via RCS torque command (roll axis).
                commands.rcs_torque_cmd_body = DVec3::new(0.0, 0.0, bank_angle);

                // Hold angle of attack (nose up).
                commands.target_attitude = attitude_from_direction(up_dir);

                // Transition to powered descent when slow enough.
                if speed < 500.0 && altitude_m < descent_config.powered_descent_altitude_m {
                    autopilot.mode = AutopilotMode::PoweredDescent;
                    *mission_state = RocketMissionState::PoweredDescent;
                }
            }
            AutopilotMode::PoweredDescent => {
                // Use convex optimization powered descent guidance.
                let target_pos = autopilot.target_landing_position_m;
                if target_pos.length() < 1.0 {
                    // Default to point below current position.
                    autopilot.target_landing_position_m = position_m * (altitude_m / radius);
                }

                let max_thrust = propulsion.vehicle.stages[propulsion.active_stage]
                    .engines
                    .iter()
                    .map(|e| e.max_thrust_kn as f64 * 1000.0)
                    .sum::<f64>();
                let min_thrust = max_thrust * 0.1; // Assume 10% minimum throttle

                // Estimate gravity at current altitude.
                let mu = gravitational_parameter(planet.domain_planet.mass_kg);
                let gravity_accel = mu / (radius * radius);

                // Use orbital elements for time-to-go estimate.
                let t_go = if orbital.orbital_period_s.is_finite() {
                    orbital.orbital_period_s / 4.0 // Quarter orbit approximation
                } else {
                    60.0
                }
                .min(300.0)
                .max(10.0);

                let (thrust_vec, thrust_att) = powered_descent_guidance_convex(
                    position_m,
                    velocity,
                    autopilot.target_landing_position_m,
                    mass.0,
                    max_thrust,
                    min_thrust,
                    15.0_f64.to_radians(),
                    gravity_accel,
                    t_go,
                );
                commands.target_attitude = thrust_att;
                commands.throttle_cmd = (thrust_vec.length() / max_thrust).clamp(0.0, 1.0) as f32;

                // Check for terminal guidance transition.
                if altitude_m < descent_config.terminal_descent_altitude_m {
                    autopilot.mode = AutopilotMode::Landing;
                }
            }
            AutopilotMode::Landing => {
                // Suicide burn / hover-slam terminal guidance.
                let max_thrust = propulsion.vehicle.stages[propulsion.active_stage]
                    .engines
                    .iter()
                    .map(|e| e.max_thrust_kn as f64 * 1000.0)
                    .sum::<f64>();

                let mu = gravitational_parameter(planet.domain_planet.mass_kg);
                let gravity_accel = mu / (radius * radius);

                let (thrust_vec, thrust_att, _suicide_alt, should_burn) = suicide_burn_guidance(
                    position_m,
                    velocity,
                    autopilot.target_landing_position_m,
                    mass.0,
                    max_thrust,
                    gravity_accel,
                );

                if should_burn {
                    commands.target_attitude = thrust_att;
                    commands.throttle_cmd =
                        (thrust_vec.length() / max_thrust).clamp(0.0, 1.0) as f32;
                } else {
                    // Hover-slam: maintain low descent rate.
                    let (hv_thrust_vec, hv_att) = hover_slam_guidance(
                        position_m,
                        velocity,
                        autopilot.target_landing_position_m,
                        mass.0,
                        max_thrust,
                        gravity_accel,
                        -1.0, // Target -1 m/s descent rate
                    );
                    commands.target_attitude = hv_att;
                    commands.throttle_cmd =
                        (hv_thrust_vec.length() / max_thrust).clamp(0.0, 1.0) as f32;
                }

                // Check for touchdown.
                if altitude_m < 5.0 && speed < 2.0 {
                    *mission_state = RocketMissionState::Landed;
                    autopilot.mode = AutopilotMode::Off;
                    commands.throttle_cmd = 0.0;
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
                // Manual or no guidance.
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
            commands.rcs_torque_cmd_body = DVec3::new(0.0, 0.0, bank_angle);
        }
    }
}

/// Attitude control: converts the guidance target and current state into
/// commanded throttle, gimbal deflections, and RCS torque using the PID with
/// anti-windup. Writes only the command interface; never the vehicle's motion.
/// Uses SimulationTime fixed timestep.
pub fn control_system(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &mut RocketCommands,
        &RocketPhysicsState,
        &RocketGeometry,
        &RocketMass,
        &RocketMissionState,
        &RocketPropulsion,
        &mut RocketAutopilot,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (mut commands, rocket, geometry, mass, mission_state, propulsion, mut autopilot) in
        rocket_query.iter_mut()
    {
        // Throttle schedule from the mission phase.
        commands.throttle_cmd = match *mission_state {
            RocketMissionState::Launch | RocketMissionState::Ascent => 1.0,
            RocketMissionState::PoweredDescent => 0.7, // Hover throttle
            RocketMissionState::Landing => 0.5,        // Terminal descent
            RocketMissionState::PreLaunch
            | RocketMissionState::Orbit
            | RocketMissionState::DeorbitBurn
            | RocketMissionState::ReentryCorridor
            | RocketMissionState::UnpoweredDescent
            | RocketMissionState::Landed
            | RocketMissionState::Crashed => 0.0,
        };

        let gains = autopilot.gains;
        let torque = control_torque_body(
            commands.target_attitude,
            rocket.dynamics.orientation,
            rocket.dynamics.angular_velocity_radps,
            &gains,
            &mut autopilot.integral,
            dt,
        );

        // Allocate the commanded torque to gimbal pitch/yaw (inverting the
        // real engine geometry) and RCS (roll).
        let Some(stage) = propulsion.vehicle.stages.get(propulsion.active_stage) else {
            continue;
        };
        let (gimbal_pitch, gimbal_yaw) = allocate_gimbal_deflections(
            &stage.engines,
            rocket.dynamics.center_of_mass_m,
            torque,
            propulsion.throttle,
        );
        commands.gimbal_pitch_cmd_rad = gimbal_pitch;
        commands.gimbal_yaw_cmd_rad = gimbal_yaw;
        commands.rcs_torque_cmd_body = DVec3::new(0.0, torque.y, 0.0);
    }
}

/// Actuation: apply the physical actuator limits (throttle slew, gimbal range,
/// RCS torque) to the control commands and deliver the bounded outputs to the
/// propulsion systems and the torque accumulator. The last layer before
/// physics; it never writes the vehicle's motion directly.
/// Uses SimulationTime fixed timestep.
pub fn actuation_system(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &RocketCommands,
        &mut RocketPropulsion,
        &RocketPhysicsState,
        &RocketGeometry,
        &mut TorqueAccumulator,
        &RocketAutopilot,
    )>,
) {
    let dt = sim_time.fixed_timestep_f32();
    for (commands, mut propulsion, rocket, geometry, mut torque_accum, autopilot) in
        rocket_query.iter_mut()
    {
        let limits = autopilot.actuation;
        propulsion.throttle = limit_throttle_slew(
            propulsion.throttle,
            commands.throttle_cmd,
            limits.max_throttle_slew_per_s,
            dt,
        );
        propulsion.gimbal_pitch_rad = clamp_deflection(
            commands.gimbal_pitch_cmd_rad,
            limits.max_gimbal_deflection_rad,
        );
        propulsion.gimbal_yaw_rad = clamp_deflection(
            commands.gimbal_yaw_cmd_rad,
            limits.max_gimbal_deflection_rad,
        );
        let rcs = clamp_rcs_torque(commands.rcs_torque_cmd_body, limits.max_rcs_torque_nm);
        torque_accum.0 += rcs;
    }
}

/// Rocket–terrain interaction: sample the authoritative collision terrain
/// (radar altitude, surface normal, slope, ground contact) from the rocket's
/// f64 planet-centered inertial position and the shared per-planet
/// `TerrainSource`. Detects landing, crash, and water splashdown (event).
/// Never writes the transform; the 6-DOF dynamics remain authoritative.
pub fn update_rocket_terrain_interaction(
    mut splashdown_writer: MessageWriter<SplashdownDetectedEvent>,
    planet_query: Query<(&PlanetComponent, &PlanetTerrain)>,
    mut rocket_query: Query<(
        Entity,
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &mut TerrainCollisionState,
        &mut RocketMissionState,
    )>,
) {
    const CONTACT_ALTITUDE_M: f64 = 3.0;
    const TOUCH_DOWN_SPEED_MPS: f64 = 5.0;
    const CRASH_SPEED_MPS: f64 = 15.0;
    /// Terrain heights within this band of mean sea level are treated as
    /// water on bodies with oceans.
    const SEA_LEVEL_TOLERANCE_M: f64 = 10.0;

    for (rocket_entity, binding, rocket, mut collision, mut mission_state) in
        rocket_query.iter_mut()
    {
        let Some((planet, planet_terrain)) = planet_query
            .iter()
            .find(|(planet, _)| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };
        let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;

        let position_m = rocket.dynamics.position_m;
        let altitude_m = radar_altitude_m(&*planet_terrain.source, position_m, radius_m);
        let dir = position_m.normalize_or_zero();
        if dir.length_squared() < 1e-12 {
            continue;
        }
        let (lat, lon) = lat_lon_from_direction(dir);
        let sample = sample_surface(&*planet_terrain.source, lat, lon, radius_m);

        collision.radar_altitude_m = altitude_m;
        collision.slope_deg = sample.slope_deg;

        // Water inference: no ocean mask data exists yet, so water is where
        // the terrain elevation sits at mean sea level (Earth only — the
        // Moon/Mars have no seas). Documented approximation.
        let has_ocean = planet.domain_planet.name == "Earth";
        collision.over_water = has_ocean && sample.height_m.abs() <= SEA_LEVEL_TOLERANCE_M;

        let vertical_speed = rocket.dynamics.velocity_mps.dot(dir);
        let contact = detect_ground_contact(
            altitude_m,
            vertical_speed,
            CONTACT_ALTITUDE_M,
            TOUCH_DOWN_SPEED_MPS,
            CRASH_SPEED_MPS,
        );
        collision.ground_contact = contact;

        match contact {
            GroundContact::Landed => {
                if matches!(
                    *mission_state,
                    RocketMissionState::PoweredDescent
                        | RocketMissionState::UnpoweredDescent
                        | RocketMissionState::Landing
                        | RocketMissionState::ReentryCorridor
                ) {
                    *mission_state = RocketMissionState::Landed;
                    if collision.over_water {
                        splashdown_writer.write(SplashdownDetectedEvent {
                            rocket: rocket_entity,
                            position_m,
                            touchdown_vertical_speed_mps: vertical_speed,
                        });
                        bevy::log::info!(
                            "Splashdown detected at ({lat:.2}, {lon:.2}), vertical speed {vertical_speed:.1} m/s"
                        );
                    }
                }
            }
            GroundContact::Crash => {
                // A pad-hold at zero speed during pre-launch is not a crash.
                if *mission_state != RocketMissionState::PreLaunch {
                    *mission_state = RocketMissionState::Crashed;
                }
            }
            GroundContact::None => {}
        }
    }
}

/// Convective heating (Sutton-Graves) and radiative heating (Tauber-Sutton).
/// Runs in FixedUpdate before force accumulation. Reads AtmosphereState and
/// writes ThermalState for the ablation system.
pub fn compute_heating(
    config: Res<EntryPhysicsConfig>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketGeometry,
        &AtmosphereState,
        &mut ThermalState,
    )>,
) {
    for (binding, rocket, geometry, atmosphere, mut thermal) in rocket_query.iter_mut() {
        let Some(planet) = planet_query
            .iter()
            .find(|planet| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        let rho = atmosphere.density_kg_m3;
        let v = rocket.dynamics.velocity_mps.length();
        let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
        let r = rocket.dynamics.position_m.length();
        let altitude_m = (r - radius_m).max(0.0);

        // Skip if no meaningful atmosphere
        if rho <= 0.0 || v < 100.0 {
            thermal.convective_heat_flux_w_m2 = 0.0;
            thermal.radiative_heat_flux_w_m2 = 0.0;
            thermal.total_heat_flux_w_m2 = 0.0;
            continue;
        }

        // Convective heating: Sutton-Graves q_dot = k * sqrt(rho/R_nose) * v^3
        let nose_radius = config.nose_radius_initial_m;
        let q_conv = config.convective_coefficient * (rho / nose_radius).sqrt() * v.powi(3);
        thermal.convective_heat_flux_w_m2 = q_conv;

        // Radiative heating: Tauber-Sutton (significant for v > 10 km/s)
        let q_rad = if v > 10_000.0 {
            config.radiative_coefficient * rho * v.powi(8) / 1e24 // Simplified scaling
        } else {
            0.0
        };
        thermal.radiative_heat_flux_w_m2 = q_rad;

        thermal.total_heat_flux_w_m2 = q_conv + q_rad;
        thermal.stagnation_point_heat_flux_w_m2 = q_conv; // Stagnation point = convective peak
    }
}

/// Ablation: char-layer recession from integrated heat load.
/// Updates nose radius and mass loss in AblationState.
/// Uses SimulationTime fixed timestep.
pub fn compute_ablation(
    sim_time: Res<SimulationTime>,
    config: Res<EntryPhysicsConfig>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &mut RocketPhysicsState,
        &RocketGeometry,
        &AtmosphereState,
        &ThermalState,
        &mut AblationState,
        &mut RocketMass,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (binding, mut rocket, geometry, _atmosphere, thermal, mut ablation, mut mass) in
        rocket_query.iter_mut()
    {
        let Some(_planet) = planet_query
            .iter()
            .find(|planet| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        let q_total = thermal.total_heat_flux_w_m2;
        if q_total <= 0.0 {
            continue;
        }

        // Integrated heat load
        ablation.cumulative_heat_load_j_m2 += q_total * dt;

        // Recession rate: dr/dt = q_dot / (rho_tps * H_abl)
        let recession_rate = q_total / (config.tps_density_kg_m3 * config.heat_of_ablation_j_kg);
        ablation.recession_depth_m += recession_rate * dt;

        // Nose radius growth from recession
        ablation.nose_radius_m = config.nose_radius_initial_m + ablation.recession_depth_m;

        // Mass loss from TPS
        let tps_area = std::f64::consts::PI * ablation.nose_radius_m.powi(2); // Approximate
        let mass_loss_rate = recession_rate * config.tps_density_kg_m3 * tps_area;
        let mass_loss = mass_loss_rate * dt;
        ablation.mass_loss_kg += mass_loss;
        ablation.tps_thickness_remaining_m =
            (config.tps_initial_thickness_m - ablation.recession_depth_m).max(0.0);

        // Update vehicle mass
        let new_mass = rocket.dynamics.mass_kg - mass_loss;
        rocket.dynamics.mass_kg = new_mass;
        mass.0 = new_mass;
    }
}

/// Plasma blackout detection from electron density (single authority: the
/// domain fit in `entry_physics`). Tracks blackout state per rocket and emits
/// a [`CommsBlackoutEvent`] on every start/stop edge. The condition is purely
/// physical (density × velocity); it is intentionally not gated on mission
/// phase, so an unexpected high-plasma ascent would also be reported.
pub fn compute_plasma_blackout(
    config: Res<EntryPhysicsConfig>,
    mut blackout_writer: MessageWriter<CommsBlackoutEvent>,
    mut rocket_query: Query<(
        Entity,
        &RocketPhysicsState,
        &AtmosphereState,
        &mut CommsState,
    )>,
) {
    for (rocket_entity, rocket, atmosphere, mut comms) in rocket_query.iter_mut() {
        let electron_density = electron_density_m3(
            atmosphere.density_kg_m3,
            rocket.dynamics.velocity_mps.length(),
        );
        let blackout_active =
            comms_blackout_active(electron_density, config.critical_electron_density_m3);

        // Edge detection against the previous tick's state.
        if blackout_active != comms.in_blackout {
            comms.in_blackout = blackout_active;
            blackout_writer.write(CommsBlackoutEvent {
                rocket: rocket_entity,
                blackout_active,
            });
            bevy::log::info!(
                "Comms blackout {} for rocket {rocket_entity}",
                if blackout_active { "started" } else { "ended" }
            );
        }
    }
}

/// Parachute deployment and drag (mortar → reefed → full). The transition
/// state machine lives in `domain::services::entry_physics` (pure, tested);
/// this system only adapts it: feed flight condition, apply the resulting
/// canopy drag to the translational accumulator. Deployment requires a
/// descending airstream, so an ascent cannot trigger the chutes.
pub fn compute_parachute_forces(
    sim_time: Res<SimulationTime>,
    config: Res<EntryPhysicsConfig>,
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &AtmosphereState,
        &mut ParachuteState,
        &mut ForceAccumulator,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    let parachute_config = config.parachute_config();
    for (rocket, atmosphere, mut parachute, mut force_accum) in rocket_query.iter_mut() {
        let rho = atmosphere.density_kg_m3;
        let velocity = rocket.dynamics.velocity_mps;
        let speed = velocity.length();
        if rho <= 0.0 || speed <= 0.0 {
            continue;
        }

        let up_dir = rocket.dynamics.position_m.normalize_or_zero();
        if up_dir.length_squared() < 1e-12 {
            continue;
        }
        let vertical_speed = velocity.dot(up_dir);
        let mach = speed / atmosphere.speed_of_sound_mps.max(1.0);

        let transitions = parachute.deployment.advance(
            &parachute_config,
            atmosphere.altitude_m,
            mach,
            vertical_speed,
            dt,
        );
        if transitions.any() {
            bevy::log::info!(
                "Parachute transition at {:.0} m: drogue_deployed={} drogue_inflated={} main_deployed={} main_inflated={}",
                atmosphere.altitude_m,
                transitions.drogue_deployed,
                transitions.drogue_inflated,
                transitions.main_deployed,
                transitions.main_inflated,
            );
        }

        // Apply combined canopy drag opposite the velocity.
        let drag_magnitude = parachute.deployment.drag_force_n(rho, speed);
        if drag_magnitude > 0.0 {
            force_accum.0 += (-velocity / speed) * drag_magnitude;
        }
    }
}

/// Supersonic retro-propulsion: plume-freestream interaction. Computes the
/// DLR base-pressure effectiveness multiplier (pure domain correlation) and
/// stores it in [`RetroPropulsionEffect`]. `propulsion_thrust` consumes the
/// multiplier, so thrust is still written by exactly one system — no double
/// counting — and the direction/ISP handling of `stage_thrust_body` applies.
pub fn compute_retro_propulsion(
    config: Res<EntryPhysicsConfig>,
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &AtmosphereState,
        &RocketPropulsion,
        &mut RetroPropulsionEffect,
    )>,
) {
    for (rocket, atmosphere, propulsion, mut retro) in rocket_query.iter_mut() {
        // Default each tick; re-derived below so state never goes stale
        // (config toggles, Mach drops below threshold, engines shut down).
        let mut multiplier = 1.0;

        if config.retro_propulsion_enabled {
            let mach =
                rocket.dynamics.velocity_mps.length() / atmosphere.speed_of_sound_mps.max(1.0);
            if mach >= config.retro_propulsion_mach_threshold {
                // Engines must actually be producing thrust at this tick;
                // the same stage_thrust_body the physics uses decides that.
                if let Some(stage) = propulsion.vehicle.stages.get(propulsion.active_stage) {
                    let remaining = propulsion
                        .propellant_remaining_kg
                        .get(propulsion.active_stage)
                        .copied()
                        .unwrap_or(0.0);
                    let throttle = propulsion.throttle.clamp(0.0, 1.0);
                    if throttle > 0.0 && remaining > 0.0 {
                        let (thrust_body, _) =
                            stage_thrust_body(&stage.engines, throttle, atmosphere.density_kg_m3);
                        if thrust_body.length_squared() > 0.0 {
                            multiplier = retro_propulsion_effectiveness(
                                mach,
                                config.retro_propulsion_mach_threshold,
                                config.base_pressure_coefficient,
                            );
                        }
                    }
                }
            }
        }

        retro.thrust_multiplier = multiplier;
    }
}
