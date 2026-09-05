use bevy::prelude::*;

use crate::domain::services::landing_gear::LandingGearSpec;

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

/// A single rocket engine. Positions/axes are expressed in the stage-local
/// body frame where +Y is the longitudinal axis (nose-up).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThrustReference {
    SeaLevel,
    Vacuum,
}

#[derive(Clone, Debug)]
pub struct RocketEngine {
    /// Engine station in meters, relative to its stage cylinder's geometric
    /// center. This is never a full-stack coordinate.
    pub position_m: Vec3,
    /// Body-frame unit vector of the thrust force applied to the vehicle.
    pub thrust_axis: Vec3,
    pub isp_sea_level: f32,
    pub isp_vacuum: f32,
    pub gimbal_range_deg: f32,
    /// Full-throttle rated thrust, kilonewtons, at [`Self::thrust_reference`].
    pub rated_thrust_kn: f32,
    /// Ambient-pressure endpoint at which `rated_thrust_kn` is specified.
    pub thrust_reference: ThrustReference,
    /// Lowest commanded throttle the engine can hold (0..1).
    pub throttle_min: f32,
    /// Highest commanded throttle the engine can produce (0..1).
    pub throttle_max: f32,
    /// Catalogued lifetime start budget. This is configuration, while
    /// `ignition_count` and `state` are authoritative runtime lifecycle data.
    pub max_ignitions: u32,
    /// Number of successful starts since the vehicle was loaded or reset.
    pub ignition_count: u32,
    pub state: EngineState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineState {
    Off,
    Running,
    /// The engine cannot be started again during this vehicle life.
    Depleted,
}

impl RocketEngine {
    /// Restore the lifecycle that a freshly loaded or relaunched vehicle has.
    pub fn reset_lifecycle(&mut self) {
        self.ignition_count = 0;
        self.state = EngineState::Off;
    }

    /// Apply a commanded ignition or cutoff. A final permitted start remains
    /// running; it becomes terminal only when subsequently shut down.
    pub fn command_lifecycle(&mut self, run_commanded: bool, ignition_permitted: bool) {
        if !run_commanded {
            if self.state == EngineState::Running {
                self.state = if self.ignition_count >= self.max_ignitions {
                    EngineState::Depleted
                } else {
                    EngineState::Off
                };
            }
            return;
        }

        if self.state != EngineState::Off || !ignition_permitted {
            return;
        }
        if self.ignition_count >= self.max_ignitions {
            self.state = EngineState::Depleted;
            return;
        }
        self.ignition_count += 1;
        self.state = EngineState::Running;
    }

    /// Mark hardware unavailable for the remainder of this vehicle life, such
    /// as after its stage has exhausted all propellant.
    pub fn deplete(&mut self) {
        self.state = EngineState::Depleted;
    }
}

/// A rocket stage: structure plus propellant and its engines.
#[derive(Clone, Debug)]
pub struct RocketStage {
    pub name: String,
    /// Outer cylindrical diameter used by active-stage aerodynamic and inertia
    /// approximations, meters.
    pub diameter_m: f32,
    /// Physical stage length used by active-stage aerodynamic and inertia
    /// approximations, meters.
    pub height_m: f32,
    pub dry_mass_kg: f32,
    pub propellant_mass_kg: f32,
    /// Propellant retained by a recoverable lower stage after separation. The
    /// separated recovery vehicle owns and consumes this reserve through the
    /// normal propulsion pipeline; `None` keeps the stage expendable.
    pub recovery_propellant_reserve_kg: Option<f32>,
    /// Physical landing gear installed on this serial stage. The ECS contact
    /// component is assembled only for the currently active or recovering
    /// stage, never for the complete vehicle definition.
    pub landing_gear: Option<LandingGearSpec>,
    /// Payload-fairing dry mass attached to this serial stage, kg. Config
    /// validation permits this only on the final serial stage; runtime mass is
    /// still tracked by the active vehicle's authoritative payload inventory.
    pub fairing_dry_mass_kg: Option<f32>,
    pub engines: Vec<RocketEngine>,
}

/// Identical boosters mounted in parallel with the serial core stack.
/// Attachment positions are booster cylinder origins in the full stack body
/// frame; the engines in `stage` remain stage-local.
#[derive(Clone, Debug)]
pub struct ParallelBoosters {
    pub count: u32,
    pub stage: RocketStage,
    pub attachment_positions_m: Vec<Vec3>,
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
    pub parallel_boosters: Option<ParallelBoosters>,
}

impl Rocket {
    /// Restore every attached engine to the lifecycle of a freshly loaded
    /// vehicle. Serial stages and parallel boosters share the same reset rule.
    pub(crate) fn reset_engine_lifecycles(&mut self) {
        for stage in &mut self.stages {
            for engine in &mut stage.engines {
                engine.reset_lifecycle();
            }
        }
        if let Some(boosters) = &mut self.parallel_boosters {
            for engine in &mut boosters.stage.engines {
                engine.reset_lifecycle();
            }
        }
    }

