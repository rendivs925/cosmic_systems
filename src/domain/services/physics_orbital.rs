use crate::domain::entities::planet::Planet;
use crate::domain::services::physics_kepler::solve_kepler_adaptive;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::math::{Quat, Vec3};

pub const MOON_ORBIT_SCALE: f32 = 60.0;

#[derive(Clone, Copy, Debug)]
pub struct OrbitalElements {
    pub semi_major_axis_au: f32,
    pub eccentricity: f32,
    pub inclination_rad: f32,
    pub long_asc_node_rad: f32,
    pub arg_periapsis_rad: f32,
    pub mean_anomaly_rad: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct OrbitShape {
    pub semi_major_axis_units: f32,
    pub eccentricity: f32,
    pub inclination_rad: f32,
    pub long_asc_node_rad: f32,
    pub arg_periapsis_rad: f32,
}

/// Calculate the position of a planet/moon in its orbit at a given time
/// Uses simplified circular orbit approximation (Kepler's first law)
/// iterations parameter controls Kepler solver accuracy (default: 8)
pub fn calculate_planet_position(
    planet: &Planet,
    time_days: f32,
    solar_params: &SolarSystemParameters,
    parent_position: Vec3,
    parent_axial_tilt_deg: Option<f32>,
) -> Vec3 {
    calculate_planet_position_with_quality(
        planet,
        time_days,
        solar_params,
        parent_position,
        parent_axial_tilt_deg,
        8,
    )
}

/// Calculate the position of a planet/moon with configurable quality/performance
pub fn calculate_planet_position_with_quality(
    planet: &Planet,
    time_days: f32,
    solar_params: &SolarSystemParameters,
    parent_position: Vec3,
    parent_axial_tilt_deg: Option<f32>,
    kepler_iterations: u32,
) -> Vec3 {
    if planet.name == "Sun" {
        // Sun is at the origin
        return Vec3::ZERO;
    }

    // Calculate angle based on orbital period
    let angle = 2.0 * std::f32::consts::PI * time_days / planet.orbital_period_days;

    let mut relative_pos = if planet.parent_entity.is_some() {
        // Moons - use real orbital elements for accurate position calculation
        if let Some(elements) = get_moon_orbital_elements(&planet.name) {
            let mean_motion = mean_motion_from_period_days(planet.orbital_period_days);
            let mean_anomaly =
                normalize_radians(elements.mean_anomaly_rad + mean_motion * time_days);
            let eccentric_anomaly =
                solve_kepler_adaptive(mean_anomaly, elements.eccentricity, kepler_iterations);
            let true_anomaly = true_anomaly(eccentric_anomaly, elements.eccentricity);
            let radius_au = elements.semi_major_axis_au
                * (1.0 - elements.eccentricity * eccentric_anomaly.cos());
            let r = solar_params.au_to_units(radius_au) * MOON_ORBIT_SCALE;

            // Position in orbital plane (periapsis at +X)
            let x_orbital = r * true_anomaly.cos();
            let z_orbital = r * true_anomaly.sin();

            // Transform to 3D space using same method as orbit mesh
            transform_orbital_point(
                x_orbital,
                z_orbital,
                elements.inclination_rad,
                elements.long_asc_node_rad,
                elements.arg_periapsis_rad,
            )
        } else {
            // Fallback to simple circular orbit for moons without defined elements
            // Use 3D transformation for consistency (no inclination/nodes for fallback)
            let distance = calculate_orbit_radius_units(planet, solar_params);
            let x_orbital = distance * angle.cos();
            let z_orbital = distance * angle.sin();
            transform_orbital_point(x_orbital, z_orbital, 0.0, 0.0, 0.0)
        }
    } else if let Some(elements) = get_orbital_elements(&planet.name) {
        let mean_motion = mean_motion_rad_per_day(elements.semi_major_axis_au);
        let mean_anomaly = normalize_radians(elements.mean_anomaly_rad + mean_motion * time_days);
        let eccentric_anomaly =
            solve_kepler_adaptive(mean_anomaly, elements.eccentricity, kepler_iterations);
        let true_anomaly = true_anomaly(eccentric_anomaly, elements.eccentricity);
        let radius_au =
            elements.semi_major_axis_au * (1.0 - elements.eccentricity * eccentric_anomaly.cos());
        let r = solar_params.au_to_units(radius_au);

        // Position in orbital plane (periapsis at +X)
        let x_orbital = r * true_anomaly.cos();
        let z_orbital = r * true_anomaly.sin();

        // Transform to 3D space using same method as orbit mesh
        transform_orbital_point(
            x_orbital,
            z_orbital,
            elements.inclination_rad,
            elements.long_asc_node_rad,
            elements.arg_periapsis_rad,
        )
    } else {
        // Fallback to a circular orbit when no elements are defined
        // Use 3D transformation for consistency (no inclination/nodes for fallback)
        let distance = calculate_orbit_radius_units(planet, solar_params);
        let x_orbital = distance * angle.cos();
        let z_orbital = distance * angle.sin();
        transform_orbital_point(x_orbital, z_orbital, 0.0, 0.0, 0.0)
    };

    if planet.parent_entity.is_some() {
        if let Some(tilt_deg) = parent_axial_tilt_deg {
            let tilt = Quat::from_rotation_z(tilt_deg.to_radians());
            relative_pos = tilt * relative_pos;
        }
    }

    // Add parent position to get absolute position
    parent_position + relative_pos
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
    } else if let Some(elements) = get_orbital_elements(&planet.name) {
        solar_params.au_to_units(elements.semi_major_axis_au)
    } else {
        solar_params.au_to_units(planet.orbital_distance_au)
    }
}

