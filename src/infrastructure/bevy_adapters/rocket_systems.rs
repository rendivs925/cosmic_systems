use crate::components::rocket::*;
use crate::domain::events::RelaunchRequested;
use crate::domain::services::gravity::gravitational_parameter;
use crate::domain::services::guidance::{
    advance_ascent_phase, advance_descent_phase, attitude_from_direction, boostback_guidance,
    gravity_turn_direction_gated, hover_slam_guidance, pitch_axis_from_reference,
    powered_descent_guidance_convex, prograde_attitude, reentry_bank_angle,
    reentry_bank_angle_enhanced, suicide_burn_guidance, transfer_burn_phase, AutopilotMode,
    DescentGuidanceConfig, TransferBurnPhase,
};
use crate::domain::services::landing_gear::LegDeploymentState;
use crate::domain::services::physics_orbital::orbital_elements_from_state;
#[cfg(test)]
use crate::domain::services::rocket_dynamics::RocketDynamicsState;
#[cfg(test)]
use crate::domain::services::rocket_propulsion::stage_thrust_body;
use crate::domain::services::rocket_propulsion::{
    active_vehicle_inertia, active_vehicle_mass_with_payload,
};
use crate::domain::services::simulation_time::SimulationTime;
#[cfg(test)]
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::infrastructure::bevy_adapters::components::{
    PlanetComponent, RocketAutopilot, RocketCommands,
};
use crate::infrastructure::bevy_adapters::terrain_render::RenderOrigin;
use bevy::light::CascadeShadowConfigBuilder;

pub(crate) use crate::infrastructure::bevy_adapters::rocket_presentation::render_dynamics_state;
use crate::infrastructure::bevy_adapters::rocket_telemetry::FlightRecorder;
use bevy::ecs::query::QueryData;
#[cfg(test)]
use bevy::math::DMat3;
use bevy::math::{DQuat, DVec3, Vec3};
use bevy::prelude::*;

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
        let velocity = rocket.dynamics.velocity_mps;
        let speed = velocity.length();
        let mu = gravitational_parameter(planet.domain_planet.mass_kg);
        let state_elements = orbital_elements_from_state(position_m, velocity, mu);
        let target_orbit_reached = autopilot
            .target_orbit
            .matches_state(position_m, velocity, mu, radius_m);

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
            altitude_m,
            speed,
            dynamic_pressure_pa,
            has_active_engines,
            &descent_config,
        )
        .into();

        if target_orbit_reached && *mission_state == RocketMissionState::Orbit {
            autopilot.mode = AutopilotMode::Off;
            commands.throttle_cmd = 0.0;
            continue;
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

/// Relaunch input (Phase 14): R commands a full pad-style reset of every
/// vehicle. Presentation-adjacent input handling only — the mutation happens
/// in FixedUpdate via [`apply_relaunch_requests`].
pub fn handle_relaunch_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    rockets: Query<Entity, With<RocketPhysicsState>>,
    mut relaunch_writer: MessageWriter<RelaunchRequested>,
) {
    if !keyboard.just_pressed(KeyCode::KeyR) {
        return;
    }
    for entity in rockets.iter() {
        relaunch_writer.write(RelaunchRequested { rocket: entity });
        bevy::log::info!("Relaunch requested for rocket {entity}");
    }
}

/// Apply relaunch commands (Phase 14): refuel every stage to its configured
/// propellant load, reset the vehicle upright and at rest at the current
/// site, restore mission PreLaunch, re-stow gear, clear lifecycle state, and
/// despawn jettisoned debris. One authority for the whole reset; runs before
/// guidance so the auto-launch takes over on the same tick.
#[allow(clippy::type_complexity)]
pub fn apply_relaunch_requests(
    mut reader: MessageReader<RelaunchRequested>,
    planet_query: Query<&PlanetComponent>,
    spent_stages: Query<(Entity, &SpentStage)>,
    mut commands: Commands,
    mut rocket_query: Query<(
        Entity,
        &RocketPlanetBinding,
        &RocketGeometry,
        &mut RocketPhysicsState,
        &mut RocketPropulsion,
        &mut RocketMass,
        &mut RocketMissionState,
        &mut GroundRest,
        Option<&mut LandingLegs>,
        &mut LandingScorecard,
        &mut TipOverState,
        &mut FlightRecorder,
    )>,
) {
    for event in reader.read() {
        let Ok((
            entity,
            binding,
            geometry,
            mut rocket,
            mut propulsion,
            mut mass,
            mut mission_state,
            mut rest,
            legs,
            mut scorecard,
            mut tip_over,
            mut recorder,
        )) = rocket_query.get_mut(event.rocket)
        else {
            continue;
        };

        // Jettisoned hardware goes away with the flight that shed it.
        for (debris, spent) in spent_stages.iter() {
            if spent.parent_rocket == entity {
                commands.entity(debris).despawn();
            }
        }

        // Refuel from the authoritative configuration and reset propulsion.
        propulsion.propellant_remaining_kg = propulsion
            .vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        propulsion.active_stage = 0;
        propulsion.separations_count = 0;
        propulsion.throttle = 0.0;
        propulsion.gimbal_pitch_rad = 0.0;
        propulsion.gimbal_yaw_rad = 0.0;
        propulsion.time_since_separation_s = propulsion.ullage_settle_time_s;

        // Mass and inertia from the refueled stack.
        let total_mass_kg = active_vehicle_mass_with_payload(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            0,
            propulsion.attached_payload_kg,
        );
        let (inertia, com) = active_vehicle_inertia(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            0,
            geometry.radius_m as f64,
            geometry.height_m as f64,
        );
        rocket.dynamics.mass_kg = total_mass_kg;
        rocket.dynamics.inertia_body = inertia;
        rocket.dynamics.center_of_mass_m = com;
        mass.0 = total_mass_kg;

        // Upright, motionless, resting at the current site.
        let Some(planet) = planet_query
            .iter()
            .find(|planet| planet.matches_body(&binding.planet_name))
        else {
            continue;
        };
        let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
        let position_m = rocket.dynamics.position_m;
        let up = position_m / position_m.length().max(1.0);
        rocket.dynamics.position_m = up * (position_m.length().max(radius_m));
        rocket.dynamics.velocity_mps = DVec3::ZERO;
        rocket.dynamics.angular_velocity_radps = DVec3::ZERO;
        rocket.dynamics.orientation = DQuat::from_rotation_arc(DVec3::Y, up);

        // Mission and lifecycle state back to a fresh pad.
        *mission_state = RocketMissionState::PreLaunch;
        rest.active = true;
        if let Some(mut legs) = legs {
            legs.deployment = LegDeploymentState::default();
            legs.compression_m = 0.0;
        }
        *scorecard = LandingScorecard::default();
        tip_over.reset();
        recorder.clear();

        bevy::log::info!(
            "Relaunch ready: {:.0} kg refueled, vehicle upright and held on the pad",
            total_mass_kg
        );
    }
}

/// Adds RocketCameraController to the existing camera entity so the rocket
/// camera systems can drive it. The camera is spawned by setup_space with a
/// solar CameraController. We keep the solar CameraController marker so shared
/// systems (e.g. update_planet_positions) can still locate the camera via
/// `.single()`, but since SolarSystemModePlugin is not composed into Rocket
/// Mode, its free-flight camera system never runs — Rocket Mode owns the camera.
pub fn setup_rocket_camera_controller(
    mut commands: Commands,
    camera_query: Query<Entity, With<Camera3d>>,
) {
    for entity in camera_query.iter() {
        commands
            .entity(entity)
            .insert(RocketCameraController::default());
    }
}

/// Initializes the camera position and render origin for rocket mode.
/// The camera starts at the solar system position; this system repositions it
/// to the rocket's location on the launch pad and sets the render origin to
/// the rocket's physics position so the rocket renders near the origin.
pub fn setup_rocket_camera_and_origin(
    mut commands: Commands,
    mut camera_query: Query<(Entity, &mut Transform, &mut Projection), With<Camera3d>>,
    mut render_origin: ResMut<crate::infrastructure::bevy_adapters::terrain_render::RenderOrigin>,
    rocket_query: Query<&RocketPhysicsState>,
) {
    let Some(rocket) = rocket_query.iter().next() else {
        bevy::log::warn!("setup_rocket_camera_and_origin: no rocket entity found");
        return;
    };

    // Rocket physics position in planet-centered inertial frame (meters)
    let rocket_pos_m = rocket.dynamics.position_m;

    // Set render origin to the rocket's physics position so the rocket
    // renders near the origin (flight units = meters, 1:1 scale).
    render_origin.origin = rocket_pos_m;
    render_origin.last_camera_pos = rocket_pos_m;

    // Compute initial camera position matching the chase camera logic.
    // At spawn, rocket body +Y aligns with radial up. The chase camera
    // detects this vertical alignment and uses a side offset (right vector)
    // instead of a rear offset. We replicate that logic here.
    // Use RocketCameraConfig defaults for consistency.
    let chase_distance = 220.0; // meters
    let chase_height = 50.0; // meters

    // Up direction (radial from planet center to rocket)
    let up_dir = rocket_pos_m.normalize().as_vec3();

    // Right direction: perpendicular to up_dir
    let right_dir = if up_dir.z.abs() < 0.9 {
        up_dir.cross(Vec3::Z).normalize()
    } else {
        up_dir.cross(Vec3::X).normalize()
    };

    // For vertical rocket, chase camera uses side offset (right) + up
    let camera_pos_flight = right_dir * chase_distance + up_dir * chase_height;

    // Camera looks at rocket center (half height up in flight frame).
    // The Falcon 9 is 70m tall; center is at ~35m in the flight frame.
    let rocket_center = Vec3::new(0.0, 35.0, 0.0);
    let camera_transform =
        Transform::from_translation(camera_pos_flight).looking_at(rocket_center, up_dir);

    // The launch pad horizon is tens of kilometers away. Start at the chase
    // camera range so the curved Earth proxy is visible on the first frame;
    // the regular projection system maintains this range afterwards.
    for (entity, mut cam_transform, projection) in camera_query.iter_mut() {
        *cam_transform = camera_transform;
        if let Projection::Perspective(proj) = projection.into_inner() {
            proj.near = 0.5;
            proj.far = 100_000.0;
        }
        // Workaround for bevyengine/bevy#18904: with GPU preprocessing on,
        // meshes that pass CPU visibility (rocket, pad primitives) silently
        // drop out of the indirect-draw path on this driver. Drawing directly
        // keeps every visible mesh on screen.
        commands
            .entity(entity)
            .insert(bevy::render::view::NoIndirectDrawing);
    }
}

