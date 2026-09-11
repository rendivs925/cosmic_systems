//! Cube-sphere terrain streaming (AGENTS.md sections 22-23).
//!
//! A `TerrainPatchManager` resource is driven each tick by a streaming system
//! that keeps a complete coarse cube-sphere cover with viewport-wide refinement through the
//! requested → generating → ready → visible → cached → evicted lifecycle, and
//! enforces the configured memory budget by evicting least-recently-used cached
//! patches. Generated patch geometry is built deterministically from the shared
//! per-planet `TerrainSource`. Coarse roots provide fallback coverage for the
//! active viewport; only its visible terrain is refined.

use crate::domain::services::body_orientation::BodyOrientation;
use crate::domain::services::cube_sphere::{
    build_patch_geometry_with_stitches, direction_to_lat_lon, face_uv_to_direction,
    projected_patch_error_px, select_quadtree_leaves, CameraProjection, PatchEdge, PatchGeometry,
    QuadtreePatchState, QuadtreeSelectionConfig, TerrainPatch,
};
use crate::domain::services::reference_frames::{
    body_fixed_to_planet_inertial_rotation, planet_inertial_to_body_fixed,
};
use crate::domain::services::terrain_patch_manager::{PatchState, TerrainPatchManager};
use crate::domain::services::terrain_source::{ElevationBounds, TerrainSource};
use crate::infrastructure::bevy_adapters::entity_components::*;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::performance_components::PerformanceMetricsConfig;
use crate::infrastructure::bevy_adapters::rocket::components::{
    RocketMissionState, RocketPhysicsState, RocketPlanetBinding, SpentStage,
};
use crate::infrastructure::bevy_adapters::rocket::contact::TerrainSurfaceSampleCache;
use crate::infrastructure::bevy_adapters::terrain::performance::TerrainPerformanceTelemetry;
use crate::infrastructure::bevy_adapters::terrain::render::{
    RenderOrigin, TerrainPatchCached, TerrainPatchEvicted, TerrainPatchReady, TerrainRenderConfig,
};
use crate::infrastructure::bevy_adapters::terrain::surface::{
    prepare_patch_surface, supports_local_surfaces, supports_vegetation, PreparedPatchSurface,
    LOCAL_SURFACE_MAP_BYTES, MAX_VEGETATION_MESH_BYTES,
};
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use bevy::{math::DVec3, prelude::*};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

/// Camera/LOD constants.
/// The initial hierarchy keeps global roots inexpensive while allowing local
/// detail down to roughly 2.4 km patches at Earth scale. Higher-resolution DEM
/// and imagery work can raise this only after profiling the visible leaf budget.
const MAX_PATCH_LEVEL: u32 = 14;
const FOV_RAD: f64 = 1.0;
const SCREEN_HEIGHT_PX: f64 = 1080.0;
const SCREEN_ERROR_PX: f64 = 4.0;
/// Domain geometry retains f64 position/normal and two UV sets. The renderer
/// creates a second f32 mesh with position, normal, two UVs, and vertex color,
/// so budget both copies.
const DOMAIN_BYTES_PER_VERTEX: u64 = 64;
const RENDER_BYTES_PER_VERTEX: u64 = 56;
const BYTES_PER_INDEX: u64 = 4;
const DEFAULT_BUDGET_BYTES: u64 = 128 * 1024 * 1024;
const METRICS_GENERATED_TILE_INTERVAL: usize = 32;
/// Minimum distance for LOD calculation when on the ground.
/// Uses estimated camera-to-terrain distance (~150m) instead of orbital heuristic.
const SURFACE_LOD_DISTANCE_M: f64 = 150.0;
/// Altitude threshold below which surface LOD distance is used.
const SURFACE_LOD_ALTITUDE_THRESHOLD_M: f64 = 10_000.0;
/// Extra frustum angle retained around the viewport so terrain does not pop at
/// its edge while the camera is moving between streaming updates.
const VIEWPORT_PREFETCH_MARGIN_RAD: f64 = 0.2;
/// Admit a bounded pair of CPU terrain bakes per presentation frame. This fills
/// the reserved worker pool quickly after a camera move without queueing an
/// unbounded cold-start burst or blocking the render thread.
const MAX_TERRAIN_TASKS_PER_FRAME: usize = 2;
/// Bound active coverage, including its protected progressive fallback chain,
/// to the 128 MiB cache budget. The previous 512-leaf limit could make active
/// coverage alone exceed that budget, leaving LRU eviction with no legal
/// candidate during an orbital camera view.
const MAX_VIEWPORT_TARGET_LEAVES: usize = 300;
/// Reserve leaf budget for 2:1 neighbour balancing at the viewport boundary.
/// The visible traversal itself stops below the published-cover limit.
const MAX_VIEWPORT_UNBALANCED_LEAVES: usize = MAX_VIEWPORT_TARGET_LEAVES - 64;
/// Full quadtree reconciliation is bounded to this rate while async job polling
/// remains per-frame. Camera movement beyond the thresholds below bypasses it.
const STREAM_RECONCILE_INTERVAL_S: f64 = 1.0 / 30.0;
const FOCUS_RECONCILE_ANGLE_RAD: f64 = 0.01;
const VIEWPORT_POSITION_RECONCILE_RATIO: f64 = 0.01;
const LOD_HYSTERESIS_RATIO: f64 = 0.2;
/// Smooth `[0,1]` ramp used to blend the on-ground and orbital LOD distances so
/// the terrain LOD level steps gradually rather than jumping several at once.
fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bevy resource wrapping the domain streaming manager plus the generated
/// patch geometry cache.
#[derive(Resource)]
pub struct TerrainStreamingResource {
    pub manager: TerrainPatchManager,
    pub budget_bytes: u64,
    pub generated: HashMap<TerrainPatch, CachedTerrainGeometry>,
    /// Background geometry jobs for requested tiles. Every LOD, including roots,
    /// samples the authoritative source off the render thread.
    inflight: BTreeMap<TerrainPatch, InflightTerrainPatch>,
    /// Leaves currently published to the renderer. Generated descendants remain
    /// cached until all siblings are ready, then replace their parent together.
    pub published: BTreeSet<TerrainPatch>,
    /// Previous desired cover used to hold per-node split/merge decisions
    /// inside the hysteresis band.
    target_leaves: BTreeSet<TerrainPatch>,
    /// Planet that owns every entry in this cache. Patch coordinates alone are
    /// not sufficient when a rocket changes its bound celestial body.
    active_planet: Option<Entity>,
    /// Next generated-tile count at which streaming metrics are reported.
    next_metrics_report_at: usize,
    cadence: TerrainStreamingCadence,
}

/// Startup warmup work remains off the main thread. Holding the tasks until
/// completion prevents their cancellation while the first presentation frame
/// is being prepared.
#[derive(Resource, Default)]
pub struct TerrainWarmupTasks(Vec<Task<()>>);

/// Prime the shared terrain evaluator before first presentation. `sample` also
/// computes the source normal, which exercises the same height probes used by
/// ground contact while preallocating the exact 512-entry contact cache.
pub fn warmup_terrain_system(
    surface_cache: Res<TerrainSurfaceSampleCache>,
    planet_query: Query<(Entity, &PlanetComponent, &PlanetTerrain)>,
    mut warmup_tasks: ResMut<TerrainWarmupTasks>,
) {
    if !warmup_tasks.0.is_empty() {
        return;
    }
    let task_pool = AsyncComputeTaskPool::get();
    for (planet_entity, planet, terrain) in &planet_query {
        let radius_m = planet.domain_planet.radius_km as f64 * 1_000.0;
        let source = terrain.source.clone();
        let surface_cache = surface_cache.clone();
        warmup_tasks.0.push(task_pool.spawn(async move {
            surface_cache.sample(planet_entity, source.as_ref(), 0.0, 0.0, radius_m);
        }));
    }
}

/// Drop completed startup warmup tasks without blocking presentation.
pub fn collect_terrain_warmup_tasks(mut warmup_tasks: ResMut<TerrainWarmupTasks>) {
    warmup_tasks
        .0
        .retain_mut(|task| block_on(future::poll_once(task)).is_none());
}

impl Default for TerrainStreamingResource {
    fn default() -> Self {
        Self {
            manager: TerrainPatchManager::new(),
            budget_bytes: DEFAULT_BUDGET_BYTES,
            generated: HashMap::new(),
            inflight: BTreeMap::new(),
            published: BTreeSet::new(),
            target_leaves: TerrainPatch::roots().into_iter().collect(),
            active_planet: None,
            next_metrics_report_at: 0,
            cadence: TerrainStreamingCadence::default(),
        }
    }
}

impl TerrainStreamingResource {
    /// The planet that owns `generated` and `published` terrain patches.
    pub fn active_planet(&self) -> Option<Entity> {
        self.active_planet
    }

    fn begin_bake(&mut self, request: TerrainPatchBakeRequest, task_pool: &AsyncComputeTaskPool) {
        self.manager.begin_generation(&request.patch);
        self.inflight
            .insert(request.patch, request.spawn(task_pool));
    }

    fn collect_completed_generation(&mut self) -> TerrainGenerationBatch {
        let completed: Vec<_> = self
            .inflight
            .iter_mut()
            .filter_map(|(patch, inflight)| {
                block_on(future::poll_once(&mut inflight.task)).map(|generated| (*patch, generated))
            })
            .collect();
        let mut batch = TerrainGenerationBatch::default();
        for (patch, generated) in completed {
            self.inflight.remove(&patch);
            self.generated.insert(
                patch,
                CachedTerrainGeometry {
                    geometry: generated.geometry,
                    surface: Some(generated.surface),
                    stitch_mask: generated.stitch_mask,
                },
            );
            self.manager.mark_ready(&patch);
            batch.record(generated.generation_ms);
        }
        batch
    }

    fn cancel_unrequested(&mut self, requested: &BTreeSet<TerrainPatch>) -> TerrainCancellation {
        let stale_inflight: Vec<_> = self
            .inflight
            .keys()
            .copied()
            .filter(|patch| !requested.contains(patch))
            .collect();
        let mut cancellation = TerrainCancellation {
            inflight: stale_inflight.len(),
            ..default()
        };
        for patch in stale_inflight {
            self.inflight.remove(&patch);
            self.manager.cancel_pending(&patch);
        }

        let stale_requested: Vec<_> = self
            .manager
            .patch_states()
            .filter_map(|(patch, state)| {
                (state == PatchState::Requested && !requested.contains(&patch)).then_some(patch)
            })
            .collect();
        cancellation.requested = stale_requested.len();
        for patch in stale_requested {
            self.manager.cancel_pending(&patch);
        }
        cancellation
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Metrics intentionally capture the complete cadence-limited streaming snapshot."
    )]
    fn metrics(
        &mut self,
        requested: &BTreeSet<TerrainPatch>,
        target: &BTreeSet<TerrainPatch>,
        completed: TerrainGenerationBatch,
        cancellation: TerrainCancellation,
        evicted_tiles: usize,
        prelaunch: bool,
        focus_max_lod: u32,
        culling: TerrainCullingStats,
        main_thread_ms: f64,
    ) -> Option<TerrainStreamingMetrics> {
        if !completed.is_reportable() || self.generated.len() < self.next_metrics_report_at {
            return None;
        }

        self.next_metrics_report_at = self.generated.len() + METRICS_GENERATED_TILE_INTERVAL;
        Some(TerrainStreamingMetrics::capture(
            self,
            requested,
            target,
            completed,
            cancellation,
            evicted_tiles,
            prelaunch,
            focus_max_lod,
            culling,
            main_thread_ms,
        ))
    }
}

