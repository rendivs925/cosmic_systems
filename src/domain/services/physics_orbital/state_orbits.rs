//! Orbital elements derived from state vectors and the shared two-body
//! transfer/speed math used by guidance and flight systems.

use crate::domain::math::DVec3;

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

/// Nominal Earth orbit constraints evaluated from an authoritative f64 state
/// vector. Altitudes are above the bound body's reference radius in meters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LowEarthOrbitTarget {
    pub target_apoapsis_altitude_m: f64,
    pub target_periapsis_altitude_m: f64,
    pub altitude_tolerance_m: f64,
    pub maximum_eccentricity: f64,
    pub target_inclination_rad: f64,
    pub inclination_tolerance_rad: f64,
    pub minimum_safe_periapsis_altitude_m: f64,
}

impl Default for LowEarthOrbitTarget {
    fn default() -> Self {
        Self {
            target_apoapsis_altitude_m: 200_000.0,
            target_periapsis_altitude_m: 200_000.0,
            altitude_tolerance_m: 25_000.0,
            maximum_eccentricity: 0.02,
            target_inclination_rad: 28.5_f64.to_radians(),
            inclination_tolerance_rad: 2.0_f64.to_radians(),
            minimum_safe_periapsis_altitude_m: 160_000.0,
        }
    }
}

impl LowEarthOrbitTarget {
    /// Returns whether a bound osculating state satisfies the target's safety
    /// and geometry constraints. This never relies on cached ECS telemetry.
    pub fn matches_state(
        self,
        position_m: DVec3,
        velocity_mps: DVec3,
        mu: f64,
        body_radius_m: f64,
    ) -> bool {
        self.matches_state_in_reference_frame(
            position_m,
            velocity_mps,
            mu,
            body_radius_m,
            DVec3::Z,
            DVec3::X,
        )
    }

    /// Evaluate the target against elements measured in an explicit inertial
    /// reference plane. Planet-centered flight uses the body's equatorial
    /// plane, while solar-map elements retain the conventional +Z plane.
    pub fn matches_state_in_reference_frame(
        self,
        position_m: DVec3,
        velocity_mps: DVec3,
        mu: f64,
        body_radius_m: f64,
        reference_normal: DVec3,
        reference_x_axis: DVec3,
    ) -> bool {
        if !self.is_valid() || !body_radius_m.is_finite() || body_radius_m <= 0.0 {
            return false;
        }

        let Some(specific_energy) = specific_orbital_energy(position_m, velocity_mps, mu) else {
            return false;
        };
        if specific_energy >= 0.0 {
            return false;
        }

        let elements = orbital_elements_from_state_in_reference_frame(
            position_m,
            velocity_mps,
            mu,
            reference_normal,
            reference_x_axis,
        );
        let apoapsis_altitude_m = elements.apoapsis_m - body_radius_m;
        let periapsis_altitude_m = elements.periapsis_m - body_radius_m;
        if !apoapsis_altitude_m.is_finite()
            || !periapsis_altitude_m.is_finite()
            || !elements.eccentricity.is_finite()
            || !elements.inclination_rad.is_finite()
        {
            return false;
        }

        (apoapsis_altitude_m - self.target_apoapsis_altitude_m).abs() <= self.altitude_tolerance_m
            && (periapsis_altitude_m - self.target_periapsis_altitude_m).abs()
                <= self.altitude_tolerance_m
            && periapsis_altitude_m >= self.minimum_safe_periapsis_altitude_m
            && elements.eccentricity <= self.maximum_eccentricity
            && (elements.inclination_rad - self.target_inclination_rad).abs()
                <= self.inclination_tolerance_rad
    }

    fn is_valid(self) -> bool {
        self.target_apoapsis_altitude_m.is_finite()
            && self.target_periapsis_altitude_m.is_finite()
            && self.altitude_tolerance_m.is_finite()
            && self.maximum_eccentricity.is_finite()
            && self.target_inclination_rad.is_finite()
            && self.inclination_tolerance_rad.is_finite()
            && self.minimum_safe_periapsis_altitude_m.is_finite()
            && self.target_apoapsis_altitude_m >= self.target_periapsis_altitude_m
            && self.target_periapsis_altitude_m >= self.minimum_safe_periapsis_altitude_m
            && self.altitude_tolerance_m >= 0.0
            && (0.0..1.0).contains(&self.maximum_eccentricity)
            && (0.0..=std::f64::consts::PI).contains(&self.target_inclination_rad)
            && self.inclination_tolerance_rad >= 0.0
    }
}

/// Exact f64 positions of the apsides of a bound two-body orbit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ApsisEndpoints {
    pub apoapsis_position_m: DVec3,
    pub periapsis_position_m: DVec3,
}

/// Specific orbital energy in J/kg for a planet-centered inertial state.
pub fn specific_orbital_energy(position_m: DVec3, velocity_mps: DVec3, mu: f64) -> Option<f64> {
    let radius_m = position_m.length();
    if !radius_m.is_finite()
        || radius_m <= 0.0
        || !velocity_mps.is_finite()
        || !mu.is_finite()
        || mu <= 0.0
    {
        return None;
    }

    Some(velocity_mps.length_squared() * 0.5 - mu / radius_m)
}

