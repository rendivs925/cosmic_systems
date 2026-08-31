//! Collision terrain for the rocket (AGENTS.md section 24).
//!
//! Collision is a second consumer of the shared [`TerrainSource`] (a coarser
//! LOD than the render mesh, refined near the rocket). It provides radar
//! altitude, surface normal, slope, and ground contact (landing/crash)
//! detection. There is no full-planet physics mesh: heights are sampled on
//! demand from the source.

use crate::domain::services::reference_frames::body_fixed_to_terrain_lat_lon;
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
    // ~5.5 m at Earth's equator: this resolves the pad-scale clearance
    // footprint and the shared source's local relief instead of averaging a
    // launch pad with terrain kilometres away.
    const STEP_DEG: f64 = 0.00005;
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
/// planet-centered body-fixed position. TerrainSource coordinates are fixed to
/// the rotating body, so callers must convert inertial simulation state first.
pub fn radar_altitude_m(
    source: &dyn TerrainSource,
    position_body_fixed_m: DVec3,
    planet_radius_m: f64,
) -> f64 {
    let r = position_body_fixed_m.length();
    if r < 1e-6 {
        return 0.0;
    }
    let dir = position_body_fixed_m / r;
    let (lat, lon) = body_fixed_to_terrain_lat_lon(dir);
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

// ---------------------------------------------------------------------------
// Multi-criteria touchdown evaluation
// ---------------------------------------------------------------------------

/// Radar-altitude band within which a touchdown verdict is evaluated.
pub const TOUCHDOWN_BAND_M: f64 = 3.0;

/// Exponential tangential damping rate while resting (1/s). Applied as
/// `v_t *= exp(-rate*dt)`, so it is deterministic and frame-rate independent.
pub const REST_TANGENTIAL_DAMPING_PER_S: f64 = 12.0;

/// Acceptance limits for a touchdown. All four must pass for a landing; any
/// violated limit is a crash.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchdownCriteria {
    /// Maximum into-ground normal speed at contact (m/s).
    pub max_vertical_speed_mps: f64,
    /// Maximum speed along the tangent plane at contact (m/s).
    pub max_lateral_speed_mps: f64,
    /// Maximum local terrain slope under the vehicle (degrees).
    pub max_slope_deg: f64,
    /// Maximum tilt of the vehicle's longitudinal axis from the surface
    /// normal (degrees).
    pub max_tilt_deg: f64,
}

impl Default for TouchdownCriteria {
    fn default() -> Self {
        Self {
            // Preserves the historical vertical-speed boundary (5 m/s).
            max_vertical_speed_mps: 5.0,
            // No gear is modeled: nothing absorbs lateral drift.
            max_lateral_speed_mps: 3.0,
            max_slope_deg: 10.0,
            max_tilt_deg: 15.0,
        }
    }
}

/// Surface-relative velocity components against the local ground plane.
///
/// The dynamics integrate in a planet-centered non-rotating frame and the
/// atmosphere model carries no wind, so inertial velocity *is*
/// surface-relative; vertical/lateral mean decomposition against the local
/// surface normal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VelocityComponents {
    /// Component along the surface normal (+ = moving away from the ground).
    pub normal_mps: f64,
    /// Magnitude of the component within the tangent plane.
    pub lateral_mps: f64,
}

/// Decompose a velocity into surface-normal and tangent-plane parts.
pub fn decompose_velocity(velocity_mps: DVec3, surface_normal: DVec3) -> VelocityComponents {
    let n = surface_normal.normalize_or_zero();
    let normal_mps = velocity_mps.dot(n);
    let lateral = velocity_mps - n * normal_mps;
    VelocityComponents {
        normal_mps,
        lateral_mps: lateral.length(),
    }
}