/// Generated geometry is valid only for the LOD stitch pattern used to build
/// its index buffer. Reusing it with a different neighboring LOD would reopen
/// T-junction cracks along the changed edge.
pub struct CachedTerrainGeometry {
    pub geometry: PatchGeometry,
    pub(crate) surface: Option<PreparedPatchSurface>,
    stitch_mask: u8,
}

struct InflightTerrainPatch {
    task: Task<GeneratedTerrainPatch>,
    started_at: Instant,
}

struct TerrainPatchBakeRequest {
    patch: TerrainPatch,
    source: Arc<dyn TerrainSource>,
    radius_m: f64,
    resolution: u32,
    skirt_depth_m: f64,
    stitched_edges: Vec<PatchEdge>,
}

impl TerrainPatchBakeRequest {
    fn spawn(self, task_pool: &AsyncComputeTaskPool) -> InflightTerrainPatch {
        let patch = self.patch;
        let task = task_pool.spawn(async move {
            let generation_started = Instant::now();
            let (lat, lon) = direction_to_lat_lon(patch.center_direction());
            self.source.prepare_sample(lat, lon);
            let geometry = build_streamed_patch_geometry(
                &patch,
                self.source.as_ref(),
                self.radius_m,
                self.resolution,
                self.skirt_depth_m,
                &self.stitched_edges,
            );
            let surface =
                prepare_patch_surface(self.source.as_ref(), &patch, &geometry, self.radius_m);
            GeneratedTerrainPatch {
                geometry,
                surface,
                stitch_mask: stitch_mask(&self.stitched_edges),
                generation_ms: generation_started.elapsed().as_secs_f64() * 1_000.0,
            }
        });
        InflightTerrainPatch {
            task,
            started_at: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct TerrainGenerationBatch {
    completed_tiles: usize,
    generation_ms: f64,
}

impl TerrainGenerationBatch {
    fn record(&mut self, generation_ms: f64) {
        self.completed_tiles += 1;
        self.generation_ms += generation_ms;
    }

    fn is_reportable(self) -> bool {
        self.completed_tiles > 0
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct TerrainCancellation {
    inflight: usize,
    requested: usize,
}

impl TerrainCancellation {
    fn total(self) -> usize {
        self.inflight + self.requested
    }
}

#[derive(Debug)]
struct PatchLevelDistribution(BTreeMap<u32, usize>);

impl PatchLevelDistribution {
    fn from_patches(patches: impl IntoIterator<Item = TerrainPatch>) -> Self {
        let mut levels = BTreeMap::new();
        for patch in patches {
            *levels.entry(patch.level).or_default() += 1;
        }
        Self(levels)
    }
}

#[derive(Debug)]
struct TerrainStreamingMetrics {
    requested_tiles: usize,
    target_tiles: usize,
    visible_tiles: usize,
    blocked_target_tiles: usize,
    generated_tiles: usize,
    resident_tiles: usize,
    upload_backlog_tiles: usize,
    estimated_resident_mib: f64,
    budget_mib: f64,
    inflight_tiles: usize,
    oldest_inflight_ms: f64,
    cancelled_tiles: usize,
    evicted_tiles: usize,
    prelaunch: bool,
    focus_max_lod: u32,
    culling: TerrainCullingStats,
    main_thread_ms: f64,
    requested_lods: PatchLevelDistribution,
    target_lods: PatchLevelDistribution,
    visible_lods: PatchLevelDistribution,
    completed: TerrainGenerationBatch,
}

impl TerrainStreamingMetrics {
    #[expect(
        clippy::too_many_arguments,
        reason = "Metrics intentionally capture the complete cadence-limited streaming snapshot."
    )]
    fn capture(
        streaming: &TerrainStreamingResource,
        requested: &BTreeSet<TerrainPatch>,
        target: &BTreeSet<TerrainPatch>,
        completed: TerrainGenerationBatch,
        cancellation: TerrainCancellation,
        evicted_tiles: usize,
        prelaunch: bool,
        focus_max_lod: u32,
        culling: TerrainCullingStats,
        main_thread_ms: f64,
    ) -> Self {
        let upload_backlog_tiles = streaming
            .generated
            .values()
            .filter(|cached| cached.surface.is_some())
            .count();
        Self {
            requested_tiles: requested.len(),
            target_tiles: target.len(),
            visible_tiles: streaming.published.len(),
            blocked_target_tiles: target.difference(&streaming.published).count(),
            generated_tiles: streaming.generated.len(),
            resident_tiles: streaming.manager.ready_patch_count(),
            upload_backlog_tiles,
            estimated_resident_mib: streaming.manager.resident_bytes() as f64 / (1024.0 * 1024.0),
            budget_mib: streaming.budget_bytes as f64 / (1024.0 * 1024.0),
            inflight_tiles: streaming.inflight.len(),
            oldest_inflight_ms: streaming
                .inflight
                .values()
                .map(|inflight| inflight.started_at.elapsed().as_secs_f64() * 1_000.0)
                .fold(0.0, f64::max),
            cancelled_tiles: cancellation.total(),
            evicted_tiles,
            prelaunch,
            focus_max_lod,
            culling,
            main_thread_ms,
            requested_lods: PatchLevelDistribution::from_patches(requested.iter().copied()),
            target_lods: PatchLevelDistribution::from_patches(target.iter().copied()),
            visible_lods: PatchLevelDistribution::from_patches(streaming.published.iter().copied()),
            completed,
        }
    }

    fn log(&self) {
        info!(
            target: "terrain_streaming",
            requested_tiles = self.requested_tiles,
            target_tiles = self.target_tiles,
            visible_tiles = self.visible_tiles,
            blocked_target_tiles = self.blocked_target_tiles,
            generated_tiles = self.generated_tiles,
            resident_tiles = self.resident_tiles,
            upload_backlog_tiles = self.upload_backlog_tiles,
            estimated_resident_mib = self.estimated_resident_mib,
            budget_mib = self.budget_mib,
            inflight_tiles = self.inflight_tiles,
            oldest_inflight_ms = self.oldest_inflight_ms,
            cancelled_tiles = self.cancelled_tiles,
            evicted_tiles = self.evicted_tiles,
            prelaunch = self.prelaunch,
            focus_max_lod = self.focus_max_lod,
            culling_candidates = self.culling.candidates,
            horizon_rejected = self.culling.horizon_rejected,
            frustum_rejected = self.culling.frustum_rejected,
            main_thread_ms = self.main_thread_ms,
            requested_lods = ?self.requested_lods.0,
            target_lods = ?self.target_lods.0,
            visible_lods = ?self.visible_lods.0,
            completed_batch_tiles = self.completed.completed_tiles,
            completed_batch_ms = self.completed.generation_ms,
            "Terrain streaming metrics"
        );
    }
}

struct GeneratedTerrainPatch {
    geometry: PatchGeometry,
    surface: PreparedPatchSurface,
    stitch_mask: u8,
    generation_ms: f64,
}

/// Camera data in the terrain source's body-fixed frame. Streaming uses this
/// only for presentation culling; terrain geometry remains source-authoritative.
#[derive(Debug, Clone, Copy)]
struct TerrainViewport {
    position_m: DVec3,
    forward: DVec3,
    right: DVec3,
    up: DVec3,
    half_fov_rad: f64,
    vertical_fov_rad: f64,
    viewport_height_px: f64,
    horizontal_tan: f64,
    vertical_tan: f64,
    horizontal_sec: f64,
    vertical_sec: f64,
}

/// Per-reconciliation traversal decisions. This is intentionally a small,
/// cadence-limited diagnostic for deciding whether visibility work is removing
/// enough terrain work to justify its CPU cost.
#[derive(Debug, Clone, Copy, Default)]
struct TerrainCullingStats {
    candidates: usize,
    horizon_rejected: usize,
    frustum_rejected: usize,
}

impl TerrainCullingStats {
    fn record(&mut self, visibility: PatchViewportVisibility) {
        self.candidates += 1;
        match visibility {
            PatchViewportVisibility::Visible => {}
            PatchViewportVisibility::BehindHorizon => self.horizon_rejected += 1,
            PatchViewportVisibility::OutsideFrustum => self.frustum_rejected += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatchViewportVisibility {
    Visible,
    BehindHorizon,
    OutsideFrustum,
}

#[derive(Default)]
struct TerrainStreamingCadence {
    last_reconcile_at_s: f64,
    focus_direction: Option<DVec3>,
    camera_position_m: Option<DVec3>,
    half_fov_rad: Option<f64>,
    max_focus_level: Option<u32>,
}

/// Streaming system: keep viewport-visible root tiles available for the bound
/// planet, refine the rocket-facing neighborhood by projected geometric error,
/// generate deterministic geometry from the shared terrain source, and enforce
/// the memory budget. It only updates the streaming resource; it never writes
/// rendered geometry or the rocket's state.
#[expect(
    clippy::too_many_arguments,
    reason = "This streaming system coordinates independent ECS resources, events, and views."
)]
pub(crate) fn stream_terrain_patches(
    mut streaming: ResMut<TerrainStreamingResource>,
    planet_query: Query<(Entity, &PlanetComponent, &PlanetTerrain)>,
    rocket_query: Query<
        (
            &RocketPlanetBinding,
            &RocketPhysicsState,
            &RocketMissionState,
        ),
        Without<SpentStage>,
    >,
    mut ready_events: MessageWriter<TerrainPatchReady>,
    mut cached_events: MessageWriter<TerrainPatchCached>,
    mut evicted_events: MessageWriter<TerrainPatchEvicted>,
    config: Res<TerrainRenderConfig>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    time: Res<Time>,
    render_origin: Res<RenderOrigin>,
    camera_query: Query<(&Camera, &Transform, &Projection), With<Camera3d>>,
    performance_config: Res<PerformanceMetricsConfig>,
    mut terrain_performance: ResMut<TerrainPerformanceTelemetry>,
) {
    let instrumentation_enabled = performance_config.instrumentation_enabled();
    terrain_performance.begin_frame(instrumentation_enabled);
    let streaming_started = Instant::now();
    // No rocket yet: keep the manager tidy and return.
    let Some((binding, rocket, mission)) = rocket_query.iter().next() else {
        let active_planet = streaming.active_planet.take();
        let evicted = clear_terrain_cache(&mut streaming);
        if let Some(planet_entity) = active_planet {
            for patch in evicted {
                evicted_events.write(TerrainPatchEvicted {
                    patch,
                    planet_entity,
                });
            }
        }
        return;
    };
    let Some((planet_entity, planet, planet_terrain)) = planet_query
        .iter()
        .find(|(_, planet, _)| planet.matches_body(&binding.planet_name))
    else {
        let active_planet = streaming.active_planet.take();
        let evicted = clear_terrain_cache(&mut streaming);
        if let Some(planet_entity) = active_planet {
            for patch in evicted {
                evicted_events.write(TerrainPatchEvicted {
                    patch,
                    planet_entity,
                });
            }
        }
        return;
    };

    if streaming
        .active_planet
        .is_some_and(|active| active != planet_entity)
    {
        let previous_planet = streaming.active_planet.take().expect("checked above");
        for patch in clear_terrain_cache(&mut streaming) {
            evicted_events.write(TerrainPatchEvicted {
                patch,
                planet_entity: previous_planet,
            });
        }
    }
    streaming.active_planet = Some(planet_entity);

    let completion_poll_started = instrumentation_enabled.then(Instant::now);
    let completed_batch = streaming.collect_completed_generation();
    if let (Some(started), Some(record)) = (
        completion_poll_started,
        terrain_performance.current_mut(instrumentation_enabled),
    ) {
        record.completion_poll_ms += started.elapsed().as_secs_f64() * 1_000.0;
        record.tasks_completed += completed_batch.completed_tiles;
    }

    let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
    let elevation_bounds = planet_terrain.source.elevation_bounds_m();

    let position_m = rocket.dynamics.position_m;
    let r = position_m.length();
    if r < 1e-6 {
        return;
    }
    // Terrain source coordinates are planet body-fixed geographic coordinates;
    // the rocket state remains planet-centered inertial everywhere else.
    let Some(orientation) =
        ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
    else {
        return;
    };
    let position_bf = planet_inertial_to_body_fixed(position_m, orientation);
    let dir = position_bf.normalize_or_zero();
    let viewport_started = instrumentation_enabled.then(Instant::now);
    let viewport = terrain_viewport(&camera_query, &render_origin, orientation);
    if let (Some(started), Some(record)) = (
        viewport_started,
        terrain_performance.current_mut(instrumentation_enabled),
    ) {
        record.viewport_culling_ms += started.elapsed().as_secs_f64() * 1_000.0;
    }
    let prelaunch = *mission == RocketMissionState::PreLaunch;
    // The presentation camera determines the visible terrain quality. The
    // launch direction is only a fallback while no usable viewport exists.
    let focus_direction = viewport_focus_direction(viewport.as_ref(), radius_m, dir);
    let lod_camera_position_m = viewport
        .as_ref()
        .map(|viewport| viewport.position_m)
        .unwrap_or(position_bf);
    let altitude_m = (lod_camera_position_m.length() - radius_m).max(0.0);

    // Age the LRU on every presentation frame rather than only on the 30 Hz
    // selection cadence. This keeps eviction order representative of actual
    // residency time during a stationary camera view.
    streaming.manager.tick();
    if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
        record.visible_patches = streaming.published.len();
    }
    if !should_reconcile_terrain(
        &streaming.cadence,
        time.elapsed_secs_f64(),
        focus_direction,
        lod_camera_position_m,
        viewport.as_ref().map(|viewport| viewport.half_fov_rad),
        completed_batch.is_reportable(),
    ) {
        return;
    }
    let previous_max_focus_level = streaming.cadence.max_focus_level;
    streaming.cadence = TerrainStreamingCadence {
        last_reconcile_at_s: time.elapsed_secs_f64(),
        focus_direction: Some(focus_direction),
        camera_position_m: Some(lod_camera_position_m),
        half_fov_rad: viewport.as_ref().map(|viewport| viewport.half_fov_rad),
        max_focus_level: previous_max_focus_level,
    };

    // Blend ground and orbital camera distance before evaluating projected
    // error. The selection itself remains a pure f64 domain calculation.
    let ground_distance_m = SURFACE_LOD_DISTANCE_M;
    let orbital_distance_m = (altitude_m + radius_m * 0.05).max(10_000.0);
    let blend = smoothstep(
        SURFACE_LOD_ALTITUDE_THRESHOLD_M * 0.5,
        SURFACE_LOD_ALTITUDE_THRESHOLD_M,
        altitude_m,
    );
    let lod_distance_m =
        ground_distance_m + (orbital_distance_m.max(ground_distance_m) - ground_distance_m) * blend;

    let max_focus_level = lod_for_distance_with_hysteresis(
        streaming.cadence.max_focus_level,
        lod_distance_m,
        radius_m,
    );
    streaming.cadence.max_focus_level = Some(max_focus_level);
    let camera_projection = CameraProjection {
        position_m: lod_camera_position_m,
        vertical_fov_rad: viewport
            .as_ref()
            .map_or(FOV_RAD, |viewport| viewport.vertical_fov_rad),
        viewport_height_px: viewport
            .as_ref()
            .map_or(SCREEN_HEIGHT_PX, |viewport| viewport.viewport_height_px),
    };
    let culling_started = instrumentation_enabled.then(Instant::now);
    let (mut errors, culling) = if let Some(viewport) = viewport.as_ref() {
        projected_errors_for_viewport(
            viewport,
            max_focus_level,
            radius_m,
            camera_projection,
            planet_terrain.source.as_ref(),
            elevation_bounds,
        )
    } else {
        (
            projected_errors_for_focus(
                focus_direction,
                max_focus_level,
                radius_m,
                camera_projection,
                planet_terrain.source.as_ref(),
            ),
            TerrainCullingStats::default(),
        )
    };
    if let (Some(started), Some(record)) = (
        culling_started,
        terrain_performance.current_mut(instrumentation_enabled),
    ) {
        record.viewport_culling_ms += started.elapsed().as_secs_f64() * 1_000.0;
    }
    let lod_selection_started = instrumentation_enabled.then(Instant::now);
    apply_selection_hysteresis(&mut errors, &streaming.target_leaves);
    retain_visible_detail_errors(
        &mut errors,
        &streaming.target_leaves,
        &streaming.generated,
        viewport.as_ref(),
        radius_m,
        elevation_bounds,
    );
    let mut state = QuadtreePatchState {
        ready: streaming.generated.keys().copied().collect(),
        // Roots remain the complete fallback coverage even when the camera is
        // near the surface and child selection is sparse.
        visible: TerrainPatch::roots().into_iter().collect(),
    };
    let mut selection = select_quadtree_leaves(
        &state,
        &errors,
        QuadtreeSelectionConfig {
            max_level: max_focus_level,
            max_projected_error_px: SCREEN_ERROR_PX,
            max_neighbor_level_difference: 1,
        },
    );

    // Geometry caches include the stitch index pattern. Never destroy a
    // published variant during a render handoff: its skirt remains a safe
    // fallback until the patch leaves publication and can be regenerated.
    let stale_stitch_variants = stale_cached_stitch_variants(&streaming, &selection.target_leaves);
    let stale_stitch_variant_count = stale_stitch_variants.len();
    let invalidated_stitch_variants = !stale_stitch_variants.is_empty();
    for patch in stale_stitch_variants {
        streaming.manager.mark_cached(&patch);
        streaming.manager.evict(&patch);
        streaming.generated.remove(&patch);
        evicted_events.write(TerrainPatchEvicted {
            patch,
            planet_entity,
        });
    }
    streaming.manager.sweep_evicted();
    if invalidated_stitch_variants {
        state.ready = streaming.generated.keys().copied().collect();
        selection = select_quadtree_leaves(
            &state,
            &errors,
            QuadtreeSelectionConfig {
                max_level: max_focus_level,
                max_projected_error_px: SCREEN_ERROR_PX,
                max_neighbor_level_difference: 1,
            },
        );
    }
    let lod_splits = selection
        .target_leaves
        .iter()
        .filter(|patch| {
            patch
                .parent()
                .is_some_and(|parent| streaming.target_leaves.contains(&parent))
        })
        .count();
    let lod_merges = streaming
        .target_leaves
        .iter()
        .filter(|patch| {
            patch
                .parent()
                .is_some_and(|parent| selection.target_leaves.contains(&parent))
        })
        .count();
    streaming.target_leaves = selection.target_leaves.clone();
    if let (Some(started), Some(record)) = (
        lod_selection_started,
        terrain_performance.current_mut(instrumentation_enabled),
    ) {
        record.lod_selection_ms += started.elapsed().as_secs_f64() * 1_000.0;
        record.visible_candidates += culling
            .candidates
            .saturating_sub(culling.horizon_rejected + culling.frustum_rejected);
        record.culled_nodes += culling.horizon_rejected + culling.frustum_rejected;
        record.lod_evaluations += errors.len();
        record.lod_splits += lod_splits;
        record.lod_merges += lod_merges;
    }

    // Retain the focused root as an asynchronous fallback even when the launch
    // camera is temporarily blocked by vehicle geometry. This is bounded to one
    // root and prevents a prelaunch view from losing all ground coverage.
    let scheduling_started = instrumentation_enabled.then(Instant::now);
    let focused_root = TerrainPatch::for_direction(focus_direction, 0);
    let mut requested =
        root_requests_for_viewport(focused_root, viewport.as_ref(), radius_m, elevation_bounds);
    for patch in selection.requested.iter().copied().filter(|patch| {
        patch_intersects_viewport(*patch, viewport.as_ref(), radius_m, elevation_bounds)
    }) {
        add_viewport_lod_group(patch, &selection.requested, &mut requested);
    }

    // The camera can move before a queued refinement finishes. Cancel obsolete
    // unfinished tasks now; completed geometry follows the cache lifecycle below
    // so it can still be reused if the patch becomes visible again.
    let cancellation = streaming.cancel_unrequested(&requested);

    // A task may have completed just before the target changed. Keep that
    // geometry reusable, but make it eligible for budget eviction rather than
    // letting an unpublished Ready patch accumulate indefinitely.
    let stale_ready: Vec<_> = streaming
        .manager
        .patch_states()
        .filter_map(|(patch, state)| {
            (state == PatchState::Ready
                && !requested.contains(&patch)
                && !streaming.published.contains(&patch))
            .then_some(patch)
        })
        .collect();
    for patch in stale_ready {
        streaming.manager.mark_cached(&patch);
    }

    let mut duplicate_requests = 0;
    for patch in &requested {
        if streaming.manager.state_of(patch) == Some(PatchState::Requested) {
            duplicate_requests += 1;
        }
        let size_bytes = estimated_patch_bytes(*patch, config.patch_resolution_for(*patch));
        streaming.manager.request(*patch, size_bytes);
    }

    let generation_order = prioritize_generation_requests(
        &requested,
        &streaming.published,
        &selection.target_leaves,
        focus_direction,
        focused_root,
        viewport.as_ref(),
        radius_m,
        elevation_bounds,
    );
    if let (Some(started), Some(record)) = (
        scheduling_started,
        terrain_performance.current_mut(instrumentation_enabled),
    ) {
        record.scheduling_ms += started.elapsed().as_secs_f64() * 1_000.0;
        record.requests += requested.len();
        record.duplicate_requests += duplicate_requests;
        record.tasks_cancelled += cancellation.total();
    }
    let task_admission_started = instrumentation_enabled.then(Instant::now);
    let task_pool = AsyncComputeTaskPool::get();
    let generation_limit = generation_capacity(task_pool.thread_num(), streaming.inflight.len());
    let batch = generation_batch(
        &generation_order,
        &streaming.manager,
        &streaming.generated,
        generation_limit,
    );
    let tasks_started = batch.len();
    for patch in batch {
        let stitch_edges = stitch_edges_for(patch, &selection.target_leaves);
        streaming.begin_bake(
            TerrainPatchBakeRequest {
                patch,
                source: planet_terrain.source.clone(),
                radius_m,
                resolution: config.patch_resolution_for(patch),
                skirt_depth_m: config.skirt_depth_m,
                stitched_edges: stitch_edges,
            },
            task_pool,
        );
    }
    if let (Some(started), Some(record)) = (
        task_admission_started,
        terrain_performance.current_mut(instrumentation_enabled),
    ) {
        record.task_admission_ms += started.elapsed().as_secs_f64() * 1_000.0;
        record.tasks_started += tasks_started;
    }

    // Publish only a complete ready leaf cover. Cached child meshes are never
    // spawned until every sibling can replace the parent, preventing z-fighting
    // and blank-space transitions.
    let publication_started = instrumentation_enabled.then(Instant::now);
    let current_visible: BTreeSet<_> = selection
        .visible_leaves
        .into_iter()
        .filter(|patch| requested.contains(patch))
        .collect();
    let departed: Vec<_> = streaming
        .published
        .difference(&current_visible)
        .copied()
        .collect();
    for patch in departed {
        streaming.manager.mark_cached(&patch);
        cached_events.write(TerrainPatchCached {
            patch,
            planet_entity,
        });
    }
    let arrived: Vec<_> = current_visible
        .difference(&streaming.published)
        .copied()
        .collect();
    let patches_published = arrived.len();
    for patch in arrived {
        streaming.manager.mark_visible(&patch);
        ready_events.write(TerrainPatchReady {
            patch,
            planet_entity,
        });
    }
    streaming.published = current_visible;
    if let (Some(started), Some(record)) = (
        publication_started,
        terrain_performance.current_mut(instrumentation_enabled),
    ) {
        record.publication_ms += started.elapsed().as_secs_f64() * 1_000.0;
        record.visible_patches = streaming.published.len();
        record.patches_published += patches_published;
    }

    let eviction_started = instrumentation_enabled.then(Instant::now);
    let budget = streaming.budget_bytes;
    // The complete requested chain is progressive render fallback. It may
    // temporarily exceed the cache budget but must never be evicted mid-handoff.
    let protected = &requested;
    let evicted = streaming
        .manager
        .enforce_memory_budget_protecting(budget, protected);
    for patch in &evicted {
        streaming.generated.remove(patch);
        evicted_events.write(TerrainPatchEvicted {
            patch: *patch,
            planet_entity,
        });
    }
    if let (Some(started), Some(record)) = (
        eviction_started,
        terrain_performance.current_mut(instrumentation_enabled),
    ) {
        record.eviction_ms += started.elapsed().as_secs_f64() * 1_000.0;
        record.patches_evicted += stale_stitch_variant_count + evicted.len();
    }

    if let Some(metrics) = streaming.metrics(
        &requested,
        &selection.target_leaves,
        completed_batch,
        cancellation,
        evicted.len(),
        prelaunch,
        max_focus_level,
        culling,
        streaming_started.elapsed().as_secs_f64() * 1_000.0,
    ) {
        metrics.log();
    }
}

fn build_streamed_patch_geometry(
    patch: &TerrainPatch,
    source: &dyn TerrainSource,
    radius_m: f64,
    resolution: u32,
    skirt_depth_m: f64,
    stitched_edges: &[PatchEdge],
) -> PatchGeometry {
    // Roots and refinements always use the same source field. Cold-start work
    // remains asynchronous and throttled by the caller.
    build_patch_geometry_with_stitches(
        patch,
        source,
        radius_m,
        resolution,
        skirt_depth_m,
        stitched_edges,
    )
}

fn lod_for_distance_with_hysteresis(previous: Option<u32>, distance_m: f64, radius_m: f64) -> u32 {
    let Some(mut level) = previous else {
        return crate::domain::services::cube_sphere::lod_for_distance(
            distance_m,
            radius_m,
            FOV_RAD,
            SCREEN_HEIGHT_PX,
            SCREEN_ERROR_PX,
            MAX_PATCH_LEVEL,
        );
    };
    while level < MAX_PATCH_LEVEL
        && crate::domain::services::cube_sphere::screen_space_error_m(
            crate::domain::services::cube_sphere::patch_world_size_m(level, radius_m),
            distance_m,
            FOV_RAD,
            SCREEN_HEIGHT_PX,
        ) > SCREEN_ERROR_PX * (1.0 + LOD_HYSTERESIS_RATIO)
    {
        level += 1;
    }
    while level > 0
        && crate::domain::services::cube_sphere::screen_space_error_m(
            crate::domain::services::cube_sphere::patch_world_size_m(level - 1, radius_m),
            distance_m,
            FOV_RAD,
            SCREEN_HEIGHT_PX,
        ) <= SCREEN_ERROR_PX * (1.0 - LOD_HYSTERESIS_RATIO)
    {
        level -= 1;
    }
    level
}

/// Keep terrain generation within the explicit async-worker budget. Submitting
/// more jobs than workers only queues expensive bakes and starves the
/// render/simulation task pools on a cold start.
fn generation_capacity(worker_count: usize, inflight_count: usize) -> usize {
    worker_count
        .saturating_sub(inflight_count)
        .min(MAX_TERRAIN_TASKS_PER_FRAME)
}

fn should_reconcile_terrain(
    cadence: &TerrainStreamingCadence,
    now_s: f64,
    focus_direction: DVec3,
    camera_position_m: DVec3,
    half_fov_rad: Option<f64>,
    completed_generation: bool,
) -> bool {
    if completed_generation || cadence.focus_direction.is_none() {
        return true;
    }
    if now_s - cadence.last_reconcile_at_s >= STREAM_RECONCILE_INTERVAL_S {
        return true;
    }
    if cadence
        .focus_direction
        .is_some_and(|previous| previous.dot(focus_direction) < FOCUS_RECONCILE_ANGLE_RAD.cos())
    {
        return true;
    }
    if cadence.camera_position_m.is_some_and(|previous| {
        previous.distance(camera_position_m)
            > previous.length().max(1.0) * VIEWPORT_POSITION_RECONCILE_RATIO
    }) {
        return true;
    }
    cadence.half_fov_rad != half_fov_rad
}

fn terrain_viewport(
    camera_query: &Query<(&Camera, &Transform, &Projection), With<Camera3d>>,
    render_origin: &RenderOrigin,
    orientation: &BodyOrientation,
) -> Option<TerrainViewport> {
    let (camera, transform, projection) = camera_query
        .iter()
        .find(|(camera, _, _)| camera.is_active)?;
    let vertical_fov_rad = match projection {
        Projection::Perspective(perspective) => perspective.fov as f64,
        _ => return None,
    };
    let aspect_ratio = camera
        .logical_viewport_size()
        .filter(|size| size.y > 0.0)
        .map(|size| (size.x / size.y) as f64)
        .unwrap_or(16.0 / 9.0);
    let viewport_height_px = camera
        .physical_viewport_size()
        .filter(|size| size.y > 0)
        .map_or(SCREEN_HEIGHT_PX, |size| f64::from(size.y));
    let horizontal_fov_rad = 2.0 * ((vertical_fov_rad * 0.5).tan() * aspect_ratio).atan();
    let body_to_inertial = body_fixed_to_planet_inertial_rotation(orientation);
    let inertial_to_body = body_to_inertial.inverse();
    let camera_position_inertial = render_origin.origin + transform.translation.as_dvec3();
    let forward_inertial = transform.forward().as_vec3().as_dvec3();
    let right_inertial = transform.right().as_vec3().as_dvec3();
    let up_inertial = transform.up().as_vec3().as_dvec3();
    let horizontal_half_fov_rad = horizontal_fov_rad * 0.5 + VIEWPORT_PREFETCH_MARGIN_RAD;
    let vertical_half_fov_rad = vertical_fov_rad * 0.5 + VIEWPORT_PREFETCH_MARGIN_RAD;

    Some(TerrainViewport {
        position_m: inertial_to_body * camera_position_inertial,
        forward: (inertial_to_body * forward_inertial).normalize_or_zero(),
        right: (inertial_to_body * right_inertial).normalize_or_zero(),
        up: (inertial_to_body * up_inertial).normalize_or_zero(),
        half_fov_rad: vertical_fov_rad.max(horizontal_fov_rad) * 0.5,
        vertical_fov_rad,
        viewport_height_px,
        horizontal_tan: horizontal_half_fov_rad.tan(),
        vertical_tan: vertical_half_fov_rad.tan(),
        horizontal_sec: horizontal_half_fov_rad.cos().recip(),
        vertical_sec: vertical_half_fov_rad.cos().recip(),
    })
}

fn apply_selection_hysteresis(
    errors: &mut BTreeMap<TerrainPatch, f64>,
    previous_leaves: &BTreeSet<TerrainPatch>,
) {
    let split_threshold = SCREEN_ERROR_PX * (1.0 + LOD_HYSTERESIS_RATIO);
    let merge_threshold = SCREEN_ERROR_PX * (1.0 - LOD_HYSTERESIS_RATIO);
    for (patch, error_px) in errors {
        let was_split = previous_leaves
            .iter()
            .any(|leaf| patch.level < leaf.level && patch.is_ancestor_of(leaf));
        if was_split && *error_px > merge_threshold {
            *error_px = error_px.max(SCREEN_ERROR_PX * (1.0 + 1e-12));
        } else if !was_split && *error_px <= split_threshold {
            *error_px = error_px.min(SCREEN_ERROR_PX);
        }
    }
}

/// Keep generated refinement selected until it has actually left the expanded
/// viewport. The moving 3x3 error neighborhood otherwise drops a tile as soon
/// as its focus cell changes, even though it remains visible on screen.
fn retain_visible_detail_errors(
    errors: &mut BTreeMap<TerrainPatch, f64>,
    previous_target_leaves: &BTreeSet<TerrainPatch>,
    generated: &HashMap<TerrainPatch, CachedTerrainGeometry>,
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) {
    let Some(viewport) = viewport else {
        return;
    };
    for leaf in previous_target_leaves {
        if leaf.level == 0
            || !generated.contains_key(leaf)
            || !patch_intersects_viewport(*leaf, Some(viewport), radius_m, elevation_bounds)
        {
            continue;
        }
        // Force the existing leaf's ancestor path to remain split. The leaf
        // itself must not be forced, or selection would refine one more level.
        let mut ancestor = leaf.parent();
        while let Some(patch) = ancestor {
            errors
                .entry(patch)
                .and_modify(|error| *error = error.max(SCREEN_ERROR_PX * (1.0 + 1e-12)))
                .or_insert(SCREEN_ERROR_PX * (1.0 + 1e-12));
            ancestor = patch.parent();
        }
    }
}

/// Intersect the presentation camera's forward ray with the terrain sphere.
/// LOD selection must use the same focus as viewport culling; using the rocket
/// position here generates detailed tiles behind a free or orbital camera.
fn viewport_focus_direction(
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
    fallback_direction: DVec3,
) -> DVec3 {
    let Some(viewport) = viewport else {
        return fallback_direction;
    };
    let b = viewport.position_m.dot(viewport.forward);
    let c = viewport.position_m.length_squared() - radius_m * radius_m;
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return fallback_direction;
    }
    let distance_m = -b - discriminant.sqrt();
    if distance_m < 0.0 {
        return fallback_direction;
    }
    (viewport.position_m + viewport.forward * distance_m).normalize_or_zero()
}

/// Refinement is published only when every child replacing a parent is ready.
/// Once a child intersects the viewport, retain its selected sibling group and
/// ancestors as a bounded prefetch unit. Culling individual siblings would
/// strand the parent forever because publication requires a complete group.
fn add_viewport_lod_group(
    patch: TerrainPatch,
    selected: &BTreeSet<TerrainPatch>,
    requested: &mut BTreeSet<TerrainPatch>,
) {
    let mut current = patch;
    loop {
        let Some(parent) = current.parent() else {
            requested.insert(current);
            break;
        };
        for sibling in parent.children() {
            if selected.contains(&sibling) {
                requested.insert(sibling);
            }
        }
        current = parent;
    }
}

/// Root coverage is scoped to the camera's conservative viewport. The root
/// containing the active focus is retained as an asynchronous launch fallback.
fn root_requests_for_viewport(
    focused_root: TerrainPatch,
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) -> BTreeSet<TerrainPatch> {
    let mut roots: BTreeSet<_> = TerrainPatch::roots()
        .into_iter()
        .filter(|root| patch_intersects_viewport(*root, viewport, radius_m, elevation_bounds))
        .collect();
    roots.insert(focused_root);
    roots
}

/// Conservative bounding-sphere frustum test. It intentionally retains a small
/// margin for smooth camera motion; patches outside it are not requested,
/// rendered, or retained in the cache.
fn patch_intersects_viewport(
    patch: TerrainPatch,
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) -> bool {
    matches!(
        patch_viewport_visibility(patch, viewport, radius_m, elevation_bounds),
        PatchViewportVisibility::Visible
    )
}

fn patch_viewport_visibility(
    patch: TerrainPatch,
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) -> PatchViewportVisibility {
    let Some(viewport) = viewport else {
        return PatchViewportVisibility::Visible;
    };
    if viewport.forward.length_squared() < 0.5 {
        return PatchViewportVisibility::Visible;
    }

    if patch_is_behind_horizon(patch, viewport.position_m, radius_m, elevation_bounds) {
        return PatchViewportVisibility::BehindHorizon;
    }

    let (bounding_center_m, bounding_radius_m) =
        patch_bounding_sphere(patch, radius_m, elevation_bounds);
    if sphere_intersects_viewport_frustum(viewport, bounding_center_m, bounding_radius_m) {
        PatchViewportVisibility::Visible
    } else {
        PatchViewportVisibility::OutsideFrustum
    }
}

/// Conservative rectangular-frustum test for a terrain bounding sphere. The
/// viewport coefficients include the streaming prefetch margin, while the
/// radius expansion keeps limb and edge tiles intact.
fn sphere_intersects_viewport_frustum(
    viewport: &TerrainViewport,
    bounding_center_m: DVec3,
    bounding_radius_m: f64,
) -> bool {
    let to_center = bounding_center_m - viewport.position_m;
    if to_center.length_squared() <= bounding_radius_m * bounding_radius_m {
        return true;
    }

    let forward_distance_m = viewport.forward.dot(to_center);
    if forward_distance_m + bounding_radius_m <= 0.0 {
        return false;
    }

    let depth_m = forward_distance_m.max(0.0);
    viewport.right.dot(to_center).abs()
        <= depth_m * viewport.horizontal_tan + bounding_radius_m * viewport.horizontal_sec
        && viewport.up.dot(to_center).abs()
            <= depth_m * viewport.vertical_tan + bounding_radius_m * viewport.vertical_sec
}

/// A conservative sphere enclosing the patch at both extrema of the terrain
/// source's elevation interval. Centering at the radial midpoint is much tighter
/// than adding the entire height range to a mean-radius sphere, while still
/// retaining silhouettes at either declared source extreme.
fn patch_bounding_sphere(
    patch: TerrainPatch,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) -> (DVec3, f64) {
    let center = patch.center_direction();
    let (u0, v0, u1, v1) = patch.uv_bounds();
    let patch_radius_rad = [
        face_uv_to_direction(patch.face, u0, v0),
        face_uv_to_direction(patch.face, u1, v0),
        face_uv_to_direction(patch.face, u0, v1),
        face_uv_to_direction(patch.face, u1, v1),
    ]
    .into_iter()
    .map(|corner| center.dot(corner).clamp(-1.0, 1.0).acos())
    .fold(0.0, f64::max);

    let min_surface_radius_m = radius_m + elevation_bounds.min_m;
    let max_surface_radius_m = radius_m + elevation_bounds.max_m;
    let center_radius_m = (min_surface_radius_m + max_surface_radius_m) * 0.5;
    let radial_half_range_m = (max_surface_radius_m - min_surface_radius_m) * 0.5;
    // The chord reaches the farthest patch corner. An arc-length estimate here
    // is too small and could cull a tile that still contributes to the limb.
    let angular_radius_m = 2.0 * max_surface_radius_m * (patch_radius_rad * 0.5).sin();
    (
        center * center_radius_m,
        angular_radius_m + radial_half_range_m,
    )
}

/// Reject a quadtree node only when its conservative bounding sphere lies
/// wholly behind the tangent plane of the planet as seen by the camera.
///
/// This runs before queueing a `TerrainSource` bake, including coarse roots.
/// Nodes behind the limb never consume a task.
fn patch_is_behind_horizon(
    patch: TerrainPatch,
    camera_position_m: DVec3,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) -> bool {
    let camera_distance_m = camera_position_m.length();
    if camera_distance_m <= radius_m {
        return false;
    }

    let (bounding_center_m, bounding_radius_m) =
        patch_bounding_sphere(patch, radius_m, elevation_bounds);

    camera_position_m.dot(bounding_center_m) + camera_distance_m * bounding_radius_m
        < radius_m * radius_m
}

fn patch_needs_geometry(state: Option<PatchState>, has_geometry: bool) -> bool {
    matches!(state, Some(PatchState::Requested)) && !has_geometry
}

/// Drop every patch state associated with the current terrain owner. Render
/// eviction events are emitted by the caller because this pure cleanup step
/// does not know which planet owns the cache.
fn clear_terrain_cache(streaming: &mut TerrainStreamingResource) -> Vec<TerrainPatch> {
    streaming.inflight.clear();

    let managed: Vec<_> = streaming.manager.patch_states().collect();
    for (patch, state) in managed {
        match state {
            PatchState::Requested | PatchState::Generating | PatchState::Loading => {
                streaming.manager.cancel_pending(&patch);
            }
            PatchState::Ready | PatchState::Visible | PatchState::Cached => {
                streaming.manager.mark_cached(&patch);
                streaming.manager.evict(&patch);
            }
            PatchState::Evicted => {}
        }
    }
    streaming.manager.sweep_evicted();

    let evicted = streaming.generated.keys().copied().collect();
    streaming.generated.clear();
    streaming.published.clear();
    streaming.target_leaves = TerrainPatch::roots().into_iter().collect();
    streaming.cadence = TerrainStreamingCadence::default();
    evicted
}

fn generation_batch(
    ordered_window: &[TerrainPatch],
    manager: &TerrainPatchManager,
    generated: &HashMap<TerrainPatch, CachedTerrainGeometry>,
    limit: usize,
) -> Vec<TerrainPatch> {
    ordered_window
        .iter()
        .copied()
        .filter(|patch| {
            patch_needs_geometry(manager.state_of(patch), generated.contains_key(patch))
        })
        .take(limit)
        .collect()
}

/// Order existing requests by the next visible improvement they unlock. A
/// complete child group must be ready before it can replace its rendered parent,
/// so scheduling broad low-level viewport work first leaves the screen coarse
/// even while useful child tiles wait in the queue.
#[expect(
    clippy::too_many_arguments,
    reason = "Priority combines immutable selection, publication, camera, and source-bound inputs without introducing a duplicate streaming resource."
)]
fn prioritize_generation_requests(
    requested: &BTreeSet<TerrainPatch>,
    published: &BTreeSet<TerrainPatch>,
    target_leaves: &BTreeSet<TerrainPatch>,
    focus_direction: DVec3,
    focused_root: TerrainPatch,
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
    elevation_bounds: ElevationBounds,
) -> Vec<TerrainPatch> {
    let mut ordered: Vec<_> = requested.iter().copied().collect();
    ordered.sort_by_key(|patch| {
        let (tier, anchor) =
            generation_priority_group(*patch, published, target_leaves, focus_direction);
        (
            (
                tier,
                !patch_intersects_viewport(anchor, viewport, radius_m, elevation_bounds),
                anchor.level == 0 && anchor != focused_root,
                anchor.level,
                angular_distance_key(anchor.center_direction(), focus_direction),
                anchor.face,
                anchor.tile_y,
                anchor.tile_x,
            ),
            (
                patch.level,
                angular_distance_key(patch.center_direction(), focus_direction),
                patch.face,
                patch.tile_y,
                patch.tile_x,
            ),
        )
    });
    ordered
}

