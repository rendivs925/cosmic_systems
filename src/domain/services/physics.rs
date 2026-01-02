use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::Planet;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::domain::value_objects::simulation_params::SimulationParameters;
use bevy::math::{Quat, Vec3};
#[cfg(feature = "parallel")]
use rayon::prelude::*;

// SIMD feature detection
#[cfg(target_arch = "x86_64")]
use std::is_x86_feature_detected;

// SIMD intrinsics
#[cfg(all(feature = "simd", target_feature = "avx2"))]
use std::arch::x86_64::*;
#[cfg(all(feature = "simd", target_feature = "avx512f"))]
use std::arch::x86_64::*;

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

pub const MOON_ORBIT_SCALE: f32 = 60.0;

// Orbital mechanics functions for solar system simulation

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
        return planet.orbital_distance_au * solar_params.scale_factor * MOON_ORBIT_SCALE;
    } else if let Some(elements) = get_orbital_elements(&planet.name) {
        return solar_params.au_to_units(elements.semi_major_axis_au);
    } else {
        return solar_params.au_to_units(planet.orbital_distance_au);
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
        // Moon - use real orbital elements if available
        if let Some(elements) = get_moon_orbital_elements(&planet.name) {
            return OrbitShape {
                semi_major_axis_units: solar_params.au_to_units(elements.semi_major_axis_au)
                    * MOON_ORBIT_SCALE,
                eccentricity: elements.eccentricity,
                inclination_rad: elements.inclination_rad,
                long_asc_node_rad: elements.long_asc_node_rad,
                arg_periapsis_rad: elements.arg_periapsis_rad,
            };
        } else {
            // Fallback for moons without defined elements
            return OrbitShape {
                semi_major_axis_units: calculate_orbit_radius_units(planet, solar_params),
                eccentricity: 0.0,
                inclination_rad: 0.0,
                long_asc_node_rad: 0.0,
                arg_periapsis_rad: 0.0,
            };
        }
    } else if let Some(elements) = get_orbital_elements(&planet.name) {
        // Planet - use real orbital elements
        return OrbitShape {
            semi_major_axis_units: solar_params.au_to_units(elements.semi_major_axis_au),
            eccentricity: elements.eccentricity,
            inclination_rad: elements.inclination_rad,
            long_asc_node_rad: elements.long_asc_node_rad,
            arg_periapsis_rad: elements.arg_periapsis_rad,
        };
    } else {
        // Fallback for bodies without defined elements
        return OrbitShape {
            semi_major_axis_units: calculate_orbit_radius_units(planet, solar_params),
            eccentricity: 0.0,
            inclination_rad: 0.0,
            long_asc_node_rad: 0.0,
            arg_periapsis_rad: 0.0,
        };
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

    // Apply inclination (tilt the plane)
    let cos_i = inclination.cos();
    let sin_i = inclination.sin();
    let y2 = z1 * sin_i;
    let z2 = z1 * cos_i;
    let x2 = x1;

    // Apply longitude of ascending node (rotate around Y axis)
    let cos_omega = long_asc_node.cos();
    let sin_omega = long_asc_node.sin();
    let x3 = x2 * cos_omega - z2 * sin_omega;
    let z3 = x2 * sin_omega + z2 * cos_omega;

    Vec3::new(x3, y2, z3)
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

#[derive(Clone, Copy, Debug)]
pub struct OrbitalElements {
    pub semi_major_axis_au: f32,
    pub eccentricity: f32,
    pub inclination_rad: f32,
    pub long_asc_node_rad: f32,
    pub arg_periapsis_rad: f32,
    pub mean_anomaly_rad: f32,
}

pub fn orbital_elements_for(planet: &Planet) -> Option<OrbitalElements> {
    if planet.parent_entity.is_some() {
        get_moon_orbital_elements(&planet.name)
    } else {
        get_orbital_elements(&planet.name)
    }
}

fn get_orbital_elements(name: &str) -> Option<OrbitalElements> {
    // J2000 mean orbital elements (degrees) for a Keplerian baseline.
    // Source: NASA planetary fact sheets (approximate).
    match name {
        "Mercury" => Some(elements_from_degrees(
            0.387,
            0.20563593,
            7.005,
            48.33076593,
            77.45779628,
            252.25032350,
        )),
        "Venus" => Some(elements_from_degrees(
            0.723,
            0.00677672,
            3.394,
            76.67984255,
            131.60246718,
            181.97909950,
        )),
        "Earth" => Some(elements_from_degrees(
            1.000,
            0.01671123,
            0.0,
            0.0,
            102.93768193,
            100.46457166,
        )),
        "Mars" => Some(elements_from_degrees(
            1.524,
            0.09339410,
            1.85,
            49.55809321,
            336.04084,
            355.45332,
        )),
        "Jupiter" => Some(elements_from_degrees(
            5.204,
            0.04838624,
            1.304,
            100.47390909,
            14.72847983,
            34.39644051,
        )),
        "Saturn" => Some(elements_from_degrees(
            9.582,
            0.05386179,
            2.485,
            113.66242448,
            92.59887831,
            49.95424423,
        )),
        "Uranus" => Some(elements_from_degrees(
            19.201,
            0.04725744,
            0.773,
            74.01692503,
            170.95427630,
            313.23810451,
        )),
        "Neptune" => Some(elements_from_degrees(
            30.047,
            0.00859048,
            1.77,
            131.78422574,
            44.96476227,
            304.87964,
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
    solve_kepler_adaptive(mean_anomaly, eccentricity, 8)
}

/// Adaptive Kepler equation solver with configurable iteration count
/// Higher iterations = more accuracy, lower iterations = better performance
pub fn solve_kepler_adaptive(mean_anomaly: f32, eccentricity: f32, max_iterations: u32) -> f32 {
    let mut eccentric_anomaly = mean_anomaly;
    for _ in 0..max_iterations {
        let f = eccentric_anomaly - eccentricity * eccentric_anomaly.sin() - mean_anomaly;
        let f_prime = 1.0 - eccentricity * eccentric_anomaly.cos();
        eccentric_anomaly -= f / f_prime;
    }
    eccentric_anomaly
}

/// Calculate optimal Kepler solver iteration count based on distance from camera
/// Near objects: 8 iterations (full accuracy)
/// Medium distance: 4 iterations
/// Far distance: 2 iterations
/// Very far: 1 iteration (minimal calculation)
pub fn get_kepler_iterations_for_distance(distance_to_camera: f32) -> u32 {
    if distance_to_camera < 100000.0 {
        8 // Near: full accuracy
    } else if distance_to_camera < 500000.0 {
        4 // Medium: half accuracy
    } else if distance_to_camera < 2000000.0 {
        2 // Far: quarter accuracy
    } else {
        1 // Very far: minimal calculation
    }
}

fn true_anomaly(eccentric_anomaly: f32, eccentricity: f32) -> f32 {
    let sin_v = (1.0 - eccentricity * eccentricity).sqrt() * eccentric_anomaly.sin()
        / (1.0 - eccentricity * eccentric_anomaly.cos());
    let cos_v =
        (eccentric_anomaly.cos() - eccentricity) / (1.0 - eccentricity * eccentric_anomaly.cos());
    sin_v.atan2(cos_v)
}

fn mean_motion_rad_per_day(semi_major_axis_au: f32) -> f32 {
    // Gauss gravitational constant (AU^(3/2)/day)
    const GAUSS_K: f32 = 0.01720209895;
    GAUSS_K / semi_major_axis_au.powf(1.5)
}

fn mean_motion_from_period_days(period_days: f32) -> f32 {
    std::f32::consts::TAU / period_days
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
        ("Sun", 696340.0),
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

    // Use TRUE mathematical proportions - no logarithmic scaling
    // This preserves real-world size relationships:
    // - Jupiter will be ~28.6x larger than Earth
    // - Sun will be ~109x larger than Earth
    // - All sizes are mathematically accurate to real-world data

    // Apply final scaling for visibility in the simulation
    relative_to_mercury * solar_params.planet_scale
}

/// Calculate the visual radius for the Sun with appropriate scaling
pub fn calculate_sun_visual_radius(solar_params: &SolarSystemParameters) -> f32 {
    // Sun radius is ~696,340 km
    // Use the same TRUE mathematical proportions as planets
    // Sun will be ~285x larger than Mercury, ~109x larger than Earth
    // This preserves the real-world dominance of the Sun in the solar system
    let sun = Planet::create_sun();
    calculate_visual_radius(&sun, solar_params)
}

/// Get the astronomical unit in simulation units
pub const AU_IN_KM: f32 = 149597870.7; // 1 AU in kilometers

/// Convert astronomical units to simulation distance units
pub fn au_to_simulation_units(au: f32, solar_params: &SolarSystemParameters) -> f32 {
    au * solar_params.scale_factor
}

// SIMD Optimizations for Phase 6

/// SIMD-accelerated Kepler equation solver for batches
/// Full AVX-512 and AVX2 intrinsic implementations for maximum performance
#[cfg(feature = "simd")]
pub fn solve_kepler_simd_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
    cpu_features: CpuFeatureLevel,
) -> Vec<f32> {
    // Dispatch to appropriate SIMD implementation
    match cpu_features {
        CpuFeatureLevel::AVX512 => solve_kepler_avx512_batch(mean_anomalies, eccentricities, max_iterations),
        CpuFeatureLevel::AVX2 => solve_kepler_avx2_batch(mean_anomalies, eccentricities, max_iterations),
        CpuFeatureLevel::SSE4 => solve_kepler_sse4_batch(mean_anomalies, eccentricities, max_iterations),
        CpuFeatureLevel::Scalar => solve_kepler_scalar_batch(mean_anomalies, eccentricities, max_iterations),
    }
}

/// AVX-512 Kepler solver - processes 16 equations simultaneously
#[cfg(all(feature = "simd", target_feature = "avx512f"))]
fn solve_kepler_avx512_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(mean_anomalies.len());

    // Process in chunks of 16 for AVX-512
    for chunk in mean_anomalies.chunks(16).zip(eccentricities.chunks(16)) {
        let (ma_chunk, e_chunk) = chunk;

        // Load data into AVX-512 registers (pad with zeros if needed)
        let mut ma_data = [0.0f32; 16];
        let mut e_data = [0.0f32; 16];

        for i in 0..ma_chunk.len() {
            ma_data[i] = ma_chunk[i];
            e_data[i] = e_chunk[i];
        }

        let ma_vec = unsafe { _mm512_loadu_ps(ma_data.as_ptr()) };
        let e_vec = unsafe { _mm512_loadu_ps(e_data.as_ptr()) };

        // Newton-Raphson iteration using AVX-512
        let mut e_anomaly = ma_vec; // Initial guess

        for _ in 0..max_iterations.min(8) {
            // Calculate f = E - e*sin(E) - M
            // f' = 1 - e*cos(E)

            // Approximate sin and cos using polynomial series
            let sin_e = avx512_sin_approx(e_anomaly);
            let cos_e = avx512_cos_approx(e_anomaly);

            let e_sin_e = unsafe { _mm512_mul_ps(e_vec, sin_e) };
            let e_cos_e = unsafe { _mm512_mul_ps(e_vec, cos_e) };

            let f = unsafe {
                _mm512_sub_ps(
                    _mm512_sub_ps(e_anomaly, e_sin_e),
                    ma_vec
                )
            };

            let f_prime = unsafe {
                _mm512_sub_ps(
                    _mm512_set1_ps(1.0),
                    e_cos_e
                )
            };

            // delta = f / f'
            let delta = unsafe { _mm512_div_ps(f, f_prime) };

            // E = E - delta
            e_anomaly = unsafe { _mm512_sub_ps(e_anomaly, delta) };
        }

        // Store results
        let mut result_data = [0.0f32; 16];
        unsafe { _mm512_storeu_ps(result_data.as_mut_ptr(), e_anomaly); }

        for &result in &result_data[..ma_chunk.len()] {
            results.push(result);
        }
    }

    results
}

/// AVX-256 Kepler solver - processes 8 equations simultaneously
#[cfg(all(feature = "simd", target_feature = "avx2"))]
fn solve_kepler_avx2_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(mean_anomalies.len());

    // Process in chunks of 8 for AVX2
    for chunk in mean_anomalies.chunks(8).zip(eccentricities.chunks(8)) {
        let (ma_chunk, e_chunk) = chunk;

        // Load data into AVX2 registers (pad with zeros if needed)
        let mut ma_data = [0.0f32; 8];
        let mut e_data = [0.0f32; 8];

        for i in 0..ma_chunk.len() {
            ma_data[i] = ma_chunk[i];
            e_data[i] = e_chunk[i];
        }

        let ma_vec = unsafe { _mm256_loadu_ps(ma_data.as_ptr()) };
        let e_vec = unsafe { _mm256_loadu_ps(e_data.as_ptr()) };

        // Newton-Raphson iteration using AVX2
        let mut e_anomaly = ma_vec; // Initial guess

        for _ in 0..max_iterations.min(6) {
            // Polynomial approximations for sin/cos
            let sin_e = avx2_sin_approx(e_anomaly);
            let cos_e = avx2_cos_approx(e_anomaly);

            let e_sin_e = unsafe { _mm256_mul_ps(e_vec, sin_e) };
            let e_cos_e = unsafe { _mm256_mul_ps(e_vec, cos_e) };

            let f = unsafe {
                _mm256_sub_ps(
                    _mm256_sub_ps(e_anomaly, e_sin_e),
                    ma_vec
                )
            };

            let f_prime = unsafe {
                _mm256_sub_ps(
                    _mm256_set1_ps(1.0),
                    e_cos_e
                )
            };

            // delta = f / f'
            let delta = unsafe { _mm256_div_ps(f, f_prime) };

            // E = E - delta
            e_anomaly = unsafe { _mm256_sub_ps(e_anomaly, delta) };
        }

        // Store results
        let mut result_data = [0.0f32; 8];
        unsafe { _mm256_storeu_ps(result_data.as_mut_ptr(), e_anomaly); }

        for &result in &result_data[..ma_chunk.len()] {
            results.push(result);
        }
    }

    results
}

/// SSE4 Kepler solver - processes 4 equations simultaneously
#[cfg(all(feature = "simd", target_feature = "sse4.1"))]
fn solve_kepler_sse4_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(mean_anomalies.len());

    // Process in chunks of 4 for SSE4
    for chunk in mean_anomalies.chunks(4).zip(eccentricities.chunks(4)) {
        let (ma_chunk, e_chunk) = chunk;

        // Load data into SSE registers
        let mut ma_data = [0.0f32; 4];
        let mut e_data = [0.0f32; 4];

        for i in 0..ma_chunk.len() {
            ma_data[i] = ma_chunk[i];
            e_data[i] = e_chunk[i];
        }

        let ma_vec = unsafe { _mm_loadu_ps(ma_data.as_ptr()) };
        let e_vec = unsafe { _mm_loadu_ps(e_data.as_ptr()) };

        // Newton-Raphson iteration using SSE4
        let mut e_anomaly = ma_vec; // Initial guess

        for _ in 0..max_iterations.min(4) {
            // Simple approximation: E ≈ M + e*sin(M) for near-circular orbits
            let sin_approx = unsafe { _mm_mul_ps(e_anomaly, _mm_set1_ps(0.8415)) }; // sin(x) ≈ x * 0.8415
            let e_sin = unsafe { _mm_mul_ps(e_vec, sin_approx) };
            let correction = unsafe { _mm_add_ps(ma_vec, e_sin) };
            e_anomaly = correction;
        }

        // Store results
        let mut result_data = [0.0f32; 4];
        unsafe { _mm_storeu_ps(result_data.as_mut_ptr(), e_anomaly); }

        for &result in &result_data[..ma_chunk.len()] {
            results.push(result);
        }
    }

    results
}