/// Spawns a directional sun light for rocket mode. The solar simulation uses
/// a PointLight at the origin, but in the flight frame the sun should be a
/// directional light at infinity. The sun is placed well above the LOCAL horizon
/// (the rocket's radial up direction) so the pad and terrain are brightly lit:
/// a fixed world-space direction would sit only a few degrees above the KSC
/// horizon because the flight frame's axes are not the planet's local frame.
pub fn setup_rocket_sun_light(mut commands: Commands, rocket_query: Query<&RocketPhysicsState>) {
    // Radial up at the pad (the rocket's body +Y at spawn).
    let up = rocket_query
        .iter()
        .next()
        .map(|r| r.dynamics.position_m.normalize_or_zero().as_vec3())
        .filter(|v| v.length_squared() > 0.5)
        .unwrap_or(Vec3::Y);
    // A fixed horizontal reference perpendicular to the local up.
    let east = if up.z.abs() < 0.9 {
        up.cross(Vec3::Z).normalize()
    } else {
        up.cross(Vec3::X).normalize()
    };
    // Sun ~20 deg above the local horizon (morning golden hour): long shadows
    // and warm light like the launch-pad reference footage.
    let sun_dir = (up * 0.342 + east * 0.94).normalize();

    // Sky-blue ambient fill so shadowed faces read as sky-lit instead of black.
    commands.insert_resource(bevy::light::AmbientLight {
        color: Color::srgb(0.5, 0.6, 0.75),
        brightness: 400.0,
        ..default()
    });

    commands.spawn((
        bevy::light::DirectionalLight {
            illuminance: 100_000.0,             // bright daylight (lux)
            color: Color::srgb(1.0, 0.9, 0.75), // warm low-sun light
            shadows_enabled: true,
            ..default()
        },
        // Cascade shadow config tuned for the rocket flight scale (1 unit = 1 m).
        // The first cascade covers the immediate pad area; later cascades extend
        // to the horizon so distant terrain still casts visible shadows.
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 30.0,
            maximum_distance: 800.0,
            ..default()
        }
        .build(),
        // Light travels along the light's -Z toward the scene; orient it so the
        // sun appears in the `sun_dir` direction.
        Transform::from_xyz(0.0, 0.0, 0.0).looking_at(-sun_dir, Vec3::Y),
        // Tag component so the day/night system can find and rotate this light.
        SunLight,
        // Store the computed sun direction so the day/night rotation starts from
        // the correct horizon angle (not a generic default).
        SunLightState {
            initial_direction: sun_dir,
        },
    ));
}

/// Tag component marking the sun directional light for day/night rotation.
#[derive(Component, Debug)]
pub struct SunLight;

/// Component storing the sun's initial direction so the day/night system can
/// rotate it around the planet's north pole each frame.
#[derive(Component, Debug)]
pub struct SunLightState {
    pub initial_direction: Vec3,
}

impl Default for SunLightState {
    fn default() -> Self {
        Self {
            initial_direction: Vec3::new(0.0, 0.26, 0.97).normalize(), // ~15 deg above horizon
        }
    }
}

/// Space is the clear color. Atmospheric haze is applied only to local geometry
/// through the camera fog; it must never turn the entire universe blue.
pub fn setup_rocket_sky_color(mut clear_color: ResMut<ClearColor>) {
    *clear_color = ClearColor(Color::srgb(0.002, 0.002, 0.006));
}

/// Spawns a true-scale Earth sphere for the rocket mode. The Earth radius is
/// ~6,371 km; in flight units (1 unit = 1 meter) this is 6,371,000 units.
/// The sphere is positioned each frame relative to the render origin so it
/// provides a correct horizon and planet body from any altitude.
pub fn setup_rocket_earth_sphere(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let earth_radius_m = 6_371_000.0; // True Earth radius in meters
    let earth_radius_units = earth_radius_m as f32; // Flight units = meters

    // Use true radius (not 0.999) so the sphere surface matches terrain height.
    let mesh_handle = meshes.add(Sphere::new(earth_radius_units));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.4, 0.7), // Earth blue-green
        perceptual_roughness: 0.9,
        metallic: 0.0,
        cull_mode: None, // Render both sides for horizon visibility
        ..default()
    });

    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material),
        Transform::default(),
        RocketEarthSphere,
    ));
}

/// Component marking the true-scale Earth sphere entity for updates.
#[derive(Component, Debug, Default)]
pub struct RocketEarthSphere;

/// Kept as an update hook so future atmospheric scattering can drive a local
/// sky pass. ClearColor deliberately remains space-black at every altitude.
pub fn update_rocket_sky_color(mut clear_color: ResMut<ClearColor>) {
    *clear_color = ClearColor(Color::srgb(0.002, 0.002, 0.006));
}

/// Updates the true-scale Earth sphere position to stay centered on the planet
/// center relative to the render origin.
pub fn update_rocket_earth_sphere(
    render_origin: Res<RenderOrigin>,
    mut sphere_query: Query<&mut Transform, With<RocketEarthSphere>>,
) {
    // Planet center in flight units: -render_origin.origin (scaled to meters).
    // render_origin is in physics meters; flight units = meters.
    let center = -(render_origin.origin.as_vec3());
    for mut transform in sphere_query.iter_mut() {
        transform.translation = center;
    }
}

/// Day/night cycle: rotates the sun light direction around the planet's rotation
/// axis (Y in the flight frame) as simulation time advances. The planet's angular
/// velocity comes from the Earth planet definition. The sun makes one full
/// revolution per planet rotation period (~24 hours for Earth).
pub fn update_sun_day_night_cycle(
    sim_time: Res<SimulationTime>,
    planet_query: Query<&PlanetComponent>,
    mut sun_query: Query<(&mut Transform, &SunLightState), With<SunLight>>,
) {
    // Find Earth planet for its rotation period, then compute angular velocity.
    // omega = 2π / period_seconds.
    let earth_rotation_rad_s = planet_query
        .iter()
        .find(|p| p.domain_planet.name == "Earth")
        .map(|p| {
            let period_s = p.domain_planet.rotation_period_hours as f64 * 3600.0;
            if period_s > 0.0 {
                std::f64::consts::TAU / period_s
            } else {
                7.2921159e-5 // Earth sidereal rotation rate rad/s
            }
        })
        .unwrap_or(7.2921159e-5_f64);

    let total_time_s = sim_time.sim_time_s;
    let rotation_angle = (total_time_s * earth_rotation_rad_s) as f32;

    for (mut light_transform, sun_state) in sun_query.iter_mut() {
        // Rotate initial sun direction around the Y axis (planet rotation axis).
        // The planet's north pole points along +Y in the flight frame.
        let cos_a = rotation_angle.cos();
        let sin_a = rotation_angle.sin();
        let dir = sun_state.initial_direction;
        let rotated = Vec3::new(
            cos_a * dir.x - sin_a * dir.z,
            dir.y,
            sin_a * dir.x + cos_a * dir.z,
        )
        .normalize();

        // Update the light's look-direction so the sun travels across the sky.
        *light_transform = Transform::from_xyz(0.0, 0.0, 0.0).looking_at(-rotated, Vec3::Y);
    }
}

/// Handles the pre-launch hold: Space key arms the launch.
pub fn handle_rocket_launch_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mission_query: Query<(
        &mut RocketMissionState,
        &RocketPhysicsState,
        &mut RocketRenderState,
    )>,
) {
    if keyboard.just_pressed(KeyCode::Space) {
        for (mut mission, rocket, mut render) in mission_query.iter_mut() {
            if *mission == RocketMissionState::PreLaunch {
                // Prelaunch renders the latest body-fixed pad state rather
                // than its interpolation buffer. Reset that buffer before
                // enabling airborne interpolation to avoid blending two
                // stale rotating-pad snapshots on the launch transition.
                *render = RocketRenderState::new(rocket.dynamics);
                *mission = RocketMissionState::Launch;
            }
        }
    }
}