    /// Stage-cylinder center in an attached cylindrical stack. The stack
    /// origin is its geometric center; any height beyond the stage cylinders
    /// represents attached adapters or fairing volume above the final stage.
    pub fn stage_origin_in_stack_m(
        stages: &[RocketStage],
        stack_height_m: f32,
        stage_index: usize,
    ) -> Option<Vec3> {
        let stage = stages.get(stage_index)?;
        let preceding_height_m: f32 = stages
            .iter()
            .take(stage_index)
            .map(|stage| stage.height_m)
            .sum();
        Some(Vec3::Y * (-stack_height_m * 0.5 + preceding_height_m + stage.height_m * 0.5))
    }

    /// Convert a stage-local engine station to the currently attached stack's
    /// body frame. A detached one-stage vehicle therefore uses its declared
    /// local engine station unchanged.
    pub fn engine_position_in_stack_m(
        stages: &[RocketStage],
        stack_height_m: f32,
        stage_index: usize,
        engine: &RocketEngine,
    ) -> Option<Vec3> {
        Some(
            Self::stage_origin_in_stack_m(stages, stack_height_m, stage_index)? + engine.position_m,
        )
    }

    /// Convert a parallel booster's local engine station into the attached
    /// full-stack frame. Detached boosters keep their local stations.
    pub fn parallel_booster_engine_position_in_stack_m(
        boosters: &ParallelBoosters,
        booster_index: usize,
        engine: &RocketEngine,
    ) -> Option<Vec3> {
        Some(*boosters.attachment_positions_m.get(booster_index)? + engine.position_m)
    }

    /// Lowest attached cylindrical extent in the full-stack body frame.
    pub fn lower_extent_in_stack_m(&self) -> f32 {
        let mut lower_y_m = -self.height_m * 0.5;
        if let Some(boosters) = &self.parallel_boosters {
            for attachment in &boosters.attachment_positions_m {
                lower_y_m = lower_y_m.min(attachment.y - boosters.stage.height_m * 0.5);
            }
        }
        lower_y_m
    }

    /// Hardcoded test fixture. The RON catalog is the runtime vehicle authority.
    #[cfg(test)]
    pub fn falcon9_test_fixture() -> Self {
        // Two-stage Falcon 9 preserving the original aggregate Falcon-9
        // parameters: 22 200 kg dry, 120 000 kg propellant, 7 607 kN total
        // first-stage thrust, 282/311 s ISP, 5° gimbal, 3.7 m diameter,
        // 70 m height.
        let stage1_engines = (0..9)
            .map(|i| {
                let angle = i as f32 * 2.0 * std::f32::consts::PI / 9.0;
                RocketEngine {
                    position_m: Vec3::new(1.2 * angle.cos(), -20.6, 1.2 * angle.sin()),
                    thrust_axis: Vec3::Y,
                    isp_sea_level: 282.0,
                    isp_vacuum: 311.0,
                    gimbal_range_deg: 5.0,
                    rated_thrust_kn: 7607.0 / 9.0,
                    thrust_reference: ThrustReference::SeaLevel,
                    throttle_min: 0.0,
                    throttle_max: 1.0,
                    max_ignitions: 3,
                    ignition_count: 1,
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
                    diameter_m: 3.7,
                    height_m: 41.2,
                    dry_mass_kg: 18_000.0,
                    propellant_mass_kg: 90_000.0,
                    recovery_propellant_reserve_kg: Some(15_000.0),
                    landing_gear: Some(LandingGearSpec {
                        count: 4,
                        base_radius_m: 4.5,
                        stroke_m: 3.0,
                        max_landing_mass_kg: Some(30_000.0),
                        deploy_altitude_m: 100.0,
                    }),
                    fairing_dry_mass_kg: None,
                    engines: stage1_engines,
                },
                RocketStage {
                    name: "Falcon 9 Stage 2".to_string(),
                    diameter_m: 3.7,
                    height_m: 13.2,
                    dry_mass_kg: 4_200.0,
                    propellant_mass_kg: 30_000.0,
                    recovery_propellant_reserve_kg: None,
                    landing_gear: None,
                    fairing_dry_mass_kg: None,
                    engines: vec![RocketEngine {
                        position_m: Vec3::new(0.0, -6.6, 0.0),
                        thrust_axis: Vec3::Y,
                        isp_sea_level: 311.0,
                        isp_vacuum: 348.0,
                        gimbal_range_deg: 5.0,
                        rated_thrust_kn: 934.0,
                        thrust_reference: ThrustReference::Vacuum,
                        throttle_min: 0.0,
                        throttle_max: 1.0,
                        max_ignitions: 2,
                        ignition_count: 1,
                        state: EngineState::Running,
                    }],
                },
            ],
            parallel_boosters: None,
        }
    }