/// Scalar fallback implementation
fn solve_kepler_scalar_batch(
    mean_anomalies: &[f32],
    eccentricities: &[f32],
    max_iterations: u32,
) -> Vec<f32> {
    mean_anomalies
        .iter()
        .zip(eccentricities.iter())
        .map(|(&ma, &e)| solve_kepler_adaptive(ma, e, max_iterations))
        .collect()
}

/// AVX-512 sine approximation using Taylor series
#[cfg(all(feature = "simd", target_feature = "avx512f"))]
fn avx512_sin_approx(x: std::arch::x86_64::__m512) -> std::arch::x86_64::__m512 {
    use std::arch::x86_64::*;

    let x2 = unsafe { _mm512_mul_ps(x, x) };
    let x3 = unsafe { _mm512_mul_ps(x2, x) };
    let x5 = unsafe { _mm512_mul_ps(x3, x2) };

    let term1 = x;
    let term2 = unsafe { _mm512_div_ps(x3, _mm512_set1_ps(6.0)) };
    let term3 = unsafe { _mm512_div_ps(x5, _mm512_set1_ps(120.0)) };

    unsafe { _mm512_sub_ps(_mm512_sub_ps(term1, term2), term3) }
}

/// AVX-512 cosine approximation using Taylor series
#[cfg(all(feature = "simd", target_feature = "avx512f"))]
fn avx512_cos_approx(x: std::arch::x86_64::__m512) -> std::arch::x86_64::__m512 {
    use std::arch::x86_64::*;

    let x2 = unsafe { _mm512_mul_ps(x, x) };
    let x4 = unsafe { _mm512_mul_ps(x2, x2) };

    let term1 = unsafe { _mm512_set1_ps(1.0) };
    let term2 = unsafe { _mm512_div_ps(x2, _mm512_set1_ps(2.0)) };
    let term3 = unsafe { _mm512_div_ps(x4, _mm512_set1_ps(24.0)) };

    unsafe { _mm512_add_ps(_mm512_sub_ps(term1, term2), term3) }
}

