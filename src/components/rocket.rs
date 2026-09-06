// Granular rocket components for ECS query isolation.
// Each component has a single responsibility, enabling parallel system execution.

use crate::domain::entities::rocket::{
    ParallelBoosters, Rocket, RocketMissionState as DomainRocketMissionState, RocketStage,
};
use crate::domain::services::atmosphere::FlightConditions;
use crate::domain::services::gravity::ForceModelReport;
use crate::domain::services::landing_gear::{LandingGear, LegDeploymentState};
use crate::domain::services::rocket_dynamics::RocketDynamicsState;
use crate::domain::services::rocket_propulsion::{
    ActiveVehicleMassProperties, ActiveVehicleMassPropertiesInput,
};
use crate::domain::services::terrain_collision::GroundContact;
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use bevy::math::{DQuat, DVec3, Vec3};
use bevy::prelude::*;
use std::ops::Deref;

/// Authoritative 6-DOF physics state in planet-centered inertial frame (f64).
/// Single source of truth for rocket motion. Only `integrate_6dof` writes this.
#[derive(Component, Debug, Clone)]
pub struct RocketPhysicsState {
    pub dynamics: RocketDynamicsState,
}

impl RocketPhysicsState {
    /// Rebuild the attached vehicle's mass, inertia, and center of mass from
    /// one propulsion inventory snapshot. These rigid-body properties must
    /// always change together.
    pub(crate) fn refresh_attached_mass_properties(
        &mut self,
        propulsion: &RocketPropulsion,
        geometry: RocketGeometry,
        ablation_mass_loss_kg: f64,
    ) -> f64 {
        let mass_properties = propulsion.mass_properties(geometry, ablation_mass_loss_kg);
        self.dynamics.mass_kg = mass_properties.mass_kg;
        self.dynamics.inertia_body = mass_properties.inertia_body;
        self.dynamics.center_of_mass_m = mass_properties.center_of_mass_m;
        mass_properties.mass_kg
    }
}

/// Previous/current physics snapshots for render interpolation (AGENTS.md
/// section 49). Physics runs in `FixedUpdate` while rendering runs every frame,
/// so the mesh is drawn at an interpolated sub-step position instead of
/// jumping between fixed ticks.
///
/// Written by `capture_render_state` (FixedUpdate); read by
/// `interpolate_render_transform` (Update). Overwritten in place so it needs
/// no allocation.
#[derive(Component, Debug, Clone, Copy)]
pub struct RocketRenderState {
    pub prev: RocketDynamicsState,
    pub current: RocketDynamicsState,
}

impl RocketRenderState {
    pub fn new(dynamics: RocketDynamicsState) -> Self {
        Self {
            prev: dynamics,
            current: dynamics,
        }
    }
}

/// Exterior geometry of the currently attached vehicle assembly. It starts as
/// the full launch stack and changes atomically at stage separation so every
/// aerodynamic, inertia, recovery, and contact consumer sees the same body.
#[derive(Component, Debug, Clone, Copy)]
pub struct RocketGeometry {
    pub radius_m: f32,
    pub height_m: f32,
    /// Lowest attached-cylinder extent in the shared stack frame, meters.
    pub lower_extent_y_m: f32,
}

impl RocketGeometry {
    /// Lowest point of the cylindrical contact approximation, measured from
    /// the assembly geometric center in the body frame. Engine bells and
    /// landing-leg visuals remain presentation-only; contact uses this single
    /// cylindrical extent.
    pub fn lower_extent_body_m(self) -> DVec3 {
        DVec3::Y * f64::from(self.lower_extent_y_m.min(0.0))
    }
}

/// Mission phase state machine component wrapping the domain enum.
/// Updated by guidance_system, terrain_interaction.
/// Read by control, actuation, propulsion, guidance systems.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RocketMissionState(pub DomainRocketMissionState);

#[expect(
    non_upper_case_globals,
    reason = "PascalCase aliases preserve the established mission-state component API."
)]
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

/// Attached booster fuel is valid only while the hardware remains on the core.
/// Keeping the lifecycle and inventory together prevents a detached stack from
/// continuing to contribute booster mass, thrust, or torque.
#[derive(Debug, Clone)]
pub(crate) enum BoosterAttachmentState {
    Detached,
    Attached(AttachedBoosterInventory),
}

#[derive(Debug, Clone)]
pub(crate) struct AttachedBoosterInventory {
    propellant_remaining_kg: Vec<f32>,
}

impl BoosterAttachmentState {
    pub(crate) fn fresh_for(vehicle: &Rocket) -> Self {
        vehicle
            .parallel_boosters
            .as_ref()
            .map_or(Self::Detached, |boosters| {
                Self::Attached(AttachedBoosterInventory {
                    propellant_remaining_kg: vec![
                        boosters.stage.propellant_mass_kg;
                        boosters.count()
                    ],
                })
            })
    }