/// Evaluate a touchdown against [`TouchdownCriteria`]. `descent_speed_mps`
/// is the into-ground normal speed (>= 0).
pub fn evaluate_touchdown(
    descent_speed_mps: f64,
    lateral_speed_mps: f64,
    slope_deg: f64,
    tilt_deg: f64,
    criteria: &TouchdownCriteria,
) -> GroundContact {
    if descent_speed_mps > criteria.max_vertical_speed_mps
        || lateral_speed_mps > criteria.max_lateral_speed_mps
        || slope_deg > criteria.max_slope_deg
        || tilt_deg > criteria.max_tilt_deg
    {
        return GroundContact::Crash;
    }
    GroundContact::Landed
}

// ---------------------------------------------------------------------------
// Resting-contact resolution
// ---------------------------------------------------------------------------

/// State after enforcing the resting-contact constraint for one step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactResolution {
    pub position_m: DVec3,
    pub velocity_mps: DVec3,
}

/// Enforce rest on terrain for one fixed step:
///
/// - penetration is prevented by clamping the radial position out to the
///   sampled surface (`surface_radius_m` = mean planet radius + terrain
///   height at the sub-vehicle point),
/// - normal velocity is removed entirely — the ground absorbs motion into
///   *and* numerical drift away from the surface; leaving rest happens only
///   through [`liftoff_from_rest`],
/// - tangential velocity decays exponentially (simple damping, deliberately
///   not a Coulomb friction model — AGENTS.md section 10 scope).
///
/// `surface_normal` must point away from the ground.
pub fn resolve_resting_contact(
    position_m: DVec3,
    velocity_mps: DVec3,
    surface_radius_m: f64,
    surface_normal: DVec3,
    dt_s: f64,
) -> ContactResolution {
    let radial_dir = position_m.normalize_or_zero();
    let position_m = if position_m.length() < surface_radius_m {
        radial_dir * surface_radius_m
    } else {
        position_m
    };
    let n = surface_normal.normalize_or_zero();
    let normal_mps = velocity_mps.dot(n);
    let mut tangential = velocity_mps - n * normal_mps;
    tangential *= (-REST_TANGENTIAL_DAMPING_PER_S * dt_s).exp();
    ContactResolution {
        position_m,
        velocity_mps: tangential,
    }
}