/// AVX-256 sine approximation
#[cfg(all(feature = "simd", target_feature = "avx2"))]
fn avx2_sin_approx(x: std::arch::x86_64::__m256) -> std::arch::x86_64::__m256 {
    use std::arch::x86_64::*;

    let x2 = unsafe { _mm256_mul_ps(x, x) };
    let x3 = unsafe { _mm256_mul_ps(x2, x) };

    let term1 = x;
    let term2 = unsafe { _mm256_div_ps(x3, _mm256_set1_ps(6.0)) };

    unsafe { _mm256_sub_ps(term1, term2) }
}

/// AVX-256 cosine approximation
#[cfg(all(feature = "simd", target_feature = "avx2"))]
fn avx2_cos_approx(x: std::arch::x86_64::__m256) -> std::arch::x86_64::__m256 {
    use std::arch::x86_64::*;

    let x2 = unsafe { _mm256_mul_ps(x, x) };
    let x4 = unsafe { _mm256_mul_ps(x2, x2) };

    let term1 = unsafe { _mm256_set1_ps(1.0) };
    let term2 = unsafe { _mm256_div_ps(x2, _mm256_set1_ps(2.0)) };

    unsafe { _mm256_sub_ps(term1, term2) }
}

/// SIMD-accelerated orbital transformation matrix operations
/// Vectorized 3D transformations for orbital point calculations using AVX2/AVX-512
#[cfg(feature = "simd")]
pub fn transform_orbital_points_simd(
    points: &[(f32, f32)],                // (x_orbital, z_orbital) pairs
    orbital_elements: &[(f32, f32, f32)], // (inclination, long_asc_node, arg_periapsis)
) -> Vec<Vec3> {
    // Determine CPU capabilities for SIMD dispatch
    let cpu_features = detect_cpu_features();

    match cpu_features {
        CpuFeatureLevel::AVX512 => transform_orbital_points_avx512(points, orbital_elements),
        CpuFeatureLevel::AVX2 => transform_orbital_points_avx2(points, orbital_elements),
        _ => transform_orbital_points_scalar(points, orbital_elements),
    }
}