fn generation_priority_group(
    patch: TerrainPatch,
    published: &BTreeSet<TerrainPatch>,
    target_leaves: &BTreeSet<TerrainPatch>,
    focus_direction: DVec3,
) -> (u8, TerrainPatch) {
    // A refinement cannot become visible until its complete parent chain is
    // ready. Cold-starting with a deep focus tile therefore leaves the entire
    // viewport blank for multiple task batches. Publish the requested coarse
    // roots first; subsequent work retains the normal replacement priority.
    if patch.level == 0 && !published.contains(&patch) {
        return (0, patch);
    }
    let Some(parent) = patch.parent() else {
        return (2, patch);
    };

    if published.contains(&parent) && !target_leaves.contains(&parent) {
        return (0, parent);
    }

    if TerrainPatch::for_direction(focus_direction, parent.level) == parent {
        return (1, parent);
    }

    (2, patch)
}

fn angular_distance_key(center_direction: DVec3, focus_direction: DVec3) -> u64 {
    ((1.0 - center_direction.dot(focus_direction)).max(0.0) * 1_000_000.0) as u64
}

fn estimated_patch_bytes(patch: TerrainPatch, resolution: u32) -> u64 {
    let resolution = u64::from(resolution.max(2));
    let vertices = resolution * resolution + 4 * (resolution - 1);
    let indices = 6 * ((resolution - 1) * (resolution - 1) + 4 * (resolution - 1));
    let mut terrain_bytes = vertices * (DOMAIN_BYTES_PER_VERTEX + RENDER_BYTES_PER_VERTEX)
        + indices * 2 * BYTES_PER_INDEX;
    if supports_local_surfaces(patch.level) {
        terrain_bytes += LOCAL_SURFACE_MAP_BYTES;
    }
    if supports_vegetation(patch.level) {
        terrain_bytes += MAX_VEGETATION_MESH_BYTES;
    }
    terrain_bytes
}

