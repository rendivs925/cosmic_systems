//! Autonomous stage-recovery domain models (spec: `staging-recovery`).
//!
//! Pure, app-free logic for the three mechanical recovery subsystems that are
//! missing from the current ECS wiring:
//!
//! - [`DroneShip`] + [`StationKeeper`]: a drone barge that holds station using
//!   its own thrusters while the landing guidance targets its *predicted*
//!   (drifted) position (spec scenarios "ship position prediction" and "ship
//!   motion compensation").
//! - [`CatchTower`] + [`catch_verdict`]: the chopstick capture envelope and its
//!   success criteria — relative velocity below threshold, attitude within
//!   limits, hardpoint alignment (spec scenarios "catch envelope" and "capture
//!   success criteria").
//!
//! The grid-fin mixer (spec "grid-fin atmospheric control") lives in
//! [`actuation`](crate::domain::services::actuation), with the boostback
//! guidance in [`guidance`](crate::domain::services::guidance). Those already
//! exist. Bevy systems adapt these values into ECS; nothing here depends on
//! Bevy while using the shared domain vector representation.

use crate::domain::math::DVec3;

/// A drone ship (drone barge) drifting under current/wind, holding station with
/// thrusters. `position_m` and `velocity_mps` are in the inertial prediction
/// frame; `external_accel_mps2` is the environmental drift acceleration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DroneShip {
    pub position_m: DVec3,
    pub velocity_mps: DVec3,
    pub external_accel_mps2: DVec3,
    pub mass_kg: f64,
}

impl DroneShip {
    /// Predicted position after `horizon_s` under a constant acceleration
    /// (ballistic drift). This is the moving target the divert guidance should
    /// aim for so the boostback margin covers the ship's motion over the
    /// remaining descent.
    pub fn predict_position(&self, horizon_s: f64) -> DVec3 {
        self.position_m
            + self.velocity_mps * horizon_s
            + 0.5 * self.external_accel_mps2 * horizon_s * horizon_s
    }

    /// Estimated drift velocity after `horizon_s`.
    pub fn predict_velocity(&self, horizon_s: f64) -> DVec3 {
        self.velocity_mps + self.external_accel_mps2 * horizon_s
    }
}

/// A simple station-keeping controller: gains on position error and drift
/// velocity, plus feed-forward against the environmental acceleration, all
/// bounded by the ship's available thrust. Since the sign convention has the
/// ship countering `external_accel_mps2`, the controller drives the predicted
/// position back to the target while actively resisting drift.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StationKeeper {
    pub kp: f64,
    pub kd: f64,
    pub max_thrust_n: f64,
}

impl StationKeeper {
    /// Required station-keeping thrust (N) to hold `DroneShip` at `target_m`.
    /// `thrust = m · (kp·err + kd·(-v) - a_ext)`, clamped to the thruster
    /// envelope. The damping term opposes drift; the P term recovers from any
    /// positional offset; the feed-forward cancels the constant disturbance.
    pub fn thrust(&self, ship: &DroneShip, target_m: DVec3) -> DVec3 {
        let err = target_m - ship.position_m;
        let desired_accel = self.kp * err - self.kd * ship.velocity_mps - ship.external_accel_mps2;
        let thrust = desired_accel * ship.mass_kg;
        let magnitude = thrust.length();
        if magnitude > self.max_thrust_n && magnitude > 0.0 {
            thrust * (self.max_thrust_n / magnitude)
        } else {
            thrust
        }
    }
}

/// A catch tower's chopstick-arm capture volume and its tolerances.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CatchTower {
    /// Where the arms clamp the stage hardpoints, meters.
    pub capture_position_m: DVec3,
    /// Maximum distance from the capture point for the arms to engage, m.
    pub capture_radius_m: f64,
    /// Maximum relative speed for a safe capture, m/s.
    pub max_relative_velocity_mps: f64,
    /// Maximum attitude error (radians) for hardpoint alignment.
    pub max_attitude_error_rad: f64,
}

/// Why a capture failed (spec "capture success criteria").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMiss {
    /// Outside the arm's reach.
    Position,
    /// Too fast for the arms to close.
    Velocity,
    /// Hardpoints not aligned (attitude error too large).
    Attitude,
}

/// The outcome of a catch-tower attempt (spec scenarios "catch envelope" and
/// "capture success criteria").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatchVerdict {
    /// In the envelope but a criterion failed.
    Missed(CaptureMiss),
    /// All criteria satisfied — arms engage on the hardpoints.
    Captured,
}

