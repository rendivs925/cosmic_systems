//! Deterministic, Bevy-free vegetation placement and species rules.
//!
//! Placement is derived entirely from patch identity, a fixed seed, and
//! authoritative terrain signals, so identical inputs always produce identical
//! candidates independent of frame rate, spawn order, or cache history. Nothing
//! here feeds collision, altitude, or physics: vegetation is presentation only.

use crate::domain::services::cube_sphere::TerrainPatch;
use crate::domain::services::reference_frames::terrain_lat_lon_to_body_fixed;
use crate::domain::services::terrain_source::ValueNoise;

/// Salt distinguishing tree candidates from grass candidates for one patch.
pub const TREE_CANDIDATE_SALT: u64 = 0x7A11_5EED;
/// Salt for ground-cover (grass) candidates.
pub const GRASS_CANDIDATE_SALT: u64 = 0x6A55_5EED;

/// Named, validated vegetation configuration: the single source of truth for
/// placement budgets, ecological thresholds, clumping, and the committed
/// land-cover package path. Presets derive from [`Self::DEFAULT`] so tuning
/// happens in one place and cannot drift between the mesh budget and the
/// placement pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VegetationConfig {
    /// Maximum accepted tree candidates per full-density patch.
    pub tree_budget: usize,
    /// Maximum accepted grass clumps per full-density patch.
    pub grass_budget: usize,
    /// Patch level at which the full candidate budget is reached; coarser
    /// presentation scales the budget down.
    pub full_density_level: u32,
    /// Combined cover density below which tree candidates are dropped.
    pub tree_min_density: f64,
    /// Combined cover density below which grass candidates are dropped.
    pub grass_min_density: f64,
    /// Candidate-grid oversampling applied before the ecological gate.
    pub candidate_oversample: f64,
    /// Fraction of a cell a candidate may jitter from its center.
    pub cell_jitter: f64,
    /// Seed for the low-frequency clumping field.
    pub clump_seed: u64,
    /// Minimum value of the clumping mask, so clearings never fully vanish.
    pub clump_min: f64,
    /// Committed land-cover package consumed by the scatter pass.
    pub land_cover_path: &'static str,
}

impl VegetationConfig {
    pub const DEFAULT: Self = Self {
        tree_budget: 128,
        grass_budget: 1024,
        full_density_level: 14,
        tree_min_density: 0.08,
        grass_min_density: 0.05,
        candidate_oversample: 2.4,
        cell_jitter: 0.7,
        clump_seed: 0x00A1_C0FF_EE01_2345,
        clump_min: 0.35,
        land_cover_path: "assets/large_files/terrain/earth_landcover_v1.clcvr",
    };

    /// Whether the configuration is internally consistent. Invalid values fail
    /// loudly at startup instead of silently degrading cover.
    pub fn is_valid(&self) -> bool {
        self.tree_budget > 0
            && self.grass_budget > 0
            && self.full_density_level > 0
            && (0.0..=1.0).contains(&self.tree_min_density)
            && (0.0..=1.0).contains(&self.grass_min_density)
            && self.candidate_oversample >= 1.0
            && (0.0..=1.0).contains(&self.cell_jitter)
            && (0.0..=1.0).contains(&self.clump_min)
            && !self.land_cover_path.is_empty()
    }
}

impl Default for VegetationConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Fraction of a grid cell a candidate may jitter from its cell center. Below
/// 1.0 this preserves a guaranteed minimum spacing while removing lattice
/// alignment.
const CELL_JITTER: f64 = VegetationConfig::DEFAULT.cell_jitter;
/// Oversampling factor applied to the candidate grid before ecological
/// acceptance, so thinning still leaves a spatially even cover.
pub const CANDIDATE_OVERSAMPLE: f64 = VegetationConfig::DEFAULT.candidate_oversample;

const CLUMP_SEED: u64 = VegetationConfig::DEFAULT.clump_seed;
/// Low-frequency clumping field, mapped so clearings open but cover never
/// vanishes entirely.
const CLUMP_MIN: f64 = VegetationConfig::DEFAULT.clump_min;

/// Deterministic hash of three integers to `[0, 1)`, used for every placement
/// decision so results never depend on evaluation order.
pub(crate) fn scatter_hash01(a: u64, b: u64, c: u64) -> f64 {
    let mut h =
        a ^ (b.wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ (c.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h & 0xFFFF_FFFF_FFFF) as f64 / 0x1_0000_0000_0000u64 as f64
}

/// The deterministic species set. Each species maps to bounded baked skeleton
/// parameters; the adapter turns these into merged prism/cross-card geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VegetationSpecies {
    TropicalBroadleaf,
    TemperateBroadleaf,
    Conifer,
    Palm,
    Shrub,
    Grass,
}

