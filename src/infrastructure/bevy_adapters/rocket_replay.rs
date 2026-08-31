//! Fixed-tick authoritative replay snapshots for rocket flight.
//!
//! This deliberately does not reuse `FlightRecorder`: telemetry is sampled at
//! a lower rate for analysis, while replay must restore every state value that
//! can affect a subsequent physics tick.

use crate::components::rocket::{
    AblationState, CommsState, DroneShip, DroneShipLandingTarget, ForceAccumulator,
    GravityAcceleration, GroundRest, LandingLegs, LandingScorecard, MaxQTracker, ParachuteState,
    PayloadFairing, RetroPropulsionEffect, RocketAutopilot, RocketCommands, RocketFlightConditions,
    RocketMissionState, RocketPhysicsState, RocketPropulsion, RocketRenderState,
    SpecificForceAcceleration, SpentStage, TerrainCollisionState, ThermalState, TipOverState,
    TorqueAccumulator,
};
use crate::domain::services::simulation_time::SimulationTime;
use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use std::collections::VecDeque;

/// One minute of exact 60 Hz history by default. Call
/// [`ReplaySnapshotStream::new`] to select a different retention capacity.
pub const DEFAULT_REPLAY_SNAPSHOT_CAPACITY: usize = 3_600;

/// A complete authoritative state for one rocket at one fixed simulation tick.
#[derive(Debug, Clone)]
pub struct RocketReplaySnapshot {
    pub entity: Entity,
    pub timestamp_s: f64,
    pub physics: RocketPhysicsState,
    pub mission: RocketMissionState,
    pub propulsion: RocketPropulsion,
    pub commands: RocketCommands,
    pub autopilot: RocketAutopilot,
    pub flight_conditions: RocketFlightConditions,
    pub terrain_collision: TerrainCollisionState,
    pub ground_rest: GroundRest,
    pub landing_legs: Option<LandingLegs>,
    pub thermal: ThermalState,
    pub ablation: AblationState,
    pub comms: CommsState,
    pub parachute: ParachuteState,
    pub retro_propulsion: RetroPropulsionEffect,
    pub max_q: MaxQTracker,
    pub tip_over: TipOverState,
    pub landing_scorecard: LandingScorecard,
    pub payload_fairing: Option<PayloadFairing>,
    pub drone_ship_target: Option<DroneShipLandingTarget>,
    pub force_accumulator: ForceAccumulator,
    pub torque_accumulator: TorqueAccumulator,
    pub specific_force: Option<SpecificForceAcceleration>,
}

/// All rocket snapshots captured at one completed fixed tick.
#[derive(Debug, Clone)]
pub struct ReplayFrame {
    pub timestamp_s: f64,
    pub rockets: Vec<RocketReplaySnapshot>,
    spent_stages: Vec<SpentStageReplaySnapshot>,
    drone_ships: Vec<DroneShipReplaySnapshot>,
}

/// The mutable authoritative state of one jettisoned hardware entity.
#[derive(Debug, Clone)]
struct SpentStageReplaySnapshot {
    entity: Entity,
    physics: RocketPhysicsState,
    flight_conditions: RocketFlightConditions,
    gravity: GravityAcceleration,
    force_accumulator: ForceAccumulator,
}

/// The mutable authoritative state of one drone ship.
#[derive(Debug, Clone)]
struct DroneShipReplaySnapshot {
    entity: Entity,
    ship: DroneShip,
}

#[derive(Debug, Clone)]
struct ReplaySession {
    live_time_acceleration: f64,
    live_was_paused: bool,
    live_rockets: ReplayFrame,
    selected_frame: usize,
}

/// Separate fixed-tick ring buffer and replay session state.
#[derive(Resource, Debug)]
pub struct ReplaySnapshotStream {
    frames: VecDeque<ReplayFrame>,
    capacity: usize,
    session: Option<ReplaySession>,
}

impl Default for ReplaySnapshotStream {
    fn default() -> Self {
        Self::new(DEFAULT_REPLAY_SNAPSHOT_CAPACITY)
    }
}

impl ReplaySnapshotStream {
    pub fn new(capacity: usize) -> Self {
        Self {
            frames: VecDeque::with_capacity(capacity),
            capacity,
            session: None,
        }
    }

    /// Frames in chronological order, from oldest retained to newest.
    pub fn frames(&self) -> &VecDeque<ReplayFrame> {
        &self.frames
    }

    pub fn is_replaying(&self) -> bool {
        self.session.is_some()
    }

    pub fn selected_frame(&self) -> Option<usize> {
        self.session.as_ref().map(|session| session.selected_frame)
    }

    fn push(&mut self, frame: ReplayFrame) {
        if self.capacity == 0 {
            return;
        }
        if self.frames.len() == self.capacity {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
    }
}

/// Minimal replay controls for a future HUD or input adapter.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayAction {
    BeginLatest,
    Seek { frame_index: usize },
    Resume,
}

