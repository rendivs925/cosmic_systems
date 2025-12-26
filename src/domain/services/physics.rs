use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::Planet;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::SimulationParameters;
use bevy::math::Vec3;

pub fn calculate_precession_angle(precession_rate: f32, delta_time: f32) -> f32 {
    precession_rate * delta_time
}

pub fn calculate_thrust_magnitude(gyro: &Gyroscope, params: &SimulationParameters) -> f32 {
    params.thrust_scale * gyro.asymmetry * (gyro.spin_rate.powi(2)) * gyro.precession_rate
}

pub fn calculate_total_thrust(gyros: &[&Gyroscope], params: &SimulationParameters) -> Vec3 {
    let mut total_thrust = Vec3::ZERO;
    for gyro in gyros {
        let thrust_mag = calculate_thrust_magnitude(gyro, params);
        total_thrust += thrust_mag * gyro.angular_momentum.normalize_or_zero();
    }
    // Average thrust if multiple gyros
    if !gyros.is_empty() {
        total_thrust /= gyros.len() as f32;
    }
    total_thrust
}

pub fn calculate_arrow_scale(thrust: Vec3) -> f32 {
    thrust.length().clamp(0.1, 10.0)
}

// Orbital mechanics functions for solar system simulation

/// Calculate the position of a planet/moon in its orbit at a given time
/// Uses simplified circular orbit approximation (Kepler's first law)
pub fn calculate_planet_position(
    planet: &Planet,
    time_days: f32,
    solar_params: &SolarSystemParameters,
    parent_position: Vec3,
) -> Vec3 {
    if planet.name == "Sun" {
        // Sun is at the origin
        return Vec3::ZERO;
    }

    // Calculate angle based on orbital period
    let angle = 2.0 * std::f32::consts::PI * time_days / planet.orbital_period_days;

    let relative_pos = if planet.parent_entity.is_some() {
        // Moons keep circular orbits around their parent for simplicity.
        let distance = calculate_orbit_radius_units(planet, solar_params);
        Vec3::new(distance * angle.cos(), 0.0, distance * angle.sin())
    } else if let Some(elements) = get_orbital_elements(&planet.name) {
        let mean_motion = mean_motion_rad_per_day(elements.semi_major_axis_au);
        let mean_anomaly = normalize_radians(elements.mean_anomaly_rad + mean_motion * time_days);
        let eccentric_anomaly = solve_kepler(mean_anomaly, elements.eccentricity);
        let true_anomaly = true_anomaly(eccentric_anomaly, elements.eccentricity);
        let radius_au = elements.semi_major_axis_au
            * (1.0 - elements.eccentricity * eccentric_anomaly.cos());
        let r = solar_params.au_to_units(radius_au);
        let arg = elements.arg_periapsis_rad + true_anomaly;
        let cos_omega = elements.long_asc_node_rad.cos();
        let sin_omega = elements.long_asc_node_rad.sin();
        let cos_i = elements.inclination_rad.cos();
        let sin_i = elements.inclination_rad.sin();
        let cos_arg = arg.cos();
        let sin_arg = arg.sin();

        Vec3::new(
            r * (cos_omega * cos_arg - sin_omega * sin_arg * cos_i),
            r * (sin_arg * sin_i),
            r * (sin_omega * cos_arg + cos_omega * sin_arg * cos_i),
        )
    } else {
        // Fallback to a circular orbit when no elements are defined.
        let distance = calculate_orbit_radius_units(planet, solar_params);
        Vec3::new(distance * angle.cos(), 0.0, distance * angle.sin())
    };

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
        planet.orbital_distance_au * solar_params.scale_factor * 500.0
    } else if let Some(elements) = get_orbital_elements(&planet.name) {
        solar_params.au_to_units(elements.semi_major_axis_au)
    } else {
        solar_params.au_to_units(planet.orbital_distance_au)
    }
}

pub struct OrbitShape {
    pub semi_major_axis_units: f32,
    pub eccentricity: f32,
    pub inclination_rad: f32,
    pub long_asc_node_rad: f32,
    pub arg_periapsis_rad: f32,
}

