//! Authoritative terrain height source (AGENTS.md sections 20-21).
//!
//! `TerrainSource` is the single terrain-data boundary. Render meshes and
//! collision queries consume the same authoritative source.
//!
//! Heights are in meters above the planet's mean radius (geocentric). The
//! source is planet-scoped: the planet is bound when the source is attached to
//! a celestial body, so the interface takes latitude/longitude only.
//!
//! Procedural generation is deterministic: it uses seeded value noise with no
//! runtime state, so identical inputs always produce identical output
//! independent of frame rate, spawn order, or camera movement.
//!
//! The implementation is split into cohesive submodules: [`noise`] (value
//! noise), [`procedural`] (procedural sources), [`layered`] (layer
//! composition), [`appearance`] (surface appearance law), and [`catalog`]
//! (Earth/Moon/Mars data-backed sources).

use crate::domain::services::cube_sphere::{PatchGeometricError, TerrainPatch};
use std::fmt::Debug;

mod appearance;
mod catalog;
mod layered;
mod noise;
mod procedural;

pub use appearance::{slope_deg_at, surface_appearance, with_river_appearance, SurfaceAppearance};
pub use catalog::EarthTerrainSource;
#[cfg(feature = "dem")]
pub use catalog::{
    EarthTerrainDataError, MarsTerrainSource, MoonTerrainSource, DEFAULT_EARTH_DEM_PATH,
};
pub use layered::{DetailLodFade, LayeredTerrainSource, TerrainDetailLayer, TerrainElevationLayer};
pub use noise::ValueNoise;
pub use procedural::{
    PadFlatZone, ProceduralDetailSource, ProceduralTerrainConfig, ProceduralTerrainSource,
};

/// Helper: `smoothstep(edge0, edge1, x)`.
pub(crate) fn ss(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Broad surface classification supplied by the authoritative terrain source.
/// More detailed material or biome distinctions remain presentation concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurfaceClass {
    /// The source has no physical land-cover classification.
    #[default]
    Unknown,
    /// Surface lies at or below the source's zero-elevation sea-level datum.
    Ocean,
    /// Surface lies above the source's zero-elevation sea-level datum.
    Land,
}

/// A coherent terrain sample from the authoritative source.
///
/// Heights are meters above mean radius; moisture and river strength are
/// normalized to `[0, 1]`. The type deliberately excludes a surface normal:
/// normals require a body radius and remain the responsibility of
/// `terrain_collision::sample_surface`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainSurfaceSample {
    pub final_height_m: f64,
    pub moisture: f64,
    pub river_strength: f64,
    pub surface_class: SurfaceClass,
}

impl Default for TerrainSurfaceSample {
    fn default() -> Self {
        Self {
            final_height_m: 0.0,
            moisture: 0.5,
            river_strength: 0.0,
            surface_class: SurfaceClass::Unknown,
        }
    }
}

/// A source of terrain surface heights in meters above the mean radius.
pub trait TerrainSource: Send + Sync + Debug {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64;

    /// Conservative elevation interval for this source in meters above the
    /// body's mean radius. Streaming uses it for culling and LOD; collision and
    /// mesh sampling remain authoritative through [`Self::height_m`].
    fn elevation_bounds_m(&self) -> ElevationBounds;

    /// Conservative source-specific geometric error for a cube-sphere patch.
    /// The default retains the global envelope used before terrain sources could
    /// expose indexed metadata, so all existing sources remain safe.
    fn patch_geometric_error(&self, _patch: &TerrainPatch) -> PatchGeometricError {
        let bounds = self.elevation_bounds_m();
        PatchGeometricError::from_elevation_bounds(bounds.min_m, bounds.max_m)
    }

    /// Coherent authoritative sample. Existing height and material metadata
    /// methods remain supported as compatibility accessors.
    fn surface_sample(&self, latitude_deg: f64, longitude_deg: f64) -> TerrainSurfaceSample {
        TerrainSurfaceSample {
            final_height_m: self.height_m(latitude_deg, longitude_deg),
            moisture: self.moisture(latitude_deg, longitude_deg).clamp(0.0, 1.0),
            river_strength: self
                .river_strength(latitude_deg, longitude_deg)
                .clamp(0.0, 1.0),
            surface_class: self.surface_class(latitude_deg, longitude_deg),
        }
    }

