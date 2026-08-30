//! Read-only contract for scientific long-arc trajectory propagation.
//!
//! This service is intentionally separate from the fixed rocket pipeline. It
//! predicts unpowered planet-centered inertial motion from an owned f64 state;
//! it never accepts ECS data or mutates authoritative flight, contact, or
//! propulsion state.

use bevy::math::DVec3;

use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
use crate::domain::services::gravity::{ForceModelConfig, ForceModelTier};

/// A translational vehicle state in a planet-centered inertial frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LongArcState {
    /// Position relative to the bound body's center, meters.
    pub position_m: DVec3,
    /// Velocity relative to the bound body's center, meters per second.
    pub velocity_mps: DVec3,
}

impl LongArcState {
    pub const fn new(position_m: DVec3, velocity_mps: DVec3) -> Self {
        Self {
            position_m,
            velocity_mps,
        }
    }

    pub fn is_finite(self) -> bool {
        self.position_m.is_finite() && self.velocity_mps.is_finite()
    }
}

/// The only supported state frame for the initial long-arc implementation.
///
/// The central body is recorded separately in the request and provenance. It
/// is required to evaluate the configured point-mass, J2, and differential
/// third-body terms without using a render or solar-map frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LongArcReferenceFrame {
    PlanetCenteredInertial,
}

/// Deterministic numerical method selected for long-arc propagation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LongArcIntegrationMethod {
    /// Embedded Dormand-Prince 8(5,3) with deterministic adaptive decisions.
    DormandPrince853,
}

/// Error-control and bounded-work policy for one long-arc request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LongArcIntegrationSettings {
    pub method: LongArcIntegrationMethod,
    /// Relative error tolerance shared by position and velocity state values.
    pub relative_tolerance: f64,
    /// Absolute position error tolerance, meters.
    pub absolute_position_tolerance_m: f64,
    /// Absolute velocity error tolerance, meters per second.
    pub absolute_velocity_tolerance_mps: f64,
    /// Largest accepted integration step, seconds.
    pub maximum_step_s: f64,
    /// Smallest attempted integration step, seconds.
    pub minimum_step_s: f64,
    /// Explicit work bound, independent of render-frame timing.
    pub maximum_steps: usize,
}

impl LongArcIntegrationSettings {
    pub fn is_valid(self) -> bool {
        self.relative_tolerance.is_finite()
            && self.relative_tolerance > 0.0
            && self.absolute_position_tolerance_m.is_finite()
            && self.absolute_position_tolerance_m > 0.0
            && self.absolute_velocity_tolerance_mps.is_finite()
            && self.absolute_velocity_tolerance_mps > 0.0
            && self.maximum_step_s.is_finite()
            && self.maximum_step_s > 0.0
            && self.minimum_step_s.is_finite()
            && self.minimum_step_s > 0.0
            && self.minimum_step_s <= self.maximum_step_s
            && self.maximum_steps > 0
    }
}

impl Default for LongArcIntegrationSettings {
    fn default() -> Self {
        Self {
            method: LongArcIntegrationMethod::DormandPrince853,
            relative_tolerance: 1.0e-10,
            absolute_position_tolerance_m: 1.0e-3,
            absolute_velocity_tolerance_mps: 1.0e-6,
            maximum_step_s: 60.0,
            minimum_step_s: 1.0e-3,
            maximum_steps: 1_000_000,
        }
    }
}

/// Long-arc scenarios with published numerical integration budgets.
///
/// These budgets bound residuals against a stricter integration of the same
/// documented force model. They validate the numerical method, not the physical
/// completeness of that model; task 7 records external reference trajectories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LongArcValidationScenario {
    Leo,
    EarthJ2Precession,
    LunarTransfer,
    EarthEscape,
}

/// Numerical residual limits for one documented long-arc scenario.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LongArcScenarioErrorBudget {
    pub scenario: LongArcValidationScenario,
    pub force_model: ForceModelTier,
    /// Largest validated propagation horizon, seconds.
    pub maximum_horizon_s: f64,
    /// Maximum checkpoint position residual, meters.
    pub maximum_position_residual_m: f64,
    /// Maximum checkpoint velocity residual, meters per second.
    pub maximum_velocity_residual_mps: f64,
}

