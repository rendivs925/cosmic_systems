// Rocket telemetry computation - encapsulated, trait-based design.

use crate::components::rocket::*;
use crate::domain::services::aerodynamics::{
    angle_of_attack, angle_of_sideslip, dynamic_pressure_q,
};
use crate::domain::services::gravity::gravitational_parameter;
use crate::domain::services::rocket_propulsion::{
    active_vehicle_mass, stage_thrust_body, STANDARD_GRAVITY_MPS2,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::components::{PlanetAtmosphere, PlanetComponent};
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

        let (total_thrust_n, isp_vac) = self.compute_thrust(rho);
        let tw_ratio = if weight > 0.0 {
            total_thrust_n / weight
        } else {
            0.0
        };

        let body_velocity = self.orientation.inverse() * self.velocity_mps;
        let aoa = angle_of_attack(body_velocity).to_degrees();
        let aos = angle_of_sideslip(body_velocity).to_degrees();

        let ang_vel = self.angular_velocity_radps;
        let roll_rate = ang_vel.y.to_degrees();
        let pitch_rate = ang_vel.x.to_degrees();
        let yaw_rate = ang_vel.z.to_degrees();

        let body_x = self.orientation * DVec3::X;
        let body_x_horizontal = body_x - body_x.dot(up_dir) * up_dir;
        let bank = if body_x_horizontal.length_squared() > 1e-6 {
            body_x_horizontal.angle_between(DVec3::Z).to_degrees()
        } else {
            0.0
        };

        let (dry_mass, propellant_fraction) = self.compute_mass_properties();

        let delta_v = Self::compute_delta_v(self.mass_kg, dry_mass, isp_vac);

        let electron_density = 1e-4 * rho * speed.powi(3);
        let plasma_blackout = matches!(*self.mission_state, RocketMissionState::ReentryCorridor)
            && electron_density > 6.6e16;

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
            dry_mass,
            propellant_fraction,
            delta_v,
            aoa,
            aos,
            roll_rate,
            pitch_rate,
            yaw_rate,
            bank,
            electron_density,
            plasma_blackout,
        }
    }

    fn compute_thrust(&self, rho: f64) -> (f64, f64) {
        if self.propulsion.active_stage >= self.propulsion.vehicle.stages.len() {
            return (0.0, 0.0);
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
            return (0.0, 0.0);
        }
        let (thrust_body, _) = stage_thrust_body(&stage.engines, throttle, rho);
        let total_thrust_n = thrust_body.length();
        let isp_vac = stage
            .engines
            .iter()
            .find(|e| e.state == crate::domain::entities::rocket::EngineState::Running)
            .map(|e| e.isp_vacuum as f64)
            .unwrap_or(0.0);
        (total_thrust_n, isp_vac)
    }

    fn compute_mass_properties(&self) -> (f64, f64) {
        let dry_mass = active_vehicle_mass(
            &self.propulsion.vehicle.stages,
            &self.propulsion.propellant_remaining_kg,
            self.propulsion.active_stage,
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
    dry_mass: f64,
    propellant_fraction: f64,
    delta_v: f64,
    aoa: f64,
    aos: f64,
    roll_rate: f64,
    pitch_rate: f64,
    yaw_rate: f64,
    bank: f64,
    electron_density: f64,
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
        g_load: d.speed / STANDARD_GRAVITY_MPS2,
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
        drogue_deployed: ctx.parachute.drogue_deployed,
        main_deployed: ctx.parachute.main_deployed,
        time_since_liftoff_s: ctx.autopilot.time_since_liftoff_s,
        downrange_m: 0.0,
        crossrange_m: 0.0,
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

/// Ring buffer flight recorder.
#[derive(Component, Debug)]
pub struct FlightRecorder {
    entries: Vec<FlightLogEntry>,
    max_entries: usize,
    record_interval_s: f64,
    last_record_time_s: f64,
    recording: bool,
}

impl FlightRecorder {
    pub fn new(max_entries: usize, record_interval_s: f64) -> Self {
        Self {
            entries: Vec::with_capacity(max_entries),
            max_entries,
            record_interval_s,
            last_record_time_s: 0.0,
            recording: true,
        }
    }

    pub fn entries(&self) -> &[FlightLogEntry] {
        &self.entries
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn set_recording(&mut self, recording: bool) {
        self.recording = recording;
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.last_record_time_s = 0.0;
    }
}

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
        g_load: d.speed / STANDARD_GRAVITY_MPS2,
        total_thrust_n: d.total_thrust_n,
        throttle: ctx.propulsion.throttle,
        mission_phase: *ctx.mission_state,
        active_stage: ctx.propulsion.active_stage,
        propellant_fraction: d.propellant_fraction,
        apoapsis_altitude_m: ctx.orbital.apoapsis_m - ctx.planet_radius_m,
        periapsis_altitude_m: ctx.orbital.periapsis_m - ctx.planet_radius_m,
        convective_heat_flux_w_m2: ctx.thermal.convective_heat_flux_w_m2,
        plasma_blackout: d.plasma_blackout,
        drogue_deployed: ctx.parachute.drogue_deployed,
        main_deployed: ctx.parachute.main_deployed,
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
        &AerodynamicForces,
        &ThermalState,
        &AblationState,
        &ParachuteState,
        &TerrainCollisionState,
    )>,
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
            aero_forces: aero,
            thermal,
            ablation,
            parachute,
            collision,
        };

        *telemetry = compute_telemetry_from_context(&ctx);
    }
}

/// System: record flight data at intervals.
pub fn record_flight_data_system(
    sim_time: Res<SimulationTime>,
    planet_query: Query<(&PlanetComponent, &PlanetAtmosphere)>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketGeometry,
        &RocketMass,
        &RocketPropulsion,
        &RocketMissionState,
        &RocketAutopilot,
        &OrbitalElements,
        &AtmosphereState,
        &AerodynamicForces,
        &ThermalState,
        &AblationState,
        &ParachuteState,
        &TerrainCollisionState,
        &mut FlightRecorder,
    )>,
) {
    let current_time = sim_time.sim_time_s;
    let dt = sim_time.fixed_timestep();

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
        aero,
        thermal,
        ablation,
        parachute,
        collision,
        mut recorder,
    ) in rocket_query.iter_mut()
    {
        if !recorder.should_record(current_time) {
            continue;
        }

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
            aero_forces: aero,
            thermal,
            ablation,
            parachute,
            collision,
        };

        let entry = build_flight_log_entry(&ctx, current_time);
        recorder.record(entry, current_time);
    }
}

/// Flight recorder input actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightRecorderAction {
    ToggleRecording,
    ClearLog,
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
