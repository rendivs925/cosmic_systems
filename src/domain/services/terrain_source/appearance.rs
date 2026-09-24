//! Continuous surface appearance law (albedo/roughness) shared by the
//! renderer and presentation systems.

use super::{ss, TerrainSource};

/// Continuous surface appearance (albedo/roughness/metallic) blended from
/// elevation, soil moisture, latitude zone and local slope — the "one
/// continuous law" terrain best-practice (glassy wash → soft hills → textured
/// slopes → carved rock), replacing hard biome bands with soft ecotones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceAppearance {
    pub albedo: [f32; 3],
    /// Perceptual roughness `[0, 1]` (higher = rougher/lambertian).
    pub roughness: f32,
    pub metallic: f32,
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f64) -> [f32; 3] {
    let t = t as f32;
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn lerp_f(a: f32, b: f32, t: f64) -> f32 {
    a + (b - a) * t as f32
}

/// Compute a continuous, deterministic surface appearance at a terrain point.
/// `elevation_m` is height above mean radius, `moisture`/`zone_lat` in `[0,1]`,
/// `slope_deg` the local gradient (≥ 0). Pure function — unit-testable and
/// shared by the render and any future biome-gated systems.
pub fn surface_appearance(
    elevation_m: f64,
    moisture: f64,
    zone_lat: f64,
    slope_deg: f64,
) -> SurfaceAppearance {
    // Linear broadband reflectance ranges keep sunlit terrain grounded rather
    // than turning vegetation and rock into emissive-looking pastel colors.
    const SAND: [f32; 3] = [0.36, 0.30, 0.16];
    const GRASS: [f32; 3] = [0.10, 0.22, 0.04];
    const FOREST: [f32; 3] = [0.025, 0.10, 0.015];
    const SAVANNA: [f32; 3] = [0.24, 0.20, 0.055];
    const DESERT: [f32; 3] = [0.42, 0.31, 0.16];
    const TUNDRA: [f32; 3] = [0.18, 0.18, 0.14];
    const POLAR: [f32; 3] = [0.75, 0.80, 0.84];
    const ROCK: [f32; 3] = [0.20, 0.18, 0.15];
    const SNOW: [f32; 3] = [0.78, 0.82, 0.86];
    const SEAFLOOR: [f32; 3] = [0.015, 0.04, 0.08];

    // Seafloor below sea level.
    if elevation_m < 0.0 {
        let depth = (-elevation_m).min(4000.0) / 4000.0;
        let albedo = lerp3([0.035, 0.11, 0.17], SEAFLOOR, depth);
        return SurfaceAppearance {
            albedo,
            roughness: lerp_f(0.25, 0.7, depth),
            metallic: 0.0,
        };
    }

    // Shoreline → sand.
    let sand_t = 1.0 - ss(0.0, 4.0, elevation_m.min(4.0));
    let mut albedo = lerp3(SAND, GRASS, 1.0 - sand_t);

    // Moisture drives grass → forest (wet) / savanna → desert (dry).
    if moisture < 0.4 {
        let dry_t = ss(0.4, 0.15, moisture); // dries below 0.4
        albedo = lerp3(albedo, SAVANNA, dry_t * 0.6);
        albedo = lerp3(albedo, DESERT, (dry_t * 0.5) * ss(120.0, 0.0, elevation_m));
    } else {
        let wet_t = ss(0.4, 0.75, moisture);
        albedo = lerp3(albedo, FOREST, wet_t * 0.7);
    }

    // Latitude zone: cold toward the poles.
    let polar_dist = (zone_lat - 0.5).abs() * 2.0; // 0 equator → 1 pole
    let cold_t = ss(0.55, 0.9, polar_dist);
    albedo = lerp3(albedo, TUNDRA, cold_t * 0.6);
    albedo = lerp3(albedo, POLAR, cold_t * ss(0.8, 1.0, polar_dist));

    // Steep → bare rock, cliff edges keep their sharp character.
    let rock_t = ss(35.0, 55.0, slope_deg);
    albedo = lerp3(albedo, ROCK, rock_t);

    // Damp foreshore: the first couple of metres above the waterline read as
    // darker, glossier wet ground rather than dry sand.
    let wet_shore = 1.0 - ss(0.0, 2.5, elevation_m.min(2.5));
    let wet_target = [albedo[0] * 0.5, albedo[1] * 0.5, albedo[2] * 0.5];
    albedo = lerp3(albedo, wet_target, wet_shore * 0.5);

    // Snow line: high altitude above the snow band turns white. The band
    // descends from the equator toward the poles, so polar terrain is snow
    // covered at much lower altitude than tropical terrain.
    let snow_start = 4500.0 * (1.0 - polar_dist).powf(1.6);
    let snow_t = ss(snow_start, snow_start + 700.0, elevation_m);
    albedo = lerp3(albedo, SNOW, snow_t);
    let roughness = (0.85 + 0.08 * rock_t - 0.35 * snow_t - 0.15 * wet_shore).clamp(0.0, 1.0);

    SurfaceAppearance {
        albedo,
        roughness: roughness as f32,
        metallic: 0.0,
    }
}

/// Blend a ground appearance with a source-derived river channel. The strength
/// comes from the erosion source, rather than an independent render-time water
/// field, so terrain color agrees with its carved drainage network.
pub fn with_river_appearance(
    mut appearance: SurfaceAppearance,
    river_strength: f64,
) -> SurfaceAppearance {
    let strength = river_strength.clamp(0.0, 1.0);
    let wet_bank = strength.sqrt() * 0.32;
    appearance.albedo = lerp3(appearance.albedo, [0.035, 0.09, 0.025], wet_bank);

    let channel = ss(0.12, 0.65, strength);
    appearance.albedo = lerp3(appearance.albedo, [0.006, 0.025, 0.055], channel);
    appearance.roughness = lerp_f(appearance.roughness, 0.18, channel);
    appearance
}

/// Local terrain slope (degrees ≥ 0) at a lat/lon by central differences over
/// a small arc. Uses the authoritative source; deterministic for a fixed source.
pub fn slope_deg_at(source: &dyn TerrainSource, latitude_deg: f64, longitude_deg: f64) -> f64 {
    let d = 0.02; // ~2 km probe for a stable, feature-scale gradient
    let hx = source.height_m(latitude_deg, longitude_deg + d)
        - source.height_m(latitude_deg, longitude_deg - d);
    let hy = source.height_m(latitude_deg + d, longitude_deg)
        - source.height_m(latitude_deg - d, longitude_deg);
    let lat_m = 111_320.0; // meters per degree of latitude
    let lon_m = (111_320.0 * latitude_deg.to_radians().cos()).abs().max(1.0);
    let grad = (hx / (2.0 * d * lon_m)).hypot(hy / (2.0 * d * lat_m));
    grad.atan().to_degrees()
}
