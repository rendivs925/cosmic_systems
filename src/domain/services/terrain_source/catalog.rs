//! Data-backed planet terrain sources (Earth, Moon, Mars).

#[cfg(feature = "dem")]
use super::{
    DetailLodFade, FarDetailLayer, LayeredTerrainSource, PadFlatZone, PadTrenchZone,
    ProceduralDetailSource, ProceduralFarDetailSource, SurfaceClass, TerrainDetailLayer,
    TerrainElevationLayer, LAUNCH_TRENCH_BLEND_M, LAUNCH_TRENCH_DEPTH_M, LAUNCH_TRENCH_FAR_Z_M,
    LAUNCH_TRENCH_HALF_WIDTH_M, LAUNCH_TRENCH_NEAR_Z_M,
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
use crate::domain::value_objects::launch_site_coordinates::{
    predefined_sites, LaunchSiteCoordinates, PredefinedLaunchSite,
};
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
/// Default elevation package roots, composed in order. Later roots refine
/// earlier ones at identical patch addresses. The current package keeps every
/// reviewed region under one root; separate roots are supported for future
/// independently versioned regional packages.
#[cfg(feature = "dem")]
pub const DEFAULT_EARTH_ELEVATION_MANIFEST_ROOTS: &[&str] = &[
    DEFAULT_EARTH_ELEVATION_MANIFEST_ROOT,
    "assets/large_files/terrain/earth_elevation_pyramid_ksc",
    "assets/large_files/terrain/earth_elevation_pyramid_ksc_3dep",
    "assets/large_files/terrain/earth_elevation_pyramid_starbase",
    "assets/large_files/terrain/earth_elevation_pyramid_vandenberg",
    "assets/large_files/terrain/earth_elevation_pyramid_wallops",
    "assets/large_files/terrain/earth_elevation_pyramid_kourou",
    "assets/large_files/terrain/earth_elevation_pyramid_baikonur",
    "assets/large_files/terrain/earth_elevation_pyramid_mahia",
];
#[cfg(feature = "dem")]
pub const DEFAULT_MOON_DEM_PATH: &str =
    "assets/large_files/terrain/moon_lola_ldem_128_cs4096_v1.csdem";
#[cfg(feature = "dem")]
pub const DEFAULT_MARS_DEM_PATH: &str =
    "assets/large_files/terrain/mars_mola_megr_64_cs4096_v1.csdem";

/// Deterministic seed for Earth's procedural detail contribution.
#[cfg(feature = "dem")]
const EARTH_PROCEDURAL_DETAIL_SEED: u64 = 0x00E4_27A6_1E5E_ED01;
/// Deterministic master seed for Earth's thermal/hydraulic erosion field.
#[cfg(feature = "dem")]
const EARTH_EROSION_SEED: u64 = 0x00E4_2705_E0D1_5EED;
/// Deterministic seed for the coarse, mesh-only far relief band (TM-02).
#[cfg(feature = "dem")]
const EARTH_FAR_DETAIL_SEED: u64 = 0x00E4_27A6_1E5E_FADE;
/// Radius around a launch site where the composed surface is graded level.
///
/// This is the prepared-platform footprint, not a visual fade: it has to contain
/// the facility and its apron (about 50 m across) and no more. It was 750 m,
/// which flattened a 1.5 km pancake around every site; 250 m then still left a
/// visible pale disc, because levelling the ground changes the slope-derived
/// ground material over the whole graded footprint. 110 m keeps the clearing
/// inside the pad and its immediate surroundings.
#[cfg(feature = "dem")]
const EARTH_PAD_FLAT_RADIUS_M: f64 = 110.0;

/// Minimum height of a launch platform above the local sea datum. The measured
/// ground at Starbase and Papua is under a metre above the waterline, so a
/// level grade alone still leaves the tower in the sea; a pad is a
/// construction, not a terrain sample (TD-11).
pub const EARTH_SITE_PLATFORM_CLEARANCE_M: f64 = 3.0;
/// Radius over which the graded pad blends back into natural relief.
#[cfg(feature = "dem")]
const EARTH_PAD_BLEND_RADIUS_M: f64 = 280.0;

/// Height above the local sea datum below which Earth's synthetic detail may
/// add relief but must not lower the ground (TD-06).
///
/// The detail octave is signed (`−36..+24 m`), and ETOPO on a coastal plain sits
/// only a metre or two above the datum, so the negative half would push land
/// below sea level and the ocean cap would render it as open water. Below this
/// height every negative octave is removed.
#[cfg(feature = "dem")]
const EARTH_DETAIL_SEA_GUARD_M: f64 = 4.0;

/// Height above the local sea datum at which the negative detail octave is
/// fully restored.
///
/// This is not a visual fade: it is bounded by the layer's own negative
/// amplitude. The removal weight is a smoothstep over
/// `[guard, full]`, so the residual negative contribution is at most
/// `amplitude * (1 - smoothstep)`. Keeping `full - guard` at least
/// `1.5 * amplitude` (here `54 m`) makes that residual fall faster than the
/// base height rises, so the composed surface cannot cross the datum anywhere
/// in the window (see `sea_level_guard_cannot_lower_ground_below_the_datum`).
#[cfg(feature = "dem")]
const EARTH_DETAIL_SEA_GUARD_FULL_M: f64 = 60.0;

/// Namespace that scopes Earth's preprocessed erosion tiles to this measured
/// base *and* bake semantics. Changing the DEM path, or changing what the
/// rasters are simulated from, invalidates any persisted tiles. Erosion is
/// baked from the stable global measured DEM; resident regional payloads are a
/// sampling overlay, so their load order must never enter the bake identity.
#[cfg(feature = "dem")]
pub fn earth_erosion_namespace() -> String {
    format!("{DEFAULT_EARTH_DEM_PATH}#bake-base=global-measured-v1")
}

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
    let bake_base = Arc::clone(&base);
    earth_eroded_base_over(base, bake_base)
}

