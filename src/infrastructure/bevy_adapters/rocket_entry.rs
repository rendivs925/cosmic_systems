use crate::components::rocket::{
    AblationState, CommsState, ForceAccumulator, GroundRest, ParachuteState, RetroPropulsionEffect,
    RocketFlightConditions, RocketMissionState, RocketPhysicsState, RocketPropulsion,
    TerrainCollisionState, ThermalState,
};
use crate::domain::events::CommsBlackoutEvent;
use crate::domain::services::entry_physics::{
    capped_tps_recession_m, comms_blackout_active, convective_heat_flux_w_m2, electron_density_m3,
    radiative_heat_flux_w_m2, retro_propulsion_effectiveness, tps_mass_loss_kg,
    tps_recession_rate_mps,
};
use crate::domain::services::rocket_propulsion::stage_thrust_body;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_collision::GroundContact;
use crate::infrastructure::bevy_adapters::components::EntryPhysicsConfig;
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
        &crate::components::rocket::RocketGeometry,
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
pub fn compute_parachute_forces(
    sim_time: Res<SimulationTime>,
    config: Res<EntryPhysicsConfig>,
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketFlightConditions,
        &TerrainCollisionState,
        &GroundRest,
        &RocketMissionState,
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
        mut parachute,
        mut force_accum,
    ) in rocket_query.iter_mut()
    {
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
        &RocketFlightConditions,
        &RocketPropulsion,
        &mut RetroPropulsionEffect,
    )>,
) {
    for (conditions, propulsion, mut retro) in rocket_query.iter_mut() {
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

        retro.thrust_multiplier = multiplier;
    }
}