/// Evaluate a stage's state against a catch tower. `stage_attitude_error_rad`
/// is the mis-alignment of the stage's hardpoint axis from the tower's. The
/// first failing criterion short-circuits to a [`CaptureMiss`].
pub fn catch_verdict(
    stage_position_m: DVec3,
    stage_velocity_mps: DVec3,
    stage_attitude_error_rad: f64,
    tower: &CatchTower,
) -> CatchVerdict {
    let radial_distance = (stage_position_m - tower.capture_position_m).length();
    if radial_distance > tower.capture_radius_m {
        return CatchVerdict::Missed(CaptureMiss::Position);
    }
    let relative_speed = stage_velocity_mps.length();
    if relative_speed > tower.max_relative_velocity_mps {
        return CatchVerdict::Missed(CaptureMiss::Velocity);
    }
    if stage_attitude_error_rad.abs() > tower.max_attitude_error_rad {
        return CatchVerdict::Missed(CaptureMiss::Attitude);
    }
    CatchVerdict::Captured
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ship_prediction_advances_linearly_then_quadratically() {
        let ship = DroneShip {
            position_m: DVec3::ZERO,
            velocity_mps: DVec3::new(0.5, 0.0, 0.0),
            external_accel_mps2: DVec3::new(0.1, 0.0, 0.0),
            mass_kg: 4.0e6,
        };
        let t = 100.0;
        let p = ship.predict_position(t);
        let expected = 0.5 * t + 0.5 * 0.1 * t * t; // 50 + 500 = 550 m
        assert!((p.x - expected).abs() < 1e-9);
        assert!((ship.predict_velocity(t).x - (0.5 + 0.1 * t)).abs() < 1e-9);
    }

    #[test]
    fn station_keeper_rejects_drift_and_drives_to_target() {
        // With a cross-disturbance and an offset, the controller must produce
        // thrust opposing both: term sign is opposite the error and the drift.
        let ship = DroneShip {
            position_m: DVec3::new(10.0, 0.0, 0.0),
            velocity_mps: DVec3::new(-1.0, 0.0, 0.0), // drifting toward target
            external_accel_mps2: DVec3::new(0.3, 0.0, 0.0), // pushing away
            mass_kg: 1_000.0,
        };
        let target = DVec3::ZERO;
        let keeper = StationKeeper {
            kp: 0.05,
            kd: 0.2,
            max_thrust_n: 1.0e6,
        };
        let thrust = keeper.thrust(&ship, target);
        // Error is -10 m ⇒ P term negative (−500); drift is −1 m/s ⇒ +200;
        // external accel +0.3 ⇒ −300. Total ≈ −600 N (pushing toward target).
        assert!(thrust.x < 0.0, "must thrust toward the target");
        // A bigger disturbance saturates the thrust at the thruster ceiling.
        let strong = DroneShip {
            external_accel_mps2: DVec3::new(100.0, 0.0, 0.0),
            ..ship
        };
        let sat = keeper.thrust(&strong, target);
        assert!(
            sat.length() <= keeper.max_thrust_n + 1e-9,
            "thrust must be bounded"
        );
    }

    #[test]
    fn station_keeper_converges_over_time() {
        // Discrete-time sanity: applying the controller's bounded thrust (with
        // the external disturbance removed) should drive the offset to ~zero.
        let mut ship = DroneShip {
            position_m: DVec3::new(20.0, 0.0, 0.0),
            velocity_mps: DVec3::ZERO,
            external_accel_mps2: DVec3::ZERO,
            mass_kg: 1_000.0,
        };
        let target = DVec3::ZERO;
        let keeper = StationKeeper {
            kp: 0.5,
            kd: 1.0,
            max_thrust_n: 1.0e5,
        };
        let dt = 0.5;
        for _ in 0..40 {
            let thrust = keeper.thrust(&ship, target);
            let a = thrust / ship.mass_kg;
            ship.velocity_mps += a * dt;
            ship.position_m += ship.velocity_mps * dt;
        }
        assert!(
            ship.position_m.length() < 0.5,
            "station-keeping should converge, residual {} m",
            ship.position_m.length()
        );
    }

    #[test]
    fn catch_verdict_requires_all_criteria() {
        let tower = CatchTower {
            capture_position_m: DVec3::ZERO,
            capture_radius_m: 2.0,
            max_relative_velocity_mps: 0.8,
            max_attitude_error_rad: 0.05,
        };
        // Perfect capture.
        assert_eq!(
            catch_verdict(DVec3::ZERO, DVec3::ZERO, 0.0, &tower),
            CatchVerdict::Captured
        );
        // Outside radius.
        assert_eq!(
            catch_verdict(DVec3::new(3.0, 0.0, 0.0), DVec3::ZERO, 0.0, &tower),
            CatchVerdict::Missed(CaptureMiss::Position)
        );
        // Too fast.
        assert_eq!(
            catch_verdict(DVec3::ZERO, DVec3::new(0.0, 1.0, 0.0), 0.0, &tower),
            CatchVerdict::Missed(CaptureMiss::Velocity)
        );
        // Mis-aligned hardpoints.
        assert_eq!(
            catch_verdict(DVec3::ZERO, DVec3::ZERO, 0.2, &tower),
            CatchVerdict::Missed(CaptureMiss::Attitude)
        );
    }

    #[test]
    fn catch_verdict_boundary_velocities_are_inclusive() {
        let tower = CatchTower {
            capture_position_m: DVec3::ZERO,
            capture_radius_m: 2.0,
            max_relative_velocity_mps: 1.0,
            max_attitude_error_rad: 0.1,
        };
        // At exactly the limits, capture succeeds (≤ boundary).
        assert_eq!(
            catch_verdict(
                DVec3::new(2.0, 0.0, 0.0),
                DVec3::new(1.0, 0.0, 0.0),
                0.1,
                &tower
            ),
            CatchVerdict::Captured
        );
    }
}