impl LongArcValidationScenario {
    pub const fn error_budget(self) -> LongArcScenarioErrorBudget {
        match self {
            Self::Leo => LongArcScenarioErrorBudget {
                scenario: self,
                force_model: ForceModelTier::TwoBody,
                maximum_horizon_s: 86_400.0,
                maximum_position_residual_m: 1.0,
                maximum_velocity_residual_mps: 1.0e-3,
            },
            Self::EarthJ2Precession => LongArcScenarioErrorBudget {
                scenario: self,
                force_model: ForceModelTier::EarthJ2,
                maximum_horizon_s: 259_200.0,
                maximum_position_residual_m: 5.0,
                maximum_velocity_residual_mps: 5.0e-3,
            },
            Self::LunarTransfer => LongArcScenarioErrorBudget {
                scenario: self,
                force_model: ForceModelTier::EarthMoonSun,
                maximum_horizon_s: 259_200.0,
                maximum_position_residual_m: 100.0,
                maximum_velocity_residual_mps: 0.1,
            },
            Self::EarthEscape => LongArcScenarioErrorBudget {
                scenario: self,
                force_model: ForceModelTier::TwoBody,
                maximum_horizon_s: 259_200.0,
                maximum_position_residual_m: 10.0,
                maximum_velocity_residual_mps: 1.0e-2,
            },
        }
    }
}

/// Immutable inputs to one read-only long-arc propagation request.
#[derive(Clone, Debug, PartialEq)]
pub struct LongArcPropagationRequest {
    pub initial_state: LongArcState,
    pub start_epoch: TdbEpoch,
    pub central_body: NaifBodyId,
    pub force_model: ForceModelConfig,
    pub settings: LongArcIntegrationSettings,
    /// Propagation duration after `start_epoch`, seconds.
    pub horizon_s: f64,
    /// Ordered output times after `start_epoch`, seconds.
    pub checkpoint_offsets_s: Vec<f64>,
}

impl LongArcPropagationRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        initial_state: LongArcState,
        start_epoch: TdbEpoch,
        central_body: NaifBodyId,
        force_model: ForceModelConfig,
        settings: LongArcIntegrationSettings,
        horizon_s: f64,
        checkpoint_offsets_s: Vec<f64>,
    ) -> Result<Self, LongArcPropagationError> {
        let request = Self {
            initial_state,
            start_epoch,
            central_body,
            force_model,
            settings,
            horizon_s,
            checkpoint_offsets_s,
        };
        request.validate()?;
        Ok(request)
    }

    pub const fn reference_frame(&self) -> LongArcReferenceFrame {
        LongArcReferenceFrame::PlanetCenteredInertial
    }

    pub fn validate(&self) -> Result<(), LongArcPropagationError> {
        if !self.initial_state.is_finite() {
            return Err(LongArcPropagationError::NonFiniteInitialState);
        }
        if !self.horizon_s.is_finite() || self.horizon_s <= 0.0 {
            return Err(LongArcPropagationError::InvalidHorizon);
        }
        if !self.settings.is_valid() {
            return Err(LongArcPropagationError::InvalidIntegrationSettings);
        }

        let mut previous_offset_s = None;
        for (index, &offset_s) in self.checkpoint_offsets_s.iter().enumerate() {
            if !offset_s.is_finite() || offset_s < 0.0 || offset_s > self.horizon_s {
                return Err(LongArcPropagationError::InvalidCheckpoint { index });
            }
            if previous_offset_s.is_some_and(|previous| offset_s <= previous) {
                return Err(LongArcPropagationError::UnorderedCheckpoints { index });
            }
            previous_offset_s = Some(offset_s);
        }
        Ok(())
    }

    pub fn epoch_at_offset_s(&self, offset_s: f64) -> Result<TdbEpoch, LongArcPropagationError> {
        if !offset_s.is_finite() || offset_s < 0.0 || offset_s > self.horizon_s {
            return Err(LongArcPropagationError::InvalidCheckpoint { index: 0 });
        }
        TdbEpoch::from_seconds_since_j2000(self.start_epoch.seconds_since_j2000() + offset_s)
            .map_err(|_| LongArcPropagationError::InvalidEpoch)
    }

    pub fn provenance(&self) -> LongArcPropagationProvenance {
        LongArcPropagationProvenance {
            start_epoch: self.start_epoch,
            central_body: self.central_body,
            reference_frame: self.reference_frame(),
            force_model: self.force_model,
            settings: self.settings,
        }
    }

    /// Propagate this immutable request through one typed acceleration model.
    pub fn propagate_with(
        &self,
        acceleration_model: &dyn LongArcAccelerationModel,
    ) -> Result<LongArcPropagationResult, LongArcPropagationError> {
        Dop853Integrator::new(self, acceleration_model).propagate()
    }
}

/// A state emitted at a requested same-epoch propagation checkpoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LongArcCheckpoint {
    pub offset_s: f64,
    pub epoch: TdbEpoch,
    pub state: LongArcState,
}

/// Configuration provenance recorded with each propagation result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LongArcPropagationProvenance {
    pub start_epoch: TdbEpoch,
    pub central_body: NaifBodyId,
    pub reference_frame: LongArcReferenceFrame,
    pub force_model: ForceModelConfig,
    pub settings: LongArcIntegrationSettings,
}