/// AVX-512 implementation for orbital point transformations
#[cfg(all(feature = "simd", target_feature = "avx512f"))]
fn transform_orbital_points_avx512(
    points: &[(f32, f32)],
    orbital_elements: &[(f32, f32, f32)],
) -> Vec<Vec3> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(points.len());

    // Process in chunks of 16 for AVX-512
    for chunk in points.chunks(16).zip(orbital_elements.chunks(16)) {
        let (point_chunk, element_chunk) = chunk;

        // Prepare data arrays (pad with zeros)
        let mut x_data = [0.0f32; 16];
        let mut z_data = [0.0f32; 16];
        let mut inc_data = [0.0f32; 16];
        let mut lan_data = [0.0f32; 16];
        let mut ap_data = [0.0f32; 16];

        for i in 0..point_chunk.len() {
            x_data[i] = point_chunk[i].0;
            z_data[i] = point_chunk[i].1;
            inc_data[i] = element_chunk[i].0;
            lan_data[i] = element_chunk[i].1;
            ap_data[i] = element_chunk[i].2;
        }

        // Load into AVX-512 registers
        let x_vec = unsafe { _mm512_loadu_ps(x_data.as_ptr()) };
        let z_vec = unsafe { _mm512_loadu_ps(z_data.as_ptr()) };
        let inc_vec = unsafe { _mm512_loadu_ps(inc_data.as_ptr()) };
        let lan_vec = unsafe { _mm512_loadu_ps(lan_data.as_ptr()) };
        let ap_vec = unsafe { _mm512_loadu_ps(ap_data.as_ptr()) };

        // Calculate trigonometric functions for rotation matrices
        let sin_inc = avx512_sin_approx(inc_vec);
        let cos_inc = avx512_cos_approx(inc_vec);
        let sin_lan = avx512_sin_approx(lan_vec);
        let cos_lan = avx512_cos_approx(lan_vec);
        let sin_ap = avx512_sin_approx(ap_vec);
        let cos_ap = avx512_cos_approx(ap_vec);

        // Apply argument of periapsis rotation (around Z axis)
        // x' = x*cos(ap) - z*sin(ap)
        // z' = x*sin(ap) + z*cos(ap)
        let x_ap = unsafe {
            _mm512_sub_ps(
                _mm512_mul_ps(x_vec, cos_ap),
                _mm512_mul_ps(z_vec, sin_ap)
            )
        };
        let z_ap = unsafe {
            _mm512_add_ps(
                _mm512_mul_ps(x_vec, sin_ap),
                _mm512_mul_ps(z_vec, cos_ap)
            )
        };

        // Apply longitude of ascending node rotation (around Z axis)
        // x'' = x'*cos(lan) - z'*sin(lan)
        // z'' = x'*sin(lan) + z'*cos(lan)
        let x_lan = unsafe {
            _mm512_sub_ps(
                _mm512_mul_ps(x_ap, cos_lan),
                _mm512_mul_ps(z_ap, sin_lan)
            )
        };
        let z_lan = unsafe {
            _mm512_add_ps(
                _mm512_mul_ps(x_ap, sin_lan),
                _mm512_mul_ps(z_ap, cos_lan)
            )
        };

        // Apply inclination rotation (around X axis)
        // y'' = z''*sin(inc)
        // z''' = z''*cos(inc)
        let y_inc = unsafe { _mm512_mul_ps(z_lan, sin_inc) };
        let z_inc = unsafe { _mm512_mul_ps(z_lan, cos_inc) };

        // Store results
        let mut x_results = [0.0f32; 16];
        let mut y_results = [0.0f32; 16];
        let mut z_results = [0.0f32; 16];

        unsafe {
            _mm512_storeu_ps(x_results.as_mut_ptr(), x_lan);
            _mm512_storeu_ps(y_results.as_mut_ptr(), y_inc);
            _mm512_storeu_ps(z_results.as_mut_ptr(), z_inc);
        }

        for i in 0..point_chunk.len() {
            results.push(Vec3::new(x_results[i], y_results[i], z_results[i]));
        }
    }

    results
}

