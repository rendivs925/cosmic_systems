//! Orbital-trajectory prediction cache and drawing.

use super::super::components::*;
use super::RocketDebugConfig;
use super::RocketDebugGizmos;
use crate::infrastructure::bevy_adapters::entity_components::PlanetComponent;
use bevy::math::DVec3;
use bevy::prelude::*;

/// Samples along one full orbit for the trajectory polyline.
const TRAJECTORY_SAMPLE_COUNT: usize = 128;

/// Recompute only when this changes (stage switch, thrust toggle, or km-level
/// apoapsis/periapsis drift), not every frame.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TrajectoryStateKey {
    active_stage: usize,
    thrusting: bool,
    apoapsis_km: i64,
    periapsis_km: i64,
}

impl TrajectoryStateKey {
    fn new(propulsion: &RocketPropulsion, orbital: &OrbitalElements) -> Self {
        Self {
            active_stage: propulsion.active_stage,
            thrusting: propulsion.throttle.clamp(0.0, 1.0) > 0.0,
            apoapsis_km: (orbital.apoapsis_m / 1000.0) as i64,
            periapsis_km: (orbital.periapsis_m / 1000.0) as i64,
        }
    }
}

/// Cached trajectory points in planet-centered meters, plus the state key that
/// produced them. Drawn offset by the live planet transform each frame.
#[derive(Resource, Default)]
pub(super) struct TrajectoryCache {
    key: Option<TrajectoryStateKey>,
    /// Planet-centered sample positions (meters) forming the orbit polyline.
    points_planet_frame: Vec<DVec3>,
}

/// Rebuild the trajectory polyline only when orbital state meaningfully
/// changes; otherwise reuse cached samples (drawn against the live planet
/// transform). Avoids per-frame Kepler propagation during coast flight.
pub(super) fn update_trajectory_cache(
    config: Res<RocketDebugConfig>,
    rocket_query: Query<(&RocketPropulsion, &OrbitalElements)>,
    mut cache: ResMut<TrajectoryCache>,
) {
    if !config.show_trajectory {
        return;
    }

    let Some((propulsion, orbital)) = rocket_query.iter().next() else {
        return;
    };

    let period = orbital.orbital_period_s;
    // Skip hyperbolic/escape or degenerate orbits.
    if !period.is_finite() || period <= 0.0 || period > 86400.0 * 365.0 {
        cache.points_planet_frame.clear();
        return;
    }

    let key = TrajectoryStateKey::new(propulsion, orbital);
    if cache.key.as_ref() == Some(&key) && !cache.points_planet_frame.is_empty() {
        return;
    }

    let steps = TRAJECTORY_SAMPLE_COUNT;
    let e = orbital.eccentricity.clamp(0.0, 0.999);
    let a = orbital.semi_major_axis_m;

    // Precompute the perifocal-to-inertial rotation terms once.
    let cos_raan = orbital.longitude_ascending_node_rad.cos();
    let sin_raan = orbital.longitude_ascending_node_rad.sin();
    let cos_inc = orbital.inclination_rad.cos();
    let sin_inc = orbital.inclination_rad.sin();
    let cos_arg = orbital.argument_of_periapsis_rad.cos();
    let sin_arg = orbital.argument_of_periapsis_rad.sin();

    cache.points_planet_frame.clear();
    cache.points_planet_frame.reserve(steps + 1);

    for i in 0..=steps {
        let mean_anomaly =
            orbital.mean_anomaly_rad + 2.0 * std::f64::consts::PI * i as f64 / steps as f64;

        // Newton iteration for Kepler's equation: M = E - e·sin(E).
        let mut eccentric = mean_anomaly;
        for _ in 0..8 {
            eccentric -=
                (eccentric - e * eccentric.sin() - mean_anomaly) / (1.0 - e * eccentric.cos());
        }

        // True anomaly and radius from the conic equation.
        let cos_e = eccentric.cos();
        let sin_e = eccentric.sin();
        let cos_nu = (cos_e - e) / (1.0 - e * cos_e);
        let sin_nu = (1.0 - e * e).sqrt() * sin_e / (1.0 - e * cos_e);
        let r = a * (1.0 - e * cos_e);

        // Perifocal coordinates.
        let u = r * cos_nu;
        let v = r * sin_nu;
        let angle = orbital.argument_of_periapsis_rad;
        let p_x = u * angle.cos() - v * angle.sin();
        let p_y = u * angle.sin() + v * angle.cos();

        let x = (cos_raan * cos_arg - sin_raan * sin_arg * cos_inc) * p_x
            + (-cos_raan * sin_arg - sin_raan * cos_arg * cos_inc) * p_y;
        let y = (sin_raan * cos_arg + cos_raan * sin_arg * cos_inc) * p_x
            + (-sin_raan * sin_arg + cos_raan * cos_arg * cos_inc) * p_y;
        let z = sin_arg * sin_inc * p_x + cos_arg * sin_inc * p_y;

        cache.points_planet_frame.push(DVec3::new(x, y, z));
    }

    cache.key = Some(key);
}

pub(super) fn draw_orbital_trajectory(
    planet_query: Query<(&PlanetComponent, &Transform)>,
    cache: Res<TrajectoryCache>,
    mut gizmos: Gizmos<RocketDebugGizmos>,
) {
    if cache.points_planet_frame.len() < 2 {
        return;
    }

    let Some((_, planet_transform)) = planet_query.iter().next() else {
        return;
    };
    let planet_pos = planet_transform.translation.as_dvec3();

    let to_world = |p: DVec3| (planet_pos + p).as_vec3();

    // Polyline via linestrip (one primitive for the whole orbit).
    gizmos.linestrip(
        cache.points_planet_frame.iter().map(|p| to_world(*p)),
        Color::srgb(0.5, 0.5, 1.0),
    );

    // Apoapsis (green) / periapsis (red) from cached extremes.
    if let (Some(ap), Some(pe)) = (
        cache
            .points_planet_frame
            .iter()
            .max_by(|a, b| a.length().total_cmp(&b.length())),
        cache
            .points_planet_frame
            .iter()
            .min_by(|a, b| a.length().total_cmp(&b.length())),
    ) {
        gizmos.sphere(to_world(*ap), 3.0, Color::srgb(0.0, 1.0, 0.0));
        gizmos.sphere(to_world(*pe), 3.0, Color::srgb(1.0, 0.0, 0.0));
    }
}