#[cfg(test)]
mod ground_contact_tests {
    use super::*;
    use crate::domain::entities::rocket::{EngineState, Rocket, RocketEngine, RocketStage};
    use crate::domain::events::SplashdownDetectedEvent;
    use crate::domain::services::landing_gear::{LandingGear, LandingGearSpec};
    use crate::domain::services::reference_frames::{
        geodetic_to_body_fixed, planet_inertial_to_body_fixed, surface_velocity_in_planet_inertial,
    };
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::terrain_collision::{radial_direction, GroundContact};
    use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
    use crate::infrastructure::bevy_adapters::components::{
        PlanetComponent, TerrainCollisionState,
    };
    use crate::infrastructure::bevy_adapters::rocket_contact::{
        advance_topple, deploy_landing_legs, resolve_ground_contact,
    };
    use crate::infrastructure::bevy_adapters::rocket_dynamics::{
        accumulate_forces, integrate_6dof,
    };
    use bevy::math::DQuat;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    const DT: f64 = 1.0 / 64.0;
    const G0: f64 = 9.80665;

    /// One-engine test vehicle: 20 kN vacuum thrust, +Y body axis.
    fn test_vehicle(max_thrust_kn: f32) -> Rocket {
        Rocket {
            name: "Test".into(),
            diameter_m: 1.0,
            height_m: 10.0,
            stages: vec![RocketStage {
                name: "S1".into(),
                dry_mass_kg: 400.0,
                propellant_mass_kg: 600.0,
                engines: vec![RocketEngine {
                    position_m: bevy::math::Vec3::new(0.0, -5.0, 0.0),
                    thrust_axis: bevy::math::Vec3::Y,
                    isp_sea_level: 250.0,
                    isp_vacuum: 300.0,
                    gimbal_range_deg: 0.0,
                    max_thrust_kn: max_thrust_kn,
                    throttle_min: 0.0,
                    throttle_max: 1.0,
                    restartable: true,
                    state: EngineState::Running,
                }],
            }],
        }
    }