    pub(crate) fn is_attached(&self) -> bool {
        matches!(self, Self::Attached(_))
    }

    pub(crate) fn remaining_kg(&self) -> Option<&[f32]> {
        match self {
            Self::Detached => None,
            Self::Attached(inventory) => Some(&inventory.propellant_remaining_kg),
        }
    }

    pub(crate) fn remaining_kg_mut(&mut self) -> Option<&mut [f32]> {
        match self {
            Self::Detached => None,
            Self::Attached(inventory) => Some(&mut inventory.propellant_remaining_kg),
        }
    }

    pub(crate) fn all_propellant_depleted(&self) -> bool {
        self.remaining_kg().is_some_and(|remaining_kg| {
            !remaining_kg.is_empty() && remaining_kg.iter().all(|mass_kg| *mass_kg <= 0.0)
        })
    }

    pub(crate) fn detach(&mut self) {
        *self = Self::Detached;
    }
}

/// Runtime propulsion state: vehicle definition, active stage, throttle, gimbal.
/// Updated by actuation_system, propulsion_consumption, propulsion_staging.
#[derive(Component, Debug, Clone)]
pub struct RocketPropulsion {
    pub vehicle: Rocket,
    pub active_stage: usize,
    pub propellant_remaining_kg: Vec<f32>,
    pub(crate) booster_attachment: BoosterAttachmentState,
    pub throttle: f32,
    pub gimbal_pitch_rad: f32,
    pub gimbal_yaw_rad: f32,
    /// Seconds since the last stage separation; reset to zero on staging and
    /// advanced by the ullage system. Gates engine restarts when
    /// `ullage_settle_time_s` is configured.
    pub time_since_separation_s: f32,
    /// Required settle time after staging before engines may ignite (ullage).
    /// Zero disables the gate.
    pub ullage_settle_time_s: f32,
    /// Number of stage separations so far. After any separation, an engine
    /// start must wait for the configured ullage settle interval.
    pub separations_count: u32,
    /// Mass of attached payload hardware (payload fairing) still on the
    /// vehicle, kg. Included in every mass recompute so consumption, staging,
    /// and fairing jettison share one mass authority. Zeroed on jettison.
    pub attached_payload_kg: f32,
}

/// Borrowed read capability for the configured active core stage and its live
/// inventory. This keeps reserve-aware eligibility aligned across guidance,
/// actuation, forces, and presentation without exposing parallel vectors.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ActiveCoreStage<'a> {
    stage: &'a RocketStage,
    remaining_propellant_kg: f32,
}

impl ActiveCoreStage<'_> {
    pub(crate) fn stage(&self) -> &RocketStage {
        self.stage
    }

    pub(crate) fn recovery_reserve_kg(&self) -> f32 {
        self.stage.recovery_propellant_reserve_kg.unwrap_or(0.0)
    }

    pub(crate) fn burnable_propellant_kg(&self) -> f32 {
        (self.remaining_propellant_kg - self.recovery_reserve_kg()).max(0.0)
    }

    pub(crate) fn has_burnable_propellant(&self) -> bool {
        self.burnable_propellant_kg() > 0.0
    }

    pub(crate) fn has_running_engines(&self) -> bool {
        self.stage
            .engines
            .iter()
            .any(|engine| engine.state == crate::domain::entities::rocket::EngineState::Running)
    }
}

impl RocketPropulsion {
    /// Create a fully fueled vehicle whose engine-start budget and ullage gate
    /// are ready for its first pad ignition.
    pub(crate) fn for_fresh_flight(
        vehicle: Rocket,
        attached_payload_kg: f32,
        ullage_settle_time_s: f32,
    ) -> Self {
        let mut propulsion = Self {
            vehicle,
            active_stage: 0,
            propellant_remaining_kg: Vec::new(),
            booster_attachment: BoosterAttachmentState::Detached,
            throttle: 0.0,
            gimbal_pitch_rad: 0.0,
            gimbal_yaw_rad: 0.0,
            time_since_separation_s: 0.0,
            ullage_settle_time_s,
            separations_count: 0,
            attached_payload_kg: 0.0,
        };
        propulsion.reset_for_relaunch(attached_payload_kg);
        propulsion
    }

    /// Restore configuration-derived inventories and flight-actuator state for
    /// a pad relaunch. The configured ullage settle interval is retained, while
    /// the gate starts open because the first ignition is ground supported.
    pub(crate) fn reset_for_relaunch(&mut self, attached_payload_kg: f32) {
        self.vehicle.reset_engine_lifecycles();
        self.propellant_remaining_kg = self
            .vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        self.booster_attachment = BoosterAttachmentState::fresh_for(&self.vehicle);
        self.active_stage = 0;
        self.throttle = 0.0;
        self.gimbal_pitch_rad = 0.0;
        self.gimbal_yaw_rad = 0.0;
        self.time_since_separation_s = self.ullage_settle_time_s;
        self.separations_count = 0;
        self.attached_payload_kg = attached_payload_kg;
    }

