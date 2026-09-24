//! Solar-map orbital geometry: approximate parent-relative moon and planet
//! display positions. Kernel-backed primary bodies use `EphemerisSnapshot`
//! instead; these helpers are presentation-only map coordinates.

use crate::domain::entities::planet::Planet;
use crate::domain::math::DVec3;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;

pub const MOON_ORBIT_SCALE: f32 = 1.0;

#[derive(Clone, Copy, Debug)]
pub struct OrbitalElements {
    pub semi_major_axis_au: f32,
    pub eccentricity: f32,
    pub inclination_rad: f32,
    pub long_asc_node_rad: f32,
    pub arg_periapsis_rad: f32,
    pub mean_anomaly_rad: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitShape {
    pub semi_major_axis_units: f32,
    pub eccentricity: f32,
    pub inclination_rad: f32,
    pub long_asc_node_rad: f32,
    pub arg_periapsis_rad: f32,
}

/// Evaluate an explicitly approximate parent-relative moon position in f64
/// display units. Kernel-backed primary bodies must use `EphemerisSnapshot`.
pub fn calculate_planet_position_f64(
    planet: &Planet,
    time_days: f64,
    solar_params: &SolarSystemParameters,
    parent_position: DVec3,
    parent_axial_tilt_deg: Option<f32>,
) -> DVec3 {
    if planet.name == "Sun" {
        return DVec3::ZERO;
    }

    if planet.parent_entity.is_none() {
        return parent_position;
    }

    // Parent-relative moon propagation and ribbon generation share this shape.
    let orbit_shape = orbit_shape_for_at_time(planet, solar_params, time_days);
    let eccentric_anomaly = orbital_eccentric_anomaly(planet, &orbit_shape, time_days);
    let relative_position = orbit_point_f64(&orbit_shape, eccentric_anomaly);

    let relative_position = if planet.parent_entity.is_some() {
        parent_axial_tilt_deg.map_or(relative_position, |tilt_deg| {
            let tilt = (tilt_deg as f64).to_radians();
            DVec3::new(
                relative_position.x * tilt.cos() - relative_position.y * tilt.sin(),
                relative_position.x * tilt.sin() + relative_position.y * tilt.cos(),
                relative_position.z,
            )
        })
    } else {
        relative_position
    };

    parent_position + relative_position
}

/// Sample the same eccentric-anomaly ellipse used by the orbit ribbon.
pub fn orbit_point_f64(orbit_shape: &OrbitShape, eccentric_anomaly: f64) -> DVec3 {
    let eccentricity = orbit_shape.eccentricity.clamp(0.0, 0.99) as f64;
    let semi_major_axis = orbit_shape.semi_major_axis_units as f64;
    let semi_minor = semi_major_axis * (1.0 - eccentricity * eccentricity).sqrt();
    transform_orbital_point_f64(
        semi_major_axis * (eccentric_anomaly.cos() - eccentricity),
        semi_minor * eccentric_anomaly.sin(),
        orbit_shape.inclination_rad as f64,
        orbit_shape.long_asc_node_rad as f64,
        orbit_shape.arg_periapsis_rad as f64,
    )
}

pub(super) fn orbital_eccentric_anomaly(
    planet: &Planet,
    orbit_shape: &OrbitShape,
    time_days: f64,
) -> f64 {
    let mean_anomaly = orbital_elements_for(planet).map_or_else(
        || std::f64::consts::TAU * time_days / planet.orbital_period_days as f64,
        |elements| {
            elements.mean_anomaly_rad as f64
                + std::f64::consts::TAU * time_days / planet.orbital_period_days as f64
        },
    );

    solve_kepler_f64(
        mean_anomaly.rem_euclid(std::f64::consts::TAU),
        orbit_shape.eccentricity.clamp(0.0, 0.99) as f64,
    )
}

fn solve_kepler_f64(mean_anomaly_rad: f64, eccentricity: f64) -> f64 {
    let mut eccentric_anomaly = if eccentricity < 0.8 {
        mean_anomaly_rad
    } else {
        std::f64::consts::PI
    };
    for _ in 0..16 {
        let residual =
            eccentric_anomaly - eccentricity * eccentric_anomaly.sin() - mean_anomaly_rad;
        let derivative = 1.0 - eccentricity * eccentric_anomaly.cos();
        eccentric_anomaly -= residual / derivative;
    }
    eccentric_anomaly
}

pub fn calculate_orbit_radius_units(planet: &Planet, solar_params: &SolarSystemParameters) -> f32 {
    if planet.name == "Sun" {
        return 0.0;
    }

    if planet.parent_entity.is_some() {
        // Moon orbiting a planet - convert astronomical distance to simulation units
        // orbital_distance_au represents actual AU distance from parent planet
        // Scale massively for clear separation while maintaining relative accuracy
        planet.orbital_distance_au * solar_params.scale_factor * MOON_ORBIT_SCALE
    } else {
        solar_params.au_to_units(planet.orbital_distance_au)
    }
}

pub fn orbit_shape_for(planet: &Planet, solar_params: &SolarSystemParameters) -> OrbitShape {
    orbit_shape_for_at_time(planet, solar_params, 0.0)
}

/// Evaluate the explicitly approximate moon orbit shape at a J2000-relative
/// TDB epoch. Primary-body ribbons are sampled from DE440 instead.
pub fn orbit_shape_for_at_time(
    planet: &Planet,
    solar_params: &SolarSystemParameters,
    _days_from_j2000_tdb: f64,
) -> OrbitShape {
    if planet.parent_entity.is_some() {
        // Moon - use real orbital elements if available
        if let Some(elements) = get_moon_orbital_elements(planet) {
            OrbitShape {
                semi_major_axis_units: solar_params.au_to_units(elements.semi_major_axis_au)
                    * MOON_ORBIT_SCALE,
                eccentricity: elements.eccentricity,
                inclination_rad: elements.inclination_rad,
                long_asc_node_rad: elements.long_asc_node_rad,
                arg_periapsis_rad: elements.arg_periapsis_rad,
            }
        } else {
            // Fallback for moons without defined elements
            OrbitShape {
                semi_major_axis_units: calculate_orbit_radius_units(planet, solar_params),
                eccentricity: 0.0,
                inclination_rad: 0.0,
                long_asc_node_rad: 0.0,
                arg_periapsis_rad: 0.0,
            }
        }
    } else {
        // Fallback for bodies without defined elements
        OrbitShape {
            semi_major_axis_units: calculate_orbit_radius_units(planet, solar_params),
            eccentricity: 0.0,
            inclination_rad: 0.0,
            long_asc_node_rad: 0.0,
            arg_periapsis_rad: 0.0,
        }
    }
}

pub fn orbital_elements_for(planet: &Planet) -> Option<OrbitalElements> {
    planet
        .parent_entity
        .as_ref()
        .and_then(|_| get_moon_orbital_elements(planet))
}

// Real-world orbital elements for major moons. The shared celestial catalog is
// the authority for their semimajor axes; this table contains only the
// remaining parent-relative Keplerian elements. Sources: NASA planetary fact
// sheets and JPL Horizons.
fn get_moon_orbital_elements(planet: &Planet) -> Option<OrbitalElements> {
    // Orbital elements relative to planet's equator (degrees)
    match planet.name.as_str() {
        // Earth
        "Moon" => Some(moon_elements_from_degrees(
            planet, 0.0549, 5.145, 0.0, 318.15, 135.27,
        )),

        // Mars
        "Phobos" => Some(moon_elements_from_degrees(
            planet, 0.0151, 1.08, 0.0, 0.0, 0.0,
        )),
        "Deimos" => Some(moon_elements_from_degrees(
            planet, 0.0002, 1.79, 0.0, 0.0, 0.0,
        )),

        // Jupiter (Galilean moons)
        "Io" => Some(moon_elements_from_degrees(
            planet, 0.0041, 0.05, 0.0, 0.0, 0.0,
        )),
        "Europa" => Some(moon_elements_from_degrees(
            planet, 0.0094, 0.47, 0.0, 0.0, 0.0,
        )),
        "Ganymede" => Some(moon_elements_from_degrees(
            planet, 0.0013, 0.20, 0.0, 0.0, 0.0,
        )),
        "Callisto" => Some(moon_elements_from_degrees(
            planet, 0.0074, 0.51, 0.0, 0.0, 0.0,
        )),

        // Saturn
        "Mimas" => Some(moon_elements_from_degrees(
            planet, 0.0196, 1.53, 0.0, 0.0, 0.0,
        )),
        "Enceladus" => Some(moon_elements_from_degrees(
            planet, 0.0047, 0.00, 0.0, 0.0, 0.0,
        )),
        "Tethys" => Some(moon_elements_from_degrees(
            planet, 0.0001, 1.12, 0.0, 0.0, 0.0,
        )),
        "Dione" => Some(moon_elements_from_degrees(
            planet, 0.0022, 0.02, 0.0, 0.0, 0.0,
        )),
        "Rhea" => Some(moon_elements_from_degrees(
            planet, 0.0010, 0.35, 0.0, 0.0, 0.0,
        )),
        "Titan" => Some(moon_elements_from_degrees(
            planet, 0.0288, 0.33, 0.0, 0.0, 0.0,
        )),
        "Hyperion" => Some(moon_elements_from_degrees(
            planet, 0.0274, 0.43, 0.0, 0.0, 0.0,
        )),
        "Iapetus" => Some(moon_elements_from_degrees(
            planet, 0.0286, 15.47, 0.0, 0.0, 0.0,
        )),

        // Uranus
        "Miranda" => Some(moon_elements_from_degrees(
            planet, 0.0013, 4.34, 0.0, 0.0, 0.0,
        )),
        "Ariel" => Some(moon_elements_from_degrees(
            planet, 0.0012, 0.26, 0.0, 0.0, 0.0,
        )),
        "Umbriel" => Some(moon_elements_from_degrees(
            planet, 0.0039, 0.13, 0.0, 0.0, 0.0,
        )),
        "Titania" => Some(moon_elements_from_degrees(
            planet, 0.0011, 0.34, 0.0, 0.0, 0.0,
        )),
        "Oberon" => Some(moon_elements_from_degrees(
            planet, 0.0014, 0.07, 0.0, 0.0, 0.0,
        )),

        // Neptune
        "Triton" => Some(moon_elements_from_degrees(
            planet, 0.0000, 156.87, 0.0, 0.0, 0.0,
        )),
        "Proteus" => Some(moon_elements_from_degrees(
            planet, 0.0005, 0.55, 0.0, 0.0, 0.0,
        )),
        "Nereid" => Some(moon_elements_from_degrees(
            planet, 0.7512, 7.23, 0.0, 0.0, 0.0,
        )),
        "Larissa" => Some(moon_elements_from_degrees(
            planet, 0.0014, 0.20, 0.0, 0.0, 0.0,
        )),

        _ => None,
    }
}

fn moon_elements_from_degrees(
    planet: &Planet,
    e: f32,
    i_deg: f32,
    long_asc_node_deg: f32,
    long_peri_deg: f32,
    mean_longitude_deg: f32,
) -> OrbitalElements {
    elements_from_degrees(
        planet.orbital_distance_au,
        e,
        i_deg,
        long_asc_node_deg,
        long_peri_deg,
        mean_longitude_deg,
    )
}

fn elements_from_degrees(
    a_au: f32,
    e: f32,
    i_deg: f32,
    long_asc_node_deg: f32,
    long_peri_deg: f32,
    mean_longitude_deg: f32,
) -> OrbitalElements {
    // Convert to radians and compute argument of periapsis
    let i_rad = i_deg.to_radians();
    let long_asc_node_rad = long_asc_node_deg.to_radians();
    let long_peri_rad = long_peri_deg.to_radians();
    let mean_longitude_rad = mean_longitude_deg.to_radians();

    // Argument of periapsis = longitude of periapsis - longitude of ascending node
    let arg_periapsis_rad = long_peri_rad - long_asc_node_rad;

    // Mean anomaly = mean longitude - longitude of periapsis
    let mean_anomaly_rad = mean_longitude_rad - long_peri_rad;

    OrbitalElements {
        semi_major_axis_au: a_au,
        eccentricity: e,
        inclination_rad: i_rad,
        long_asc_node_rad,
        arg_periapsis_rad,
        mean_anomaly_rad,
    }
}

#[cfg(test)]
pub(super) fn true_anomaly(eccentric_anomaly: f32, eccentricity: f32) -> f32 {
    let cos_e = eccentric_anomaly.cos();
    let sin_e = eccentric_anomaly.sin();
    let numerator = (1.0 - eccentricity * eccentricity).sqrt() * sin_e;
    let denominator = cos_e - eccentricity;
    numerator.atan2(denominator)
}

pub fn transform_orbital_point_f64(
    x_orbital: f64,
    z_orbital: f64,
    inclination_rad: f64,
    long_asc_node_rad: f64,
    arg_periapsis_rad: f64,
) -> DVec3 {
    let cos_w = arg_periapsis_rad.cos();
    let sin_w = arg_periapsis_rad.sin();
    let x1 = x_orbital * cos_w - z_orbital * sin_w;
    let z1 = x_orbital * sin_w + z_orbital * cos_w;

    let cos_i = inclination_rad.cos();
    let sin_i = inclination_rad.sin();
    let y2 = z1 * sin_i;
    let z2 = z1 * cos_i;

    let cos_omega = long_asc_node_rad.cos();
    let sin_omega = long_asc_node_rad.sin();
    DVec3::new(
        x1 * cos_omega - z2 * sin_omega,
        y2,
        x1 * sin_omega + z2 * cos_omega,
    )
}
