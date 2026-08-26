use crate::domain::entities::planet::Planet;
use crate::domain::services::physics_kepler::solve_kepler_adaptive;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::math::{DVec3, Quat, Vec3};

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
        None,       // No parent for Earth
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

/// Orbital elements computed from state vectors in planet-centered inertial frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateVectorOrbitalElements {
    pub semi_major_axis_m: f64,
    pub eccentricity: f64,
    pub inclination_rad: f64,
    pub longitude_ascending_node_rad: f64,
    pub argument_of_periapsis_rad: f64,
    pub true_anomaly_rad: f64,
    pub mean_anomaly_rad: f64,
    pub orbital_period_s: f64,
    pub apoapsis_m: f64,
    pub periapsis_m: f64,
}

/// Below this eccentricity, an ellipse has no useful unique apsis direction.
pub const APSIS_ECCENTRICITY_EPSILON: f64 = 1e-6;

/// Exact f64 positions of the apsides of a bound two-body orbit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ApsisEndpoints {
    pub apoapsis_position_m: DVec3,
    pub periapsis_position_m: DVec3,
}

/// Derive the exact apsis positions of a bound, non-circular osculating orbit.
/// The returned vectors remain in the caller's planet-centered inertial frame.
pub fn apsis_endpoints_from_state(
    position_m: DVec3,
    velocity_mps: DVec3,
    mu: f64,
) -> Option<ApsisEndpoints> {
    let radius_m = position_m.length();
    if !radius_m.is_finite() || !velocity_mps.is_finite() || !mu.is_finite() || mu <= 0.0 {
        return None;
    }

    let angular_momentum = position_m.cross(velocity_mps);
    if angular_momentum.length_squared() <= f64::EPSILON {
        return None;
    }
    let specific_energy = velocity_mps.length_squared() * 0.5 - mu / radius_m;
    if !specific_energy.is_finite() || specific_energy >= 0.0 {
        return None;
    }

    let eccentricity_vector = velocity_mps.cross(angular_momentum) / mu - position_m / radius_m;
    let eccentricity = eccentricity_vector.length();
    if !eccentricity.is_finite() || eccentricity <= APSIS_ECCENTRICITY_EPSILON {
        return None;
    }

    let semi_major_axis_m = -mu / (2.0 * specific_energy);
    if !semi_major_axis_m.is_finite() || semi_major_axis_m <= 0.0 {
        return None;
    }
    let periapsis_radius_m = semi_major_axis_m * (1.0 - eccentricity);
    let apoapsis_radius_m = semi_major_axis_m * (1.0 + eccentricity);
    if periapsis_radius_m <= 0.0 || !apoapsis_radius_m.is_finite() {
        return None;
    }

    let periapsis_direction = eccentricity_vector / eccentricity;
    Some(ApsisEndpoints {
        apoapsis_position_m: -periapsis_direction * apoapsis_radius_m,
        periapsis_position_m: periapsis_direction * periapsis_radius_m,
    })
}

