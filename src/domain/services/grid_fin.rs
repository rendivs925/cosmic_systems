//! Grid-fin geometry, atmospheric authority, and torque allocation.
//!
//! ## Model
//!
//! A stage carries four identical grid fins in an X arrangement at 45°, 135°,
//! 225°, and 315° around the body's longitudinal +Y axis. Each fin is treated
//! as a flat control surface mounted at radius [`GridFinSpec::radial_arm_m`]
//! and axial station [`GridFinSpec::axial_station_m`] in the stage body frame.
//!
//! The fin normal force is
//!
//! ```text
//! N_i = q · A · C_N_alpha · sin(δ_i) · η(M)
//! ```
//!
//! where `q` is the shared dynamic pressure, `A` the fin area, `δ_i` the
//! commanded deflection, and `η(M)` the existing hypersonic effectiveness
//! taper. The force acts tangentially around the body, so the torque about the
//! center of mass is
//!
//! ```text
//! τ_i = N_i · (arm_y · e_r - r · Y)
//! ```
//!
//! with `arm_y` the axial distance from the center of mass to the fin station
//! and `r` the radial arm. The allocation below inverts that linear map with a
//! minimum-norm solution, then converts each normal-force request into a
//! deflection. Saturation is resolved by the caller: control recomputes the
//! torque the saturated fins actually produce and sends the remainder to RCS.
//!
//! This is a reduced-order actuator model. It deliberately omits the deflected
//! lattice's induced axial drag and uses a representative normal-force slope
//! rather than a wind-tunnel table; the config declares the values as
//! `Representative`. The hinge is ideal (no rate limit or actuator lag).

use crate::domain::math::DVec3;
use crate::domain::services::actuation::grid_fin_effectiveness;

/// Number of fins in the X arrangement this model supports.
pub const GRID_FIN_COUNT: usize = 4;

/// Static grid-fin installation on a stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridFinSpec {
    /// Number of fins. The current X mixer supports exactly four.
    pub count: u32,
    /// Planform area of one fin, m².
    pub fin_area_m2: f64,
    /// Distance from the body axis to the fin center of pressure, m.
    pub radial_arm_m: f64,
    /// Fin center of pressure along the stage body +Y axis, m from the stage
    /// geometric center. The center of mass is subtracted at runtime.
    pub axial_station_m: f64,
    /// Mechanical deflection limit, radians.
    pub max_deflection_rad: f64,
    /// Representative normal-force slope `C_N_alpha`, per radian.
    pub normal_force_coefficient_per_rad: f64,
}

