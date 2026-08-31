use bevy::prelude::*;

/// Mission phase of a rocket flight. Drives which guidance targets are
/// produced (AGENTS.md section 18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RocketMissionState {
    #[default]
    PreLaunch,
    Launch,
    Ascent,
    Orbit,
    DeorbitBurn,
    ReentryCorridor,
    PoweredDescent,
    UnpoweredDescent,
    Landing,
    Landed,
    Crashed,
}

/// A single rocket engine. Positions/axes are expressed in the vehicle body
/// frame where +Y is the longitudinal axis (nose-up).
#[derive(Clone, Debug)]
pub struct RocketEngine {
    /// Engine position in the vehicle body frame, meters.
    pub position_m: Vec3,
    /// Body-frame unit vector of the thrust force applied to the vehicle.
    pub thrust_axis: Vec3,
    pub isp_sea_level: f32,
    pub isp_vacuum: f32,
    pub gimbal_range_deg: f32,
    /// Full-throttle thrust at standard sea-level pressure, kilonewtons.
    pub max_thrust_kn: f32,
    /// Lowest commanded throttle the engine can hold (0..1).
    pub throttle_min: f32,
    /// Highest commanded throttle the engine can produce (0..1).
    pub throttle_max: f32,
    /// Whether the engine can ignite again after a stage separation
    /// (air-start capability). Non-restartable engines cannot light a new
    /// stage once separated (e.g. solid motors).
    pub restartable: bool,
    pub state: EngineState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineState {
    Off,
    Running,
}

/// A rocket stage: structure plus propellant and its engines.
#[derive(Clone, Debug)]
pub struct RocketStage {
    pub name: String,
    pub dry_mass_kg: f32,
    pub propellant_mass_kg: f32,
    /// Propellant retained by a recoverable lower stage after separation. The
    /// separated recovery vehicle owns and consumes this reserve through the
    /// normal propulsion pipeline; `None` keeps the stage expendable.
    pub recovery_propellant_reserve_kg: Option<f32>,
    pub engines: Vec<RocketEngine>,
}

impl RocketStage {
    pub fn total_mass_kg(&self) -> f32 {
        self.dry_mass_kg + self.propellant_mass_kg
    }
}

/// Full vehicle definition composed of stages.
#[derive(Clone, Debug)]
pub struct Rocket {
    pub name: String,
    pub diameter_m: f32,
    pub height_m: f32,
    pub stages: Vec<RocketStage>,
}

impl Rocket {
    pub fn falcon9() -> Self {
        // Two-stage Falcon 9 preserving the original aggregate Falcon-9
        // parameters: 22 200 kg dry, 120 000 kg propellant, 7 607 kN total
        // first-stage thrust, 282/311 s ISP, 5° gimbal, 3.7 m diameter,
        // 70 m height.
        let stage1_engines = (0..9)
            .map(|i| {
                let angle = i as f32 * 2.0 * std::f32::consts::PI / 9.0;
                RocketEngine {
                    position_m: Vec3::new(1.2 * angle.cos(), -32.0, 1.2 * angle.sin()),
                    thrust_axis: Vec3::Y,
                    isp_sea_level: 282.0,
                    isp_vacuum: 311.0,
                    gimbal_range_deg: 5.0,
                    max_thrust_kn: 7607.0 / 9.0,
                    throttle_min: 0.0,
                    throttle_max: 1.0,
                    restartable: true,
                    state: EngineState::Running,
                }
            })
            .collect::<Vec<_>>();

        Self {
            name: "Falcon 9".to_string(),
            diameter_m: 3.7,
            height_m: 70.0,
            stages: vec![
                RocketStage {
                    name: "Falcon 9 Stage 1".to_string(),
                    dry_mass_kg: 18_000.0,
                    propellant_mass_kg: 90_000.0,
                    recovery_propellant_reserve_kg: Some(15_000.0),
                    engines: stage1_engines,
                },
                RocketStage {
                    name: "Falcon 9 Stage 2".to_string(),
                    dry_mass_kg: 4_200.0,
                    propellant_mass_kg: 30_000.0,
                    recovery_propellant_reserve_kg: None,
                    engines: vec![RocketEngine {
                        position_m: Vec3::new(0.0, 12.0, 0.0),
                        thrust_axis: Vec3::Y,
                        isp_sea_level: 311.0,
                        isp_vacuum: 348.0,
                        gimbal_range_deg: 5.0,
                        max_thrust_kn: 934.0,
                        throttle_min: 0.0,
                        throttle_max: 1.0,
                        restartable: true,
                        state: EngineState::Running,
                    }],
                },
            ],
        }
    }

    pub fn total_dry_mass_kg(&self) -> f32 {
        self.stages.iter().map(|s| s.dry_mass_kg).sum()
    }

    pub fn total_propellant_mass_kg(&self) -> f32 {
        self.stages.iter().map(|s| s.propellant_mass_kg).sum()
    }

    pub fn total_mass_kg(&self) -> f32 {
        self.stages.iter().map(|s| s.total_mass_kg()).sum()
    }

    /// Total sea-level thrust of the first (initial) stage, kN.
    pub fn max_thrust_kn(&self) -> f32 {
        self.stages
            .first()
            .map(|s| s.engines.iter().map(|e| e.max_thrust_kn).sum())
            .unwrap_or(0.0)
    }

    /// Single authoritative mass-flow formula: `m_dot = T / (Isp · g0)`.
    /// All mass-flow calculations go through here (AGENTS.md section 15).
    pub fn mass_flow_rate_kg_s(&self, throttle: f32, isp_s: f32) -> f32 {
        let thrust_n = self.max_thrust_kn() * 1000.0 * throttle.clamp(0.0, 1.0);
        thrust_n
            / (isp_s * crate::domain::services::rocket_propulsion::STANDARD_GRAVITY_MPS2 as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falcon9_preserves_aggregate_parameters() {
        let rocket = Rocket::falcon9();
        assert_eq!(rocket.stages.len(), 2);
        assert!((rocket.total_dry_mass_kg() - 22_200.0).abs() < 1.0);
        assert!((rocket.total_propellant_mass_kg() - 120_000.0).abs() < 1.0);
        assert!((rocket.total_mass_kg() - 142_200.0).abs() < 1.0);
        assert!((rocket.max_thrust_kn() - 7_607.0).abs() < 1.0);
        assert_eq!(rocket.diameter_m, 3.7);
        assert_eq!(rocket.height_m, 70.0);
    }

    #[test]
    fn first_stage_has_nine_running_engines() {
        let rocket = Rocket::falcon9();
        let stage1 = &rocket.stages[0];
        assert_eq!(stage1.engines.len(), 9);
        assert!(stage1
            .engines
            .iter()
            .all(|e| e.state == EngineState::Running));
    }

    #[test]
    fn mass_flow_matches_thrust_and_isp() {
        let rocket = Rocket::falcon9();
        let mdot = rocket.mass_flow_rate_kg_s(1.0, 282.0) as f64;
        // T = m_dot * Isp * g0
        let thrust_n = rocket.max_thrust_kn() as f64 * 1000.0;
        let isp = 282.0;
        let g0 = crate::domain::services::rocket_propulsion::STANDARD_GRAVITY_MPS2;
        assert!((thrust_n - mdot * isp * g0).abs() < 1.0);
    }
}
