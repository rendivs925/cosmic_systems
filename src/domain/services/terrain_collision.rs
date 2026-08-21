//! Collision terrain for the rocket (AGENTS.md section 24).
//!
//! Collision is a second consumer of the shared [`TerrainSource`] (a coarser
//! LOD than the render mesh, refined near the rocket). It provides radar
//! altitude, surface normal, slope, and ground contact (landing/crash)
//! detection. There is no full-planet physics mesh: heights are sampled on
//! demand from the source.

use crate::domain::services::terrain_source::TerrainSource;
use bevy::math::DVec3;

/// A surface sample at a latitude/longitude.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceSample {
    pub height_m: f64,
    pub normal: DVec3,
    pub slope_deg: f64,
}

/// Sample the surface height and normal at lat/lon (degrees) on a planet of
/// mean radius `planet_radius_m`.
pub fn sample_surface(
    source: &dyn TerrainSource,
    latitude_deg: f64,
    longitude_deg: f64,
    planet_radius_m: f64,
) -> SurfaceSample {
    let height_m = source.height_m(latitude_deg, longitude_deg);
    let normal = surface_normal(source, latitude_deg, longitude_deg, planet_radius_m);
    let radial = radial_direction(latitude_deg, longitude_deg);
    let slope = normal.dot(radial).clamp(-1.0, 1.0).acos().to_degrees();
    SurfaceSample {
        height_m,
        normal,
        slope_deg: slope,
    }
}

/// The radial (up) unit vector at lat/lon (degrees).
pub fn radial_direction(latitude_deg: f64, longitude_deg: f64) -> DVec3 {
    let lat = latitude_deg.to_radians();
    let lon = longitude_deg.to_radians();
    DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin())
}

/// Surface normal from central differences of the height field, at lat/lon.
pub fn surface_normal(
    source: &dyn TerrainSource,
    latitude_deg: f64,
    longitude_deg: f64,
    planet_radius_m: f64,
) -> DVec3 {
    const STEP_DEG: f64 = 0.05;
    let r = planet_radius_m;

    let at = |la_deg: f64, lo_deg: f64| -> DVec3 {
        radial_direction(la_deg, lo_deg) * (r + source.height_m(la_deg, lo_deg))
    };
    let p = at(latitude_deg, longitude_deg);
    let p_lat = at(latitude_deg + STEP_DEG, longitude_deg);
    let p_lon = at(latitude_deg, longitude_deg + STEP_DEG);
    let step_m = STEP_DEG.to_radians() * r;

    let dlat = (p_lat - p) / step_m;
    let dlon = (p_lon - p) / step_m;
    let normal = dlat.cross(dlon).normalize_or_zero();
    if normal.length_squared() < 1e-12 {
        return radial_direction(latitude_deg, longitude_deg);
    }
    // Orient outward (away from the planet center).
    let radial = radial_direction(latitude_deg, longitude_deg);
    if normal.dot(radial) < 0.0 {
        -normal
    } else {
        normal
    }
}

/// Radar altitude above the terrain along the radial direction for a
/// planet-centered inertial position.
pub fn radar_altitude_m(
    source: &dyn TerrainSource,
    position_m: DVec3,
    planet_radius_m: f64,
) -> f64 {
    let r = position_m.length();
    if r < 1e-6 {
        return 0.0;
    }
    let dir = position_m / r;
    let (lat, lon) = lat_lon_from_direction(dir);
    let surface_height_m = source.height_m(lat, lon);
    let surface_radius = planet_radius_m + surface_height_m;
    (r - surface_radius).max(0.0)
}

/// Ground contact outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroundContact {
    #[default]
    None,
    Landed,
    Crash,
}

