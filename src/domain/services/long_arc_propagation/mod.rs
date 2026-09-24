//! Read-only contract for scientific long-arc trajectory propagation.
//!
//! This service is intentionally separate from the fixed rocket pipeline. It
//! predicts unpowered planet-centered inertial motion from an owned f64 state;
//! it never accepts ECS data or mutates authoritative flight, contact, or
//! propulsion state.
//!
//! The implementation is split into cohesive submodules: [`model`] (request,
//! result, provenance, errors, and the acceleration-model contract) and
//! [`integrator`] (the adaptive DOP853 integrator).

mod integrator;
mod model;

pub use model::{
    LongArcAccelerationModel, LongArcCheckpoint, LongArcIntegrationMethod,
    LongArcIntegrationSettings, LongArcPropagationError, LongArcPropagationProvenance,
    LongArcPropagationRequest, LongArcPropagationResult, LongArcReferenceFrame,
    LongArcScenarioErrorBudget, LongArcState, LongArcValidationScenario, TwoBodyAccelerationModel,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::math::DVec3;
    use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
    use crate::domain::services::gravity::{
        differential_gravitational_acceleration_from_mu, earth_j2_acceleration,
        gravitational_acceleration_from_mu, EarthJ2GravityModel, ForceModelConfig, ForceModelTier,
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