    pub(crate) fn boosters_attached(&self) -> bool {
        self.booster_attachment.is_attached()
    }

    pub(crate) fn attached_booster_inventory(&self) -> Option<&[f32]> {
        self.booster_attachment.remaining_kg()
    }

    pub(crate) fn attached_booster_inventory_mut(&mut self) -> Option<&mut [f32]> {
        self.booster_attachment.remaining_kg_mut()
    }

    pub(crate) fn attached_boosters(&self) -> Option<(&ParallelBoosters, &[f32])> {
        Some((
            self.vehicle.parallel_boosters.as_ref()?,
            self.attached_booster_inventory()?,
        ))
    }

    pub(crate) fn attached_boosters_are_depleted(&self) -> bool {
        self.booster_attachment.all_propellant_depleted()
    }

    pub(crate) fn detach_boosters(&mut self) {
        self.booster_attachment.detach();
    }

    pub(crate) fn booster_is_ignitable(&self, booster_index: usize) -> bool {
        self.attached_boosters()
            .is_some_and(|(boosters, inventory)| {
                self.throttle > 0.0
                    && inventory.get(booster_index).copied().unwrap_or(0.0) > 0.0
                    && boosters.stage.engines.iter().any(|engine| {
                        engine.state == crate::domain::entities::rocket::EngineState::Running
                    })
            })
    }

    /// Returns active-stage configuration without requiring a matching live
    /// inventory. Control uses this during command allocation before actuation
    /// decides whether a tank can support the command.
    pub(crate) fn active_stage_configuration(&self) -> Option<&RocketStage> {
        self.vehicle.stages.get(self.active_stage)
    }

    /// Origin of the active stage's local engine stations in the current
    /// attached-stack frame. An invalid active-stage index has no origin.
    pub(crate) fn active_stage_origin_in_stack_m(&self, stack_height_m: f32) -> Option<DVec3> {
        let attached_stages = self.vehicle.stages.get(self.active_stage..)?;
        Rocket::stage_origin_in_stack_m(attached_stages, stack_height_m, 0)
            .map(|origin| origin.as_dvec3())
    }

    /// Returns the active core stage only when its configuration and inventory
    /// remain synchronized. Consumers must not independently index these two
    /// parallel data sources.
    pub(crate) fn active_core_stage(&self) -> Option<ActiveCoreStage<'_>> {
        Some(ActiveCoreStage {
            stage: self.active_stage_configuration()?,
            remaining_propellant_kg: *self.propellant_remaining_kg.get(self.active_stage)?,
        })
    }

    /// Returns the active core stage only when it can produce thrust this tick.
    /// Force, torque, consumption, telemetry, and debug presentation share
    /// this exact engine and reserve eligibility rule.
    pub(crate) fn running_core_stage(&self) -> Option<(ActiveCoreStage<'_>, f32)> {
        let stage = self.active_core_stage()?;
        let throttle = self.throttle.clamp(0.0, 1.0);
        if throttle <= 0.0 || !stage.has_burnable_propellant() || !stage.has_running_engines() {
            return None;
        }
        Some((stage, throttle))
    }

    /// Stores the post-burn core inventory while preserving the configured
    /// recovery reserve. Only propulsion consumption is allowed to call this.
    pub(crate) fn set_active_core_burnable_propellant_kg(
        &mut self,
        burnable_propellant_kg: f32,
    ) -> bool {
        let Some(recovery_reserve_kg) = self
            .active_core_stage()
            .map(|stage| stage.recovery_reserve_kg())
        else {
            return false;
        };
        let Some(remaining_propellant_kg) = self.propellant_remaining_kg.get_mut(self.active_stage)
        else {
            return false;
        };
        *remaining_propellant_kg = recovery_reserve_kg + burnable_propellant_kg.max(0.0);
        true
    }

    /// Derive mass, inertia, and center of mass from the one authoritative
    /// attached vehicle inventory.
    pub(crate) fn mass_properties(
        &self,
        geometry: RocketGeometry,
        ablation_mass_loss_kg: f64,
    ) -> ActiveVehicleMassProperties {
        let (boosters, booster_propellant_remaining_kg) = self
            .attached_boosters()
            .map_or((None, &[][..]), |(boosters, inventory)| {
                (Some(boosters), inventory)
            });
        ActiveVehicleMassPropertiesInput {
            stages: &self.vehicle.stages,
            propellant_remaining_kg: &self.propellant_remaining_kg,
            active_stage: self.active_stage,
            attached_payload_kg: self.attached_payload_kg,
            ablation_mass_loss_kg,
            radius_m: f64::from(geometry.radius_m),
            height_m: f64::from(geometry.height_m),
            boosters,
            booster_propellant_remaining_kg,
        }
        .calculate()
    }
}