    /// Compatibility classification for sources that only expose elevation.
    fn surface_class(&self, latitude_deg: f64, longitude_deg: f64) -> SurfaceClass {
        if self.height_m(latitude_deg, longitude_deg) <= 0.0 {
            SurfaceClass::Ocean
        } else {
            SurfaceClass::Land
        }
    }

    /// Presentation height at one cube-sphere LOD. The default is the exact
    /// physical sample. Expensive layered sources may return a deterministic
    /// coarse representation for distant meshes, but collision, altitude, and
    /// all simulation systems must continue to use [`Self::height_m`].
    fn mesh_height_m(&self, latitude_deg: f64, longitude_deg: f64, _patch_level: u32) -> f64 {
        self.height_m(latitude_deg, longitude_deg)
    }

    /// Prepare expensive, deterministic data for a sample. This is invoked only
    /// by terrain worker tasks; fixed-step collision queries must use `height_m`
    /// without causing I/O or a terrain bake.
    fn prepare_sample(&self, _latitude_deg: f64, _longitude_deg: f64) {}

    /// Coarse, non-authoritative height for whole-body presentation such as the
    /// rocket overview map. Sources with expensive local detail should expose a
    /// cheap base value here; physics, collision, and terrain meshes must keep
    /// using [`Self::height_m`].
    fn overview_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.height_m(latitude_deg, longitude_deg)
    }

    /// Normalized soil moisture in `[0, 1]` (drives vegetation/biome). Sources
    /// without a moisture model default to a neutral `0.5` so biomes still
    /// vary by elevation and latitude.
    fn moisture(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.5
    }

    /// Normalized river-channel strength in `[0, 1]`. This is presentation
    /// metadata derived from the same authoritative terrain source; a default
    /// of zero keeps sources without hydrology dry.
    fn river_strength(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.0
    }

    /// Normalized vegetation cover in `[0, 1]` used to place scatter. This is
    /// the authoritative presentation signal for whether ground is forest,
    /// grassland, or bare. It is deliberately independent of the rendered
    /// albedo, and defaults to bare so a source without a climate model grows no
    /// scatter until it explicitly overrides this or composes one.
    fn vegetation_density(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.0
    }

    /// Coarse, non-authoritative moisture for whole-body presentation.
    fn overview_moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.moisture(latitude_deg, longitude_deg)
    }

    /// Coarse, non-authoritative slope for whole-body presentation. It derives
    /// from overview heights so it cannot initialize local terrain caches.
    fn overview_slope_deg(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let d = 0.02; // ~2 km probe for a stable, feature-scale gradient
        let hx = self.overview_height_m(latitude_deg, longitude_deg + d)
            - self.overview_height_m(latitude_deg, longitude_deg - d);
        let hy = self.overview_height_m(latitude_deg + d, longitude_deg)
            - self.overview_height_m(latitude_deg - d, longitude_deg);
        let lat_m = 111_320.0;
        let lon_m = (111_320.0 * latitude_deg.to_radians().cos()).abs().max(1.0);
        let gradient = (hx / (2.0 * d * lon_m)).hypot(hy / (2.0 * d * lat_m));
        gradient.atan().to_degrees()
    }

    /// Normalized latitude zone in `[0, 1]` (`0` = south pole, `1` = north
    /// pole), used to fade cold-biome coloring toward the poles. Default maps
    /// latitude linearly; planets cold/temperate callers may override.
    fn zone_lat(&self, latitude_deg: f64) -> f64 {
        ((latitude_deg + 90.0) / 180.0).clamp(0.0, 1.0)
    }
}

/// Conservative elevation interval in meters above mean radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElevationBounds {
    pub min_m: f64,
    pub max_m: f64,
}

impl ElevationBounds {
    pub fn new(min_m: f64, max_m: f64) -> Self {
        assert!(
            min_m.is_finite() && max_m.is_finite() && min_m <= max_m,
            "terrain elevation bounds must be finite and ordered"
        );
        Self { min_m, max_m }
    }

    /// Conservative bounds for the sum of two independent elevation layers.
    pub fn combine(self, other: Self) -> Self {
        Self::new(self.min_m + other.min_m, self.max_m + other.max_m)
    }
}

/// Central angle in degrees between two lat/lon points.
pub fn central_angle_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (a1, b1, a2, b2) = (
        lat1.to_radians(),
        lon1.to_radians(),
        lat2.to_radians(),
        lon2.to_radians(),
    );
    let d = (a1.sin() * a2.sin() + a1.cos() * a2.cos() * (b2 - b1).cos())
        .clamp(-1.0, 1.0)
        .acos();
    d.to_degrees()
}

