//! Offline geographic crop to sparse Earth cube-sphere imagery tile converter.

use cosmic_systems_wasm::domain::services::cube_sphere::{CubeFace, TerrainPatch};
use cosmic_systems_wasm::domain::services::terrain_imagery::{
    convert_geographic_bounds_image, GeographicBounds,
};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.len() != 11 {
        return usage();
    }
    let Some(face) = parse_face(&arguments[2]) else {
        return usage();
    };
    let (
        Ok(level),
        Ok(tile_x),
        Ok(tile_y),
        Ok(west_longitude_deg),
        Ok(south_latitude_deg),
        Ok(east_longitude_deg),
        Ok(north_latitude_deg),
        Ok(tile_pixels),
    ) = (
        arguments[3].parse(),
        arguments[4].parse(),
        arguments[5].parse(),
        arguments[6].parse(),
        arguments[7].parse(),
        arguments[8].parse(),
        arguments[9].parse(),
        arguments[10].parse(),
    )
    else {
        return usage();
    };
    match convert_geographic_bounds_image(
        &arguments[0],
        PathBuf::from(&arguments[1]),
        TerrainPatch {
            face,
            level,
            tile_x,
            tile_y,
        },
        GeographicBounds {
            west_longitude_deg,
            south_latitude_deg,
            east_longitude_deg,
            north_latitude_deg,
        },
        tile_pixels,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Earth imagery crop conversion failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_face(value: &str) -> Option<CubeFace> {
    match value {
        "pos_x" => Some(CubeFace::PosX),
        "neg_x" => Some(CubeFace::NegX),
        "pos_y" => Some(CubeFace::PosY),
        "neg_y" => Some(CubeFace::NegY),
        "pos_z" => Some(CubeFace::PosZ),
        "neg_z" => Some(CubeFace::NegZ),
        _ => None,
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: cargo run --bin earth_imagery_crop_convert -- <source.png> <tile-root> <face> <level> <tile-x> <tile-y> <west-deg> <south-deg> <east-deg> <north-deg> <tile-pixels>"
    );
    ExitCode::FAILURE
}