/// Flight computer command interface between guidance, control, actuation, physics.
/// Guidance writes attitude and throttle targets; control writes gimbal/RCS;
/// actuation applies physical limits before the physics step.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct RocketCommands {
    pub target_attitude: DQuat,
    pub throttle_cmd: f32,
    pub gimbal_pitch_cmd_rad: f32,
    pub gimbal_yaw_cmd_rad: f32,
    pub rcs_torque_cmd_body: DVec3,
}

/// Autopilot configuration: guidance targets, PID gains, actuator limits, and mode.
#[derive(Component, Debug, Clone, Default)]
pub struct RocketAutopilot {
    pub gains: crate::domain::services::control::PidGains,
    pub integral: DVec3,
    pub ascent_profile: crate::domain::services::guidance::AscentGuidanceProfile,
    pub target_orbit: crate::domain::services::physics_orbital::LowEarthOrbitTarget,
    pub actuation: crate::domain::services::actuation::ActuationLimits,
    pub mode: crate::domain::services::guidance::AutopilotMode,
    pub time_since_liftoff_s: f64,
    pub target_landing_position_m: DVec3,
    /// Target circular-orbit radius for [`crate::domain::services::guidance::
    /// AutopilotMode::Transfer`] (planet-centered, meters). Zero disables the
    /// mode (no configured target).
    pub transfer_target_radius_m: f64,
}

/// Net force accumulator (world/planet-inertial frame), cleared each frame by integrate_6dof.
/// Written by: gravity, aero, propulsion, parachutes, retro-propulsion.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ForceAccumulator(DVec3);

impl ForceAccumulator {
    #[cfg(test)]
    pub(crate) fn from_force_n(force_n: DVec3) -> Self {
        Self(force_n)
    }

    pub(crate) fn add_force_n(&mut self, force_n: DVec3) {
        self.0 += force_n;
    }

    #[cfg(test)]
    pub(crate) fn force_n(&self) -> DVec3 {
        self.0
    }

    /// Consumes the completed fixed-tick force budget. Only integration calls
    /// this, ensuring no force leaks into the following simulation step.
    pub(crate) fn take_force_n(&mut self) -> DVec3 {
        std::mem::take(&mut self.0)
    }
}

/// Net torque accumulator (body frame), cleared each frame by integrate_6dof.
/// Written by: aero torque, gimbal, RCS, gimbal actuation.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TorqueAccumulator(DVec3);

impl TorqueAccumulator {
    #[cfg(test)]
    pub(crate) fn from_torque_nm(torque_nm: DVec3) -> Self {
        Self(torque_nm)
    }

    pub(crate) fn add_torque_nm(&mut self, torque_nm: DVec3) {
        self.0 += torque_nm;
    }

    #[cfg(test)]
    pub(crate) fn torque_nm(&self) -> DVec3 {
        self.0
    }

    /// Consumes the completed fixed-tick torque budget. Only integration calls
    /// this, ensuring no torque leaks into the following simulation step.
    pub(crate) fn take_torque_nm(&mut self) -> DVec3 {
        std::mem::take(&mut self.0)
    }
}

/// Binds rocket to its dominant gravity body.
#[derive(Component, Debug, Clone)]
pub struct RocketPlanetBinding {
    pub planet_name: CelestialBodyId,
}

/// Presentation-only procedural launch facility anchor in the bound planet's
/// body-fixed meter frame. It follows the same terrain sample and ephemeris
/// orientation as the prelaunch rocket, but never participates in contact or
/// flight physics.
#[derive(Component, Debug, Clone)]
pub struct LaunchPadPresentation {
    pub planet_name: CelestialBodyId,
    pub position_body_fixed_m: DVec3,
    pub normal_body_fixed: DVec3,
    pub heading_body_fixed: DVec3,
}

/// Authoritative gravitational acceleration (m/s²) acting on vehicle.
/// Computed by update_rocket_gravity, consumed by accumulate_forces.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct GravityAcceleration {
    pub value: DVec3,
}

/// Acceleration a vehicle-mounted accelerometer would sense (m/s²), excluding
/// gravity. Captured from the completed fixed integration force budget for use
/// by telemetry and the following tick's guidance.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SpecificForceAcceleration {
    pub value: DVec3,
}

/// Cached fixed-tick atmosphere sample and atmosphere-relative motion.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct RocketFlightConditions(FlightConditions);

impl RocketFlightConditions {
    /// Replaces the complete fixed-tick atmosphere sample. The refresh system
    /// is the only production writer so density, Mach, pressure, and airspeed
    /// always originate from the same atmosphere evaluation.
    pub(crate) fn replace_sample(&mut self, sample: FlightConditions) {
        self.0 = sample;
    }