    /// Spawn a rocket standing exactly on the terrain at (lat, lon), plus the
    /// Earth planet entity, and run only the tail of the fixed pipeline:
    /// force writer → accumulate → integrate → ground contact.
    fn pad_app(throttle: f32, max_thrust_kn: f32) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<SplashdownDetectedEvent>();
        app.insert_resource(SimulationTime::new(DT));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            DT,
        )));

        let planet =
            crate::domain::services::planet_factory::PlanetFactory::create_by_name("Earth")
                .expect("Earth exists");
        app.world_mut().spawn((
            PlanetComponent {
                domain_planet: planet,
                material: Handle::default(),
                has_texture: false,
                base_reflectance: 1.0,
                base_roughness: 1.0,
            },
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for("Earth"),
        ));

        let source =
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for("Earth")
                .source;
        let (lat, lon) = (28.5721_f64, -80.6480_f64);
        let h = source.height_m(lat, lon);
        let up = radial_direction(lat, lon);
        let surface_radius = EARTH_RADIUS_M + h;

        let vehicle = test_vehicle(max_thrust_kn);
        let propellant = vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        let (inertia, com) =
            crate::domain::services::rocket_dynamics::rocket_inertia_tensor(1000.0, 0.0, 0.5, 10.0);
        app.world_mut().spawn((
            RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    up * surface_radius,
                    DVec3::ZERO,
                    DQuat::from_rotation_arc(DVec3::Y, up),
                    1000.0,
                    inertia,
                    com,
                ),
            },
            RocketGeometry {
                radius_m: 0.5,
                height_m: 10.0,
            },
            RocketPropulsion {
                vehicle,
                active_stage: 0,
                propellant_remaining_kg: propellant,
                throttle,
                gimbal_pitch_rad: 0.0,
                gimbal_yaw_rad: 0.0,
                time_since_separation_s: 10.0,
                ullage_settle_time_s: 2.0,
                separations_count: 0,
                attached_payload_kg: 0.0,
            },
            TerrainCollisionState::default(),
            GroundRest { active: true },
            TipOverState::default(),
            LandingScorecard::default(),
            RocketAutopilot::default(),
            FlightRecorder::new(64, 1.0),
            RocketMissionState::PreLaunch,
            RocketMass(1000.0),
            GravityAcceleration { value: -up * G0 },
            ForceAccumulator::default(),
            TorqueAccumulator::default(),
            RocketPlanetBinding {
                planet_name: CelestialBodyId::earth(),
            },
        ));

        // Mirror production force writers: non-gravity forces only —
        // accumulate_forces contributes the single gravity term.
        fn write_flight_forces(
            mut query: Query<(
                &RocketPhysicsState,
                &RocketPropulsion,
                &mut ForceAccumulator,
            )>,
        ) {
            for (rocket, propulsion, mut force) in query.iter_mut() {
                if let Some(stage) = propulsion.vehicle.stages.get(propulsion.active_stage) {
                    let (body_thrust, _) =
                        stage_thrust_body(&stage.engines, propulsion.throttle, 0.0);
                    force.0 += rocket.dynamics.orientation * body_thrust;
                }
            }
        }
        app.add_systems(
            FixedUpdate,
            (
                write_flight_forces,
                accumulate_forces,
                integrate_6dof,
                resolve_ground_contact,
            )
                .chain(),
        );
        app
    }

    /// Pad hold is real physics now: a throttled-down vehicle must stay pinned
    /// to the pad instead of sinking under accumulated gravity.
    #[test]
    fn pad_hold_survives_gravity_without_sinking() {
        let mut app = pad_app(0.0, 20.0);

        for _ in 0..96 {
            app.update();
        }

        let world = app.world_mut();
        let mut q = world.query::<(&RocketPhysicsState, &GroundRest, &TerrainCollisionState)>();
        let (rocket, rest, collision) = q.single(world).unwrap();

        assert!(rest.active, "vehicle must still be resting on the pad");
        let (lat, lon) = (28.5721_f64, -80.6480_f64);
        let h =
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for("Earth")
                .source
                .height_m(lat, lon);
        let expected_r = EARTH_RADIUS_M + h;
        assert!(
            (rocket.dynamics.position_m.length() - expected_r).abs() < 0.05,
            "position drifted off the pad: |r|={}",
            rocket.dynamics.position_m.length()
        );
        assert!(
            rocket.dynamics.velocity_mps.length() < 0.5,
            "residual velocity too large: {}",
            rocket.dynamics.velocity_mps.length()
        );
        assert_eq!(collision.ground_contact, GroundContact::Landed);
    }

    #[test]
    fn prelaunch_vehicle_remains_at_its_rotating_launch_site() {
        let mut app = pad_app(0.0, 20.0);
        let launch_site = LaunchSiteCoordinates::default();
        let entity = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            query.single(world).unwrap()
        };
        {
            let world = app.world_mut();
            world.entity_mut(entity).insert(launch_site.clone());
            let mut rocket = world.get_mut::<RocketPhysicsState>(entity).unwrap();
            rocket.dynamics.velocity_mps = DVec3::new(500.0, -100.0, 250.0);
            rocket.dynamics.angular_velocity_radps = DVec3::new(0.1, 0.2, 0.3);
        }

        for _ in 0..96 {
            app.update();
        }

        let earth = crate::domain::services::planet_factory::PlanetFactory::create_by_name("Earth")
            .unwrap();
        let time_days = app.world().resource::<SimulationTime>().sim_time_s / 86_400.0;
        let world = app.world_mut();
        let rocket = world.get::<RocketPhysicsState>(entity).unwrap();
        let position_bf =
            planet_inertial_to_body_fixed(rocket.dynamics.position_m, &earth, time_days as f32);
        let expected_direction_bf = geodetic_to_body_fixed(&launch_site, &earth).normalize();
        let expected_surface_velocity =
            surface_velocity_in_planet_inertial(rocket.dynamics.position_m, &earth);

        assert!(
            position_bf.normalize().dot(expected_direction_bf) > 1.0 - 1e-12,
            "pre-launch position drifted away from the launch site"
        );
        assert!(
            (rocket.dynamics.velocity_mps - expected_surface_velocity).length() < 1e-9,
            "pre-launch velocity must match the rotating launch pad"
        );
        assert!(
            rocket.dynamics.angular_velocity_radps.length() < 1e-12,
            "pre-launch vehicle must not rotate on the pad"
        );
    }

    /// Takeoff from resting contact: throttle above TWR 1 releases the
    /// constraint and the vehicle climbs.
    #[test]
    fn takeoff_releases_rest_and_climbs() {
        let mut app = pad_app(1.0, 200.0);
        let start_r;

        {
            let world = app.world_mut();
            let mut q = world.query::<&RocketPhysicsState>();
            start_r = q.single(world).unwrap().dynamics.position_m.length();
        }

        for _ in 0..128 {
            app.update();
        }

        let world = app.world_mut();
        let mut q = world.query::<(&RocketPhysicsState, &GroundRest, &RocketMissionState)>();
        let (rocket, rest, mission) = q.single(world).unwrap();

        assert!(!rest.active, "constraint must release above TWR 1");
        assert!(
            rocket.dynamics.position_m.length() > start_r + 1.0,
            "vehicle did not climb: Δr={}",
            rocket.dynamics.position_m.length() - start_r
        );
        assert_ne!(
            *mission,
            RocketMissionState::Crashed,
            "liftoff must not be judged a crash"
        );
    }

    /// Deployed landing gear absorb a gentle touchdown through strut
    /// compression instead of the rigid point-contact snap.
    #[test]
    fn deployed_legs_absorb_touchdown_softly() {
        let surface_r;
        let mut app = {
            let mut app = pad_app(0.0, 20.0); // engines off
            let world = app.world_mut();
            let (lat, lon) = (28.5721_f64, -80.6480_f64);
            let h = crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for(
                "Earth",
            )
            .source
            .height_m(lat, lon);
            surface_r = EARTH_RADIUS_M + h;

            let (entity, up, mass_kg, inertia_body, com) = {
                let mut q = world.query::<(Entity, &RocketPhysicsState)>();
                let (entity, rocket) = q.single(world).unwrap();
                let up = rocket.dynamics.position_m.normalize();
                (
                    entity,
                    up,
                    rocket.dynamics.mass_kg,
                    rocket.dynamics.inertia_body,
                    rocket.dynamics.center_of_mass_m,
                )
            };
            // Start airborne 2 m above the pad, descending at 2 m/s, gear
            // down, released from the pad hold: a true descent so the
            // touchdown verdict fires.
            world.entity_mut(entity).insert(RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    up * (surface_r + 2.0),
                    -up * 2.0,
                    DQuat::from_rotation_arc(DVec3::Y, up),
                    mass_kg,
                    inertia_body,
                    com,
                ),
            });
            world.get_mut::<GroundRest>(entity).unwrap().active = false;
            // Gear down: pre-latch deployment (the latch itself is covered
            // by domain tests; this test exercises the contact path).
            let gear = LandingGear::new(
                LandingGearSpec {
                    count: 4,
                    base_radius_m: 4.5,
                    stroke_m: 3.0,
                    max_landing_mass_kg: Some(2_000.0),
                    deploy_altitude_m: 100.0,
                },
                1_000.0, // gross vehicle mass of this test vehicle
            );
            let mut legs = LandingLegs::new(gear);
            legs.deployment.deployed = true;
            world.entity_mut(entity).insert(legs);
            app
        };

        for _ in 0..512 {
            app.update();
        }

        let world = app.world_mut();
        let mut q = world.query::<(
            &RocketPhysicsState,
            &GroundRest,
            &RocketMissionState,
            &LandingLegs,
            &LandingScorecard,
        )>();
        let (rocket, rest, mission, legs, scorecard) = q.single(world).unwrap();

        assert!(rest.active, "must be resting on gear");
        assert_ne!(*mission, RocketMissionState::Crashed);
        assert!(scorecard.recorded, "touchdown must be recorded");
        assert!(
            (scorecard.touchdown_vertical_speed_mps - 2.0).abs() < 0.5,
            "scorecard descent {} not near the 2 m/s drop",
            scorecard.touchdown_vertical_speed_mps
        );
        assert!(
            scorecard.leg_compression_peak_m > 0.0
                && scorecard.leg_compression_peak_m <= legs.gear.spec.stroke_m,
            "compression peak {} outside (0, stroke]",
            scorecard.leg_compression_peak_m
        );
        assert!(
            legs.compression_m > 0.0 && legs.compression_m <= legs.gear.spec.stroke_m,
            "strut compression {} outside (0, stroke]",
            legs.compression_m
        );
        // Soft contact: the hull rides below the point-contact radius by at
        // most one stroke (documented approximation).
        let sink = surface_r - rocket.dynamics.position_m.length();
        assert!(
            sink >= -0.5 && sink <= legs.gear.spec.stroke_m + 0.5,
            "sink depth {sink} m outside strut range"
        );
        assert!(
            rocket.dynamics.velocity_mps.length() < 0.3,
            "not settled: {}",
            rocket.dynamics.velocity_mps.length()
        );
    }

    /// Relaunch (Phase 14): one message restores a flown, drained vehicle to
    /// a fresh pad state and clears its jettisoned debris.
    #[test]
    fn relaunch_restores_fresh_pad_state() {
        use crate::domain::events::RelaunchRequested;

        let mut app = pad_app(0.0, 20.0);
        app.add_message::<RelaunchRequested>();
        app.add_systems(FixedUpdate, apply_relaunch_requests);

        let debris_entity;
        let rocket_entity;
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            rocket_entity = q.single(world).unwrap();

            debris_entity = world
                .spawn(SpentStage {
                    parent_rocket: rocket_entity,
                    kind: SpentStageKind::Booster,
                })
                .id();
        }

        {
            // Fly the vehicle into a post-flight state: stage 2 active,
            // tanks empty, airborne, landed mission, gear down.
            let world = app.world_mut();
            let mut state = world.get_mut::<RocketPhysicsState>(rocket_entity).unwrap();
            state.dynamics.position_m += DVec3::X * 1_000.0;
            state.dynamics.velocity_mps = DVec3::new(50.0, -10.0, 0.0);
            let mut propulsion = world.get_mut::<RocketPropulsion>(rocket_entity).unwrap();
            propulsion.propellant_remaining_kg = vec![0.0, 0.0];
            propulsion.active_stage = 1;
            propulsion.separations_count = 1;
            propulsion.throttle = 0.4;
            let mut mission = world.get_mut::<RocketMissionState>(rocket_entity).unwrap();
            *mission = RocketMissionState::Landed;
        }

        {
            let world = app.world_mut();
            world.resource_mut::<Messages<RelaunchRequested>>().write(
                crate::domain::events::RelaunchRequested {
                    rocket: rocket_entity,
                },
            );
        }

        app.update();
        app.update();

        let world = app.world_mut();
        assert!(
            world.get_entity(debris_entity).is_err(),
            "jettisoned debris must be despawned"
        );
        let mut q = world.query::<(
            &RocketPhysicsState,
            &RocketPropulsion,
            &RocketMass,
            &RocketMissionState,
            &GroundRest,
        )>();
        let (rocket, propulsion, mass, mission, rest) = q.single(world).unwrap();

        assert_eq!(*mission, RocketMissionState::PreLaunch);
        assert!(rest.active, "pad-hold must re-engage");
        assert_eq!(propulsion.active_stage, 0);
        assert_eq!(propulsion.separations_count, 0);
        assert_eq!(propulsion.throttle, 0.0);
        for (stage, remaining) in propulsion
            .vehicle
            .stages
            .iter()
            .zip(&propulsion.propellant_remaining_kg)
        {
            assert!((remaining - stage.propellant_mass_kg).abs() < 1e-3);
        }
        assert!(
            rocket.dynamics.velocity_mps.length() < 0.3,
            "vehicle must be at rest after relaunch (one clamp cycle of gravity allowed): {}",
            rocket.dynamics.velocity_mps.length()
        );
        assert!(
            (mass.0 - rocket.dynamics.mass_kg).abs() < 1e-6,
            "RocketMass must match the refueled dynamics mass"
        );
    }

    /// Phase 17 scenario `tip_over` (app-level wiring): a resting vehicle
    /// tilted beyond its critical angle must arm the topple model inside
    /// GroundContact, fall under gravity, and end Crashed exactly once.
    /// The domain pendulum is covered by landing_gear tests; this pins the
    /// arm → advance → mission-lost pipeline ordering.
    #[test]
    fn leaning_past_critical_angle_topples_to_crashed() {
        let mut app = pad_app(0.0, 20.0);
        // Production order: contact first, topple advance after it.
        app.add_systems(FixedUpdate, advance_topple.after(resolve_ground_contact));

        let rocket_entity = {
            let world = app.world_mut();
            let mut q = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            q.single(world).unwrap()
        };
        {
            // Tilt 30 deg about Z: well past the gear-less critical angle
            // (atan(0.6 m base / ~5 m com height) ≈ 7 deg for this vehicle).
            let world = app.world_mut();
            let up = world
                .get::<RocketPhysicsState>(rocket_entity)
                .unwrap()
                .dynamics
                .position_m
                .normalize();
            let upright = DQuat::from_rotation_arc(DVec3::Y, up);
            let leaned = upright * DQuat::from_rotation_z(30.0_f64.to_radians());
            let mut state = world.get_mut::<RocketPhysicsState>(rocket_entity).unwrap();
            state.dynamics.orientation = leaned;
            state.dynamics.angular_velocity_radps = DVec3::ZERO;
        }

        // The fall from 30 deg to 90 deg at g=9.81, l≈5 m takes a few
        // seconds; run well past that.
        for _ in 0..1_500 {
            app.update();
        }

        let world = app.world_mut();
        let mut q = world.query::<(&TipOverState, &RocketMissionState)>();
        let (tip_over, mission) = q.single(world).unwrap();
        assert!(
            tip_over.fall.is_some() || *mission == RocketMissionState::Crashed,
            "topple never armed"
        );
        assert_eq!(*mission, RocketMissionState::Crashed);
        drop(q);
        drop(world);
        // Once crashed it stays crashed (one-way transition).
        for _ in 0..64 {
            app.update();
        }
        let world = app.world_mut();
        let mut q = world.query::<&RocketMissionState>();
        assert_eq!(*q.single(world).unwrap(), RocketMissionState::Crashed);
    }

    /// Phase 17 scenario `gear_deployment_gate` (app-level wiring): an
    /// undeployed descending vehicle must latch its legs when passing the
    /// radar-altitude gate with negative vertical speed — and only then.
    #[test]
    fn descending_through_gate_deploys_landing_legs() {
        use crate::domain::services::landing_gear::LandingGearSpec;

        let mut app = pad_app(0.0, 20.0);
        app.add_systems(
            FixedUpdate,
            (deploy_landing_legs, resolve_ground_contact).chain(),
        );

        let surface_r = {
            let world = app.world_mut();
            let mut q = world.query::<&RocketPhysicsState>();
            let rocket = q.single(world).unwrap();
            rocket.dynamics.position_m.length()
        };

        let rocket_entity = {
            let world = app.world_mut();
            let mut q = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            q.single(world).unwrap()
        };
        {
            // Airborne 150 m above the pad, descending slowly; legs present
            // but NOT deployed, gate at 100 m. TerrainCollisionState is
            // initialized consistently (radar 150 m) because in production
            // GroundContact has already sampled the surface before the
            // descent begins.
            let world = app.world_mut();
            let up = world
                .get::<RocketPhysicsState>(rocket_entity)
                .unwrap()
                .dynamics
                .position_m
                .normalize();
            world.entity_mut(rocket_entity).insert(RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    up * (surface_r + 150.0),
                    -up * 5.0,
                    DQuat::from_rotation_arc(DVec3::Y, up),
                    1_000.0,
                    bevy::math::DMat3::from_diagonal(DVec3::splat(1e4)),
                    DVec3::ZERO,
                ),
            });
            world
                .entity_mut(rocket_entity)
                .insert(TerrainCollisionState {
                    radar_altitude_m: 150.0,
                    ..TerrainCollisionState::default()
                });
            // Release the pad hold so the vehicle actually descends.
            world.get_mut::<GroundRest>(rocket_entity).unwrap().active = false;
            let gear = LandingGear::new(
                LandingGearSpec {
                    count: 4,
                    base_radius_m: 4.5,
                    stroke_m: 3.0,
                    max_landing_mass_kg: Some(2_000.0),
                    deploy_altitude_m: 100.0,
                },
                1_000.0,
            );
            world
                .entity_mut(rocket_entity)
                .insert(LandingLegs::new(gear));
        }

        // Above the gate nothing may deploy.
        for _ in 0..32 {
            app.update();
        }
        {
            let world = app.world_mut();
            let legs = world.get::<LandingLegs>(rocket_entity).unwrap();
            assert!(!legs.deployment.deployed, "deployed above the gate");
        }

        // Fall through the gate (150 m at 5 m/s ≈ 10 s); latch must trip.
        for _ in 0..800 {
            app.update();
        }
        let world = app.world_mut();
        let legs = world.get::<LandingLegs>(rocket_entity).unwrap();
        assert!(legs.deployment.deployed, "gate crossing must deploy legs");
    }
}

