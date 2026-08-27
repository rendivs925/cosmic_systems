//! Fixed-tick authoritative replay snapshots for rocket flight.
//!
//! This deliberately does not reuse `FlightRecorder`: telemetry is sampled at
//! a lower rate for analysis, while replay must restore every state value that
//! can affect a subsequent physics tick.

use crate::components::rocket::{
    AblationState, CommsState, ForceAccumulator, GroundRest, LandingLegs, LandingScorecard,
    MaxQTracker, ParachuteState, PayloadFairing, RetroPropulsionEffect, RocketAutopilot,
    RocketCommands, RocketFlightConditions, RocketMass, RocketMissionState, RocketPhysicsState,
    RocketPropulsion, RocketRenderState, TerrainCollisionState, ThermalState, TipOverState,
    TorqueAccumulator,
};
use crate::domain::services::simulation_time::SimulationTime;
use bevy::ecs::query::QueryData;
use bevy::prelude::*;

/// One minute of exact 60 Hz history by default. Call
/// [`ReplaySnapshotStream::new`] to select a different retention capacity.
pub const DEFAULT_REPLAY_SNAPSHOT_CAPACITY: usize = 3_600;

/// A complete authoritative state for one rocket at one fixed simulation tick.
#[derive(Debug, Clone)]
pub struct RocketReplaySnapshot {
    pub entity: Entity,
    pub timestamp_s: f64,
    pub physics: RocketPhysicsState,
    pub mass: RocketMass,
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
    pub force_accumulator: ForceAccumulator,
    pub torque_accumulator: TorqueAccumulator,
}

/// All rocket snapshots captured at one completed fixed tick.
#[derive(Debug, Clone)]
pub struct ReplayFrame {
    pub timestamp_s: f64,
    pub rockets: Vec<RocketReplaySnapshot>,
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
    frames: Vec<ReplayFrame>,
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
            frames: Vec::with_capacity(capacity),
            capacity,
            session: None,
        }
    }

    pub fn frames(&self) -> &[ReplayFrame] {
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
            self.frames.remove(0);
        }
        self.frames.push(frame);
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
    pub mass: &'static RocketMass,
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
    pub force_accumulator: &'static ForceAccumulator,
    pub torque_accumulator: &'static TorqueAccumulator,
}

/// Mutable counterpart used only while physics is paused for replay.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct ReplayRestoreAccess {
    pub entity: Entity,
    pub physics: &'static mut RocketPhysicsState,
    pub mass: &'static mut RocketMass,
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
    pub force_accumulator: &'static mut ForceAccumulator,
    pub torque_accumulator: &'static mut TorqueAccumulator,
    pub render: &'static mut RocketRenderState,
}

fn capture_frame(rockets: &Query<ReplayCaptureAccess>, timestamp_s: f64) -> ReplayFrame {
    ReplayFrame {
        timestamp_s,
        rockets: rockets
            .iter()
            .map(|rocket| RocketReplaySnapshot {
                entity: rocket.entity,
                timestamp_s,
                physics: rocket.physics.clone(),
                mass: *rocket.mass,
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
                force_accumulator: *rocket.force_accumulator,
                torque_accumulator: *rocket.torque_accumulator,
            })
            .collect(),
    }
}

/// Append one full-authority frame after each live fixed tick.
pub fn record_replay_snapshot_system(
    sim_time: Res<SimulationTime>,
    mut stream: ResMut<ReplaySnapshotStream>,
    rockets: Query<ReplayCaptureAccess>,
) {
    if stream.is_replaying() {
        return;
    }
    stream.push(capture_frame(&rockets, sim_time.sim_time_s));
}

