//! Patch-scale rock shell built from the Module 3 density field.
//!
//! This is the seam between the pure domain field and the terrain streaming /
//! rendering layers: it samples the density field over a bounded patch-local
//! volume, extracts the Dual Contouring surface, and returns body-fixed
//! planet-centred geometry in the same frame the heightmap patch mesh uses. It
//! is presentation-only and never consulted by collision or physics.

use crate::domain::math::DVec3;
use crate::domain::services::terrain_source::TerrainSource;

use super::{extract_surface, DensityField, PatchFrame, VolumetricGeologyConfig, VoxelGrid};

/// A Dual Contoured rock shell in body-fixed planet-centred coordinates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RockShell {
    pub positions: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
    pub indices: Vec<u32>,
}

impl RockShell {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Build a rock shell for one patch, or `None` when the patch is too flat to
/// warrant volumetric geometry or the field yields no surface.
///
/// `half_extent_m` is the patch half-width, `vertical_range_m` the altitude band
/// scanned around the patch-centre height, and `cell_m` the voxel size. Work is
/// bounded by `(2·half/cell)² · (vertical/cell)` density samples.
#[allow(clippy::too_many_arguments)]
pub fn build_rock_shell(
    source: &dyn TerrainSource,
    center_direction: DVec3,
    radius_m: f64,
    half_extent_m: f64,
    vertical_range_m: f64,
    cell_m: f64,
    config: VolumetricGeologyConfig,
) -> Option<RockShell> {
    if config.validate().is_err()
        || half_extent_m <= 0.0
        || vertical_range_m <= 0.0
        || cell_m <= 0.0
    {
        return None;
    }

    let frame = PatchFrame::new(center_direction, radius_m);
    let field = DensityField::new(source, frame, config)
        .with_edge_taper(half_extent_m, config.edge_taper_fraction);

    // Steepness gate: a nearly flat patch has no overhang and stays a pure
    // heightmap, so skip the whole volumetric evaluation.
    let probe = half_extent_m * 0.8;
    let mut max_mask = 0.0f64;
    for (x, y) in [
        (-probe, -probe),
        (probe, -probe),
        (-probe, probe),
        (probe, probe),
        (0.0, 0.0),
    ] {
        max_mask = max_mask.max(field.slope_mask(x, y));
    }
    if max_mask < 0.5 {
        return None;
    }

    let center_height = field.height_m(0.0, 0.0);
    let horizontal = ((half_extent_m * 2.0 / cell_m).round() as usize).max(1) + 1;
    let vertical = ((vertical_range_m / cell_m).round() as usize).max(1) + 1;
    let origin = DVec3::new(
        -half_extent_m,
        -half_extent_m,
        center_height - vertical_range_m * 0.5,
    );
    let grid = VoxelGrid::sample_with_bias(
        &field,
        origin,
        cell_m,
        [horizontal, horizontal, vertical],
        config.surface_bias_m,
    );
    let surface = extract_surface(&grid);
    if surface.is_empty() {
        return None;
    }

    let mut positions = Vec::with_capacity(surface.vertices.len());
    let mut normals = Vec::with_capacity(surface.vertices.len());
    for (vertex, normal) in surface.vertices.iter().zip(&surface.normals) {
        positions.push(frame.local_to_body(*vertex).to_array());
        normals.push(
            frame
                .local_vector_to_body(*normal)
                .normalize_or_zero()
                .to_array(),
        );
    }
    Some(RockShell {
        positions,
        normals,
        indices: surface.indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::terrain_source::ElevationBounds;
    use std::fmt::Debug;

    #[derive(Debug)]
    struct Flat(f64);
    impl TerrainSource for Flat {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            self.0
        }
        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(self.0, self.0)
        }
    }

    /// A cliff: height ramps steeply with longitude, giving a strong slope.
    #[derive(Debug)]
    struct Cliff {
        radius_m: f64,
    }
    impl TerrainSource for Cliff {
        fn height_m(&self, _latitude_deg: f64, longitude_deg: f64) -> f64 {
            // ~2 m of rise per metre of ground at the probe scale.
            longitude_deg.to_radians() * self.radius_m * 2.0
        }
        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(-1.0e7, 1.0e7)
        }
    }

    #[test]
    fn flat_patch_yields_no_shell() {
        let source = Flat(0.0);
        let shell = build_rock_shell(
            &source,
            DVec3::new(0.2, 0.3, 0.93),
            6_371_000.0,
            400.0,
            200.0,
            25.0,
            VolumetricGeologyConfig::default(),
        );
        assert!(shell.is_none());
    }

    #[test]
    fn steep_patch_yields_a_bounded_shell() {
        let source = Cliff {
            radius_m: 6_371_000.0,
        };
        let shell = build_rock_shell(
            &source,
            DVec3::new(0.2, 0.3, 0.93),
            6_371_000.0,
            400.0,
            200.0,
            25.0,
            VolumetricGeologyConfig::default(),
        )
        .expect("steep patch should produce a shell");
        assert!(!shell.is_empty());
        assert_eq!(shell.positions.len(), shell.normals.len());
        assert!(shell
            .positions
            .iter()
            .all(|p| p.iter().all(|c| c.is_finite())));
        for normal in &shell.normals {
            let n = DVec3::from_array(*normal);
            assert!((n.length() - 1.0).abs() < 1e-6);
        }
        assert!(shell
            .indices
            .iter()
            .all(|i| (*i as usize) < shell.positions.len()));
    }

    #[test]
    fn shell_generation_is_deterministic() {
        let source = Cliff {
            radius_m: 6_371_000.0,
        };
        let build = || {
            build_rock_shell(
                &source,
                DVec3::new(-0.1, 0.6, 0.79),
                6_371_000.0,
                300.0,
                150.0,
                25.0,
                VolumetricGeologyConfig::default(),
            )
        };
        assert_eq!(build(), build());
    }

    /// At the streaming resolution the extracted mesh must contain genuine
    /// overhangs, not just steep heightfield facets: a pure heightmap can never
    /// produce a face whose normal points back toward the planet centre.
    #[test]
    fn shell_at_streaming_resolution_contains_overhangs() {
        let source = Cliff {
            radius_m: 6_371_000.0,
        };
        let center = DVec3::new(0.2, 0.3, 0.93);
        let shell = build_rock_shell(
            &source,
            center,
            6_371_000.0,
            45.0,
            90.0,
            1.5,
            VolumetricGeologyConfig::default(),
        )
        .expect("steep patch should produce a shell");
        let up = center.normalize_or_zero();
        let min_radial = shell
            .normals
            .iter()
            .map(|n| DVec3::from_array(*n).dot(up))
            .fold(f64::INFINITY, f64::min);
        assert!(
            min_radial < 0.0,
            "expected an overhanging (downward-facing) normal, got min radial {min_radial}"
        );
    }
}
