//! Data-backed planet terrain sources (Earth, Moon, Mars) and the local
//! elevation overlay.

#[cfg(feature = "dem")]
use super::{
    DetailLodFade, LayeredTerrainSource, PadFlatZone, ProceduralDetailSource, SurfaceClass,
    TerrainDetailLayer, TerrainElevationLayer,
};
use super::{ElevationBounds, TerrainSource};
#[cfg(not(feature = "dem"))]
use super::{ProceduralTerrainConfig, ProceduralTerrainSource};
#[cfg(feature = "dem")]
use crate::domain::math::DVec3;
#[cfg(feature = "dem")]
use crate::domain::services::cube_sphere::face_uv;
use crate::domain::services::cube_sphere::{PatchGeometricError, TerrainPatch};
#[cfg(feature = "dem")]
use crate::domain::services::dem_terrain_source::{DemError, DemTerrainSource};
use crate::domain::services::elevation_pyramid::TileElevationSource;
#[cfg(feature = "dem")]
use crate::domain::services::elevation_pyramid::{
    ElevationPyramidError, DEFAULT_ELEVATION_TILE_CAPACITY,
};
#[cfg(feature = "dem")]
use crate::domain::services::erosion::{ErodedTerrainSource, ErosionConfig};
#[cfg(feature = "dem")]
use crate::domain::services::local_elevation::{LocalElevationError, LocalElevationPackage};
#[cfg(feature = "dem")]
use crate::domain::services::planet_factory::PlanetFactory;
#[cfg(feature = "dem")]
use crate::domain::services::reference_frames::geodetic_to_terrain_lat_lon;
#[cfg(feature = "dem")]
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
#[cfg(feature = "dem")]
use crate::domain::value_objects::launch_site_coordinates::predefined_sites;
#[cfg(feature = "dem")]
use std::path::Path;
use std::sync::Arc;

/// Default measured Earth height package path.
#[cfg(feature = "dem")]
pub const DEFAULT_EARTH_DEM_PATH: &str =
    "assets/large_files/terrain/earth_etopo1_ice_surface_cs2048_v2.csdem";
/// Default optional high-resolution elevation payload package root. A missing
/// package is not an error: Earth falls back to the resident measured CSDEM.
#[cfg(feature = "dem")]
pub const DEFAULT_EARTH_ELEVATION_MANIFEST_ROOT: &str =
    "assets/large_files/terrain/earth_elevation_pyramid";
#[cfg(feature = "dem")]
const DEFAULT_MOON_DEM_PATH: &str = "assets/large_files/terrain/moon_lola_ldem_16_cs2048_v2.csdem";
#[cfg(feature = "dem")]
const DEFAULT_MARS_DEM_PATH: &str = "assets/large_files/terrain/mars_mola_megr_32_cs2048_v2.csdem";

#[cfg(feature = "dem")]
impl LocalElevationOverlayTerrainSource {
    fn local_intersects_patch(&self, patch: &TerrainPatch) -> bool {
        let (west, south, east, north) = self.local.coverage_bounds_deg();
        let mut bounds: Option<(f64, f64, f64, f64)> = None;
        for (latitude_deg, longitude_deg) in
            [(south, west), (south, east), (north, west), (north, east)]
        {
            let latitude_rad = latitude_deg.to_radians();
            let longitude_rad = longitude_deg.to_radians();
            let direction = DVec3::new(
                latitude_rad.cos() * longitude_rad.cos(),
                latitude_rad.sin(),
                latitude_rad.cos() * longitude_rad.sin(),
            );
            let (face, u, v) = face_uv(direction);
            if face != patch.face {
                return true;
            }
            bounds = Some(match bounds {
                Some((u0, v0, u1, v1)) => (u0.min(u), v0.min(v), u1.max(u), v1.max(v)),
                None => (u, v, u, v),
            });
        }
        let Some((u0, v0, u1, v1)) = bounds else {
            return true;
        };
        let (patch_u0, patch_v0, patch_u1, patch_v1) = patch.uv_bounds();
        u0 <= patch_u1 && u1 >= patch_u0 && v0 <= patch_v1 && v1 >= patch_v0
    }
}

