// Rocket telemetry computation - encapsulated, trait-based design.

use crate::components::rocket::*;
use crate::domain::entities::rocket::EngineState;
use crate::domain::events::{
    CommsBlackoutEvent, FairingSeparatedEvent, SplashdownDetectedEvent, StageSeparatedEvent,
};
use crate::domain::services::aerodynamics::{
    angle_of_attack, angle_of_sideslip, dynamic_pressure_q,
};
use crate::domain::services::gravity::gravitational_parameter;
use crate::domain::services::rocket_propulsion::{
    active_vehicle_mass_with_payload, stage_thrust_body, STANDARD_GRAVITY_MPS2,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::components::{
    PlanetAtmosphere, PlanetComponent, Selectable,
};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

/// Trait for computing telemetry from rocket state.
/// Allows different implementations (realtime, recorded, simulated).
pub trait TelemetryComputer<'a> {
    type Output;

    fn compute(&self, ctx: &TelemetryContext<'a>) -> Self::Output;
}

/// Context containing all data needed for telemetry computation.
#[derive(Debug, Clone)]
pub struct TelemetryContext<'a> {
    pub sim_time: f64,
    pub dt: f64,
    pub planet_mass: f64,
    pub planet_radius_m: f64,
    pub position_m: DVec3,
    pub velocity_mps: DVec3,
    pub orientation: DQuat,
    pub angular_velocity_radps: DVec3,
    pub mass_kg: f64,
    pub rocket_mass: &'a RocketMass,
    pub geometry: &'a RocketGeometry,
    pub propulsion: &'a RocketPropulsion,
    pub mission_state: &'a RocketMissionState,
    pub autopilot: &'a RocketAutopilot,
    pub orbital: &'a OrbitalElements,
    pub atmosphere: &'a AtmosphereState,
    pub comms: &'a CommsState,
    pub aero_forces: &'a AerodynamicForces,
    pub thermal: &'a ThermalState,
    pub ablation: &'a AblationState,
    pub parachute: &'a ParachuteState,
    pub collision: &'a TerrainCollisionState,
}

impl<'a> TelemetryContext<'a> {
    /// Compute derived values used by multiple telemetry fields.
    fn derived(&self) -> DerivedTelemetry {
        let radius = self.position_m.length();
        if radius < 1.0 {
            return DerivedTelemetry::default();
        }

        let up_dir = self.position_m / radius;
        let altitude_m = (radius - self.planet_radius_m).max(0.0);
        let speed = self.velocity_mps.length();
        let vertical_speed = self.velocity_mps.dot(up_dir);
        let horizontal_speed = (speed * speed - vertical_speed * vertical_speed)
            .max(0.0)
            .sqrt();

        let rho = self.atmosphere.density_kg_m3;
        let sos = self.atmosphere.speed_of_sound_mps.max(1.0);
        let mach = speed / sos;
        let q = dynamic_pressure_q(rho, speed);

        let mu = gravitational_parameter(self.planet_mass);
        let gravity_accel = mu / (radius * radius);
        let weight = self.mass_kg * gravity_accel;

        let (thrust_body, isp_vac) = self.compute_thrust(rho);
        let total_thrust_n = thrust_body.length();
        let tw_ratio = if weight > 0.0 {
            total_thrust_n / weight
        } else {
            0.0
        };

        // Sensed load factor: magnitude of the non-gravitational specific
        // force (thrust + aerodynamics), divided by standard gravity. An
        // accelerometer reads exactly this; a coasting vehicle in vacuum
        // reads 0 regardless of orbital speed.
        let sensed_force_world = self.orientation * (thrust_body + self.aero_forces.force_body);
        let g_load = sensed_force_world.length() / (self.mass_kg * STANDARD_GRAVITY_MPS2);

        let body_velocity = self.orientation.inverse() * self.velocity_mps;
        let aoa = angle_of_attack(body_velocity).to_degrees();
        let aos = angle_of_sideslip(body_velocity).to_degrees();

        let ang_vel = self.angular_velocity_radps;
        let roll_rate = ang_vel.y.to_degrees();
        let pitch_rate = ang_vel.x.to_degrees();
        let yaw_rate = ang_vel.z.to_degrees();

        // Bank angle: roll relative to the local horizontal frame (north-up-east).
        // In the local frame: up = radial, north = perpendicular to up and velocity,
        // east = up × north. Bank is the angle of body X axis projected onto
        // the horizontal plane, relative to north.
        let body_x = self.orientation * DVec3::X;
        let body_x_horizontal = body_x - body_x.dot(up_dir) * up_dir;
        // Compute local north: perpendicular to up and velocity (or arbitrary if speed ~0).
        let north = if body_velocity.length_squared() > 1e-6 {
            up_dir.cross(body_velocity).normalize_or_zero()
        } else {
            // At rest on pad, use an arbitrary but consistent north (perpendicular to up).
            if up_dir.z.abs() < 0.9 {
                up_dir.cross(DVec3::Z).normalize_or_zero()
            } else {
                up_dir.cross(DVec3::X).normalize_or_zero()
            }
        };
        let bank = if body_x_horizontal.length_squared() > 1e-6 && north.length_squared() > 1e-6 {
            // Angle between body X horizontal and local north, signed by east component.
            let angle = body_x_horizontal.angle_between(north).to_degrees();
            // Sign: positive = right wing down (body X east of north).
            let east = up_dir.cross(north);
            if body_x_horizontal.dot(east) < 0.0 {
                -angle
            } else {
                angle
            }
        } else {
            0.0
        };

        let (dry_mass, propellant_fraction) = self.compute_mass_properties();

        let delta_v = Self::compute_delta_v(self.mass_kg, dry_mass, isp_vac);

        // Blackout authority is the CommsState component written by
        // compute_plasma_blackout; telemetry only mirrors it here.
        let plasma_blackout = self.comms.in_blackout;

        DerivedTelemetry {
            up_dir,
            altitude_m,
            speed,
            vertical_speed,
            horizontal_speed,
            mach,
            q,
            gravity_accel,
            weight,
            total_thrust_n,
            isp_vac,
            tw_ratio,
            g_load,
            dry_mass,
            propellant_fraction,
            delta_v,
            aoa,
            aos,
            roll_rate,
            pitch_rate,
            yaw_rate,
            bank,
            plasma_blackout,
        }
    }