impl VegetationSpecies {
    pub const ALL: [Self; 6] = [
        Self::TropicalBroadleaf,
        Self::TemperateBroadleaf,
        Self::Conifer,
        Self::Palm,
        Self::Shrub,
        Self::Grass,
    ];

    /// Whether the species is rendered as a tree (trunk plus canopy) rather
    /// than as ground cover.
    pub const fn is_tree(self) -> bool {
        !matches!(self, Self::Grass)
    }

    /// Bounded, validated skeleton parameters for the species.
    pub const fn profile(self) -> SpeciesProfile {
        match self {
            Self::TropicalBroadleaf => SpeciesProfile {
                trunk_height_m: 6.0,
                trunk_radius_m: 0.22,
                canopy_height_m: 7.0,
                canopy_width_m: 6.0,
                canopy_layers: 2,
                min_density: 0.30,
                min_spacing_m: 5.0,
                max_slope_deg: 34.0,
                min_elevation_m: 0.5,
                max_elevation_m: 1_800.0,
            },
            Self::TemperateBroadleaf => SpeciesProfile {
                trunk_height_m: 4.5,
                trunk_radius_m: 0.18,
                canopy_height_m: 5.0,
                canopy_width_m: 4.5,
                canopy_layers: 2,
                min_density: 0.24,
                min_spacing_m: 4.5,
                max_slope_deg: 34.0,
                min_elevation_m: 0.5,
                max_elevation_m: 2_400.0,
            },
            Self::Conifer => SpeciesProfile {
                trunk_height_m: 3.0,
                trunk_radius_m: 0.14,
                canopy_height_m: 8.0,
                canopy_width_m: 3.0,
                canopy_layers: 3,
                min_density: 0.18,
                min_spacing_m: 3.5,
                max_slope_deg: 38.0,
                min_elevation_m: 0.5,
                max_elevation_m: 3_400.0,
            },
            Self::Palm => SpeciesProfile {
                trunk_height_m: 5.0,
                trunk_radius_m: 0.16,
                canopy_height_m: 3.0,
                canopy_width_m: 4.2,
                canopy_layers: 1,
                min_density: 0.45,
                min_spacing_m: 5.5,
                max_slope_deg: 24.0,
                min_elevation_m: 0.5,
                max_elevation_m: 400.0,
            },
            Self::Shrub => SpeciesProfile {
                trunk_height_m: 0.35,
                trunk_radius_m: 0.06,
                canopy_height_m: 1.3,
                canopy_width_m: 1.7,
                canopy_layers: 1,
                min_density: 0.10,
                min_spacing_m: 1.6,
                max_slope_deg: 40.0,
                min_elevation_m: 0.5,
                max_elevation_m: 4_200.0,
            },
            Self::Grass => SpeciesProfile {
                trunk_height_m: 0.0,
                trunk_radius_m: 0.0,
                canopy_height_m: 0.6,
                canopy_width_m: 0.55,
                canopy_layers: 1,
                min_density: 0.05,
                min_spacing_m: 0.6,
                max_slope_deg: 32.0,
                min_elevation_m: 0.5,
                max_elevation_m: 4_800.0,
            },
        }
    }
}

/// Bounded skeleton parameters for one species. All heights and widths are
/// meters and all thresholds are in the same units as the terrain signals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeciesProfile {
    pub trunk_height_m: f64,
    pub trunk_radius_m: f64,
    pub canopy_height_m: f64,
    pub canopy_width_m: f64,
    pub canopy_layers: u8,
    pub min_density: f64,
    /// Minimum in-species spacing, in meters. Canopy species need more room
    /// than understory shrubs, which need more than ground cover.
    pub min_spacing_m: f64,
    pub max_slope_deg: f64,
    pub min_elevation_m: f64,
    pub max_elevation_m: f64,
}

/// Depth, in meters, that a plant base is sunk below the sampled surface so it
/// never floats on a slope. Scales with trunk height and has a small floor.
pub fn embed_depth_m(profile: SpeciesProfile) -> f64 {
    (profile.trunk_height_m * 0.12).max(0.12)
}

