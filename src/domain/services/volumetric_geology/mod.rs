//! Module 3 volumetric geology: a deterministic 3D density field and Dual
//! Contouring isosurface extraction for cliff overhangs, undercuts, and arches.
//!
//! Source requirements: `docs/technical_modules/3.md`.
//!
//! The authoritative planetary surface stays
//! [`TerrainSource::height_m`](crate::domain::services::terrain_source::TerrainSource::height_m).
//! This module never competes with it: the density field is a pure, seeded
//! function *of* that authority, evaluated in a patch-local tangent frame, and
//! the extracted mesh is a presentation representation only. Collision, radar
//! altitude, guidance, and physics continue to sample the heightmap.
//!
//! ```text
//! TerrainSource::height_m  (authority)
//!         |
//!   PatchFrame (local east/north/up metres)
//!         |
//!   DensityField  D(p) = H(x,y) - z + Omega(p)
//!         |
//!   Dual Contouring (QEF)  ->  ExtractedSurface
//!         |
//!   presentation mesh (added by the streaming/render integration)
//! ```
//!
//! Everything here is pure Rust: no Bevy types, no wall-clock time, no
//! unordered iteration, so identical inputs are bitwise reproducible.

mod dual_contouring;
mod field;
mod shell;

pub use dual_contouring::{extract_surface, ExtractedSurface, VoxelGrid};
pub use field::{DensityField, PatchFrame};
pub use shell::{build_rock_shell, RockShell};

/// Configuration for the Module 3 volumetric density field. Every field carries
/// explicit units; defaults follow `docs/technical_modules/3.md` section 4.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumetricGeologyConfig {
    /// Spatial frequency of sedimentary strata along the tilted vertical axis,
    /// in cycles per metre (spec range 0.05..0.5).
    pub strata_frequency_per_m: f64,
    /// Inverse average fractured rock block / column size, in cycles per metre
    /// (spec range 0.02..0.25).
    pub joint_scale_per_m: f64,
    /// Maximum horizontal undercut depth for overhangs and caves, in metres
    /// (spec range 1..20).
    pub overhang_amplitude_m: f64,
    /// Slope-mask lower bound: gradient magnitude (m/m) below which the surface
    /// stays a pure heightmap and 3D perturbation is suppressed.
    pub slope_flat: f64,
    /// Slope-mask upper bound: gradient magnitude at or above which the full 3D
    /// perturbation applies.
    pub slope_steep: f64,
    /// Recess depth of joint lines along the surface normal, in metres.
    pub fracture_depth_m: f64,
    /// Strata tilt vector `(x, y)` applied to the vertical layer coordinate, so
    /// bedding planes dip instead of staying horizontal.
    pub tilt: [f64; 2],
    /// Bounded octave count for the sedimentary layer series.
    pub strata_octaves: u32,
    /// Domain-warp displacement applied before the overhang noise sample, in
    /// metres (spec's `warp * 5.0`).
    pub domain_warp_m: f64,
    /// Jointing-gap width `w_joint` in normalized Voronoi cell units.
    pub joint_width: f64,
    /// Fracture sharpness exponent `p` (spec range 2..8).
    pub joint_sharpness: f64,
    /// Baseline distance over which the local height gradient is probed, in
    /// metres. The Module 3 specification has no equivalent field; it is named
    /// here so the slope mask has a documented, configuration-driven scale.
    pub slope_probe_m: f64,
    /// Presentation bias added to the density before extraction, in metres. A
    /// small positive bias lifts the extracted shell just above the heightmap
    /// surface so the two are not coplanar where the 3D perturbation vanishes,
    /// avoiding z-fighting. It does not change the physical field.
    pub surface_bias_m: f64,
    /// Fraction of the shell footprint over which the 3D perturbation fades
    /// from full amplitude to zero at the boundary, in `[0, 1)`. This blends
    /// the extracted shell into the surrounding heightmap so the shell edge is
    /// not a hard wall of rock (a crack at the footprint boundary). Zero
    /// disables the taper.
    pub edge_taper_fraction: f64,
    /// Deterministic seed for every noise and Voronoi lookup.
    pub seed: u64,
}