#[cfg(test)]
#[derive(Debug)]
struct FlatTerrainSource;

#[cfg(test)]
impl TerrainSource for FlatTerrainSource {
    fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.0
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        ElevationBounds::new(0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "dem")]
    use super::catalog::LocalElevationOverlayTerrainSource;
    use super::*;
    #[cfg(feature = "dem")]
    use crate::domain::services::dem_terrain_source::{CubeSphereDem, DemTerrainSource};
    #[cfg(feature = "dem")]
    use crate::domain::services::local_elevation::LocalElevationPackage;
    #[cfg(feature = "dem")]
    use crate::domain::services::planet_factory::PlanetFactory;
    #[cfg(feature = "dem")]
    use crate::domain::services::reference_frames::geodetic_to_terrain_lat_lon;
    #[cfg(feature = "dem")]
    use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
    #[cfg(feature = "dem")]
    use crate::domain::value_objects::launch_site_coordinates::predefined_sites;
    use std::sync::Arc;

    #[test]
    fn procedural_regeneration_is_identical() {
        let source = ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0);
        let a = source.height_m(12.34, -45.67);
        let b = source.height_m(12.34, -45.67);
        assert_eq!(a, b);
    }

    #[test]
    fn surface_sample_preserves_legacy_terrain_metadata_defaults() {
        let source = FlatTerrainSource;
        assert_eq!(
            source.surface_sample(0.0, 0.0),
            TerrainSurfaceSample {
                final_height_m: 0.0,
                moisture: 0.5,
                river_strength: 0.0,
                surface_class: SurfaceClass::Ocean,
            }
        );
    }

    #[derive(Debug)]
    struct TerrainMetadataSource {
        moisture: f64,
        river_strength: f64,
    }

    impl TerrainSource for TerrainMetadataSource {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            0.0
        }

        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(0.0, 0.0)
        }

        fn moisture(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            self.moisture
        }

        fn river_strength(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            self.river_strength
        }
    }

    #[test]
    fn layered_source_exposes_detail_biomes_and_drainage() {
        let layered = LayeredTerrainSource::new(
            TerrainElevationLayer::new(Arc::new(FlatTerrainSource), ElevationBounds::new(0.0, 0.0)),
            Some(TerrainElevationLayer::new(
                Arc::new(TerrainMetadataSource {
                    moisture: 0.2,
                    river_strength: 0.1,
                }),
                ElevationBounds::new(0.0, 0.0),
            )),
            Some(TerrainDetailLayer::new(
                Arc::new(TerrainMetadataSource {
                    moisture: 0.8,
                    river_strength: 0.7,
                }),
                ElevationBounds::new(0.0, 0.0),
                DetailLodFade::new(3, 6),
            )),
        );

        assert!((layered.moisture(0.0, 0.0) - 0.59).abs() < 1e-12);
        assert_eq!(layered.river_strength(0.0, 0.0), 0.7);
    }

    #[cfg(feature = "dem")]
    #[test]
    fn moon_dem_treats_negative_elevation_as_solid_surface() {
        let source = MoonTerrainSource {
            source: Arc::new(DemTerrainSource::from_dem(
                CubeSphereDem::new(2, vec![-200; 24]).expect("valid cube-sphere DEM"),
            )),
        };

        assert_eq!(source.height_m(0.0, 0.0), -200.0);
        assert_eq!(source.surface_class(0.0, 0.0), SurfaceClass::Land);
        assert_eq!(
            source.surface_sample(0.0, 0.0).surface_class,
            SurfaceClass::Land
        );
    }

    #[cfg(feature = "dem")]
    #[test]
    fn mars_dem_treats_negative_elevation_as_solid_surface() {
        let source = MarsTerrainSource {
            source: Arc::new(DemTerrainSource::from_dem(
                CubeSphereDem::new(2, vec![-200; 24]).expect("valid cube-sphere DEM"),
            )),
        };

        assert_eq!(source.height_m(0.0, 0.0), -200.0);
        assert_eq!(source.surface_class(0.0, 0.0), SurfaceClass::Land);
    }

    #[cfg(feature = "dem")]
    #[test]
    fn local_elevation_overlay_replaces_only_covered_global_samples() {
        let source = LocalElevationOverlayTerrainSource {
            global: Arc::new(DemTerrainSource::from_dem(
                CubeSphereDem::new(2, vec![10; 24]).expect("valid cube-sphere DEM"),
            )),
            local: Arc::new(
                LocalElevationPackage::from_samples(
                    2,
                    2,
                    -1.0,
                    -1.0,
                    1.0,
                    1.0,
                    crate::domain::services::local_elevation::LocalElevationMetadata {
                        body: "Earth".into(),
                        coordinate_frame: "terrain-radial-degrees".into(),
                        horizontal_datum: "WGS84".into(),
                        vertical_datum: "test".into(),
                        source_resolution_m: 1.0,
                        nodata_policy: "fallback".into(),
                        source_sha256: "0".repeat(64),
                        license: "test".into(),
                        conversion_version: 1,
                        blend_border_m: 0.0,
                    },
                    vec![50.0; 4],
                )
                .expect("valid local elevation package"),
            ),
        };

        assert_eq!(source.height_m(0.0, 0.0), 50.0);
        assert_eq!(source.height_m(10.0, 10.0), 10.0);
        assert_eq!(
            source.elevation_bounds_m(),
            ElevationBounds::new(10.0, 50.0)
        );
    }

    #[test]
    fn procedural_is_independent_of_evaluation_order() {
        let source = ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0);
        let points = [(-30.0, 120.0), (30.0, -120.0), (0.0, 0.0)];
        let forward: Vec<f64> = points
            .iter()
            .map(|(la, lo)| source.height_m(*la, *lo))
            .collect();
        let reverse: Vec<f64> = points
            .iter()
            .rev()
            .map(|(la, lo)| source.height_m(*la, *lo))
            .collect();
        assert_eq!(forward[0], reverse[2]);
        assert_eq!(forward[2], reverse[0]);
    }

    #[test]
    fn different_seeds_differ() {
        let a = ProceduralTerrainSource::new(1, 2_500.0, 1_200.0, 0);
        let b = ProceduralTerrainSource::new(2, 2_500.0, 1_200.0, 0);
        let (la, lo) = (10.0, 20.0);
        assert_ne!(a.height_m(la, lo), b.height_m(la, lo));
    }

    #[cfg(feature = "dem")]
    #[test]
    fn default_earth_source_adds_bounded_detail_over_the_measured_dem_package() {
        let default_source = EarthTerrainSource::new();
        let package_source = DemTerrainSource::from_path(DEFAULT_EARTH_DEM_PATH)
            .expect("resident Earth ETOPO1 terrain package must load");

        for (latitude_deg, longitude_deg) in [(-40.0, 100.0), (62.0, -35.0), (-12.0, 145.0)] {
            let detail_m = default_source.height_m(latitude_deg, longitude_deg)
                - package_source.height_m(latitude_deg, longitude_deg);
            assert!(
                (ProceduralDetailSource::elevation_bounds_m().min_m
                    ..=ProceduralDetailSource::elevation_bounds_m().max_m)
                    .contains(&detail_m),
                "procedural detail must stay within its declared bounds: {detail_m}"
            );
        }
    }

    #[cfg(feature = "dem")]
    #[test]
    fn default_earth_source_grades_launch_pads_level() {
        let source = EarthTerrainSource::new();
        let package_source = DemTerrainSource::from_path(DEFAULT_EARTH_DEM_PATH)
            .expect("resident Earth ETOPO1 terrain package must load");
        let earth = PlanetFactory::create_by_id(&CelestialBodyId::earth()).expect("Earth exists");

        for site in [
            predefined_sites::kennedy_space_center(),
            predefined_sites::papua_indonesia_coastal_lowland(),
        ] {
            let (latitude_deg, longitude_deg) = geodetic_to_terrain_lat_lon(&site, &earth);
            assert_eq!(
                source.height_m(latitude_deg, longitude_deg),
                package_source.height_m(latitude_deg, longitude_deg),
                "a graded pad must suppress procedural detail at {latitude_deg}, {longitude_deg}"
            );
        }
    }

    #[test]
    fn procedural_detail_is_deterministic_seam_safe_and_varied_nearby() {
        let detail = ProceduralDetailSource::new(99);
        let point = (28.573, -80.647);
        assert_eq!(
            detail.height_m(point.0, point.1),
            detail.height_m(point.0, point.1)
        );

        let nearby: Vec<f64> = (0..8)
            .map(|step| detail.height_m(point.0, point.1 + step as f64 * 0.0005))
            .collect();
        let range = nearby.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - nearby.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(range > 0.01, "expected local relief, got range {range}");
        assert!(
            nearby.iter().all(|height| (-36.0..=24.0).contains(height)),
            "local detail exceeded its bounded envelope: {nearby:?}"
        );

        let east = detail.height_m(10.0, 179.9999);
        let west = detail.height_m(10.0, -179.9999);
        assert!(
            (east - west).abs() < 1.0,
            "detail must remain continuous across the longitude seam: {east} vs {west}"
        );
    }

    #[derive(Debug)]
    struct ConstantTerrain(f64);

    impl TerrainSource for ConstantTerrain {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            self.0
        }

        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(self.0, self.0)
        }
    }

    #[test]
    fn layered_source_sums_explicit_contributions_with_conservative_bounds() {
        let source = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ConstantTerrain(10.0)),
                ElevationBounds::new(10.0, 10.0),
            ),
            Some(TerrainElevationLayer::new(
                Arc::new(ConstantTerrain(-4.0)),
                ElevationBounds::new(-4.0, -4.0),
            )),
            Some(TerrainDetailLayer::new(
                Arc::new(ConstantTerrain(-2.0)),
                ElevationBounds::new(-2.0, 4.0),
                DetailLodFade::new(3, 6),
            )),
        );

        assert_eq!(source.height_m(12.0, -45.0), 4.0);
        assert_eq!(source.elevation_bounds_m(), ElevationBounds::new(4.0, 10.0));
    }

    #[test]
    fn detail_lod_fade_is_continuous_and_does_not_change_physical_height() {
        let fade = DetailLodFade::new(3, 6);
        assert_eq!(fade.weight_for_level(3), 0.0);
        assert_eq!(fade.weight_for_level(6), 1.0);
        assert!(fade.weight_for_level(4) > 0.0 && fade.weight_for_level(4) < 1.0);
        assert!(fade.weight_for_level(5) > fade.weight_for_level(4));
        assert!((fade.weight_for_level(4) + fade.weight_for_level(5) - 1.0).abs() < 1e-12);

        let source = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ConstantTerrain(10.0)),
                ElevationBounds::new(10.0, 10.0),
            ),
            None,
            Some(TerrainDetailLayer::new(
                Arc::new(ConstantTerrain(4.0)),
                ElevationBounds::new(4.0, 4.0),
                fade,
            )),
        );
        assert_eq!(source.height_m(0.0, 0.0), 14.0);
        assert_eq!(source.height_m(0.0, 0.0), 14.0);
    }

    #[test]
    fn layered_source_is_deterministic_and_continuous_at_the_longitude_seam() {
        let make_source = || {
            let base = Arc::new(ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0));
            let detail = Arc::new(ProceduralDetailSource::new(99));
            LayeredTerrainSource::new(
                TerrainElevationLayer::new(base.clone(), base.elevation_bounds_m()),
                None,
                Some(TerrainDetailLayer::new(
                    detail,
                    ProceduralDetailSource::elevation_bounds_m(),
                    DetailLodFade::new(3, 6),
                )),
            )
        };
        let first = make_source();
        let second = make_source();
        let point = (10.0, 179.9999);
        assert_eq!(
            first.height_m(point.0, point.1),
            second.height_m(point.0, point.1)
        );

        let east = first.height_m(10.0, 180.0);
        let west = first.height_m(10.0, -180.0);
        assert!(
            (east - west).abs() < 1e-6,
            "layered source must be continuous at the longitude seam: {east} vs {west}"
        );
    }

    #[test]
    fn collision_and_render_samples_agree_on_composed_height() {
        let base = Arc::new(ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0));
        let detail = Arc::new(ProceduralDetailSource::new(99));
        let source = LayeredTerrainSource::new(
            TerrainElevationLayer::new(base.clone(), base.elevation_bounds_m()),
            None,
            Some(TerrainDetailLayer::new(
                detail,
                ProceduralDetailSource::elevation_bounds_m(),
                DetailLodFade::new(3, 6),
            )),
        );
        let (lat, lon) = (33.0, -110.0);
        let render_height = source.height_m(lat, lon);
        let collision = crate::domain::services::terrain_collision::sample_surface(
            &source,
            lat,
            lon,
            6_371_000.0,
        );
        assert_eq!(collision.height_m, render_height);
    }

    #[test]
    fn crater_height_is_a_depression() {
        // At the crater center the height is the negative depth (bowl).
        let h = ProceduralTerrainSource::crater_height(10.0, 10.0, 10.0, 10.0, 3.0, 500.0);
        assert!((h + 500.0).abs() < 1e-6);
        // Far away the crater contributes nothing.
        assert_eq!(
            ProceduralTerrainSource::crater_height(10.0, 40.0, 10.0, 10.0, 3.0, 500.0),
            0.0
        );
    }

    #[test]
    fn crater_rim_is_continuous_at_its_outer_radius() {
        let radius_deg = 3.0;
        let depth_m = 500.0;
        let just_inside = ProceduralTerrainSource::crater_height(
            radius_deg - 0.000_001,
            10.0,
            0.0,
            10.0,
            radius_deg,
            depth_m,
        );
        let at_radius = ProceduralTerrainSource::crater_height(
            radius_deg, 10.0, 0.0, 10.0, radius_deg, depth_m,
        );
        let outside = ProceduralTerrainSource::crater_height(
            radius_deg + 0.000_001,
            10.0,
            0.0,
            10.0,
            radius_deg,
            depth_m,
        );

        assert!(
            (just_inside - at_radius).abs() < 0.001,
            "crater rim must not jump at its outer radius: {just_inside} vs {at_radius}"
        );
        assert_eq!(at_radius, outside);
    }

    #[test]
    fn longitude_noise_is_seamless() {
        let source = ProceduralTerrainSource::default();
        let a = source.height_m(10.0, 179.5);
        let b = source.height_m(10.0, -179.5);
        // Points 1° apart across the ±180° seam stay continuous (3D noise).
        assert!((a - b).abs() < 800.0, "seam discontinuity: {a} vs {b}");
        // ...and are far closer than two distant longitudes.
        let far = source.height_m(10.0, 20.0);
        assert!(
            (a - far).abs() > 5.0,
            "expected terrain to vary between distant longitudes: {a} vs {far}"
        );
    }

    #[test]
    fn domain_warped_height_stays_deterministic_and_bounded() {
        // Adjacent points are continuous; continental and rolling fBm remain
        // bounded before the regional mountain contribution is added.
        let source = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let a = source.height_m(36.5, -90.4);
        let b = source.height_m(36.5, -90.4);
        assert_eq!(a, b, "height must be deterministic");
        let h = source.height_m(10.0, 20.0);
        assert!(h.is_finite());
        let bounds = source.elevation_bounds_m();
        for (la, lo) in [(-20.0, 30.0), (50.0, -120.0), (0.0, 0.0), (80.0, 90.0)] {
            let v = source.height_m(la, lo);
            assert!(
                v >= bounds.min_m && v <= bounds.max_m,
                "height {v} exceeded envelope {bounds:?} at ({la},{lo})"
            );
        }
    }

    #[test]
    fn continental_orogeny_excludes_oceanic_mountains() {
        let source = ProceduralTerrainSource::from_config(ProceduralTerrainConfig::new(
            99, 2_000.0, 800.0, 0,
        ));
        let mut has_oceanic_region = false;
        let mut has_mountain_region = false;

        for latitude_deg in (-80..=80).step_by(10) {
            for longitude_deg in (-180..180).step_by(10) {
                let direction = ProceduralTerrainSource::direction(
                    f64::from(latitude_deg),
                    f64::from(longitude_deg),
                );
                let continent = source.continental_mask(direction);
                let mountain = source.mountain_region_mask(direction, continent);
                let established_land = ss(0.52, 0.68, continent);
                assert!(mountain <= established_land + 1e-12);
                has_oceanic_region |= continent < 0.001 && mountain == 0.0;
                if continent <= 0.52 {
                    assert_eq!(
                        mountain, 0.0,
                        "submerged continental shelf must not receive mountain uplift"
                    );
                }
                has_mountain_region |= mountain > 0.1;
            }
        }

        assert!(
            has_oceanic_region,
            "expected an oceanic region without mountains"
        );
        assert!(has_mountain_region, "expected a continental mountain belt");
    }

    #[test]
    #[should_panic(expected = "rolling terrain amplitude")]
    fn terrain_config_rejects_negative_rolling_amplitude() {
        let _ = ProceduralTerrainConfig::new(1, -1.0, 1_200.0, 0);
    }

    #[test]
    fn moisture_is_normalized_deterministic() {
        let source = ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0);
        for (la, lo) in [(0.0, 0.0), (30.0, -60.0), (-45.0, 120.0)] {
            let m = source.moisture(la, lo);
            assert!((0.0..=1.0).contains(&m), "moisture {m} out of range");
            assert_eq!(m, source.moisture(la, lo));
        }
    }

    #[test]
    fn surface_appearance_varies_continuously() {
        // Wet grassland → arid desert: distinct albedo.
        let wet = surface_appearance(300.0, 0.8, 0.5, 5.0);
        let dry = surface_appearance(300.0, 0.05, 0.5, 5.0);
        assert_ne!(wet.albedo, dry.albedo, "moisture must change the biome");
        // Snow line shifts toward white above 5 km, and is rougher below.
        let low = surface_appearance(1_000.0, 0.5, 0.5, 10.0);
        let high = surface_appearance(5_500.0, 0.5, 0.5, 10.0);
        assert!(
            high.albedo[0] > 0.75 && high.albedo[1] > 0.8,
            "snow must be near-white"
        );
        assert!(high.roughness < low.roughness, "snow is less rough");
        // Steep faces read as bare rock.
        let cliff = surface_appearance(1_000.0, 0.5, 0.5, 60.0);
        assert!(
            (cliff.albedo[0] - 0.20).abs() < 0.08 && (cliff.albedo[1] - 0.18).abs() < 0.08,
            "cliff should trend toward rocky grey {:?}",
            cliff.albedo
        );
        // Seafloor below sea level.
        assert_eq!(surface_appearance(-100.0, 0.5, 0.5, 0.0).metallic, 0.0);
    }

    #[test]
    fn vegetation_density_tracks_climate_not_rendered_color() {
        let detail = ProceduralDetailSource::new(0x00E4_27A6);
        // Humid tropics outrank the dry subtropical belt regardless of how the
        // albedo happens to render there.
        let tropics = detail.vegetation_density(-8.0, 139.5);
        let subtropics = detail.vegetation_density(25.0, 10.0);
        assert!(
            tropics > subtropics,
            "tropical lowland {tropics} should exceed dry subtropics {subtropics}"
        );
        assert!((0.0..=1.0).contains(&tropics));
    }

    #[test]
    fn grassland_uses_a_natural_non_pastel_green_reflectance() {
        let grassland = surface_appearance(300.0, 0.5, 0.5, 5.0);

        assert!(grassland.albedo[1] > grassland.albedo[0]);
        assert!(grassland.albedo[1] > grassland.albedo[2]);
        assert!(
            grassland.albedo[1] < 0.25,
            "grass reflectance should remain physically subdued: {:?}",
            grassland.albedo
        );
    }

    #[test]
    fn snow_line_descends_toward_the_poles() {
        // The same altitude is bare ground at the equator and snow-covered near
        // the pole because the snow line drops with latitude.
        let equatorial = surface_appearance(1_200.0, 0.5, 0.5, 5.0);
        let polar = surface_appearance(1_200.0, 0.5, 0.98, 5.0);

        assert!(
            equatorial.albedo[0] < 0.5,
            "equatorial lowland must not be snow: {:?}",
            equatorial.albedo
        );
        assert!(
            polar.albedo[0] > 0.7 && polar.albedo[1] > 0.75,
            "polar terrain must read as snow: {:?}",
            polar.albedo
        );
        assert!(polar.roughness < equatorial.roughness, "snow is less rough");
    }

    #[test]
    fn river_appearance_is_darker_smoother_and_blue_shifted() {
        let ground = surface_appearance(300.0, 0.5, 0.5, 5.0);
        let river = with_river_appearance(ground, 1.0);

        assert!(river.albedo[0] < ground.albedo[0]);
        assert!(river.albedo[2] > river.albedo[1]);
        assert!(river.roughness < ground.roughness);
    }

    #[test]
    fn slope_deg_at_is_finite_and_zero_on_flat() {
        let flat = FlatTerrainSource;
        let s = slope_deg_at(&flat, 0.0, 0.0);
        assert!(s.abs() < 1e-6, "flat terrain must have ~0 slope, got {s}");
        let proc = ProceduralTerrainSource::default();
        assert!(slope_deg_at(&proc, 10.0, 20.0).is_finite());
    }
}