/// Read-only output from the long-arc propagator.
#[derive(Clone, Debug, PartialEq)]
pub struct LongArcPropagationResult {
    pub final_epoch: TdbEpoch,
    pub final_state: LongArcState,
    pub checkpoints: Vec<LongArcCheckpoint>,
    pub accepted_steps: usize,
    pub rejected_steps: usize,
    pub provenance: LongArcPropagationProvenance,
}

/// Input validation failures reported before propagation starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LongArcPropagationError {
    NonFiniteInitialState,
    InvalidEpoch,
    InvalidHorizon,
    InvalidIntegrationSettings,
    InvalidCentralGravitationalParameter,
    InvalidCheckpoint {
        index: usize,
    },
    UnorderedCheckpoints {
        index: usize,
    },
    ForceModelMismatch {
        expected: ForceModelTier,
        actual: ForceModelTier,
    },
    NonFiniteAcceleration,
    StepSizeUnderflow,
    StepLimitExceeded,
}

/// Typed source of acceleration for one immutable long-arc request.
///
/// Implementations receive the exact stage epoch and the selected force-model
/// configuration. They must use the shared domain gravity and ephemeris
/// authorities; they cannot access or mutate ECS state.
pub trait LongArcAccelerationModel {
    fn acceleration_mps2(
        &self,
        epoch: TdbEpoch,
        state: LongArcState,
        force_model: ForceModelConfig,
    ) -> Result<DVec3, LongArcPropagationError>;
}

/// Shared two-body acceleration model for coast-only propagation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TwoBodyAccelerationModel {
    central_mu_m3_s2: f64,
}

impl TwoBodyAccelerationModel {
    pub fn new(central_mu_m3_s2: f64) -> Result<Self, LongArcPropagationError> {
        if !central_mu_m3_s2.is_finite() || central_mu_m3_s2 <= 0.0 {
            return Err(LongArcPropagationError::InvalidCentralGravitationalParameter);
        }
        Ok(Self { central_mu_m3_s2 })
    }
}

impl LongArcAccelerationModel for TwoBodyAccelerationModel {
    fn acceleration_mps2(
        &self,
        _: TdbEpoch,
        state: LongArcState,
        force_model: ForceModelConfig,
    ) -> Result<DVec3, LongArcPropagationError> {
        if force_model.tier() != ForceModelTier::TwoBody {
            return Err(LongArcPropagationError::ForceModelMismatch {
                expected: ForceModelTier::TwoBody,
                actual: force_model.tier(),
            });
        }
        Ok(
            crate::domain::services::gravity::gravitational_acceleration_from_mu(
                self.central_mu_m3_s2,
                state.position_m,
                DVec3::ZERO,
            ),
        )
    }
}

#[derive(Clone, Copy)]
struct StateDerivative {
    position_mps: DVec3,
    velocity_mps2: DVec3,
}

const DOP853_C: [f64; 12] = [
    0.0,
    5.260_015_195_876_773e-2,
    7.890_022_793_815_16e-2,
    1.183_503_419_072_274e-1,
    2.816_496_580_927_726e-1,
    1.0 / 3.0,
    0.25,
    4.0 / 13.0,
    6.512_820_512_820_513e-1,
    0.6,
    6.0 / 7.0,
    1.0,
];

const DOP853_A: [[f64; 12]; 12] = [
    [0.0; 12],
    [
        5.260_015_195_876_773e-2,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ],
    [
        1.972_505_698_453_79e-2,
        5.917_517_095_361_37e-2,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ],
    [
        2.958_758_547_680_685e-2,
        0.0,
        8.876_275_643_042_055e-2,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ],
    [
        2.413_651_341_592_667e-1,
        0.0,
        -8.845_494_793_282_86e-1,
        9.248_340_032_617_92e-1,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ],
    [
        3.703_703_703_703_704e-2,
        0.0,
        0.0,
        1.708_286_087_294_738_7e-1,
        1.254_676_875_668_224_3e-1,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ],
    [
        3.710_937_5e-2,
        0.0,
        0.0,
        1.702_522_110_195_440_4e-1,
        6.021_653_898_045_596e-2,
        -1.757_812_5e-2,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ],
    [
        3.709_200_011_850_479_3e-2,
        0.0,
        0.0,
        1.703_839_257_122_399_9e-1,
        1.072_620_304_463_732_8e-1,
        -1.531_943_774_862_440_2e-2,
        8.273_789_163_814_023e-3,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ],
    [
        6.241_109_587_160_757e-1,
        0.0,
        0.0,
        -3.360_892_629_446_941_3,
        -8.682_193_468_417_26e-1,
        2.759_209_969_944_671e1,
        2.015_406_755_047_789_3e1,
        -4.348_988_418_106_995_5e1,
        0.0,
        0.0,
        0.0,
        0.0,
    ],
    [
        4.776_625_364_382_644e-1,
        0.0,
        0.0,
        -2.488_114_619_971_668,
        -5.902_908_268_368_43e-1,
        2.123_005_144_818_119_5e1,
        1.527_923_363_288_242_1e1,
        -3.328_821_096_898_486e1,
        -2.033_120_170_850_862_6e-2,
        0.0,
        0.0,
        0.0,
    ],
    [
        -9.371_424_300_859_873e-1,
        0.0,
        0.0,
        5.186_372_428_844_063,
        1.091_437_348_996_729_6,
        -8.149_787_010_746_926,
        -1.852_006_565_999_966e1,
        2.273_948_709_935_05e1,
        2.493_605_552_679_652_4,
        -3.046_764_471_898_219_6,
        0.0,
        0.0,
    ],
    [
        2.273_310_147_516_538,
        0.0,
        0.0,
        -1.053_449_546_673_725e1,
        -2.000_872_058_224_862_5,
        -1.795_893_186_311_88e1,
        2.794_888_452_941_996e1,
        -2.858_998_277_135_054,
        -8.872_856_933_530_63,
        1.236_056_717_579_430_4e1,
        6.433_927_460_157_635e-1,
        0.0,
    ],
];