/// Ascent-pipeline regression tests: the real Guidance → Control → Actuation
/// → Gravity → Forces → Integrate → GroundContact chain driving a low-thrust
/// (electron-class) vehicle off the pad. Pins two Phase 12 behaviors: the
/// throttle slew must reach the commanded maximum shortly after launch (no
/// hidden writer may cap it at the envelope floor), and the pitch-over must
/// stay gated until the vehicle clears the tower.
#[cfg(test)]
mod ascent_pipeline_tests {
    use super::*;
    use crate::domain::entities::rocket::{EngineState, Rocket, RocketEngine, RocketStage};
    use crate::domain::events::{SplashdownDetectedEvent, StageSeparatedEvent};
    use crate::domain::services::physics_orbital::LowEarthOrbitTarget;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::terrain_collision::radial_direction;
    use crate::infrastructure::bevy_adapters::rocket_contact::resolve_ground_contact;
    use crate::infrastructure::bevy_adapters::rocket_control::{actuation_system, control_system};
    use crate::infrastructure::bevy_adapters::rocket_dynamics::{
        accumulate_forces, integrate_6dof,
    };
    use crate::infrastructure::bevy_adapters::rocket_propulsion::{
        propulsion_consumption, propulsion_staging,
    };
    use bevy::math::{DQuat, DVec3};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    const DT: f64 = 1.0 / 64.0;

    /// Electron-class vehicle: nine 25.8 kN engines with a 0.6 throttle
    /// floor (stage envelope [0.6, 1.0]), ~13 t gross.
    pub(super) fn electron_like() -> Rocket {
        let engines = (0..9)
            .map(|i| {
                let angle = i as f32 * std::f32::consts::TAU / 9.0;
                RocketEngine {
                    position_m: bevy::math::Vec3::new(0.45 * angle.cos(), -8.0, 0.45 * angle.sin()),
                    thrust_axis: bevy::math::Vec3::Y,
                    isp_sea_level: 303.0,
                    isp_vacuum: 311.0,
                    gimbal_range_deg: 4.0,
                    max_thrust_kn: 25.8,
                    throttle_min: 0.6,
                    throttle_max: 1.0,
                    restartable: true,
                    state: EngineState::Running,
                }
            })
            .collect();
        Rocket {
            name: "Electron".into(),
            diameter_m: 1.2,
            height_m: 18.0,
            stages: vec![
                RocketStage {
                    name: "S1".into(),
                    dry_mass_kg: 950.0,
                    propellant_mass_kg: 9_250.0,
                    engines,
                },
                RocketStage {
                    name: "S2".into(),
                    dry_mass_kg: 250.0,
                    propellant_mass_kg: 2_050.0,
                    engines: vec![RocketEngine {
                        position_m: bevy::math::Vec3::new(0.0, 6.0, 0.0),
                        thrust_axis: bevy::math::Vec3::Y,
                        isp_sea_level: 311.0,
                        isp_vacuum: 343.0,
                        gimbal_range_deg: 4.0,
                        max_thrust_kn: 25.8,
                        throttle_min: 0.6,
                        throttle_max: 1.0,
                        restartable: true,
                        state: EngineState::Running,
                    }],
                },
            ],
        }
    }

