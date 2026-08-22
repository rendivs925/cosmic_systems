// System sets for rocket simulation pipeline ordering.
// Enforces correct execution order: Guidance → Control → Actuation → Forces → Integration → Sync

use bevy::ecs::schedule::SystemSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RocketSet {
    Guidance,
    Control,
    Actuation,
    Gravity,
    OrbitalElements,
    TerrainInteraction,
    Atmosphere,
    /// Jettisoned hardware (spent stages, fairing halves): drag-only flight
    /// and lifecycle despawn.
    SpentStage,
    EntryPhysics,
    AeroForces,
    AeroTorque,
    PropulsionThrust,
    PropulsionGimbal,
    PropulsionConsumption,
    PropulsionStaging,
    AccumulateForces,
    Integrate,
    SyncRender,
    Telemetry,
}
