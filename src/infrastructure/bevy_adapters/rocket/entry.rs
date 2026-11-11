use super::components::{
    AblationState, CommsState, ForceAccumulator, GroundRest, LandingLegs, ParachuteState,
    RetroPropulsionEffect, RocketFlightConditions, RocketGeometry, RocketMissionState,
    RocketPhysicsState, RocketPropulsion, TerrainCollisionState, ThermalState,
};
use super::events::CommsBlackoutEvent;
use crate::domain::services::entry_physics::{
    capped_tps_recession_m, comms_blackout_active, convective_heat_flux_w_m2, electron_density_m3,
    radiative_heat_flux_w_m2, retro_propulsion_effectiveness,
    retro_propulsion_plume_faces_freestream, tps_mass_loss_kg, tps_recession_rate_mps,
};
use crate::domain::services::rocket_propulsion::stage_thrust_body;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_collision::GroundContact;
use crate::infrastructure::bevy_adapters::entity_components::EntryPhysicsConfig;
use bevy::prelude::{Entity, MessageWriter, Query, Res};

/// Convective heating (Sutton-Graves) and radiative heating (Tauber-Sutton).
/// Runs in FixedUpdate before force accumulation. Reads RocketFlightConditions and
/// writes ThermalState for the ablation system.
pub fn compute_heating(
    config: Res<EntryPhysicsConfig>,
    mut rocket_query: Query<(&RocketFlightConditions, &AblationState, &mut ThermalState)>,
) {
    for (conditions, ablation, mut thermal) in rocket_query.iter_mut() {
        let rho = conditions.density_kg_m3;
        let v = conditions.airspeed_mps;

        // Skip if no meaningful atmosphere
        if rho <= 0.0 || v < 100.0 {
            thermal.convective_heat_flux_w_m2 = 0.0;
            thermal.radiative_heat_flux_w_m2 = 0.0;
            thermal.total_heat_flux_w_m2 = 0.0;
            continue;
        }

        // Convective heating: Sutton-Graves q_dot = k * sqrt(rho/R_nose) * v^3
        // (single authority: domain::services::entry_physics, AGENTS.md 50).
        let nose_radius = ablation.nose_radius_m.max(config.nose_radius_initial_m);
        let q_conv = convective_heat_flux_w_m2(config.convective_coefficient, rho, nose_radius, v);
        thermal.convective_heat_flux_w_m2 = q_conv;

        // Radiative heating: Tauber-Sutton (significant for v > 10 km/s).
        let q_rad = radiative_heat_flux_w_m2(config.radiative_coefficient, rho, v);
        thermal.radiative_heat_flux_w_m2 = q_rad;

        thermal.total_heat_flux_w_m2 = q_conv + q_rad;
        thermal.stagnation_point_heat_flux_w_m2 = q_conv; // Stagnation point = convective peak
    }
}

/// Establishes the configured physical TPS state once after spawn or relaunch.
/// A default component deliberately carries no body-specific constants.
pub fn initialize_thermal_protection(
    config: Res<EntryPhysicsConfig>,
    mut rocket_query: Query<&mut AblationState>,
) {
    for mut ablation in rocket_query.iter_mut() {
        if ablation.nose_radius_m > 0.0 || ablation.tps_exhausted {
            continue;
        }
        ablation.nose_radius_m = config.nose_radius_initial_m;
        ablation.tps_thickness_remaining_m = config.tps_initial_thickness_m;
    }
}

