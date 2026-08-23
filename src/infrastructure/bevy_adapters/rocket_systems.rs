use crate::components::rocket::*;
use crate::domain::events::{
    CommsBlackoutEvent, RelaunchRequested, SplashdownDetectedEvent, StageSeparatedEvent,
};
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
    advance_ascent_phase, advance_descent_phase, attitude_from_direction, boostback_guidance,
    gravity_turn_direction_gated, hover_slam_guidance, pitch_axis_from_reference,
    powered_descent_guidance_convex, prograde_attitude, reentry_bank_angle,
    reentry_bank_angle_enhanced, suicide_burn_guidance, AutopilotMode, DescentGuidanceConfig,
};
use crate::domain::services::landing_gear::{topple_critical_angle_rad, ToppleFall};
use crate::domain::services::physics_orbital::orbital_elements_from_state;
use crate::domain::services::rocket_propulsion::{
    active_vehicle_inertia, active_vehicle_mass_with_payload, air_start_allowed,
    allocate_gimbal_deflections, clamp_gimbal, clamp_throttle_range, consume_propellant,
    gimbal_torque_body, ignition_allowed_during_ullage, separation_impulse, shed_stage,
    stage_throttle_envelope, stage_thrust_body, MIN_SEPARATION_CLEARANCE_M,
    SEPARATION_UPPER_DV_MPS, SPENT_STAGE_RETRO_DV_MPS,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_collision::{
    decompose_velocity, evaluate_touchdown, lat_lon_from_direction, liftoff_from_rest,
    resolve_resting_contact, sample_surface, GroundContact, TouchdownCriteria, TOUCHDOWN_BAND_M,
};
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::components::{
    AerodynamicForces, AtmosphereState, EntryPhysicsConfig, MaxQTracker, PlanetAtmosphere,
    PlanetComponent, PlanetTerrain, RocketAutopilot, RocketCommands, TerrainCollisionState,
};
use crate::infrastructure::bevy_adapters::rocket_separation::{spawn_spent_stage, SpentStageSpec};
use crate::infrastructure::bevy_adapters::rocket_telemetry::FlightRecorder;
use bevy::ecs::query::QueryData;
use bevy::math::{DMat3, DQuat, DVec3};
use bevy::prelude::*;

/// Fraction of the circular orbital speed at which ascent guidance declares
/// orbit insertion.
const ORBIT_SPEED_FRACTION: f64 = 0.98;

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
    pub orbital: &'static OrbitalElements,
    pub commands: &'static mut RocketCommands,
}

/// Bundled access for the ground-contact authority ([`RocketSet::GroundContact`]):
/// the just-integrated dynamics plus every piece of state a contact verdict or
/// constraint may touch. Gear access is optional — gear-less vehicles take the
/// rigid point-contact path.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct GroundContactAccess {
    pub entity: Entity,
    pub binding: &'static RocketPlanetBinding,
    pub dynamics: &'static mut RocketPhysicsState,
    pub propulsion: &'static RocketPropulsion,
    pub geometry: &'static RocketGeometry,
    pub collision: &'static mut TerrainCollisionState,
    pub rest: &'static mut GroundRest,
    pub mission_state: &'static mut RocketMissionState,
    pub legs: Option<&'static mut LandingLegs>,
    /// Tip-over monitor/fall model (Phase 14 lifecycle).
    pub tip_over: &'static mut TipOverState,
    /// One-shot touchdown record (Phase 14 lifecycle).
    pub scorecard: &'static mut LandingScorecard,
    /// Autopilot state, read for the landing-target distance record.
    pub autopilot: &'static RocketAutopilot,
}

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

