//! Long-arc propagation request, result, provenance, and acceleration-model
//! contract. Pure domain types; no ECS and no integrator internals.

use crate::domain::math::DVec3;

use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
use crate::domain::services::gravity::{ForceModelConfig, ForceModelTier};

use super::integrator::Dop853Integrator;

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
