//! Deterministic, non-authoritative terrain overview raster generation.

use crate::domain::services::terrain_source::{surface_appearance, TerrainSource};

pub const TERRAIN_OVERVIEW_WIDTH: u32 = 192;
pub const TERRAIN_OVERVIEW_HEIGHT: u32 = 96;

/// Build the body-fixed presentation raster from the shared terrain source and
/// appearance law. Offline asset generation and tests share this exact path.
pub fn terrain_overview_raster(source: &dyn TerrainSource, width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let latitude_deg = 90.0 - (y as f64 + 0.5) * 180.0 / height as f64;
        for x in 0..width {
            let longitude_deg = -180.0 + (x as f64 + 0.5) * 360.0 / width as f64;
            let elevation_m = source.overview_height_m(latitude_deg, longitude_deg);
            let appearance = surface_appearance(
                elevation_m,
                source.overview_moisture(latitude_deg, longitude_deg),
                source.zone_lat(latitude_deg),
                source.overview_slope_deg(latitude_deg, longitude_deg),
            );
            pixels.extend(appearance.albedo.map(|channel| (channel * 255.0) as u8));
            pixels.push(255);
        }
    }
    pixels
}