    #[cfg(test)]
    pub(crate) fn from_sample(sample: FlightConditions) -> Self {
        Self(sample)
    }
}

impl Deref for RocketFlightConditions {
    type Target = FlightConditions;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
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
    /// True once the configured TPS layer is fully consumed. Heating remains
    /// observable, but ablation and TPS mass loss cannot continue past zero.
    pub tps_exhausted: bool,
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

/// What kind of jettisoned hardware a debris entity is.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpentStageKind {
    /// A separated booster / lower stage.
    Booster,
    /// One half of a jettisoned payload fairing.
    FairingHalf,
}

/// A jettisoned stage or fairing flying as uncontrolled debris. Carries its
/// own simplified physics (gravity + drag only) and despawns on ground
/// contact or below the lifecycle altitude threshold.
#[derive(Component, Debug, Clone, Copy)]
pub struct SpentStage {
    pub parent_rocket: Entity,
    pub kind: SpentStageKind,
}

/// A separated booster retained in the normal rocket flight pipeline for a
/// configured recovery. It remains a `SpentStage` for lifecycle identity, but
/// is excluded from the drag-only debris/despawn systems.
#[derive(Component, Debug, Clone, Copy)]
pub struct RecoveringStage;

/// Payload fairing attached to the vehicle. Presence of the component means
/// the fairing is still attached; jettison removes it and drops its mass.
#[derive(Component, Debug, Clone, Copy)]
pub struct PayloadFairing {
    pub dry_mass_kg: f32,
}

/// Immutable launch configuration retained after fairing separation so a
/// relaunch can restore the original vehicle mass and presentation state.
#[derive(Component, Debug, Clone, Copy)]
pub struct InitialPayloadFairing {
    pub dry_mass_kg: f32,
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
    /// True when the surface below is water. Authority: the body's explicit
    /// `has_ocean` config flag combined with terrain height at mean sea
    /// level (LIMITATION, Phase 15: no coastline polygons — a coastal strip
    /// within ±10 m of sea level reads as water anywhere on an ocean body).
    pub over_water: bool,
}

/// Persistent ground-contact state: true while the vehicle rests on terrain
/// and the post-integration contact constraint holds it there (pad hold and
/// landings alike). Released when available thrust exceeds weight.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct GroundRest {
    pub active: bool,
}

/// Deployable landing gear attached to the active or recovering stage. It
/// composes the pure domain assembly (`LandingGear`: spec + sized struts) with
/// the one-way deployment latch; GroundContact alone advances compression.
#[derive(Component, Debug, Clone)]
pub struct LandingLegs {
    pub gear: LandingGear,
    pub deployment: LegDeploymentState,
    /// Current strut compression, meters (0 = fully extended).
    pub compression_m: f64,
}

impl LandingLegs {
    pub fn new(gear: LandingGear) -> Self {
        Self {
            gear,
            deployment: LegDeploymentState::default(),
            compression_m: 0.0,
        }
    }

    pub fn deployed(&self) -> bool {
        self.deployment.deployed
    }

    /// Deploy-gate altitude from the assembly spec.
    pub fn deploy_gate_altitude_m(&self) -> f64 {
        self.gear.spec.deploy_altitude_m
    }
}

/// Tip-over lifecycle for a grounded vehicle (Phase 14): a sustained-tilt
/// monitor arms the pure domain fall model
/// (`landing_gear::ToppleFall`), which the GroundContact set then advances —
/// rigid rotation about the foot-plane edge under gravity torque. Terminal.
#[derive(Component, Debug, Clone, Default)]
pub struct TipOverState {
    /// Seconds the tilt has continuously exceeded the critical angle while
    /// resting (reset when the lean recovers).
    pub exceeded_for_s: f64,
    /// `Some` once the fall is armed; advancing it is terminal.
    pub fall: Option<crate::domain::services::landing_gear::ToppleFall>,
    /// Center-of-mass height above the pivot, captured at arm time, m.
    pub com_height_m: f64,
}

impl TipOverState {
    /// Fully reset (fresh flight).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// True once the fall is armed or complete.
    pub fn is_toppling(&self) -> bool {
        self.fall.is_some()
    }
}

