//! The Module 3 scalar density field `D(p) = H_eroded(x, y) - z + Omega(p)`.
//!
//! `p` is a patch-local position in metres: `x` east, `y` north, `z` altitude
//! above the planet's mean radius — the same reference as
//! [`TerrainSource::height_m`](crate::domain::services::terrain_source::TerrainSource::height_m).
//! A positive density is solid rock, a negative density is air, and the rock
//! surface is the zero isosurface. Every term is slope-masked so flat valley
//! floors and plains stay a pure heightmap and no floating rock is introduced.

use crate::domain::math::DVec3;
use crate::domain::services::cube_sphere::direction_to_lat_lon;
use crate::domain::services::terrain_source::{ss, TerrainSource, ValueNoise};

use super::VolumetricGeologyConfig;

/// Seed salts keep the strata, warp, overhang, and Voronoi lookups independent.
const STRATA_SEED: u64 = 0x5354_5241_5441_0001;
const WARP_SEED: u64 = 0x5741_5250_0000_0002;
const OVERHANG_SEED: u64 = 0x4F56_4552_4841_0003;
const VORONOI_SEED: u64 = 0x564F_524F_4E4F_0004;
/// Overhang noise frequency, in cycles per metre (spec `* 0.015`).
const OVERHANG_FREQUENCY_PER_M: f64 = 0.015;
/// Domain-warp noise frequency, in cycles per metre (spec `* 0.03`).
const WARP_FREQUENCY_PER_M: f64 = 0.03;
/// Hard/soft strata displacement amplitude around the hardness midpoint.
const STRATA_DISPLACEMENT_SCALE: f64 = 1.5;

/// A patch-local tangent frame centred on a surface direction.
///
/// Local coordinates are metres: `x` east, `y` north, `z` radial altitude. The
/// frame is a small-angle tangent plane around the patch centre; it is accurate
/// over a patch footprint and never used for planet-scale conversion.
#[derive(Debug, Clone, Copy)]
pub struct PatchFrame {
    center_direction: DVec3,
    east: DVec3,
    north: DVec3,
    radius_m: f64,
}

impl PatchFrame {
    /// Build a tangent frame at `center_direction` for a body of `radius_m`.
    pub fn new(center_direction: DVec3, radius_m: f64) -> Self {
        let up = center_direction.normalize_or_zero();
        let reference = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
        let east = reference.cross(up).normalize_or_zero();
        let north = up.cross(east).normalize_or_zero();
        Self {
            center_direction: up,
            east,
            north,
            radius_m: radius_m.max(1.0),
        }
    }

    pub fn center_direction(&self) -> DVec3 {
        self.center_direction
    }

    pub fn radius_m(&self) -> f64 {
        self.radius_m
    }

    /// Map a local tangent offset `(east_m, north_m)` to a normalized direction.
    /// This is the exact small-angle tangent projection
    /// `normalize(center + east·x/R + north·y/R)`; it agrees with the
    /// cube-sphere mesh sampling at the patch centre and stays smooth across the
    /// footprint.
    pub fn direction_at(&self, east_m: f64, north_m: f64) -> DVec3 {
        (self.center_direction
            + self.east * (east_m / self.radius_m)
            + self.north * (north_m / self.radius_m))
            .normalize_or_zero()
    }

    /// Embed a local point `(x east, y north, z altitude above the mean radius)`
    /// as a body-fixed planet-centred position. The embedding is a flat tangent
    /// plane around the patch centre, exact to well under a metre over a patch
    /// footprint.
    pub fn local_to_body(&self, local: DVec3) -> DVec3 {
        self.center_direction * (self.radius_m + local.z)
            + self.east * local.x
            + self.north * local.y
    }

    /// Rotate a local direction into the body-fixed frame without translation.
    pub fn local_vector_to_body(&self, local: DVec3) -> DVec3 {
        self.east * local.x + self.north * local.y + self.center_direction * local.z
    }
}

/// The Module 3 density field bound to one terrain source and patch frame.
pub struct DensityField<'a> {
    source: &'a dyn TerrainSource,
    frame: PatchFrame,
    config: VolumetricGeologyConfig,
    noise: ValueNoise,
}

impl<'a> DensityField<'a> {
    /// Build a field. `config` must already be valid (see
    /// [`VolumetricGeologyConfig::validate`]).
    pub fn new(
        source: &'a dyn TerrainSource,
        frame: PatchFrame,
        config: VolumetricGeologyConfig,
    ) -> Self {
        Self {
            source,
            frame,
            config,
            noise: ValueNoise,
        }
    }

    pub fn config(&self) -> &VolumetricGeologyConfig {
        &self.config
    }

    pub fn frame(&self) -> &PatchFrame {
        &self.frame
    }

    /// Authoritative height in metres above the mean radius at local
    /// `(east_m, north_m)`. This is the only authority the field consumes.
    pub fn height_m(&self, east_m: f64, north_m: f64) -> f64 {
        let direction = self.frame.direction_at(east_m, north_m);
        let (latitude_deg, longitude_deg) = direction_to_lat_lon(direction);
        self.source.height_m(latitude_deg, longitude_deg)
    }

