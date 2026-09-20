//! Domain events for decoupled cross-system reactions (AGENTS.md section 31).
//!
//! These are Bevy buffered messages (`#[derive(Message)]`); they describe
//! *what happened* so HUD, flight log, and audio systems can react without
//! polling simulation state.

use crate::domain::entities::rocket::RocketMissionState;
use bevy::math::DVec3;
use bevy::prelude::{Entity, Message};

/// A vehicle's comms link entered or exited plasma blackout.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct CommsBlackoutEvent {
    pub rocket: Entity,
    /// `true` = blackout started (signal lost), `false` = ended (signal
    /// reacquired).
    pub blackout_active: bool,
}

/// A vehicle touched down on water.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct SplashdownDetectedEvent {
    pub rocket: Entity,
    /// Touchdown point in planet-centered inertial meters.
    pub position_m: DVec3,
    /// Vertical speed at touchdown [m/s] (negative = descending).
    pub touchdown_vertical_speed_mps: f64,
}

/// A stage separated from a vehicle and became its own debris entity.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct StageSeparatedEvent {
    /// The remaining (upper-stage) vehicle.
    pub rocket: Entity,
    /// The newly spawned spent-stage debris entity.
    pub spent_stage: Entity,
    /// Total mass shed with the stage (dry + residual propellant) [kg].
    pub shed_mass_kg: f64,
}

/// A payload fairing was jettisoned.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct FairingSeparatedEvent {
    pub rocket: Entity,
    /// Mass dropped with the fairing halves [kg].
    pub fairing_mass_kg: f64,
}

/// The active stage ignited one or more engines this tick.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct EngineIgnitionEvent {
    pub rocket: Entity,
    /// Active stage index at ignition.
    pub stage_index: u32,
    /// Engines that transitioned from off/depeleted to running this tick.
    pub engines_started: u32,
}

/// The active stage shut down one or more running engines this tick.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct EngineCutoffEvent {
    pub rocket: Entity,
    /// Active stage index at cutoff.
    pub stage_index: u32,
    /// Engines that stopped running this tick.
    pub engines_stopped: u32,
}

/// Upward thrust exceeded weight and the vehicle released from the surface.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct LiftoffEvent {
    pub rocket: Entity,
    /// Contact point in planet-centered inertial meters.
    pub position_m: DVec3,
    /// Net upward thrust at release [N].
    pub upward_thrust_n: f64,
    /// Vehicle weight at release [N].
    pub weight_n: f64,
}

/// A vehicle touched down on land or a recovered deck (water uses
/// [`SplashdownDetectedEvent`]).
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct TouchdownEvent {
    pub rocket: Entity,
    /// Touchdown point in planet-centered inertial meters.
    pub position_m: DVec3,
    /// Into-surface speed at touchdown [m/s].
    pub vertical_speed_mps: f64,
    /// Tangential speed at touchdown [m/s].
    pub lateral_speed_mps: f64,
    /// Tilt from the surface normal at touchdown [deg].
    pub tilt_deg: f64,
    /// Surface slope at touchdown [deg].
    pub slope_deg: f64,
}

/// A vehicle impacted the surface beyond its landing limits.
#[derive(Debug, Clone, Copy, PartialEq, Message)]
pub struct CrashEvent {
    pub rocket: Entity,
    /// Impact point in planet-centered inertial meters.
    pub position_m: DVec3,
    /// Into-surface speed at impact [m/s].
    pub vertical_speed_mps: f64,
    /// Tilt from the surface normal at impact [deg].
    pub tilt_deg: f64,
}

/// The authoritative mission phase advanced to a new state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Message)]
pub struct MissionPhaseChangedEvent {
    pub rocket: Entity,
    pub previous: RocketMissionState,
    pub current: RocketMissionState,
}