/// One-shot record of how a touchdown went (Phase 14). Filled by the
/// GroundContact authority at the verdict tick and extended with the strut
/// compression peak while resting; HUD/telemetry only read it.
#[derive(Component, Debug, Clone, Default)]
pub struct LandingScorecard {
    /// Into-ground normal speed at the verdict, m/s (≥ 0).
    pub touchdown_vertical_speed_mps: f64,
    /// Tangent-plane speed at the verdict, m/s.
    pub touchdown_lateral_speed_mps: f64,
    /// Longitudinal-axis tilt from the surface normal at the verdict, deg.
    pub touchdown_tilt_deg: f64,
    /// Local terrain slope under the vehicle at the verdict, deg.
    pub touchdown_slope_deg: f64,
    /// Surface distance between the touchdown point and the autopilot's
    /// landing target, m.
    pub distance_to_target_m: f64,
    /// Deepest strut compression observed after touchdown, m.
    pub leg_compression_peak_m: f64,
    /// True when the surface below was water.
    pub over_water: bool,
    /// True once filled by the contact authority.
    pub recorded: bool,
}

/// Autonomous recovery vessel state. The embedded domain model is the
/// authoritative f64 inertial state; presentation may follow it but never
/// drives the vessel.
#[derive(Component, Debug, Clone)]
pub struct DroneShip {
    pub state: crate::domain::services::recovery::DroneShip,
    /// Inertial position that station keeping attempts to hold, meters.
    pub station_target_position_m: DVec3,
    pub station_keeper: crate::domain::services::recovery::StationKeeper,
    /// Half-width of the square landing deck, meters.
    pub deck_half_extent_m: f64,
}

/// Associates a recovery stage with a moving drone-ship deck. Guidance updates
/// the existing landing target from the ship's domain prediction; deck contact
/// latches only after a successful deck-relative touchdown verdict.
#[derive(Component, Debug, Clone, Copy)]
pub struct DroneShipLandingTarget {
    pub drone_ship: Entity,
    /// Fixed prediction horizon supplied by the active recovery profile, s.
    pub prediction_horizon_s: f64,
    pub deck_contact: bool,
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
    /// Named gravity-model selection and its declared active terms.
    pub force_model: ForceModelReport,
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
    /// True when a touchdown has been recorded (Phase 14 scorecard).
    pub touchdown_recorded: bool,
    /// Into-ground speed at the recorded touchdown, m/s.
    pub touchdown_vertical_speed_mps: f64,
    /// Tangent-plane speed at the recorded touchdown, m/s.
    pub touchdown_lateral_speed_mps: f64,
    /// Tilt at the recorded touchdown, degrees.
    pub touchdown_tilt_deg: f64,
    /// Terrain slope at the recorded touchdown, degrees.
    pub touchdown_slope_deg: f64,
    /// Surface distance to the configured landing target, m.
    pub touchdown_distance_to_target_m: f64,
    /// Deepest strut compression after touchdown, m.
    pub leg_compression_peak_m: f64,
    /// True while a gravity-driven topple is in progress or complete.
    pub toppling: bool,
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
    /// Chase camera pitch angle in radians; negative values aim toward the planet.
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
            // For 70m tall rocket: distance ~3x height, height ~0.7x height
            // so the whole rocket from engines to nose is framed.
            chase_distance: 220.0,
            chase_height: 50.0,
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
#[derive(Component, Debug, Clone)]
pub struct RocketCameraController {
    pub current_mode: RocketCameraMode,
    pub target_mode: RocketCameraMode,
    pub transition_progress: f32,
    /// Camera pose in the target mode's presentation frame at the start of a
    /// mode change. Vehicle-attached modes use the body frame; planet-relative
    /// modes use the render frame so contact attitude cannot rotate the camera.
    pub transition_start_pose: Option<Transform>,
    /// Presentation-only camera pose in the target mode's presentation frame.
    pub smoothed_pose: Option<Transform>,
    /// Free-fly (space) camera orbit angles, radians, and distance from the
    /// rocket. Adjusted by mouse drag / scroll while in `Free` mode.
    pub free_orbit_yaw: f32,
    pub free_orbit_pitch: f32,
    pub free_orbit_distance: f32,
}

impl Default for RocketCameraController {
    fn default() -> Self {
        Self {
            current_mode: RocketCameraMode::default(),
            target_mode: RocketCameraMode::default(),
            transition_progress: 0.0,
            transition_start_pose: None,
            smoothed_pose: None,
            free_orbit_yaw: 0.0,
            free_orbit_pitch: 0.35,
            free_orbit_distance: 600.0,
        }
    }
}

impl RocketCameraController {
    pub fn request_mode(&mut self, mode: RocketCameraMode) {
        if self.target_mode == mode {
            return;
        }
        self.target_mode = mode;
        self.transition_progress = 0.0;
        self.transition_start_pose = None;
    }

    pub fn begin_transition(&mut self, pose: Transform) -> Transform {
        *self.transition_start_pose.get_or_insert(pose)
    }

    pub fn complete_transition(&mut self) {
        self.current_mode = self.target_mode;
        self.transition_progress = 0.0;
        self.transition_start_pose = None;
    }