/// Read-only authoritative state collected after each completed fixed tick.
#[derive(QueryData)]
pub struct ReplayCaptureAccess {
    pub entity: Entity,
    pub physics: &'static RocketPhysicsState,
    pub mission: &'static RocketMissionState,
    pub propulsion: &'static RocketPropulsion,
    pub commands: &'static RocketCommands,
    pub autopilot: &'static RocketAutopilot,
    pub flight_conditions: &'static RocketFlightConditions,
    pub terrain_collision: &'static TerrainCollisionState,
    pub ground_rest: &'static GroundRest,
    pub landing_legs: Option<&'static LandingLegs>,
    pub thermal: &'static ThermalState,
    pub ablation: &'static AblationState,
    pub comms: &'static CommsState,
    pub parachute: &'static ParachuteState,
    pub retro_propulsion: &'static RetroPropulsionEffect,
    pub max_q: &'static MaxQTracker,
    pub tip_over: &'static TipOverState,
    pub landing_scorecard: &'static LandingScorecard,
    pub payload_fairing: Option<&'static PayloadFairing>,
    pub drone_ship_target: Option<&'static DroneShipLandingTarget>,
    pub force_accumulator: &'static ForceAccumulator,
    pub torque_accumulator: &'static TorqueAccumulator,
    pub specific_force: Option<&'static SpecificForceAcceleration>,
}

/// Mutable counterpart used only while physics is paused for replay.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct ReplayRestoreAccess {
    pub entity: Entity,
    pub physics: &'static mut RocketPhysicsState,
    pub mission: &'static mut RocketMissionState,
    pub propulsion: &'static mut RocketPropulsion,
    pub commands: &'static mut RocketCommands,
    pub autopilot: &'static mut RocketAutopilot,
    pub flight_conditions: &'static mut RocketFlightConditions,
    pub terrain_collision: &'static mut TerrainCollisionState,
    pub ground_rest: &'static mut GroundRest,
    pub landing_legs: Option<&'static mut LandingLegs>,
    pub thermal: &'static mut ThermalState,
    pub ablation: &'static mut AblationState,
    pub comms: &'static mut CommsState,
    pub parachute: &'static mut ParachuteState,
    pub retro_propulsion: &'static mut RetroPropulsionEffect,
    pub max_q: &'static mut MaxQTracker,
    pub tip_over: &'static mut TipOverState,
    pub landing_scorecard: &'static mut LandingScorecard,
    pub payload_fairing: Option<&'static mut PayloadFairing>,
    pub drone_ship_target: Option<&'static mut DroneShipLandingTarget>,
    pub force_accumulator: &'static mut ForceAccumulator,
    pub torque_accumulator: &'static mut TorqueAccumulator,
    pub specific_force: Option<&'static mut SpecificForceAcceleration>,
    pub render: &'static mut RocketRenderState,
}

/// Authoritative mutable state needed for an already-spawned spent stage.
#[derive(QueryData)]
pub struct ReplaySpentStageCaptureAccess {
    pub entity: Entity,
    pub physics: &'static RocketPhysicsState,
    pub flight_conditions: &'static RocketFlightConditions,
    pub gravity: &'static GravityAcceleration,
    pub force_accumulator: &'static ForceAccumulator,
}

/// Mutable counterpart used only while physics is paused for replay.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct ReplaySpentStageRestoreAccess {
    pub entity: Entity,
    pub physics: &'static mut RocketPhysicsState,
    pub flight_conditions: &'static mut RocketFlightConditions,
    pub gravity: &'static mut GravityAcceleration,
    pub force_accumulator: &'static mut ForceAccumulator,
}

fn capture_frame(
    rockets: &Query<ReplayCaptureAccess>,
    spent_stages: &Query<ReplaySpentStageCaptureAccess, With<SpentStage>>,
    drone_ships: &Query<(Entity, &DroneShip)>,
    timestamp_s: f64,
) -> ReplayFrame {
    ReplayFrame {
        timestamp_s,
        rockets: capture_rockets(rockets, timestamp_s),
        spent_stages: capture_spent_stages(spent_stages),
        drone_ships: capture_drone_ships(drone_ships),
    }
}

fn capture_rockets(
    rockets: &Query<ReplayCaptureAccess>,
    timestamp_s: f64,
) -> Vec<RocketReplaySnapshot> {
    rockets
        .iter()
        .map(|rocket| RocketReplaySnapshot {
            entity: rocket.entity,
            timestamp_s,
            physics: rocket.physics.clone(),
            mission: *rocket.mission,
            propulsion: rocket.propulsion.clone(),
            commands: *rocket.commands,
            autopilot: rocket.autopilot.clone(),
            flight_conditions: *rocket.flight_conditions,
            terrain_collision: *rocket.terrain_collision,
            ground_rest: *rocket.ground_rest,
            landing_legs: rocket.landing_legs.cloned(),
            thermal: *rocket.thermal,
            ablation: *rocket.ablation,
            comms: *rocket.comms,
            parachute: *rocket.parachute,
            retro_propulsion: *rocket.retro_propulsion,
            max_q: *rocket.max_q,
            tip_over: rocket.tip_over.clone(),
            landing_scorecard: rocket.landing_scorecard.clone(),
            payload_fairing: rocket.payload_fairing.cloned(),
            drone_ship_target: rocket.drone_ship_target.copied(),
            force_accumulator: *rocket.force_accumulator,
            torque_accumulator: *rocket.torque_accumulator,
            specific_force: rocket.specific_force.copied(),
        })
        .collect()
}