    fn compute_thrust(&self, rho: f64) -> (DVec3, f64) {
        if self.propulsion.active_stage >= self.propulsion.vehicle.stages.len() {
            return (DVec3::ZERO, 0.0);
        }
        let stage = &self.propulsion.vehicle.stages[self.propulsion.active_stage];
        let throttle = self.propulsion.throttle.clamp(0.0, 1.0);
        let remaining = self
            .propulsion
            .propellant_remaining_kg
            .get(self.propulsion.active_stage)
            .copied()
            .unwrap_or(0.0);
        if throttle <= 0.0 || remaining <= 0.0 {
            return (DVec3::ZERO, 0.0);
        }
        let (thrust_body, _) = stage_thrust_body(&stage.engines, throttle, rho);
        let isp_vac = stage
            .engines
            .iter()
            .find(|e| e.state == EngineState::Running)
            .map(|e| e.isp_vacuum as f64)
            .unwrap_or(0.0);
        (thrust_body, isp_vac)
    }

    fn compute_mass_properties(&self) -> (f64, f64) {
        // Attached payload hardware (fairing) counts as shed structure, so it
        // belongs to the dry mass of the active stack until jettison.
        let dry_mass = active_vehicle_mass_with_payload(
            &self.propulsion.vehicle.stages,
            &self.propulsion.propellant_remaining_kg,
            self.propulsion.active_stage,
            self.propulsion.attached_payload_kg,
        ) - self.propulsion.propellant_remaining_kg.iter().sum::<f32>() as f64;

        let total_propellant_initial: f64 = self
            .propulsion
            .vehicle
            .stages
            .iter()
            .map(|s| s.propellant_mass_kg as f64)
            .sum();
        let total_propellant_remaining: f64 =
            self.propulsion.propellant_remaining_kg.iter().sum::<f32>() as f64;
        let propellant_fraction = if total_propellant_initial > 0.0 {
            total_propellant_remaining / total_propellant_initial
        } else {
            0.0
        };
        (dry_mass, propellant_fraction)
    }

    fn compute_delta_v(initial_mass: f64, dry_mass: f64, isp_vac: f64) -> f64 {
        if initial_mass > dry_mass && dry_mass > 0.0 && isp_vac > 0.0 {
            isp_vac * STANDARD_GRAVITY_MPS2 * (initial_mass / dry_mass).ln()
        } else {
            0.0
        }
    }
}

/// Pre-computed derived telemetry values to avoid duplication.
#[derive(Debug, Clone, Default)]
struct DerivedTelemetry {
    up_dir: DVec3,
    altitude_m: f64,
    speed: f64,
    vertical_speed: f64,
    horizontal_speed: f64,
    mach: f64,
    q: f64,
    gravity_accel: f64,
    weight: f64,
    total_thrust_n: f64,
    isp_vac: f64,
    tw_ratio: f64,
    g_load: f64,
    dry_mass: f64,
    propellant_fraction: f64,
    delta_v: f64,
    aoa: f64,
    aos: f64,
    roll_rate: f64,
    pitch_rate: f64,
    yaw_rate: f64,
    bank: f64,
    plasma_blackout: bool,
}

/// Real-time telemetry computer that writes to RocketTelemetry resource.
pub struct RealtimeTelemetryComputer;

