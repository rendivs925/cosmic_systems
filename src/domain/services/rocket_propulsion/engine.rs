//! Engine thrust, specific impulse, mass flow, propellant consumption, and
//! throttle envelopes.

use super::STANDARD_GRAVITY_MPS2;
use crate::domain::entities::rocket::{EngineState, RocketEngine, ThrustReference};
use crate::domain::math::DVec3;
use crate::domain::services::atmosphere::SEA_LEVEL_PRESSURE_PA;

/// Thrust from mass flow and specific impulse: `T = m_dot · Isp · g0`.
pub fn thrust_from_isp(mass_flow_kg_s: f64, isp_s: f32) -> f64 {
    mass_flow_kg_s * isp_s as f64 * STANDARD_GRAVITY_MPS2
}

/// Mass flow for a given thrust and specific impulse: `m_dot = T / (Isp · g0)`.
pub fn mass_flow_from_thrust(thrust_n: f64, isp_s: f32) -> f64 {
    thrust_n / (isp_s as f64 * STANDARD_GRAVITY_MPS2)
}

/// Select the effective specific impulse from ambient pressure, linearly
/// interpolating the configured sea-level and vacuum endpoints. With a fixed
/// nozzle mass flow, the pressure-thrust term is linear in back pressure.
pub fn selected_isp(isp_sea_level: f32, isp_vacuum: f32, ambient_pressure_pa: f64) -> f32 {
    let t = (1.0 - (ambient_pressure_pa / SEA_LEVEL_PRESSURE_PA).clamp(0.0, 1.0)) as f32;
    isp_sea_level + (isp_vacuum - isp_sea_level) * t
}

/// One engine's coherent operating point at a fixed throttle and back pressure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineOperatingPoint {
    pub specific_impulse_s: f32,
    pub mass_flow_kg_s: f64,
    pub thrust_n: f64,
}

impl EngineOperatingPoint {
    pub fn from_engine(engine: &RocketEngine, throttle: f32, ambient_pressure_pa: f64) -> Self {
        let rated_thrust_n =
            engine.rated_thrust_kn as f64 * 1000.0 * throttle.clamp(0.0, 1.0) as f64;
        let rated_isp_s = match engine.thrust_reference {
            ThrustReference::SeaLevel => engine.isp_sea_level,
            ThrustReference::Vacuum => engine.isp_vacuum,
        };
        let mass_flow_kg_s = mass_flow_from_thrust(rated_thrust_n, rated_isp_s);
        let specific_impulse_s =
            selected_isp(engine.isp_sea_level, engine.isp_vacuum, ambient_pressure_pa);
        Self {
            specific_impulse_s,
            mass_flow_kg_s,
            thrust_n: thrust_from_isp(mass_flow_kg_s, specific_impulse_s),
        }
    }
}

/// Full-throttle thrust at an ambient pressure. Fixed mass flow is derived from
/// the engine's declared rated-thrust endpoint; pressure-selected Isp then
/// determines force at every other ambient pressure.
pub fn engine_thrust_n(engine: &RocketEngine, throttle: f32, ambient_pressure_pa: f64) -> f64 {
    EngineOperatingPoint::from_engine(engine, throttle, ambient_pressure_pa).thrust_n
}

/// Result of one bounded propellant-consumption step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropellantConsumption {
    /// Propellant left after the step, kg.
    pub remaining_kg: f32,
    /// Propellant actually consumed during the step, kg.
    pub consumed_kg: f32,
}

/// Consume propellant at the given mass flow for `dt` seconds. Consumption
/// never exceeds the available propellant.
pub fn consume_propellant(
    propellant_kg: f32,
    mass_flow_kg_s: f64,
    dt: f64,
) -> PropellantConsumption {
    let remaining_kg = (propellant_kg - (mass_flow_kg_s * dt) as f32).max(0.0);
    PropellantConsumption {
        remaining_kg,
        consumed_kg: propellant_kg - remaining_kg,
    }
}

/// Duration for which a stage can actually produce thrust during one fixed
/// step. A nearly empty tank cannot deliver a full-step impulse.
pub fn burn_duration_s(propellant_kg: f32, mass_flow_kg_s: f64, dt: f64) -> f64 {
    if propellant_kg <= 0.0 || mass_flow_kg_s <= 0.0 || dt <= 0.0 {
        return 0.0;
    }
    (propellant_kg as f64 / mass_flow_kg_s).min(dt)
}

/// Clamp a commanded throttle to a per-engine throttle range and then to the
/// physical 0..1 band. A zero (or negative) command means "engine off" and
/// stays zero — otherwise an engine with a positive minimum throttle could
/// never be shut down.
pub fn clamp_throttle_range(cmd: f32, throttle_min: f32, throttle_max: f32) -> f32 {
    if cmd <= 0.0 {
        return 0.0;
    }
    cmd.clamp(
        throttle_min.clamp(0.0, 1.0),
        throttle_max.clamp(throttle_min.min(1.0), 1.0),
    )
}

/// Commanded-throttle envelope shared by every engine in a stage: the command
/// must be valid for each engine individually, so the stage envelope is the
/// intersection (highest lower bound, lowest upper bound).
pub fn stage_throttle_envelope(engines: &[RocketEngine]) -> (f32, f32) {
    let Some(first) = engines.first() else {
        return (0.0, 1.0);
    };
    (
        engines
            .iter()
            .map(|e| e.throttle_min)
            .fold(first.throttle_min, f32::max),
        engines
            .iter()
            .map(|e| e.throttle_max)
            .fold(first.throttle_max, f32::min),
    )
}

/// Total running-engine thrust (body frame) for the active stage at a throttle,
/// honoring per-engine ISP selection by ambient pressure. Only engines in
/// [`EngineState::Running`] contribute — every thrust consumer routes through
/// here so shutdown state is respected consistently everywhere.
pub fn stage_thrust_body(
    engines: &[RocketEngine],
    throttle: f32,
    ambient_pressure_pa: f64,
) -> (DVec3, f64) {
    let throttle = throttle.clamp(0.0, 1.0);
    let mut force = DVec3::ZERO;
    let mut mass_flow = 0.0;
    for engine in engines {
        if engine.state != EngineState::Running {
            continue;
        }
        let point = EngineOperatingPoint::from_engine(engine, throttle, ambient_pressure_pa);
        force += engine.thrust_axis.as_dvec3() * point.thrust_n;
        mass_flow += point.mass_flow_kg_s;
    }
    (force, mass_flow)
}

/// Maximum thrust available to guidance before an engine is commanded to run.
/// Off engines with remaining ignition budget are startable; terminally
/// depleted engines are not. This is a planning capability only: physical
/// force, torque, and mass flow must continue to use [`stage_thrust_body`].
pub fn stage_available_thrust_body(
    engines: &[RocketEngine],
    throttle: f32,
    ambient_pressure_pa: f64,
) -> (DVec3, f64) {
    let throttle = throttle.clamp(0.0, 1.0);
    let mut force = DVec3::ZERO;
    let mut mass_flow = 0.0;
    for engine in engines {
        if engine.state == EngineState::Depleted {
            continue;
        }
        let point = EngineOperatingPoint::from_engine(engine, throttle, ambient_pressure_pa);
        force += engine.thrust_axis.as_dvec3() * point.thrust_n;
        mass_flow += point.mass_flow_kg_s;
    }
    (force, mass_flow)
}