/// Deterministic seed for Earth's procedural detail contribution.
#[cfg(feature = "dem")]
const EARTH_PROCEDURAL_DETAIL_SEED: u64 = 0x00E4_27A6_1E5E_ED01;
/// Deterministic master seed for Earth's thermal/hydraulic erosion field.
#[cfg(feature = "dem")]
const EARTH_EROSION_SEED: u64 = 0x00E4_2705_E0D1_5EED;
/// Radius around a launch site where the procedural detail is graded flat.
#[cfg(feature = "dem")]
const EARTH_PAD_FLAT_RADIUS_M: f64 = 750.0;
/// Radius over which the graded pad blends back into natural relief.
#[cfg(feature = "dem")]
const EARTH_PAD_BLEND_RADIUS_M: f64 = 3_000.0;

/// Earth's declared, validated erosion configuration. It shares the planet's
/// deterministic seed so a baked and a runtime-eroded tile are identical.
#[cfg(feature = "dem")]
pub(crate) fn earth_erosion_config() -> ErosionConfig {
    ErosionConfig {
        seed: EARTH_EROSION_SEED,
        ..ErosionConfig::default()
    }
}

/// Wrap an Earth elevation layer in the one authoritative erosion/hydrology
/// field. The cached `ErodedTerrainSource` samples the eroded height plus D8
/// flow, moisture, and river strength; it is the only erosion implementation.
#[cfg(feature = "dem")]
pub(crate) fn earth_eroded_base(base: Arc<dyn TerrainSource>) -> Arc<dyn TerrainSource> {
    Arc::new(ErodedTerrainSource::new(base, earth_erosion_config()))
}

/// Compose Earth's measured base terrain with the bounded procedural detail
/// contribution. Rendering, collision, radar altitude, and biome metadata all
/// sample this one surface; the detail is deterministic and never changes with
/// camera movement or frame rate. The measured ETOPO1 package stays the base
/// elevation authority, erosion is applied to that base, and the detail is
/// documented presentation-scale relief.
#[cfg(feature = "dem")]
fn earth_layered_terrain(base: Arc<dyn TerrainSource>) -> LayeredTerrainSource {
    let base_bounds = base.elevation_bounds_m();
    let eroded = earth_eroded_base(base);
    let detail: Arc<dyn TerrainSource> = Arc::new(ProceduralDetailSource::with_flat_zones(
        EARTH_PROCEDURAL_DETAIL_SEED,
        earth_pad_flat_zones(),
    ));
    LayeredTerrainSource::new(
        TerrainElevationLayer::new(eroded, base_bounds),
        None,
        Some(TerrainDetailLayer::new(
            detail,
            ProceduralDetailSource::elevation_bounds_m(),
            DetailLodFade::new(11, 14),
        )),
    )
}

/// Level pads at the predefined Earth launch sites. The sites are declared in
/// geodetic coordinates, so they are converted to the terrain-radial
/// convention that the height function samples before becoming flat zones.
#[cfg(feature = "dem")]
fn earth_pad_flat_zones() -> Vec<PadFlatZone> {
    let Some(earth) = PlanetFactory::create_by_id(&CelestialBodyId::earth()) else {
        return Vec::new();
    };
    let radius_m = earth.radius_km as f64 * 1_000.0;
    [
        predefined_sites::kennedy_space_center(),
        predefined_sites::papua_indonesia_coastal_lowland(),
    ]
    .into_iter()
    .map(|site| {
        let (latitude_deg, longitude_deg) = geodetic_to_terrain_lat_lon(&site, &earth);
        PadFlatZone::new(
            latitude_deg,
            longitude_deg,
            radius_m,
            EARTH_PAD_FLAT_RADIUS_M,
            EARTH_PAD_BLEND_RADIUS_M,
        )
    })
    .collect()
}

/// Earth's one authoritative terrain composition. Native builds require the
/// resident measured ETOPO1 height package as the base surface and add a
/// deterministic bounded procedural detail layer; rendering and collision use
/// the same source with no runtime download.
///
/// When a valid elevation payload package is present, it is composed *behind*
/// the same authority: the resident CSDEM is the streamed base and installed
/// payload tiles override it only where they are resident. The optional
/// `tiles` handle is shared with the streaming layer; it is never a second
/// terrain authority.
#[derive(Debug)]
pub struct EarthTerrainSource {
    source: Arc<dyn TerrainSource>,
    tiles: Option<Arc<TileElevationSource>>,
}

