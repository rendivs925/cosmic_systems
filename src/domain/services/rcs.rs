//! Reaction-control thruster set and torque allocation.
//!
//! The catalogue has no public RCS thruster stations, so this module provides a
//! **representative pure-couple set**: one thruster for each sign of each body
//! torque axis, with a fixed moment arm about the current center of mass. It
//! reproduces the validated per-axis torque authority exactly while adding
//! per-thruster state, failure, propellant draw, and a minimum-impulse
//! deadband. Real stations and a coupled (non-pure) layout are a data task
//! (SC-04 in `docs/scientific_correctness_audit.md`).
//!
//! The representative cluster draws from the final stage's main propellant
//! inventory because the catalogued vehicles have no separate RCS tank; its
//! mass flow uses that stage's vacuum specific impulse. This is a documented
//! simplification, not a propellant-chemistry model.

use crate::domain::math::DVec3;
use crate::domain::services::rocket_propulsion::STANDARD_GRAVITY_MPS2;

/// Number of thrusters in the representative pure-couple cluster.
pub const RCS_THRUSTER_COUNT: usize = 6;

/// Representative moment arm from the center of mass, meters.
pub const REPRESENTATIVE_RCS_ARM_M: f64 = 2.0;

/// Representative minimum impulse bit, N·s. A requested impulse below this is
/// dropped; larger requests are treated continuously within the fixed tick.
pub const REPRESENTATIVE_RCS_MIN_IMPULSE_NS: f64 = 0.1;

/// Representative specific impulse when the caller cannot supply the final
/// stage's vacuum Isp, seconds.
pub const REPRESENTATIVE_RCS_ISP_S: f64 = 220.0;

/// One reaction-control thruster.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RcsThruster {
    /// Offset from the current center of mass, meters. The representative
    /// layout keeps this arm fixed so the per-axis authority does not move with
    /// the center of mass.
    pub offset_m: DVec3,
    /// Body-frame unit force direction.
    pub force_axis: DVec3,
    /// Full-duty thrust, N.
    pub thrust_n: f64,
    /// True when the thruster is unavailable (failed or isolated).
    pub failed: bool,
}

impl RcsThruster {
    /// Torque about the body axes produced at full duty, N·m.
    pub fn torque_per_duty_nm(&self) -> DVec3 {
        self.offset_m.cross(self.force_axis * self.thrust_n)
    }
}

/// A representative RCS cluster and its propellant assumptions.
#[derive(Debug, Clone, PartialEq)]
pub struct RcsCluster {
    pub thrusters: [RcsThruster; RCS_THRUSTER_COUNT],
    /// Specific impulse used for the propellant draw, seconds.
    pub isp_s: f64,
    /// Minimum impulse bit, N·s.
    pub min_impulse_ns: f64,
}

/// Result of allocating one fixed tick's torque command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RcsAllocation {
    /// Torque actually produced, N·m. Differs from the command when a
    /// thruster is failed, the authority saturates, the minimum impulse drops
    /// a small command, or the tank cannot supply the requested mass.
    pub torque_nm: DVec3,
    /// Duty of each thruster in [`RcsCluster::thrusters`] order, 0..1.
    pub duties: [f64; RCS_THRUSTER_COUNT],
    /// Propellant consumed this tick, kg.
    pub propellant_used_kg: f64,
    /// True when the command could not be met exactly.
    pub saturated: bool,
}