    pub fn cancel_transition(&mut self) {
        self.transition_progress = 0.0;
        self.transition_start_pose = None;
    }
}

#[cfg(test)]
mod camera_controller_tests {
    use super::*;

    #[test]
    fn retargeting_clears_the_previous_transition_pose() {
        let mut controller = RocketCameraController {
            transition_start_pose: Some(Transform::from_xyz(1.0, 2.0, 3.0)),
            transition_progress: 0.5,
            ..Default::default()
        };

        controller.request_mode(RocketCameraMode::Cockpit);

        assert_eq!(controller.target_mode, RocketCameraMode::Cockpit);
        assert_eq!(controller.transition_progress, 0.0);
        assert!(controller.transition_start_pose.is_none());
    }
}

#[cfg(test)]
mod propulsion_tests {
    use super::*;
    use crate::domain::entities::rocket::{EngineState, ParallelBoosters};
    use bevy::math::{DMat3, DQuat, DVec3, Vec3};

    fn all_engines_are_fresh(vehicle: &Rocket) -> bool {
        vehicle.stages.iter().all(|stage| {
            stage
                .engines
                .iter()
                .all(|engine| engine.state == EngineState::Off && engine.ignition_count == 0)
        }) && vehicle.parallel_boosters.as_ref().is_none_or(|boosters| {
            boosters
                .stage
                .engines
                .iter()
                .all(|engine| engine.state == EngineState::Off && engine.ignition_count == 0)
        })
    }

    #[test]
    fn propulsion_reset_restores_the_configured_fresh_flight_state() {
        let mut vehicle = Rocket::falcon9_test_fixture();
        vehicle.parallel_boosters = Some(ParallelBoosters::new(
            vehicle.stages[0].clone(),
            vec![Vec3::X],
        ));
        let mut propulsion = RocketPropulsion::for_fresh_flight(vehicle, 25.0, 2.0);

        assert_eq!(propulsion.active_stage, 0);
        assert_eq!(propulsion.time_since_separation_s, 2.0);
        assert_eq!(propulsion.attached_payload_kg, 25.0);
        assert!(propulsion
            .vehicle
            .stages
            .iter()
            .zip(&propulsion.propellant_remaining_kg)
            .all(|(stage, remaining)| *remaining == stage.propellant_mass_kg));
        assert_eq!(
            propulsion.attached_booster_inventory(),
            Some(&[90_000.0][..])
        );
        assert!(all_engines_are_fresh(&propulsion.vehicle));

        propulsion.active_stage = 1;
        propulsion.propellant_remaining_kg.fill(0.0);
        propulsion.throttle = 0.7;
        propulsion.gimbal_pitch_rad = 0.2;
        propulsion.gimbal_yaw_rad = -0.2;
        propulsion.time_since_separation_s = 0.0;
        propulsion.separations_count = 1;
        propulsion.vehicle.stages[0].engines[0].state = EngineState::Depleted;
        propulsion.vehicle.stages[0].engines[0].ignition_count = 3;
        let booster_engine = &mut propulsion
            .vehicle
            .parallel_boosters
            .as_mut()
            .expect("fixture includes one booster")
            .stage
            .engines[0];
        booster_engine.state = EngineState::Depleted;
        booster_engine.ignition_count = 3;

        propulsion.reset_for_relaunch(10.0);

        assert_eq!(propulsion.active_stage, 0);
        assert_eq!(propulsion.throttle, 0.0);
        assert_eq!(propulsion.gimbal_pitch_rad, 0.0);
        assert_eq!(propulsion.gimbal_yaw_rad, 0.0);
        assert_eq!(propulsion.time_since_separation_s, 2.0);
        assert_eq!(propulsion.separations_count, 0);
        assert_eq!(propulsion.attached_payload_kg, 10.0);
        assert_eq!(
            propulsion.attached_booster_inventory(),
            Some(&[90_000.0][..])
        );
        assert!(all_engines_are_fresh(&propulsion.vehicle));
    }

    #[test]
    fn booster_attachment_transition_removes_the_inventory_atomically() {
        let mut vehicle = Rocket::falcon9_test_fixture();
        vehicle.parallel_boosters = Some(ParallelBoosters::new(
            vehicle.stages[0].clone(),
            vec![Vec3::X, Vec3::NEG_X],
        ));
        let mut attachment = BoosterAttachmentState::fresh_for(&vehicle);

        assert_eq!(attachment.remaining_kg(), Some(&[90_000.0, 90_000.0][..]));
        assert!(attachment.is_attached());
        attachment.detach();

        assert!(attachment.remaining_kg().is_none());
        assert!(!attachment.is_attached());
    }

