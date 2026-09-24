//! Versioned, offline scientific-reference case contracts.
//!
//! Cases record externally generated values and their provenance. This module
//! only validates the data contract; evaluating cases belongs to the offline
//! scientific-validation runner.
//!
//! The implementation is split into cohesive submodules: [`cases`] (the
//! versioned reference-case schema and its contract validation) and [`runner`]
//! (the deterministic state-authority contract and the offline evaluator).

mod cases;
mod runner;

pub use cases::*;
pub use runner::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};

    fn header(id: &str) -> ScientificReferenceCaseHeader {
        ScientificReferenceCaseHeader {
            metadata: ScientificReferenceMetadata {
                id: ScientificReferenceCaseId::new(id),
                source: ScientificReferenceSource {
                    provider: ScientificReferenceProvider::JplHorizons,
                    url: "https://ssd.jpl.nasa.gov/horizons/".to_string(),
                    source_version: "DE441".to_string(),
                },
                generation_command: "horizons_batch --vectors".to_string(),
                datasets: vec![ScientificReferenceDataset {
                    role: ScientificReferenceDatasetRole::Ephemeris,
                    identifier: "de441".to_string(),
                    version: "DE441".to_string(),
                    sha256: None,
                }],
            },
            coordinate_system: ScientificReferenceCoordinateSystem {
                frame: ScientificReferenceFrame::SsbIcrfJ2000,
                center: ScientificReferenceCenter::SolarSystemBarycenter,
                time_scale: ScientificReferenceTimeScale::Tdb,
                units: ScientificReferenceUnits::SiMetersSeconds,
            },
            julian_date: 2_451_545.0,
        }
    }

    fn body_state_case(id: &str) -> ScientificReferenceCase {
        ScientificReferenceCase::BodyState(BodyStateReferenceCase {
            header: header(id),
            target_naif_id: 399,
            expected: ReferenceStateVector {
                position_m: ReferenceVector3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
                velocity_mps: ReferenceVector3 {
                    x: 4.0,
                    y: 5.0,
                    z: 6.0,
                },
            },
            budget: StateResidualBudget {
                position_m: 1.0,
                velocity_mps: 1.0e-3,
            },
        })
    }

    struct MockStateAuthority {
        state: ReferenceStateVector,
    }

    impl ScientificStateAuthority for MockStateAuthority {
        fn state(
            &self,
            _: NaifBodyId,
            _: NaifBodyId,
            _: TdbEpoch,
        ) -> Result<ReferenceStateVector, String> {
            Ok(self.state)
        }
    }

    #[test]
    fn versioned_case_set_accepts_complete_typed_provenance() {
        let cases = ScientificReferenceCaseSet {
            format_version: SCIENTIFIC_REFERENCE_FORMAT_VERSION,
            cases: vec![body_state_case("earth-ssb-j2000")],
        };

        assert_eq!(cases.validate(), Ok(()));
        let encoded = ron::ser::to_string(&cases).expect("reference cases should serialize to RON");
        let decoded: ScientificReferenceCaseSet =
            ron::from_str(&encoded).expect("serialized reference cases should deserialize");
        assert_eq!(decoded, cases);
    }

    #[test]
    fn case_set_rejects_unknown_versions_duplicate_ids_and_incomplete_provenance() {
        let unknown_version = ScientificReferenceCaseSet {
            format_version: 2,
            cases: vec![],
        };
        assert_eq!(
            unknown_version.validate(),
            Err(ScientificReferenceCaseError::UnsupportedFormatVersion { actual: 2 })
        );

        let duplicate_ids = ScientificReferenceCaseSet {
            format_version: SCIENTIFIC_REFERENCE_FORMAT_VERSION,
            cases: vec![body_state_case("same"), body_state_case("same")],
        };
        assert_eq!(
            duplicate_ids.validate(),
            Err(ScientificReferenceCaseError::DuplicateCaseId)
        );

        let mut incomplete = body_state_case("incomplete");
        let ScientificReferenceCase::BodyState(incomplete_state) = &mut incomplete else {
            unreachable!("fixture is a body-state case");
        };
        incomplete_state.header.metadata.id = ScientificReferenceCaseId::new("");
        let incomplete = ScientificReferenceCaseSet {
            format_version: SCIENTIFIC_REFERENCE_FORMAT_VERSION,
            cases: vec![incomplete],
        };
        assert_eq!(
            incomplete.validate(),
            Err(ScientificReferenceCaseError::InvalidMetadata)
        );
    }

    #[test]
    fn recorded_cases_are_machine_readable_and_validated() {
        let source = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/configs/scientific_validation/reference_cases_v1.ron"
        ))
        .expect("recorded reference cases should exist");
        let cases: ScientificReferenceCaseSet =
            ron::from_str(&source).expect("recorded reference cases should deserialize");

        assert_eq!(cases.validate(), Ok(()));
        assert_eq!(cases.cases.len(), 6);
        assert!(matches!(
            cases.cases[0],
            ScientificReferenceCase::BodyState(_)
        ));
        assert!(matches!(
            cases.cases[1],
            ScientificReferenceCase::Orientation(_)
        ));
        assert!(matches!(
            cases.cases[2],
            ScientificReferenceCase::LaunchSite(_)
        ));
        assert!(matches!(
            cases.cases[3],
            ScientificReferenceCase::SunDirection(_)
        ));
        assert!(matches!(
            cases.cases[4],
            ScientificReferenceCase::Gravity(_)
        ));
        assert!(matches!(
            cases.cases[5],
            ScientificReferenceCase::Propagation(_)
        ));
    }

    #[test]
    fn runner_reports_body_state_passes_and_failures_in_physical_units() {
        let passing_case = body_state_case("passing");
        let ScientificReferenceCase::BodyState(expected_case) = &passing_case else {
            unreachable!("fixture is a body-state case");
        };
        let runner = ScientificValidationRunner::new(MockStateAuthority {
            state: expected_case.expected,
        });
        let passing_cases = ScientificReferenceCaseSet {
            format_version: SCIENTIFIC_REFERENCE_FORMAT_VERSION,
            cases: vec![passing_case],
        };
        let passing_report = runner.validate(&passing_cases).unwrap();
        assert_eq!(passing_report.passed(), 1);
        assert!(passing_report.is_verified());

        let mut failing_case = body_state_case("failing");
        let ScientificReferenceCase::BodyState(failing_state) = &mut failing_case else {
            unreachable!("fixture is a body-state case");
        };
        failing_state.expected.position_m.x += failing_state.budget.position_m * 2.0;
        let failing_cases = ScientificReferenceCaseSet {
            format_version: SCIENTIFIC_REFERENCE_FORMAT_VERSION,
            cases: vec![failing_case],
        };
        let failing_report = runner.validate(&failing_cases).unwrap();
        assert_eq!(failing_report.failed(), 1);
        assert!(!failing_report.is_verified());
    }
}