impl GridFinSpec {
    /// Validate a catalog installation. Invalid hardware must fail at load
    /// rather than silently produce impossible authority in flight.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.count as usize != GRID_FIN_COUNT {
            return Err("grid fins require exactly four fins for the X mixer");
        }
        if !self.fin_area_m2.is_finite() || self.fin_area_m2 <= 0.0 {
            return Err("grid-fin area must be finite and positive");
        }
        if !self.radial_arm_m.is_finite() || self.radial_arm_m <= 0.0 {
            return Err("grid-fin radial arm must be finite and positive");
        }
        if !self.axial_station_m.is_finite() {
            return Err("grid-fin axial station must be finite");
        }
        if !self.max_deflection_rad.is_finite()
            || self.max_deflection_rad <= 0.0
            || self.max_deflection_rad >= std::f64::consts::FRAC_PI_2
        {
            return Err("grid-fin deflection limit must be inside (0, 90) degrees");
        }
        if !self.normal_force_coefficient_per_rad.is_finite()
            || self.normal_force_coefficient_per_rad <= 0.0
        {
            return Err("grid-fin normal-force slope must be finite and positive");
        }
        Ok(())
    }

    /// Azimuth of fin `index` in the body XZ plane, measured from +X toward +Z.
    pub fn fin_azimuth_rad(index: usize) -> f64 {
        std::f64::consts::FRAC_PI_4 + index as f64 * std::f64::consts::FRAC_PI_2
    }

    /// Normal-force scale `q·A·C_N_alpha·η(M)` in newtons. Each fin's actual
    /// normal force is this scale times `sin(deflection)`.
    pub fn normal_force_scale_n(&self, dynamic_pressure_pa: f64, mach: f64) -> f64 {
        if !dynamic_pressure_pa.is_finite() || dynamic_pressure_pa <= 0.0 {
            return 0.0;
        }
        let scale = dynamic_pressure_pa
            * self.fin_area_m2
            * self.normal_force_coefficient_per_rad
            * grid_fin_effectiveness(mach);
        if scale.is_finite() {
            scale.max(0.0)
        } else {
            0.0
        }
    }

    /// Minimum-norm deflection allocation for a commanded body torque.
    ///
    /// `center_of_mass_y_m` is the body-frame center-of-mass ordinate, so the
    /// pitch/yaw arm moves as propellant is consumed. With no dynamic pressure
    /// or a degenerate arm the fins have no authority and the allocation is
    /// zero; the caller still has RCS.
    pub fn allocate_deflections(
        &self,
        torque_body_nm: DVec3,
        dynamic_pressure_pa: f64,
        mach: f64,
        center_of_mass_y_m: f64,
    ) -> [f64; GRID_FIN_COUNT] {
        let arm_y = self.axial_station_m - center_of_mass_y_m;
        let scale = self.normal_force_scale_n(dynamic_pressure_pa, mach);
        if self.count as usize != GRID_FIN_COUNT
            || !arm_y.is_finite()
            || arm_y.abs() < 1.0e-3
            || !self.radial_arm_m.is_finite()
            || self.radial_arm_m <= 0.0
            || scale <= f64::MIN_POSITIVE
            || !torque_body_nm.is_finite()
        {
            return [0.0; GRID_FIN_COUNT];
        }

        let mut deflections = [0.0; GRID_FIN_COUNT];
        for (index, deflection) in deflections.iter_mut().enumerate() {
            let azimuth = Self::fin_azimuth_rad(index);
            let normal_force_n = (azimuth.cos() * torque_body_nm.x
                + azimuth.sin() * torque_body_nm.z)
                / (2.0 * arm_y)
                - torque_body_nm.y / (4.0 * self.radial_arm_m);
            let ratio = (normal_force_n / scale).clamp(-1.0, 1.0);
            *deflection = ratio
                .asin()
                .clamp(-self.max_deflection_rad, self.max_deflection_rad);
        }
        deflections
    }

    /// Normal force produced by each fin at the given deflections, newtons.
    pub fn normal_forces_n(
        &self,
        deflections_rad: &[f64; GRID_FIN_COUNT],
        dynamic_pressure_pa: f64,
        mach: f64,
    ) -> [f64; GRID_FIN_COUNT] {
        let scale = self.normal_force_scale_n(dynamic_pressure_pa, mach);
        deflections_rad.map(|deflection| scale * deflection.sin())
    }

    /// Body-frame force from the fins, newtons.
    pub fn force_body_n(
        &self,
        deflections_rad: &[f64; GRID_FIN_COUNT],
        dynamic_pressure_pa: f64,
        mach: f64,
    ) -> DVec3 {
        let forces = self.normal_forces_n(deflections_rad, dynamic_pressure_pa, mach);
        forces
            .iter()
            .enumerate()
            .map(|(index, force_n)| {
                let azimuth = Self::fin_azimuth_rad(index);
                DVec3::new(-azimuth.sin(), 0.0, azimuth.cos()) * *force_n
            })
            .sum()
    }

    /// Body-frame torque about the center of mass from the fins, N·m.
    pub fn torque_body_nm(
        &self,
        deflections_rad: &[f64; GRID_FIN_COUNT],
        dynamic_pressure_pa: f64,
        mach: f64,
        center_of_mass_y_m: f64,
    ) -> DVec3 {
        let arm_y = self.axial_station_m - center_of_mass_y_m;
        if self.count as usize != GRID_FIN_COUNT || !arm_y.is_finite() {
            return DVec3::ZERO;
        }
        let forces = self.normal_forces_n(deflections_rad, dynamic_pressure_pa, mach);
        forces
            .iter()
            .enumerate()
            .map(|(index, force_n)| {
                let azimuth = Self::fin_azimuth_rad(index);
                DVec3::new(
                    arm_y * azimuth.cos(),
                    -self.radial_arm_m,
                    arm_y * azimuth.sin(),
                ) * *force_n
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> GridFinSpec {
        GridFinSpec {
            count: 4,
            fin_area_m2: 1.86,
            radial_arm_m: 2.4,
            axial_station_m: 18.5,
            max_deflection_rad: 30.0_f64.to_radians(),
            normal_force_coefficient_per_rad: 2.0,
        }
    }

    /// Dynamic pressure high enough that a modest torque does not saturate.
    const Q_PA: f64 = 20_000.0;
    const MACH: f64 = 0.5;

    #[test]
    fn pure_roll_command_uses_equal_fin_deflections() {
        let spec = spec();
        let requested_nm = DVec3::new(0.0, 40_000.0, 0.0);
        let deflections = spec.allocate_deflections(requested_nm, Q_PA, MACH, 0.0);
        for deflection in deflections {
            assert!(
                (deflection - deflections[0]).abs() < 1.0e-12,
                "a pure roll command must deflect every fin equally: {deflections:?}"
            );
        }
        let produced = spec.torque_body_nm(&deflections, Q_PA, MACH, 0.0);
        assert!(
            (produced.y - requested_nm.y).abs() < 1.0,
            "roll torque mismatch: requested {:?}, produced {:?}",
            requested_nm,
            produced
        );
        assert!(produced.x.abs() < 1.0 && produced.z.abs() < 1.0);
    }

    #[test]
    fn pure_pitch_command_cancels_roll_and_yaw() {
        let spec = spec();
        let requested_nm = DVec3::new(60_000.0, 0.0, 0.0);
        let deflections = spec.allocate_deflections(requested_nm, Q_PA, MACH, 0.0);
        let produced = spec.torque_body_nm(&deflections, Q_PA, MACH, 0.0);
        assert!(
            (produced.x - requested_nm.x).abs() < 1.0,
            "pitch torque mismatch: requested {:?}, produced {:?}",
            requested_nm,
            produced
        );
        assert!(
            produced.y.abs() < 1.0 && produced.z.abs() < 1.0,
            "a pure pitch request must not leak into roll/yaw: {produced:?}"
        );
    }

    #[test]
    fn fin_torque_reverses_with_the_command() {
        let spec = spec();
        let positive = spec.allocate_deflections(DVec3::new(0.0, 0.0, 25_000.0), Q_PA, MACH, 0.0);
        let negative = spec.allocate_deflections(DVec3::new(0.0, 0.0, -25_000.0), Q_PA, MACH, 0.0);
        for (positive, negative) in positive.iter().zip(negative.iter()) {
            assert!((positive + negative).abs() < 1.0e-12);
        }
    }

    #[test]
    fn no_dynamic_pressure_means_no_fin_authority() {
        let spec = spec();
        assert_eq!(
            spec.allocate_deflections(DVec3::splat(1.0e6), 0.0, 5.0, 0.0),
            [0.0; GRID_FIN_COUNT]
        );
        assert_eq!(
            spec.torque_body_nm(&[0.2; GRID_FIN_COUNT], 0.0, 5.0, 0.0),
            DVec3::ZERO
        );
    }

    #[test]
    fn allocation_respects_the_mechanical_limit() {
        let spec = spec();
        let deflections = spec.allocate_deflections(DVec3::splat(1.0e9), Q_PA, MACH, 0.0);
        for deflection in deflections {
            assert!(deflection.abs() <= spec.max_deflection_rad + 1.0e-12);
        }
    }

    #[test]
    fn invalid_installations_are_rejected() {
        assert!(spec().validate().is_ok());
        let mut invalid = spec();
        invalid.count = 3;
        assert!(invalid.validate().is_err());
        let mut invalid = spec();
        invalid.fin_area_m2 = 0.0;
        assert!(invalid.validate().is_err());
        let mut invalid = spec();
        invalid.max_deflection_rad = std::f64::consts::FRAC_PI_2;
        assert!(invalid.validate().is_err());
    }
}