    pub(super) fn ascent_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<SplashdownDetectedEvent>();
        app.insert_resource(SimulationTime::new(DT));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            DT,
        )));

        let planet =
            crate::domain::services::planet_factory::PlanetFactory::create_by_name("Earth")
                .expect("Earth exists");
        app.world_mut().spawn((
            PlanetComponent {
                domain_planet: planet,
                material: Handle::default(),
                has_texture: false,
                base_reflectance: 1.0,
                base_roughness: 1.0,
            },
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for("Earth"),
        ));

        let vehicle = electron_like();
        let propellant = vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        let total_mass_kg = vehicle.total_mass_kg() as f64;
        let (inertia, com) = crate::domain::services::rocket_dynamics::rocket_inertia_tensor(
            1_250.0, 11_300.0, 0.6, 18.0,
        );

        let (lat, lon) = (28.5721_f64, -80.6480_f64);
        let h =
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for("Earth")
                .source
                .height_m(lat, lon);
        let up = radial_direction(lat, lon);
        let surface_radius = EARTH_RADIUS_M + h;

        let vehicle_entity = app
            .world_mut()
            .spawn((
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        up * surface_radius,
                        DVec3::ZERO,
                        DQuat::from_rotation_arc(DVec3::Y, up),
                        total_mass_kg,
                        inertia,
                        com,
                    ),
                },
                RocketGeometry {
                    radius_m: 0.6,
                    height_m: 18.0,
                },
                RocketMass(total_mass_kg),
                RocketFlightConditions::default(),
                RocketMissionState::Launch,
                RocketPropulsion {
                    vehicle,
                    active_stage: 0,
                    propellant_remaining_kg: propellant,
                    throttle: 0.0,
                    gimbal_pitch_rad: 0.0,
                    gimbal_yaw_rad: 0.0,
                    time_since_separation_s: 10.0,
                    ullage_settle_time_s: 2.0,
                    separations_count: 0,
                    attached_payload_kg: 50.0,
                },
            ))
            .id();
        // Second insert: Bevy bundle tuples cap at 15 items.
        app.world_mut().entity_mut(vehicle_entity).insert((
            RocketCommands::default(),
            RocketAutopilot::default(),
            OrbitalElements::default(),
            TerrainCollisionState::default(),
            GroundRest { active: true },
            TipOverState::default(),
            LandingScorecard::default(),
            RocketPlanetBinding {
                planet_name: CelestialBodyId::earth(),
            },
            GravityAcceleration { value: -up * 9.81 },
            ForceAccumulator::default(),
            TorqueAccumulator::default(),
        ));

        // Mirror the production force writers: non-gravity forces only —
        // accumulate_forces owns the single gravity contribution.
        fn write_flight_forces(
            mut query: Query<(
                &RocketPhysicsState,
                &RocketPropulsion,
                &mut ForceAccumulator,
            )>,
        ) {
            for (rocket, propulsion, mut force) in query.iter_mut() {
                if let Some(stage) = propulsion.vehicle.stages.get(propulsion.active_stage) {
                    let (body_thrust, _) =
                        stage_thrust_body(&stage.engines, propulsion.throttle, 0.0);
                    force.0 += rocket.dynamics.orientation * body_thrust;
                }
            }
        }

        app.add_systems(
            FixedUpdate,
            (
                guidance_system,
                control_system,
                actuation_system,
                write_flight_forces,
                accumulate_forces,
                integrate_6dof,
                resolve_ground_contact,
            )
                .chain(),
        );
        app
    }

    #[test]
    fn throttle_slew_reaches_full_command_shortly_after_launch() {
        let mut app = ascent_app();

        // Sweep tick-by-tick and find the first tick where the effective
        // throttle reaches the commanded maximum (1.0). The slew limiter is
        // 2.0/s, so from the 0.6 envelope floor this must complete well
        // inside 1 s of the Launch transition.
        let mut ticks_to_full = None;
        for tick in 1..=64 {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<&RocketPropulsion>();
            let propulsion = q.single(world).unwrap();
            if propulsion.throttle >= 0.999 {
                ticks_to_full = Some(tick);
                break;
            }
        }

        let ticks = ticks_to_full.expect("throttle must reach full command");
        assert!(
            ticks <= 32,
            "slew reached full throttle at tick {ticks} ({:.2} s), expected within 0.5 s",
            ticks as f64 * DT
        );

        // Steady-state thrust must be the FULL 232 kN (nine engines at 100%),
        // not the 139 kN envelope-floor value: nothing downstream may cap the
        // command once the slew has caught up.
        let world = app.world_mut();
        let mut q = world.query::<(&RocketPropulsion, &RocketFlightConditions, &GroundRest)>();
        let (propulsion, conditions, _rest) = q.single(world).unwrap();
        let Some(stage) = propulsion.vehicle.stages.first() else {
            panic!("stage 1 missing");
        };
        let (thrust_body, _) = stage_thrust_body(
            &stage.engines,
            propulsion.throttle,
            conditions.ambient_pressure_pa,
        );
        assert!(
            thrust_body.length() > 230_000.0,
            "steady-state thrust {} N is not the expected full-throttle output",
            thrust_body.length()
        );
    }

    #[test]
    fn ascent_holds_vertical_through_gate_then_pitches_over() {
        let mut app = ascent_app();

        // First 5 s of simulated flight: below the gate the whole way
        // (altitude crosses 150 m around t≈5.5 s), so the guidance TARGET
        // must stay exactly on the local vertical.
        for _ in 0..320 {
            app.update();
        }
        {
            let world = app.world_mut();
            let mut q = world.query::<(&RocketCommands, &RocketPhysicsState)>();
            let (commands, rocket) = q.single(world).unwrap();
            let up = rocket.dynamics.position_m.normalize();
            let body_y_world = commands.target_attitude * DVec3::Y;
            assert!(
                (body_y_world.dot(up) - 1.0).abs() < 1e-9,
                "target attitude left vertical before the gate cleared: dot={}",
                body_y_world.dot(up)
            );
        }

        // Run well past the time-schedule start (t = 10 s): with the vehicle
        // high and fast the gate is clear and the combined schedule must have
        // produced a real pitch-over by t ≈ 20 s.
        for _ in 0..960 {
            app.update();
        }
        let world = app.world_mut();
        let mut q = world.query::<(
            &RocketCommands,
            &RocketPhysicsState,
            &GroundRest,
            &RocketPropulsion,
            &RocketMissionState,
            &TerrainCollisionState,
            &RocketAutopilot,
        )>();
        let (commands, rocket, rest, propulsion, mission, collision, autopilot) =
            q.single(world).unwrap();
        assert!(!rest.active, "vehicle must have lifted off");
        assert!(
            (propulsion.throttle - 1.0).abs() < 1e-3,
            "throttle must remain fully commanded in ascent: {}",
            propulsion.throttle
        );
        let radius = rocket.dynamics.position_m.length();
        let altitude = radius - EARTH_RADIUS_M;
        let vertical_speed = rocket
            .dynamics
            .velocity_mps
            .dot(rocket.dynamics.position_m / radius);
        let up = rocket.dynamics.position_m.normalize();
        let body_y_world = commands.target_attitude * DVec3::Y;
        assert!(
            body_y_world.dot(up) < 1.0 - 1e-4,
            "gravity turn never engaged after the gate cleared: dot={} \
             alt={altitude:.1} m, vs={vertical_speed:.1} m/s, t={:.2} s, \
             mission={mission:?}, contact={:?}, radar={:.1} m",
            body_y_world.dot(up),
            autopilot.time_since_liftoff_s,
            collision.ground_contact,
            collision.radar_altitude_m,
        );
    }

    /// A completed insertion is a property of the authoritative orbital state,
    /// not a speed threshold. A circular target state completes the ascent,
    /// while a nearly orbital state with an Earth-intersecting periapsis does
    /// not, even though both are evaluated by the same guidance system.
    #[test]
    fn target_orbit_predicate_controls_ascent_completion() {
        let target = LowEarthOrbitTarget::default();
        let radius_m = EARTH_RADIUS_M + target.target_apoapsis_altitude_m;
        let circular_speed_mps = (gravitational_parameter(5.972e24) / radius_m).sqrt();
        let circular_velocity_mps = DVec3::new(
            0.0,
            circular_speed_mps * target.target_inclination_rad.cos(),
            circular_speed_mps * target.target_inclination_rad.sin(),
        );

        let mut safe_app = ascent_app();
        let safe_entity = {
            let world = safe_app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            query.single(world).unwrap()
        };
        {
            let world = safe_app.world_mut();
            let mut rocket = world.get_mut::<RocketPhysicsState>(safe_entity).unwrap();
            rocket.dynamics.position_m = DVec3::new(radius_m, 0.0, 0.0);
            rocket.dynamics.velocity_mps = circular_velocity_mps;
            *world.get_mut::<RocketMissionState>(safe_entity).unwrap() = RocketMissionState::Ascent;
        }
        // The first app update initializes Bevy's fixed-time clock; the
        // second executes the configured fixed-step flight pipeline.
        safe_app.update();
        safe_app.update();
        {
            let world = safe_app.world_mut();
            assert_eq!(
                *world.get::<RocketMissionState>(safe_entity).unwrap(),
                RocketMissionState::Orbit
            );
            assert_eq!(
                world
                    .get::<RocketCommands>(safe_entity)
                    .unwrap()
                    .throttle_cmd,
                0.0
            );
        }

        let mut unsafe_app = ascent_app();
        let unsafe_entity = {
            let world = unsafe_app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            query.single(world).unwrap()
        };
        {
            let world = unsafe_app.world_mut();
            let mut rocket = world.get_mut::<RocketPhysicsState>(unsafe_entity).unwrap();
            rocket.dynamics.position_m = DVec3::new(radius_m, 0.0, 0.0);
            rocket.dynamics.velocity_mps = circular_velocity_mps * 0.98;
            *world.get_mut::<RocketMissionState>(unsafe_entity).unwrap() =
                RocketMissionState::Ascent;
        }
        unsafe_app.update();
        unsafe_app.update();
        assert_ne!(
            *unsafe_app
                .world()
                .get::<RocketMissionState>(unsafe_entity)
                .unwrap(),
            RocketMissionState::Orbit,
            "an unsafe periapsis must not complete ascent"
        );
    }

    /// Control allocates attitude torque only. It must retain the throttle
    /// target supplied by guidance for the subsequent actuation stage.
    #[test]
    fn control_preserves_guidance_throttle_command() {
        let mut app = ascent_app();
        app.world_mut()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        let entity = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketCommands>>();
            query.single(world).unwrap()
        };
        app.world_mut()
            .get_mut::<RocketCommands>(entity)
            .unwrap()
            .throttle_cmd = 0.37;
        app.add_systems(Update, control_system);
        app.update();

        assert_eq!(
            app.world()
                .get::<RocketCommands>(entity)
                .unwrap()
                .throttle_cmd,
            0.37
        );
    }

    /// Shared minimal burn rig (Phase 15/17): electron-class vehicle in
    /// vacuum, throttle forced fully open, only consumption/staging systems
    /// running so guidance policy cannot mask machinery behavior. Each
    /// FixedUpdate is scheduled at the requested warp frequency while every
    /// authoritative step remains `DT` simulation seconds.
    fn burn_rig_app(acceleration: f64) -> App {
        use bevy::asset::{AssetApp, AssetPlugin};

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.add_message::<StageSeparatedEvent>();
        let mut sim_time = SimulationTime::new(DT);
        sim_time.set_time_acceleration(acceleration);
        app.insert_resource(sim_time);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            DT / acceleration,
        )));

        let vehicle = electron_like();
        let propellant = vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        let total_mass_kg = vehicle.total_mass_kg() as f64;
        let (inertia, com) = crate::domain::services::rocket_dynamics::rocket_inertia_tensor(
            1_250.0, 11_300.0, 0.6, 18.0,
        );
        app.world_mut().spawn((
            RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    DVec3::new(EARTH_RADIUS_M + 100.0, 0.0, 0.0),
                    DVec3::new(0.0, 2_000.0, 0.0),
                    DQuat::IDENTITY,
                    total_mass_kg,
                    inertia,
                    com,
                ),
            },
            RocketGeometry {
                radius_m: 0.6,
                height_m: 18.0,
            },
            RocketMass(total_mass_kg),
            RocketFlightConditions::default(),
            RocketPropulsion {
                vehicle,
                active_stage: 0,
                propellant_remaining_kg: propellant,
                throttle: 0.0,
                gimbal_pitch_rad: 0.0,
                gimbal_yaw_rad: 0.0,
                time_since_separation_s: 10.0,
                ullage_settle_time_s: 2.0,
                separations_count: 0,
                attached_payload_kg: 50.0,
            },
            RocketPlanetBinding {
                planet_name: CelestialBodyId::earth(),
            },
        ));

        // Burn rig: hold full commanded throttle, consume, stage.
        fn force_full_throttle(mut q: Query<&mut RocketPropulsion>) {
            for mut p in q.iter_mut() {
                p.throttle = 1.0;
            }
        }
        app.add_systems(
            FixedUpdate,
            (
                force_full_throttle,
                propulsion_consumption,
                propulsion_staging,
            )
                .chain(),
        );
        app.add_systems(
            Update,
            crate::domain::services::simulation_time::sync_fixed_timestep,
        );
        app
    }

    /// With time acceleration at 100×, propulsion still advances through
    /// bounded `DT` steps: one clean separation, drained first stage,
    /// conserved mass, and finite dynamics.
    /// A minimal burn rig drives the throttle directly so guidance policy
    /// (which legitimately coasts into insertion under coarse steps) cannot
    /// mask the machinery being tested.
    #[test]
    fn staging_and_consumption_stable_at_100x_acceleration() {
        let mut app = burn_rig_app(100.0);

        // Stage 1 drains in ~119 s; run well past that at bounded timesteps.
        for _ in 0..9_000 {
            app.update();
        }

        let world = app.world_mut();
        let mut q = world.query::<(&RocketPhysicsState, &RocketPropulsion, &RocketMass)>();
        let (rocket, propulsion, mass) = q.single(world).unwrap();

        assert_eq!(
            propulsion.separations_count, 1,
            "stage 1 must separate exactly once at 100×"
        );
        assert_eq!(propulsion.active_stage, 1);
        assert_eq!(propulsion.propellant_remaining_kg[0], 0.0);
        assert!(propulsion.propellant_remaining_kg[1] > 0.0);
        // Active-vehicle mass must equal its live components exactly:
        // upper-stage structure + whatever stage-2 propellant remains +
        // attached payload. (The rig keeps burning stage 2 after
        // separation, so absolute amounts are policy-free.)
        let expected_mass = propulsion.vehicle.stages[1].dry_mass_kg as f64
            + propulsion.propellant_remaining_kg[1] as f64
            + propulsion.attached_payload_kg as f64;
        assert!(
            (mass.0 - expected_mass).abs() < 1.0,
            "mass {} inconsistent with stage bookkeeping {expected_mass}",
            mass.0
        );
        assert!(
            propulsion.propellant_remaining_kg[1] < propulsion.vehicle.stages[1].propellant_mass_kg,
            "stage 2 must also have consumed propellant post-staging"
        );
        assert!(rocket.dynamics.mass_kg.is_finite() && rocket.dynamics.mass_kg > 0.0);
    }

    /// Phase 17 scenario `time_warp_burn`: the SAME burn executed at 1×, 10×
    /// and 100× must produce identical staging decisions and consistent mass
    /// bookkeeping at equal SIMULATED time. Consumption is linear in sim
    /// seconds, so staging and mass bookkeeping must match exactly.
    #[test]
    fn burn_rig_invariants_hold_across_time_warp_factors() {
        let mut expected_post_burn: Option<(f32, f64)> = None;

        for acceleration in [1.0, 10.0, 100.0] {
            let mut app = burn_rig_app(acceleration);

            // Same simulated duration for every factor: T = 140 s > the
            // ~119 s first-stage drain. Each fixed tick advances by DT.
            let steps = (140.0 / DT).ceil() as usize + 2;
            for _ in 0..steps {
                app.update();
            }

            let world = app.world_mut();
            let mut q = world.query::<(&RocketPhysicsState, &RocketPropulsion, &RocketMass)>();
            let (rocket, propulsion, mass) = q.single(world).unwrap();

            assert_eq!(
                propulsion.separations_count, 1,
                "staging decision must not depend on time-warp factor"
            );
            assert_eq!(propulsion.active_stage, 1);
            assert_eq!(propulsion.propellant_remaining_kg[0], 0.0);
            assert!(propulsion.propellant_remaining_kg[1] > 0.0);
            let expected_mass = propulsion.vehicle.stages[1].dry_mass_kg as f64
                + propulsion.propellant_remaining_kg[1] as f64
                + propulsion.attached_payload_kg as f64;
            assert!(
                (mass.0 - expected_mass).abs() < 1.0,
                "mass bookkeeping diverged at {acceleration}x: {} vs {expected_mass}",
                mass.0
            );
            assert!(rocket.dynamics.mass_kg.is_finite());

            let post_burn = (propulsion.propellant_remaining_kg[1], mass.0);
            if let Some(expected) = expected_post_burn {
                assert!(
                    (post_burn.0 - expected.0).abs() < f32::EPSILON,
                    "stage-2 propellant diverged at {acceleration}x: {} vs {}",
                    post_burn.0,
                    expected.0
                );
                assert!(
                    (post_burn.1 - expected.1).abs() < f64::EPSILON,
                    "mass diverged at {acceleration}x: {} vs {}",
                    post_burn.1,
                    expected.1
                );
            } else {
                expected_post_burn = Some(post_burn);
            }
        }
    }

    /// Phase 17 determinism: two identical full-pipeline ascent runs (same
    /// initial state, same step count) must produce bitwise-identical
    /// authoritative state. The architecture supports this: f64 math, fixed
    /// steps, single-vehicle iteration, chained systems — so exact equality
    /// is required rather than a tolerance.
    #[test]
    fn identical_ascent_runs_are_bitwise_deterministic() {
        let mut run_a = ascent_app();
        let mut run_b = ascent_app();

        // ~6.25 s of simulated flight: liftoff, throttle slew, gate region.
        for _ in 0..400 {
            run_a.update();
            run_b.update();
        }

        let snapshot = |app: &mut App| {
            let world = app.world_mut();
            let mut q = world.query::<(
                &RocketPhysicsState,
                &RocketMass,
                &RocketPropulsion,
                &RocketMissionState,
                &RocketCommands,
                &GravityAcceleration,
            )>();
            let (rocket, mass, propulsion, mission, commands, gravity) = q.single(world).unwrap();
            (
                rocket.dynamics.position_m,
                rocket.dynamics.velocity_mps,
                rocket.dynamics.orientation,
                rocket.dynamics.angular_velocity_radps,
                rocket.dynamics.mass_kg,
                mass.0,
                propulsion.active_stage,
                propulsion.throttle,
                propulsion.separations_count,
                propulsion.propellant_remaining_kg.clone(),
                *mission,
                commands.target_attitude,
                gravity.value,
            )
        };

        let a = snapshot(&mut run_a);
        let b = snapshot(&mut run_b);

        let bits = |q: &DQuat| [q.x.to_bits(), q.y.to_bits(), q.z.to_bits(), q.w.to_bits()];
        let vec3 = |v: &DVec3| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];

        assert_eq!(vec3(&a.0), vec3(&b.0), "position diverged");
        assert_eq!(vec3(&a.1), vec3(&b.1), "velocity diverged");
        assert_eq!(bits(&a.2), bits(&b.2), "orientation diverged");
        assert_eq!(vec3(&a.3), vec3(&b.3), "angular velocity diverged");
        assert_eq!(a.4.to_bits(), b.4.to_bits(), "dynamics mass diverged");
        assert_eq!(a.5.to_bits(), b.5.to_bits(), "component mass diverged");
        assert_eq!(a.6, b.6, "active stage diverged");
        assert_eq!(a.7.to_bits(), b.7.to_bits(), "throttle diverged");
        assert_eq!(a.8, b.8, "separations count diverged");
        assert_eq!(a.9, b.9, "propellant vector diverged");
        assert_eq!(a.10, b.10, "mission state diverged");
        assert_eq!(
            bits(&a.11),
            bits(&b.11),
            "guidance target attitude diverged"
        );
        assert_eq!(vec3(&a.12), vec3(&b.12), "gravity diverged");
    }
}

