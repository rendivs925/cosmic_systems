//! Offline launch-site height delta report for two cube-sphere DEM packages.
//!
//! This tool exists to review a global relief upgrade before physics baselines
//! are regenerated. It samples the WGS 84 ellipsoid heights of both packages at
//! the priority sites and prints the signed delta. It is a reporting tool, not
//! a runtime dependency and not a terrain authority.
//!
//! Usage:
//!   terrain_site_delta <old.csdem> <new.csdem>

use cosmic_systems_wasm::domain::services::dem_terrain_source::DemTerrainSource;
use cosmic_systems_wasm::domain::services::terrain_source::TerrainSource;
use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

/// Report coordinates for the priority open-science regions. KSC and Papua
/// match the runtime predefined sites; the rest are approximate pad centers
/// used only to review a relief upgrade.
const SITES: &[(&str, f64, f64)] = &[
    ("Kennedy Space Center LC-39A", 28.6084, -80.6043),
    ("Starbase / Boca Chica", 25.9967, -97.1560),
    ("Vandenberg SLC-4E", 34.6321, -120.6108),
    ("Wallops Pad 0A", 37.8340, -75.4870),
    ("Kourou ELA-3", 5.2360, -52.7686),
    ("Baikonur Site 1/5", 45.9200, 63.3422),
    ("Mahia LC-1A", -39.2608, 177.8647),
    ("Papua coastal lowland", -8.0, 139.5),
];

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let Some(old_path) = arguments.next() else {
        return usage();
    };
    let Some(new_path) = arguments.next() else {
        return usage();
    };
    if arguments.next().is_some() {
        return usage();
    }
    match run(PathBuf::from(old_path), PathBuf::from(new_path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("terrain site delta failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(old_path: PathBuf, new_path: PathBuf) -> Result<(), Box<dyn Error>> {
    let old = DemTerrainSource::from_path(&old_path)
        .map_err(|error| format!("failed to open {}: {error}", old_path.display()))?;
    let new = DemTerrainSource::from_path(&new_path)
        .map_err(|error| format!("failed to open {}: {error}", new_path.display()))?;

    println!("old: {}", old_path.display());
    println!("new: {}", new_path.display());
    println!();
    println!("| site | lat deg | lon deg | old m | new m | delta m |");
    println!("| --- | ---: | ---: | ---: | ---: | ---: |");
    for (name, latitude_deg, longitude_deg) in SITES {
        let old_m = old.height_m(*latitude_deg, *longitude_deg);
        let new_m = new.height_m(*latitude_deg, *longitude_deg);
        println!(
            "| {name} | {latitude_deg:.4} | {longitude_deg:.4} | {old_m:.1} | {new_m:.1} | {:.1} |",
            new_m - old_m
        );
    }
    Ok(())
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: cargo run --features dem --bin terrain_site_delta -- <old.csdem> <new.csdem>"
    );
    ExitCode::FAILURE
}
