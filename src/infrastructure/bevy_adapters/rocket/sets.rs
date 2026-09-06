// System sets for rocket simulation pipeline ordering.
// Enforces correct execution order: Guidance → Control → Actuation → Forces → Integration → Sync

use bevy::ecs::schedule::SystemSet;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RocketSet {
    Guidance,
    /// Autonomous recovery targets and station-keeping, before guidance reads
    /// the moving landing target.
    Recovery,
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
    /// Advance the authoritative epoch after integration, before any system
    /// samples body-fixed terrain or records the completed state.
    AdvanceTime,
    /// Post-integration terrain contact: touchdown verdict, resting-contact
    /// constraint (penetration clamp / normal-velocity removal), liftoff
    /// release. Acts on the just-integrated authoritative state.
    GroundContact,
    SyncRender,
    Telemetry,
    /// Complete fixed-tick replay snapshot capture after all authoritative
    /// state and telemetry have been updated.
    Replay,
}