const DOP853_B: [f64; 12] = [
    5.429_373_411_656_876e-2,
    0.0,
    0.0,
    0.0,
    0.0,
    4.450_312_892_752_409,
    1.891_517_899_314_500_3,
    -5.801_203_960_010_585,
    3.111_643_669_578_199e-1,
    -1.521_609_496_625_161e-1,
    2.013_654_008_040_304_8e-1,
    4.471_061_572_777_259e-2,
];

const DOP853_E3: [f64; 13] = [
    -1.897_807_541_072_407_7e-1,
    0.0,
    0.0,
    0.0,
    0.0,
    4.450_312_892_752_409,
    1.891_517_899_314_500_3,
    -5.801_203_960_010_585,
    -4.222_030_213_237_919e-1,
    -1.521_609_496_625_161e-1,
    2.013_654_008_040_304_8e-1,
    2.265_179_219_836_082e-2,
    0.0,
];

const DOP853_E5: [f64; 13] = [
    1.312_004_499_419_488e-2,
    0.0,
    0.0,
    0.0,
    0.0,
    -1.225_156_446_376_204_4,
    -4.957_589_496_572_502e-1,
    1.664_377_182_454_986_6,
    -3.503_288_487_499_736_8e-1,
    3.341_791_181_178_015e-1,
    8.192_320_648_511_571e-2,
    -2.235_530_786_388_629e-2,
    0.0,
];

struct Dop853Integrator<'a> {
    request: &'a LongArcPropagationRequest,
    acceleration_model: &'a dyn LongArcAccelerationModel,
}

impl<'a> Dop853Integrator<'a> {
    fn new(
        request: &'a LongArcPropagationRequest,
        acceleration_model: &'a dyn LongArcAccelerationModel,
    ) -> Self {
        Self {
            request,
            acceleration_model,
        }
    }

    fn propagate(&self) -> Result<LongArcPropagationResult, LongArcPropagationError> {
        self.request.validate()?;

        let mut state = self.request.initial_state;
        let mut elapsed_s = 0.0;
        let mut step_s = self
            .request
            .settings
            .maximum_step_s
            .min(self.request.horizon_s);
        let mut accepted_steps = 0;
        let mut rejected_steps = 0;
        let mut checkpoints = Vec::with_capacity(self.request.checkpoint_offsets_s.len());

        for target_index in 0..=self.request.checkpoint_offsets_s.len() {
            let is_checkpoint = target_index < self.request.checkpoint_offsets_s.len();
            let target_s = if is_checkpoint {
                self.request.checkpoint_offsets_s[target_index]
            } else {
                self.request.horizon_s
            };
            if target_s == elapsed_s {
                if is_checkpoint {
                    checkpoints.push(self.checkpoint(target_s, state)?);
                }
                continue;
            }

            while elapsed_s < target_s {
                if accepted_steps + rejected_steps >= self.request.settings.maximum_steps {
                    return Err(LongArcPropagationError::StepLimitExceeded);
                }
                let remaining_s = target_s - elapsed_s;
                let clipped_to_target = step_s >= remaining_s;
                let trial_step_s = step_s.min(remaining_s);
                let (candidate, error_norm) = self.step(state, elapsed_s, trial_step_s)?;

                if error_norm <= 1.0 {
                    state = candidate;
                    elapsed_s = if clipped_to_target {
                        target_s
                    } else {
                        elapsed_s + trial_step_s
                    };
                    accepted_steps += 1;
                    step_s = self.next_step_size(trial_step_s, error_norm, true);
                    continue;
                }

                rejected_steps += 1;
                step_s = self.next_step_size(trial_step_s, error_norm, false);
                if step_s < self.request.settings.minimum_step_s {
                    return Err(LongArcPropagationError::StepSizeUnderflow);
                }
            }

            if is_checkpoint {
                checkpoints.push(self.checkpoint(target_s, state)?);
            }
        }

        Ok(LongArcPropagationResult {
            final_epoch: self.request.epoch_at_offset_s(self.request.horizon_s)?,
            final_state: state,
            checkpoints,
            accepted_steps,
            rejected_steps,
            provenance: self.request.provenance(),
        })
    }

