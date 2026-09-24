//! Orbital transfer and deorbit targeting guidance.

use crate::domain::math::{DQuat, DVec3};
use crate::domain::services::physics_orbital::{
    circular_speed_mps, hohmann_transfer_dv, vis_viva_speed_mps,
};

/// Deorbit burn delta-v (m/s) to lower periapsis to `target_periapsis_m`
/// from a circular orbit at `orbit_radius_m` around a body with `mu_m3_s2`.
/// Uses the vis-viva equation for a Hohmann transfer to an elliptical orbit
/// with the target periapsis.
pub fn deorbit_burn_dv(orbit_radius_m: f64, target_periapsis_m: f64, mu_m3_s2: f64) -> f64 {
    let r1 = orbit_radius_m;
    let r2 = target_periapsis_m;
    let a = (r1 + r2) / 2.0; // semi-major axis of transfer ellipse
    let v_circular = (mu_m3_s2 / r1).sqrt();
    let v_transfer = (mu_m3_s2 * (2.0 / r1 - 1.0 / a)).sqrt();
    (v_circular - v_transfer).max(0.0)
}

// ---------------------------------------------------------------------------
// Orbital transfers (Phase 15)
// ---------------------------------------------------------------------------

/// Orbit-radius ratio above which a sufficiently tall bi-elliptic transfer
/// beats the equivalent Hohmann (classical 11.94 boundary).
pub const BIELLIPTIC_FAVORABLE_RATIO: f64 = 11.94;

/// Result of a two-impulse Hohmann computation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransferSolution {
    /// Prograde Δv at the departure orbit, m/s.
    pub departure_dv_mps: f64,
    /// Prograde/retrograde Δv at arrival for circularization, m/s.
    pub arrival_dv_mps: f64,
    /// Half-period of the transfer ellipse — coast duration, s.
    pub transfer_time_s: f64,
}

impl TransferSolution {
    /// Total Δv budget of the two impulses, m/s.
    pub fn total_dv_mps(&self) -> f64 {
        self.departure_dv_mps + self.arrival_dv_mps
    }
}

/// Result of a three-impulse bi-elliptic computation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiellipticSolution {
    /// Δv raising apoapsis from r1 to rb, m/s.
    pub departure_dv_mps: f64,
    /// Δv at rb raising periapsis to r2, m/s.
    pub mid_dv_mps: f64,
    /// Δv at r2 circularizing, m/s.
    pub arrival_dv_mps: f64,
    /// Full transfer duration (two half-ellipses), s.
    pub transfer_time_s: f64,
}

impl BiellipticSolution {
    /// Total Δv budget of the three impulses, m/s.
    pub fn total_dv_mps(&self) -> f64 {
        self.departure_dv_mps + self.mid_dv_mps + self.arrival_dv_mps
    }
}

/// Two-impulse Hohmann transfer between coplanar circular orbits at `r1_m`
/// and `r2_m` around a body with gravitational parameter `mu_m3_s2`.
/// Works in both directions (raising or lowering); Δvs are magnitudes. The
/// impulse magnitudes come from the shared transfer authority; this wrapper
/// adds the transfer time and the guidance solution type.
pub fn hohmann_transfer(r1_m: f64, r2_m: f64, mu_m3_s2: f64) -> TransferSolution {
    let (departure_dv, arrival_dv) = hohmann_transfer_dv(r1_m, r2_m, mu_m3_s2);
    let a_transfer = (r1_m + r2_m) / 2.0;
    let transfer_time =
        std::f64::consts::PI * (a_transfer * a_transfer * a_transfer / mu_m3_s2).sqrt();
    TransferSolution {
        departure_dv_mps: departure_dv,
        arrival_dv_mps: arrival_dv,
        transfer_time_s: transfer_time,
    }
}