    #[test]
    fn active_core_stage_preserves_its_recovery_reserve() {
        let vehicle = Rocket::falcon9_test_fixture();
        let mut propulsion = RocketPropulsion::for_fresh_flight(vehicle, 0.0, 0.0);
        let reserve_kg = propulsion.vehicle.stages[0]
            .recovery_propellant_reserve_kg
            .expect("fixture first stage has a recovery reserve");

        let active_stage = propulsion
            .active_core_stage()
            .expect("fresh flight has synchronized active inventory");
        assert_eq!(active_stage.recovery_reserve_kg(), reserve_kg);
        assert_eq!(
            active_stage.burnable_propellant_kg() + active_stage.recovery_reserve_kg(),
            propulsion.vehicle.stages[0].propellant_mass_kg
        );
        assert!(active_stage.has_burnable_propellant());

        propulsion.throttle = 1.0;
        propulsion.vehicle.stages[0].engines[0].state = EngineState::Running;
        assert!(propulsion.running_core_stage().is_some());

        assert!(propulsion.set_active_core_burnable_propellant_kg(0.0));
        let reserved_stage = propulsion
            .active_core_stage()
            .expect("reserve-only inventory remains synchronized");
        assert_eq!(reserved_stage.burnable_propellant_kg(), 0.0);
        assert!(!reserved_stage.has_burnable_propellant());
        assert!(propulsion.running_core_stage().is_none());
    }

    #[test]
    fn active_core_stage_rejects_unsynchronized_inventory() {
        let vehicle = Rocket::falcon9_test_fixture();
        let mut propulsion = RocketPropulsion::for_fresh_flight(vehicle, 0.0, 0.0);
        propulsion.propellant_remaining_kg.clear();

        assert!(propulsion.active_core_stage().is_none());
        assert!(!propulsion.set_active_core_burnable_propellant_kg(1.0));
    }

    #[test]
    fn mass_properties_follow_attached_inventory_and_ablation() {
        let mut vehicle = Rocket::falcon9_test_fixture();
        let booster_stage = vehicle.stages[0].clone();
        let booster_mass_kg = f64::from(booster_stage.total_mass_kg()) * 2.0;
        vehicle.parallel_boosters = Some(ParallelBoosters::new(
            booster_stage,
            vec![Vec3::X, Vec3::NEG_X],
        ));
        let mut propulsion = RocketPropulsion::for_fresh_flight(vehicle, 0.0, 0.0);
        let geometry = RocketGeometry {
            radius_m: 2.0,
            height_m: 70.0,
            lower_extent_y_m: -35.0,
        };

        let attached = propulsion.mass_properties(geometry, 0.0);
        propulsion.detach_boosters();
        let detached = propulsion.mass_properties(geometry, 0.0);
        let ablated = propulsion.mass_properties(geometry, 50.0);

        assert!((attached.mass_kg - detached.mass_kg - booster_mass_kg).abs() < 1e-6);
        assert!((detached.mass_kg - ablated.mass_kg - 50.0).abs() < 1e-6);
    }

    #[test]
    fn physics_state_refreshes_all_attached_mass_properties_together() {
        let propulsion =
            RocketPropulsion::for_fresh_flight(Rocket::falcon9_test_fixture(), 25.0, 0.0);
        let geometry = RocketGeometry {
            radius_m: 2.0,
            height_m: 70.0,
            lower_extent_y_m: -35.0,
        };
        let expected = propulsion.mass_properties(geometry, 50.0);
        let mut rocket = RocketPhysicsState {
            dynamics: RocketDynamicsState::new(
                DVec3::ZERO,
                DVec3::ZERO,
                DQuat::IDENTITY,
                1.0,
                DMat3::IDENTITY,
                DVec3::ZERO,
            ),
        };

        let mass_kg = rocket.refresh_attached_mass_properties(&propulsion, geometry, 50.0);

        assert_eq!(mass_kg, expected.mass_kg);
        assert_eq!(rocket.dynamics.mass_kg, expected.mass_kg);
        assert_eq!(rocket.dynamics.inertia_body, expected.inertia_body);
        assert_eq!(rocket.dynamics.center_of_mass_m, expected.center_of_mass_m);
    }
}

#[cfg(test)]
mod flight_conditions_tests {
    use super::*;

    #[test]
    fn replacing_the_fixed_tick_sample_preserves_the_complete_snapshot() {
        let sample = FlightConditions {
            altitude_m: 1_000.0,
            atmosphere_relative_velocity_mps: DVec3::new(1.0, 2.0, 3.0),
            airspeed_mps: 3.741_657_386_773_941_3,
            ..default()
        };
        let mut conditions = RocketFlightConditions::default();

        conditions.replace_sample(sample);

        assert_eq!(*conditions, sample);
    }
}

/// Resource flag indicating the application is running in Rocket mode.
/// Used by shared systems (e.g. planet updates) to conditionally skip
/// solar-system-scale work when the camera is in the flight frame.
#[derive(Resource, Debug, Default)]
pub struct RocketMode;
