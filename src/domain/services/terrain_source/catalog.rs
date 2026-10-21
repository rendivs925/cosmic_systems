//! Data-backed planet terrain sources (Earth, Moon, Mars).

#[cfg(feature = "dem")]
use super::{
    DetailLodFade, LayeredTerrainSource, PadFlatZone, ProceduralDetailSource, SurfaceClass,
    TerrainDetailLayer, TerrainElevationLayer,
};
use super::{ElevationBounds, TerrainSource};
#[cfg(not(feature = "dem"))]
use super::{ProceduralTerrainConfig, ProceduralTerrainSource};
use crate::domain::services::cube_sphere::{PatchGeometricError, TerrainPatch};
#[cfg(feature = "dem")]
use crate::domain::services::dem_terrain_source::{DemError, DemTerrainSource};
use crate::domain::services::elevation_pyramid::TileElevationSource;
#[cfg(feature = "dem")]
use crate::domain::services::elevation_pyramid::{
    ElevationPyramidError, DEFAULT_ELEVATION_TILE_CAPACITY,
};
#[cfg(feature = "dem")]
use crate::domain::services::erosion::{ErodedTerrainSource, ErosionConfig, ErosionTileStore};
#[cfg(feature = "dem")]
use crate::domain::services::global_raster::GlobalRasterSpec;
#[cfg(feature = "dem")]
use crate::domain::services::planet_factory::PlanetFactory;
#[cfg(feature = "dem")]
use crate::domain::services::reference_frames::geodetic_to_terrain_lat_lon;
#[cfg(feature = "dem")]
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
#[cfg(feature = "dem")]
use crate::domain::value_objects::launch_site_coordinates::predefined_sites;
#[cfg(feature = "dem")]
use std::env;
#[cfg(feature = "dem")]
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Default measured Earth height package path.
#[cfg(feature = "dem")]
pub const DEFAULT_EARTH_DEM_PATH: &str =
    "assets/large_files/terrain/earth_etopo2022_30s_ice_surface_cs2048_v3.csdem";
/// EGM2008 geoid undulation grid used to convert Earth's ellipsoidal terrain
/// heights back to sea-level-relative values for classification and water.
#[cfg(feature = "dem")]
pub const DEFAULT_EARTH_GEOID_PATH: &str =
    "assets/large_files/terrain/earth_egm2008_geoid_0p5deg_i16cm.raw";
/// Default optional high-resolution elevation payload package root. A missing
/// package is not an error: Earth falls back to the resident measured CSDEM.
#[cfg(feature = "dem")]
pub const DEFAULT_EARTH_ELEVATION_MANIFEST_ROOT: &str =
    "assets/large_files/terrain/earth_elevation_pyramid";
#[cfg(feature = "dem")]
pub const DEFAULT_MOON_DEM_PATH: &str =
    "assets/large_files/terrain/moon_lola_ldem_16_cs2048_v2.csdem";
#[cfg(feature = "dem")]
pub const DEFAULT_MARS_DEM_PATH: &str =
    "assets/large_files/terrain/mars_mola_megr_32_cs2048_v2.csdem";

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

/// Namespace that scopes Earth's preprocessed erosion tiles to this measured
/// base. Changing the DEM path invalidates any persisted tiles.
#[cfg(feature = "dem")]
pub const EARTH_EROSION_NAMESPACE: &str = DEFAULT_EARTH_DEM_PATH;

/// Default ceiling for the on-disk erosion tile cache. The cache is a
/// regenerable preprocessed artifact, so it is bounded rather than allowed to
/// grow without limit. Override with `COSMIC_SYSTEMS_EROSION_CACHE_MAX_BYTES`.
#[cfg(feature = "dem")]
pub const DEFAULT_EROSION_CACHE_MAX_BYTES: u64 = 4 << 30;

/// Earth's declared, validated erosion configuration. It shares the planet's
/// deterministic seed so a baked and a runtime-eroded tile are identical. The
/// quality preset is selectable with `COSMIC_SYSTEMS_EROSION_QUALITY`
/// (`standard`, `high`, or `ultra`).
#[cfg(feature = "dem")]
pub fn earth_erosion_config() -> ErosionConfig {
    let quality = env::var("COSMIC_SYSTEMS_EROSION_QUALITY").unwrap_or_default();
    let mut cfg = ErosionConfig::from_quality_name(&quality);
    cfg.seed = EARTH_EROSION_SEED;
    // Residency only bounds the in-memory LRU; it never changes the raster, so
    // it is a runtime knob rather than part of the bake signature.
    if let Some(tiles) = env::var("COSMIC_SYSTEMS_EROSION_CACHE_TILES")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|tiles| *tiles > 0)
    {
        cfg.cache_max_tiles = tiles;
    }
    cfg
}

