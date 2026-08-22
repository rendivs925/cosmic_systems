// Granular rocket components for ECS query isolation.
// Each component has a single responsibility, enabling parallel system execution.

use crate::domain::entities::rocket::{Rocket, RocketMissionState as DomainRocketMissionState};
use crate::domain::services::rocket_dynamics::RocketDynamicsState;
use crate::domain::services::terrain_collision::GroundContact;
use bevy::math::{DQuat, DVec3, Quat, Vec3};
use bevy::prelude::*;
use std::ops::Deref;

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

/// Mission phase state machine component wrapping the domain enum.
/// Updated by guidance_system, terrain_interaction.
/// Read by control, actuation, propulsion, guidance systems.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RocketMissionState(pub DomainRocketMissionState);

impl RocketMissionState {
    pub const PreLaunch: Self = Self(DomainRocketMissionState::PreLaunch);
    pub const Launch: Self = Self(DomainRocketMissionState::Launch);
    pub const Ascent: Self = Self(DomainRocketMissionState::Ascent);
    pub const Orbit: Self = Self(DomainRocketMissionState::Orbit);
    pub const DeorbitBurn: Self = Self(DomainRocketMissionState::DeorbitBurn);
    pub const ReentryCorridor: Self = Self(DomainRocketMissionState::ReentryCorridor);
    pub const PoweredDescent: Self = Self(DomainRocketMissionState::PoweredDescent);
    pub const UnpoweredDescent: Self = Self(DomainRocketMissionState::UnpoweredDescent);
    pub const Landing: Self = Self(DomainRocketMissionState::Landing);
    pub const Landed: Self = Self(DomainRocketMissionState::Landed);
    pub const Crashed: Self = Self(DomainRocketMissionState::Crashed);
}

impl From<DomainRocketMissionState> for RocketMissionState {
    fn from(state: DomainRocketMissionState) -> Self {
        Self(state)
    }
}

impl From<RocketMissionState> for DomainRocketMissionState {
    fn from(state: RocketMissionState) -> Self {
        state.0
    }
}

impl Deref for RocketMissionState {
    type Target = DomainRocketMissionState;

    fn deref(&self) -> &Self::Target {
        &self.0
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

/// Comms link state driven by plasma blackout edge detection.
/// Written by compute_plasma_blackout; read by telemetry/HUD.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CommsState {
    pub in_blackout: bool,
}

/// Thrust effectiveness multiplier from supersonic retro-propulsion
/// (DLR base-pressure correlation). Computed in EntryPhysics, consumed by
/// propulsion_thrust so there is exactly one thrust writer.
#[derive(Component, Debug, Clone, Copy)]
pub struct RetroPropulsionEffect {
    pub thrust_multiplier: f64,
}

impl Default for RetroPropulsionEffect {
    fn default() -> Self {
        // No retro effect until computed: full effectiveness.
        Self {
            thrust_multiplier: 1.0,
        }
    }
}

/// Parachute deployment state. Wraps the pure domain state machine
/// (`domain::services::entry_physics::ParachuteDeploymentState`) so the
/// transition logic stays Bevy-free and unit-testable.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ParachuteState {
    pub deployment: crate::domain::services::entry_physics::ParachuteDeploymentState,
    pub canopy_attach_point_body: DVec3,
}

impl std::ops::Deref for ParachuteState {
    type Target = crate::domain::services::entry_physics::ParachuteDeploymentState;

    fn deref(&self) -> &Self::Target {
        &self.deployment
    }
}

/// Cached terrain collision state.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TerrainCollisionState {
    pub radar_altitude_m: f64,
    pub slope_deg: f64,
    pub ground_contact: crate::domain::services::terrain_collision::GroundContact,
    /// True when the surface below is inferred to be water (Earth only;
    /// no ocean mask exists — water ≈ terrain at mean sea level).
    pub over_water: bool,
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