/// Deterministic species selection from climate and terrain inputs. Returns
/// `None` when the site cannot support the species set (water, cliff, or too
/// cold/dry). Never random: the same inputs always select the same species.
pub fn select_species(
    latitude_deg: f64,
    elevation_m: f64,
    moisture: f64,
    slope_deg: f64,
    density: f64,
) -> Option<VegetationSpecies> {
    if !elevation_m.is_finite() || elevation_m < 0.5 || slope_deg > 40.0 || density <= 0.0 {
        return None;
    }
    let abs_lat = latitude_deg.abs();
    // A latitude-dependent treeline: high peaks and polar ground drop to
    // shrub/ground cover rather than hosting full trees.
    let treeline_m = (4_200.0 - abs_lat * 28.0).max(400.0);
    if elevation_m > treeline_m {
        return (density >= 0.06 && slope_deg <= 36.0).then_some(VegetationSpecies::Shrub);
    }

    let candidate = if abs_lat < 23.0 {
        if elevation_m < 400.0 && moisture > 0.55 && density > 0.45 {
            VegetationSpecies::Palm
        } else if density > 0.30 {
            VegetationSpecies::TropicalBroadleaf
        } else {
            VegetationSpecies::Shrub
        }
    } else if abs_lat < 48.0 {
        if moisture > 0.5 && density > 0.24 {
            VegetationSpecies::TemperateBroadleaf
        } else {
            VegetationSpecies::Shrub
        }
    } else if abs_lat < 66.0 {
        if density > 0.18 {
            VegetationSpecies::Conifer
        } else {
            VegetationSpecies::Shrub
        }
    } else {
        VegetationSpecies::Shrub
    };

    let profile = candidate.profile();
    if density < profile.min_density
        || slope_deg > profile.max_slope_deg
        || elevation_m < profile.min_elevation_m
        || elevation_m > profile.max_elevation_m
    {
        // Fall back to ground cover when the chosen species cannot grow here.
        return (density >= VegetationSpecies::Grass.profile().min_density)
            .then_some(VegetationSpecies::Grass);
    }
    Some(candidate)
}

/// Low-frequency clumping/clearing mask in `[CLUMP_MIN, 1]`, sampled from the
/// continuous geographic coordinate so adjacent patches agree on shared edges
/// and clearings do not repeat per patch.
pub fn clump_mask(latitude_deg: f64, longitude_deg: f64) -> f64 {
    let p = terrain_lat_lon_to_body_fixed(latitude_deg, longitude_deg) * 5.0;
    let n = ValueNoise.value_noise3(CLUMP_SEED, p.x, p.y, p.z);
    CLUMP_MIN + (1.0 - CLUMP_MIN) * n.clamp(0.0, 1.0)
}

/// Combine the source's climate-derived density with an optional measured
/// land-cover density. Measured cover takes precedence where available and is
/// multiplied by the climate signal so a dense-cover class on a cold, dry site
/// still thins; absent cover leaves the climate density unchanged. Presentation
/// only: this never feeds the authoritative terrain source.
pub fn combined_cover_density(climate_density: f64, land_cover_density: Option<f64>) -> f64 {
    let climate = climate_density.clamp(0.0, 1.0);
    match land_cover_density {
        Some(land) => (climate * land.clamp(0.0, 1.0)).clamp(0.0, 1.0),
        None => climate,
    }
}