impl<'a> TelemetryComputer<'a> for RealtimeTelemetryComputer {
    type Output = ();

    fn compute(&self, ctx: &TelemetryContext<'a>) -> Self::Output {
        let d = ctx.derived();

        // Write to resource - this is the side effect
        // In a real system, this would be done via Bevy's ResMut in a system
        // Here we just compute; the system handles the write
    }
}

/// Compute telemetry from context into RocketTelemetry.
/// Pure function - no side effects, easy to test.
pub fn compute_telemetry_from_context<'a>(ctx: &TelemetryContext<'a>) -> RocketTelemetry {
    let d = ctx.derived();

    RocketTelemetry {
        altitude_agl_m: ctx.collision.radar_altitude_m,
        altitude_msl_m: d.altitude_m,
        velocity_total_mps: d.speed,
        velocity_vertical_mps: d.vertical_speed,
        velocity_horizontal_mps: d.horizontal_speed,
        mach_number: d.mach,
        dynamic_pressure_pa: d.q,
        g_load: d.g_load,
        apoapsis_altitude_m: ctx.orbital.apoapsis_m - ctx.planet_radius_m,
        periapsis_altitude_m: ctx.orbital.periapsis_m - ctx.planet_radius_m,
        tw_ratio: d.tw_ratio,
        delta_v_remaining_mps: d.delta_v,
        propellant_fraction: d.propellant_fraction,
        active_stage: ctx.propulsion.active_stage,
        mission_phase: *ctx.mission_state,
        total_thrust_n: d.total_thrust_n,
        mass_kg: ctx.mass_kg,
        isp_vacuum_s: d.isp_vac,
        angle_of_attack_deg: d.aoa,
        sideslip_angle_deg: d.aos,
        bank_angle_deg: d.bank,
        roll_rate_dps: d.roll_rate,
        pitch_rate_dps: d.pitch_rate,
        yaw_rate_dps: d.yaw_rate,
        throttle: ctx.propulsion.throttle,
        gimbal_pitch_deg: ctx.propulsion.gimbal_pitch_rad.to_degrees(),
        gimbal_yaw_deg: ctx.propulsion.gimbal_yaw_rad.to_degrees(),
        radar_altitude_m: ctx.collision.radar_altitude_m,
        terrain_slope_deg: ctx.collision.slope_deg,
        ground_contact: ctx.collision.ground_contact,
        convective_heat_flux_w_m2: ctx.thermal.convective_heat_flux_w_m2,
        radiative_heat_flux_w_m2: ctx.thermal.radiative_heat_flux_w_m2,
        total_heat_flux_w_m2: ctx.thermal.total_heat_flux_w_m2,
        nose_radius_m: ctx.ablation.nose_radius_m,
        tps_thickness_remaining_m: ctx.ablation.tps_thickness_remaining_m,
        plasma_blackout: d.plasma_blackout,
        drogue_deployed: ctx.parachute.deployment.drogue_deployed,
        main_deployed: ctx.parachute.deployment.main_deployed,
        over_water: ctx.collision.over_water,
        time_since_liftoff_s: ctx.autopilot.time_since_liftoff_s,
        downrange_m: 0.0,
        crossrange_m: 0.0,
        touchdown_recorded: false,
        touchdown_vertical_speed_mps: 0.0,
        touchdown_lateral_speed_mps: 0.0,
        touchdown_tilt_deg: 0.0,
        touchdown_slope_deg: 0.0,
        touchdown_distance_to_target_m: 0.0,
        leg_compression_peak_m: 0.0,
        toppling: false,
    }
}

/// Flight log entry for replay and analysis.
#[derive(Debug, Clone)]
pub struct FlightLogEntry {
    pub time_s: f64,
    pub position_m: DVec3,
    pub velocity_mps: DVec3,
    pub orientation: DQuat,
    pub angular_velocity_radps: DVec3,
    pub mass_kg: f64,
    pub altitude_agl_m: f64,
    pub altitude_msl_m: f64,
    pub velocity_total_mps: f64,
    pub mach_number: f64,
    pub dynamic_pressure_pa: f64,
    pub g_load: f64,
    pub total_thrust_n: f64,
    pub throttle: f32,
    pub mission_phase: RocketMissionState,
    pub active_stage: usize,
    pub propellant_fraction: f64,
    pub apoapsis_altitude_m: f64,
    pub periapsis_altitude_m: f64,
    pub convective_heat_flux_w_m2: f64,
    pub plasma_blackout: bool,
    pub drogue_deployed: bool,
    pub main_deployed: bool,
}

/// Trait for flight recording strategies.
pub trait FlightRecorderStrategy {
    fn should_record(&self, current_time: f64) -> bool;
    fn record(&mut self, entry: FlightLogEntry, current_time: f64);
}