impl RcsCluster {
    /// Build the representative six-thruster pure-couple cluster.
    ///
    /// Each axis gets a positive and a negative thruster. `max_torque_per_axis_nm`
    /// is the validated per-axis authority; `isp_s` should be the final stage's
    /// vacuum specific impulse because the model draws from its main tank.
    pub fn representative(max_torque_per_axis_nm: f64, isp_s: f64) -> Self {
        let arm_m = REPRESENTATIVE_RCS_ARM_M;
        let thrust_n = (max_torque_per_axis_nm.max(0.0) / arm_m).max(0.0);
        let thruster = |offset_m: DVec3, force_axis: DVec3| RcsThruster {
            offset_m,
            force_axis,
            thrust_n,
            failed: false,
        };
        // +X and -X: a force along ±Z at a +Y arm.
        // +Y and -Y: a force along ∓Z at a +X arm.
        // +Z and -Z: a force along ∓X at a +Y arm.
        Self {
            thrusters: [
                thruster(DVec3::new(0.0, arm_m, 0.0), DVec3::Z),
                thruster(DVec3::new(0.0, arm_m, 0.0), -DVec3::Z),
                thruster(DVec3::new(arm_m, 0.0, 0.0), -DVec3::Z),
                thruster(DVec3::new(arm_m, 0.0, 0.0), DVec3::Z),
                thruster(DVec3::new(0.0, arm_m, 0.0), -DVec3::X),
                thruster(DVec3::new(0.0, arm_m, 0.0), DVec3::X),
            ],
            isp_s: if isp_s.is_finite() && isp_s > 0.0 {
                isp_s
            } else {
                REPRESENTATIVE_RCS_ISP_S
            },
            min_impulse_ns: REPRESENTATIVE_RCS_MIN_IMPULSE_NS,
        }
    }

    /// Allocate a commanded body torque to the thrusters.
    ///
    /// The representative cluster is pure-couple: each thruster contributes to
    /// one body axis only, so each axis independently selects the thruster whose
    /// sign matches the command. A coupled real layout needs a matrix
    /// allocator, which is deliberately not pretended here.
    pub fn allocate(
        &self,
        torque_cmd: DVec3,
        dt_s: f64,
        available_propellant_kg: f64,
    ) -> RcsAllocation {
        let dt = dt_s.max(0.0);
        let mut duties = [0.0_f64; RCS_THRUSTER_COUNT];
        let mut saturated = false;

        for (axis, command) in [torque_cmd.x, torque_cmd.y, torque_cmd.z]
            .into_iter()
            .enumerate()
        {
            if !command.is_finite() || command == 0.0 {
                continue;
            }
            let mut best: Option<(usize, f64)> = None;
            for (index, thruster) in self.thrusters.iter().enumerate() {
                if thruster.failed {
                    continue;
                }
                let torque = thruster.torque_per_duty_nm();
                let component = [torque.x, torque.y, torque.z][axis];
                if component * command > 0.0
                    && best.is_none_or(|(_, magnitude)| component.abs() > magnitude)
                {
                    best = Some((index, component));
                }
            }
            let Some((index, component)) = best else {
                saturated = true;
                continue;
            };
            let requested = (command / component).abs();
            if requested > 1.0 {
                saturated = true;
            }
            duties[index] = requested.min(1.0);
        }

        // Minimum impulse bit: a requested impulse below the bit is dropped.
        // Dropping means the command was not met exactly, so report it.
        for (index, thruster) in self.thrusters.iter().enumerate() {
            let impulse = duties[index] * thruster.thrust_n * dt;
            if impulse > 0.0 && impulse < self.min_impulse_ns {
                duties[index] = 0.0;
                saturated = true;
            }
        }

        // The tank is the authority on total propellant, not the command.
        let mass_flow_kg_s: f64 = duties
            .iter()
            .zip(self.thrusters.iter())
            .map(|(duty, thruster)| duty * thruster.thrust_n / (self.isp_s * STANDARD_GRAVITY_MPS2))
            .sum();
        let requested_kg = mass_flow_kg_s * dt;
        let available_kg = available_propellant_kg.max(0.0);
        let scale = if requested_kg > available_kg && requested_kg > 0.0 {
            saturated = true;
            (available_kg / requested_kg).clamp(0.0, 1.0)
        } else {
            1.0
        };

        let mut torque_nm = DVec3::ZERO;
        let mut propellant_used_kg = 0.0;
        for (index, thruster) in self.thrusters.iter().enumerate() {
            let duty = duties[index] * scale;
            duties[index] = duty;
            torque_nm += thruster.torque_per_duty_nm() * duty;
            propellant_used_kg +=
                duty * thruster.thrust_n / (self.isp_s * STANDARD_GRAVITY_MPS2) * dt;
        }

        RcsAllocation {
            torque_nm,
            duties,
            propellant_used_kg,
            saturated,
        }
    }