/// Detect ground contact from radar altitude and vertical speed.
/// - Low altitude + low speed → Landed.
/// - Low altitude + excessive speed → Crash.
pub fn detect_ground_contact(
    radar_altitude_m: f64,
    vertical_speed_mps: f64,
    contact_altitude_m: f64,
    touch_down_speed_mps: f64,
    crash_speed_mps: f64,
) -> GroundContact {
    if radar_altitude_m > contact_altitude_m {
        return GroundContact::None;
    }
    if vertical_speed_mps.abs() <= touch_down_speed_mps {
        GroundContact::Landed
    } else if vertical_speed_mps.abs() >= crash_speed_mps {
        GroundContact::Crash
    } else {
        // Between touch-down and crash speed: still descending on contact.
        GroundContact::Crash
    }
}

/// Direction → latitude/longitude in degrees.
pub fn lat_lon_from_direction(dir: DVec3) -> (f64, f64) {
    let d = dir.normalize();
    (d.y.asin().to_degrees(), d.z.atan2(d.x).to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::terrain_source::{
        crater_height, ProceduralTerrainSource, TerrainSource,
    };
    use bevy::math::DVec3;

    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    fn source() -> ProceduralTerrainSource {
        ProceduralTerrainSource::new(7, 1_000.0, 500.0, 0)
    }

    #[test]
    fn radar_altitude_is_radial_above_surface() {
        let s = source();
        // Position on the surface at KSC latitude/longitude + 5 km altitude.
        let (lat, lon) = (28.5721_f64, -80.6480_f64);
        let h = s.height_m(lat, lon);
        let r = EARTH_RADIUS_M + h + 5_000.0;
        let pos = radial_direction(lat, lon) * r;
        let altitude = radar_altitude_m(&s, pos, EARTH_RADIUS_M);
        assert!(
            (altitude - 5_000.0).abs() < 1.0,
            "radar altitude {altitude} should be ~5000 m, terrain h={h}"
        );
    }

    #[test]
    fn normal_points_away_and_slope_is_small_on_gentle_terrain() {
        let s = source();
        let sample = sample_surface(&s, 10.0, 20.0, EARTH_RADIUS_M);
        let radial = radial_direction(10.0, 20.0);
        assert!(sample.normal.dot(radial) > 0.0, "normal must point outward");
        assert!(sample.slope_deg < 90.0);
    }

    #[test]
    fn crater_rim_raises_above_bowl() {
        // A deep crater raises a lip near its outer edge.
        let bowl = crater_height(10.0, 10.0, 10.0, 10.0, 3.0, 2_000.0);
        let rim = crater_height(12.8, 10.0, 10.0, 10.0, 3.0, 2_000.0);
        assert!((bowl + 2_000.0).abs() < 1e-6, "bowl should be -depth");
        assert!(rim > bowl, "rim {rim} must sit above the bowl {bowl}");
        assert!(rim > 0.0, "outer rim should be elevated, got {rim}");
        // Outside the crater there is no contribution.
        assert_eq!(crater_height(40.0, 10.0, 10.0, 10.0, 3.0, 2_000.0), 0.0);
    }

    #[test]
    fn ground_contact_detection() {
        assert_eq!(
            detect_ground_contact(0.5, 1.0, 2.0, 2.0, 10.0),
            GroundContact::Landed
        );
        assert_eq!(
            detect_ground_contact(0.5, 50.0, 2.0, 2.0, 10.0),
            GroundContact::Crash
        );
        assert_eq!(
            detect_ground_contact(50.0, 50.0, 2.0, 2.0, 10.0),
            GroundContact::None
        );
    }

    #[test]
    fn collision_matches_render_source_height() {
        let s = source();
        let (lat, lon) = (33.0, -110.0);
        let render_height = s.height_m(lat, lon);
        let sample = sample_surface(&s, lat, lon, EARTH_RADIUS_M);
        // Collision samples the same TerrainSource → identical height.
        assert!((sample.height_m - render_height).abs() < 1e-9);
    }

    #[test]
    fn direction_to_lat_lon_round_trips() {
        let dir = DVec3::new(0.5, 0.3, 0.81).normalize();
        let (lat, lon) = lat_lon_from_direction(dir);
        let back = radial_direction(lat, lon);
        assert!((back - dir).length() < 1e-9);
    }
}