/// A notable flight event captured by the flight recorder.
#[derive(Debug, Clone)]
pub struct FlightEventRecord {
    pub time_s: f64,
    pub label: String,
}

/// Ring buffer flight recorder.
#[derive(Component, Debug)]
pub struct FlightRecorder {
    entries: Vec<FlightLogEntry>,
    events: Vec<FlightEventRecord>,
    max_entries: usize,
    record_interval_s: f64,
    last_record_time_s: f64,
    recording: bool,
}

impl FlightRecorder {
    pub fn new(max_entries: usize, record_interval_s: f64) -> Self {
        Self {
            entries: Vec::with_capacity(max_entries),
            events: Vec::new(),
            max_entries,
            record_interval_s,
            last_record_time_s: 0.0,
            recording: true,
        }
    }

    pub fn entries(&self) -> &[FlightLogEntry] {
        &self.entries
    }

    /// Record a notable flight event (staging, fairing, splashdown, blackout).
    /// Capped at [`MAX_RECORDED_EVENTS`] entries.
    pub fn note_event(&mut self, time_s: f64, label: String) {
        if self.events.len() >= MAX_RECORDED_EVENTS {
            self.events.remove(0);
        }
        self.events.push(FlightEventRecord { time_s, label });
    }

    pub fn events(&self) -> &[FlightEventRecord] {
        &self.events
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn set_recording(&mut self, recording: bool) {
        self.recording = recording;
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.events.clear();
        self.last_record_time_s = 0.0;
    }
}

/// Maximum number of notable events kept in the flight log.
pub const MAX_RECORDED_EVENTS: usize = 100;

impl FlightRecorderStrategy for FlightRecorder {
    fn should_record(&self, current_time: f64) -> bool {
        self.recording && current_time - self.last_record_time_s >= self.record_interval_s
    }

    fn record(&mut self, entry: FlightLogEntry, current_time: f64) {
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(entry);
        self.last_record_time_s = current_time;
    }
}

/// Build a FlightLogEntry from TelemetryContext.
fn build_flight_log_entry<'a>(ctx: &TelemetryContext<'a>, current_time: f64) -> FlightLogEntry {
    let d = ctx.derived();

    FlightLogEntry {
        time_s: current_time,
        position_m: ctx.position_m,
        velocity_mps: ctx.velocity_mps,
        orientation: ctx.orientation,
        angular_velocity_radps: ctx.angular_velocity_radps,
        mass_kg: ctx.mass_kg,
        altitude_agl_m: ctx.collision.radar_altitude_m,
        altitude_msl_m: d.altitude_m,
        velocity_total_mps: d.speed,
        mach_number: d.mach,
        dynamic_pressure_pa: d.q,
        g_load: d.g_load,
        total_thrust_n: d.total_thrust_n,
        throttle: ctx.propulsion.throttle,
        mission_phase: *ctx.mission_state,
        active_stage: ctx.propulsion.active_stage,
        propellant_fraction: d.propellant_fraction,
        apoapsis_altitude_m: ctx.orbital.apoapsis_m - ctx.planet_radius_m,
        periapsis_altitude_m: ctx.orbital.periapsis_m - ctx.planet_radius_m,
        convective_heat_flux_w_m2: ctx.thermal.convective_heat_flux_w_m2,
        plasma_blackout: d.plasma_blackout,
        drogue_deployed: ctx.parachute.deployment.drogue_deployed,
        main_deployed: ctx.parachute.deployment.main_deployed,
    }
}

