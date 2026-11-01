//! Offline terrain-authority transect report. Requires `--features dem`.
//!
//! Prints every composition layer of the Earth terrain authority at a grid of
//! points: the measured ETOPO base, the eroded field, the composed surface at
//! two LODs, the local sea-level datum, and the resulting height above the
//! datum — the same quantity the rendered `height` diagnostic mode measures.
//!
//! It exists so a rendered frame can be checked against the data it came from
//! without launching the app: a visual defect in the terrain has to be either in
//! the authority or above it, and this is how that fork is decided. It is a
//! reporting tool, not a runtime dependency and not a terrain authority.
//!
//! Usage:
//!   terrain_authority_transect <lat0> <lat1> <lon0> <lon1> <step_deg>
//!
//! Output columns:
//!   lat lon base_m eroded_m composed_l5_m composed_l12_m sea_m above_l5_m above_l12_m

use cosmic_systems_wasm::domain::services::dem_terrain_source::DemTerrainSource;
use cosmic_systems_wasm::domain::services::global_raster::GlobalRasterSpec;
use cosmic_systems_wasm::domain::services::terrain_source::{
    earth_eroded_base, EarthTerrainSource, TerrainSource, DEFAULT_EARTH_DEM_PATH,
    DEFAULT_EARTH_GEOID_PATH,
};
use std::process::ExitCode;
use std::sync::Arc;

/// The renderer places its water surface this far above the sea-level datum.
/// Kept local because this tool must not become a second authority for it.
const WATER_SURFACE_OFFSET_M: f64 = 0.5;

/// LODs printed on either side of the procedural-detail fade band (`11..=14`),
/// so a reader can see whether a visible feature is detail or base terrain.
const COARSE_LOD: u32 = 5;
const FULL_DETAIL_LOD: u32 = 12;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() != 5 {
        return usage();
    }
    let parse = |value: &str| value.parse::<f64>();
    let (Ok(lat0), Ok(lat1), Ok(lon0), Ok(lon1), Ok(step)) = (
        parse(&arguments[0]),
        parse(&arguments[1]),
        parse(&arguments[2]),
        parse(&arguments[3]),
        parse(&arguments[4]),
    ) else {
        return usage();
    };
    let ordered = lat0 <= lat1 && lon0 <= lon1 && step > 0.0 && step.is_finite();
    if !ordered {
        return usage();
    }
    match run(lat0, lat1, lon0, lon1, step) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("terrain authority transect failed: {message}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: terrain_authority_transect <lat0> <lat1> <lon0> <lon1> <step_deg>");
    ExitCode::FAILURE
}

fn run(lat0: f64, lat1: f64, lon0: f64, lon1: f64, step: f64) -> Result<(), String> {
    let measured = DemTerrainSource::from_path_with_geoid(
        DEFAULT_EARTH_DEM_PATH,
        DEFAULT_EARTH_GEOID_PATH,
        GlobalRasterSpec::egm2008_geoid_0p5deg(),
    )
    .map_err(|error| format!("failed to open the measured Earth DEM: {error}"))?;
    let base: Arc<dyn TerrainSource> = Arc::new(measured);
    let eroded = earth_eroded_base(Arc::clone(&base));
    // The composed source is the one the renderer, collision and radar all use.
    let composed = EarthTerrainSource::new();

    println!(
        "# lat lon base_m eroded_m composed_l{COARSE_LOD}_m composed_l{FULL_DETAIL_LOD}_m \
         sea_m above_l{COARSE_LOD}_m above_l{FULL_DETAIL_LOD}_m"
    );
    let mut latitude = lat0;
    while latitude <= lat1 + 1e-9 {
        let mut longitude = lon0;
        while longitude <= lon1 + 1e-9 {
            let base_m = base.height_m(latitude, longitude);
            let eroded_m = eroded.height_m(latitude, longitude);
            let coarse_m = composed.mesh_height_m(latitude, longitude, COARSE_LOD);
            let detailed_m = composed.mesh_height_m(latitude, longitude, FULL_DETAIL_LOD);
            let sea_m = composed.sea_level_height_m(latitude, longitude);
            let above = |height_m: f64| height_m - sea_m - WATER_SURFACE_OFFSET_M;
            println!(
                "{latitude:.4} {longitude:.4} {base_m:.2} {eroded_m:.2} {coarse_m:.2} \
                 {detailed_m:.2} {sea_m:.2} {:.2} {:.2}",
                above(coarse_m),
                above(detailed_m)
            );
            longitude += step;
        }
        latitude += step;
    }
    Ok(())
}