impl EarthTerrainSource {
    pub fn new() -> Self {
        #[cfg(feature = "dem")]
        {
            // The resident CSDEM is mandatory. The payload pyramid is optional:
            // a missing or invalid package silently falls back to the current
            // base+detail composition so no mode depends on the new data.
            let measured: Arc<dyn TerrainSource> = Arc::new(
                DemTerrainSource::from_path(DEFAULT_EARTH_DEM_PATH).unwrap_or_else(|error| {
                    panic!("Earth measured terrain authority is unavailable or invalid: {error}")
                }),
            );
            let root = Path::new(DEFAULT_EARTH_ELEVATION_MANIFEST_ROOT);
            Self::assemble(measured, None, Some(root)).unwrap_or_else(|_| {
                let measured: Arc<dyn TerrainSource> = Arc::new(
                    DemTerrainSource::from_path(DEFAULT_EARTH_DEM_PATH)
                        .expect("resident Earth DEM was validated above"),
                );
                Self {
                    source: Arc::new(earth_layered_terrain(measured)),
                    tiles: None,
                }
            })
        }
        #[cfg(not(feature = "dem"))]
        Self {
            source: Arc::new(ProceduralTerrainSource::from_config(
                ProceduralTerrainConfig::earth(),
            )),
            tiles: None,
        }
    }

    /// Construct Earth terrain from a validated local cube-sphere DEM. The
    /// caller chooses this configuration at startup; it preserves measured
    /// elevations without adding a synthetic landscape or local-detail layer.
    #[cfg(feature = "dem")]
    pub fn with_dem_path(path: impl AsRef<Path>) -> Result<Self, DemError> {
        let base: Arc<dyn TerrainSource> = Arc::new(DemTerrainSource::from_path(path)?);
        Ok(Self {
            source: Arc::new(earth_layered_terrain(base)),
            tiles: None,
        })
    }

    /// Construct Earth terrain from a validated CSDEM plus an explicit
    /// elevation payload package root. Unlike [`Self::new`], an invalid or
    /// missing package is an error: callers that name a package must get it.
    #[cfg(feature = "dem")]
    pub fn with_dem_and_elevation_manifest(
        global_dem_path: impl AsRef<Path>,
        manifest_root: impl AsRef<Path>,
    ) -> Result<Self, EarthTerrainDataError> {
        let base: Arc<dyn TerrainSource> = Arc::new(DemTerrainSource::from_path(global_dem_path)?);
        Self::assemble(base, None, Some(manifest_root.as_ref()))
    }

    /// Use a reviewed local elevation package as an absolute replacement within
    /// its coverage. Its samples must already use the Earth's terrain datum;
    /// this constructor does not transform horizontal or vertical reference
    /// frames. The global DEM remains authoritative outside local coverage.
    #[cfg(feature = "dem")]
    pub fn with_dem_and_local_elevation_paths(
        global_dem_path: impl AsRef<Path>,
        local_elevation_path: impl AsRef<Path>,
    ) -> Result<Self, EarthTerrainDataError> {
        let global: Arc<dyn TerrainSource> =
            Arc::new(DemTerrainSource::from_path(global_dem_path)?);
        let local = load_local_elevation(local_elevation_path)?;
        Self::assemble(global, Some(local), None)
    }

    /// Compose the reviewed measured local package over the global streamed
    /// coverage. Local samples override both the resident base and any resident
    /// payload tile within their footprint, with the package's declared
    /// continuous blend border. Payload tiles remain authoritative elsewhere.
    #[cfg(feature = "dem")]
    pub fn with_dem_local_and_elevation_manifest_paths(
        global_dem_path: impl AsRef<Path>,
        local_elevation_path: impl AsRef<Path>,
        manifest_root: impl AsRef<Path>,
    ) -> Result<Self, EarthTerrainDataError> {
        let global: Arc<dyn TerrainSource> =
            Arc::new(DemTerrainSource::from_path(global_dem_path)?);
        let local = load_local_elevation(local_elevation_path)?;
        Self::assemble(global, Some(local), Some(manifest_root.as_ref()))
    }