/// Three-impulse bi-elliptic transfer via an intermediate apoapsis `rb_m`
/// (rb > max(r1, r2)). Beats the Hohmann only for large radius ratios and a
/// sufficiently high rb ([`BIELLIPTIC_FAVORABLE_RATIO`]).
pub fn bielliptic_transfer(r1_m: f64, r2_m: f64, rb_m: f64, mu_m3_s2: f64) -> BiellipticSolution {
    let a1 = (r1_m + rb_m) / 2.0; // first ellipse: r1 → rb
    let a2 = (r2_m + rb_m) / 2.0; // second ellipse: rb → r2
    let dv1 = (vis_viva_speed_mps(mu_m3_s2, r1_m, a1) - circular_speed_mps(mu_m3_s2, r1_m)).abs();
    let dv_mid =
        (vis_viva_speed_mps(mu_m3_s2, rb_m, a2) - vis_viva_speed_mps(mu_m3_s2, rb_m, a1)).abs();
    let dv2 = (circular_speed_mps(mu_m3_s2, r2_m) - vis_viva_speed_mps(mu_m3_s2, r2_m, a2)).abs();
    let time = std::f64::consts::PI * ((a1.powi(3) + a2.powi(3)) / mu_m3_s2).sqrt();
    BiellipticSolution {
        departure_dv_mps: dv1,
        mid_dv_mps: dv_mid,
        arrival_dv_mps: dv2,
        transfer_time_s: time,
    }
}

/// True when the target/current radius ratio is large enough that a tall
/// bi-elliptic transfer can beat the Hohmann (verify per case with rb).
pub fn bielliptic_potentially_favorable(r1_m: f64, r2_m: f64) -> bool {
    let ratio = r1_m.max(r2_m) / r1_m.min(r2_m);
    ratio > BIELLIPTIC_FAVORABLE_RATIO
}

/// Combined-maneuver identity: performing a tangential burn `dv1_mps` and a
/// plane rotation whose pure cost would be `dv2_mps` **simultaneously** costs
/// the vector sum
/// `√(dv1² + dv2² − 2·dv1·dv2·cos(i))`,
/// strictly less than burning sequentially whenever both are positive.
/// `angle_between_rad` is the angle between the two Δv vectors (for a plane
/// change paired with a prograde burn this is π − i; callers pass the angle
/// they mean).
pub fn combined_maneuver_dv(dv1_mps: f64, dv2_mps: f64, angle_between_rad: f64) -> f64 {
    (dv1_mps * dv1_mps + dv2_mps * dv2_mps - 2.0 * dv1_mps * dv2_mps * angle_between_rad.cos())
        .sqrt()
}

/// Which leg of the two-impulse transfer the autopilot is executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransferBurnPhase {
    /// Prograde burn until the orbit reaches the target radius as apoapsis
    /// (or periapsis, when lowering).
    #[default]
    Departure,
    /// Ballistic coast along the transfer ellipse to the arrival apsis.
    Coast,
    /// Circularization burn at the arrival apsis (hands off to
    /// [`super::AutopilotMode::OrbitInsertion`] machinery once eccentricity is low).
    Arrival,
    /// Target reached: transfer complete.
    Done,
}

/// Classify the current state into a [`TransferBurnPhase`] for a transfer
/// between the current circular radius and `target_radius_m`. Pure function;
/// the system maps phases onto throttle/attitude commands.
pub fn transfer_burn_phase(
    current_radius_m: f64,
    target_radius_m: f64,
    apoapsis_m: f64,
    eccentricity: f64,
) -> TransferBurnPhase {
    if (current_radius_m - target_radius_m).abs() < 1.0 && eccentricity < 0.01 {
        return TransferBurnPhase::Done;
    }
    let raising = target_radius_m > current_radius_m;
    let apsis_at_target = if raising {
        apoapsis_m >= target_radius_m * 0.999
    } else {
        apoapsis_m <= target_radius_m * 1.001 || (current_radius_m - target_radius_m).abs() < 1.0
    };
    if !apsis_at_target {
        return TransferBurnPhase::Departure;
    }
    if eccentricity > 0.01 {
        // On the transfer ellipse heading to (or sitting near) the apsis.
        TransferBurnPhase::Coast
    } else {
        TransferBurnPhase::Arrival
    }
}

/// Deorbit burn targeting: computes the retrograde delta-v, burn attitude
/// (retrograde), and ignition time to achieve a target periapsis.
pub fn deorbit_burn_targeting(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_periapsis_m: f64,
    mu_m3_s2: f64,
) -> (f64, DQuat) {
    let r = position_m.length();
    let v = velocity_mps.length();
    let dv = deorbit_burn_dv(r, target_periapsis_m, mu_m3_s2);
    // Retrograde attitude: opposite to velocity vector.
    let burn_attitude = if v > 1e-6 {
        DQuat::from_rotation_arc(DVec3::Y, -velocity_mps / v)
    } else {
        DQuat::IDENTITY
    };
    (dv, burn_attitude)
}