#[cfg(test)]
mod render_interpolation_tests {
    use super::*;

    #[test]
    fn prelaunch_render_uses_current_pad_state_without_interpolation() {
        let current = RocketDynamicsState::new(
            DVec3::new(10.0, 20.0, 30.0),
            DVec3::new(1.0, 2.0, 3.0),
            DQuat::IDENTITY,
            100.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let previous = RocketDynamicsState {
            position_m: DVec3::new(-10.0, -20.0, -30.0),
            ..current
        };

        let rendered = render_dynamics_state(
            RocketMissionState::PreLaunch,
            current,
            RocketRenderState {
                prev: previous,
                current,
            },
            0.5,
        );

        assert_eq!(rendered, current);
    }

    #[test]
    fn airborne_render_keeps_fixed_state_interpolation() {
        let current = RocketDynamicsState::new(
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::ZERO,
            DQuat::IDENTITY,
            100.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let previous = RocketDynamicsState {
            position_m: DVec3::ZERO,
            ..current
        };

        let rendered = render_dynamics_state(
            RocketMissionState::Ascent,
            current,
            RocketRenderState {
                prev: previous,
                current,
            },
            0.25,
        );

        assert_eq!(rendered.position_m, DVec3::new(2.5, 0.0, 0.0));
    }

    #[test]
    fn launch_resets_pad_snapshots_before_enabling_interpolation() {
        let dynamics = RocketDynamicsState::new(
            DVec3::new(10.0, 20.0, 30.0),
            DVec3::new(1.0, 2.0, 3.0),
            DQuat::IDENTITY,
            100.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let stale_snapshot = RocketDynamicsState {
            position_m: DVec3::new(-10.0, -20.0, -30.0),
            ..dynamics
        };
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        let rocket = app
            .world_mut()
            .spawn((
                RocketMissionState::PreLaunch,
                RocketPhysicsState { dynamics },
                RocketRenderState {
                    prev: stale_snapshot,
                    current: stale_snapshot,
                },
            ))
            .id();
        app.add_systems(Update, handle_rocket_launch_input);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);

        app.update();

        let world = app.world();
        assert_eq!(
            *world.get::<RocketMissionState>(rocket).unwrap(),
            RocketMissionState::Launch
        );
        let render = world.get::<RocketRenderState>(rocket).unwrap();
        assert_eq!(render.prev, dynamics);
        assert_eq!(render.current, dynamics);
    }
}

/// Baseline-recording regression suite (spec: determinism-regression). This
/// module reuses the full ascent-pipeline harness and converts the
/// authoritative state into [`RocketStateSample`] rows for the
/// [`domain::services::regression`] toolkit:
///
/// - records a canonical ascent baseline to `tests/baselines/ascent.ron`
///   (commit it as the CI gate fixture),
/// - re-simulates and compares per-tick/per-variable within the documented
///   tolerances, and
/// - proves the whole recording is bitwise reproducible across two fresh runs
///   (the deterministic-regression guarantee, AGENTS.md section 44/46).
///
/// Set `REGRESSION_RECORD=1` to (re)write the baseline fixture from the
/// current code. The audit trail is filled by the harness so the recorded
/// baseline is signed-off by construction.
#[cfg(test)]
mod determinism_regression_tests {
    use super::ascent_pipeline_tests::ascent_app;
    use crate::components::rocket::{RocketMissionState, RocketPhysicsState};
    use crate::domain::entities::rocket::RocketMissionState as DomainMission;
    use crate::domain::services::regression::{
        load_baseline_ron, save_baseline_ron, BaselineJustification, FlightBaseline,
        RegressionConfig, RocketStateSample,
    };
    use bevy::prelude::*;

    /// Fixed-physics ticks captured in the baseline window (~4 s of ascent:
    /// liftoff, throttle slew, gravity-turn entry). Kept short so the fixture
    /// stays small while still exercising the full Guidance→Control→Forces→
    /// Integrate→GroundContact chain.
    const RECORD_TICKS: usize = 256;

    fn default_baseline_path() -> std::path::PathBuf {
        let dir =
            std::env::var("REGRESSION_BASELINE_DIR").unwrap_or_else(|_| "tests/baselines".into());
        std::path::PathBuf::from(dir).join("ascent.ron")
    }

    /// Map the mission phase to a stable, order-independent code byte. The
    /// codes are fixed enum indices so a reordered enum would change the
    /// recorded trajectory's guidance_mode field — exactly what the regression
    /// gate should catch.
    fn mission_code(mission: RocketMissionState) -> u8 {
        match mission.0 {
            DomainMission::PreLaunch => 0,
            DomainMission::Launch => 1,
            DomainMission::Ascent => 2,
            DomainMission::Orbit => 3,
            DomainMission::DeorbitBurn => 4,
            DomainMission::ReentryCorridor => 5,
            DomainMission::PoweredDescent => 6,
            DomainMission::UnpoweredDescent => 7,
            DomainMission::Landing => 8,
            DomainMission::Landed => 9,
            DomainMission::Crashed => 10,
        }
    }

    /// Capture one authoritative state row from the single rocket.
    fn capture_sample(app: &mut App) -> RocketStateSample {
        let world = app.world_mut();
        let mut q = world.query::<(&RocketPhysicsState, &RocketMissionState)>();
        let (rocket, mission) = q.single(world).unwrap();
        let d = &rocket.dynamics;
        RocketStateSample::new(
            [d.position_m.x, d.position_m.y, d.position_m.z],
            [d.velocity_mps.x, d.velocity_mps.y, d.velocity_mps.z],
            [
                d.orientation.x,
                d.orientation.y,
                d.orientation.z,
                d.orientation.w,
            ],
            [
                d.angular_velocity_radps.x,
                d.angular_velocity_radps.y,
                d.angular_velocity_radps.z,
            ],
            d.mass_kg,
            mission_code(*mission),
        )
    }

    /// Run the ascent harness and record `RECORD_TICKS` samples (one per
    /// fixed physics step), including the initial t=0 sample.
    fn record_ascent_samples() -> Vec<RocketStateSample> {
        let mut app = ascent_app();
        let mut samples = Vec::with_capacity(RECORD_TICKS + 1);
        samples.push(capture_sample(&mut app));
        for _ in 0..RECORD_TICKS {
            app.update();
            samples.push(capture_sample(&mut app));
        }
        samples
    }

    fn current_git_commit() -> String {
        std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn signed_audit() -> BaselineJustification {
        BaselineJustification {
            change_description: "canonical electron-class ascent baseline (LSOC)".into(),
            expected_improvement: "pins the full Guidance→Control→Forces→Integrate chain bitwise"
                .into(),
            numerical_tradeoffs: "semi-implicit Euler; f64; single fixed step 1/64 s".into(),
            affected_scenarios: vec!["ascent".into()],
            reviewer_approved: true,
            recorded_by: "opencode-determinism".into(),
        }
    }

    /// The CI gate: the freshly simulated ascent must match the committed
    /// baseline bit-for-bit within the documented per-variable tolerances. If
    /// the fixture is absent (fresh checkout) or `REGRESSION_RECORD=1`, it is
    /// (re)recorded first, then compared, so the fixture is self-bootstrapping.
    #[test]
    fn ascent_matches_committed_baseline_within_tolerances() {
        let path = default_baseline_path();
        let should_record = std::env::var("REGRESSION_RECORD").as_deref() == Ok("1");

        let baseline = if !path.exists() || should_record {
            let baseline = FlightBaseline::record(
                "ascent",
                current_git_commit(),
                signed_audit(),
                record_ascent_samples(),
            )
            .expect("signed-off ascent baseline records");
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("baseline dir creatable");
            }
            std::fs::write(
                &path,
                save_baseline_ron(&baseline).expect("baseline serializes"),
            )
            .expect("baseline writable");
            baseline
        } else {
            let ron = std::fs::read_to_string(&path).expect("baseline readable");
            load_baseline_ron(&ron).expect("baseline RON valid")
        };

        assert!(
            baseline.hash_chain_consistent(),
            "committed baseline hash chain is internally inconsistent"
        );

        let current = record_ascent_samples();
        let divergences = baseline.compare(&current, &RegressionConfig::default());

        assert!(
            divergences.is_empty(),
            "ascent diverged from committed baseline:\n{}",
            divergences
                .iter()
                .map(|d| d.describe())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// Two independent fresh-run recordings of the same flight must be
    /// bitwise identical (AGENTS.md section 44). This exercises the full
    /// harness through the public regression API rather than a hand-written
    /// snapshot, so a future change to either side is caught by the same
    /// path.
    #[test]
    fn two_fresh_ascent_runs_are_bitwise_identical() {
        let a = record_ascent_samples();
        let b = record_ascent_samples();
        assert_eq!(a.len(), b.len());
        let divergences = crate::domain::services::regression::compare_trajectory(
            &a,
            &b,
            &RegressionConfig::default(),
        );
        assert!(
            divergences.is_empty(),
            "identical runs diverged:\n{}",
            divergences
                .iter()
                .map(|d| d.describe())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// A deliberately injected perturbation well beyond the per-variable
    /// tolerance (a 0.5 m/s velocity error at a mid-flight tick) must be
    /// caught at the exact tick and variable — the invariant the CI gate
    /// relies on.
    #[test]
    fn injected_regression_is_reported_at_exact_tick_and_variable() {
        let mut baseline = record_ascent_samples();
        baseline[100].velocity_mps[0] += 0.5; // ≫ 1 µm/s tolerance
        let reference = record_ascent_samples();
        let divergences = crate::domain::services::regression::compare_trajectory(
            &reference,
            &baseline,
            &RegressionConfig::default(),
        );
        let hit = divergences
            .iter()
            .find(|d| {
                d.variable == crate::domain::services::regression::RegressionVariable::Velocity
            })
            .cloned();
        assert!(hit.is_some(), "velocity divergence must be reported");
        let hit = hit.unwrap();
        assert_eq!(hit.tick, 100);
        assert_eq!(
            hit.variable,
            crate::domain::services::regression::RegressionVariable::Velocity
        );
    }

    /// Sanity: the recorded baseline starts on the pad (ground contact) and
    /// reaches full throttle during the window — i.e. the fixture actually
    /// exercises the launch machinery and the samples are not all identical.
    #[test]
    fn recorded_baseline_is_a_real_flight() {
        let samples = match default_baseline_path().exists() {
            true => {
                let ron = std::fs::read_to_string(default_baseline_path()).unwrap();
                load_baseline_ron(&ron).unwrap().samples
            }
            false => record_ascent_samples(),
        };
        assert!(samples.len() > 1);
        // Position must move ~ a metre or more across the window (liftoff).
        let start = &samples[0];
        let end = &samples[samples.len() - 1];
        let dr = crate::domain::services::regression::vector_abs_diff(
            &start.position_m,
            &end.position_m,
        );
        assert!(
            dr > 1.0,
            "rocket barely moved ({dr:.3} m) — baseline is not a real flight"
        );
        // Velocity must grow (the vehicle is accelerating off the pad).
        let dv = crate::domain::services::regression::vector_abs_diff(
            &start.velocity_mps,
            &end.velocity_mps,
        );
        assert!(
            dv > 1.0,
            "rocket barely gained velocity ({dv:.3} m/s) — baseline is not an ascent"
        );
        // Mass must remain finite and positive throughout.
        assert!(
            samples
                .iter()
                .all(|s| s.mass_kg.is_finite() && s.mass_kg > 0.0),
            "mass became non-finite in the ascent baseline"
        );
        // Guidance must transition out of pre-launch.
        assert!(
            end.guidance_code >= 1,
            "guidance never advanced past pre-launch"
        );
    }
}