    /// Build the single authority from a measured global source, an optional
    /// reviewed local overlay, and an optional payload package root. The
    /// payload, when present, sits between the measured base and the local
    /// overlay so local measured data always wins where it is covered.
    #[cfg(feature = "dem")]
    fn assemble(
        measured_global: Arc<dyn TerrainSource>,
        local: Option<Arc<LocalElevationPackage>>,
        manifest_root: Option<&Path>,
    ) -> Result<Self, EarthTerrainDataError> {
        let (global, tiles) = match manifest_root {
            Some(root) => {
                let tiles = Arc::new(TileElevationSource::from_manifest_root(
                    measured_global,
                    root,
                    DEFAULT_ELEVATION_TILE_CAPACITY,
                )?);
                (Arc::clone(&tiles) as Arc<dyn TerrainSource>, Some(tiles))
            }
            None => (measured_global, None),
        };
        let authority: Arc<dyn TerrainSource> = match local {
            Some(local) => Arc::new(LocalElevationOverlayTerrainSource { global, local }),
            None => global,
        };
        Ok(Self {
            source: Arc::new(earth_layered_terrain(authority)),
            tiles,
        })
    }
}

/// Validate and load a reviewed local elevation package as Earth coverage.
#[cfg(feature = "dem")]
fn load_local_elevation(
    local_elevation_path: impl AsRef<Path>,
) -> Result<Arc<LocalElevationPackage>, EarthTerrainDataError> {
    let local = LocalElevationPackage::from_path(local_elevation_path)?;
    if local.metadata().body != "Earth" {
        return Err(EarthTerrainDataError::InvalidLocalElevation(
            "local elevation package body must be Earth".into(),
        ));
    }
    if local.elevation_bounds_m().is_none() {
        return Err(EarthTerrainDataError::InvalidLocalElevation(
            "local elevation package contains no valid samples".into(),
        ));
    }
    Ok(Arc::new(local))
}

/// Failures when assembling Earth's reviewed global and local elevation data.
#[cfg(feature = "dem")]
#[derive(Debug)]
pub enum EarthTerrainDataError {
    GlobalDem(DemError),
    LocalElevation(LocalElevationError),
    ElevationPyramid(ElevationPyramidError),
    InvalidLocalElevation(String),
}

#[cfg(feature = "dem")]
impl From<DemError> for EarthTerrainDataError {
    fn from(error: DemError) -> Self {
        Self::GlobalDem(error)
    }
}

#[cfg(feature = "dem")]
impl From<LocalElevationError> for EarthTerrainDataError {
    fn from(error: LocalElevationError) -> Self {
        Self::LocalElevation(error)
    }
}

#[cfg(feature = "dem")]
impl From<ElevationPyramidError> for EarthTerrainDataError {
    fn from(error: ElevationPyramidError) -> Self {
        Self::ElevationPyramid(error)
    }
}

/// Selects measured, datum-normalized local elevation over the global source.
/// The global side is any authoritative source, including the tile-backed
/// composition, so streamed payload tiles remain authoritative outside the
/// local footprint.
#[cfg(feature = "dem")]
#[derive(Debug)]
pub(crate) struct LocalElevationOverlayTerrainSource {
    pub(crate) global: Arc<dyn TerrainSource>,
    pub(crate) local: Arc<LocalElevationPackage>,
}

#[cfg(feature = "dem")]
impl TerrainSource for LocalElevationOverlayTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let global = self.global.height_m(latitude_deg, longitude_deg);
        let Some(local) = self.local.sample_m(latitude_deg, longitude_deg) else {
            return global;
        };
        let border_deg = self.local.metadata().blend_border_m / 111_320.0;
        if border_deg == 0.0 {
            return local;
        }
        let edge_distance = self.local.edge_distance_deg(latitude_deg, longitude_deg);
        let t = (edge_distance / border_deg).clamp(0.0, 1.0);
        global + (local - global) * (t * t * (3.0 - 2.0 * t))
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        let global = self.global.elevation_bounds_m();
        let Some((local_min_m, local_max_m)) = self.local.elevation_bounds_m() else {
            return global;
        };
        ElevationBounds::new(global.min_m.min(local_min_m), global.max_m.max(local_max_m))
    }

    fn patch_geometric_error(&self, patch: &TerrainPatch) -> PatchGeometricError {
        if !self.local_intersects_patch(patch) {
            return self.global.patch_geometric_error(patch);
        }
        let bounds = self.elevation_bounds_m();
        PatchGeometricError::from_elevation_bounds(bounds.min_m, bounds.max_m)
    }

    fn surface_class(&self, latitude_deg: f64, longitude_deg: f64) -> SurfaceClass {
        if self.height_m(latitude_deg, longitude_deg) <= 0.0 {
            SurfaceClass::Ocean
        } else {
            SurfaceClass::Land
        }
    }
}

