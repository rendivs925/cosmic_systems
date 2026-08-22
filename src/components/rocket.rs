// Granular rocket components for ECS query isolation.
// Each component has a single responsibility, enabling parallel system execution.

use crate::domain::entities::rocket::Rocket;
use crate::domain::services::rocket_dynamics::RocketDynamicsState;
use bevy::math::{DQuat, DVec3, Quat, Vec3};
use bevy::prelude::*;

/// Authoritative 6-DOF physics state in planet-centered inertial frame (f64).
/// Single source of truth for rocket motion. Only `integrate_6dof` writes this.
#[derive(Component, Debug, Clone)]
pub struct RocketPhysicsState {
    pub dynamics: RocketDynamicsState,
}

/// Static vehicle geometry (immutable after spawn).
#[derive(Component, Debug, Clone, Copy)]
pub struct RocketGeometry {
    pub radius_m: f32,
    pub height_m: f32,
}

/// Current vehicle mass (f64 for physics, derived from PhysicsState).
/// Updated by propulsion_consumption, propulsion_staging.
/// Read by gravity, integration, control systems.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct RocketMass(pub f64);

/// Mission phase state machine. Updated by guidance_system, terrain_interaction.
/// Read by control, actuation, propulsion, guidance systems.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RocketMissionState {
    #[default]
    PreLaunch,
    Launch,
    Ascent,
    Orbit,
    DeorbitBurn,
    ReentryCorridor,
    PoweredDescent,
    UnpoweredDescent,
    Landing,
    Landed,
    Crashed,
}

impl From<crate::domain::entities::rocket::RocketMissionState> for RocketMissionState {
    fn from(state: crate::domain::entities::rocket::RocketMissionState) -> Self {
        match state {
            crate::domain::entities::rocket::RocketMissionState::PreLaunch => Self::PreLaunch,
            crate::domain::entities::rocket::RocketMissionState::Launch => Self::Launch,
            crate::domain::entities::rocket::RocketMissionState::Ascent => Self::Ascent,
            crate::domain::entities::rocket::RocketMissionState::Orbit => Self::Orbit,
            crate::domain::entities::rocket::RocketMissionState::DeorbitBurn => Self::DeorbitBurn,
            crate::domain::entities::rocket::RocketMissionState::ReentryCorridor => {
                Self::ReentryCorridor
            }
            crate::domain::entities::rocket::RocketMissionState::PoweredDescent => {
                Self::PoweredDescent
            }
            crate::domain::entities::rocket::RocketMissionState::UnpoweredDescent => {
                Self::UnpoweredDescent
            }
            crate::domain::entities::rocket::RocketMissionState::Landing => Self::Landing,
            crate::domain::entities::rocket::RocketMissionState::Landed => Self::Landed,
            crate::domain::entities::rocket::RocketMissionState::Crashed => Self::Crashed,
        }
    }
}

impl From<RocketMissionState> for crate::domain::entities::rocket::RocketMissionState {
    fn from(state: RocketMissionState) -> Self {
        match state {
            RocketMissionState::PreLaunch => Self::PreLaunch,
            RocketMissionState::Launch => Self::Launch,
            RocketMissionState::Ascent => Self::Ascent,
            RocketMissionState::Orbit => Self::Orbit,
            RocketMissionState::DeorbitBurn => Self::DeorbitBurn,
            RocketMissionState::ReentryCorridor => Self::ReentryCorridor,
            RocketMissionState::PoweredDescent => Self::PoweredDescent,
            RocketMissionState::UnpoweredDescent => Self::UnpoweredDescent,
            RocketMissionState::Landing => Self::Landing,
            RocketMissionState::Landed => Self::Landed,
            RocketMissionState::Crashed => Self::Crashed,
        }
    }
}

/// Runtime propulsion state: vehicle definition, active stage, throttle, gimbal.
/// Updated by actuation_system, propulsion_consumption, propulsion_staging.
#[derive(Component, Debug, Clone)]
pub struct RocketPropulsion {
    pub vehicle: Rocket,
    pub active_stage: usize,
    pub propellant_remaining_kg: Vec<f32>,
    pub throttle: f32,
    pub gimbal_pitch_rad: f32,
    pub gimbal_yaw_rad: f32,
}

/// Flight computer command interface between guidance, control, actuation, physics.
/// Guidance writes target_attitude; Control writes throttle/gimbal/RCS; Actuation clamps.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct RocketCommands {
    pub target_attitude: DQuat,
    pub throttle_cmd: f32,
    pub gimbal_pitch_cmd_rad: f32,
    pub gimbal_yaw_cmd_rad: f32,
    pub rcs_torque_cmd_body: DVec3,
}