fn capture_spent_stages(
    spent_stages: &Query<ReplaySpentStageCaptureAccess, With<SpentStage>>,
) -> Vec<SpentStageReplaySnapshot> {
    spent_stages
        .iter()
        .map(|stage| SpentStageReplaySnapshot {
            entity: stage.entity,
            physics: stage.physics.clone(),
            flight_conditions: *stage.flight_conditions,
            gravity: *stage.gravity,
            force_accumulator: *stage.force_accumulator,
        })
        .collect()
}

fn capture_drone_ships(drone_ships: &Query<(Entity, &DroneShip)>) -> Vec<DroneShipReplaySnapshot> {
    drone_ships
        .iter()
        .map(|(entity, ship)| DroneShipReplaySnapshot {
            entity,
            ship: ship.clone(),
        })
        .collect()
}

/// Append one full-authority frame after each live fixed tick.
pub fn record_replay_snapshot_system(
    sim_time: Res<SimulationTime>,
    mut stream: ResMut<ReplaySnapshotStream>,
    rockets: Query<ReplayCaptureAccess>,
    spent_stages: Query<ReplaySpentStageCaptureAccess, With<SpentStage>>,
    drone_ships: Query<(Entity, &DroneShip)>,
) {
    if stream.is_replaying() {
        return;
    }
    stream.push(capture_frame(
        &rockets,
        &spent_stages,
        &drone_ships,
        sim_time.sim_time_s,
    ));
}

fn restore_frame(
    commands: &mut Commands,
    frame: &ReplayFrame,
    rockets: &mut Query<ReplayRestoreAccess>,
) {
    for snapshot in &frame.rockets {
        let landing_legs = snapshot.landing_legs.clone();
        let payload_fairing = snapshot.payload_fairing;
        let drone_ship_target = snapshot.drone_ship_target;
        let entity = {
            let Ok(mut rocket) = rockets.get_mut(snapshot.entity) else {
                continue;
            };

            *rocket.physics = snapshot.physics.clone();
            *rocket.mission = snapshot.mission;
            *rocket.propulsion = snapshot.propulsion.clone();
            *rocket.commands = snapshot.commands;
            *rocket.autopilot = snapshot.autopilot.clone();
            *rocket.flight_conditions = snapshot.flight_conditions;
            *rocket.terrain_collision = snapshot.terrain_collision;
            *rocket.ground_rest = snapshot.ground_rest;
            *rocket.thermal = snapshot.thermal;
            *rocket.ablation = snapshot.ablation;
            *rocket.comms = snapshot.comms;
            *rocket.parachute = snapshot.parachute;
            *rocket.retro_propulsion = snapshot.retro_propulsion;
            *rocket.max_q = snapshot.max_q;
            *rocket.tip_over = snapshot.tip_over.clone();
            *rocket.landing_scorecard = snapshot.landing_scorecard.clone();
            *rocket.force_accumulator = snapshot.force_accumulator;
            *rocket.torque_accumulator = snapshot.torque_accumulator;
            if let (Some(snapshot_specific_force), Some(specific_force)) = (
                snapshot.specific_force,
                rocket.specific_force.as_deref_mut(),
            ) {
                *specific_force = snapshot_specific_force;
            }
            *rocket.render = RocketRenderState::new(snapshot.physics.dynamics);

            if let Some(legs) = rocket.landing_legs.as_deref_mut() {
                if let Some(snapshot_legs) = landing_legs.as_ref() {
                    *legs = snapshot_legs.clone();
                }
            }
            if let Some(fairing) = rocket.payload_fairing.as_deref_mut() {
                if let Some(snapshot_fairing) = payload_fairing {
                    *fairing = snapshot_fairing;
                }
            }
            if let Some(target) = rocket.drone_ship_target.as_deref_mut() {
                if let Some(snapshot_target) = drone_ship_target {
                    *target = snapshot_target;
                }
            }

            rocket.entity
        };
        match landing_legs {
            Some(legs) => commands.entity(entity).insert(legs),
            None => commands.entity(entity).remove::<LandingLegs>(),
        };
        match payload_fairing {
            Some(fairing) => commands.entity(entity).insert(fairing),
            None => commands.entity(entity).remove::<PayloadFairing>(),
        };
        match drone_ship_target {
            Some(target) => commands.entity(entity).insert(target),
            None => commands.entity(entity).remove::<DroneShipLandingTarget>(),
        };
    }
}

