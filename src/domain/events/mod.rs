//! Domain events for decoupled cross-system reactions (AGENTS.md section 31).
//!
//! These are Bevy buffered messages (`#[derive(Message)]`); they describe
//! *what happened* so HUD, flight log, and audio systems can react without
//! polling simulation state.

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