/// Erosion wrapped around a sampling `base` but baked from an immutable
/// `bake_base`.
///
/// The distinction matters when `base` carries a streamed measured payload
/// overlay: a resident payload should refine the *sample*, but the erosion
/// raster must come from a surface that cannot change during the run, or its
/// height, moisture, flow and river channels would depend on which tiles
/// happened to be loaded when the tile was first touched.
#[cfg(feature = "dem")]
fn earth_eroded_base_over(
    base: Arc<dyn TerrainSource>,
    bake_base: Arc<dyn TerrainSource>,
) -> Arc<dyn TerrainSource> {
    let cfg = earth_erosion_config();
    let store = erosion_store(&cfg, &earth_erosion_namespace());
    Arc::new(ErodedTerrainSource::with_store_over_bake_base(
        base, bake_base, cfg, store,
    ))
}

/// Compose Earth's measured base terrain with the bounded procedural detail
/// contribution. Rendering, collision, radar altitude, and biome metadata all
/// sample this one surface; the detail is deterministic and never changes with
/// camera movement or frame rate. The measured ETOPO 2022 package stays the base
/// elevation authority, erosion is applied to that base, and the detail is
/// documented presentation-scale relief.
#[cfg(feature = "dem")]
fn earth_layered_terrain(base: Arc<dyn TerrainSource>) -> LayeredTerrainSource {
    earth_layered_terrain_over(Arc::clone(&base), base)
}