    /// Slope mask `S(|∇H|) ∈ [0, 1]`: zero on flat ground, one on cliffs. It
    /// restricts every 3D term to steep faces so overhangs cannot appear on
    /// valley floors.
    pub fn slope_mask(&self, east_m: f64, north_m: f64) -> f64 {
        let d = self.config.slope_probe_m;
        let dh_dx =
            (self.height_m(east_m + d, north_m) - self.height_m(east_m - d, north_m)) / (2.0 * d);
        let dh_dy =
            (self.height_m(east_m, north_m + d) - self.height_m(east_m, north_m - d)) / (2.0 * d);
        let gradient = (dh_dx * dh_dx + dh_dy * dh_dy).sqrt();
        ss(self.config.slope_flat, self.config.slope_steep, gradient)
    }

    /// Local rock hardness `K_hard(p) ∈ [0, 1]` from tilted sedimentary strata.
    /// Hard layers protrude, soft layers undercut.
    pub fn hardness(&self, p: DVec3) -> f64 {
        let cfg = &self.config;
        let zeta = p.z + p.x * cfg.tilt[0] + p.y * cfg.tilt[1];
        let mut sum = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 1.0;
        let mut norm = 0.0;
        for octave in 0..cfg.strata_octaves {
            let phase_offset = octave as f64 * 1.7;
            let warp = self.noise.value_noise3(
                cfg.seed ^ STRATA_SEED.wrapping_add(octave as u64),
                zeta * frequency,
                0.0,
                0.0,
            );
            let angle = zeta * cfg.strata_frequency_per_m * frequency + phase_offset + warp * 2.0;
            sum += amplitude * angle.sin();
            norm += amplitude;
            amplitude *= 0.5;
            frequency *= 2.0;
        }
        ss(-0.3, 0.3, sum / norm.max(f64::MIN_POSITIVE))
    }