/// Autopilot configuration: PID gains, ascent profile, actuator limits, and mode.
#[derive(Component, Debug, Clone, Default)]
pub struct RocketAutopilot {
    pub gains: crate::domain::services::control::PidGains,
    pub integral: DVec3,
    pub ascent_profile: crate::domain::services::guidance::AscentGuidanceProfile,
    pub actuation: crate::domain::services::actuation::ActuationLimits,
    pub mode: crate::domain::services::guidance::AutopilotMode,
    pub time_since_liftoff_s: f64,
    pub target_landing_position_m: DVec3,
}

/// Net force accumulator (world/planet-inertial frame), cleared each frame by integrate_6dof.
/// Written by: gravity, aero, propulsion, parachutes, retro-propulsion.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ForceAccumulator(pub DVec3);

/// Net torque accumulator (body frame), cleared each frame by integrate_6dof.
/// Written by: aero torque, gimbal, RCS, gimbal actuation.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TorqueAccumulator(pub DVec3);

/// Binds rocket to its dominant gravity body.
#[derive(Component, Debug, Clone)]
pub struct RocketPlanetBinding {
    pub planet_name: String,
}

/// Authoritative gravitational acceleration (m/s²) acting on vehicle.
/// Computed by update_rocket_gravity, consumed by accumulate_forces.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct GravityAcceleration {
    pub value: DVec3,
}

/// Cached atmosphere state at vehicle altitude.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct AtmosphereState {
    pub altitude_m: f64,
    pub temperature_k: f64,
    pub pressure_pa: f64,
    pub density_kg_m3: f64,
    pub speed_of_sound_mps: f64,
}

/// Aerodynamic force in body frame, computed by aerodynamic_forces.
/// Consumed by aerodynamic_torque for torque calculation.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct AerodynamicForces {
    pub force_body: DVec3,
    pub center_of_pressure_body: DVec3,
}

/// Running maximum dynamic pressure (Max Q) reached during flight.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct MaxQTracker {
    pub max_q_pa: f64,
}

/// f32 facade fields for rendering/compatibility, synced from authoritative state.
/// Only sync_render_transform writes this.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct RocketFacade {
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: Quat,
    pub angular_velocity: Vec3,
    pub mass: f32,
    pub dry_mass_kg: f32,
    pub fuel_mass: f32,
    pub thrust: Vec3,
}

/// Thermal state from entry physics.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ThermalState {
    pub convective_heat_flux_w_m2: f64,
    pub radiative_heat_flux_w_m2: f64,
    pub total_heat_flux_w_m2: f64,
    pub wall_temperature_k: f64,
    pub stagnation_point_heat_flux_w_m2: f64,
}

/// Ablation state tracking TPS recession and mass loss.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct AblationState {
    pub cumulative_heat_load_j_m2: f64,
    pub recession_depth_m: f64,
    pub nose_radius_m: f64,
    pub mass_loss_kg: f64,
    pub tps_thickness_remaining_m: f64,
}

/// Parachute deployment state.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ParachuteState {
    pub drogue_deployed: bool,
    pub drogue_reefed: bool,
    pub drogue_fully_inflated: bool,
    pub drogue_timer_s: f64,
    pub main_deployed: bool,
    pub main_reefed: bool,
    pub main_fully_inflated: bool,
    pub main_timer_s: f64,
    pub canopy_attach_point_body: DVec3,
    pub current_cd: f64,
    pub reference_area_m2: f64,
}

/// Cached terrain collision state.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TerrainCollisionState {
    pub radar_altitude_m: f64,
    pub slope_deg: f64,
    pub ground_contact: crate::domain::services::terrain_collision::GroundContact,
}

/// Orbital elements computed from rocket state vectors (planet-centered inertial frame).
/// Updated by orbital_elements_system for telemetry and guidance.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct OrbitalElements {
    pub semi_major_axis_m: f64,
    pub eccentricity: f64,
    pub inclination_rad: f64,
    pub longitude_ascending_node_rad: f64,
    pub argument_of_periapsis_rad: f64,
    pub true_anomaly_rad: f64,
    pub mean_anomaly_rad: f64,
    pub orbital_period_s: f64,
    pub apoapsis_m: f64,
    pub periapsis_m: f64,
}