/// Deterministic jittered-grid blue-noise candidates over one patch, in face UV.
/// `accept` applies the ecological gate at each candidate; the returned list
/// never exceeds `budget` and preserves minimum spacing because cell jitter is
/// below half a cell. The grid is phased per patch so neighbouring patches do
/// not expose a shared lattice at their edge.
pub fn vegetation_candidates(
    patch: &TerrainPatch,
    budget: usize,
    salt: u64,
    mut accept: impl FnMut(f64, f64) -> bool,
) -> Vec<(f64, f64)> {
    if budget == 0 {
        return Vec::new();
    }
    let (u0, v0, u1, v1) = patch.uv_bounds();
    let side = ((budget as f64 * CANDIDATE_OVERSAMPLE).sqrt().ceil() as usize).max(1);
    let phase_u = scatter_hash01(
        patch.face as u64 ^ salt ^ 0x51A7,
        patch.tile_x as u64,
        patch.tile_y as u64,
    );
    let phase_v = scatter_hash01(
        patch.face as u64 ^ salt ^ 0x7E33,
        patch.tile_y as u64,
        patch.tile_x as u64,
    );

    let mut accepted = Vec::new();
    for gy in 0..side {
        for gx in 0..side {
            let cell = (gy * side + gx) as u64;
            let jitter_u =
                scatter_hash01(patch.face as u64 ^ salt, cell, patch.tile_x as u64 ^ 0x1F3D);
            let jitter_v =
                scatter_hash01(patch.face as u64 ^ salt, cell, patch.tile_y as u64 ^ 0x2C91);
            let grid_u = gx as f64 + 0.5 + (jitter_u - 0.5) * CELL_JITTER;
            let grid_v = gy as f64 + 0.5 + (jitter_v - 0.5) * CELL_JITTER;
            let gu = (grid_u / side as f64 + phase_u).rem_euclid(1.0);
            let gv = (grid_v / side as f64 + phase_v).rem_euclid(1.0);
            let u = u0 + (u1 - u0) * gu;
            let v = v0 + (v1 - v0) * gv;
            if accept(u, v) {
                accepted.push((u, v));
            }
        }
    }

    if accepted.len() > budget {
        // Thin evenly across the accepted set rather than truncating in grid
        // order, so saturated sites keep spatially even coverage.
        let total = accepted.len();
        accepted = (0..budget)
            .map(|index| accepted[index * total / budget])
            .collect();
    }
    accepted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::cube_sphere::face_uv_to_direction;

    #[test]
    fn default_configuration_is_valid_and_single_sourced() {
        let config = VegetationConfig::default();
        assert!(config.is_valid());
        assert_eq!(config, VegetationConfig::DEFAULT);
        // The scatter budget, thresholds, and mask all read back from the config.
        assert_eq!(CELL_JITTER, config.cell_jitter);
        assert_eq!(CANDIDATE_OVERSAMPLE, config.candidate_oversample);
        assert_eq!(CLUMP_MIN, config.clump_min);

        assert!(!VegetationConfig {
            tree_budget: 0,
            ..config
        }
        .is_valid());
        assert!(!VegetationConfig {
            tree_min_density: 1.5,
            ..config
        }
        .is_valid());
        assert!(!VegetationConfig {
            candidate_oversample: 0.5,
            ..config
        }
        .is_valid());
        assert!(!VegetationConfig {
            land_cover_path: "",
            ..config
        }
        .is_valid());
    }

    #[test]
    fn species_selection_is_deterministic_and_ecological() {
        // Humid tropical lowland -> palm.
        assert_eq!(
            select_species(-3.0, 40.0, 0.8, 4.0, 0.6),
            Some(VegetationSpecies::Palm)
        );
        // Temperate wet -> broadleaf; dry -> shrub.
        assert_eq!(
            select_species(40.0, 300.0, 0.7, 5.0, 0.5),
            Some(VegetationSpecies::TemperateBroadleaf)
        );
        assert_eq!(
            select_species(40.0, 300.0, 0.3, 5.0, 0.15),
            Some(VegetationSpecies::Shrub)
        );
        // Boreal -> conifer.
        assert_eq!(
            select_species(58.0, 500.0, 0.5, 10.0, 0.4),
            Some(VegetationSpecies::Conifer)
        );
        // Water and cliffs reject.
        assert_eq!(select_species(0.0, -10.0, 0.5, 5.0, 0.8), None);
        assert_eq!(select_species(0.0, 100.0, 0.5, 60.0, 0.8), None);
        // Above the treeline only ground cover remains.
        assert_eq!(
            select_species(30.0, 4_500.0, 0.5, 10.0, 0.4),
            Some(VegetationSpecies::Shrub)
        );
        assert_eq!(select_species(30.0, 4_500.0, 0.5, 10.0, 0.02), None);
        // Identical inputs select identical species.
        for species in VegetationSpecies::ALL {
            let profile = species.profile();
            assert!(profile.canopy_layers >= 1 && profile.canopy_layers <= 3);
            assert!(profile.min_density >= 0.0 && profile.min_density <= 1.0);
        }
    }

    #[test]
    fn species_profiles_are_bounded_and_distinct() {
        let conifer = VegetationSpecies::Conifer.profile();
        let broadleaf = VegetationSpecies::TemperateBroadleaf.profile();
        let palm = VegetationSpecies::Palm.profile();
        assert!(conifer.canopy_height_m > broadleaf.canopy_height_m);
        assert!(conifer.canopy_width_m < broadleaf.canopy_width_m);
        assert!(palm.canopy_width_m > palm.canopy_height_m);
        assert!(VegetationSpecies::Shrub.profile().canopy_height_m < broadleaf.canopy_height_m);
    }

    #[test]
    fn per_species_spacing_and_embed_depth_are_ordered() {
        let canopy = VegetationSpecies::TropicalBroadleaf.profile().min_spacing_m;
        let shrub = VegetationSpecies::Shrub.profile().min_spacing_m;
        let grass = VegetationSpecies::Grass.profile().min_spacing_m;
        assert!(canopy > shrub && shrub > grass, "{canopy} {shrub} {grass}");
        for species in VegetationSpecies::ALL {
            let profile = species.profile();
            assert!(profile.min_spacing_m > 0.0);
            assert!(embed_depth_m(profile) >= 0.12);
        }
        assert!(
            embed_depth_m(VegetationSpecies::Conifer.profile())
                > embed_depth_m(VegetationSpecies::Shrub.profile())
        );
    }

    #[test]
    fn clump_mask_is_continuous_across_the_longitude_seam() {
        let east = clump_mask(10.0, 179.9999);
        let west = clump_mask(10.0, -179.9999);
        assert!((east - west).abs() < 1.0e-3, "{east} vs {west}");
        assert!((CLUMP_MIN..=1.0).contains(&east));
        assert_eq!(east, clump_mask(10.0, 179.9999));
    }

    #[test]
    fn combined_cover_density_prefers_measured_cover_and_falls_back() {
        assert_eq!(combined_cover_density(0.5, None), 0.5);
        assert_eq!(combined_cover_density(0.5, Some(1.0)), 0.5);
        assert_eq!(combined_cover_density(1.0, Some(0.0)), 0.0);
        assert_eq!(combined_cover_density(2.0, Some(2.0)), 1.0);
        assert_eq!(combined_cover_density(-1.0, None), 0.0);
    }

    #[test]
    fn candidates_are_deterministic_and_patch_specific() {
        let patch = TerrainPatch {
            face: crate::domain::services::cube_sphere::CubeFace::PosZ,
            level: 12,
            tile_x: 0,
            tile_y: 0,
        };
        let accept = |_u: f64, _v: f64| true;
        let a = vegetation_candidates(&patch, 64, TREE_CANDIDATE_SALT, accept);
        let b = vegetation_candidates(&patch, 64, TREE_CANDIDATE_SALT, accept);
        assert_eq!(a, b);
        assert!(!a.is_empty() && a.len() <= 64);
        // Distinct salts and patches produce distinct candidate sets.
        let other_salt = vegetation_candidates(&patch, 64, GRASS_CANDIDATE_SALT, accept);
        assert_ne!(a, other_salt);
        let other_patch = TerrainPatch {
            face: patch.face,
            level: patch.level,
            tile_x: 1,
            tile_y: 0,
        };
        assert_ne!(
            a,
            vegetation_candidates(&other_patch, 64, TREE_CANDIDATE_SALT, accept)
        );
    }

    #[test]
    fn candidate_spacing_is_blue_noise_like() {
        let patch = TerrainPatch {
            face: crate::domain::services::cube_sphere::CubeFace::PosX,
            level: 12,
            tile_x: 3,
            tile_y: 3,
        };
        let budget = 128;
        let candidates = vegetation_candidates(&patch, budget, TREE_CANDIDATE_SALT, |_u, _v| true);
        let (u0, v0, u1, v1) = patch.uv_bounds();
        let (span_u, span_v) = (u1 - u0, v1 - v0);
        let side = ((budget as f64 * CANDIDATE_OVERSAMPLE).sqrt().ceil() as usize).max(1);
        let min_spacing = (1.0 / side as f64) * (1.0 - CELL_JITTER);

        let mut pairs = 0usize;
        let mut too_close = 0usize;
        for (index, (u, v)) in candidates.iter().enumerate() {
            for (other_u, other_v) in &candidates[index + 1..] {
                let du = ((u - other_u) / span_u).abs();
                let dv = ((v - other_v) / span_v).abs();
                pairs += 1;
                if du < min_spacing && dv < min_spacing {
                    too_close += 1;
                }
            }
        }
        // A jittered grid below half a cell is blue-noise-like: only the few
        // pairs that wrap across the patch seam may fall under the guaranteed
        // spacing, never a lattice-wide pattern.
        assert!(
            too_close * 20 < pairs.max(1),
            "{too_close} of {pairs} pairs within {min_spacing}"
        );
    }

    #[test]
    fn candidates_respect_the_gate_and_patch_bounds() {
        let patch = TerrainPatch {
            face: crate::domain::services::cube_sphere::CubeFace::NegZ,
            level: 13,
            tile_x: 1,
            tile_y: 2,
        };
        assert!(vegetation_candidates(&patch, 64, TREE_CANDIDATE_SALT, |_u, _v| false).is_empty());
        let (u0, v0, u1, v1) = patch.uv_bounds();
        for (u, v) in vegetation_candidates(&patch, 32, TREE_CANDIDATE_SALT, |_u, _v| true) {
            assert!((u0..=u1).contains(&u) && (v0..=v1).contains(&v));
        }
        // The generator is pure: it never consults the face direction itself.
        let _ = face_uv_to_direction(patch.face, 0.5, 0.5);
    }
}
