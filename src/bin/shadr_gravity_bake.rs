//! Offline SHADR gravity converter for Mars GMM-3 and lunar GRGM660PRIM.
//! Requires `--features native`.
//!
//! Reads the NASA PDS comma-separated SHADR table (header row plus
//! `degree, order, C, S, sigmaC, sigmaS` rows) and writes the same versioned
//! `.csgravc` package used by the Earth harmonics tier. Both packages are data
//! readiness for future planetary flight; no current force model selects them,
//! and the simulator never reads the source table at runtime.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use cosmic_systems_wasm::domain::services::gravity_harmonics::{
    coefficient_pair_count, GravityHarmonicsMetadata, SphericalHarmonicGravityModel,
};
use sha2::{Digest, Sha256};

struct Preset {
    key: &'static str,
    model_id: &'static str,
    body: &'static str,
    source_url: &'static str,
    source_description: &'static str,
    attribution: &'static str,
    default_degree: u32,
}

const PRESETS: [Preset; 2] = [
    Preset {
        key: "gmm3",
        model_id: "GMM-3",
        body: "Mars",
        source_url: "https://pds-geosciences.wustl.edu/mro/mro-m-rss-5-sdp-v1/mrors_1xxx/data/shadr/gmm3_120_sha.tab",
        source_description: "NASA PDS MRO-M-RSS-5-SDP-V1.0 SHADR static field",
        attribution: "Genova et al. (2016), Icarus 272, 228-235; NASA/GSFC. Frame: Konopliv et al. (2006), Icarus 182, 23-50; not IAU.",
        default_degree: 120,
    },
    Preset {
        key: "grgm660",
        model_id: "GRGM660PRIM",
        body: "Moon",
        source_url: "https://pds-geosciences.wustl.edu/grail/grail-l-lgrs-5-rdr-v1/grail_1001/shadr/gggrx_0660pm_sha.tab",
        source_description: "NASA PDS GRAIL SHADR GRGM660PRIM static field",
        attribution: "Lemoine et al. (2013), High-degree gravity models from GRAIL primary mission data, JGR Planets 118, 1676-1698, doi:10.1002/jgre.20118; NASA/GSFC GRAIL. Frame: DE421 principal axes; not IAU/pck.",
        default_degree: 660,
    },
];

fn preset(key: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.key == key)
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(input) = arguments.next() else {
        return usage();
    };
    let Some(output) = arguments.next() else {
        return usage();
    };
    let Some(preset_key) = arguments.next() else {
        return usage();
    };
    let Some(preset) = preset(&preset_key) else {
        return usage();
    };
    let degree = match arguments.next() {
        Some(value) => match value.parse::<u32>() {
            Ok(value) => value,
            Err(_) => return usage(),
        },
        None => preset.default_degree,
    };
    if arguments.next().is_some() {
        return usage();
    }

    match bake(PathBuf::from(input), PathBuf::from(output), preset, degree) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("SHADR conversion failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn fields(line: &str) -> Vec<&str> {
    line.split(',').map(str::trim).collect()
}

fn bake(input: PathBuf, output: PathBuf, preset: &Preset, degree: u32) -> Result<(), String> {
    let text = fs::read_to_string(&input).map_err(|error| error.to_string())?;
    let source_sha256 = format!("{:x}", Sha256::digest(text.as_bytes()));
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let header = lines
        .next()
        .ok_or_else(|| "source is empty".to_string())
        .and_then(|line| {
            let values = fields(line);
            if values.len() < 6 {
                return Err("SHADR header has fewer than six columns".into());
            }
            let radius_km = values[0].parse::<f64>().map_err(|_| "invalid radius")?;
            let gm_km3_s2 = values[1].parse::<f64>().map_err(|_| "invalid GM")?;
            let degree = values[3].parse::<u32>().map_err(|_| "invalid degree")?;
            let order = values[4].parse::<u32>().map_err(|_| "invalid order")?;
            let normalization = values[5]
                .parse::<u32>()
                .map_err(|_| "invalid normalization")?;
            if normalization != 1 {
                return Err("source is not normalized".into());
            }
            if degree != order {
                return Err("source is not a square degree/order field".into());
            }
            Ok((radius_km, gm_km3_s2, degree))
        })?;
    let (radius_km, gm_km3_s2, available_degree) = header;
    if available_degree < degree {
        return Err(format!(
            "source reaches degree {available_degree}, cannot bake degree {degree}"
        ));
    }

    let mut coefficients = Vec::with_capacity(coefficient_pair_count(degree) * 2);
    let mut expected_degree = 1_u32;
    let mut expected_order = 0_u32;
    for (line_number, line) in lines.enumerate() {
        let values = fields(line);
        if values.len() < 4 {
            return Err(format!(
                "row {} has fewer than four columns",
                line_number + 3
            ));
        }
        let n = values[0].parse::<u32>().map_err(|_| "invalid degree")?;
        let m = values[1].parse::<u32>().map_err(|_| "invalid order")?;
        if n > degree {
            continue;
        }
        if (n, m) != (expected_degree, expected_order) {
            return Err(format!(
                "coefficient order breaks at degree {n} order {m}; expected {expected_degree} {expected_order}"
            ));
        }
        let c = values[2]
            .parse::<f64>()
            .map_err(|_| "invalid C coefficient")?;
        let s = values[3]
            .parse::<f64>()
            .map_err(|_| "invalid S coefficient")?;
        if n >= 2 {
            if !c.is_finite() || !s.is_finite() {
                return Err("coefficient is not finite".into());
            }
            coefficients.extend_from_slice(&[c, s]);
        }
        if expected_order == expected_degree {
            expected_degree += 1;
            expected_order = 0;
        } else {
            expected_order += 1;
        }
    }
    if coefficients.len() != coefficient_pair_count(degree) * 2 {
        return Err("source is missing coefficients below the requested degree".into());
    }

    let c20 = coefficients[0];
    let j2 = -(5.0_f64).sqrt() * c20;
    let metadata = GravityHarmonicsMetadata {
        model_id: preset.model_id.into(),
        body: preset.body.into(),
        source: format!("{} degree {degree}", preset.source_description),
        source_url: preset.source_url.into(),
        source_sha256,
        normalization: "fully-normalized-4pi".into(),
        tide_system: "not specified by the PDS label; static field as published".into(),
        license: "Public domain (NASA)".into(),
        attribution: preset.attribution.into(),
        conversion_version: 1,
    };
    let model = SphericalHarmonicGravityModel::from_parts(
        degree,
        radius_km * 1_000.0,
        metadata,
        coefficients,
    )
    .map_err(|error| format!("{error:?}"))?;
    model
        .write_path(&output)
        .map_err(|error| format!("{error:?}"))?;
    let bytes = fs::read(&output).map_err(|error| error.to_string())?;
    println!(
        "wrote {} ({} bytes, {} {} degree {}, {} coefficient pairs)",
        output.display(),
        bytes.len(),
        preset.body,
        preset.model_id,
        degree,
        coefficient_pair_count(degree)
    );
    println!(
        "source radius {radius_km} km, GM {gm_km3_s2:e} km3/s2 (runtime would use the authoritative DE440 mu); reference radius {} m",
        radius_km * 1_000.0
    );
    println!("degree-two C20 {c20:.15e} implies J2 {j2:.15e}");
    println!("package sha256 {:x}", Sha256::digest(&bytes));
    Ok(())
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: cargo run --features native --bin shadr_gravity_bake -- <input.tab> <output.csgravc> <gmm3|grgm660> [max-degree]"
    );
    ExitCode::FAILURE
}