    fn checkpoint(
        &self,
        offset_s: f64,
        state: LongArcState,
    ) -> Result<LongArcCheckpoint, LongArcPropagationError> {
        Ok(LongArcCheckpoint {
            offset_s,
            epoch: self.request.epoch_at_offset_s(offset_s)?,
            state,
        })
    }

    fn step(
        &self,
        state: LongArcState,
        start_offset_s: f64,
        step_s: f64,
    ) -> Result<(LongArcState, f64), LongArcPropagationError> {
        let mut stages = [StateDerivative {
            position_mps: DVec3::ZERO,
            velocity_mps2: DVec3::ZERO,
        }; 13];
        stages[0] = self.derivative(state, self.request.epoch_at_offset_s(start_offset_s)?)?;

        for stage in 1..12 {
            let mut derivative_sum = StateDerivative {
                position_mps: DVec3::ZERO,
                velocity_mps2: DVec3::ZERO,
            };
            for (prior, coefficient) in DOP853_A[stage][..stage].iter().enumerate() {
                derivative_sum.position_mps += stages[prior].position_mps * coefficient;
                derivative_sum.velocity_mps2 += stages[prior].velocity_mps2 * coefficient;
            }
            let stage_state = LongArcState {
                position_m: state.position_m + derivative_sum.position_mps * step_s,
                velocity_mps: state.velocity_mps + derivative_sum.velocity_mps2 * step_s,
            };
            let stage_epoch = self
                .request
                .epoch_at_offset_s(start_offset_s + DOP853_C[stage] * step_s)?;
            stages[stage] = self.derivative(stage_state, stage_epoch)?;
        }

        let mut weighted_sum = StateDerivative {
            position_mps: DVec3::ZERO,
            velocity_mps2: DVec3::ZERO,
        };
        for (stage, coefficient) in DOP853_B.iter().enumerate() {
            weighted_sum.position_mps += stages[stage].position_mps * coefficient;
            weighted_sum.velocity_mps2 += stages[stage].velocity_mps2 * coefficient;
        }
        let candidate = LongArcState {
            position_m: state.position_m + weighted_sum.position_mps * step_s,
            velocity_mps: state.velocity_mps + weighted_sum.velocity_mps2 * step_s,
        };
        let end_epoch = self.request.epoch_at_offset_s(start_offset_s + step_s)?;
        stages[12] = self.derivative(candidate, end_epoch)?;

        Ok((
            candidate,
            self.error_norm(state, candidate, &stages, step_s),
        ))
    }

    fn derivative(
        &self,
        state: LongArcState,
        epoch: TdbEpoch,
    ) -> Result<StateDerivative, LongArcPropagationError> {
        let acceleration_mps2 =
            self.acceleration_model
                .acceleration_mps2(epoch, state, self.request.force_model)?;
        if !acceleration_mps2.is_finite() {
            return Err(LongArcPropagationError::NonFiniteAcceleration);
        }
        Ok(StateDerivative {
            position_mps: state.velocity_mps,
            velocity_mps2: acceleration_mps2,
        })
    }

    fn error_norm(
        &self,
        state: LongArcState,
        candidate: LongArcState,
        stages: &[StateDerivative; 13],
        step_s: f64,
    ) -> f64 {
        let settings = self.request.settings;
        let mut error3 = StateDerivative {
            position_mps: DVec3::ZERO,
            velocity_mps2: DVec3::ZERO,
        };
        let mut error5 = error3;
        for stage in 0..13 {
            error3.position_mps += stages[stage].position_mps * DOP853_E3[stage];
            error3.velocity_mps2 += stages[stage].velocity_mps2 * DOP853_E3[stage];
            error5.position_mps += stages[stage].position_mps * DOP853_E5[stage];
            error5.velocity_mps2 += stages[stage].velocity_mps2 * DOP853_E5[stage];
        }

        let position_scale_m = DVec3::new(
            settings.absolute_position_tolerance_m
                + settings.relative_tolerance
                    * state.position_m.x.abs().max(candidate.position_m.x.abs()),
            settings.absolute_position_tolerance_m
                + settings.relative_tolerance
                    * state.position_m.y.abs().max(candidate.position_m.y.abs()),
            settings.absolute_position_tolerance_m
                + settings.relative_tolerance
                    * state.position_m.z.abs().max(candidate.position_m.z.abs()),
        );
        let velocity_scale_mps = DVec3::new(
            settings.absolute_velocity_tolerance_mps
                + settings.relative_tolerance
                    * state
                        .velocity_mps
                        .x
                        .abs()
                        .max(candidate.velocity_mps.x.abs()),
            settings.absolute_velocity_tolerance_mps
                + settings.relative_tolerance
                    * state
                        .velocity_mps
                        .y
                        .abs()
                        .max(candidate.velocity_mps.y.abs()),
            settings.absolute_velocity_tolerance_mps
                + settings.relative_tolerance
                    * state
                        .velocity_mps
                        .z
                        .abs()
                        .max(candidate.velocity_mps.z.abs()),
        );
        let scaled_square = |derivative: StateDerivative| {
            let position = derivative.position_mps * step_s / position_scale_m;
            let velocity = derivative.velocity_mps2 * step_s / velocity_scale_mps;
            position.length_squared() + velocity.length_squared()
        };
        let error3_sq = scaled_square(error3);
        let error5_sq = scaled_square(error5);
        let denominator = error5_sq + 0.01 * error3_sq;
        if denominator <= f64::EPSILON {
            0.0
        } else {
            error5_sq / (denominator * 6.0).sqrt()
        }
    }