/// The same composition with a distinct immutable erosion bake base.
#[cfg(feature = "dem")]
fn earth_layered_terrain_over(
    base: Arc<dyn TerrainSource>,
    bake_base: Arc<dyn TerrainSource>,
) -> LayeredTerrainSource {
    let base_bounds = base.elevation_bounds_m();
    let eroded = earth_eroded_base_over(base, bake_base);
    let detail: Arc<dyn TerrainSource> = Arc::new(ProceduralDetailSource::with_flat_zones(
        EARTH_PROCEDURAL_DETAIL_SEED,
        earth_pad_flat_zones(),
    ));
    LayeredTerrainSource::new(
        TerrainElevationLayer::new(eroded, base_bounds),
        None,
        Some(
            TerrainDetailLayer::new(
                detail,
                ProceduralDetailSource::elevation_bounds_m(),
                DetailLodFade::new(11, 14),
            )
            // Presentation relief may raise a coastal plain but must not lower it
            // into the ocean (TD-06). The guard is per-layer because only a body
            // with an ocean can be flooded by a negative octave.
            .with_sea_level_guard(EARTH_DETAIL_SEA_GUARD_M, EARTH_DETAIL_SEA_GUARD_FULL_M),
        ),
    )
    // A coarse relief band for distant ground, present only on the mesh at
    // LODs where ~4 km features resolve. Collision keeps the measured surface
    // (TM-02); the sea guard keeps it from inventing coastline.
    .with_far_detail(
        FarDetailLayer::new(
            Arc::new(ProceduralFarDetailSource::with_flat_zones(
                EARTH_FAR_DETAIL_SEED,
                earth_pad_flat_zones(),
            )),
            ProceduralFarDetailSource::elevation_bounds_m(),
            DetailLodFade::new(5, 8),
            DetailLodFade::new(9, 11),
        )
        .with_sea_level_guard(EARTH_DETAIL_SEA_GUARD_M, EARTH_DETAIL_SEA_GUARD_FULL_M),
    )
    // Grade the launch-site platforms in the composed surface itself, so the
    // mesh, collision and radar all see level ground under the facility instead
    // of a flat apron drawn over a slope.
    .with_graded_sites(earth_pad_flat_zones())
    .with_site_trenches(earth_pad_trenches())
    .with_coastal_platform_clearance(EARTH_SITE_PLATFORM_CLEARANCE_M)
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
    // Every predefined site, not only the three the vehicle currently launches
    // from: a facility floats wherever the ground under it has not been graded,
    // and the grade pins each site to its own composed height, so adding a site
    // cannot move that site's launch elevation.
    earth_pad_sites()
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

/// The single list of Earth sites that carry a presented launch facility. The
/// grade and the excavation must cover exactly the same sites, or a facility
/// stands on ungraded ground at one of them.
#[cfg(feature = "dem")]
fn earth_pad_sites() -> [LaunchSiteCoordinates; 9] {
    [
        predefined_sites::kennedy_space_center(),
        predefined_sites::kennedy_lc39a(),
        predefined_sites::starbase_boca_chica(),
        predefined_sites::vandenberg_california(),
        predefined_sites::wallops_virginia(),
        predefined_sites::kourou_french_guiana(),
        predefined_sites::baikonur_kazakhstan(),
        predefined_sites::mahia_new_zealand(),
        predefined_sites::papua_indonesia_coastal_lowland(),
    ]
}

/// Flame-trench excavations at the same graded Earth launch sites.
///
/// The rectangle is expressed in the pad's local frame (`+X` east, `+Z` north),
/// which is the frame the presentation facility is built in, so the visible
/// opening and the excavated terrain cannot drift apart. The near edge clears
/// the vehicle base and its hold-down clamps: the pad carries no collision
/// surface of its own, so the launch point must stay on grade and the resting
/// height — and every flight baseline — is unchanged.
#[cfg(feature = "dem")]
fn earth_pad_trenches() -> Vec<PadTrenchZone> {
    let Some(earth) = PlanetFactory::create_by_id(&CelestialBodyId::earth()) else {
        return Vec::new();
    };
    let radius_m = earth.radius_km as f64 * 1_000.0;
    earth_pad_sites()
        .into_iter()
        .map(|site| {
            let (latitude_deg, longitude_deg) = geodetic_to_terrain_lat_lon(&site, &earth);
            PadTrenchZone::new(
                latitude_deg,
                longitude_deg,
                radius_m,
                (-LAUNCH_TRENCH_HALF_WIDTH_M, LAUNCH_TRENCH_HALF_WIDTH_M),
                (LAUNCH_TRENCH_NEAR_Z_M, LAUNCH_TRENCH_FAR_Z_M),
                LAUNCH_TRENCH_DEPTH_M,
                LAUNCH_TRENCH_BLEND_M,
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
            let roots: Vec<PathBuf> = DEFAULT_EARTH_ELEVATION_MANIFEST_ROOTS
                .iter()
                .map(PathBuf::from)
                .filter(|root| root.exists())
                .collect();
            if roots.is_empty() {
                return Self {
                    source: Arc::new(earth_layered_terrain(fallback)),
                    tiles: None,
                };
            }
            Self::assemble_roots(measured, &roots).unwrap_or_else(|_| Self {
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

    /// Construct Earth terrain from a validated CSDEM plus several elevation
    /// package roots. Roots are composed in order; later roots refine earlier
    /// ones at identical patch addresses. Unlike [`Self::new`], an invalid or
    /// missing package is an error.
    #[cfg(feature = "dem")]
    pub fn with_dem_and_elevation_manifest_roots<I, P>(
        global_dem_path: impl AsRef<Path>,
        manifest_roots: I,
    ) -> Result<Self, EarthTerrainDataError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let base: Arc<dyn TerrainSource> = Arc::new(DemTerrainSource::from_path(global_dem_path)?);
        let roots: Vec<PathBuf> = manifest_roots
            .into_iter()
            .map(|root| root.as_ref().to_path_buf())
            .collect();
        Self::assemble_roots(base, &roots)
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
        match manifest_root {
            Some(root) => Self::assemble_roots(measured_global, &[root.to_path_buf()]),
            None => Ok(Self {
                source: Arc::new(earth_layered_terrain(measured_global)),
                tiles: None,
            }),
        }
    }

    #[cfg(feature = "dem")]
    fn assemble_roots(
        measured_global: Arc<dyn TerrainSource>,
        manifest_roots: &[PathBuf],
    ) -> Result<Self, EarthTerrainDataError> {
        // Erosion bakes from the stable global measured surface; the payload
        // pyramid is a resident overlay on top of it. Passing the tiled source
        // as the bake base made the surrounding field depend on payload load
        // order and on the erosion cache.
        let bake_base = Arc::clone(&measured_global);
        let tiles = Arc::new(TileElevationSource::from_manifest_roots(
            measured_global,
            manifest_roots,
            DEFAULT_ELEVATION_TILE_CAPACITY,
        )?);
        preload_launch_site_payloads(&tiles);
        Ok(Self {
            source: Arc::new(earth_layered_terrain_over(
                Arc::clone(&tiles) as Arc<dyn TerrainSource>,
                bake_base,
            )),
            tiles: Some(tiles),
        })
    }
}

/// Load the covering payload tile for every predefined launch site before the
/// graded platform anchors are computed.
///
/// `with_graded_sites` samples each site's composed height at construction.
/// Erosion and detail stand back where a measured payload is resident (see
/// `TerrainSource::resident_measured_height_m`), so the anchor is only the
/// payload's own probe if that tile is resident *now*. Waiting for streaming to
/// load it would anchor the pad to the coarse global fallback and put several
/// coastal sites under water. Nine tiles is a bounded, one-time startup cost.
#[cfg(feature = "dem")]
fn preload_launch_site_payloads(tiles: &TileElevationSource) {
    let Some(earth) = PlanetFactory::create_by_id(&CelestialBodyId::earth()) else {
        return;
    };
    for site in PredefinedLaunchSite::ALL {
        // Use the site's exact declared coordinates: the grade anchor samples
        // the same conversion, so the tile must cover the same point.
        let (latitude_deg, longitude_deg) =
            geodetic_to_terrain_lat_lon(&site.coordinates(), &earth);
        if let Err(error) = tiles.ensure_covering_tile_loaded(latitude_deg, longitude_deg) {
            eprintln!(
                "{} launch elevation payload failed to preload: {error:?}",
                site.label()
            );
        }
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

#[cfg(all(test, feature = "dem"))]
mod tests {
    use super::*;
    use crate::domain::services::reference_frames::terrain_lat_lon_to_body_fixed;

    /// Load the global albedo overview as the authoritative equirectangular
    /// sampler, so the transect uses the same pixel-centre wrap/clamp rules as
    /// the renderer.
    fn global_overview_rgb() -> crate::domain::services::imagery_tiles::EquirectangularRgb {
        let image = image::open("assets/large_files/terrain/earth_imagery_v1/global_overview.png")
            .expect("the global albedo overview must load")
            .to_rgba8();
        let (width, height) = image.dimensions();
        crate::domain::services::imagery_tiles::EquirectangularRgb::new(
            width,
            height,
            image.pixels().map(|pixel| pixel.0).collect(),
        )
        .expect("overview raster")
    }

    /// TM-03: the geometric waterline (DEM below the datum) and the imagery
    /// coastline must agree within one overview texel. The Great Australian
    /// Bight is a clean, straight cliff coast: open water is almost black and
    /// inland ground is bright, so the imagery waterline is the midpoint of the
    /// luminance ramp rather than a tuned colour threshold.
    #[test]
    fn coastal_transect_dem_and_imagery_agree() {
        let source = EarthTerrainSource::new();
        let overview = global_overview_rgb();

        let latitude_deg = -32.0;
        let start_lon = 132.0;
        let step_deg = 0.001;
        let samples = 200;
        let mut rows = Vec::new();
        for index in 0..samples {
            let longitude_deg = start_lon + f64::from(index) * step_deg;
            let sea_level_m = source.sea_level_height_m(latitude_deg, longitude_deg);
            let height_m = source.height_m(latitude_deg, longitude_deg) - sea_level_m;
            let pixel = overview.sample_lat_lon(latitude_deg, longitude_deg);
            let luminance = (f64::from(pixel[0]) + f64::from(pixel[1]) + f64::from(pixel[2])) / 3.0;
            rows.push((longitude_deg, height_m, luminance));
        }

        let median = |values: &mut Vec<f64>| {
            values.sort_by(f64::total_cmp);
            values[values.len() / 2]
        };
        let mut water_luminance: Vec<f64> = rows.iter().take(20).map(|row| row.2).collect();
        let mut land_luminance: Vec<f64> = rows.iter().rev().take(20).map(|row| row.2).collect();
        let water = median(&mut water_luminance);
        let land = median(&mut land_luminance);
        assert!(
            land > water + 20.0,
            "the transect must cross a strong water/land contrast: {water} vs {land}"
        );
        let midpoint = 0.5 * (water + land);

        let dem_coast_deg = rows
            .iter()
            .find(|row| row.1 > 0.0)
            .map(|row| row.0)
            .expect("the transect crosses onto land");
        let imagery_coast_deg = rows
            .iter()
            .find(|row| row.2 >= midpoint)
            .map(|row| row.0)
            .expect("the imagery crosses onto land");
        let overview_texel_deg = 360.0 / 8192.0;
        assert!(
            (dem_coast_deg - imagery_coast_deg).abs() <= overview_texel_deg,
            "DEM coast {dem_coast_deg:.4} vs imagery coast {imagery_coast_deg:.4} \
             (one overview texel is {overview_texel_deg:.4})"
        );
    }

    /// TM-02: the coarse relief band is part of the rendered mesh at the LODs
    /// where a few kilometres resolve, and absent from the physical surface and
    /// from the planet-scale / fine-LOD meshes.
    #[test]
    fn earth_far_detail_changes_only_the_coarse_mesh() {
        let source = EarthTerrainSource::new();
        let (latitude_deg, longitude_deg) = (-5.0, 141.5);
        let physical_m = source.height_m(latitude_deg, longitude_deg);
        assert_eq!(
            source.mesh_height_m(latitude_deg, longitude_deg, 14),
            physical_m,
            "fine LOD must match collision"
        );
        let coarse_m = source.mesh_height_m(latitude_deg, longitude_deg, 9);
        assert!(
            (coarse_m - physical_m).abs() > 1.0,
            "distant geometry must keep relief: {coarse_m} vs {physical_m} m"
        );
    }

    /// TD-12: the angular water bodies on the Papua plain. The finest measured
    /// payload puts the observed strip a few centimetres below the EGM2008 sea
    /// datum, and the source classifies it as ocean. The eroded base at the
    /// same coordinates is metres *above* the datum, so erosion did not carve
    /// it and the water is measurement, not a synthetic basin.
    #[test]
    fn papua_near_datum_water_is_measured_payload_not_erosion() {
        let composed = EarthTerrainSource::new();
        let tiles = composed
            .elevation_tile_source()
            .expect("Earth exposes a payload pyramid");
        let global: Arc<dyn TerrainSource> = Arc::new(
            DemTerrainSource::from_path_with_geoid(
                DEFAULT_EARTH_DEM_PATH,
                DEFAULT_EARTH_GEOID_PATH,
                GlobalRasterSpec::egm2008_geoid_0p5deg(),
            )
            .expect("the resident measured authority must load"),
        );
        let eroded = earth_eroded_base(global);

        for (latitude_deg, longitude_deg) in [(-8.01912, 139.49950), (-8.01651, 139.49950)] {
            tiles
                .ensure_covering_tile_loaded(latitude_deg, longitude_deg)
                .expect("the regional payload must decode");
            let direction = terrain_lat_lon_to_body_fixed(latitude_deg, longitude_deg);
            let measured_m = tiles
                .sample_resident(direction)
                .expect("the finest covering payload is resident");
            let sea_level_m = composed.sea_level_height_m(latitude_deg, longitude_deg);
            let composed_m = composed.height_m(latitude_deg, longitude_deg);
            let eroded_m = eroded.height_m(latitude_deg, longitude_deg);

            // Recorded composition at the observed water:
            // measured 71.363 m, sea 71.406-71.408 m, eroded ~+4 m over sea,
            // composed = measured (the payload overlay outranks erosion).
            assert!(
                (composed_m - measured_m).abs() < 1e-3,
                "the composed surface must be the measured payload: {composed_m} vs {measured_m}"
            );
            assert_eq!(
                composed.surface_class(latitude_deg, longitude_deg),
                SurfaceClass::Ocean
            );
            let below_m = sea_level_m - composed_m;
            assert!(
                (0.0..0.1).contains(&below_m),
                "the measured depression is centimetres, not a carved basin: {below_m} m"
            );
            assert!(
                eroded_m > sea_level_m + 1.0,
                "erosion must not carve this below the datum: {} m over sea",
                eroded_m - sea_level_m
            );
        }
    }
}