/// Populate the full visible viewport through a bounded breadth-first
/// traversal. The previous focus-only 3x3 neighborhood gave a single patch
/// stack near the rocket all of the available detail while visible terrain at
/// the edge of the screen remained at root quality.
fn projected_errors_for_viewport(
    viewport: &TerrainViewport,
    max_level: u32,
    radius_m: f64,
    camera: CameraProjection,
    source: &dyn TerrainSource,
    elevation_bounds: ElevationBounds,
) -> (BTreeMap<TerrainPatch, f64>, TerrainCullingStats) {
    let mut errors = BTreeMap::new();
    let mut culling = TerrainCullingStats::default();
    let mut pending = VecDeque::new();
    for patch in TerrainPatch::roots() {
        let visibility =
            patch_viewport_visibility(patch, Some(viewport), radius_m, elevation_bounds);
        culling.record(visibility);
        if visibility == PatchViewportVisibility::Visible {
            pending.push_back(patch);
        }
    }
    let mut target_leaf_count = TerrainPatch::roots().len();

    while let Some(patch) = pending.pop_front() {
        if target_leaf_count + 3 > MAX_VIEWPORT_UNBALANCED_LEAVES {
            break;
        }
        let error_px = projected_patch_error_px(
            &patch,
            source.patch_geometric_error(&patch),
            radius_m,
            camera,
        );
        errors.insert(patch, error_px);
        if patch.level >= max_level || error_px <= SCREEN_ERROR_PX {
            continue;
        }

        target_leaf_count += 3;
        for child in patch.children() {
            let visibility =
                patch_viewport_visibility(child, Some(viewport), radius_m, elevation_bounds);
            culling.record(visibility);
            if visibility == PatchViewportVisibility::Visible {
                pending.push_back(child);
            }
        }
    }

    (errors, culling)
}