    fn next_step_size(&self, step_s: f64, error_norm: f64, accepted: bool) -> f64 {
        const SAFETY: f64 = 0.9;
        const MIN_FACTOR: f64 = 0.2;
        const MAX_FACTOR: f64 = 5.0;

        let factor = if error_norm <= f64::EPSILON {
            MAX_FACTOR
        } else {
            (SAFETY * error_norm.powf(-1.0 / 8.0)).clamp(MIN_FACTOR, MAX_FACTOR)
        };
        let factor = if accepted { factor } else { factor.min(1.0) };
        (step_s * factor).min(self.request.settings.maximum_step_s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::gravity::{
        differential_gravitational_acceleration_from_mu, earth_j2_acceleration,
        gravitational_acceleration_from_mu, EarthJ2GravityModel,
    };

    const EARTH_MU_M3_S2: f64 = 3.986_004_355_070_227e14;
    const MOON_MU_M3_S2: f64 = 4.904_869_5e12;
    const SUN_MU_M3_S2: f64 = 1.327_124_400_18e20;

    fn earth_j2_model() -> EarthJ2GravityModel {
        EarthJ2GravityModel {
            model_id: "EGM2008".to_string(),
            reference_radius_m: 6_378_136.3,
            j2: 1.082_626_173_852_222_7e-3,
        }
    }

    struct ZeroAccelerationModel;

    impl LongArcAccelerationModel for ZeroAccelerationModel {
        fn acceleration_mps2(
            &self,
            _: TdbEpoch,
            _: LongArcState,
            _: ForceModelConfig,
        ) -> Result<DVec3, LongArcPropagationError> {
            Ok(DVec3::ZERO)
        }
    }

    fn request() -> LongArcPropagationRequest {
        LongArcPropagationRequest::new(
            LongArcState::new(
                DVec3::new(6_778_136.3, 0.0, 0.0),
                DVec3::new(0.0, 0.0, 7_668.6),
            ),
            TdbEpoch::j2000(),
            NaifBodyId::EARTH,
            ForceModelConfig::new(ForceModelTier::EarthMoonSun),
            LongArcIntegrationSettings::default(),
            7_200.0,
            vec![0.0, 900.0, 3_600.0, 7_200.0],
        )
        .expect("valid long-arc request")
    }

    #[test]
    fn request_records_the_scientific_state_and_result_provenance() {
        let request = request();
        let provenance = request.provenance();

        assert_eq!(
            request.reference_frame(),
            LongArcReferenceFrame::PlanetCenteredInertial
        );
        assert_eq!(provenance.start_epoch, TdbEpoch::j2000());
        assert_eq!(provenance.central_body, NaifBodyId::EARTH);
        assert_eq!(provenance.force_model.tier(), ForceModelTier::EarthMoonSun);
        assert_eq!(
            provenance.settings.method,
            LongArcIntegrationMethod::DormandPrince853
        );
        assert!(
            (request
                .epoch_at_offset_s(3_600.0)
                .unwrap()
                .seconds_since_j2000()
                - 3_600.0)
                .abs()
                < 1.0e-4
        );
    }

    #[test]
    fn invalid_state_settings_and_checkpoints_are_rejected_before_propagation() {
        let mut invalid_state = request();
        invalid_state.initial_state.position_m = DVec3::NAN;
        assert_eq!(
            invalid_state.validate(),
            Err(LongArcPropagationError::NonFiniteInitialState)
        );

        let mut invalid_settings = request();
        invalid_settings.settings.minimum_step_s = 120.0;
        assert_eq!(
            invalid_settings.validate(),
            Err(LongArcPropagationError::InvalidIntegrationSettings)
        );

        let invalid_checkpoint = LongArcPropagationRequest::new(
            request().initial_state,
            TdbEpoch::j2000(),
            NaifBodyId::EARTH,
            ForceModelConfig::default(),
            LongArcIntegrationSettings::default(),
            60.0,
            vec![30.0, 30.0],
        );
        assert_eq!(
            invalid_checkpoint,
            Err(LongArcPropagationError::UnorderedCheckpoints { index: 1 })
        );
    }

    #[test]
    fn dop853_two_body_coast_is_deterministic_and_records_exact_checkpoints() {
        let radius_m = 6_778_136.3;
        let circular_speed_mps = (EARTH_MU_M3_S2 / radius_m).sqrt();
        let period_s = std::f64::consts::TAU * (radius_m.powi(3) / EARTH_MU_M3_S2).sqrt();
        let request = LongArcPropagationRequest::new(
            LongArcState::new(DVec3::X * radius_m, DVec3::Y * circular_speed_mps),
            TdbEpoch::j2000(),
            NaifBodyId::EARTH,
            ForceModelConfig::new(ForceModelTier::TwoBody),
            LongArcIntegrationSettings {
                relative_tolerance: 1.0e-11,
                maximum_step_s: 120.0,
                ..Default::default()
            },
            period_s,
            vec![period_s * 0.25, period_s],
        )
        .unwrap();
        let two_body = TwoBodyAccelerationModel::new(EARTH_MU_M3_S2).unwrap();

        let first = request.propagate_with(&two_body).unwrap();
        let second = request.propagate_with(&two_body).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.checkpoints.len(), 2);
        assert_eq!(first.checkpoints[0].offset_s, period_s * 0.25);
        assert_eq!(first.checkpoints[1].offset_s, period_s);
        assert!(
            first
                .final_state
                .position_m
                .distance(request.initial_state.position_m)
                < 1.0e-2,
            "one-period position residual was {} m",
            first
                .final_state
                .position_m
                .distance(request.initial_state.position_m)
        );
        assert!(
            first
                .final_state
                .velocity_mps
                .distance(request.initial_state.velocity_mps)
                < 1.0e-5,
            "one-period velocity residual was {} m/s",
            first
                .final_state
                .velocity_mps
                .distance(request.initial_state.velocity_mps)
        );
        assert!(first.accepted_steps > 0);
    }

    #[test]
    fn propagation_stops_at_the_declared_work_bound_without_mutating_the_request() {
        let mut request = request();
        request.horizon_s = 100.0;
        request.checkpoint_offsets_s.clear();
        request.settings.maximum_step_s = 1.0;
        request.settings.maximum_steps = 1;
        let original_state = request.initial_state;

        assert_eq!(
            request.propagate_with(&ZeroAccelerationModel),
            Err(LongArcPropagationError::StepLimitExceeded)
        );
        assert_eq!(request.initial_state, original_state);
    }

    impl LongArcValidationScenario {
        fn test_request(self) -> LongArcPropagationRequest {
            let budget = self.error_budget();
            let radius_m = 6_778_136.3;
            let circular_speed_mps = (EARTH_MU_M3_S2 / radius_m).sqrt();
            let initial_state = match self {
                LongArcValidationScenario::Leo => {
                    LongArcState::new(DVec3::X * radius_m, DVec3::Y * circular_speed_mps)
                }
                LongArcValidationScenario::EarthJ2Precession => {
                    let inclination_rad = 98.0_f64.to_radians();
                    LongArcState::new(
                        DVec3::X * radius_m,
                        DVec3::new(
                            0.0,
                            circular_speed_mps * inclination_rad.cos(),
                            circular_speed_mps * inclination_rad.sin(),
                        ),
                    )
                }
                LongArcValidationScenario::LunarTransfer => LongArcState::new(
                    DVec3::X * radius_m,
                    DVec3::Y * (EARTH_MU_M3_S2 * (2.0 / radius_m - 1.0 / 195_000_000.0)).sqrt(),
                ),
                LongArcValidationScenario::EarthEscape => LongArcState::new(
                    DVec3::X * radius_m,
                    DVec3::Y * (2.0 * EARTH_MU_M3_S2 / radius_m).sqrt() * 1.01,
                ),
            };

            LongArcPropagationRequest::new(
                initial_state,
                TdbEpoch::j2000(),
                NaifBodyId::EARTH,
                ForceModelConfig::new(budget.force_model),
                LongArcIntegrationSettings::default(),
                budget.maximum_horizon_s,
                vec![
                    budget.maximum_horizon_s * 0.25,
                    budget.maximum_horizon_s * 0.5,
                    budget.maximum_horizon_s * 0.75,
                    budget.maximum_horizon_s,
                ],
            )
            .expect("valid scenario request")
        }
    }

    struct ScenarioAccelerationModel {
        scenario: LongArcValidationScenario,
    }

    impl ScenarioAccelerationModel {
        const fn new(scenario: LongArcValidationScenario) -> Self {
            Self { scenario }
        }
    }

    impl LongArcAccelerationModel for ScenarioAccelerationModel {
        fn acceleration_mps2(
            &self,
            epoch: TdbEpoch,
            state: LongArcState,
            force_model: ForceModelConfig,
        ) -> Result<DVec3, LongArcPropagationError> {
            let scenario = self.scenario;
            if force_model.tier() != scenario.error_budget().force_model {
                return Err(LongArcPropagationError::ForceModelMismatch {
                    expected: scenario.error_budget().force_model,
                    actual: force_model.tier(),
                });
            }
            let point_mass =
                gravitational_acceleration_from_mu(EARTH_MU_M3_S2, state.position_m, DVec3::ZERO);
            Ok(match scenario {
                LongArcValidationScenario::Leo | LongArcValidationScenario::EarthEscape => {
                    point_mass
                }
                LongArcValidationScenario::EarthJ2Precession => {
                    point_mass
                        + earth_j2_acceleration(
                            EARTH_MU_M3_S2,
                            state.position_m,
                            DVec3::Z,
                            &earth_j2_model(),
                        )
                }
                LongArcValidationScenario::LunarTransfer => {
                    // This deterministic circular Moon fixture exercises same-epoch
                    // third-body integration. External DE440 checkpoints arrive in
                    // the separate scientific-validation suite.
                    let moon_period_s = 27.321_661 * 86_400.0;
                    let moon_angle_rad =
                        std::f64::consts::TAU * epoch.seconds_since_j2000() / moon_period_s;
                    let moon_position_m = DVec3::new(
                        384_400_000.0 * moon_angle_rad.cos(),
                        384_400_000.0 * moon_angle_rad.sin(),
                        20_000_000.0 * moon_angle_rad.sin(),
                    );
                    point_mass
                        + differential_gravitational_acceleration_from_mu(
                            MOON_MU_M3_S2,
                            state.position_m,
                            DVec3::ZERO,
                            moon_position_m,
                        )
                        + differential_gravitational_acceleration_from_mu(
                            SUN_MU_M3_S2,
                            state.position_m,
                            DVec3::ZERO,
                            DVec3::new(-149_597_870_700.0, 0.0, 0.0),
                        )
                }
            })
        }
    }

    fn assert_scenario_error_budget(scenario: LongArcValidationScenario) {
        let request = scenario.test_request();
        let acceleration_model = ScenarioAccelerationModel::new(scenario);
        let reference_request = LongArcPropagationRequest {
            settings: LongArcIntegrationSettings {
                relative_tolerance: 1.0e-13,
                absolute_position_tolerance_m: 1.0e-6,
                absolute_velocity_tolerance_mps: 1.0e-9,
                maximum_step_s: 5.0,
                minimum_step_s: 1.0e-5,
                maximum_steps: 1_000_000,
                ..request.settings
            },
            ..request.clone()
        };
        let actual = request
            .propagate_with(&acceleration_model)
            .expect("scenario propagation should complete");
        let reference = reference_request
            .propagate_with(&acceleration_model)
            .expect("stricter reference propagation should complete");
        let budget = scenario.error_budget();

        for (actual_checkpoint, reference_checkpoint) in
            actual.checkpoints.iter().zip(reference.checkpoints.iter())
        {
            let position_residual_m = actual_checkpoint
                .state
                .position_m
                .distance(reference_checkpoint.state.position_m);
            let velocity_residual_mps = actual_checkpoint
                .state
                .velocity_mps
                .distance(reference_checkpoint.state.velocity_mps);
            assert!(
                position_residual_m <= budget.maximum_position_residual_m,
                "{scenario:?} position residual at {} s was {position_residual_m} m; budget {} m",
                actual_checkpoint.offset_s,
                budget.maximum_position_residual_m,
            );
            assert!(
                velocity_residual_mps <= budget.maximum_velocity_residual_mps,
                "{scenario:?} velocity residual at {} s was {velocity_residual_mps} m/s; budget {} m/s",
                actual_checkpoint.offset_s,
                budget.maximum_velocity_residual_mps,
            );
        }
    }

    #[test]
    fn leo_checkpoints_meet_the_published_numerical_budget() {
        assert_scenario_error_budget(LongArcValidationScenario::Leo);
    }

    #[test]
    fn j2_precessing_checkpoints_meet_the_published_numerical_budget() {
        assert_scenario_error_budget(LongArcValidationScenario::EarthJ2Precession);
    }

    #[test]
    fn lunar_transfer_checkpoints_meet_the_published_numerical_budget() {
        assert_scenario_error_budget(LongArcValidationScenario::LunarTransfer);
    }

    #[test]
    fn escape_checkpoints_meet_the_published_numerical_budget() {
        assert_scenario_error_budget(LongArcValidationScenario::EarthEscape);
    }
}