pub fn orbit_shape_for(planet: &Planet, solar_params: &SolarSystemParameters) -> OrbitShape {
    if planet.parent_entity.is_some() {
        // Moon - use real orbital elements if available
        if let Some(elements) = get_moon_orbital_elements(&planet.name) {
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
    } else if let Some(elements) = get_orbital_elements(&planet.name) {
        // Planet - use real orbital elements
        OrbitShape {
            semi_major_axis_units: solar_params.au_to_units(elements.semi_major_axis_au),
            eccentricity: elements.eccentricity,
            inclination_rad: elements.inclination_rad,
            long_asc_node_rad: elements.long_asc_node_rad,
            arg_periapsis_rad: elements.arg_periapsis_rad,
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

// Helper function to transform a point from orbital plane to 3D space
// This ensures orbit mesh and position calculation use identical transformation
pub fn transform_orbital_point(
    x_orbital: f32,
    z_orbital: f32,
    inclination: f32,
    long_asc_node: f32,
    arg_periapsis: f32,
) -> Vec3 {
    // Apply argument of periapsis rotation (in orbital plane)
    let cos_w = arg_periapsis.cos();
    let sin_w = arg_periapsis.sin();
    let x1 = x_orbital * cos_w - z_orbital * sin_w;
    let z1 = x_orbital * sin_w + z_orbital * cos_w;

    // Apply inclination (tilt the orbital plane)
    let cos_i = inclination.cos();
    let sin_i = inclination.sin();
    let y2 = z1 * sin_i;
    let z2 = z1 * cos_i;
    let x2 = x1;

    // Apply longitude of ascending node (rotate around Z-axis)
    let cos_omega = long_asc_node.cos();
    let sin_omega = long_asc_node.sin();
    let x3 = x2 * cos_omega - z2 * sin_omega;
    let z3 = x2 * sin_omega + z2 * cos_omega;

    Vec3::new(x3, y2, z3)
}

pub fn orbital_elements_for(planet: &Planet) -> Option<OrbitalElements> {
    if planet.parent_entity.is_some() {
        get_moon_orbital_elements(&planet.name)
    } else {
        get_orbital_elements(&planet.name)
    }
}

// Real-world orbital elements for major moons
fn get_moon_orbital_elements(name: &str) -> Option<OrbitalElements> {
    // Orbital elements relative to planet's equator (degrees)
    // Sources: NASA planetary fact sheets, JPL horizons
    match name {
        // Earth
        "Moon" => Some(moon_elements_from_degrees(
            0.002569, 0.0549, 5.145, 0.0, 318.15, 135.27,
        )),

        // Mars
        "Phobos" => Some(moon_elements_from_degrees(
            0.000063, 0.0151, 1.08, 0.0, 0.0, 0.0,
        )),
        "Deimos" => Some(moon_elements_from_degrees(
            0.000157, 0.0002, 1.79, 0.0, 0.0, 0.0,
        )),

        // Jupiter (Galilean moons)
        "Io" => Some(moon_elements_from_degrees(
            0.002819, 0.0041, 0.05, 0.0, 0.0, 0.0,
        )),
        "Europa" => Some(moon_elements_from_degrees(
            0.004485, 0.0094, 0.47, 0.0, 0.0, 0.0,
        )),
        "Ganymede" => Some(moon_elements_from_degrees(
            0.007155, 0.0013, 0.20, 0.0, 0.0, 0.0,
        )),
        "Callisto" => Some(moon_elements_from_degrees(
            0.012585, 0.0074, 0.51, 0.0, 0.0, 0.0,
        )),

        // Saturn
        "Mimas" => Some(moon_elements_from_degrees(
            0.001239, 0.0196, 1.53, 0.0, 0.0, 0.0,
        )),
        "Enceladus" => Some(moon_elements_from_degrees(
            0.001590, 0.0047, 0.00, 0.0, 0.0, 0.0,
        )),
        "Tethys" => Some(moon_elements_from_degrees(
            0.001969, 0.0001, 1.12, 0.0, 0.0, 0.0,
        )),
        "Dione" => Some(moon_elements_from_degrees(
            0.002522, 0.0022, 0.02, 0.0, 0.0, 0.0,
        )),
        "Rhea" => Some(moon_elements_from_degrees(
            0.003521, 0.0010, 0.35, 0.0, 0.0, 0.0,
        )),
        "Titan" => Some(moon_elements_from_degrees(
            0.008168, 0.0288, 0.33, 0.0, 0.0, 0.0,
        )),
        "Hyperion" => Some(moon_elements_from_degrees(
            0.009893, 0.0274, 0.43, 0.0, 0.0, 0.0,
        )),
        "Iapetus" => Some(moon_elements_from_degrees(
            0.023781, 0.0286, 15.47, 0.0, 0.0, 0.0,
        )),

        // Uranus
        "Miranda" => Some(moon_elements_from_degrees(
            0.000867, 0.0013, 4.34, 0.0, 0.0, 0.0,
        )),
        "Ariel" => Some(moon_elements_from_degrees(
            0.001276, 0.0012, 0.26, 0.0, 0.0, 0.0,
        )),
        "Umbriel" => Some(moon_elements_from_degrees(
            0.001778, 0.0039, 0.13, 0.0, 0.0, 0.0,
        )),
        "Titania" => Some(moon_elements_from_degrees(
            0.002914, 0.0011, 0.34, 0.0, 0.0, 0.0,
        )),
        "Oberon" => Some(moon_elements_from_degrees(
            0.003898, 0.0014, 0.07, 0.0, 0.0, 0.0,
        )),

        // Neptune
        "Triton" => Some(moon_elements_from_degrees(
            0.002371, 0.0000, 156.87, 0.0, 0.0, 0.0, // Retrograde orbit!
        )),
        "Proteus" => Some(moon_elements_from_degrees(
            0.000787, 0.0005, 0.55, 0.0, 0.0, 0.0,
        )),
        "Nereid" => Some(moon_elements_from_degrees(
            0.036915, 0.7512, 7.23, 0.0, 0.0, 0.0, // Highly eccentric!
        )),
        "Larissa" => Some(moon_elements_from_degrees(
            0.000489, 0.0014, 0.20, 0.0, 0.0, 0.0,
        )),

        _ => None,
    }
}

fn moon_elements_from_degrees(
    a_au: f32,
    e: f32,
    i_deg: f32,
    long_asc_node_deg: f32,
    long_peri_deg: f32,
    mean_longitude_deg: f32,
) -> OrbitalElements {
    elements_from_degrees(
        a_au,
        e,
        i_deg,
        long_asc_node_deg,
        long_peri_deg,
        mean_longitude_deg,
    )
}

fn get_orbital_elements(name: &str) -> Option<OrbitalElements> {
    // J2000 mean orbital elements (degrees) for a Keplerian baseline.
    // Sources: NASA planetary fact sheets, JPL horizons
    match name {
        "Mercury" => Some(elements_from_degrees(
            0.387098, 0.205630, 7.00487, 48.33167, 29.12478, 252.25167,
        )),
        "Venus" => Some(elements_from_degrees(
            0.723332, 0.006772, 3.39471, 76.67984, 54.85229, 181.97973,
        )),
        "Earth" => Some(elements_from_degrees(
            1.000000, 0.016708, 0.00005, 174.87317, 288.06405, 357.52911,
        )),
        "Mars" => Some(elements_from_degrees(
            1.523679, 0.093412, 1.85061, 49.57854, 286.5373, 355.4330,
        )),
        "Jupiter" => Some(elements_from_degrees(
            5.204267, 0.048393, 1.30490, 100.46441, 273.86785, 34.35152,
        )),
        "Saturn" => Some(elements_from_degrees(
            9.582026, 0.055723, 2.48599, 113.66550, 339.39265, 50.07744,
        )),
        "Uranus" => Some(elements_from_degrees(
            19.191263, 0.047167, 0.77264, 74.00595, 96.99897, 314.05501,
        )),
        "Neptune" => Some(elements_from_degrees(
            30.068963, 0.008586, 1.77004, 131.78423, 273.21967, 304.34867,
        )),
        _ => None,
    }
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



fn true_anomaly(eccentric_anomaly: f32, eccentricity: f32) -> f32 {
    let cos_e = eccentric_anomaly.cos();
    let sin_e = eccentric_anomaly.sin();
    let tan_half = ((1.0 + eccentricity) / (1.0 - eccentricity)).sqrt() * (sin_e / (cos_e + 1.0));
    2.0 * tan_half.atan()
}

fn mean_motion_rad_per_day(semi_major_axis_au: f32) -> f32 {
    // Kepler's third law: n = sqrt(GM/a^3)
    // For solar system, GM_sun ≈ 1.327 × 10^20 m^3 s^-2
    // Convert to radians per day
    let mu = 2.9591220828559093e-4; // AU^3 / day^2 (solar gravitational parameter)
    (mu / (semi_major_axis_au.powi(3))).sqrt()
}

fn mean_motion_from_period_days(period_days: f32) -> f32 {
    2.0 * std::f32::consts::PI / period_days
}

fn normalize_radians(angle: f32) -> f32 {
    let two_pi = 2.0 * std::f32::consts::PI;
    let normalized = angle % two_pi;
    if normalized < 0.0 {
        normalized + two_pi
    } else {
        normalized
    }
}

/// Calculate the position of terrain/launch sites in orbital mechanics
/// This combines planet orbital position with terrain local coordinates
pub fn calculate_terrain_orbital_position(
    terrain_coords: &crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates,
    planet: &Planet,
    time_days: f32,
    solar_params: &SolarSystemParameters,
) -> Vec3 {
    // First, get planet's orbital position
    let planet_position = calculate_planet_position(
        planet,
        time_days,
        solar_params,
        Vec3::ZERO, // Sun at origin
        None, // No parent for Earth
    );

    // Calculate Earth's rotation at this time
    let earth_rotation_angle = if planet.name == "Earth" {
        use crate::domain::services::physics_utils::calculate_planet_rotation;
        calculate_planet_rotation(planet, time_days)
    } else {
        0.0 // For other planets, rotation not implemented yet
    };

    // Convert launch site coordinates to position relative to planet center
    let relative_position = terrain_coords.to_planet_relative_position(planet);

    // Apply planet's axial rotation
    let rotated_position = Quat::from_rotation_y(earth_rotation_angle) * relative_position;

    // Add to planet's orbital position
    planet_position + rotated_position
}