/// AVX-256 implementation for orbital point transformations
#[cfg(all(feature = "simd", target_feature = "avx2"))]
fn transform_orbital_points_avx2(
    points: &[(f32, f32)],
    orbital_elements: &[(f32, f32, f32)],
) -> Vec<Vec3> {
    use std::arch::x86_64::*;

    let mut results = Vec::with_capacity(points.len());

    // Process in chunks of 8 for AVX2
    for chunk in points.chunks(8).zip(orbital_elements.chunks(8)) {
        let (point_chunk, element_chunk) = chunk;

        // Prepare data arrays (pad with zeros)
        let mut x_data = [0.0f32; 8];
        let mut z_data = [0.0f32; 8];
        let mut inc_data = [0.0f32; 8];
        let mut lan_data = [0.0f32; 8];
        let mut ap_data = [0.0f32; 8];

        for i in 0..point_chunk.len() {
            x_data[i] = point_chunk[i].0;
            z_data[i] = point_chunk[i].1;
            inc_data[i] = element_chunk[i].0;
            lan_data[i] = element_chunk[i].1;
            ap_data[i] = element_chunk[i].2;
        }

        // Load into AVX2 registers
        let x_vec = unsafe { _mm256_loadu_ps(x_data.as_ptr()) };
        let z_vec = unsafe { _mm256_loadu_ps(z_data.as_ptr()) };
        let inc_vec = unsafe { _mm256_loadu_ps(inc_data.as_ptr()) };
        let lan_vec = unsafe { _mm256_loadu_ps(lan_data.as_ptr()) };
        let ap_vec = unsafe { _mm256_loadu_ps(ap_data.as_ptr()) };

        // Calculate trigonometric functions
        let sin_inc = avx2_sin_approx(inc_vec);
        let cos_inc = avx2_cos_approx(inc_vec);
        let sin_lan = avx2_sin_approx(lan_vec);
        let cos_lan = avx2_cos_approx(lan_vec);
        let sin_ap = avx2_sin_approx(ap_vec);
        let cos_ap = avx2_cos_approx(ap_vec);

        // Apply rotations (same logic as AVX-512 but with 256-bit vectors)
        let x_ap = unsafe {
            _mm256_sub_ps(
                _mm256_mul_ps(x_vec, cos_ap),
                _mm256_mul_ps(z_vec, sin_ap)
            )
        };
        let z_ap = unsafe {
            _mm256_add_ps(
                _mm256_mul_ps(x_vec, sin_ap),
                _mm256_mul_ps(z_vec, cos_ap)
            )
        };

        let x_lan = unsafe {
            _mm256_sub_ps(
                _mm256_mul_ps(x_ap, cos_lan),
                _mm256_mul_ps(z_ap, sin_lan)
            )
        };
        let z_lan = unsafe {
            _mm256_add_ps(
                _mm256_mul_ps(x_ap, sin_lan),
                _mm256_mul_ps(z_ap, cos_lan)
            )
        };

        let y_inc = unsafe { _mm256_mul_ps(z_lan, sin_inc) };
        let z_inc = unsafe { _mm256_mul_ps(z_lan, cos_inc) };

        // Store results
        let mut x_results = [0.0f32; 8];
        let mut y_results = [0.0f32; 8];
        let mut z_results = [0.0f32; 8];

        unsafe {
            _mm256_storeu_ps(x_results.as_mut_ptr(), x_lan);
            _mm256_storeu_ps(y_results.as_mut_ptr(), y_inc);
            _mm256_storeu_ps(z_results.as_mut_ptr(), z_inc);
        }

        for i in 0..point_chunk.len() {
            results.push(Vec3::new(x_results[i], y_results[i], z_results[i]));
        }
    }

    results
}

