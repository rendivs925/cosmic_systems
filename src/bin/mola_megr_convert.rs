//! Offline PDS MOLA MEGR 32 converter. Requires `--features dem`.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use cosmic_systems_wasm::domain::services::dem_terrain_source::convert_mola_megr_32_raw;
use cosmic_systems_wasm::domain::services::planet_factory::PlanetFactory;
use cosmic_systems_wasm::domain::services::reference_frames::planet_radius_m;

fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let Some(input) = arguments.next() else {
        return usage();
    };
    let Some(output) = arguments.next() else {
        return usage();
    };
    let resolution = match arguments.next() {
        Some(value) => match value.to_string_lossy().parse::<u32>() {
            Ok(value) => value,
            Err(_) => return usage(),
        },
        None => 2_048,
    };
    if arguments.next().is_some() {
        return usage();
    }

    let mars = PlanetFactory::create_by_name("Mars").expect("Mars catalog entry is required");
    match convert_mola_megr_32_raw(
        PathBuf::from(input),
        PathBuf::from(output),
        resolution,
        planet_radius_m(&mars),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("MOLA MEGR 32 conversion failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: cargo run --features dem --bin mola_megr_convert -- <input.img> <output.csdem> [face-resolution]"
    );
    ExitCode::FAILURE
}
