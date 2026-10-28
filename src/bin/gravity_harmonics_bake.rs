//! Offline EGM2008 coefficient converter. Requires `--features native`.
//!
//! Reads the NGA tide-free, fully normalized EGM2008 distribution (the
//! headerless `n m C S sigmaC sigmaS` variant with `D` exponents) and writes a
//! versioned `.csgravc` package truncated to a requested degree. The simulator
//! never reads the source text at runtime.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use cosmic_systems_wasm::domain::services::gravity_harmonics::{
    coefficient_pair_count, GravityHarmonicsMetadata, SphericalHarmonicGravityModel,
};
use sha2::{Digest, Sha256};

const SOURCE_URL: &str =
    "https://raw.githubusercontent.com/zcyphygeodesy/EGMallelementcalc/main/EGM2008.gfc";
const DEFAULT_DEGREE: u32 = 360;

fn parse_fortran(value: &str) -> Option<f64> {
    value
        .replace(['D', 'd'], "E")
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(input) = arguments.next() else {
        return usage();
    };
    let Some(output) = arguments.next() else {
        return usage();
    };
    let degree = match arguments.next() {
        Some(value) => match value.parse::<u32>() {
            Ok(value) => value,
            Err(_) => return usage(),
        },
        None => DEFAULT_DEGREE,
    };
    if arguments.next().is_some() {
        return usage();
    }

    match bake(PathBuf::from(input), PathBuf::from(output), degree) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("EGM2008 conversion failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn bake(input: PathBuf, output: PathBuf, degree: u32) -> Result<(), String> {
    let text = fs::read_to_string(&input).map_err(|error| error.to_string())?;
    let source_sha256 = format!("{:x}", Sha256::digest(text.as_bytes()));
    let mut header = None;
    let mut rows = Vec::new();
    for (line_number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('%') {
            continue;
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let numeric_degree = fields.first().and_then(|value| value.parse::<u32>().ok());
        if let (Some(n), Some(m)) = (
            numeric_degree,
            fields.get(1).and_then(|value| value.parse::<u32>().ok()),
        ) {
            if fields.len() < 4 || n < 2 || m > n {
                return Err(format!(
                    "line {} is not a valid coefficient row",
                    line_number + 1
                ));
            }
            let c = parse_fortran(fields[2])
                .ok_or_else(|| format!("line {} has an invalid C coefficient", line_number + 1))?;
            let s = parse_fortran(fields[3])
                .ok_or_else(|| format!("line {} has an invalid S coefficient", line_number + 1))?;
            rows.push((n, m, c, s));
            continue;
        }
        if header.is_some() || fields.len() < 3 {
            return Err(format!(
                "line {} is not a recognized header or coefficient",
                line_number + 1
            ));
        }
        let gm_m3_s2 =
            parse_fortran(fields[0]).ok_or_else(|| "header GM is not numeric".to_string())?;
        let reference_radius_m = parse_fortran(fields[1])
            .ok_or_else(|| "header reference radius is not numeric".to_string())?;
        header = Some((gm_m3_s2, reference_radius_m, fields[2..].join(" ")));
    }
    let Some((gm_m3_s2, reference_radius_m, tide_system)) = header else {
        return Err("source has no GM/radius/tide header".into());
    };
    let available_degree = rows.iter().map(|(n, _, _, _)| *n).max().unwrap_or(0);
    if available_degree < degree {
        return Err(format!(
            "source reaches degree {available_degree}, cannot bake degree {degree}"
        ));
    }

    let mut coefficients = Vec::with_capacity(coefficient_pair_count(degree) * 2);
    let mut expected = 2_u32;
    let mut expected_m = 0_u32;
    for (n, m, c, s) in rows {
        if n > degree {
            continue;
        }
        if (n, m) != (expected, expected_m) {
            return Err(format!(
                "coefficient order breaks at degree {n} order {m}; expected {expected} {expected_m}"
            ));
        }
        coefficients.extend_from_slice(&[c, s]);
        if expected_m == expected {
            expected += 1;
            expected_m = 0;
        } else {
            expected_m += 1;
        }
    }
    if coefficients.len() != coefficient_pair_count(degree) * 2 {
        return Err("source is missing coefficients below the requested degree".into());
    }
    let c20 = coefficients[0];
    let j2 = -(5.0_f64).sqrt() * c20;
    let metadata = GravityHarmonicsMetadata {
        model_id: "EGM2008".into(),
        body: "Earth".into(),
        source: format!(
            "NGA EGM2008 tide-free fully normalized coefficients, degree {degree}"
        ),
        source_url: SOURCE_URL.into(),
        source_sha256,
        normalization: "fully-normalized-4pi".into(),
        tide_system: tide_system.replace('_', "-"),
        license: "Public domain (NGA/DoD)".into(),
        attribution: "Pavlis, Holmes, Kenyon & Factor (2012), The development and evaluation of the Earth Gravitational Model 2008 (EGM2008), JGR 117, B04406; National Geospatial-Intelligence Agency.".into(),
        conversion_version: 1,
    };
    let model = SphericalHarmonicGravityModel::from_parts(
        degree,
        reference_radius_m,
        metadata,
        coefficients,
    )
    .map_err(|error| format!("{error:?}"))?;
    model
        .write_path(&output)
        .map_err(|error| format!("{error:?}"))?;
    let bytes = fs::read(&output).map_err(|error| error.to_string())?;
    println!(
        "wrote {} ({} bytes, degree {}, {} coefficient pairs)",
        output.display(),
        bytes.len(),
        degree,
        coefficient_pair_count(degree)
    );
    // EGM2008 distributions express GM in 1e14 m^3/s^2 while the radius is in
    // meters. The model itself never uses this value: the runtime keeps the
    // authoritative DE440 mu.
    let gm_m3_s2 = if gm_m3_s2 < 1.0e6 {
        gm_m3_s2 * 1.0e14
    } else {
        gm_m3_s2
    };
    println!(
        "source GM {gm_m3_s2:.6e} m3/s2 (runtime uses the authoritative DE440 mu); reference radius {reference_radius_m} m"
    );
    println!("degree-two C20 {c20:.15e} implies J2 {j2:.15e}");
    println!("package sha256 {:x}", Sha256::digest(&bytes));
    Ok(())
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: cargo run --features native --bin gravity_harmonics_bake -- <egm2008.gfc> <output.csgravc> [max-degree]"
    );
    ExitCode::FAILURE
}