/// Compute orbital elements from position and velocity vectors in a planet-centered
/// inertial frame. Uses the standard gravitational parameter μ = G·M.
pub fn orbital_elements_from_state(
    position_m: DVec3,
    velocity_mps: DVec3,
    mu: f64,
) -> StateVectorOrbitalElements {
    let r = position_m;
    let v = velocity_mps;

    // Specific angular momentum: h = r × v
    let h = r.cross(v);
    let h_mag = h.length();

    // Node vector: n = k × h (k = [0, 0, 1] for equatorial plane reference)
    let n = DVec3::new(-h.y, h.x, 0.0);
    let n_mag = n.length();

    // Specific orbital energy: ε = v²/2 - μ/r
    let r_mag = r.length();
    let v_sq = v.length_squared();
    let energy = v_sq / 2.0 - mu / r_mag;

    // Semi-major axis: a = -μ / (2ε)
    let semi_major_axis = if energy.abs() > 1e-12 {
        -mu / (2.0 * energy)
    } else {
        f64::INFINITY // Parabolic orbit
    };

    // Eccentricity vector: e = (v × h)/μ - r/|r|
    let e_vec = v.cross(h) / mu - r / r_mag;
    let eccentricity = e_vec.length();

    // Inclination: i = acos(h_z / |h|)
    let inclination = if h_mag > 1e-12 {
        (h.z / h_mag).acos()
    } else {
        0.0
    };

    // Longitude of ascending node: Ω = atan2(n_x, -n_y) or acos(n_x/|n|) with quadrant check
    let longitude_ascending_node = if n_mag > 1e-12 {
        let cos_omega = n.x / n_mag;
        let omega = cos_omega.clamp(-1.0, 1.0).acos();
        if n.y >= 0.0 {
            omega
        } else {
            2.0 * std::f64::consts::PI - omega
        }
    } else {
        0.0
    };

    // Argument of periapsis: ω = acos(n·e / (|n||e|)) with quadrant check
    let argument_of_periapsis = if n_mag > 1e-12 && eccentricity > 1e-12 {
        let cos_w = n.dot(e_vec) / (n_mag * eccentricity);
        let w = cos_w.clamp(-1.0, 1.0).acos();
        if e_vec.z >= 0.0 {
            w
        } else {
            2.0 * std::f64::consts::PI - w
        }
    } else {
        0.0
    };

    // True anomaly: ν = acos(e·r / (|e||r|)) with quadrant check
    let true_anomaly = if eccentricity > 1e-12 {
        let cos_nu = e_vec.dot(r) / (eccentricity * r_mag);
        let nu = cos_nu.clamp(-1.0, 1.0).acos();
        if r.dot(v) >= 0.0 {
            nu
        } else {
            2.0 * std::f64::consts::PI - nu
        }
    } else {
        // Circular orbit: use angle from ascending node
        if n_mag > 1e-12 {
            let cos_lat = r.dot(n) / (r_mag * n_mag);
            let lat = cos_lat.clamp(-1.0, 1.0).acos();
            if r.z >= 0.0 {
                lat
            } else {
                2.0 * std::f64::consts::PI - lat
            }
        } else {
            0.0
        }
    };

    // Mean anomaly via eccentric anomaly
    let mean_anomaly = if eccentricity < 1.0 {
        let cos_e = (eccentricity + true_anomaly.cos()) / (1.0 + eccentricity * true_anomaly.cos());
        let eccentric_anomaly = cos_e.clamp(-1.0, 1.0).acos();
        if true_anomaly > std::f64::consts::PI {
            2.0 * std::f64::consts::PI
                - (eccentric_anomaly - eccentricity * eccentric_anomaly.sin())
        } else {
            eccentric_anomaly - eccentricity * eccentric_anomaly.sin()
        }
    } else {
        // For parabolic/hyperbolic, use true anomaly directly
        true_anomaly
    };

    // Orbital period: T = 2π√(a³/μ)
    let orbital_period = if semi_major_axis.is_finite() && semi_major_axis > 0.0 {
        2.0 * std::f64::consts::PI * (semi_major_axis.powi(3) / mu).sqrt()
    } else {
        f64::INFINITY
    };

    // Apoapsis and periapsis
    let apoapsis = if semi_major_axis.is_finite() && semi_major_axis > 0.0 {
        semi_major_axis * (1.0 + eccentricity)
    } else {
        f64::INFINITY
    };

    let periapsis = if semi_major_axis.is_finite() && semi_major_axis > 0.0 {
        (semi_major_axis * (1.0 - eccentricity)).max(0.0)
    } else {
        r_mag
    };

    StateVectorOrbitalElements {
        semi_major_axis_m: semi_major_axis,
        eccentricity,
        inclination_rad: inclination,
        longitude_ascending_node_rad: longitude_ascending_node,
        argument_of_periapsis_rad: argument_of_periapsis,
        true_anomaly_rad: true_anomaly,
        mean_anomaly_rad: mean_anomaly,
        orbital_period_s: orbital_period,
        apoapsis_m: apoapsis,
        periapsis_m: periapsis,
    }
}

/// Circularize burn delta-v at current altitude to achieve circular orbit.
/// Returns the prograde delta-v required and the target circular orbit radius.
pub fn circularize_burn_dv(position_m: DVec3, velocity_mps: DVec3, mu: f64) -> (f64, f64) {
    let r = position_m.length();
    let v = velocity_mps.length();
    let v_circular = (mu / r).sqrt();
    let dv = (v_circular - v).max(0.0);
    (dv, r)
}