/// Populate the camera focus neighborhood when no presentation camera is
/// available. This keeps startup fallback bounded; regular rendering always
/// uses [`projected_errors_for_viewport`].
fn projected_errors_for_focus(
    focus_direction: DVec3,
    max_level: u32,
    radius_m: f64,
    camera: CameraProjection,
    source: &dyn TerrainSource,
) -> BTreeMap<TerrainPatch, f64> {
    let mut errors = BTreeMap::new();
    for level in 0..max_level {
        let focus = TerrainPatch::for_direction(focus_direction, level);
        for patch in patch_neighborhood(focus) {
            errors.insert(
                patch,
                projected_patch_error_px(
                    &patch,
                    source.patch_geometric_error(&patch),
                    radius_m,
                    camera,
                ),
            );
        }
    }
    errors
}

fn patch_neighborhood(focus: TerrainPatch) -> BTreeSet<TerrainPatch> {
    let mut patches = BTreeSet::from([focus]);
    for edge in PatchEdge::ALL {
        let neighbor = focus.neighbor(edge).patch;
        patches.insert(neighbor);
        let perpendicular_edges = match edge {
            PatchEdge::West | PatchEdge::East => [PatchEdge::South, PatchEdge::North],
            PatchEdge::South | PatchEdge::North => [PatchEdge::West, PatchEdge::East],
        };
        for side in perpendicular_edges {
            patches.insert(neighbor.neighbor(side).patch);
        }
    }
    patches
}

