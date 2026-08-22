use crate::components::rocket::*;
use crate::domain::services::actuation::{clamp_deflection, clamp_rcs_torque, limit_throttle_slew};
use crate::domain::services::aerodynamics::{
    aerodynamic_coefficients, aerodynamic_torque_body, angle_of_attack, angle_of_sideslip,
    center_of_pressure_m, drag_force_body, dynamic_pressure_q, lift_force_body, side_force_body,
    update_max_q,
};
use crate::domain::services::control::control_torque_body;
use crate::domain::services::gravity::{circular_orbit_speed_mps, gravitational_acceleration};
use crate::domain::services::guidance::{
    advance_ascent_phase, advance_descent_phase, deorbit_burn_targeting, pitch_axis_from_reference,
    powered_descent_guidance, reentry_bank_angle, target_attitude_for_phase, DescentGuidanceConfig,
};
use crate::domain::services::rocket_propulsion::{
    active_vehicle_inertia, active_vehicle_mass, allocate_gimbal_deflections, clamp_gimbal,
    consume_propellant, gimbal_torque_body, shed_stage, stage_thrust_body,
};
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
pub fn integrate_6dof(
    time: Res<Time>,
    mut rocket_query: Query<(
        &mut RocketPhysicsState,
        &mut RocketMass,
        &mut ForceAccumulator,
        &mut TorqueAccumulator,
    )>,
) {
    let dt = time.delta_secs() as f64;

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
/// transform.
pub fn aerodynamic_forces(
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketGeometry,
        &AtmosphereState,
        &mut AerodynamicForces,
        &mut MaxQTracker,
        &mut ForceAccumulator,
    )>,
) {
    for (rocket, geometry, atmosphere, mut aero, mut max_q, mut force_accum) in
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
        let (cd, cl, cy) = aerodynamic_coefficients(
            angle_of_attack(body_velocity),
            angle_of_sideslip(body_velocity),
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
/// planet-inertial frame. Never writes the transform.
pub fn propulsion_thrust(
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketGeometry,
        &AtmosphereState,
        &RocketPropulsion,
        &mut ForceAccumulator,
    )>,
) {
    for (rocket, geometry, atmosphere, propulsion, mut force_accum) in rocket_query.iter_mut() {
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
        force_accum.0 += thrust_world;
    }
}

/// Deplete the active stage's propellant at the engine mass flow and update the
/// vehicle mass, inertia tensor, and center of mass. Mass always derives from
/// the vehicle state (single source).
pub fn propulsion_consumption(
    time: Res<Time>,
    mut rocket_query: Query<(
        &mut RocketPhysicsState,
        &RocketGeometry,
        &AtmosphereState,
        &mut RocketPropulsion,
        &mut RocketMass,
    )>,
) {
    let dt = time.delta_secs() as f64;
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
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketGeometry,
        &RocketMass,
        &mut RocketMissionState,
        &RocketAutopilot,
        &RocketPropulsion,
        &mut RocketCommands,
    )>,
) {
    for (binding, rocket, geometry, mass, mut mission_state, autopilot, propulsion, mut commands) in
        rocket_query.iter_mut()
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

        // Auto-launch: begin the ascent flight on the first guidance pass.
        if *mission_state == RocketMissionState::PreLaunch {
            *mission_state = RocketMissionState::Launch;
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

        // Dynamic pressure from atmosphere state (cached earlier in the frame).
        // For now, approximate from density and speed if not available.
        // In a real implementation, we'd read from AtmosphereState component.
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

        // Compute target attitude based on phase.
        commands.target_attitude = target_attitude_for_phase(
            (*mission_state).into(),
            &autopilot.ascent_profile,
            up_dir,
            pitch_axis,
            altitude_m,
            velocity,
        );

        // For powered descent, compute thrust vector and attitude.
        if *mission_state == RocketMissionState::PoweredDescent {
            let target_pos = position_m * (altitude_m / radius); // Simplified: target is below current position
            let max_thrust = propulsion.vehicle.stages[propulsion.active_stage]
                .engines
                .iter()
                .map(|e| e.max_thrust_kn as f64 * 1000.0)
                .sum::<f64>();
            let (thrust_vec, thrust_att) = powered_descent_guidance(
                position_m,
                velocity,
                target_pos,
                mass.0,
                max_thrust,
                15.0_f64.to_radians(),
                1.0 / 60.0, // Approximate dt
                &descent_config,
            );
            commands.target_attitude = thrust_att;
        }

        // For reentry corridor, compute bank angle command.
        if *mission_state == RocketMissionState::ReentryCorridor {
            // Approximate g-load and heat flux for bank angle calculation.
            let g_load = 1.0; // TODO: compute from acceleration
            let heat_flux = 0.0; // TODO: compute from entry physics
            let crossrange = 0.0; // TODO: compute crossrange to target
            let bank_angle = reentry_bank_angle(
                altitude_m,
                speed,
                dynamic_pressure_pa,
                heat_flux,
                g_load,
                &descent_config,
                crossrange,
            );
            // Store bank angle in RCS torque command for control system to use.
            commands.rcs_torque_cmd_body = DVec3::new(0.0, 0.0, bank_angle);
        }
    }
}