/// Hohmann transfer delta-v from current circular orbit at r1 to target circular orbit at r2.
/// Returns (delta_v1, delta_v2) for the two burns.
pub fn hohmann_transfer_dv(r1: f64, r2: f64, mu: f64) -> (f64, f64) {
    let a_transfer = (r1 + r2) / 2.0;
    let v1_circular = (mu / r1).sqrt();
    let v2_circular = (mu / r2).sqrt();
    let v1_transfer = (mu * (2.0 / r1 - 1.0 / a_transfer)).sqrt();
    let v2_transfer = (mu * (2.0 / r2 - 1.0 / a_transfer)).sqrt();
    let dv1 = (v1_transfer - v1_circular).abs();
    let dv2 = (v2_circular - v2_transfer).abs();
    (dv1, dv2)
}

/// Plane change delta-v given current velocity and desired inclination change.
pub fn plane_change_dv(velocity_mps: f64, inclination_change_rad: f64) -> f64 {
    2.0 * velocity_mps * (inclination_change_rad / 2.0).sin()
}

/// Combined circularize + plane change delta-v (more efficient than separate burns).
pub fn circularize_and_plane_change_dv(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_inclination_rad: f64,
    mu: f64,
) -> f64 {
    let elements = orbital_elements_from_state(position_m, velocity_mps, mu);
    let v = velocity_mps.length();
    let circular_dv = circularize_burn_dv(position_m, velocity_mps, mu).0;
    let plane_dv = plane_change_dv(v, (target_inclination_rad - elements.inclination_rad).abs());
    circular_dv + plane_dv
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::DVec3;

    const EARTH_MU: f64 = 3.986004418e14; // m^3/s^2

    #[test]
    fn circular_orbit_elements() {
        let r = 6_771_000.0; // 400 km altitude
        let v_circular = (EARTH_MU / r).sqrt();
        let pos = DVec3::new(r, 0.0, 0.0);
        let vel = DVec3::new(0.0, v_circular, 0.0);

        let elements = orbital_elements_from_state(pos, vel, EARTH_MU);

        assert!((elements.eccentricity - 0.0).abs() < 1e-6);
        assert!((elements.semi_major_axis_m - r).abs() < 1.0);
        assert!((elements.inclination_rad - 0.0).abs() < 1e-6);
        assert!((elements.apoapsis_m - r).abs() < 1.0);
        assert!((elements.periapsis_m - r).abs() < 1.0);
    }

    #[test]
    fn elliptical_orbit_elements() {
        let r_p = 6_671_000.0; // 300 km periapsis
        let r_a = 7_071_000.0; // 700 km apoapsis
        let a = (r_p + r_a) / 2.0;
        let v_p = (EARTH_MU * (2.0 / r_p - 1.0 / a)).sqrt(); // Periapsis velocity

        let pos = DVec3::new(r_p, 0.0, 0.0);
        let vel = DVec3::new(0.0, v_p, 0.0);

        let elements = orbital_elements_from_state(pos, vel, EARTH_MU);

        assert!((elements.eccentricity - (r_a - r_p) / (r_a + r_p)).abs() < 1e-4);
        assert!((elements.semi_major_axis_m - a).abs() < 10.0);
        assert!((elements.periapsis_m - r_p).abs() < 10.0);
        assert!((elements.apoapsis_m - r_a).abs() < 10.0);
    }

    #[test]
    fn analytic_apsides_match_the_eccentricity_vector() {
        let periapsis_m = 6_671_000.0;
        let apoapsis_m = 7_071_000.0;
        let semi_major_axis_m = (periapsis_m + apoapsis_m) * 0.5;
        let velocity_mps = (EARTH_MU * (2.0 / periapsis_m - 1.0 / semi_major_axis_m)).sqrt();
        let apsides = apsis_endpoints_from_state(
            DVec3::new(periapsis_m, 0.0, 0.0),
            DVec3::new(0.0, velocity_mps, 0.0),
            EARTH_MU,
        )
        .expect("elliptical orbit has unique apsides");

        assert!((apsides.periapsis_position_m.length() - periapsis_m).abs() < 1e-6);
        assert!((apsides.apoapsis_position_m.length() - apoapsis_m).abs() < 1e-6);
        assert!(apsides.periapsis_position_m.x > 0.0);
        assert!(apsides.apoapsis_position_m.x < 0.0);
    }

    #[test]
    fn analytic_apsides_preserve_an_arbitrary_orbital_orientation() {
        let periapsis_m = 6_671_000.0;
        let apoapsis_m = 7_071_000.0;
        let semi_major_axis_m = (periapsis_m + apoapsis_m) * 0.5;
        let velocity_mps = (EARTH_MU * (2.0 / periapsis_m - 1.0 / semi_major_axis_m)).sqrt();
        let rotation =
            bevy::math::DQuat::from_rotation_arc(DVec3::Y, DVec3::new(0.3, 0.8, 0.5).normalize());
        let apsides = apsis_endpoints_from_state(
            rotation * DVec3::new(periapsis_m, 0.0, 0.0),
            rotation * DVec3::new(0.0, velocity_mps, 0.0),
            EARTH_MU,
        )
        .expect("rotated ellipse has unique apsides");

        assert!((apsides.periapsis_position_m - rotation * DVec3::X * periapsis_m).length() < 1e-6);
        assert!((apsides.apoapsis_position_m + rotation * DVec3::X * apoapsis_m).length() < 1e-6);
    }

    #[test]
    fn circular_orbit_has_no_unique_analytic_apsides() {
        let radius_m = 6_771_000.0;
        let velocity_mps = (EARTH_MU / radius_m).sqrt();
        assert!(apsis_endpoints_from_state(
            DVec3::new(radius_m, 0.0, 0.0),
            DVec3::new(0.0, velocity_mps, 0.0),
            EARTH_MU,
        )
        .is_none());
    }

    #[test]
    fn inclined_orbit_elements() {
        let r = 6_771_000.0;
        let v_circular = (EARTH_MU / r).sqrt();
        let inclination = 28.5_f64.to_radians(); // KSC inclination

        let pos = DVec3::new(r, 0.0, 0.0);
        let vel = DVec3::new(
            0.0,
            v_circular * inclination.cos(),
            v_circular * inclination.sin(),
        );

        let elements = orbital_elements_from_state(pos, vel, EARTH_MU);

        assert!((elements.inclination_rad - inclination).abs() < 1e-4);
    }

    #[test]
    fn circularize_burn_positive_for_suborbital() {
        let r = 6_771_000.0;
        let v_suborbital = (EARTH_MU / r).sqrt() * 0.8; // 80% of circular
        let pos = DVec3::new(r, 0.0, 0.0);
        let vel = DVec3::new(0.0, v_suborbital, 0.0);

        let (dv, target_r) = circularize_burn_dv(pos, vel, EARTH_MU);

        assert!(dv > 0.0);
        assert!((target_r - r).abs() < 1.0);
    }

    #[test]
    fn hohmann_transfer_to_higher_orbit() {
        let r1 = 6_771_000.0; // 400 km
        let r2 = 42_164_000.0; // GEO
        let (dv1, dv2) = hohmann_transfer_dv(r1, r2, EARTH_MU);

        assert!(dv1 > 0.0);
        assert!(dv2 > 0.0);
        // Total ~3.9 km/s for LEO to GEO
        assert!((dv1 + dv2 - 3900.0).abs() < 100.0);
    }

    /// Regression pin: the period derived from state vectors must equal the
    /// analytic two-body result T = 2π√(a³/μ) for a circular orbit.
    #[test]
    fn circular_orbit_period_matches_analytic() {
        let r = 6_971_000.0; // 600 km altitude
        let v_circular = (EARTH_MU / r).sqrt();
        let pos = DVec3::new(r, 0.0, 0.0);
        let vel = DVec3::new(0.0, v_circular, 0.0);

        let elements = orbital_elements_from_state(pos, vel, EARTH_MU);
        let analytic_period = 2.0 * std::f64::consts::PI * (r.powi(3) / EARTH_MU).sqrt();

        assert!(
            (elements.orbital_period_s - analytic_period).abs() < analytic_period * 1e-6,
            "period from state {} s vs analytic {} s",
            elements.orbital_period_s,
            analytic_period
        );
        // Sanity: a 600 km LEO orbit is ~96.5 minutes (analytic value).
        assert!((analytic_period / 60.0 - 96.5).abs() < 0.5);
    }
}