/// The preprocessed erosion store for a configuration, or `None` when caching
/// is disabled. `COSMIC_SYSTEMS_EROSION_CACHE` selects the directory (`off`
/// disables it); otherwise a per-user cache directory is used. Tests never
/// touch the disk.
#[cfg(feature = "dem")]
pub fn erosion_store(cfg: &ErosionConfig, namespace: &str) -> Option<Arc<ErosionTileStore>> {
    if cfg!(test) {
        return None;
    }
    let root = match env::var("COSMIC_SYSTEMS_EROSION_CACHE") {
        Ok(value) if value.trim().eq_ignore_ascii_case("off") => return None,
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value.trim()),
        _ => default_erosion_cache_root()?,
    };
    let signature = cfg.bake_signature(namespace);
    match ErosionTileStore::open_with_budget(root, signature, erosion_cache_max_bytes()) {
        Ok(store) => Some(Arc::new(store)),
        Err(error) => {
            eprintln!("erosion cache unavailable, continuing without preprocessed tiles: {error}");
            None
        }
    }
}

/// The configured disk ceiling for the erosion tile cache.
#[cfg(feature = "dem")]
fn erosion_cache_max_bytes() -> u64 {
    env::var("COSMIC_SYSTEMS_EROSION_CACHE_MAX_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|bytes| *bytes > 0)
        .unwrap_or(DEFAULT_EROSION_CACHE_MAX_BYTES)
}

#[cfg(feature = "dem")]
fn default_erosion_cache_root() -> Option<PathBuf> {
    if let Ok(xdg) = env::var("XDG_CACHE_HOME") {
        if !xdg.trim().is_empty() {
            return Some(PathBuf::from(xdg.trim()).join("cosmic_systems/erosion"));
        }
    }
    env::var("HOME")
        .ok()
        .filter(|home| !home.trim().is_empty())
        .map(|home| PathBuf::from(home.trim()).join(".cache/cosmic_systems/erosion"))
}

/// Wrap an Earth elevation layer in the one authoritative erosion/hydrology
/// field. The cached `ErodedTerrainSource` samples the eroded height plus D8
/// flow, moisture, and river strength; it is the only erosion implementation.
/// When a preprocessed store is available, tiles are loaded from disk instead
/// of being simulated at runtime.
#[cfg(feature = "dem")]
pub fn earth_eroded_base(base: Arc<dyn TerrainSource>) -> Arc<dyn TerrainSource> {
    let cfg = earth_erosion_config();
    let store = erosion_store(&cfg, EARTH_EROSION_NAMESPACE);
    Arc::new(ErodedTerrainSource::with_store(base, cfg, store))
}

/// Compose Earth's measured base terrain with the bounded procedural detail
/// contribution. Rendering, collision, radar altitude, and biome metadata all
/// sample this one surface; the detail is deterministic and never changes with
/// camera movement or frame rate. The measured ETOPO 2022 package stays the base
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
/// resident measured ETOPO 2022 height package as the base surface and add a
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
                DemTerrainSource::from_path_with_geoid(
                    DEFAULT_EARTH_DEM_PATH,
                    DEFAULT_EARTH_GEOID_PATH,
                    GlobalRasterSpec::egm2008_geoid_0p5deg(),
                )
                .unwrap_or_else(|error| {
                    panic!("Earth measured terrain authority is unavailable or invalid: {error}")
                }),
            );
            let fallback = Arc::clone(&measured);
            let root = Path::new(DEFAULT_EARTH_ELEVATION_MANIFEST_ROOT);
            Self::assemble(measured, Some(root)).unwrap_or_else(|_| Self {
                source: Arc::new(earth_layered_terrain(fallback)),
                tiles: None,
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

    /// Construct Earth terrain from a validated CSDEM and the geoid grid that
    /// defines sea level for its ellipsoidal heights.
    #[cfg(feature = "dem")]
    pub fn with_dem_path_and_geoid(
        path: impl AsRef<Path>,
        geoid_path: impl AsRef<Path>,
    ) -> Result<Self, DemError> {
        let base: Arc<dyn TerrainSource> = Arc::new(DemTerrainSource::from_path_with_geoid(
            path,
            geoid_path,
            GlobalRasterSpec::egm2008_geoid_0p5deg(),
        )?);
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
        Self::assemble(base, Some(manifest_root.as_ref()))
    }

    /// Build the single authority from a measured global source and an optional
    /// payload package root. The payload, when present, sits over the measured
    /// base; the resident CSDEM remains authoritative where no payload tile is
    /// resident.
    #[cfg(feature = "dem")]
    fn assemble(
        measured_global: Arc<dyn TerrainSource>,
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
        Ok(Self {
            source: Arc::new(earth_layered_terrain(global)),
            tiles,
        })
    }
}

/// Failures when assembling Earth's reviewed global elevation data.
#[cfg(feature = "dem")]
#[derive(Debug)]
pub enum EarthTerrainDataError {
    GlobalDem(DemError),
    ElevationPyramid(ElevationPyramidError),
}

#[cfg(feature = "dem")]
impl From<DemError> for EarthTerrainDataError {
    fn from(error: DemError) -> Self {
        Self::GlobalDem(error)
    }
}

#[cfg(feature = "dem")]
impl From<ElevationPyramidError> for EarthTerrainDataError {
    fn from(error: ElevationPyramidError) -> Self {
        Self::ElevationPyramid(error)
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

    fn sea_level_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.source.sea_level_height_m(latitude_deg, longitude_deg)
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
