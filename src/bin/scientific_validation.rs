use std::process::ExitCode;

use cosmic_systems_wasm::domain::services::scientific_validation::{
    ScientificReferenceCaseSet, ScientificValidationRunner, ScientificValidationStatus,
};

const REFERENCE_CASES_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/configs/scientific_validation/reference_cases_v1.ron"
);
const DEFAULT_EPHEMERIS_MANIFEST_PATH: &str = "assets/configs/ephemeris/de440.ron";

fn main() -> ExitCode {
    let manifest_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_EPHEMERIS_MANIFEST_PATH.to_string());
    let reference_cases = match load_reference_cases() {
        Ok(cases) => cases,
        Err(error) => return unverified(error),
    };
    let runner = match ScientificValidationRunner::load_ephemeris_manifest(&manifest_path) {
        Ok(runner) => runner,
        Err(error) => return unverified(error),
    };
    let report = match runner.validate(&reference_cases) {
        Ok(report) => report,
        Err(error) => return unverified(format!("reference-case contract is invalid: {error:?}")),
    };

    for case in &report.cases {
        match case.residual {
            Some(residual) => println!(
                "{:?} {} position={} m (budget {} m) velocity={} m/s (budget {} m/s): {}",
                case.status,
                case.case_id.as_str(),
                residual.position_m,
                residual.budget.position_m,
                residual.velocity_mps,
                residual.budget.velocity_mps,
                case.detail,
            ),
            None => println!(
                "{:?} {}: {}",
                case.status,
                case.case_id.as_str(),
                case.detail,
            ),
        }
    }
    println!(
        "Scientific validation: {} passed, {} failed, {} unverified",
        report.passed(),
        report.failed(),
        report.unverified(),
    );

    if report.failed() > 0 {
        return ExitCode::FAILURE;
    }
    if report
        .cases
        .iter()
        .any(|case| case.status == ScientificValidationStatus::Unverified)
    {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn load_reference_cases() -> Result<ScientificReferenceCaseSet, String> {
    let source = std::fs::read_to_string(REFERENCE_CASES_PATH)
        .map_err(|error| format!("cannot read {REFERENCE_CASES_PATH}: {error}"))?;
    ron::from_str(&source).map_err(|error| format!("cannot parse {REFERENCE_CASES_PATH}: {error}"))
}

fn unverified(error: impl AsRef<str>) -> ExitCode {
    eprintln!("Scientific validation UNVERIFIED: {}", error.as_ref());
    ExitCode::from(2)
}