/// Accumulate the gravitational force acting on each rocket ON TOP of the
/// forces written by earlier systems (aero, parachutes, thrust). Forces are
/// in the planet-centered inertial meter frame. This must ADD, not overwrite:
/// integration clears the accumulators at the end of every step
/// (`integrate_6dof`), so an overwrite here would silently discard every
/// non-gravity force (regression-tested in `rocket_separation::tests`).
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
        force_accum.0 += gravity_force;
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
        // Air-start gate: after any separation, only restartable engines may
        // light. Combined with the ullage settle gate below.
        let engines_restartable = stage.engines.iter().all(|e| e.restartable);
        if !air_start_allowed(propulsion.separations_count > 0, engines_restartable) {
            continue;
        }
        // Ullage gate: no ignition until propellant has settled post-staging.
        if !ignition_allowed_during_ullage(
            propulsion.time_since_separation_s,
            propulsion.ullage_settle_time_s,
        ) {
            continue;
        }
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
        // Air-start gate: consumption follows the same ignition authority as
        // thrust so mass bookkeeping can never diverge from applied force.
        let engines_restartable = stage.engines.iter().all(|e| e.restartable);
        if !air_start_allowed(propulsion.separations_count > 0, engines_restartable) {
            continue;
        }
        // Ullage gate: consumption follows the same ignition authority as
        // thrust so mass bookkeeping can never diverge from applied force.
        if !ignition_allowed_during_ullage(
            propulsion.time_since_separation_s,
            propulsion.ullage_settle_time_s,
        ) {
            continue;
        }
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

        let new_mass = active_vehicle_mass_with_payload(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
            propulsion.attached_payload_kg,
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

/// Separate the spent stage when its propellant is exhausted and the vehicle
/// is still thrusting:
/// - applies the domain separation impulse to the vehicle (upper stage),
/// - respawns the spent stage as its own debris entity (`SpentStage`) carrying
///   the pre-separation dynamics plus the retro impulse,
/// - restarts the ullage settle timer for the upper stage's next ignition,
/// - emits [`StageSeparatedEvent`].
///
/// Vehicle mass/inertia are recomputed from the active stage afterwards.
pub fn propulsion_staging(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sim_time: Res<SimulationTime>,
    mut separated_writer: MessageWriter<StageSeparatedEvent>,
    mut rocket_query: Query<(
        Entity,
        &RocketPlanetBinding,
        &RocketGeometry,
        &mut RocketPhysicsState,
        &mut RocketMass,
        &mut RocketPropulsion,
    )>,
) {
    let dt = sim_time.fixed_timestep() as f32;
    for (entity, binding, geometry, mut rocket, mut mass, mut propulsion) in rocket_query.iter_mut()
    {
        // Advance the post-separation timer every tick; reset on staging.
        propulsion.time_since_separation_s += dt;

        let remaining = propulsion
            .propellant_remaining_kg
            .get(propulsion.active_stage)
            .copied()
            .unwrap_or(0.0);
        let thrusting = propulsion.throttle.clamp(0.0, 1.0) > 0.0;
        if remaining > 0.0 || !thrusting {
            continue;
        }
        let Some((next, shed)) = shed_stage(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
        ) else {
            continue;
        };

        // Pre-separation dynamics become the spent stage's initial state.
        let pre_separation = rocket.dynamics;

        propulsion.active_stage = next;
        // The next stage's ignition is now an air-start: it requires the
        // stage's engines to be restartable, on top of the ullage settle.
        propulsion.separations_count += 1;

        // Separation impulses: pusher Δv to the upper stage, optional retro
        // Δv to the spent stage (pure domain function).
        let outcome = separation_impulse(
            pre_separation.velocity_mps,
            pre_separation.orientation,
            DVec3::Y,
            SEPARATION_UPPER_DV_MPS,
            SPENT_STAGE_RETRO_DV_MPS,
        );
        rocket.dynamics.velocity_mps = outcome.upper_velocity_mps;

        let new_mass = active_vehicle_mass_with_payload(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
            propulsion.attached_payload_kg,
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

        // Spent-stage debris: pre-separation dynamics + retro impulse, shed
        // dry + residual mass, height estimated from the stage count
        // (documented approximation until per-stage lengths exist).
        let mut spent_dynamics = pre_separation;
        spent_dynamics.velocity_mps = outcome.spent_velocity_mps;
        spent_dynamics.mass_kg = shed;
        let estimated_height_m = geometry.height_m / propulsion.vehicle.stages.len() as f32;

        // Interstage collision avoidance: the impulse model guarantees
        // growing clearance over time; defensively log a spawn that would
        // start inside the minimum clearance band (limitation documented in
        // AGENTS.md section 71 notes — no continuous collision shape).
        if MIN_SEPARATION_CLEARANCE_M > estimated_height_m as f64 {
            bevy::log::warn!(
                "Separation clearance {MIN_SEPARATION_CLEARANCE_M} m exceeds estimated stage length {estimated_height_m} m"
            );
        }

        let spent_entity = spawn_spent_stage(
            &mut commands,
            &mut meshes,
            &mut materials,
            SpentStageSpec {
                parent_rocket: entity,
                planet_name: binding.planet_name.clone(),
                dynamics: spent_dynamics,
                radius_m: geometry.radius_m,
                height_m: estimated_height_m,
                kind: SpentStageKind::Booster,
            },
        );

        // Ullage: the next ignition must wait for propellant to settle.
        propulsion.time_since_separation_s = 0.0;

        separated_writer.write(StageSeparatedEvent {
            rocket: entity,
            spent_stage: spent_entity,
            shed_mass_kg: shed,
        });
        bevy::log::info!(
            "Stage {} separated: shed {shed:.0} kg, upper stage {:.0} kg",
            propulsion.active_stage - 1,
            new_mass
        );
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
        let orbital = access.orbital;
        let mass = access.mass;
        let mission_state = &mut *access.mission_state;
        let autopilot = &mut *access.autopilot;
        let commands = &mut *access.commands;

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
        // The dynamics model carries a diagonal body-frame inertia; pass its
        // diagonal so the normalized gains produce vehicle-scaled torque.
        let inertia_diag = DVec3::new(
            rocket.dynamics.inertia_body.x_axis.x,
            rocket.dynamics.inertia_body.y_axis.y,
            rocket.dynamics.inertia_body.z_axis.z,
        );
        let torque = control_torque_body(
            commands.target_attitude,
            rocket.dynamics.orientation,
            rocket.dynamics.angular_velocity_radps,
            inertia_diag,
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
        // Slew limit first, then clamp to the active stage's per-engine
        // throttle envelope (intersection of individual engine ranges), so
        // no engine is commanded outside its own capability.
        let slewed = limit_throttle_slew(
            propulsion.throttle,
            commands.throttle_cmd,
            limits.max_throttle_slew_per_s,
            dt,
        );
        let envelope = propulsion
            .vehicle
            .stages
            .get(propulsion.active_stage)
            .map(|stage| stage_throttle_envelope(&stage.engines))
            .unwrap_or((0.0, 1.0));
        // Commanded-off must bypass the envelope floor: raising the slewed
        // value back to throttle_min would make shutdown impossible (the
        // engine can never reach zero once lit).
        propulsion.throttle = if commands.throttle_cmd <= 0.0 {
            slewed
        } else {
            clamp_throttle_range(slewed, envelope.0, envelope.1)
        };
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

/// Landing-gear deployment: advances each vehicle's one-way deployment latch
/// from the authoritative radar altitude and surface-relative vertical speed.
/// Runs in [`RocketSet::EntryPhysics`] so the gear is down before any
/// GroundContact verdict can be gear-aware. Never touches motion.
pub fn deploy_landing_legs(
    mut rocket_query: Query<(
        &TerrainCollisionState,
        &RocketPhysicsState,
        &mut LandingLegs,
    )>,
) {
    for (collision, rocket, mut legs) in rocket_query.iter_mut() {
        let radius = rocket.dynamics.position_m.length();
        if radius < 1.0 {
            continue;
        }
        let up_dir = rocket.dynamics.position_m / radius;
        let vertical_speed = rocket.dynamics.velocity_mps.dot(up_dir);
        let deploy_gate_altitude_m = legs.deploy_gate_altitude_m();
        if legs.deployment.update(
            deploy_gate_altitude_m,
            collision.radar_altitude_m,
            vertical_speed,
        ) {
            bevy::log::info!(
                "Landing legs deployed at {:.0} m AGL",
                collision.radar_altitude_m
            );
        }
    }
}

/// Authoritative rocket–terrain contact. Runs POST-integration in
/// [`RocketSet::GroundContact`], so verdicts and constraints act on the
/// just-integrated state: samples collision terrain, refreshes the
/// [`TerrainCollisionState`] sensors, evaluates multi-criteria touchdown,
/// enforces the resting-contact constraint (`resolve_resting_contact`:
/// penetration clamp + normal-velocity removal + tangential damping),
/// releases rest when thrust exceeds weight, and emits splashdown on water
/// touchdowns exactly as before.
pub fn resolve_ground_contact(
    sim_time: Res<SimulationTime>,
    mut splashdown_writer: MessageWriter<SplashdownDetectedEvent>,
    planet_query: Query<(&PlanetComponent, &PlanetTerrain)>,
    mut rocket_query: Query<GroundContactAccess>,
) {
    let dt = sim_time.fixed_timestep();
    /// Terrain heights within this band of mean sea level are treated as
    /// water on bodies with oceans.
    const SEA_LEVEL_TOLERANCE_M: f64 = 10.0;

    for mut access in rocket_query.iter_mut() {
        // Rebind the bundled fields; mutable fields deref out of Bevy's
        // change-detection wrappers.
        let rocket_entity = access.entity;
        let binding = access.binding;
        let propulsion = access.propulsion;
        let geometry = access.geometry;
        let autopilot = access.autopilot;
        let rocket = &mut *access.dynamics;
        let collision = &mut *access.collision;
        let rest = &mut *access.rest;
        let mission_state = &mut *access.mission_state;
        let mut legs = access.legs.as_deref_mut();
        let tip_over = &mut *access.tip_over;
        let scorecard = &mut *access.scorecard;
        let Some((planet, planet_terrain)) = planet_query
            .iter()
            .find(|(planet, _)| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };
        let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;

        let position_m = rocket.dynamics.position_m;
        let dir = position_m.normalize_or_zero();
        if dir.length_squared() < 1e-12 {
            continue;
        }
        let (lat, lon) = lat_lon_from_direction(dir);
        let sample = sample_surface(&*planet_terrain.source, lat, lon, radius_m);
        // Signed altitude: negative means penetrating the sampled surface.
        let surface_radius_m = radius_m + sample.height_m;
        let signed_altitude_m = position_m.length() - surface_radius_m;

        collision.radar_altitude_m = signed_altitude_m.max(0.0);
        collision.slope_deg = sample.slope_deg;

        // Water inference: no ocean mask data exists yet, so water is where
        // the terrain elevation sits at mean sea level (Earth only — the
        // Moon/Mars have no seas). Documented approximation.
        let has_ocean = planet.domain_planet.name == "Earth";
        collision.over_water = has_ocean && sample.height_m.abs() <= SEA_LEVEL_TOLERANCE_M;

        let normal = if sample.normal.length_squared() > 1e-12 {
            sample.normal
        } else {
            dir
        };
        // Tilt of the longitudinal body axis (+Y nose-up convention at spawn)
        // away from the local surface normal.
        let tilt_deg = (rocket.dynamics.orientation * DVec3::Y)
            .angle_between(normal)
            .to_degrees();
        let velocity = rocket.dynamics.velocity_mps;
        let components = decompose_velocity(velocity, normal);

        // Touchdown criteria, gear-aware when the legs are deployed: the
        // stance-aspect widening lives in the domain (`LandingGear::
        // touchdown_criteria`); undeployed (or absent) legs keep the no-gear
        // criteria exactly.
        let criteria = match legs.as_ref() {
            Some(legs) if legs.deployed() => legs
                .gear
                .touchdown_criteria(TouchdownCriteria::default(), geometry.height_m as f64),
            _ => TouchdownCriteria::default(),
        };

        // Release first: a resting vehicle leaves the ground as soon as the
        // active stage's available thrust exceeds its weight (guidance ramps
        // throttle up → constraint lets go → normal integration takes over).
        if rest.active {
            let gravity_mps2 =
                gravitational_parameter(planet.domain_planet.mass_kg) / position_m.length().powi(2);
            let weight_n = rocket.dynamics.mass_kg * gravity_mps2;
            let thrust_n = propulsion
                .vehicle
                .stages
                .get(propulsion.active_stage)
                .map(|stage| {
                    // Thrust magnitude does not depend on the density-driven
                    // Isp selection; pass vacuum-ish 0.0.
                    stage_thrust_body(&stage.engines, propulsion.throttle, 0.0)
                        .0
                        .length()
                })
                .unwrap_or(0.0);
            if liftoff_from_rest(thrust_n, weight_n) {
                rest.active = false;
                collision.ground_contact = GroundContact::None;
                bevy::log::info!(
                    "Liftoff: thrust {:.0} N exceeds weight {:.0} N, released from surface",
                    thrust_n,
                    weight_n
                );
                continue;
            }
        }

        if rest.active {
            // Hold the vehicle on the surface. With deployed landing legs
            // the strut spring-damper carries the load (penalty-method soft
            // contact: compression is measured from actual hull penetration
            // each tick); the gear-less fallback is the historical rigid
            // point contact. Constraint authority stays here, in
            // GroundContact.
            match legs.as_mut().filter(|l| l.deployed()).map(|l| {
                let penetration_m = (-signed_altitude_m).max(0.0);
                (
                    l.gear.resolve_contact_step(
                        velocity,
                        normal,
                        penetration_m,
                        rocket.dynamics.mass_kg,
                        dt,
                    ),
                    l,
                )
            }) {
                Some((outcome, legs)) if outcome.bottomed_out => {
                    // Strut out of travel: fall back to the rigid clamp so
                    // the hull cannot tunnel (documented approximation —
                    // the body reference may sink by up to one stroke on
                    // soft contact).
                    bevy::log::warn!("Landing gear bottomed out; rigid contact engaged");
                    let res =
                        resolve_resting_contact(position_m, velocity, surface_radius_m, normal, dt);
                    rocket.dynamics.position_m = res.position_m;
                    rocket.dynamics.velocity_mps = res.velocity_mps;
                    legs.compression_m = legs.gear.spec.stroke_m;
                }
                Some((outcome, legs)) => {
                    // Soft contact: position rides the struts; only the
                    // velocity changes this step (impulse form).
                    rocket.dynamics.velocity_mps = outcome.velocity_mps;
                    legs.compression_m = outcome.compression_m;
                    scorecard.leg_compression_peak_m =
                        scorecard.leg_compression_peak_m.max(outcome.compression_m);
                }
                None => {
                    let res =
                        resolve_resting_contact(position_m, velocity, surface_radius_m, normal, dt);
                    rocket.dynamics.position_m = res.position_m;
                    rocket.dynamics.velocity_mps = res.velocity_mps;
                }
            }
            collision.ground_contact = GroundContact::Landed;
            continue;
        }

        // Not resting: penetration is still forbidden for any airborne or
        // crashed vehicle — push back to the surface and kill the
        // into-ground normal component so it cannot tunnel.
        if signed_altitude_m < 0.0 {
            let radial_dir = dir;
            rocket.dynamics.position_m = radial_dir * surface_radius_m;
            let into_ground = velocity.dot(normal).min(0.0);
            rocket.dynamics.velocity_mps = velocity - normal * into_ground;
        }

        // Airborne: a touchdown verdict exists only inside the contact band
        // while actually approaching the ground (receding fly-throughs are
        // not touchdowns).
        if signed_altitude_m > TOUCHDOWN_BAND_M || components.normal_mps > 0.0 {
            collision.ground_contact = GroundContact::None;
            continue;
        }

        let verdict = evaluate_touchdown(
            -components.normal_mps,
            components.lateral_mps,
            sample.slope_deg,
            tilt_deg,
            &criteria,
        );
        collision.ground_contact = verdict;

        match verdict {
            GroundContact::Landed => {
                // Engage rest immediately so this step already ends pinned to
                // the surface. Deployed legs absorb through the struts
                // (stroke-aware); the gear-less path snaps to point contact.
                rest.active = true;
                bevy::log::info!(
                    "Touchdown at ({lat:.2}, {lon:.2}): descent {:.2} m/s, lateral {:.2} m/s, slope {:.1} deg, tilt {:.1} deg{}",
                    -components.normal_mps,
                    components.lateral_mps,
                    sample.slope_deg,
                    tilt_deg,
                    if collision.over_water { " (water)" } else { "" }
                );
                record_scorecard(
                    scorecard,
                    -components.normal_mps,
                    components.lateral_mps,
                    tilt_deg,
                    sample.slope_deg,
                    position_m,
                    radius_m,
                    autopilot.target_landing_position_m,
                    collision.over_water,
                );
                match legs.as_ref().filter(|l| l.deployed()).map(|l| {
                    l.gear.absorbs_touchdown_energy(
                        rocket.dynamics.mass_kg,
                        (-components.normal_mps).max(0.0),
                    )
                }) {
                    // Gear path: penalty-method soft contact — no position
                    // snap here. The resting branch catches the hull as soon
                    // as it actually penetrates the surface and the struts
                    // absorb from there.
                    Some(true) => {}
                    Some(false) => bevy::log::warn!(
                        "Touchdown energy exceeds strut stroke capacity (descent {:.2} m/s)",
                        -components.normal_mps
                    ),
                    // Gear-less fallback: snap to point contact immediately.
                    None => {
                        let res = resolve_resting_contact(
                            position_m,
                            velocity,
                            surface_radius_m,
                            normal,
                            dt,
                        );
                        rocket.dynamics.position_m = res.position_m;
                        rocket.dynamics.velocity_mps = res.velocity_mps;
                    }
                }

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
                            touchdown_vertical_speed_mps: -components.normal_mps,
                        });
                        bevy::log::info!(
                            "Splashdown detected at ({lat:.2}, {lon:.2}), vertical speed {:.1} m/s",
                            -components.normal_mps
                        );
                    }
                }
            }
            GroundContact::Crash => {
                // A pad-hold at zero speed during pre-launch is not a crash.
                if *mission_state != RocketMissionState::PreLaunch {
                    *mission_state = RocketMissionState::Crashed;
                    record_scorecard(
                        scorecard,
                        -components.normal_mps,
                        components.lateral_mps,
                        tilt_deg,
                        sample.slope_deg,
                        position_m,
                        radius_m,
                        autopilot.target_landing_position_m,
                        collision.over_water,
                    );
                    // A crashed vehicle with a lean beyond its critical angle
                    // falls over visibly (Phase 14 lifecycle).
                    arm_topple_if_leaning(tip_over, legs.as_deref(), geometry, tilt_deg);
                }
            }
            GroundContact::None => {}
        }

        // Sustained-lean monitor while grounded (Phase 14): a landed vehicle
        // that keeps leaning past its critical angle topples under gravity.
        if rest.active && !tip_over.is_toppling() {
            let critical_rad = topple_critical_angle_rad(
                legs.as_deref()
                    .filter(|l| l.deployed())
                    .map(|l| l.gear.spec.base_radius_m)
                    .unwrap_or(geometry.radius_m as f64),
                com_height_on_ground(legs.as_deref(), geometry),
            );
            let lean_rad = tilt_deg.to_radians();
            if lean_rad > critical_rad && critical_rad > 0.0 {
                tip_over.exceeded_for_s += dt;
                if tip_over.exceeded_for_s >= TIP_OVER_SUSTAINED_S {
                    tip_over.com_height_m = com_height_on_ground(legs.as_deref(), geometry);
                    tip_over.fall = Some(ToppleFall::from_tilt(lean_rad));
                    bevy::log::info!(
                        "Vehicle leaning {:.1} deg beyond the {:.1} deg critical angle — toppling",
                        tilt_deg,
                        critical_rad.to_degrees()
                    );
                }
            } else {
                tip_over.exceeded_for_s = 0.0;
            }
        }
    }
}

/// How long a beyond-critical lean must persist before the fall is armed, s.
const TIP_OVER_SUSTAINED_S: f64 = 0.5;

/// Center-of-mass height above the foot plane while grounded: on deployed
/// struts the ride height includes the stroke; bare-hull contact puts the
/// geometric center at hull half-height.
fn com_height_on_ground(legs: Option<&LandingLegs>, geometry: &RocketGeometry) -> f64 {
    match legs.filter(|l| l.deployed()) {
        Some(l) => l.gear.com_height_on_gear_m(geometry.height_m as f64),
        None => geometry.height_m as f64 / 2.0,
    }
}

/// Fill the one-shot landing scorecard at a verdict tick (GroundContact is
/// the only writer).
#[allow(clippy::too_many_arguments)]
fn record_scorecard(
    scorecard: &mut LandingScorecard,
    descent_speed_mps: f64,
    lateral_speed_mps: f64,
    tilt_deg: f64,
    slope_deg: f64,
    position_m: DVec3,
    planet_radius_m: f64,
    target_position_m: DVec3,
    over_water: bool,
) {
    // Surface distance between the sub-vehicle point and the configured
    // landing target (chord approximation — targets are local).
    let sub_point = position_m.normalize_or_zero() * planet_radius_m;
    let distance_to_target_m = if target_position_m.length_squared() > 1.0 {
        (sub_point - target_position_m.normalize_or_zero() * planet_radius_m).length()
    } else {
        0.0
    };
    *scorecard = LandingScorecard {
        touchdown_vertical_speed_mps: descent_speed_mps,
        touchdown_lateral_speed_mps: lateral_speed_mps,
        touchdown_tilt_deg: tilt_deg,
        touchdown_slope_deg: slope_deg,
        distance_to_target_m,
        leg_compression_peak_m: scorecard.leg_compression_peak_m,
        over_water,
        recorded: true,
    };
}

/// Arm the gravity-driven topple immediately when a crashed vehicle leans
/// beyond its critical angle (no sustained window — the verdict already
/// decided it).
fn arm_topple_if_leaning(
    tip_over: &mut TipOverState,
    legs: Option<&LandingLegs>,
    geometry: &RocketGeometry,
    tilt_deg: f64,
) -> bool {
    if tip_over.is_toppling() {
        return false;
    }
    let critical_rad = topple_critical_angle_rad(
        legs.filter(|l| l.deployed())
            .map(|l| l.gear.spec.base_radius_m)
            .unwrap_or(geometry.radius_m as f64),
        com_height_on_ground(legs, geometry),
    );
    let lean_rad = tilt_deg.to_radians();
    if critical_rad > 0.0 && lean_rad > critical_rad {
        tip_over.com_height_m = com_height_on_ground(legs, geometry);
        tip_over.fall = Some(ToppleFall::from_tilt(lean_rad));
        return true;
    }
    false
}

/// Advance an armed topple (Phase 14): rigid rotation about the foot-plane
/// edge driven by the domain gravity-pendulum model, with the tilt mapped
/// onto the authoritative orientation. Runs inside [`RocketSet::GroundContact`]
/// after `resolve_ground_contact`; ends with mission Crashed.
pub fn advance_topple(
    sim_time: Res<SimulationTime>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &mut RocketPhysicsState,
        &mut TipOverState,
        &mut RocketMissionState,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (binding, mut rocket, mut tip_over, mut mission_state) in rocket_query.iter_mut() {
        // Guard: only armed vehicles participate.
        if tip_over.fall.is_none() {
            continue;
        }
        let com_height_m = tip_over.com_height_m;
        let Some(planet) = planet_query
            .iter()
            .find(|planet| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };
        let radius = rocket.dynamics.position_m.length();
        if radius < 1.0 {
            continue;
        }
        let up = rocket.dynamics.position_m / radius;
        let body_y = rocket.dynamics.orientation * DVec3::Y;

        // Fixed fall plane: horizontal lean direction captured from the
        // current attitude; upright bodies have none and wait.
        let fall_dir_h = (body_y - up * body_y.dot(up)).normalize_or_zero();
        if fall_dir_h.length_squared() < 0.5 {
            // Upright: nothing to map yet.
            continue;
        }

        let gravity_mps2 =
            gravitational_parameter(planet.domain_planet.mass_kg) / (radius * radius);
        let fall = tip_over.fall.as_mut().expect("armed above");
        let completed = fall.advance(gravity_mps2, com_height_m, dt);

        // Rebuild the attitude with the longitudinal axis at the model's
        // tilt, preserving as much of the original roll as possible.
        let y_new = up * fall.tilt_rad.cos() + fall_dir_h * fall.tilt_rad.sin();
        let x_old = body_y.cross(y_new).cross(body_y).normalize_or_zero();
        let x_new = if x_old.length_squared() > 0.5 {
            x_old
        } else {
            fall_dir_h.cross(up).normalize_or_zero()
        };
        if x_new.length_squared() < 0.5 {
            continue;
        }
        let z_new = x_new.cross(y_new);
        rocket.dynamics.orientation = DQuat::from_mat3(&DMat3::from_cols(
            x_new.normalize(),
            y_new,
            z_new.normalize(),
        ));

        if completed && *mission_state != RocketMissionState::Crashed {
            *mission_state = RocketMissionState::Crashed;
            bevy::log::info!("Vehicle toppled over — mission lost");
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
            .find(|planet| planet.domain_planet.name == binding.planet_name)
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
            legs.deployment = crate::domain::services::landing_gear::LegDeploymentState::default();
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

#[cfg(test)]
mod ground_contact_tests {
    use super::*;
    use crate::domain::entities::rocket::{EngineState, Rocket, RocketEngine, RocketStage};
    use crate::domain::services::landing_gear::{LandingGear, LandingGearSpec};
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::terrain_collision::{radial_direction, sample_surface};
    use bevy::math::{DMat3, DQuat};
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
                planet_name: "Earth".to_string(),
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
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::terrain_collision::radial_direction;
    use bevy::math::{DQuat, DVec3};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    const DT: f64 = 1.0 / 64.0;

    /// Electron-class vehicle: nine 25.8 kN engines with a 0.6 throttle
    /// floor (stage envelope [0.6, 1.0]), ~13 t gross.
    fn electron_like() -> Rocket {
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

    fn ascent_app() -> App {
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
                RocketMissionState::PreLaunch,
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
                planet_name: "Earth".to_string(),
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
                update_rocket_gravity,
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
        let mut q = world.query::<(&RocketPropulsion, &GroundRest)>();
        let (propulsion, _rest) = q.single(world).unwrap();
        let Some(stage) = propulsion.vehicle.stages.first() else {
            panic!("stage 1 missing");
        };
        let (thrust_body, _) = stage_thrust_body(&stage.engines, propulsion.throttle, 0.0);
        assert!(
            (thrust_body.length() - 232_200.0).abs() < 500.0,
            "steady-state thrust {} N is not the expected ~232 kN",
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

        // Run well past the time-schedule start (t = 10 s): with the vehicle
        // high and fast the gate is clear and the combined schedule must
        // have produced a real pitch-over by t ≈ 20 s.
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
}