fn stitch_edges_for(patch: TerrainPatch, leaves: &BTreeSet<TerrainPatch>) -> Vec<PatchEdge> {
    PatchEdge::ALL
        .into_iter()
        .filter(|edge| {
            let neighbor_at_patch_level = patch.neighbor(*edge).patch;
            leaves.iter().any(|candidate| {
                candidate.level + 1 == patch.level
                    && candidate.is_ancestor_of(&neighbor_at_patch_level)
            })
        })
        .collect()
}

fn stitch_mask(edges: &[PatchEdge]) -> u8 {
    edges.iter().fold(0, |mask, edge| {
        mask | match edge {
            PatchEdge::West => 1,
            PatchEdge::East => 1 << 1,
            PatchEdge::South => 1 << 2,
            PatchEdge::North => 1 << 3,
        }
    })
}

fn stale_cached_stitch_variants(
    streaming: &TerrainStreamingResource,
    target_leaves: &BTreeSet<TerrainPatch>,
) -> Vec<TerrainPatch> {
    target_leaves
        .iter()
        .copied()
        .filter(|patch| {
            !streaming.published.contains(patch)
                && matches!(
                    streaming.manager.state_of(patch),
                    Some(PatchState::Ready | PatchState::Visible | PatchState::Cached)
                )
        })
        .filter(|patch| {
            streaming.generated.get(patch).is_some_and(|cached| {
                cached.stitch_mask != stitch_mask(&stitch_edges_for(*patch, target_leaves))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::cube_sphere::CubeFace;
    use crate::infrastructure::bevy_adapters::terrain::surface::VEGETATION_MIN_PATCH_LEVEL;

    #[derive(Debug)]
    struct DivergentOverviewSource;

    fn test_elevation_bounds() -> ElevationBounds {
        ElevationBounds::new(-10_036.0, 20_024.0)
    }

    fn test_viewport(
        position_m: DVec3,
        forward: DVec3,
        half_fov_rad: f64,
        vertical_fov_rad: f64,
    ) -> TerrainViewport {
        let half_horizontal_fov_rad = half_fov_rad + VIEWPORT_PREFETCH_MARGIN_RAD;
        let half_vertical_fov_rad = vertical_fov_rad * 0.5 + VIEWPORT_PREFETCH_MARGIN_RAD;
        TerrainViewport {
            position_m,
            forward,
            right: DVec3::X,
            up: DVec3::Y,
            half_fov_rad,
            vertical_fov_rad,
            viewport_height_px: 1_080.0,
            horizontal_tan: half_horizontal_fov_rad.tan(),
            vertical_tan: half_vertical_fov_rad.tan(),
            horizontal_sec: half_horizontal_fov_rad.cos().recip(),
            vertical_sec: half_vertical_fov_rad.cos().recip(),
        }
    }

    #[derive(Debug)]
    struct BoundedTerrainSource(ElevationBounds);

    impl TerrainSource for BoundedTerrainSource {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            0.0
        }

        fn elevation_bounds_m(&self) -> ElevationBounds {
            self.0
        }
    }

    impl TerrainSource for DivergentOverviewSource {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            125.0
        }

        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(125.0, 125.0)
        }

        fn overview_height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            -250.0
        }
    }

    #[test]
    fn every_render_lod_uses_the_authoritative_terrain_height() {
        let source = DivergentOverviewSource;
        let config = TerrainRenderConfig {
            patch_resolution: 3,
            ..default()
        };

        for level in [0, 7, 8, MAX_PATCH_LEVEL] {
            let patch = TerrainPatch::for_direction(DVec3::Z, level);
            let geometry = build_patch_geometry_with_stitches(
                &patch,
                &source,
                6_371_000.0,
                config.patch_resolution,
                config.skirt_depth_m,
                &[],
            );
            assert!(geometry.positions.iter().take(9).all(|position| {
                (DVec3::from_array(*position).length() - 6_371_125.0).abs() < 1e-6
            }));
        }
    }

    #[test]
    fn root_fallback_and_refinement_use_authoritative_height() {
        let source = DivergentOverviewSource;
        let root = TerrainPatch::for_direction(DVec3::Z, 0);
        let child = TerrainPatch::for_direction(DVec3::Z, 1);

        let root_geometry = build_streamed_patch_geometry(&root, &source, 6_371_000.0, 3, 5.0, &[]);
        let child_geometry =
            build_streamed_patch_geometry(&child, &source, 6_371_000.0, 3, 5.0, &[]);

        assert!(root_geometry.positions.iter().take(9).all(|position| {
            (DVec3::from_array(*position).length() - 6_371_125.0).abs() < 1e-6
        }));
        assert!(child_geometry.positions.iter().take(9).all(|position| {
            (DVec3::from_array(*position).length() - 6_371_125.0).abs() < 1e-6
        }));
    }

    #[test]
    fn lod_hysteresis_holds_level_inside_transition_band() {
        let radius_m = 6_371_000.0;
        // Stay below the configured ceiling: this exercises the hysteresis
        // band rather than a max-level clamp.
        let level = MAX_PATCH_LEVEL - 2;
        let distance_m = 1_300_000.0;
        let direct = crate::domain::services::cube_sphere::lod_for_distance(
            distance_m,
            radius_m,
            FOV_RAD,
            SCREEN_HEIGHT_PX,
            SCREEN_ERROR_PX,
            level,
        );
        let held = lod_for_distance_with_hysteresis(Some(level), distance_m, radius_m);

        assert!(
            direct < level,
            "test distance must cross the raw LOD threshold"
        );
        assert_eq!(
            held, level,
            "small threshold oscillations must not churn LOD"
        );
    }

    #[test]
    fn stale_requested_patches_are_removed_before_generation() {
        let keep = TerrainPatch::root(CubeFace::PosZ);
        let stale = TerrainPatch::root(CubeFace::NegZ);
        let mut streaming = TerrainStreamingResource::default();
        streaming.manager.request(keep, 1);
        streaming.manager.request(stale, 1);

        assert_eq!(
            streaming
                .cancel_unrequested(&BTreeSet::from([keep]))
                .requested,
            1,
        );

        assert_eq!(
            streaming.manager.state_of(&keep),
            Some(PatchState::Requested)
        );
        assert_eq!(streaming.manager.state_of(&stale), None);
    }

    #[test]
    fn patch_level_distribution_reports_each_selected_lod() {
        let root = TerrainPatch::root(CubeFace::PosZ);
        let children = root.children();
        let distribution = PatchLevelDistribution::from_patches([root, children[0], children[1]]);

        assert_eq!(distribution.0, BTreeMap::from([(0, 1), (1, 2)]));
    }

    #[test]
    fn published_parent_replacement_group_outranks_other_requests() {
        let replacement_parent = TerrainPatch::root(CubeFace::PosZ);
        let focus_parent = TerrainPatch::root(CubeFace::PosX);
        let replacement_children = replacement_parent.children();
        let focus_children = focus_parent.children();
        let requested = BTreeSet::from_iter(replacement_children.into_iter().chain(focus_children));
        let target_leaves = requested.clone();

        let ordered = prioritize_generation_requests(
            &requested,
            &BTreeSet::from([replacement_parent]),
            &target_leaves,
            DVec3::X,
            focus_parent,
            None,
            6_371_000.0,
            test_elevation_bounds(),
        );

        assert_eq!(
            BTreeSet::from_iter(ordered.into_iter().take(4)),
            BTreeSet::from(replacement_children),
            "the complete group needed to replace an on-screen parent must run first"
        );
    }

    #[test]
    fn focus_group_outranks_broad_viewport_requests() {
        let focus_parent = TerrainPatch::root(CubeFace::PosZ);
        let broad_parent = TerrainPatch::root(CubeFace::NegZ);
        let focus_children = focus_parent.children();
        let broad_children = broad_parent.children();
        let requested = BTreeSet::from_iter(focus_children.into_iter().chain(broad_children));
        let target_leaves = requested.clone();

        let ordered = prioritize_generation_requests(
            &requested,
            &BTreeSet::new(),
            &target_leaves,
            DVec3::Z,
            focus_parent,
            None,
            6_371_000.0,
            test_elevation_bounds(),
        );

        assert_eq!(
            BTreeSet::from_iter(ordered.into_iter().take(4)),
            BTreeSet::from(focus_children),
            "camera-facing sibling groups must outrank broad viewport coverage"
        );
    }

    #[test]
    fn cold_start_roots_outrank_deep_focus_refinement() {
        let focused_root = TerrainPatch::root(CubeFace::PosZ);
        let requested = BTreeSet::from_iter(
            TerrainPatch::roots()
                .into_iter()
                .chain(focused_root.children()),
        );
        let target_leaves = BTreeSet::from_iter(focused_root.children());

        let ordered = prioritize_generation_requests(
            &requested,
            &BTreeSet::new(),
            &target_leaves,
            DVec3::Z,
            focused_root,
            None,
            6_371_000.0,
            test_elevation_bounds(),
        );

        assert_eq!(
            BTreeSet::from_iter(ordered.into_iter().take(TerrainPatch::roots().len())),
            BTreeSet::from_iter(TerrainPatch::roots()),
            "cold start must generate a publishable coarse cover before refinement"
        );
    }

    #[test]
    fn incomplete_root_cover_outranks_refinement_after_the_first_publish() {
        let focused_root = TerrainPatch::root(CubeFace::PosZ);
        let published_root = TerrainPatch::root(CubeFace::NegZ);
        let requested = BTreeSet::from_iter(
            TerrainPatch::roots()
                .into_iter()
                .chain(focused_root.children()),
        );
        let target_leaves = BTreeSet::from_iter(focused_root.children());

        let ordered = prioritize_generation_requests(
            &requested,
            &BTreeSet::from([published_root]),
            &target_leaves,
            DVec3::Z,
            focused_root,
            None,
            6_371_000.0,
            test_elevation_bounds(),
        );

        let missing_roots: BTreeSet<_> = TerrainPatch::roots()
            .into_iter()
            .filter(|root| *root != published_root)
            .collect();
        assert_eq!(
            BTreeSet::from_iter(ordered.into_iter().take(missing_roots.len())),
            missing_roots,
            "a partial root cover must not be abandoned for deep focus refinement"
        );
    }

    #[test]
    fn neighborhood_crosses_face_boundaries_without_losing_coverage() {
        let focus = TerrainPatch {
            face: CubeFace::PosZ,
            level: 2,
            tile_x: 0,
            tile_y: 0,
        };
        let neighborhood = patch_neighborhood(focus);
        assert!(neighborhood.contains(&focus));
        assert!(neighborhood.iter().any(|patch| patch.face != focus.face));
        assert!(neighborhood.len() >= 5);
    }

    #[test]
    fn interior_neighborhood_contains_all_eight_adjacent_tiles() {
        let focus = TerrainPatch {
            face: CubeFace::PosZ,
            level: 3,
            tile_x: 3,
            tile_y: 3,
        };
        let neighborhood = patch_neighborhood(focus);

        assert_eq!(neighborhood.len(), 9);
        for tile_y in 2..=4 {
            for tile_x in 2..=4 {
                assert!(neighborhood.contains(&TerrainPatch {
                    face: CubeFace::PosZ,
                    level: 3,
                    tile_x,
                    tile_y,
                }));
            }
        }
    }

    #[test]
    fn viewport_selection_rejects_terrain_outside_the_camera_frustum() {
        let radius_m = 6_371_000.0;
        let viewport = test_viewport(
            DVec3::new(0.0, 0.0, radius_m + 1_000.0),
            -DVec3::Z,
            0.5,
            0.8,
        );

        assert!(patch_intersects_viewport(
            TerrainPatch::root(CubeFace::PosZ),
            Some(&viewport),
            radius_m,
            test_elevation_bounds(),
        ));
        assert!(!patch_intersects_viewport(
            TerrainPatch::root(CubeFace::PosX),
            Some(&viewport),
            radius_m,
            test_elevation_bounds(),
        ));

        assert!(
            viewport_focus_direction(Some(&viewport), radius_m, DVec3::X)
                .abs_diff_eq(DVec3::Z, 1e-9)
        );
    }

    #[test]
    fn rectangular_frustum_rejects_circular_cone_corner_but_retains_edge_spheres() {
        let viewport = test_viewport(DVec3::ZERO, -DVec3::Z, 0.5, 0.4);

        // This point is inside the old widest-FOV circular cone but outside the
        // actual vertical viewport extent.
        assert!(!sphere_intersects_viewport_frustum(
            &viewport,
            DVec3::new(5.0, 5.0, -10.0),
            0.0,
        ));
        // A sphere touching the viewport edge remains visible after radius
        // expansion, preventing frustum-edge popping.
        assert!(sphere_intersects_viewport_frustum(
            &viewport,
            DVec3::new(0.0, 5.0, -10.0),
            1.0,
        ));
    }

    #[test]
    fn viewport_error_traversal_distributes_detail_across_the_visible_surface() {
        let radius_m = 6_371_000.0;
        let viewport = test_viewport(DVec3::Z * (radius_m + 307_000.0), -DVec3::Z, 0.65, 1.0);
        let source = BoundedTerrainSource(test_elevation_bounds());
        let (errors, culling) = projected_errors_for_viewport(
            &viewport,
            8,
            radius_m,
            CameraProjection {
                position_m: viewport.position_m,
                vertical_fov_rad: viewport.vertical_fov_rad,
                viewport_height_px: viewport.viewport_height_px,
            },
            &source,
            test_elevation_bounds(),
        );

        assert!(culling.candidates > culling.horizon_rejected + culling.frustum_rejected);

        assert!(
            errors.len() > 9 * 8,
            "viewport traversal must cover more than the previous 3x3 focus neighborhood"
        );
        assert!(
            errors
                .keys()
                .any(|patch| patch.level >= 3 && patch.center_direction().dot(DVec3::Z) < 0.98),
            "detail must extend away from the center camera ray"
        );
        assert!(
            errors.len() <= MAX_VIEWPORT_TARGET_LEAVES,
            "viewport traversal must remain within the leaf budget"
        );
        let selection = select_quadtree_leaves(
            &QuadtreePatchState::default(),
            &errors,
            QuadtreeSelectionConfig {
                max_level: 8,
                max_projected_error_px: SCREEN_ERROR_PX,
                max_neighbor_level_difference: 1,
            },
        );
        assert!(
            selection.target_leaves.len() <= MAX_VIEWPORT_TARGET_LEAVES,
            "balanced viewport refinement must remain within the leaf budget; got {}",
            selection.target_leaves.len()
        );
    }

    #[test]
    fn root_coverage_is_limited_to_the_viewport_with_camera_local_refinement() {
        let parent = TerrainPatch::root(CubeFace::PosZ);
        let selected: BTreeSet<_> = parent.children().into_iter().collect();
        let radius_m = 6_371_000.0;
        let viewport = test_viewport(
            DVec3::new(0.0, 0.0, radius_m + 1_000.0),
            -DVec3::Z,
            0.5,
            0.8,
        );
        let mut requested =
            root_requests_for_viewport(parent, Some(&viewport), radius_m, test_elevation_bounds());

        add_viewport_lod_group(parent.children()[0], &selected, &mut requested);

        assert_eq!(requested.iter().filter(|patch| patch.level == 0).count(), 1);
        assert!(requested.contains(&parent));
        assert!(parent
            .children()
            .into_iter()
            .all(|child| requested.contains(&child)));
    }

    #[test]
    fn cached_terrain_is_retained_until_memory_budget_requires_eviction() {
        let patch = TerrainPatch::root(CubeFace::PosZ);
        let mut streaming = TerrainStreamingResource::default();
        streaming.manager.request(patch, 1_024);
        streaming.manager.mark_ready(&patch);
        streaming.manager.mark_visible(&patch);
        streaming.manager.mark_cached(&patch);

        assert_eq!(streaming.manager.state_of(&patch), Some(PatchState::Cached));
        assert_eq!(
            streaming
                .manager
                .enforce_memory_budget_protecting(0, &BTreeSet::new()),
            vec![patch]
        );
        assert_eq!(streaming.manager.state_of(&patch), None);
        assert_eq!(streaming.manager.resident_bytes(), 0);
    }

    #[test]
    fn active_fallback_chain_survives_memory_pressure() {
        let root = TerrainPatch::root(CubeFace::PosZ);
        let child = root.children()[0];
        let mut manager = TerrainPatchManager::new();
        for patch in [root, child] {
            manager.request(patch, 1_024);
            manager.mark_ready(&patch);
            manager.mark_visible(&patch);
            manager.mark_cached(&patch);
        }

        let protected = BTreeSet::from([root, child]);
        assert!(manager
            .enforce_memory_budget_protecting(0, &protected)
            .is_empty());
        assert_eq!(manager.resident_bytes(), 2_048);
    }

    #[test]
    fn per_node_hysteresis_holds_previous_split_inside_band() {
        let root = TerrainPatch::root(CubeFace::PosZ);
        let previous: BTreeSet<_> = root.children().into_iter().collect();
        let mut errors = BTreeMap::from([(root, SCREEN_ERROR_PX * 0.9)]);

        apply_selection_hysteresis(&mut errors, &previous);

        assert!(errors[&root] > SCREEN_ERROR_PX);
    }

    #[test]
    fn generated_detail_stays_selected_while_it_remains_in_the_viewport() {
        let radius_m = 6_371_000.0;
        let detail = TerrainPatch::for_direction(DVec3::Z, 2);
        let viewport = test_viewport(DVec3::Z * (radius_m + 1_000.0), -DVec3::Z, 0.8, 0.8);
        let mut generated = HashMap::new();
        generated.insert(
            detail,
            CachedTerrainGeometry {
                geometry: PatchGeometry {
                    positions: Vec::new(),
                    normals: Vec::new(),
                    uvs: Vec::new(),
                    local_uvs: Vec::new(),
                    indices: Vec::new(),
                },
                surface: None,
                stitch_mask: 0,
            },
        );
        let mut errors = BTreeMap::new();

        retain_visible_detail_errors(
            &mut errors,
            &BTreeSet::from([detail]),
            &generated,
            Some(&viewport),
            radius_m,
            test_elevation_bounds(),
        );

        let selection = select_quadtree_leaves(
            &QuadtreePatchState {
                ready: BTreeSet::from([detail]),
                visible: TerrainPatch::roots().into_iter().collect(),
            },
            &errors,
            QuadtreeSelectionConfig {
                max_level: MAX_PATCH_LEVEL,
                max_projected_error_px: SCREEN_ERROR_PX,
                max_neighbor_level_difference: 1,
            },
        );
        assert!(selection.target_leaves.contains(&detail));
        assert!(selection
            .target_leaves
            .iter()
            .all(|patch| !detail.is_ancestor_of(patch) || *patch == detail));
    }

    #[test]
    fn generated_detail_can_coarsen_after_leaving_the_viewport() {
        let radius_m = 6_371_000.0;
        let detail = TerrainPatch::root(CubeFace::NegZ).children()[0];
        let viewport = test_viewport(DVec3::Z * (radius_m + 1_000.0), -DVec3::Z, 0.5, 0.8);
        let mut generated = HashMap::new();
        generated.insert(
            detail,
            CachedTerrainGeometry {
                geometry: PatchGeometry {
                    positions: Vec::new(),
                    normals: Vec::new(),
                    uvs: Vec::new(),
                    local_uvs: Vec::new(),
                    indices: Vec::new(),
                },
                surface: None,
                stitch_mask: 0,
            },
        );
        let mut errors = BTreeMap::new();

        retain_visible_detail_errors(
            &mut errors,
            &BTreeSet::from([detail]),
            &generated,
            Some(&viewport),
            radius_m,
            test_elevation_bounds(),
        );

        assert!(errors.is_empty());
    }

    #[test]
    fn viewport_request_keeps_the_complete_sibling_group_for_progressive_lod() {
        let parent = TerrainPatch::root(CubeFace::PosZ);
        let selected: BTreeSet<_> = parent.children().into_iter().collect();
        let mut requested = BTreeSet::new();

        add_viewport_lod_group(parent.children()[0], &selected, &mut requested);

        assert!(parent
            .children()
            .into_iter()
            .all(|child| requested.contains(&child)));
        assert!(requested.contains(&parent));
    }

    #[test]
    fn viewport_roots_retain_the_focused_fallback() {
        let radius_m = 6_371_000.0;
        let viewport = test_viewport(
            DVec3::new(0.0, 0.0, radius_m + 1_000.0),
            -DVec3::Z,
            0.5,
            0.8,
        );
        let roots = root_requests_for_viewport(
            TerrainPatch::root(CubeFace::NegZ),
            Some(&viewport),
            radius_m,
            test_elevation_bounds(),
        );

        assert!(roots.contains(&TerrainPatch::root(CubeFace::PosZ)));
        assert!(roots.contains(&TerrainPatch::root(CubeFace::NegZ)));
    }

    #[test]
    fn cadence_skips_unchanged_frames_but_reacts_to_camera_or_job_changes() {
        let cadence = TerrainStreamingCadence {
            last_reconcile_at_s: 10.0,
            focus_direction: Some(DVec3::Z),
            camera_position_m: Some(DVec3::Z * 6_371_100.0),
            half_fov_rad: Some(0.5),
            max_focus_level: None,
        };

        assert!(!should_reconcile_terrain(
            &cadence,
            10.01,
            DVec3::Z,
            DVec3::Z * 6_371_100.0,
            Some(0.5),
            false,
        ));
        assert!(should_reconcile_terrain(
            &cadence,
            10.01,
            DVec3::X,
            DVec3::Z * 6_371_100.0,
            Some(0.5),
            false,
        ));
        assert!(should_reconcile_terrain(
            &cadence,
            10.01,
            DVec3::Z,
            DVec3::Z * 6_371_100.0,
            Some(0.5),
            true,
        ));
        assert!(should_reconcile_terrain(
            &cadence,
            10.05,
            DVec3::Z,
            DVec3::Z * 6_371_100.0,
            Some(0.5),
            false,
        ));
    }

    #[test]
    fn clearing_cache_releases_geometry_and_managed_residency() {
        let patch = TerrainPatch::root(CubeFace::PosZ);
        let mut streaming = TerrainStreamingResource::default();
        streaming.manager.request(patch, 1_024);
        streaming.manager.mark_ready(&patch);
        streaming.manager.mark_visible(&patch);
        streaming.generated.insert(
            patch,
            CachedTerrainGeometry {
                geometry: PatchGeometry {
                    positions: Vec::new(),
                    normals: Vec::new(),
                    uvs: Vec::new(),
                    local_uvs: Vec::new(),
                    indices: Vec::new(),
                },
                surface: None,
                stitch_mask: 0,
            },
        );
        streaming.published.insert(patch);

        assert_eq!(clear_terrain_cache(&mut streaming), vec![patch]);
        assert_eq!(streaming.manager.resident_bytes(), 0);
        assert_eq!(streaming.manager.resident_patch_count(), 0);
        assert!(streaming.generated.is_empty());
        assert!(streaming.published.is_empty());
    }

    #[test]
    fn smoothstep_ramps_monotonically_and_clips() {
        // Below a: 0; above b: 1; monotonic in between (the LOD-distance blend
        // depends on this so the terrain LOD never jumps backward).
        assert_eq!(smoothstep(0.0, 10.0, -5.0), 0.0);
        assert_eq!(smoothstep(0.0, 10.0, 15.0), 1.0);
        assert!((smoothstep(0.0, 10.0, 5.0) - 0.5).abs() < 1e-9);
        let mut last = 0.0;
        for i in 0..=10 {
            let s = smoothstep(0.0, 10.0, i as f64);
            assert!(s >= last - 1e-12, "must be non-decreasing");
            last = s;
        }
    }

    #[test]
    fn visible_leaves_are_already_covered_by_roots_and_requested_patches() {
        let state = QuadtreePatchState::default();
        let errors = TerrainPatch::roots()
            .into_iter()
            .map(|patch| (patch, f64::INFINITY))
            .collect();
        let selection = select_quadtree_leaves(
            &state,
            &errors,
            QuadtreeSelectionConfig {
                max_level: 1,
                max_projected_error_px: SCREEN_ERROR_PX,
                max_neighbor_level_difference: 1,
            },
        );
        let mut required: BTreeSet<_> = TerrainPatch::roots().into_iter().collect();
        required.extend(selection.requested);

        assert!(selection
            .visible_leaves
            .iter()
            .all(|patch| required.contains(patch)));
    }

    #[test]
    fn stable_patches_reuse_cached_geometry_while_new_requests_generate() {
        let mut manager = TerrainPatchManager::new();
        let patch = TerrainPatch {
            face: CubeFace::PosZ,
            level: 2,
            tile_x: 1,
            tile_y: 1,
        };
        manager.request(patch, 1);
        assert!(patch_needs_geometry(manager.state_of(&patch), false));

        manager.begin_generation(&patch);
        assert!(!patch_needs_geometry(manager.state_of(&patch), false));
        manager.mark_ready(&patch);
        manager.mark_visible(&patch);
        assert!(!patch_needs_geometry(manager.state_of(&patch), true));

        manager.mark_cached(&patch);
        assert!(!patch_needs_geometry(manager.state_of(&patch), true));
    }

    #[test]
    fn published_geometry_is_retained_when_its_stitch_pattern_changes() {
        let patch = TerrainPatch {
            face: CubeFace::PosZ,
            level: 2,
            tile_x: 1,
            tile_y: 1,
        };
        let equal_neighbor = patch.neighbor(PatchEdge::East).patch;
        let coarser_neighbor = equal_neighbor.parent().unwrap();
        let mut streaming = TerrainStreamingResource::default();
        streaming.manager.request(patch, 1);
        streaming.manager.mark_ready(&patch);
        streaming.manager.mark_visible(&patch);
        streaming.generated.insert(
            patch,
            CachedTerrainGeometry {
                geometry: PatchGeometry {
                    positions: Vec::new(),
                    normals: Vec::new(),
                    uvs: Vec::new(),
                    local_uvs: Vec::new(),
                    indices: Vec::new(),
                },
                surface: None,
                stitch_mask: stitch_mask(&stitch_edges_for(
                    patch,
                    &BTreeSet::from([patch, equal_neighbor]),
                )),
            },
        );
        streaming.published.insert(patch);

        let target = BTreeSet::from([patch, coarser_neighbor]);
        assert!(stale_cached_stitch_variants(&streaming, &target).is_empty());

        streaming.published.remove(&patch);
        streaming.manager.mark_cached(&patch);
        assert_eq!(
            stale_cached_stitch_variants(&streaming, &target),
            vec![patch]
        );
    }

    #[test]
    fn patch_byte_estimate_accounts_for_skirts_and_render_meshes() {
        let resolution = 33;
        let grid_vertices = u64::from(resolution) * u64::from(resolution);
        let skirt_vertices = 4 * (u64::from(resolution) - 1);

        assert!(
            estimated_patch_bytes(TerrainPatch::root(CubeFace::PosZ), resolution)
                > (grid_vertices + skirt_vertices) * DOMAIN_BYTES_PER_VERTEX,
            "the budget must include the renderer copy and index buffers"
        );
    }

    #[test]
    fn vegetation_budget_applies_only_to_close_range_patches() {
        let coarse = TerrainPatch::root(CubeFace::PosZ);
        let non_vegetated = TerrainPatch::for_direction(DVec3::Z, VEGETATION_MIN_PATCH_LEVEL - 1);
        let vegetated = TerrainPatch::for_direction(DVec3::Z, VEGETATION_MIN_PATCH_LEVEL);

        assert_eq!(
            estimated_patch_bytes(non_vegetated, 33),
            estimated_patch_bytes(coarse, 33),
            "only close patches reserve local material maps and scatter"
        );
        assert_eq!(
            estimated_patch_bytes(vegetated, 33),
            estimated_patch_bytes(coarse, 33) + LOCAL_SURFACE_MAP_BYTES + MAX_VEGETATION_MESH_BYTES
        );
    }

    #[test]
    fn generation_capacity_admits_a_bounded_parallel_batch() {
        let roots = TerrainPatch::roots().to_vec();
        let mut manager = TerrainPatchManager::new();
        for patch in &roots {
            manager.request(*patch, 1);
        }

        let generated = HashMap::new();
        let bootstrap_batch =
            generation_batch(&roots, &manager, &generated, generation_capacity(4, 0));
        assert_eq!(bootstrap_batch, roots[..2].to_vec());

        let mut generated = HashMap::new();
        let focus = TerrainPatch::root(CubeFace::PosZ);
        generated.insert(
            focus,
            CachedTerrainGeometry {
                geometry: PatchGeometry {
                    positions: Vec::new(),
                    normals: Vec::new(),
                    uvs: Vec::new(),
                    local_uvs: Vec::new(),
                    indices: Vec::new(),
                },
                surface: None,
                stitch_mask: 0,
            },
        );
        manager.begin_generation(&focus);
        manager.mark_ready(&focus);
        manager.mark_visible(&focus);
        let later_batch = generation_batch(&roots, &manager, &generated, generation_capacity(4, 0));
        assert_eq!(later_batch.len(), 2);
        assert!(!later_batch.contains(&focus));

        assert_eq!(generation_capacity(4, 4), 0);
        assert_eq!(generation_capacity(8, 6), 2);
        assert_eq!(generation_capacity(8, 3), 2);
    }

    #[test]
    fn horizon_culling_rejects_refined_nodes_behind_the_planetary_limb() {
        let radius_m = 6_371_000.0;
        let camera_position_m = DVec3::Z * (radius_m + 1_000.0);
        let visible = TerrainPatch::for_direction(DVec3::Z, 8);
        let hidden = TerrainPatch::for_direction(-DVec3::Z, 8);

        assert!(!patch_is_behind_horizon(
            visible,
            camera_position_m,
            radius_m,
            test_elevation_bounds(),
        ));
        assert!(patch_is_behind_horizon(
            hidden,
            camera_position_m,
            radius_m,
            test_elevation_bounds(),
        ));
    }

    #[test]
    fn source_bounds_define_streaming_geometric_error() {
        let source = BoundedTerrainSource(ElevationBounds::new(-500.0, 1_500.0));
        let error = source.patch_geometric_error(&TerrainPatch::root(CubeFace::PosX));
        assert_eq!(error.elevation_range_m, 2_000.0);
        assert_eq!(error.child_to_parent_deviation_m, 2_000.0);
    }

    #[test]
    fn culling_sphere_contains_both_default_source_elevation_extremes() {
        let radius_m = 6_371_000.0;
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 8);
        let elevation_bounds = test_elevation_bounds();
        let (sphere_center, sphere_radius_m) =
            patch_bounding_sphere(patch, radius_m, elevation_bounds);
        let (u0, v0, u1, v1) = patch.uv_bounds();

        for direction in [
            face_uv_to_direction(patch.face, u0, v0),
            face_uv_to_direction(patch.face, u1, v0),
            face_uv_to_direction(patch.face, u0, v1),
            face_uv_to_direction(patch.face, u1, v1),
        ] {
            for elevation_m in [elevation_bounds.min_m, elevation_bounds.max_m] {
                assert!(
                    (direction * (radius_m + elevation_m)).distance(sphere_center)
                        <= sphere_radius_m + 1e-6
                );
            }
        }
    }
}