    /// Mark one thruster failed or restored. The index is in
    /// [`RcsCluster::thrusters`] order.
    pub fn set_failed(&mut self, index: usize, failed: bool) -> bool {
        let Some(thruster) = self.thrusters.get_mut(index) else {
            return false;
        };
        thruster.failed = failed;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTHORITY_NM: f64 = 5_000.0;
    const DT: f64 = 1.0 / 64.0;

    fn cluster() -> RcsCluster {
        RcsCluster::representative(AUTHORITY_NM, 311.0)
    }

    #[test]
    fn representative_cluster_is_pure_couple_with_two_thrusters_per_axis() {
        let cluster = cluster();
        let mut positive = [0; 3];
        let mut negative = [0; 3];
        for thruster in &cluster.thrusters {
            let torque = thruster.torque_per_duty_nm();
            let non_zero = [torque.x, torque.y, torque.z]
                .into_iter()
                .filter(|component| component.abs() > 1e-9)
                .count();
            assert_eq!(non_zero, 1, "each thruster must be a pure couple");
            for axis in 0..3 {
                let component = [torque.x, torque.y, torque.z][axis];
                if component > 0.0 {
                    positive[axis] += 1;
                } else if component < 0.0 {
                    negative[axis] += 1;
                }
            }
        }
        assert_eq!(positive, [1, 1, 1]);
        assert_eq!(negative, [1, 1, 1]);
    }

    #[test]
    fn allocation_lights_only_the_matching_thruster_and_draws_propellant() {
        let cluster = cluster();
        let allocation = cluster.allocate(DVec3::X * 1_000.0, DT, 1_000.0);
        assert!(!allocation.saturated);
        assert!((allocation.torque_nm.x - 1_000.0).abs() < 1e-9);
        assert!(allocation.torque_nm.y.abs() < 1e-9);
        assert!(allocation.torque_nm.z.abs() < 1e-9);
        assert!((allocation.duties[0] - 0.2).abs() < 1e-9);
        assert!(allocation.duties[1..].iter().all(|duty| *duty == 0.0));
        assert!(allocation.propellant_used_kg > 0.0);
    }

    #[test]
    fn allocation_saturates_at_the_per_axis_authority() {
        let cluster = cluster();
        let allocation = cluster.allocate(DVec3::X * 1.0e9, DT, 1_000.0);
        assert!(allocation.saturated);
        assert!((allocation.torque_nm.x - AUTHORITY_NM).abs() < 1e-9);
        assert!((allocation.duties[0] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn failed_thruster_removes_that_direction() {
        let mut cluster = cluster();
        assert!(cluster.set_failed(0, true));
        let allocation = cluster.allocate(DVec3::X * 1_000.0, DT, 1_000.0);
        assert!(allocation.saturated);
        assert!(allocation.torque_nm.length() < 1e-9);
    }

    #[test]
    fn minimum_impulse_drops_a_tiny_command() {
        let cluster = cluster();
        let tiny = cluster.allocate(DVec3::X * 10.0, DT, 1_000.0);
        assert_eq!(tiny.torque_nm, DVec3::ZERO);
        assert_eq!(tiny.propellant_used_kg, 0.0);
        assert!(tiny.saturated, "a dropped impulse is not an exact command");

        // 100 N*m is above the bit at this authority and tick.
        let above = cluster.allocate(DVec3::X * 100.0, DT, 1_000.0);
        assert!(above.torque_nm.x > 0.0);
    }

    #[test]
    fn propellant_availability_scales_the_allocation() {
        let cluster = cluster();
        let full = cluster.allocate(DVec3::X * AUTHORITY_NM, DT, 1_000.0);
        let half = cluster.allocate(DVec3::X * AUTHORITY_NM, DT, full.propellant_used_kg * 0.5);
        assert!(half.saturated);
        assert!((half.propellant_used_kg - full.propellant_used_kg * 0.5).abs() < 1e-12);
        assert!((half.torque_nm.x - full.torque_nm.x * 0.5).abs() < 1e-9);
    }
}
