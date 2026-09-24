//! Adaptive DOP853 (Runge-Kutta 8(5,3)) integrator for unpowered long-arc
//! propagation. Deterministic; consumes the pure acceleration-model contract.

use super::model::{
    LongArcAccelerationModel, LongArcCheckpoint, LongArcPropagationError,
    LongArcPropagationRequest, LongArcPropagationResult, LongArcState,
};
use crate::domain::math::DVec3;
use crate::domain::services::ephemeris::TdbEpoch;

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

pub(super) struct Dop853Integrator<'a> {
    request: &'a LongArcPropagationRequest,
    acceleration_model: &'a dyn LongArcAccelerationModel,
}

impl<'a> Dop853Integrator<'a> {
    pub(super) fn new(
        request: &'a LongArcPropagationRequest,
        acceleration_model: &'a dyn LongArcAccelerationModel,
    ) -> Self {
        Self {
            request,
            acceleration_model,
        }
    }

    pub(super) fn propagate(&self) -> Result<LongArcPropagationResult, LongArcPropagationError> {
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