fn restore_frame(
    commands: &mut Commands,
    frame: &ReplayFrame,
    rockets: &mut Query<ReplayRestoreAccess>,
) {
    for snapshot in &frame.rockets {
        let landing_legs = snapshot.landing_legs.clone();
        let payload_fairing = snapshot.payload_fairing;
        let entity = {
            let Ok(mut rocket) = rockets.get_mut(snapshot.entity) else {
                continue;
            };

            *rocket.physics = snapshot.physics.clone();
            *rocket.mass = snapshot.mass;
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
    }
}

/// Restore replay frames while keeping the authoritative live clock monotonic.
pub fn apply_replay_actions_system(
    mut commands: Commands,
    mut actions: MessageReader<ReplayAction>,
    mut sim_time: ResMut<SimulationTime>,
    mut stream: ResMut<ReplaySnapshotStream>,
    mut rockets: ParamSet<(Query<ReplayCaptureAccess>, Query<ReplayRestoreAccess>)>,
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
                let live_rockets = capture_frame(&rockets.p0(), sim_time.sim_time_s);
                stream.session = Some(ReplaySession {
                    live_time_acceleration: sim_time.time_acceleration,
                    live_was_paused: sim_time.paused,
                    live_rockets,
                    selected_frame: frame_index,
                });
                sim_time.paused = true;
                restore_frame(&mut commands, &frame, &mut rockets.p1());
            }
            ReplayAction::Seek { frame_index } => {
                if !stream.is_replaying() || frame_index >= stream.frames.len() {
                    continue;
                }
                let frame = stream.frames[frame_index].clone();
                stream
                    .session
                    .as_mut()
                    .expect("replay checked above")
                    .selected_frame = frame_index;
                restore_frame(&mut commands, &frame, &mut rockets.p1());
            }
            ReplayAction::Resume => {
                let Some(session) = stream.session.take() else {
                    continue;
                };
                restore_frame(&mut commands, &session.live_rockets, &mut rockets.p1());
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
    use crate::domain::entities::rocket::Rocket;
    use crate::domain::services::atmosphere::FlightConditions;
    use crate::domain::services::guidance::AutopilotMode;
    use crate::domain::services::landing_gear::{LandingGear, LandingGearSpec};
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use bevy::math::{DMat3, DQuat, DVec3};

    fn app_with_snapshot_state() -> (App, Entity) {
        let mut app = App::new();
        app.insert_resource(SimulationTime {
            sim_time_s: 100.0,
            time_acceleration: 10.0,
            paused: false,
            ..SimulationTime::new(0.25)
        });
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
        let mut conditions = FlightConditions::default();
        conditions.altitude_m = 12_345.0;
        conditions.dynamic_pressure_pa = 678.0;
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
        let entity = app
            .world_mut()
            .spawn((
                RocketPhysicsState { dynamics },
                RocketMass(1_000.0),
                RocketMissionState::Landing,
                RocketPropulsion {
                    vehicle: Rocket::falcon9(),
                    active_stage: 1,
                    propellant_remaining_kg: vec![1.0, 2.0],
                    throttle: 0.7,
                    gimbal_pitch_rad: 0.1,
                    gimbal_yaw_rad: -0.2,
                    time_since_separation_s: 3.0,
                    ullage_settle_time_s: 0.5,
                    separations_count: 1,
                    attached_payload_kg: 4.0,
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
            rocket.get_mut::<RocketMass>().unwrap().0 = 99.0;
            rocket.get_mut::<RocketPropulsion>().unwrap().throttle = 0.0;
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
        assert_eq!(rocket.get::<RocketMass>().unwrap().0, 1_000.0);
        assert_eq!(rocket.get::<RocketPropulsion>().unwrap().throttle, 0.7);
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
    }

    #[test]
    fn seek_never_rewinds_the_live_simulation_clock() {
        let (mut app, _) = app_with_snapshot_state();
        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut()
            .resource_mut::<ReplaySnapshotStream>()
            .frames[0]
            .timestamp_s = 1.0;
        app.world_mut()
            .resource_mut::<Messages<ReplayAction>>()
            .write(ReplayAction::BeginLatest);
        app.update();
        app.world_mut()
            .resource_mut::<Messages<ReplayAction>>()
            .write(ReplayAction::Seek { frame_index: 0 });
        app.update();

        let sim_time = app.world().resource::<SimulationTime>();
        assert_eq!(sim_time.sim_time_s, 100.0);
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