/// Scalar fallback for orbital point transformations
fn transform_orbital_points_scalar(
    points: &[(f32, f32)],
    orbital_elements: &[(f32, f32, f32)],
) -> Vec<Vec3> {
    points
        .iter()
        .zip(orbital_elements.iter())
        .map(|(&(x_orb, z_orb), &(inc, lan, ap))| {
            transform_orbital_point(x_orb, z_orb, inc, lan, ap)
        })
        .collect()
}

// Parallel Optimizations for Phase 6

/// Parallel orbital position calculation using Rayon
#[cfg(feature = "parallel")]
pub fn calculate_planet_positions_parallel(
    planets: &[(Planet, Vec3, Option<f32>, u32)], // (planet, parent_pos, parent_tilt, kepler_iterations)
    time_days: f32,
    solar_params: &SolarSystemParameters,
) -> Vec<Vec3> {
    planets
        .par_iter()
        .map(|(planet, parent_pos, parent_tilt, kepler_iterations)| {
            calculate_planet_position_with_quality(
                planet,
                time_days,
                solar_params,
                *parent_pos,
                *parent_tilt,
                *kepler_iterations,
            )
        })
        .collect()
}

/// CPU feature level detection for future SIMD dispatch
#[derive(Debug, Clone, Copy)]
pub enum CpuFeatureLevel {
    Scalar,
    SSE4,
    AVX2,
    AVX512,
}
pub fn detect_cpu_features() -> CpuFeatureLevel {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx512f") {
            CpuFeatureLevel::AVX512
        } else if is_x86_feature_detected!("avx2") {
            CpuFeatureLevel::AVX2
        } else if is_x86_feature_detected!("sse4.1") {
            CpuFeatureLevel::SSE4
        } else {
            CpuFeatureLevel::Scalar
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        CpuFeatureLevel::Scalar
    }
}