/// Aggregated rocket telemetry for HUD, flight log, and external consumers.
/// Computed from authoritative state in `compute_rocket_telemetry` (FixedUpdate).
/// All units are explicit per AGENTS.md section 15.
#[derive(Resource, Debug, Clone, Default)]
pub struct RocketTelemetry {
    /// Altitude above ground level (AGL) in meters.
    pub altitude_agl_m: f64,
    /// Altitude above mean sea level (MSL) in meters.
    pub altitude_msl_m: f64,
    /// Total velocity magnitude in m/s.
    pub velocity_total_mps: f64,
    /// Vertical velocity component (positive up) in m/s.
    pub velocity_vertical_mps: f64,
    /// Horizontal velocity magnitude in m/s.
    pub velocity_horizontal_mps: f64,
    /// Mach number (velocity / local speed of sound).
    pub mach_number: f64,
    /// Dynamic pressure Q = 0.5 * rho * v^2 in Pascals.
    pub dynamic_pressure_pa: f64,
    /// G-load (total acceleration / 9.81).
    pub g_load: f64,
    /// Apoapsis altitude above body surface in meters.
    pub apoapsis_altitude_m: f64,
    /// Periapsis altitude above body surface in meters.
    pub periapsis_altitude_m: f64,
    /// Thrust-to-weight ratio (total thrust / weight).
    pub tw_ratio: f64,
    /// Remaining delta-v in m/s (ideal rocket equation).
    pub delta_v_remaining_mps: f64,
    /// Propellant remaining as fraction of initial (0.0-1.0).
    pub propellant_fraction: f64,
    /// Current active stage index.
    pub active_stage: usize,
    /// Current mission phase.
    pub mission_phase: RocketMissionState,
    /// Total thrust in Newtons.
    pub total_thrust_n: f64,
    /// Current mass in kg.
    pub mass_kg: f64,
    /// Specific impulse of active engines (vacuum) in seconds.
    pub isp_vacuum_s: f64,
    /// Angle of attack in degrees.
    pub angle_of_attack_deg: f64,
    /// Sideslip angle in degrees.
    pub sideslip_angle_deg: f64,
    /// Bank angle in degrees (roll relative to horizon).
    pub bank_angle_deg: f64,
    /// Roll rate in deg/s.
    pub roll_rate_dps: f64,
    /// Pitch rate in deg/s.
    pub pitch_rate_dps: f64,
    /// Yaw rate in deg/s.
    pub yaw_rate_dps: f64,
    /// Throttle setting (0.0-1.0).
    pub throttle: f32,
    /// Gimbal pitch deflection in degrees.
    pub gimbal_pitch_deg: f32,
    /// Gimbal yaw deflection in degrees.
    pub gimbal_yaw_deg: f32,
    /// Radar altitude from terrain collision in meters.
    pub radar_altitude_m: f64,
    /// Terrain slope in degrees.
    pub terrain_slope_deg: f64,
    /// Ground contact state.
    pub ground_contact: GroundContact,
    /// Convective heat flux in W/m².
    pub convective_heat_flux_w_m2: f64,
    /// Radiative heat flux in W/m².
    pub radiative_heat_flux_w_m2: f64,
    /// Total heat flux in W/m².
    pub total_heat_flux_w_m2: f64,
    /// Ablation nose radius in meters.
    pub nose_radius_m: f64,
    /// TPS thickness remaining in meters.
    pub tps_thickness_remaining_m: f64,
    /// Plasma blackout active.
    pub plasma_blackout: bool,
    /// Drogue parachute deployed.
    pub drogue_deployed: bool,
    /// Main parachute deployed.
    pub main_deployed: bool,
    /// True when the surface below is inferred to be water.
    pub over_water: bool,
    /// Time since liftoff in seconds.
    pub time_since_liftoff_s: f64,
    /// Downrange distance in meters.
    pub downrange_m: f64,
    /// Crossrange distance in meters.
    pub crossrange_m: f64,
}

/// Rocket camera mode for different viewing perspectives.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RocketCameraMode {
    #[default]
    Chase, // Third-person chase camera behind the rocket
    Cockpit, // First-person from rocket body
    Orbital, // Inertial frame showing orbital trajectory
    Surface, // Planet-relative for landing
    Free,    // Free camera (debug)
}

/// Configuration for rocket camera modes.
#[derive(Resource, Debug, Clone)]
pub struct RocketCameraConfig {
    /// Chase camera distance behind rocket
    pub chase_distance: f32,
    /// Chase camera height offset
    pub chase_height: f32,
    /// Chase camera pitch angle (down from horizontal)
    pub chase_pitch: f32,
    /// Cockpit camera offset from rocket center
    pub cockpit_offset: Vec3,
    /// Orbital camera distance from rocket
    pub orbital_distance: f32,
    /// Orbital camera elevation angle
    pub orbital_elevation: f32,
    /// Surface camera distance from landing target
    pub surface_distance: f32,
    /// Surface camera height above terrain
    pub surface_height: f32,
    /// Transition speed between modes
    pub transition_speed: f32,
    /// Smoothing factor for camera movement
    pub smooth_factor: f32,
}

impl Default for RocketCameraConfig {
    fn default() -> Self {
        Self {
            chase_distance: 100.0,
            chase_height: 20.0,
            chase_pitch: -0.3,
            cockpit_offset: Vec3::new(0.0, 5.0, 0.0),
            orbital_distance: 500.0,
            orbital_elevation: 0.5,
            surface_distance: 200.0,
            surface_height: 50.0,
            transition_speed: 2.0,
            smooth_factor: 0.1,
        }
    }
}

/// Rocket camera controller for managing camera state and transitions.
#[derive(Component, Debug, Clone, Default)]
pub struct RocketCameraController {
    pub current_mode: RocketCameraMode,
    pub target_mode: RocketCameraMode,
    pub transition_progress: f32,
    pub last_rocket_transform: Option<Transform>,
}