fn restore_spent_stages(
    frame: &ReplayFrame,
    spent_stages: &mut Query<ReplaySpentStageRestoreAccess, With<SpentStage>>,
) {
    for snapshot in &frame.spent_stages {
        let Ok(mut stage) = spent_stages.get_mut(snapshot.entity) else {
            continue;
        };
        *stage.physics = snapshot.physics.clone();
        *stage.flight_conditions = snapshot.flight_conditions;
        *stage.gravity = snapshot.gravity;
        *stage.force_accumulator = snapshot.force_accumulator;
    }
}

fn restore_drone_ships(frame: &ReplayFrame, drone_ships: &mut Query<(Entity, &mut DroneShip)>) {
    for snapshot in &frame.drone_ships {
        let Ok((_entity, mut ship)) = drone_ships.get_mut(snapshot.entity) else {
            continue;
        };
        *ship = snapshot.ship.clone();
    }
}

/// Spent hardware is spawned and despawned by another adapter. Re-creating an
/// entity with its original identity would invalidate its parent and deck-target
/// references, so replay only seeks across frames with the same debris and ship
/// entity sets. Attached fairings and their existing parent rockets are restored.
fn spent_stage_lifecycle_is_restorable(
    frame: &ReplayFrame,
    spent_stages: &Query<ReplaySpentStageCaptureAccess, With<SpentStage>>,
) -> bool {
    frame.spent_stages.len() == spent_stages.iter().count()
        && frame
            .spent_stages
            .iter()
            .all(|snapshot| spent_stages.get(snapshot.entity).is_ok())
}

fn drone_ship_lifecycle_is_restorable(
    frame: &ReplayFrame,
    drone_ships: &Query<(Entity, &DroneShip)>,
) -> bool {
    frame.drone_ships.len() == drone_ships.iter().count()
        && frame
            .drone_ships
            .iter()
            .all(|snapshot| drone_ships.get(snapshot.entity).is_ok())
}

/// Restore replay frames together with the frame's authoritative simulation
/// epoch. Planet rotation, terrain and all reference-frame consumers must see
/// the same epoch as the restored vehicle state.
#[expect(
    clippy::type_complexity,
    reason = "The ParamSet separates capture and restore access to replay state."
)]
pub fn apply_replay_actions_system(
    mut commands: Commands,
    mut actions: MessageReader<ReplayAction>,
    mut sim_time: ResMut<SimulationTime>,
    mut stream: ResMut<ReplaySnapshotStream>,
    mut state: ParamSet<(
        Query<ReplayCaptureAccess>,
        Query<ReplayRestoreAccess>,
        Query<ReplaySpentStageCaptureAccess, With<SpentStage>>,
        Query<ReplaySpentStageRestoreAccess, With<SpentStage>>,
        Query<(Entity, &DroneShip)>,
        Query<(Entity, &mut DroneShip)>,
    )>,
) {
    for action in actions.read() {
        match *action {
            ReplayAction::BeginLatest => {
                if stream.is_replaying() {
                    continue;
                }
                let Some(frame_index) = stream.frames.len().checked_sub(1) else {
                    continue;
                };
                let frame = stream.frames[frame_index].clone();
                let spent_stages_restorable =
                    spent_stage_lifecycle_is_restorable(&frame, &state.p2());
                let drone_ships_restorable =
                    drone_ship_lifecycle_is_restorable(&frame, &state.p4());
                if !spent_stages_restorable || !drone_ships_restorable {
                    bevy::log::warn!(
                        "Replay unavailable: spent-stage or drone-ship lifecycle differs from the selected frame"
                    );
                    continue;
                }
                let live_rocket_snapshots = capture_rockets(&state.p0(), sim_time.sim_time_s);
                let live_spent_stage_snapshots = capture_spent_stages(&state.p2());
                let live_drone_ship_snapshots = capture_drone_ships(&state.p4());
                let live_rockets = ReplayFrame {
                    timestamp_s: sim_time.sim_time_s,
                    rockets: live_rocket_snapshots,
                    spent_stages: live_spent_stage_snapshots,
                    drone_ships: live_drone_ship_snapshots,
                };
                stream.session = Some(ReplaySession {
                    live_time_acceleration: sim_time.time_acceleration,
                    live_was_paused: sim_time.paused,
                    live_rockets,
                    selected_frame: frame_index,
                });
                sim_time.paused = true;
                sim_time.sim_time_s = frame.timestamp_s;
                restore_frame(&mut commands, &frame, &mut state.p1());
                restore_spent_stages(&frame, &mut state.p3());
                restore_drone_ships(&frame, &mut state.p5());
            }
            ReplayAction::Seek { frame_index } => {
                if !stream.is_replaying() || frame_index >= stream.frames.len() {
                    continue;
                }
                let frame = stream.frames[frame_index].clone();
                let spent_stages_restorable =
                    spent_stage_lifecycle_is_restorable(&frame, &state.p2());
                let drone_ships_restorable =
                    drone_ship_lifecycle_is_restorable(&frame, &state.p4());
                if !spent_stages_restorable || !drone_ships_restorable {
                    bevy::log::warn!(
                        "Replay seek rejected: spent-stage or drone-ship lifecycle differs from the selected frame"
                    );
                    continue;
                }
                stream
                    .session
                    .as_mut()
                    .expect("replay checked above")
                    .selected_frame = frame_index;
                sim_time.sim_time_s = frame.timestamp_s;
                restore_frame(&mut commands, &frame, &mut state.p1());
                restore_spent_stages(&frame, &mut state.p3());
                restore_drone_ships(&frame, &mut state.p5());
            }
            ReplayAction::Resume => {
                let Some(session) = stream.session.take() else {
                    continue;
                };
                restore_frame(&mut commands, &session.live_rockets, &mut state.p1());
                restore_spent_stages(&session.live_rockets, &mut state.p3());
                restore_drone_ships(&session.live_rockets, &mut state.p5());
                sim_time.sim_time_s = session.live_rockets.timestamp_s;
                sim_time.time_acceleration = session.live_time_acceleration;
                sim_time.paused = session.live_was_paused;
            }
        }
    }
}