impl Default for EarthTerrainSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Moon terrain is an unlayered, data-backed LOLA CSDEM. Unlike Earth, it has
/// no procedural local-detail or ocean model: its downloaded DEM is the only
/// physical surface authority.
#[cfg(feature = "dem")]
#[derive(Debug)]
pub struct MoonTerrainSource {
    pub(crate) source: Arc<DemTerrainSource>,
}

#[cfg(feature = "dem")]
impl MoonTerrainSource {
    /// Load the reviewed local LOLA CSDEM. Missing data is surfaced to startup
    /// composition so the Moon remains non-landable rather than procedural.
    pub fn new() -> Result<Self, DemError> {
        Self::with_dem_path(DEFAULT_MOON_DEM_PATH)
    }

    pub fn with_dem_path(path: impl AsRef<Path>) -> Result<Self, DemError> {
        Ok(Self {
            source: Arc::new(DemTerrainSource::from_path(path)?),
        })
    }
}

#[cfg(feature = "dem")]
impl TerrainSource for MoonTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.source.height_m(latitude_deg, longitude_deg)
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        self.source.elevation_bounds_m()
    }

    fn patch_geometric_error(&self, patch: &TerrainPatch) -> PatchGeometricError {
        self.source.patch_geometric_error(patch)
    }

    fn surface_class(&self, _latitude_deg: f64, _longitude_deg: f64) -> SurfaceClass {
        SurfaceClass::Land
    }
}

/// Mars terrain is an unlayered, data-backed MOLA CSDEM. Its source radius is
/// converted to the catalog mean-radius datum before runtime loading, so terrain
/// meshes and collision use the same physical surface.
#[cfg(feature = "dem")]
#[derive(Debug)]
pub struct MarsTerrainSource {
    pub(crate) source: Arc<DemTerrainSource>,
}

#[cfg(feature = "dem")]
impl MarsTerrainSource {
    pub fn new() -> Result<Self, DemError> {
        Self::with_dem_path(DEFAULT_MARS_DEM_PATH)
    }

    pub fn with_dem_path(path: impl AsRef<Path>) -> Result<Self, DemError> {
        Ok(Self {
            source: Arc::new(DemTerrainSource::from_path(path)?),
        })
    }
}

#[cfg(feature = "dem")]
impl TerrainSource for MarsTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.source.height_m(latitude_deg, longitude_deg)
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        self.source.elevation_bounds_m()
    }

    fn patch_geometric_error(&self, patch: &TerrainPatch) -> PatchGeometricError {
        self.source.patch_geometric_error(patch)
    }

    fn surface_class(&self, _latitude_deg: f64, _longitude_deg: f64) -> SurfaceClass {
        SurfaceClass::Land
    }
}

impl TerrainSource for EarthTerrainSource {
    fn vegetation_density(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.source.vegetation_density(latitude_deg, longitude_deg)
    }

    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.source.height_m(latitude_deg, longitude_deg)
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        self.source.elevation_bounds_m()
    }

    fn patch_geometric_error(&self, patch: &TerrainPatch) -> PatchGeometricError {
        self.source.patch_geometric_error(patch)
    }

    fn elevation_tile_source(&self) -> Option<Arc<TileElevationSource>> {
        self.tiles.clone()
    }

    fn mesh_height_m(&self, latitude_deg: f64, longitude_deg: f64, patch_level: u32) -> f64 {
        self.source
            .mesh_height_m(latitude_deg, longitude_deg, patch_level)
    }

    fn prepare_sample(&self, latitude_deg: f64, longitude_deg: f64) {
        self.source.prepare_sample(latitude_deg, longitude_deg);
    }

    fn moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.source.moisture(latitude_deg, longitude_deg)
    }

    fn river_strength(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.source.river_strength(latitude_deg, longitude_deg)
    }

    fn overview_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.source.overview_height_m(latitude_deg, longitude_deg)
    }

    fn overview_moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.source.overview_moisture(latitude_deg, longitude_deg)
    }

    fn overview_slope_deg(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.source.overview_slope_deg(latitude_deg, longitude_deg)
    }

    fn zone_lat(&self, latitude_deg: f64) -> f64 {
        self.source.zone_lat(latitude_deg)
    }
}