pub fn orbit_shape_for(planet: &Planet, solar_params: &SolarSystemParameters) -> OrbitShape {
    if planet.parent_entity.is_some() {
        OrbitShape {
            semi_major_axis_units: calculate_orbit_radius_units(planet, solar_params),
            eccentricity: 0.0,
            inclination_rad: 0.0,
            long_asc_node_rad: 0.0,
            arg_periapsis_rad: 0.0,
        }
    } else if let Some(elements) = get_orbital_elements(&planet.name) {
        OrbitShape {
            semi_major_axis_units: solar_params.au_to_units(elements.semi_major_axis_au),
            eccentricity: elements.eccentricity,
            inclination_rad: elements.inclination_rad,
            long_asc_node_rad: elements.long_asc_node_rad,
            arg_periapsis_rad: elements.arg_periapsis_rad,
        }
    } else {
        OrbitShape {
            semi_major_axis_units: calculate_orbit_radius_units(planet, solar_params),
            eccentricity: 0.0,
            inclination_rad: 0.0,
            long_asc_node_rad: 0.0,
            arg_periapsis_rad: 0.0,
        }
    }
}

struct OrbitalElements {
    semi_major_axis_au: f32,
    eccentricity: f32,
    inclination_rad: f32,
    long_asc_node_rad: f32,
    arg_periapsis_rad: f32,
    mean_anomaly_rad: f32,
}

fn get_orbital_elements(name: &str) -> Option<OrbitalElements> {
    // J2000 mean orbital elements (degrees) for a Keplerian baseline.
    // Source: NASA planetary fact sheets (approximate).
    match name {
        "Mercury" => Some(elements_from_degrees(
            0.38709927, 0.20563593, 7.00497902, 48.33076593, 77.45779628, 252.25032350,
        )),
        "Venus" => Some(elements_from_degrees(
            0.72333566, 0.00677672, 3.39467605, 76.67984255, 131.60246718, 181.97909950,
        )),
        "Earth" => Some(elements_from_degrees(
            1.00000261, 0.01671123, -0.00001531, 0.0, 102.93768193, 100.46457166,
        )),
        "Mars" => Some(elements_from_degrees(
            1.52371034, 0.09339410, 1.84969142, 49.55809321, 336.04084, 355.45332,
        )),
        "Jupiter" => Some(elements_from_degrees(
            5.20288700, 0.04838624, 1.30439695, 100.47390909, 14.72847983, 34.39644051,
        )),
        "Saturn" => Some(elements_from_degrees(
            9.53667594, 0.05386179, 2.48599187, 113.66242448, 92.59887831, 49.95424423,
        )),
        "Uranus" => Some(elements_from_degrees(
            19.18916464, 0.04725744, 0.77263783, 74.01692503, 170.95427630, 313.23810451,
        )),
        "Neptune" => Some(elements_from_degrees(
            30.06992276, 0.00859048, 1.77004347, 131.78422574, 44.96476227, 304.87964,
        )),
        _ => None,
    }
}

fn elements_from_degrees(
    a: f32,
    e: f32,
    i_deg: f32,
    long_asc_node_deg: f32,
    long_peri_deg: f32,
    mean_longitude_deg: f32,
) -> OrbitalElements {
    let mean_anomaly_deg = mean_longitude_deg - long_peri_deg;
    let arg_peri_deg = long_peri_deg - long_asc_node_deg;
    OrbitalElements {
        semi_major_axis_au: a,
        eccentricity: e,
        inclination_rad: i_deg.to_radians(),
        long_asc_node_rad: long_asc_node_deg.to_radians(),
        arg_periapsis_rad: arg_peri_deg.to_radians(),
        mean_anomaly_rad: mean_anomaly_deg.to_radians(),
    }
}

fn solve_kepler(mean_anomaly: f32, eccentricity: f32) -> f32 {
    let mut eccentric_anomaly = mean_anomaly;
    for _ in 0..8 {
        let f = eccentric_anomaly - eccentricity * eccentric_anomaly.sin() - mean_anomaly;
        let f_prime = 1.0 - eccentricity * eccentric_anomaly.cos();
        eccentric_anomaly -= f / f_prime;
    }
    eccentric_anomaly
}

fn true_anomaly(eccentric_anomaly: f32, eccentricity: f32) -> f32 {
    let sin_v = (1.0 - eccentricity * eccentricity).sqrt() * eccentric_anomaly.sin()
        / (1.0 - eccentricity * eccentric_anomaly.cos());
    let cos_v = (eccentric_anomaly.cos() - eccentricity)
        / (1.0 - eccentricity * eccentric_anomaly.cos());
    sin_v.atan2(cos_v)
}