/// Run condition for live mutation controls while replay is displaying history.
pub fn replay_inactive(stream: Res<ReplaySnapshotStream>) -> bool {
    !stream.is_replaying()
}

/// Run condition for presentation systems that must derive their data from the
/// currently selected replay snapshot.
pub fn replay_active(stream: Res<ReplaySnapshotStream>) -> bool {
    stream.is_replaying()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::rocket::SpentStageKind;
    use crate::domain::entities::rocket::Rocket;
    use crate::domain::services::atmosphere::FlightConditions;
    use crate::domain::services::guidance::AutopilotMode;
    use crate::domain::services::landing_gear::{LandingGear, LandingGearSpec};
    use crate::domain::services::recovery::{DroneShip as DomainDroneShip, StationKeeper};
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use bevy::math::{DMat3, DQuat, DVec3};

    fn app_with_snapshot_state() -> (App, Entity) {
        let mut app = App::new();
        let mut simulation_time = SimulationTime::new(0.25);
        simulation_time.sim_time_s = 100.0;
        simulation_time.time_acceleration = 10.0;
        app.insert_resource(simulation_time);
        app.insert_resource(ReplaySnapshotStream::new(4));
        app.add_message::<ReplayAction>();
        app.add_systems(FixedUpdate, record_replay_snapshot_system);
        app.add_systems(Update, apply_replay_actions_system);

        let dynamics = RocketDynamicsState {
            position_m: DVec3::new(1.0, 2.0, 3.0),
            velocity_mps: DVec3::new(4.0, 5.0, 6.0),
            orientation: DQuat::from_rotation_y(0.25),
            angular_velocity_radps: DVec3::new(0.1, 0.2, 0.3),
            angular_acceleration_radps2: DVec3::new(0.4, 0.5, 0.6),
            mass_kg: 1_000.0,
            inertia_body: DMat3::IDENTITY,
            center_of_mass_m: DVec3::new(0.0, -2.0, 0.0),
        };
        let conditions = FlightConditions {
            altitude_m: 12_345.0,
            dynamic_pressure_pa: 678.0,
            ..Default::default()
        };
        let gear = LandingGear::new(
            LandingGearSpec {
                count: 4,
                base_radius_m: 2.0,
                stroke_m: 1.0,
                max_landing_mass_kg: None,
                deploy_altitude_m: 100.0,
            },
            1_000.0,
        );
        let mut vehicle = Rocket::falcon9_test_fixture();
        vehicle.stages[1].fairing_dry_mass_kg = Some(6.0);
        let entity = app
            .world_mut()
            .spawn((
                RocketPhysicsState { dynamics },
                RocketMissionState::Landing,
                RocketPropulsion {
                    vehicle,
                    active_stage: 1,
                    propellant_remaining_kg: vec![1.0, 2.0],
                    booster_propellant_remaining_kg: Vec::new(),
                    boosters_attached: false,
                    throttle: 0.7,
                    gimbal_pitch_rad: 0.1,
                    gimbal_yaw_rad: -0.2,
                    time_since_separation_s: 3.0,
                    ullage_settle_time_s: 0.5,
                    separations_count: 1,
                    attached_payload_kg: 6.0,
                },
                RocketCommands {
                    target_attitude: DQuat::from_rotation_x(0.5),
                    throttle_cmd: 0.8,
                    gimbal_pitch_cmd_rad: 0.2,
                    gimbal_yaw_cmd_rad: -0.3,
                    rcs_torque_cmd_body: DVec3::new(7.0, 8.0, 9.0),
                },
                RocketAutopilot {
                    integral: DVec3::new(1.0, 2.0, 3.0),
                    mode: AutopilotMode::Boostback,
                    time_since_liftoff_s: 42.0,
                    ..default()
                },
                RocketFlightConditions(conditions),
                TerrainCollisionState {
                    radar_altitude_m: 12.0,
                    slope_deg: 3.0,
                    over_water: true,
                    ..default()
                },
                GroundRest { active: true },
                LandingLegs {
                    gear,
                    deployment: crate::domain::services::landing_gear::LegDeploymentState {
                        deployed: true,
                    },
                    compression_m: 0.4,
                },
                ThermalState {
                    total_heat_flux_w_m2: 44.0,
                    ..default()
                },
                AblationState {
                    mass_loss_kg: 5.0,
                    ..default()
                },
                CommsState { in_blackout: true },
                ParachuteState {
                    deployment: crate::domain::services::entry_physics::ParachuteDeploymentState {
                        drogue_deployed: true,
                        current_cd: 1.5,
                        ..default()
                    },
                    canopy_attach_point_body: DVec3::new(0.0, -3.0, 0.0),
                },
                RetroPropulsionEffect {
                    thrust_multiplier: 0.9,
                },
            ))
            .id();
        app.world_mut().entity_mut(entity).insert((
            MaxQTracker { max_q_pa: 999.0 },
            TipOverState {
                exceeded_for_s: 0.2,
                fall: None,
                com_height_m: 10.0,
            },
            LandingScorecard {
                touchdown_vertical_speed_mps: 1.0,
                recorded: true,
                ..default()
            },
            PayloadFairing { dry_mass_kg: 6.0 },
            ForceAccumulator(DVec3::new(10.0, 11.0, 12.0)),
            TorqueAccumulator(DVec3::new(13.0, 14.0, 15.0)),
            RocketRenderState::new(dynamics),
        ));
        (app, entity)
    }

    fn empty_frame(timestamp_s: f64) -> ReplayFrame {
        ReplayFrame {
            timestamp_s,
            rockets: Vec::new(),
            spent_stages: Vec::new(),
            drone_ships: Vec::new(),
        }
    }

    fn spawn_spent_stage(app: &mut App, parent_rocket: Entity) -> Entity {
        let dynamics = RocketDynamicsState {
            position_m: DVec3::new(7.0, 8.0, 9.0),
            velocity_mps: DVec3::new(10.0, 11.0, 12.0),
            orientation: DQuat::IDENTITY,
            angular_velocity_radps: DVec3::ZERO,
            angular_acceleration_radps2: DVec3::ZERO,
            mass_kg: 50.0,
            inertia_body: DMat3::IDENTITY,
            center_of_mass_m: DVec3::ZERO,
        };
        app.world_mut()
            .spawn((
                SpentStage {
                    parent_rocket,
                    kind: SpentStageKind::Booster,
                },
                RocketPhysicsState { dynamics },
                RocketFlightConditions(FlightConditions {
                    altitude_m: 1_000.0,
                    ..default()
                }),
                GravityAcceleration {
                    value: DVec3::new(0.0, -9.0, 0.0),
                },
                ForceAccumulator(DVec3::new(1.0, 2.0, 3.0)),
            ))
            .id()
    }

    fn spawn_drone_ship(app: &mut App) -> Entity {
        app.world_mut()
            .spawn(DroneShip {
                state: DomainDroneShip {
                    position_m: DVec3::new(100.0, 200.0, 300.0),
                    velocity_mps: DVec3::new(4.0, 5.0, 6.0),
                    external_accel_mps2: DVec3::new(0.1, 0.2, 0.3),
                    mass_kg: 1_000.0,
                },
                station_target_position_m: DVec3::new(101.0, 201.0, 301.0),
                station_keeper: StationKeeper {
                    kp: 0.1,
                    kd: 0.2,
                    max_thrust_n: 3_000.0,
                },
                deck_half_extent_m: 20.0,
            })
            .id()
    }

    #[test]
    fn snapshot_stream_retains_chronological_capacity_without_front_shifts() {
        let mut stream = ReplaySnapshotStream::new(2);
        stream.push(empty_frame(1.0));
        stream.push(empty_frame(2.0));
        stream.push(empty_frame(3.0));

        assert_eq!(stream.frames().len(), 2);
        assert_eq!(stream.frames()[0].timestamp_s, 2.0);
        assert_eq!(stream.frames()[1].timestamp_s, 3.0);
    }

    #[test]
    fn replay_restores_stable_spent_stage_and_drone_ship_state() {
        let (mut app, rocket) = app_with_snapshot_state();
        let spent_stage = spawn_spent_stage(&mut app, rocket);
        let drone_ship = spawn_drone_ship(&mut app);
        app.world_mut()
            .entity_mut(rocket)
            .insert(DroneShipLandingTarget {
                drone_ship,
                prediction_horizon_s: 15.0,
                deck_contact: true,
            });
        app.world_mut().run_schedule(FixedUpdate);

        {
            let mut stage = app.world_mut().entity_mut(spent_stage);
            stage
                .get_mut::<RocketPhysicsState>()
                .unwrap()
                .dynamics
                .position_m = DVec3::ZERO;
            stage
                .get_mut::<RocketPhysicsState>()
                .unwrap()
                .dynamics
                .mass_kg = 1.0;
            stage
                .get_mut::<RocketFlightConditions>()
                .unwrap()
                .0
                .altitude_m = 0.0;
            stage.get_mut::<GravityAcceleration>().unwrap().value = DVec3::ZERO;
            stage.get_mut::<ForceAccumulator>().unwrap().0 = DVec3::ZERO;
        }
        app.world_mut()
            .entity_mut(drone_ship)
            .get_mut::<DroneShip>()
            .unwrap()
            .state
            .position_m = DVec3::ZERO;
        app.world_mut()
            .entity_mut(rocket)
            .get_mut::<DroneShipLandingTarget>()
            .unwrap()
            .deck_contact = false;

        app.world_mut()
            .resource_mut::<Messages<ReplayAction>>()
            .write(ReplayAction::BeginLatest);
        app.update();

        let stage = app.world().entity(spent_stage);
        assert_eq!(
            stage
                .get::<RocketPhysicsState>()
                .unwrap()
                .dynamics
                .position_m,
            DVec3::new(7.0, 8.0, 9.0)
        );
        assert_eq!(
            stage.get::<RocketPhysicsState>().unwrap().dynamics.mass_kg,
            50.0
        );
        assert_eq!(
            stage.get::<RocketFlightConditions>().unwrap().altitude_m,
            1_000.0
        );
        assert_eq!(
            stage.get::<GravityAcceleration>().unwrap().value,
            DVec3::new(0.0, -9.0, 0.0)
        );
        assert_eq!(
            stage.get::<ForceAccumulator>().unwrap().0,
            DVec3::new(1.0, 2.0, 3.0)
        );
        assert_eq!(
            app.world()
                .entity(drone_ship)
                .get::<DroneShip>()
                .unwrap()
                .state
                .position_m,
            DVec3::new(100.0, 200.0, 300.0)
        );
        assert!(
            app.world()
                .entity(rocket)
                .get::<DroneShipLandingTarget>()
                .unwrap()
                .deck_contact
        );
    }

    #[test]
    fn replay_rejects_seeks_across_spent_stage_lifecycle_changes() {
        let (mut app, rocket) = app_with_snapshot_state();
        app.world_mut().run_schedule(FixedUpdate);
        let spent_stage = spawn_spent_stage(&mut app, rocket);
        app.world_mut().run_schedule(FixedUpdate);

        app.world_mut()
            .resource_mut::<Messages<ReplayAction>>()
            .write(ReplayAction::BeginLatest);
        app.update();
        assert_eq!(
            app.world()
                .resource::<ReplaySnapshotStream>()
                .selected_frame(),
            Some(1)
        );

        app.world_mut()
            .resource_mut::<Messages<ReplayAction>>()
            .write(ReplayAction::Seek { frame_index: 0 });
        app.update();

        assert_eq!(
            app.world()
                .resource::<ReplaySnapshotStream>()
                .selected_frame(),
            Some(1)
        );
        assert!(app.world().get_entity(spent_stage).is_ok());
    }

    #[test]
    fn replay_snapshot_round_trip_restores_authoritative_state_exactly() {
        let (mut app, entity) = app_with_snapshot_state();
        app.world_mut().run_schedule(FixedUpdate);

        {
            let mut rocket = app.world_mut().entity_mut(entity);
            rocket
                .get_mut::<RocketPhysicsState>()
                .unwrap()
                .dynamics
                .position_m = DVec3::splat(99.0);
            rocket
                .get_mut::<RocketPhysicsState>()
                .unwrap()
                .dynamics
                .mass_kg = 99.0;
            rocket.get_mut::<RocketPropulsion>().unwrap().throttle = 0.0;
            let engine =
                &mut rocket.get_mut::<RocketPropulsion>().unwrap().vehicle.stages[0].engines[0];
            engine.ignition_count = engine.max_ignitions;
            engine.state = crate::domain::entities::rocket::EngineState::Depleted;
            rocket.get_mut::<RocketCommands>().unwrap().throttle_cmd = 0.0;
            rocket.get_mut::<RocketAutopilot>().unwrap().integral = DVec3::ZERO;
            rocket
                .get_mut::<RocketFlightConditions>()
                .unwrap()
                .0
                .altitude_m = 0.0;
            rocket
                .get_mut::<TerrainCollisionState>()
                .unwrap()
                .radar_altitude_m = 0.0;
            rocket.get_mut::<GroundRest>().unwrap().active = false;
            rocket.get_mut::<LandingLegs>().unwrap().compression_m = 0.0;
            rocket
                .get_mut::<ThermalState>()
                .unwrap()
                .total_heat_flux_w_m2 = 0.0;
            rocket.get_mut::<AblationState>().unwrap().mass_loss_kg = 0.0;
            rocket.get_mut::<CommsState>().unwrap().in_blackout = false;
            rocket
                .get_mut::<ParachuteState>()
                .unwrap()
                .deployment
                .drogue_deployed = false;
            rocket
                .get_mut::<RetroPropulsionEffect>()
                .unwrap()
                .thrust_multiplier = 1.0;
            rocket.get_mut::<MaxQTracker>().unwrap().max_q_pa = 0.0;
            rocket.get_mut::<TipOverState>().unwrap().com_height_m = 0.0;
            rocket
                .get_mut::<LandingScorecard>()
                .unwrap()
                .touchdown_vertical_speed_mps = 0.0;
            rocket.get_mut::<ForceAccumulator>().unwrap().0 = DVec3::ZERO;
            rocket.get_mut::<TorqueAccumulator>().unwrap().0 = DVec3::ZERO;
            rocket.remove::<PayloadFairing>();
        }
        app.world_mut()
            .resource_mut::<Messages<ReplayAction>>()
            .write(ReplayAction::BeginLatest);
        app.update();

        let rocket = app.world().entity(entity);
        assert_eq!(
            rocket
                .get::<RocketPhysicsState>()
                .unwrap()
                .dynamics
                .position_m,
            DVec3::new(1.0, 2.0, 3.0)
        );
        assert_eq!(
            rocket.get::<RocketPhysicsState>().unwrap().dynamics.mass_kg,
            1_000.0
        );
        assert_eq!(rocket.get::<RocketPropulsion>().unwrap().throttle, 0.7);
        let engine = &rocket.get::<RocketPropulsion>().unwrap().vehicle.stages[0].engines[0];
        assert_eq!(engine.ignition_count, 1);
        assert_eq!(
            engine.state,
            crate::domain::entities::rocket::EngineState::Running
        );
        assert_eq!(rocket.get::<RocketCommands>().unwrap().throttle_cmd, 0.8);
        assert_eq!(
            rocket.get::<RocketAutopilot>().unwrap().integral,
            DVec3::new(1.0, 2.0, 3.0)
        );
        assert_eq!(
            rocket.get::<ThermalState>().unwrap().total_heat_flux_w_m2,
            44.0
        );
        assert_eq!(
            rocket.get::<RocketFlightConditions>().unwrap().altitude_m,
            12_345.0
        );
        assert_eq!(
            rocket
                .get::<TerrainCollisionState>()
                .unwrap()
                .radar_altitude_m,
            12.0
        );
        assert!(rocket.get::<GroundRest>().unwrap().active);
        assert_eq!(rocket.get::<LandingLegs>().unwrap().compression_m, 0.4);
        assert_eq!(rocket.get::<AblationState>().unwrap().mass_loss_kg, 5.0);
        assert!(rocket.get::<CommsState>().unwrap().in_blackout);
        assert!(
            rocket
                .get::<ParachuteState>()
                .unwrap()
                .deployment
                .drogue_deployed
        );
        assert_eq!(
            rocket
                .get::<RetroPropulsionEffect>()
                .unwrap()
                .thrust_multiplier,
            0.9
        );
        assert_eq!(rocket.get::<MaxQTracker>().unwrap().max_q_pa, 999.0);
        assert_eq!(rocket.get::<TipOverState>().unwrap().com_height_m, 10.0);
        assert_eq!(
            rocket
                .get::<LandingScorecard>()
                .unwrap()
                .touchdown_vertical_speed_mps,
            1.0
        );
        assert_eq!(
            rocket.get::<ForceAccumulator>().unwrap().0,
            DVec3::new(10.0, 11.0, 12.0)
        );
        assert_eq!(
            rocket.get::<TorqueAccumulator>().unwrap().0,
            DVec3::new(13.0, 14.0, 15.0)
        );
        assert_eq!(rocket.get::<PayloadFairing>().unwrap().dry_mass_kg, 6.0);
        assert_eq!(
            rocket
                .get::<RocketPropulsion>()
                .unwrap()
                .attached_payload_kg,
            6.0
        );
        assert_eq!(
            rocket.get::<RocketPropulsion>().unwrap().vehicle.stages[1].fairing_dry_mass_kg,
            Some(6.0)
        );
    }

    #[test]
    fn seek_restores_the_selected_simulation_epoch_and_resume_restores_live_epoch() {
        let (mut app, _) = app_with_snapshot_state();
        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut()
            .resource_mut::<ReplaySnapshotStream>()
            .frames[0]
            .timestamp_s = 1.0;
        app.world_mut()
            .resource_mut::<ReplaySnapshotStream>()
            .push(empty_frame(2.0));
        app.world_mut()
            .resource_mut::<Messages<ReplayAction>>()
            .write(ReplayAction::BeginLatest);
        app.update();
        app.world_mut()
            .resource_mut::<Messages<ReplayAction>>()
            .write(ReplayAction::Seek { frame_index: 0 });
        app.update();

        let sim_time = app.world().resource::<SimulationTime>();
        assert_eq!(sim_time.sim_time_s, 1.0);
        assert!(sim_time.paused);

        app.world_mut()
            .resource_mut::<Messages<ReplayAction>>()
            .write(ReplayAction::Resume);
        app.update();
        let sim_time = app.world().resource::<SimulationTime>();
        assert_eq!(sim_time.sim_time_s, 100.0);
        assert_eq!(sim_time.time_acceleration, 10.0);
        assert!(!sim_time.paused);
    }
}