impl Default for VolumetricGeologyConfig {
    fn default() -> Self {
        Self {
            strata_frequency_per_m: 0.15,
            joint_scale_per_m: 0.08,
            overhang_amplitude_m: 6.0,
            slope_flat: 0.4,
            slope_steep: 1.2,
            fracture_depth_m: 0.6,
            tilt: [0.05, 0.02],
            strata_octaves: 3,
            domain_warp_m: 5.0,
            joint_width: 0.25,
            joint_sharpness: 4.0,
            slope_probe_m: 2.0,
            surface_bias_m: 0.0,
            edge_taper_fraction: 0.35,
            seed: 0,
        }
    }
}

impl VolumetricGeologyConfig {
    /// Validate the configuration, returning a human-readable reason on failure.
    /// Called at feature construction so an invalid field never reaches the
    /// mesh pipeline.
    pub fn validate(&self) -> Result<(), &'static str> {
        let finite = |v: f64| v.is_finite();
        if !finite(self.strata_frequency_per_m) || self.strata_frequency_per_m <= 0.0 {
            return Err("strata_frequency_per_m must be finite and positive");
        }
        if !finite(self.joint_scale_per_m) || self.joint_scale_per_m <= 0.0 {
            return Err("joint_scale_per_m must be finite and positive");
        }
        if !finite(self.overhang_amplitude_m) || self.overhang_amplitude_m < 0.0 {
            return Err("overhang_amplitude_m must be finite and non-negative");
        }
        if !finite(self.slope_flat)
            || !finite(self.slope_steep)
            || self.slope_flat >= self.slope_steep
        {
            return Err("slope_flat must be finite and strictly below slope_steep");
        }
        if !finite(self.fracture_depth_m) || self.fracture_depth_m < 0.0 {
            return Err("fracture_depth_m must be finite and non-negative");
        }
        if !self.tilt.iter().all(|v| finite(*v)) {
            return Err("tilt must be finite");
        }
        if self.strata_octaves == 0 || self.strata_octaves > 6 {
            return Err("strata_octaves must be within 1..=6");
        }
        if !finite(self.domain_warp_m) || self.domain_warp_m < 0.0 {
            return Err("domain_warp_m must be finite and non-negative");
        }
        if !finite(self.joint_width) || self.joint_width <= 0.0 {
            return Err("joint_width must be finite and positive");
        }
        if !finite(self.joint_sharpness) || self.joint_sharpness < 1.0 {
            return Err("joint_sharpness must be finite and at least 1");
        }
        if !finite(self.slope_probe_m) || self.slope_probe_m <= 0.0 {
            return Err("slope_probe_m must be finite and positive");
        }
        if !finite(self.surface_bias_m) || self.surface_bias_m < 0.0 {
            return Err("surface_bias_m must be finite and non-negative");
        }
        if !finite(self.edge_taper_fraction) || !(0.0..1.0).contains(&self.edge_taper_fraction) {
            return Err("edge_taper_fraction must be finite and within 0..1");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::math::DVec3;

    #[test]
    fn default_config_is_valid() {
        assert!(VolumetricGeologyConfig::default().validate().is_ok());
    }

    #[test]
    fn validate_rejects_unordered_slope_bounds() {
        let cfg = VolumetricGeologyConfig {
            slope_flat: 1.2,
            slope_steep: 0.4,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_non_positive_frequencies() {
        let cfg = VolumetricGeologyConfig {
            joint_scale_per_m: 0.0,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_out_of_range_edge_taper() {
        let cfg = VolumetricGeologyConfig {
            edge_taper_fraction: 1.0,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn patch_frame_direction_is_unit_and_continuous() {
        let frame = PatchFrame::new(DVec3::new(0.3, 0.5, 0.8), 6_371_000.0);
        let d = frame.direction_at(100.0, -50.0);
        assert!((d.length() - 1.0).abs() < 1e-12);
        // The frame centre maps back to the original direction.
        let c = frame.direction_at(0.0, 0.0);
        assert!((c - frame.center_direction()).length() < 1e-12);
    }
}