/// A grounded vehicle breaks rest only when available thrust exceeds its
/// weight (strict TWR > 1); anything less cannot overcome the contact.
pub fn liftoff_from_rest(thrust_n: f64, weight_n: f64) -> bool {
    thrust_n > weight_n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::terrain_source::{ProceduralTerrainSource, TerrainSource};
    use bevy::math::DVec3;

    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    fn source() -> ProceduralTerrainSource {
        ProceduralTerrainSource::new(7, 1_000.0, 500.0, 0)
    }

    #[test]
    fn radar_altitude_is_radial_above_surface() {
        let s = source();
        // Position on the surface at representative radial terrain coordinates
        // plus 5 km altitude.
        let (lat, lon) = (28.4_f64, -80.65_f64);
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
        let bowl = ProceduralTerrainSource::crater_height(10.0, 10.0, 10.0, 10.0, 3.0, 2_000.0);
        let rim = ProceduralTerrainSource::crater_height(12.8, 10.0, 10.0, 10.0, 3.0, 2_000.0);
        assert!((bowl + 2_000.0).abs() < 1e-6, "bowl should be -depth");
        assert!(rim > bowl, "rim {rim} must sit above the bowl {bowl}");
        assert!(rim > 0.0, "outer rim should be elevated, got {rim}");
        // Outside the crater there is no contribution.
        assert_eq!(
            ProceduralTerrainSource::crater_height(40.0, 10.0, 10.0, 10.0, 3.0, 2_000.0),
            0.0
        );
    }

    #[test]
    fn valid_vertical_touchdown_is_a_landing() {
        let criteria = TouchdownCriteria::default();
        assert_eq!(
            evaluate_touchdown(2.0, 1.0, 4.0, 5.0, &criteria),
            GroundContact::Landed
        );
    }

    #[test]
    fn excessive_lateral_speed_crashes() {
        let criteria = TouchdownCriteria::default();
        assert_eq!(
            evaluate_touchdown(0.5, 30.0, 2.0, 3.0, &criteria),
            GroundContact::Crash
        );
    }

    #[test]
    fn excessive_descent_speed_crashes() {
        let criteria = TouchdownCriteria::default();
        // Preserves the old hard-impact boundary.
        assert_eq!(
            evaluate_touchdown(50.0, 0.5, 2.0, 3.0, &criteria),
            GroundContact::Crash
        );
        // The old 5–15 m/s "between" band was already a crash.
        assert_eq!(
            evaluate_touchdown(10.0, 0.5, 2.0, 3.0, &criteria),
            GroundContact::Crash
        );
    }

    #[test]
    fn excessive_slope_crashes() {
        let criteria = TouchdownCriteria::default();
        assert_eq!(
            evaluate_touchdown(1.0, 1.0, 45.0, 3.0, &criteria),
            GroundContact::Crash
        );
    }

    #[test]
    fn excessive_tilt_crashes() {
        let criteria = TouchdownCriteria::default();
        assert_eq!(
            evaluate_touchdown(1.0, 1.0, 3.0, 60.0, &criteria),
            GroundContact::Crash
        );
    }

    #[test]
    fn valid_slope_landing_within_limits() {
        let criteria = TouchdownCriteria::default();
        // Slope and tilt inside limits, speeds gentle: a legal hillside landing.
        assert_eq!(
            evaluate_touchdown(2.0, 2.0, 8.0, 12.0, &criteria),
            GroundContact::Landed
        );
    }

    #[test]
    fn resting_contact_removes_normal_velocity_and_damps_slide() {
        let dt = 1.0 / 64.0;
        let surface_radius_m = 100.0;
        let normal = DVec3::Y;
        // Slightly above surface, sinking at 1 m/s, sliding sideways at 2 m/s.
        let position = DVec3::new(0.0, surface_radius_m + 0.05, 0.0);
        let velocity = DVec3::new(2.0, -1.0, 0.0);

        let res = resolve_resting_contact(position, velocity, surface_radius_m, normal, dt);

        // Normal motion fully absorbed; tangential decayed, not zeroed.
        assert!(res.velocity_mps.dot(normal).abs() < 1e-12);
        let expected_tangential = 2.0 * (-REST_TANGENTIAL_DAMPING_PER_S * dt).exp();
        assert!((res.velocity_mps.x - expected_tangential).abs() < 1e-9);
        assert_eq!(res.velocity_mps.z, 0.0);
        // No penetration clamp needed when above the surface.
        assert!((res.position_m.y - position.y).abs() < 1e-12);
    }

    #[test]
    fn liftoff_requires_thrust_above_weight() {
        assert!(liftoff_from_rest(120_000.0, 100_000.0));
        assert!(!liftoff_from_rest(99_000.0, 100_000.0));
        // Exactly balanced cannot break the contact (strict inequality).
        assert!(!liftoff_from_rest(100_000.0, 100_000.0));
    }

    #[test]
    fn penetration_is_clamped_to_surface() {
        let surface_radius_m = 100.0;
        let normal = DVec3::Y;
        // Deep under the surface along a tilted radial direction.
        let position = DVec3::new(30.0, 30.0, 40.0);
        let velocity = DVec3::ZERO;

        let res = resolve_resting_contact(position, velocity, surface_radius_m, normal, 1.0 / 64.0);

        let expected = position.normalize_or_zero() * surface_radius_m;
        assert!((res.position_m - expected).length() < 1e-9);
        assert!((res.position_m.length() - surface_radius_m).abs() < 1e-9);
        assert!(res.velocity_mps == DVec3::ZERO);
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
        let (lat, lon) = body_fixed_to_terrain_lat_lon(dir);
        let back = radial_direction(lat, lon);
        assert!((back - dir).length() < 1e-9);
    }
}