    /// Voronoi fracture jointing `J(p) ∈ [0, 1]`: one along a fracture seam,
    /// zero inside a block. Uses the deterministic value-noise hash for seeds.
    pub fn jointing(&self, p: DVec3) -> f64 {
        let scaled = p * self.config.joint_scale_per_m;
        let base = DVec3::new(scaled.x.floor(), scaled.y.floor(), scaled.z.floor());
        let f = scaled - base;
        let mut f1 = f64::INFINITY;
        let mut f2 = f64::INFINITY;
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let g = DVec3::new(dx as f64, dy as f64, dz as f64);
                    let cell = base + g;
                    let offset = DVec3::new(
                        self.hash(cell, VORONOI_SEED),
                        self.hash(cell, VORONOI_SEED ^ 0x9E37),
                        self.hash(cell, VORONOI_SEED ^ 0x1F2E),
                    );
                    let r = g + offset - f;
                    let distance = r.length_squared();
                    if distance < f1 {
                        f2 = f1;
                        f1 = distance;
                    } else if distance < f2 {
                        f2 = distance;
                    }
                }
            }
        }
        let diff = f2.sqrt() - f1.sqrt();
        let t = (diff / self.config.joint_width).clamp(0.0, 1.0);
        (1.0 - t).powf(self.config.joint_sharpness)
    }

    /// Scalar density `D(p)`: positive solid rock, negative air, zero on the
    /// rock surface.
    pub fn density(&self, p: DVec3) -> f64 {
        let height = self.height_m(p.x, p.y);
        height - p.z + self.omega(p)
    }

    /// Outward-pointing gradient of the density field, from central differences
    /// with step `step_m`. Used as the Hermite normal for Dual Contouring.
    pub fn gradient(&self, p: DVec3, step_m: f64) -> DVec3 {
        let step = step_m.max(f64::MIN_POSITIVE);
        let dx = self.density(p + DVec3::X * step) - self.density(p - DVec3::X * step);
        let dy = self.density(p + DVec3::Y * step) - self.density(p - DVec3::Y * step);
        let dz = self.density(p + DVec3::Z * step) - self.density(p - DVec3::Z * step);
        DVec3::new(dx, dy, dz).normalize_or_zero()
    }

    /// The slope-masked 3D perturbation `Omega(p)`, exposed for tests and
    /// diagnostics.
    pub fn omega(&self, p: DVec3) -> f64 {
        let cfg = &self.config;
        let hardness = self.hardness(p);

        let warp = DVec3::new(
            self.noise.fbm(
                cfg.seed ^ WARP_SEED,
                p.x * WARP_FREQUENCY_PER_M,
                p.y * WARP_FREQUENCY_PER_M,
                p.z * WARP_FREQUENCY_PER_M,
                3,
            ) * 2.0
                - 1.0,
            self.noise.fbm(
                cfg.seed ^ (WARP_SEED ^ 0x11),
                p.x * WARP_FREQUENCY_PER_M + 5.2,
                p.y * WARP_FREQUENCY_PER_M + 1.3,
                p.z * WARP_FREQUENCY_PER_M + 0.8,
                3,
            ) * 2.0
                - 1.0,
            self.noise.fbm(
                cfg.seed ^ (WARP_SEED ^ 0x22),
                p.x * WARP_FREQUENCY_PER_M + 1.7,
                p.y * WARP_FREQUENCY_PER_M + 9.2,
                p.z * WARP_FREQUENCY_PER_M + 3.4,
                3,
            ) * 2.0
                - 1.0,
        );
        let warped = p + warp * cfg.domain_warp_m;
        let overhang = self.noise.fbm(
            cfg.seed ^ OVERHANG_SEED,
            warped.x * OVERHANG_FREQUENCY_PER_M,
            warped.y * OVERHANG_FREQUENCY_PER_M,
            warped.z * OVERHANG_FREQUENCY_PER_M,
            4,
        ) * 2.0
            - 1.0;

        let strata_displacement = (hardness - 0.5) * STRATA_DISPLACEMENT_SCALE;
        let fracture_displacement = -self.jointing(p) * (1.2 - hardness) * cfg.fracture_depth_m;

        let total =
            overhang * cfg.overhang_amplitude_m + strata_displacement + fracture_displacement;
        total * self.slope_mask(p.x, p.y)
    }

    /// Deterministic hash of a 3D lattice cell to `[0, 1)`.
    fn hash(&self, cell: DVec3, salt: u64) -> f64 {
        self.noise.cell3(
            self.config.seed ^ salt,
            cell.x as i64,
            cell.y as i64,
            cell.z as i64,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::terrain_source::ElevationBounds;
    use std::fmt::Debug;

    /// A flat, constant-height source: slope is exactly zero everywhere, so the
    /// slope mask vanishes and the density reduces to `H - z`.
    #[derive(Debug)]
    struct FlatSource(f64);

    impl TerrainSource for FlatSource {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            self.0
        }
        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(self.0, self.0)
        }
    }

    /// A linear ramp `H = a·east + b·north`, giving a constant, controlled
    /// slope for mask tests.
    #[derive(Debug)]
    struct RampSource {
        radius_m: f64,
        gradient: f64,
    }

    impl TerrainSource for RampSource {
        fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
            // Convert lat/lon back to an approximate east/north offset so the
            // ramp is defined in the same tangent space the field uses.
            let (lat, lon) = (latitude_deg.to_radians(), longitude_deg.to_radians());
            let east = lon * self.radius_m * lat.cos();
            self.gradient * east
        }
        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(-1.0e6, 1.0e6)
        }
    }

    fn field_for(source: &dyn TerrainSource) -> DensityField<'_> {
        let frame = PatchFrame::new(DVec3::new(0.2, 0.3, 0.93), 6_371_000.0);
        DensityField::new(source, frame, VolumetricGeologyConfig::default())
    }

    #[test]
    fn flat_ground_density_is_height_minus_z() {
        let source = FlatSource(120.0);
        let field = field_for(&source);
        assert!(field.slope_mask(10.0, -20.0) < 1e-12);
        assert!((field.density(DVec3::new(10.0, -20.0, 100.0)) - 20.0).abs() < 1e-9);
        assert!(field.density(DVec3::new(10.0, -20.0, 140.0)) < 0.0);
        assert!(field.density(DVec3::new(10.0, -20.0, 90.0)) > 0.0);
    }

    #[test]
    fn slope_mask_is_bounded_and_increases_with_slope() {
        let gentle = field_for(&RampSource {
            radius_m: 6_371_000.0,
            gradient: 0.1,
        });
        let steep = field_for(&RampSource {
            radius_m: 6_371_000.0,
            gradient: 2.0,
        });
        let m_gentle = gentle.slope_mask(0.0, 0.0);
        let m_steep = steep.slope_mask(0.0, 0.0);
        assert!((0.0..=1.0).contains(&m_gentle));
        assert!((0.0..=1.0).contains(&m_steep));
        assert!(m_steep > m_gentle);
        assert!(m_gentle < 1e-6);
        assert!(m_steep > 0.99);
    }

    #[test]
    fn hardness_and_jointing_are_bounded() {
        let source = FlatSource(0.0);
        let field = field_for(&source);
        for i in 0..64 {
            let p = DVec3::new(i as f64 * 3.0, i as f64 * -1.5, i as f64 * 2.0);
            assert!((0.0..=1.0).contains(&field.hardness(p)));
            assert!((0.0..=1.0).contains(&field.jointing(p)));
        }
    }

    #[test]
    fn density_and_gradient_are_deterministic() {
        let source = RampSource {
            radius_m: 6_371_000.0,
            gradient: 0.9,
        };
        let field = field_for(&source);
        let p = DVec3::new(12.0, -7.0, 5.0);
        assert_eq!(field.density(p), field.density(p));
        assert_eq!(field.gradient(p, 1.0), field.gradient(p, 1.0));
        let g = field.gradient(p, 1.0);
        assert!((g.length() - 1.0).abs() < 1e-9);
    }
}