/// Ablation: char-layer recession from integrated heat load.
/// Updates nose radius and mass loss in AblationState.
/// Uses SimulationTime fixed timestep.
pub fn compute_ablation(
    sim_time: Res<SimulationTime>,
    config: Res<EntryPhysicsConfig>,
    mut rocket_query: Query<(
        &mut RocketPhysicsState,
        &RocketGeometry,
        &RocketPropulsion,
        &ThermalState,
        &mut AblationState,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (mut rocket, geometry, propulsion, thermal, mut ablation) in rocket_query.iter_mut() {
        let q_total = thermal.total_heat_flux_w_m2;
        if q_total <= 0.0 {
            continue;
        }

        // Integrated heat load
        ablation.cumulative_heat_load_j_m2 += q_total * dt;

        // Recession rate: dr/dt = q_dot / (rho_tps * H_abl)
        if ablation.tps_exhausted {
            continue;
        }
        let recession_rate = tps_recession_rate_mps(
            q_total,
            config.tps_density_kg_m3,
            config.heat_of_ablation_j_kg,
        );
        let recession_m =
            capped_tps_recession_m(recession_rate, dt, ablation.tps_thickness_remaining_m);
        if recession_m <= 0.0 {
            ablation.tps_exhausted = true;
            continue;
        }

        // Nose radius growth from recession
        let nose_radius_before_m = ablation.nose_radius_m.max(config.nose_radius_initial_m);
        ablation.recession_depth_m += recession_m;
        ablation.nose_radius_m = nose_radius_before_m + recession_m;

        // Mass loss from the TPS cap over the actual bounded recession.
        ablation.mass_loss_kg +=
            tps_mass_loss_kg(config.tps_density_kg_m3, nose_radius_before_m, recession_m);
        ablation.tps_thickness_remaining_m =
            (ablation.tps_thickness_remaining_m - recession_m).max(0.0);
        ablation.tps_exhausted = ablation.tps_thickness_remaining_m <= 0.0;

        // Rebuild all rigid-body quantities from the same attached mass
        // inventory. Incremental subtraction left COM/inertia stale.
        rocket.refresh_attached_mass_properties(propulsion, *geometry, ablation.mass_loss_kg);
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
    mut rocket_query: Query<(Entity, &RocketFlightConditions, &mut CommsState)>,
) {
    for (rocket_entity, conditions, mut comms) in rocket_query.iter_mut() {
        let electron_density =
            electron_density_m3(conditions.density_kg_m3, conditions.airspeed_mps);
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
#[expect(
    clippy::type_complexity,
    reason = "The query groups the state this single force writer reads, including the landing-gear marker that disables chutes."
)]
pub fn compute_parachute_forces(
    sim_time: Res<SimulationTime>,
    config: Res<EntryPhysicsConfig>,
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketFlightConditions,
        &TerrainCollisionState,
        &GroundRest,
        &RocketMissionState,
        Option<&LandingLegs>,
        &mut ParachuteState,
        &mut ForceAccumulator,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    let parachute_config = config.parachute_config();
    for (
        rocket,
        conditions,
        collision,
        ground_rest,
        mission_state,
        landing_legs,
        mut parachute,
        mut force_accum,
    ) in rocket_query.iter_mut()
    {
        // Deployable landing gear is the marker for a propulsively recovered
        // vehicle. A booster that lands on its own engine is not fitted with a
        // drogue/main parachute: the canopy state machine would otherwise
        // deploy on every descending Earth vehicle because the parachute model
        // is per-body, not per-vehicle. The configured recovery path owns the
        // descent; this system must not add a second recovery mechanism.
        if landing_legs.is_some() {
            continue;
        }

        // Parachutes only deploy during descent/landing phases, not on the pad.
        if *mission_state == RocketMissionState::PreLaunch
            || *mission_state == RocketMissionState::Launch
            || *mission_state == RocketMissionState::Ascent
            || *mission_state == RocketMissionState::Orbit
            || ground_rest.active
            || collision.ground_contact == GroundContact::Landed
        {
            continue;
        }

        let rho = conditions.density_kg_m3;
        let velocity = conditions.atmosphere_relative_velocity_mps;
        let speed = conditions.airspeed_mps;
        if rho <= 0.0 || speed <= 0.0 {
            continue;
        }

        let up_dir = rocket.dynamics.position_m.normalize_or_zero();
        if up_dir.length_squared() < 1e-12 {
            continue;
        }
        let vertical_speed = velocity.dot(up_dir);

        let transitions = parachute.deployment.advance(
            &parachute_config,
            conditions.altitude_m,
            conditions.mach_number,
            vertical_speed,
            dt,
        );
        if transitions.any() {
            bevy::log::info!(
                "Parachute transition at {:.0} m: drogue_deployed={} drogue_inflated={} main_deployed={} main_inflated={}",
                conditions.altitude_m,
                transitions.drogue_deployed,
                transitions.drogue_inflated,
                transitions.main_deployed,
                transitions.main_inflated,
            );
        }

        // Apply combined canopy drag opposite the velocity.
        let drag_magnitude = parachute.deployment.drag_force_n(rho, speed);
        if drag_magnitude > 0.0 {
            force_accum.add_force_n((-velocity / speed) * drag_magnitude);
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
        &RocketFlightConditions,
        &RocketPropulsion,
        &mut RetroPropulsionEffect,
    )>,
) {
    for (rocket, conditions, propulsion, mut retro) in rocket_query.iter_mut() {
        // Default each tick; re-derived below so state never goes stale
        // (config toggles, Mach drops below threshold, engines shut down).
        let mut multiplier = 1.0;

        if config.retro_propulsion_enabled {
            let mach = conditions.mach_number;
            if mach >= config.retro_propulsion_mach_threshold {
                // The shared running-stage capability applies throttle, engine
                // lifecycle, synchronized inventory, and recovery reserve.
                if let Some((active_core_stage, throttle)) = propulsion.running_core_stage() {
                    let (thrust_body, _) = stage_thrust_body(
                        &active_core_stage.stage().engines,
                        throttle,
                        conditions.ambient_pressure_pa,
                    );
                    // The base-pressure correlation only exists when the plume
                    // faces the oncoming flow. A powered ascent thrusts along
                    // its velocity and must keep its full rated thrust.
                    let thrust_inertial = rocket.dynamics.orientation * thrust_body;
                    if retro_propulsion_plume_faces_freestream(
                        thrust_inertial,
                        conditions.atmosphere_relative_velocity_mps,
                    ) {
                        multiplier = retro_propulsion_effectiveness(
                            mach,
                            config.retro_propulsion_mach_threshold,
                            config.base_pressure_coefficient,
                        );
                    }
                }
            }
        }

        retro.thrust_multiplier = multiplier;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::math::{DMat3, DQuat, DVec3};
    use crate::domain::services::atmosphere::FlightConditions;
    use crate::domain::services::landing_gear::{LandingGear, LandingGearSpec};
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::simulation_time::SimulationTime;
    use crate::infrastructure::bevy_adapters::rocket::components::BoosterAttachmentState;
    use bevy::prelude::{App, Update};

    /// A supersonic, full-throttle vehicle sample whose air-relative velocity
    /// points along `velocity_mps` in the inertial frame. Identity attitude
    /// puts body +Y on inertial +Y.
    fn supersonic_app(velocity_mps: DVec3) -> (App, Entity) {
        let mut app = App::new();
        app.insert_resource(EntryPhysicsConfig::default());
        app.add_systems(Update, compute_retro_propulsion);

        let vehicle = crate::domain::entities::rocket::Rocket::falcon9_test_fixture();
        let propellant = vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        let conditions = FlightConditions {
            altitude_m: 11_000.0,
            temperature_k: 216.65,
            ambient_pressure_pa: 22_632.0,
            density_kg_m3: 0.3639,
            speed_of_sound_mps: 295.0,
            atmosphere_relative_velocity_mps: velocity_mps,
            airspeed_mps: velocity_mps.length(),
            mach_number: velocity_mps.length() / 295.0,
            dynamic_pressure_pa: 0.5 * 0.3639 * velocity_mps.length_squared(),
        };

        let entity = app
            .world_mut()
            .spawn((
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        DVec3::new(0.0, 6.371e6, 0.0),
                        velocity_mps,
                        DQuat::IDENTITY,
                        549_000.0,
                        DMat3::IDENTITY,
                        DVec3::ZERO,
                    ),
                },
                RocketFlightConditions::from_sample(conditions),
                RocketPropulsion {
                    vehicle,
                    active_stage: 0,
                    propellant_remaining_kg: propellant,
                    booster_attachment: BoosterAttachmentState::Detached,
                    throttle: 1.0,
                    gimbal_pitch_rad: 0.0,
                    gimbal_yaw_rad: 0.0,
                    time_since_separation_s: 10.0,
                    ullage_settle_time_s: 0.0,
                    separations_count: 0,
                    attached_payload_kg: 0.0,
                },
                RetroPropulsionEffect::default(),
            ))
            .id();

        (app, entity)
    }

    fn multiplier(app: &mut App, entity: Entity) -> f64 {
        app.update();
        app.world()
            .get::<RetroPropulsionEffect>(entity)
            .expect("retro effect")
            .thrust_multiplier
    }

    #[test]
    fn powered_ascent_keeps_full_thrust_above_the_mach_threshold() {
        // Thrust along the velocity is not a retro-propulsion interaction. The
        // pre-gate code reduced ascent thrust by the base-pressure correlation
        // purely because Mach exceeded 1.2.
        let velocity = DVec3::new(0.0, 900.0, 0.0);
        let (mut app, entity) = supersonic_app(velocity);
        assert_eq!(multiplier(&mut app, entity), 1.0);
    }

    #[test]
    fn retro_burn_applies_the_base_pressure_correlation() {
        // Moving up while thrusting down: the plume faces the freestream.
        let velocity = DVec3::new(0.0, -900.0, 0.0);
        let (mut app, entity) = supersonic_app(velocity);
        let applied = multiplier(&mut app, entity);
        let expected = retro_propulsion_effectiveness(900.0 / 295.0, 1.2, 0.1);
        assert!(
            (applied - expected).abs() < 1e-12,
            "retro burn must apply the correlation: {applied} vs {expected}"
        );
        assert!(applied < 1.0);
    }

    #[test]
    fn subsonic_flight_keeps_full_thrust() {
        let velocity = DVec3::new(0.0, -200.0, 0.0);
        let (mut app, entity) = supersonic_app(velocity);
        assert_eq!(multiplier(&mut app, entity), 1.0);
    }

    /// A descending vehicle fitted with deployable landing gear is recovering
    /// propulsively and must not also deploy parachutes. The same vehicle
    /// without gear still uses the per-body chute model.
    #[test]
    fn parachutes_skip_landing_gear_vehicles() {
        fn descending_vehicle(app: &mut App, with_gear: bool) -> Entity {
            let velocity_mps = DVec3::new(0.0, -120.0, 0.0);
            let conditions = FlightConditions {
                altitude_m: 4_000.0,
                temperature_k: 262.0,
                ambient_pressure_pa: 61_660.0,
                density_kg_m3: 0.8194,
                speed_of_sound_mps: 324.0,
                atmosphere_relative_velocity_mps: velocity_mps,
                airspeed_mps: velocity_mps.length(),
                mach_number: velocity_mps.length() / 324.0,
                dynamic_pressure_pa: 0.5 * 0.8194 * velocity_mps.length_squared(),
            };
            let entity = app
                .world_mut()
                .spawn((
                    RocketPhysicsState {
                        dynamics: RocketDynamicsState::new(
                            DVec3::new(0.0, 6.371e6 + 4_000.0, 0.0),
                            velocity_mps,
                            DQuat::IDENTITY,
                            25_600.0,
                            DMat3::IDENTITY,
                            DVec3::ZERO,
                        ),
                    },
                    RocketFlightConditions::from_sample(conditions),
                    TerrainCollisionState::default(),
                    GroundRest::default(),
                    RocketMissionState::UnpoweredDescent,
                    ParachuteState::default(),
                    ForceAccumulator::default(),
                ))
                .id();
            if with_gear {
                app.world_mut()
                    .entity_mut(entity)
                    .insert(LandingLegs::new(LandingGear::new(
                        LandingGearSpec {
                            count: 4,
                            base_radius_m: 5.0,
                            stroke_m: 0.8,
                            max_landing_mass_kg: Some(30_000.0),
                            deploy_altitude_m: 2_000.0,
                        },
                        25_600.0,
                    )));
            }
            entity
        }

        let mut app = App::new();
        app.insert_resource(EntryPhysicsConfig::default());
        app.insert_resource(SimulationTime::new(1.0 / 64.0));
        app.add_systems(Update, compute_parachute_forces);

        let geared = descending_vehicle(&mut app, true);
        let ungreared = descending_vehicle(&mut app, false);
        app.update();

        let geared_parachute = app.world().get::<ParachuteState>(geared).unwrap();
        assert!(
            !geared_parachute.deployment.drogue_deployed,
            "a propulsively recovered vehicle must not deploy a drogue"
        );
        let geared_force = app
            .world()
            .get::<ForceAccumulator>(geared)
            .unwrap()
            .force_n();
        assert_eq!(geared_force, DVec3::ZERO);

        let ungeared_parachute = app.world().get::<ParachuteState>(ungreared).unwrap();
        assert!(
            ungeared_parachute.deployment.drogue_deployed,
            "a gearless descending vehicle still uses the chute model"
        );
        let ungeared_force = app
            .world()
            .get::<ForceAccumulator>(ungreared)
            .unwrap()
            .force_n();
        assert!(
            ungeared_force.length() > 0.0,
            "the deployed drogue must add drag"
        );
    }
}