fn mean_motion_rad_per_day(semi_major_axis_au: f32) -> f32 {
    // Gauss gravitational constant (AU^(3/2)/day)
    const GAUSS_K: f32 = 0.01720209895;
    GAUSS_K / semi_major_axis_au.powf(1.5)
}

fn normalize_radians(angle: f32) -> f32 {
    angle.rem_euclid(std::f32::consts::TAU)
}

/// Calculate the rotation angle of a planet at a given time
pub fn calculate_planet_rotation(planet: &Planet, time_days: f32) -> f32 {
    // Convert days to hours and calculate rotation
    let time_hours = time_days * 24.0;
    2.0 * std::f32::consts::PI * time_hours / planet.rotation_period_hours
}

/// Calculate the visual radius for a planet/moon based on actual astronomical sizes with correct relative proportions
pub fn calculate_visual_radius(planet: &Planet, solar_params: &SolarSystemParameters) -> f32 {
    // Astronomical radii in kilometers (actual values)
    let astronomical_radii = [
        ("Sun", 696342.0),
        ("Jupiter", 69911.0),
        ("Saturn", 58232.0),
        ("Uranus", 25362.0),
        ("Neptune", 24622.0),
        ("Earth", 6371.0),
        ("Venus", 6051.8),
        ("Mars", 3389.5),
        ("Mercury", 2439.7),
        // Major moons
        ("Moon", 1737.4),
        ("Ganymede", 2634.1),
        ("Callisto", 2410.3),
        ("Io", 1821.6),
        ("Europa", 1560.8),
        ("Titan", 2574.7),
        ("Rhea", 763.8),
        ("Iapetus", 734.5),
        ("Dione", 561.4),
        ("Tethys", 531.1),
        ("Enceladus", 252.1),
        ("Mimas", 198.2),
        ("Hyperion", 135.0),
        ("Titania", 788.4),
        ("Oberon", 761.4),
        ("Umbriel", 584.7),
        ("Ariel", 578.9),
        ("Miranda", 235.8),
        ("Triton", 1353.4),
        ("Phobos", 11.1),
        ("Deimos", 6.2),
    ];

    // Find the actual astronomical radius for this planet
    let astronomical_radius_km = astronomical_radii
        .iter()
        .find(|(name, _)| *name == planet.name)
        .map(|(_, radius)| *radius)
        .unwrap_or(planet.radius_km); // Fallback to stored value

    // Calculate size relative to Mercury (smallest planet) to establish baseline
    let mercury_radius_km = 2439.7;
    let relative_to_mercury = astronomical_radius_km / mercury_radius_km;

    // Apply logarithmic scaling to make large objects manageable while preserving proportions
    // This ensures Mercury is visible but Sun doesn't overwhelm the scene
    let scaled_relative = if astronomical_radius_km > mercury_radius_km * 10.0 {
        // For large objects (planets and Sun), use logarithmic scaling
        (relative_to_mercury.ln() * 2.0).max(1.0)
    } else {
        // For smaller objects (moons), maintain more linear scaling
        relative_to_mercury.max(0.1)
    };

    // Apply final scaling for visibility in the simulation
    scaled_relative * solar_params.planet_scale
}

/// Calculate the visual radius for the Sun with appropriate scaling
pub fn calculate_sun_visual_radius(solar_params: &SolarSystemParameters) -> f32 {
    // Sun radius is ~696,000 km, Jupiter is ~71,000 km
    // Sun is about 9.7 times larger than Jupiter
    // For visualization, make it reasonably larger than Jupiter but not overwhelming
    let jupiter_visual_radius = calculate_visual_radius(&Planet::create_jupiter(), solar_params);
    jupiter_visual_radius * 2.5 // Sun appears 2.5x larger than Jupiter visually
}

/// Get the astronomical unit in simulation units
pub const AU_IN_KM: f32 = 149597870.7; // 1 AU in kilometers

/// Convert astronomical units to simulation distance units
pub fn au_to_simulation_units(au: f32, solar_params: &SolarSystemParameters) -> f32 {
    au * solar_params.scale_factor
}