/// Derive the exact apsis positions of a bound, non-circular osculating orbit.
/// The returned vectors remain in the caller's planet-centered inertial frame.
pub fn apsis_endpoints_from_state(
    position_m: DVec3,
    velocity_mps: DVec3,
    mu: f64,
) -> Option<ApsisEndpoints> {
    let radius_m = position_m.length();

    let angular_momentum = position_m.cross(velocity_mps);
    if angular_momentum.length_squared() <= f64::EPSILON {
        return None;
    }
    let specific_energy = specific_orbital_energy(position_m, velocity_mps, mu)?;
    if specific_energy >= 0.0 {
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
    orbital_elements_from_state_in_reference_frame(position_m, velocity_mps, mu, DVec3::Z, DVec3::X)
}

/// Compute osculating orbital elements in an explicit inertial reference
/// frame. `reference_normal` is the reference plane normal and
/// `reference_x_axis` defines zero longitude within that plane.
pub fn orbital_elements_from_state_in_reference_frame(
    position_m: DVec3,
    velocity_mps: DVec3,
    mu: f64,
    reference_normal: DVec3,
    reference_x_axis: DVec3,
) -> StateVectorOrbitalElements {
    let r = position_m;
    let v = velocity_mps;
    let reference_normal = reference_normal.normalize_or_zero();
    let reference_x_axis = (reference_x_axis
        - reference_normal * reference_x_axis.dot(reference_normal))
    .normalize_or_zero();

    // Specific angular momentum: h = r × v
    let h = r.cross(v);
    let h_mag = h.length();

    // Node vector: n = k × h, where k is the caller's reference-plane normal.
    let n = reference_normal.cross(h);
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

    // Inclination: i = acos(h·k / |h|)
    let inclination = if h_mag > 1e-12 {
        (h.dot(reference_normal) / h_mag).clamp(-1.0, 1.0).acos()
    } else {
        0.0
    };

    // Longitude of ascending node is measured from the supplied in-plane axis.
    let longitude_ascending_node = if n_mag > 1e-12 {
        positive_angle(reference_x_axis, n, reference_normal)
    } else {
        0.0
    };

    // Argument of periapsis is measured from the ascending node around h.
    let argument_of_periapsis = if n_mag > 1e-12 && eccentricity > 1e-12 {
        positive_angle(n, e_vec, h)
    } else {
        0.0
    };

    // True anomaly is measured from periapsis around h.
    let true_anomaly = if eccentricity > 1e-12 {
        positive_angle(e_vec, r, h)
    } else {
        // Circular orbit: use angle from ascending node
        if n_mag > 1e-12 {
            positive_angle(n, r, h)
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

fn positive_angle(from: DVec3, to: DVec3, normal: DVec3) -> f64 {
    let from = from.normalize_or_zero();
    let to = to.normalize_or_zero();
    let normal = normal.normalize_or_zero();
    if from == DVec3::ZERO || to == DVec3::ZERO || normal == DVec3::ZERO {
        return 0.0;
    }
    normal
        .dot(from.cross(to))
        .atan2(from.dot(to))
        .rem_euclid(std::f64::consts::TAU)
}

/// Vis-viva orbital speed on an ellipse with semi-major axis `a_m` at radius
/// `r_m`, m/s. The single authority for transfer speed math.
pub fn vis_viva_speed_mps(mu_m3_s2: f64, r_m: f64, a_m: f64) -> f64 {
    (mu_m3_s2 * (2.0 / r_m - 1.0 / a_m)).sqrt()
}

/// Circular-orbit speed at radius `r_m` around a body with gravitational
/// parameter `mu_m3_s2`, m/s.
pub fn circular_speed_mps(mu_m3_s2: f64, r_m: f64) -> f64 {
    (mu_m3_s2 / r_m).sqrt()
}

/// Circularize burn delta-v at current altitude to achieve circular orbit.
/// Returns the prograde delta-v required and the target circular orbit radius.
pub fn circularize_burn_dv(position_m: DVec3, velocity_mps: DVec3, mu: f64) -> (f64, f64) {
    let r = position_m.length();
    let v = velocity_mps.length();
    let dv = (circular_speed_mps(mu, r) - v).max(0.0);
    (dv, r)
}

/// Hohmann transfer delta-v from current circular orbit at r1 to target circular orbit at r2.
/// Returns (delta_v1, delta_v2) for the two burns.
pub fn hohmann_transfer_dv(r1: f64, r2: f64, mu: f64) -> (f64, f64) {
    let a_transfer = (r1 + r2) / 2.0;
    let dv1 = (vis_viva_speed_mps(mu, r1, a_transfer) - circular_speed_mps(mu, r1)).abs();
    let dv2 = (circular_speed_mps(mu, r2) - vis_viva_speed_mps(mu, r2, a_transfer)).abs();
    (dv1, dv2)
}

/// Plane change delta-v given current velocity and desired inclination change.
pub fn plane_change_dv(velocity_mps: f64, inclination_change_rad: f64) -> f64 {
    2.0 * velocity_mps * (inclination_change_rad / 2.0).sin()
}
