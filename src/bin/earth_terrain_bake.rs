//! Offline bake of Earth's measured elevation into immutable height and
//! surface-channel packages.

use cosmic_systems_wasm::domain::services::dem_terrain_source::{CubeSphereDem, CubeSphereSurface};
use cosmic_systems_wasm::domain::services::terrain_source::EarthTerrainSource;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let (Some(input), Some(height_output), Some(surface_output)) =
        (arguments.next(), arguments.next(), arguments.next())
    else {
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

    let source = match EarthTerrainSource::with_dem_path(&input) {
        Ok(source) => source,
        Err(error) => return fail(&format!("unable to load source terrain: {error}")),
    };
    let height = match CubeSphereDem::from_terrain_source(&source, resolution) {
        Ok(height) => height,
        Err(error) => return fail(&format!("unable to bake terrain heights: {error}")),
    };
    let surface = match CubeSphereSurface::from_terrain_source(&source, resolution) {
        Ok(surface) => surface,
        Err(error) => return fail(&format!("unable to bake terrain surface channels: {error}")),
    };
    if let Err(error) = height.write_path(PathBuf::from(&height_output)) {
        return fail(&format!("unable to write baked height package: {error}"));
    }
    if let Err(error) = surface.write_path(PathBuf::from(&surface_output)) {
        return fail(&format!("unable to write baked surface package: {error}"));
    }
    ExitCode::SUCCESS
}

fn fail(message: &str) -> ExitCode {
    eprintln!("earth terrain bake failed: {message}");
    ExitCode::FAILURE
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: cargo run --release --features dem --bin earth_terrain_bake -- <input.csdem> <output.csdem> <output.cssurf> [face-resolution]"
    );
    ExitCode::FAILURE
}