/// Attitude control: converts the guidance target and current state into
/// commanded throttle, gimbal deflections, and RCS torque using the PID with
/// anti-windup. Writes only the command interface; never the vehicle's motion.
pub fn control_system(
    time: Res<Time>,
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
    let dt = time.delta_secs() as f64;
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
pub fn actuation_system(
    time: Res<Time>,
    mut rocket_query: Query<(
        &RocketCommands,
        &mut RocketPropulsion,
        &RocketPhysicsState,
        &RocketGeometry,
        &mut TorqueAccumulator,
        &RocketAutopilot,
    )>,
) {
    let dt = time.delta_secs() as f32;
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
/// `TerrainSource`. Detects landing and crash states. Never writes the
/// transform; the 6-DOF dynamics remain authoritative.
pub fn update_rocket_terrain_interaction(
    planet_query: Query<(&PlanetComponent, &PlanetTerrain)>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketGeometry,
        &mut TerrainCollisionState,
        &mut RocketMissionState,
    )>,
) {
    const CONTACT_ALTITUDE_M: f64 = 3.0;
    const TOUCH_DOWN_SPEED_MPS: f64 = 5.0;
    const CRASH_SPEED_MPS: f64 = 15.0;

    for (binding, rocket, geometry, mut collision, mut mission_state) in rocket_query.iter_mut() {
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
                ) {
                    *mission_state = RocketMissionState::Landed;
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
pub fn compute_ablation(
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
        ablation.cumulative_heat_load_j_m2 += q_total * (1.0 / 60.0); // Approximate dt

        // Recession rate: dr/dt = q_dot / (rho_tps * H_abl)
        let recession_rate = q_total / (config.tps_density_kg_m3 * config.heat_of_ablation_j_kg);
        let dt = 1.0 / 60.0;
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

/// Plasma blackout detection from electron density.
/// Emits CommsBlackoutEvent when blackout starts/ends.
pub fn compute_plasma_blackout(
    config: Res<EntryPhysicsConfig>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &AtmosphereState,
        &ThermalState,
        &RocketMissionState,
    )>,
) {
    for (binding, rocket, atmosphere, thermal, mission_state) in rocket_query.iter_mut() {
        let Some(_planet) = planet_query
            .iter()
            .find(|planet| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        let rho = atmosphere.density_kg_m3;
        let v = rocket.dynamics.velocity_mps.length();

        // Electron density model: n_e = C * rho^a * v^b (empirical fit)
        // Simplified: n_e proportional to rho * v^3
        let electron_density = 1e-4 * rho * v.powi(3);

        let was_blackout = *mission_state == RocketMissionState::ReentryCorridor
            && electron_density > config.critical_electron_density_m3;

        if was_blackout {
            // TODO: emit CommsBlackoutEvent
        }
    }
}

/// Parachute deployment and drag (mortar → reefed → full).
/// Applies drag forces to the translational accumulator.
pub fn compute_parachute_forces(
    config: Res<EntryPhysicsConfig>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketGeometry,
        &AtmosphereState,
        &mut ParachuteState,
        &mut ForceAccumulator,
        &mut RocketMissionState,
    )>,
) {
    for (
        binding,
        rocket,
        geometry,
        atmosphere,
        mut parachute,
        mut force_accum,
        mut mission_state,
    ) in rocket_query.iter_mut()
    {
        let Some(_planet) = planet_query
            .iter()
            .find(|planet| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        let rho = atmosphere.density_kg_m3;
        let v = rocket.dynamics.velocity_mps.length();
        let altitude_m = atmosphere.altitude_m;
        let mach = v / atmosphere.speed_of_sound_mps.max(1.0);

        if rho <= 0.0 || v <= 0.0 {
            continue;
        }

        // Drogue deployment logic
        if !parachute.drogue_deployed
            && mach <= config.drogue_deploy_mach
            && altitude_m <= config.drogue_deploy_altitude_m
        {
            parachute.drogue_deployed = true;
            parachute.drogue_timer_s = 0.0;
        }

        if parachute.drogue_deployed && !parachute.drogue_fully_inflated {
            parachute.drogue_timer_s += 1.0 / 60.0;
            if parachute.drogue_timer_s < config.drogue_reef_time_s {
                parachute.current_cd = config.drogue_reef_cd;
                parachute.reference_area_m2 = config.drogue_reference_area_m2;
            } else {
                parachute.drogue_fully_inflated = true;
                parachute.current_cd = config.drogue_full_cd;
            }
        }

        // Main parachute deployment logic
        if parachute.drogue_fully_inflated
            && !parachute.main_deployed
            && altitude_m <= config.main_deploy_altitude_m
        {
            parachute.main_deployed = true;
            parachute.main_timer_s = 0.0;
        }

        if parachute.main_deployed && !parachute.main_fully_inflated {
            parachute.main_timer_s += 1.0 / 60.0;
            if parachute.main_timer_s < config.main_reef_time_s {
                parachute.current_cd = config.main_reef_cd;
                parachute.reference_area_m2 = config.main_reference_area_m2;
            } else {
                parachute.main_fully_inflated = true;
                parachute.current_cd = config.main_full_cd;
            }
        }

        // Apply parachute drag if deployed
        if parachute.drogue_deployed || parachute.main_deployed {
            let drag = 0.5 * rho * v.powi(2) * parachute.current_cd * parachute.reference_area_m2;
            let drag_dir = -rocket.dynamics.velocity_mps.normalize_or_zero();
            force_accum.0 += drag_dir * drag;
        }
    }
}

/// Supersonic retro-propulsion: plume-freestream interaction.
/// Modifies effective thrust and base pressure at Mach > 1.
pub fn compute_retro_propulsion(
    config: Res<EntryPhysicsConfig>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketGeometry,
        &AtmosphereState,
        &RocketPropulsion,
        &mut ForceAccumulator,
    )>,
) {
    for (binding, rocket, geometry, atmosphere, propulsion, mut force_accum) in
        rocket_query.iter_mut()
    {
        let Some(_planet) = planet_query
            .iter()
            .find(|planet| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        if !config.retro_propulsion_enabled {
            return;
        }

        let mach = rocket.dynamics.velocity_mps.length() / atmosphere.speed_of_sound_mps.max(1.0);
        if mach < config.retro_propulsion_mach_threshold {
            return;
        }

        // Check if engines are active and thrusting
        let active_stage = propulsion.active_stage;
        if active_stage >= propulsion.vehicle.stages.len() {
            return;
        }
        let stage = &propulsion.vehicle.stages[active_stage];
        let thrust_n = stage
            .engines
            .iter()
            .filter(|e| e.state == crate::domain::entities::rocket::EngineState::Running)
            .map(|e| e.max_thrust_kn as f64 * 1000.0 * propulsion.throttle as f64)
            .sum::<f64>();

        if thrust_n <= 0.0 {
            return;
        }

        // DLR base pressure correlation for supersonic retro-propulsion
        // Simplified: base pressure reduction proportional to Mach and thrust
        let base_pressure_factor: f64 =
            1.0 - config.base_pressure_coefficient * (mach - 1.0).min(5.0);
        let effective_thrust = thrust_n * base_pressure_factor.max(0.1);

        // Apply effective thrust along body +Y axis
        let thrust_body = DVec3::Y * effective_thrust;
        let orientation = rocket.dynamics.orientation;
        let thrust_inertial = orientation * thrust_body;
        force_accum.0 += thrust_inertial;
    }
}