/// System: compute rocket telemetry and write to resource.
pub fn compute_rocket_telemetry_system(
    sim_time: Res<SimulationTime>,
    planet_query: Query<(&PlanetComponent, &PlanetAtmosphere)>,
    mut telemetry: ResMut<RocketTelemetry>,
    rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketGeometry,
        &RocketMass,
        &RocketPropulsion,
        &RocketMissionState,
        &RocketAutopilot,
        &OrbitalElements,
        &AtmosphereState,
        &CommsState,
        &AerodynamicForces,
        &ThermalState,
        &AblationState,
        &ParachuteState,
        &TerrainCollisionState,
    )>,
    // Phase 14 extras, read-only and disjoint from the tuple above.
    lifecycle_query: Query<(&LandingScorecard, &TipOverState)>,
) {
    let dt = sim_time.fixed_timestep();
    let current_time = sim_time.sim_time_s;

    for (
        binding,
        rocket,
        geometry,
        mass,
        propulsion,
        mission_state,
        autopilot,
        orbital,
        atmosphere,
        comms,
        aero,
        thermal,
        ablation,
        parachute,
        collision,
    ) in rocket_query.iter()
    {
        let Some((planet, _)) = planet_query
            .iter()
            .find(|(p, _)| p.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        let ctx = TelemetryContext {
            sim_time: current_time,
            dt,
            planet_mass: planet.domain_planet.mass_kg,
            planet_radius_m: planet.domain_planet.radius_km as f64 * 1000.0,
            position_m: rocket.dynamics.position_m,
            velocity_mps: rocket.dynamics.velocity_mps,
            orientation: rocket.dynamics.orientation,
            angular_velocity_radps: rocket.dynamics.angular_velocity_radps,
            mass_kg: mass.0,
            rocket_mass: mass,
            geometry,
            propulsion,
            mission_state,
            autopilot,
            orbital,
            atmosphere,
            comms,
            aero_forces: aero,
            thermal,
            ablation,
            parachute,
            collision,
        };

        *telemetry = compute_telemetry_from_context(&ctx);
    }

    // Overlay the Phase 14 lifecycle extras (scorecard + tip-over). The
    // primary telemetry tuple is already at the query-size cap, so these
    // read-only components ride in a second query; with a single primary
    // vehicle the merge is exact.
    for (scorecard, tip_over) in lifecycle_query.iter() {
        if !scorecard.recorded {
            continue;
        }
        telemetry.touchdown_recorded = true;
        telemetry.touchdown_vertical_speed_mps = scorecard.touchdown_vertical_speed_mps;
        telemetry.touchdown_lateral_speed_mps = scorecard.touchdown_lateral_speed_mps;
        telemetry.touchdown_tilt_deg = scorecard.touchdown_tilt_deg;
        telemetry.touchdown_slope_deg = scorecard.touchdown_slope_deg;
        telemetry.touchdown_distance_to_target_m = scorecard.distance_to_target_m;
        telemetry.leg_compression_peak_m = scorecard.leg_compression_peak_m;
        telemetry.toppling = tip_over.is_toppling();
    }
}

/// System: record flight data at intervals.
/// The rocket state is split across two read-only queries because Bevy query
/// tuples cap at 15 items; joining on Entity keeps it one logical record.
pub fn record_flight_data_system(
    sim_time: Res<SimulationTime>,
    planet_query: Query<(&PlanetComponent, &PlanetAtmosphere)>,
    mut rocket_query: Query<(
        Entity,
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketGeometry,
        &RocketMass,
        &RocketPropulsion,
        &RocketMissionState,
        &RocketAutopilot,
        &OrbitalElements,
        &AtmosphereState,
        &CommsState,
        &AerodynamicForces,
        &ThermalState,
        &AblationState,
    )>,
    recovery_query: Query<(&ParachuteState, &TerrainCollisionState)>,
    mut recorder_query: Query<&mut FlightRecorder>,
) {
    let current_time = sim_time.sim_time_s;
    let dt = sim_time.fixed_timestep();

    for (
        entity,
        binding,
        rocket,
        geometry,
        mass,
        propulsion,
        mission_state,
        autopilot,
        orbital,
        atmosphere,
        comms,
        aero,
        thermal,
        ablation,
    ) in rocket_query.iter_mut()
    {
        if !recorder_query
            .get_mut(entity)
            .map(|mut r| r.should_record(current_time))
            .unwrap_or(false)
        {
            continue;
        }
        let Ok((parachute, collision)) = recovery_query.get(entity) else {
            continue;
        };

        let Some((planet, _)) = planet_query
            .iter()
            .find(|(p, _)| p.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        let ctx = TelemetryContext {
            sim_time: current_time,
            dt,
            planet_mass: planet.domain_planet.mass_kg,
            planet_radius_m: planet.domain_planet.radius_km as f64 * 1000.0,
            position_m: rocket.dynamics.position_m,
            velocity_mps: rocket.dynamics.velocity_mps,
            orientation: rocket.dynamics.orientation,
            angular_velocity_radps: rocket.dynamics.angular_velocity_radps,
            mass_kg: mass.0,
            rocket_mass: mass,
            geometry,
            propulsion,
            mission_state,
            autopilot,
            orbital,
            atmosphere,
            comms,
            aero_forces: aero,
            thermal,
            ablation,
            parachute,
            collision,
        };

        let entry = build_flight_log_entry(&ctx, current_time);
        if let Ok(mut recorder) = recorder_query.get_mut(entity) {
            recorder.record(entry, current_time);
        }
    }
}

/// Flight recorder input actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightRecorderAction {
    ToggleRecording,
    ClearLog,
}

/// How long the latest event stays visible on the HUD (s).
pub const EVENT_FEED_VISIBLE_S: f32 = 5.0;

/// Latest notable flight event for HUD display (empty = none recent).
#[derive(Resource, Debug, Clone, Default)]
pub struct RocketEventFeed {
    pub latest: String,
    pub visible_for_s: f32,
}

impl RocketEventFeed {
    fn push(&mut self, label: String) {
        self.latest = label;
        self.visible_for_s = EVENT_FEED_VISIBLE_S;
    }

    fn tick(&mut self, dt: f32) {
        if self.visible_for_s > 0.0 {
            self.visible_for_s -= dt;
            if self.visible_for_s <= 0.0 {
                self.latest.clear();
            }
        }
    }
}

/// Consume rocket domain events (staging, fairing, splashdown, blackout):
/// update the HUD feed and append entries to each vehicle's flight recorder.
/// Runs in Update; physics systems are untouched (AGENTS.md section 29).
#[allow(clippy::too_many_arguments)]
pub fn rocket_event_feed_system(
    time: Res<Time>,
    sim_time: Res<SimulationTime>,
    mut staging_reader: MessageReader<StageSeparatedEvent>,
    mut fairing_reader: MessageReader<FairingSeparatedEvent>,
    mut splashdown_reader: MessageReader<SplashdownDetectedEvent>,
    mut blackout_reader: MessageReader<CommsBlackoutEvent>,
    mut feed: ResMut<RocketEventFeed>,
    mut recorders: Query<&mut FlightRecorder>,
) {
    let now = sim_time.sim_time_s;

    for event in staging_reader.read() {
        let label = format!("STAGE SEPARATED (-{:.0} kg)", event.shed_mass_kg);
        feed.push(label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in fairing_reader.read() {
        let label = format!("FAIRING JETTISONED (-{:.0} kg)", event.fairing_mass_kg);
        feed.push(label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in splashdown_reader.read() {
        let label = "SPLASHDOWN".to_string();
        feed.push(label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in blackout_reader.read() {
        let label = if event.blackout_active {
            "COMMS BLACKOUT STARTED".to_string()
        } else {
            "COMMS REACQUIRED".to_string()
        };
        feed.push(label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }

    feed.tick(time.delta_secs());
}

/// System: handle flight recorder input.
pub fn handle_flight_recorder_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut rocket_query: Query<&mut FlightRecorder>,
) {
    let action = if keyboard.just_pressed(KeyCode::F9) {
        Some(FlightRecorderAction::ToggleRecording)
    } else if keyboard.just_pressed(KeyCode::F10) {
        Some(FlightRecorderAction::ClearLog)
    } else {
        None
    };

    let Some(action) = action else {
        return;
    };

    for mut recorder in rocket_query.iter_mut() {
        match action {
            FlightRecorderAction::ToggleRecording => {
                let was_recording = recorder.is_recording();
                recorder.set_recording(!was_recording);
                bevy::log::info!(
                    "Flight recording {}",
                    if !was_recording { "STARTED" } else { "STOPPED" }
                );
            }
            FlightRecorderAction::ClearLog => {
                recorder.clear();
                bevy::log::info!("Flight log cleared");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Flight-recorder CSV export (Phase 15)
// ---------------------------------------------------------------------------

/// Directory receiving exported flight recordings, relative to the working
/// directory. Created on demand.
pub const FLIGHT_EXPORT_DIR: &str = "exports";

/// One vehicle's recording serialized as CSV: a header row per recorded
/// field plus a `#`-prefixed notable-events section. Pure function so the
/// format is testable without touching the filesystem.
pub fn flight_recorder_csv(name: &str, recorder: &FlightRecorder) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "vehicle,time_s,alt_agl_m,alt_msl_m,speed_mps,mach,q_pa,g_load,thrust_n,throttle,phase,stage,propellant_fraction,apoapsis_m,periapsis_m,blackout,drogue,main"
    );
    for e in recorder.entries() {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{},{:.4},{},{},{},{},{}",
            name,
            e.time_s,
            e.altitude_agl_m,
            e.altitude_msl_m,
            e.velocity_total_mps,
            e.mach_number,
            e.dynamic_pressure_pa,
            e.g_load,
            e.total_thrust_n,
            e.throttle,
            format!("{:?}", e.mission_phase).as_str(),
            e.active_stage,
            e.propellant_fraction,
            e.apoapsis_altitude_m,
            e.periapsis_altitude_m,
            e.plasma_blackout,
            e.drogue_deployed,
            e.main_deployed,
        );
    }
    for ev in recorder.events() {
        let _ = writeln!(out, "#event,{},{},{}", ev.time_s, name, ev.label);
    }
    out
}

/// Make a vehicle name safe for a filename component.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// F11 dumps every vehicle's ring-buffer contents and notable events to
/// `exports/flight_<stamp>_<vehicle>.csv`. IO problems are logged, never
/// fatal (AGENTS.md section 38): the recording itself is untouched.
pub fn handle_flight_recorder_export_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut rocket_query: Query<(Entity, &Selectable, &mut FlightRecorder)>,
) {
    if !keyboard.just_pressed(KeyCode::F11) {
        return;
    }

    // Guard: an unwritable export directory disables the feature cleanly.
    if let Err(e) = std::fs::create_dir_all(FLIGHT_EXPORT_DIR) {
        bevy::log::warn!(
            "Flight export disabled: cannot create {}: {e}",
            FLIGHT_EXPORT_DIR
        );
        return;
    }

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    for (entity, selectable, recorder) in rocket_query.iter_mut() {
        let path = std::path::Path::new(FLIGHT_EXPORT_DIR).join(format!(
            "flight_{stamp}_{}_{}.csv",
            sanitize_filename(&selectable.name),
            entity.index()
        ));
        let csv = flight_recorder_csv(&selectable.name, &recorder);
        match std::fs::write(&path, csv) {
            Ok(()) => bevy::log::info!(
                "Flight recording exported: {} ({} entries, {} events)",
                path.display(),
                recorder.entries().len(),
                recorder.events().len()
            ),
            Err(e) => bevy::log::warn!("Flight export failed ({}): {e}", path.display()),
        }
    }
}

#[cfg(test)]
mod export_tests {
    use super::*;
    use crate::domain::entities::rocket::RocketMissionState;

    /// A minimal recorder entry at t=0 and t=1 s.
    fn sample_entry(time_s: f64) -> FlightLogEntry {
        FlightLogEntry {
            time_s,
            position_m: DVec3::ZERO,
            velocity_mps: DVec3::ZERO,
            orientation: DQuat::IDENTITY,
            angular_velocity_radps: DVec3::ZERO,
            mass_kg: 1_000.0,
            altitude_agl_m: 100.0 * time_s,
            altitude_msl_m: 100.0 * time_s,
            velocity_total_mps: 50.0,
            mach_number: 0.15,
            dynamic_pressure_pa: 700.0,
            g_load: 1.2,
            total_thrust_n: 20_000.0,
            throttle: 0.8,
            mission_phase: RocketMissionState::Ascent.into(),
            active_stage: 0,
            propellant_fraction: 0.5,
            apoapsis_altitude_m: 200_000.0,
            periapsis_altitude_m: -6_370_000.0,
            convective_heat_flux_w_m2: 0.0,
            plasma_blackout: false,
            drogue_deployed: false,
            main_deployed: false,
        }
    }

    #[test]
    fn csv_contains_header_rows_and_events() {
        let mut recorder = FlightRecorder::new(16, 0.5);
        recorder.record(sample_entry(0.0), 0.0);
        recorder.record(sample_entry(1.0), 1.0);
        recorder.note_event(0.75, "STAGE SEPARATED (-500 kg)".into());

        let csv = flight_recorder_csv("Test Rocket", &recorder);
        let lines: Vec<&str> = csv.lines().collect();

        // Header + two entries + one event line.
        assert_eq!(lines.len(), 4, "unexpected line count: {csv:?}");
        assert!(lines[0].starts_with("vehicle,time_s"));
        assert!(lines[1].starts_with("Test Rocket,0,"), "{}", lines[1]);
        assert!(lines[2].starts_with("Test Rocket,1,"));
        assert!(lines[3].starts_with("#event,0.75,Test Rocket,STAGE SEPARATED"));
    }
}

#[cfg(test)]
mod g_load_tests {
    use super::*;
    use crate::domain::entities::rocket::{Rocket, RocketEngine, RocketStage};

    const MASS_KG: f64 = 1_000.0;
    const EARTH_MASS_KG: f64 = 5.97237e24;
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    /// One-engine vehicle whose thrust axis is body +Y.
    fn single_engine_vehicle(max_thrust_kn: f32) -> Rocket {
        Rocket {
            name: "G-Load Test".into(),
            diameter_m: 1.0,
            height_m: 10.0,
            stages: vec![RocketStage {
                name: "S1".into(),
                dry_mass_kg: 400.0,
                propellant_mass_kg: 600.0,
                engines: vec![RocketEngine {
                    position_m: bevy::math::Vec3::new(0.0, -5.0, 0.0),
                    thrust_axis: bevy::math::Vec3::Y,
                    isp_sea_level: 250.0,
                    isp_vacuum: 300.0,
                    gimbal_range_deg: 0.0,
                    max_thrust_kn,
                    throttle_min: 0.0,
                    throttle_max: 1.0,
                    restartable: true,
                    state: EngineState::Running,
                }],
            }],
        }
    }

    fn telemetry(ctx: &TelemetryContext) -> RocketTelemetry {
        compute_telemetry_from_context(ctx)
    }

    /// Regression (Phase 17): g_load must be the sensed load factor
    /// |thrust + aero| / (m·g0), NOT speed/g0 — the old formula had units of
    /// seconds and read ~785 "g" during an orbital coast.
    #[test]
    fn hover_at_one_g_reads_one_g() {
        // 9.807 kN on 1000 kg with identity orientation: exactly 1 g.
        let vehicle = single_engine_vehicle(STANDARD_GRAVITY_MPS2 as f32);
        // Old formula would report ~713 "g" here while hovering.
        let speed_mps = 7_000.0;
        let mass = RocketMass(MASS_KG);
        let geometry = RocketGeometry {
            radius_m: 1.0,
            height_m: 10.0,
        };
        let propulsion = RocketPropulsion {
            vehicle: vehicle.clone(),
            active_stage: 0,
            propellant_remaining_kg: vec![600.0],
            throttle: 1.0,
            gimbal_pitch_rad: 0.0,
            gimbal_yaw_rad: 0.0,
            time_since_separation_s: 0.0,
            ullage_settle_time_s: 0.0,
            separations_count: 0,
            attached_payload_kg: 0.0,
        };
        let mission_state = RocketMissionState::default();
        let autopilot = RocketAutopilot::default();
        let orbital = OrbitalElements::default();
        let atmosphere = AtmosphereState::default();
        let comms = CommsState::default();
        let aero_forces = AerodynamicForces {
            force_body: DVec3::ZERO,
            center_of_pressure_body: DVec3::ZERO,
        };
        let thermal = ThermalState::default();
        let ablation = AblationState::default();
        let parachute = ParachuteState::default();
        let collision = TerrainCollisionState::default();

        // Hovering just above the equator at low altitude.
        let position = DVec3::new(EARTH_RADIUS_M + 1_000.0, 0.0, 0.0);
        // Velocity purely horizontal so it does not contribute to sensed force.
        let velocity = DVec3::new(0.0, 0.0, speed_mps);

        let t = telemetry(&TelemetryContext {
            sim_time: 0.0,
            dt: 1.0 / 60.0,
            planet_mass: EARTH_MASS_KG,
            planet_radius_m: EARTH_RADIUS_M,
            position_m: position,
            velocity_mps: velocity,
            orientation: DQuat::IDENTITY,
            angular_velocity_radps: DVec3::ZERO,
            mass_kg: MASS_KG,
            rocket_mass: &mass,
            geometry: &geometry,
            propulsion: &propulsion,
            mission_state: &mission_state,
            autopilot: &autopilot,
            orbital: &orbital,
            atmosphere: &atmosphere,
            comms: &comms,
            aero_forces: &aero_forces,
            thermal: &thermal,
            ablation: &ablation,
            parachute: &parachute,
            collision: &collision,
        });

        assert!(
            (t.g_load - 1.0).abs() < 1e-6,
            "hovering at weight must read 1 g, got {}",
            t.g_load
        );
    }

    #[test]
    fn orbital_coast_in_vacuum_reads_zero_g() {
        let vehicle = single_engine_vehicle(9.807);
        let speed_mps = 7_790.0;
        let mass = RocketMass(MASS_KG);
        let geometry = RocketGeometry {
            radius_m: 1.0,
            height_m: 10.0,
        };
        let propulsion = RocketPropulsion {
            vehicle: vehicle.clone(),
            active_stage: 0,
            propellant_remaining_kg: vec![600.0],
            throttle: 0.0,
            gimbal_pitch_rad: 0.0,
            gimbal_yaw_rad: 0.0,
            time_since_separation_s: 0.0,
            ullage_settle_time_s: 0.0,
            separations_count: 0,
            attached_payload_kg: 0.0,
        };
        let mission_state = RocketMissionState::default();
        let autopilot = RocketAutopilot::default();
        let orbital = OrbitalElements::default();
        let atmosphere = AtmosphereState::default();
        let comms = CommsState::default();
        let aero_forces = AerodynamicForces {
            force_body: DVec3::ZERO,
            center_of_pressure_body: DVec3::ZERO,
        };
        let thermal = ThermalState::default();
        let ablation = AblationState::default();
        let parachute = ParachuteState::default();
        let collision = TerrainCollisionState::default();

        let t = telemetry(&TelemetryContext {
            sim_time: 0.0,
            dt: 1.0 / 60.0,
            planet_mass: EARTH_MASS_KG,
            planet_radius_m: EARTH_RADIUS_M,
            position_m: DVec3::new(EARTH_RADIUS_M + 400_000.0, 0.0, 0.0),
            velocity_mps: DVec3::new(0.0, 0.0, speed_mps),
            orientation: DQuat::IDENTITY,
            angular_velocity_radps: DVec3::ZERO,
            mass_kg: MASS_KG,
            rocket_mass: &mass,
            geometry: &geometry,
            propulsion: &propulsion,
            mission_state: &mission_state,
            autopilot: &autopilot,
            orbital: &orbital,
            atmosphere: &atmosphere,
            comms: &comms,
            aero_forces: &aero_forces,
            thermal: &thermal,
            ablation: &ablation,
            parachute: &parachute,
            collision: &collision,
        });

        assert!(
            t.g_load.abs() < 1e-12,
            "coasting in vacuum must read 0 g, got {}",
            t.g_load
        );
    }
}