    pub fn total_dry_mass_kg(&self) -> f32 {
        self.stages.iter().map(|s| s.dry_mass_kg).sum::<f32>()
            + self.parallel_boosters.as_ref().map_or(0.0, |boosters| {
                boosters.stage.dry_mass_kg * boosters.count as f32
            })
    }

    pub fn total_propellant_mass_kg(&self) -> f32 {
        self.stages
            .iter()
            .map(|s| s.propellant_mass_kg)
            .sum::<f32>()
            + self.parallel_boosters.as_ref().map_or(0.0, |boosters| {
                boosters.stage.propellant_mass_kg * boosters.count as f32
            })
    }

    pub fn total_mass_kg(&self) -> f32 {
        self.stages.iter().map(|s| s.total_mass_kg()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falcon9_fixture_preserves_aggregate_parameters() {
        let rocket = Rocket::falcon9_test_fixture();
        assert_eq!(rocket.stages.len(), 2);
        assert!(rocket.stages[0].landing_gear.is_some());
        assert!(rocket.stages[1].landing_gear.is_none());
        assert!((rocket.total_dry_mass_kg() - 22_200.0).abs() < 1.0);
        assert!((rocket.total_propellant_mass_kg() - 120_000.0).abs() < 1.0);
        assert!((rocket.total_mass_kg() - 142_200.0).abs() < 1.0);
        let rated_thrust_kn: f32 = rocket.stages[0]
            .engines
            .iter()
            .map(|engine| engine.rated_thrust_kn)
            .sum();
        assert!((rated_thrust_kn - 7_607.0).abs() < 1.0);
        assert_eq!(rocket.diameter_m, 3.7);
        assert_eq!(rocket.height_m, 70.0);
    }

    #[test]
    fn first_stage_has_nine_running_engines() {
        let rocket = Rocket::falcon9_test_fixture();
        let stage1 = &rocket.stages[0];
        assert_eq!(stage1.engines.len(), 9);
        assert!(stage1
            .engines
            .iter()
            .all(|e| e.state == EngineState::Running));
    }

    #[test]
    fn lifecycle_reset_restores_off_state_and_start_budget() {
        let mut engine = Rocket::falcon9_test_fixture().stages[0].engines[0].clone();
        engine.command_lifecycle(false, true);
        engine.command_lifecycle(true, true);
        assert_eq!(engine.ignition_count, 2);
        engine.reset_lifecycle();
        assert_eq!(engine.state, EngineState::Off);
        assert_eq!(engine.ignition_count, 0);
    }

    #[test]
    fn stack_conversion_translates_stage_local_engine_stations() {
        let rocket = Rocket::falcon9_test_fixture();
        let booster_engine = Rocket::engine_position_in_stack_m(
            &rocket.stages,
            rocket.height_m,
            0,
            &rocket.stages[0].engines[0],
        )
        .unwrap();
        let upper_engine = Rocket::engine_position_in_stack_m(
            &rocket.stages,
            rocket.height_m,
            1,
            &rocket.stages[1].engines[0],
        )
        .unwrap();

        assert!((booster_engine.y + rocket.height_m * 0.5).abs() < 1e-5);
        assert!((upper_engine.y - 6.2).abs() < 1e-5);
        assert_eq!(
            Rocket::engine_position_in_stack_m(
                &rocket.stages[1..],
                rocket.stages[1].height_m,
                0,
                &rocket.stages[1].engines[0],
            )
            .unwrap(),
            rocket.stages[1].engines[0].position_m,
            "a detached stage must preserve its declared local station"
        );
    }
}
