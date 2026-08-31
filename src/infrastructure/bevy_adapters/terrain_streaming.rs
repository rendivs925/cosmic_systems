//! Cube-sphere terrain streaming (AGENTS.md sections 22-23).
//!
//! A `TerrainPatchManager` resource is driven each tick by a streaming system
//! that keeps a complete coarse cube-sphere cover with viewport-local refinement through the
//! requested → generating → ready → visible → cached → evicted lifecycle, and
//! enforces the configured memory budget by evicting least-recently-used cached
//! patches. Generated patch geometry is built deterministically from the shared
//! per-planet `TerrainSource`. Coarse roots provide fallback coverage for the
//! active viewport; only its camera neighborhood is refined.

use crate::components::rocket::RocketMissionState;
use crate::domain::services::body_orientation::BodyOrientation;
use crate::domain::services::cube_sphere::{
    build_patch_geometry_with_stitches, direction_to_lat_lon, face_uv_to_direction,
    projected_patch_error_px, select_quadtree_leaves, CameraProjection, PatchEdge,
    PatchGeometricError, PatchGeometry, QuadtreePatchState, QuadtreeSelectionConfig, TerrainPatch,
};
use crate::domain::services::reference_frames::{
    body_fixed_to_planet_inertial_rotation, planet_inertial_to_body_fixed,
};
use crate::domain::services::terrain_patch_manager::{PatchState, TerrainPatchManager};
use crate::domain::services::terrain_source::TerrainSource;
use crate::infrastructure::bevy_adapters::components::*;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::rocket_contact::TerrainSurfaceSampleCache;
use crate::infrastructure::bevy_adapters::terrain_render::{
    RenderOrigin, TerrainPatchCached, TerrainPatchEvicted, TerrainPatchReady, TerrainRenderConfig,
};
#[cfg(test)]
use crate::infrastructure::bevy_adapters::terrain_surface::VEGETATION_MIN_PATCH_LEVEL;
use crate::infrastructure::bevy_adapters::terrain_surface::{
    prepare_patch_surface, supports_local_surfaces, supports_vegetation, PreparedPatchSurface,
    LOCAL_SURFACE_MAP_BYTES, MAX_VEGETATION_MESH_BYTES,
};
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use bevy::{math::DVec3, prelude::*};
use std::collections::{BTreeMap, BTreeSet, HashMap};
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
/// Terrain envelopes stay within this conservative height margin for horizon
/// culling. Keeping the sphere deliberately large can retain a tile, but can
/// never reject terrain that could break the visible silhouette.
const HORIZON_CULL_HEIGHT_MARGIN_M: f64 = 20_000.0;
/// Admit a bounded pair of CPU terrain bakes per presentation frame. This fills
/// the reserved worker pool quickly after a camera move without queueing an
/// unbounded cold-start burst or blocking the render thread.
const MAX_TERRAIN_TASKS_PER_FRAME: usize = 2;
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
    inflight: BTreeMap<TerrainPatch, Task<GeneratedTerrainPatch>>,
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

/// Start the root that contains the stationary launch vehicle while it is in
/// the pre-launch hold. The regular streaming system owns subsequent task
/// completion and publication, so this only advances the existing lifecycle.
pub fn prebake_prelaunch_launchpad_patch(
    mut streaming: ResMut<TerrainStreamingResource>,
    planet_query: Query<(Entity, &PlanetComponent, &PlanetTerrain)>,
    rocket_query: Query<
        (
            &RocketMissionState,
            &RocketPlanetBinding,
            &RocketPhysicsState,
        ),
        Without<SpentStage>,
    >,
    config: Res<TerrainRenderConfig>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
) {
    let Some((mission, binding, rocket)) = rocket_query.iter().next() else {
        return;
    };
    if *mission != RocketMissionState::PreLaunch {
        return;
    }
    let Some((planet_entity, planet, terrain)) = planet_query
        .iter()
        .find(|(_, planet, _)| planet.matches_body(&binding.planet_name))
    else {
        return;
    };

    let radius_m = planet.domain_planet.radius_km as f64 * 1_000.0;
    let Some(orientation) =
        ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
    else {
        return;
    };
    let position_bf = planet_inertial_to_body_fixed(rocket.dynamics.position_m, orientation);
    let Some(direction) = position_bf.try_normalize() else {
        return;
    };
    let patch = TerrainPatch::for_direction(direction, 0);
    if streaming.inflight.contains_key(&patch) || streaming.generated.contains_key(&patch) {
        return;
    }

    streaming.active_planet = Some(planet_entity);
    streaming.manager.tick();
    streaming.manager.request(
        patch,
        estimated_patch_bytes(patch, config.patch_resolution_for(patch)),
    );
    streaming.manager.begin_generation(&patch);

    let source = terrain.source.clone();
    let patch_resolution = config.patch_resolution_for(patch);
    let skirt_depth_m = config.skirt_depth_m;
    let task = AsyncComputeTaskPool::get().spawn(async move {
        let generation_started = Instant::now();
        let (lat, lon) = direction_to_lat_lon(patch.center_direction());
        source.prepare_sample(lat, lon);
        let geometry = build_streamed_patch_geometry(
            &patch,
            source.as_ref(),
            radius_m,
            patch_resolution,
            skirt_depth_m,
            &[],
        );
        let surface = prepare_patch_surface(source.as_ref(), &patch, &geometry, radius_m);
        GeneratedTerrainPatch {
            geometry,
            surface,
            stitch_mask: 0,
            generation_ms: generation_started.elapsed().as_secs_f64() * 1_000.0,
        }
    });
    streaming.inflight.insert(patch, task);
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
}

/// Generated geometry is valid only for the LOD stitch pattern used to build
/// its index buffer. Reusing it with a different neighboring LOD would reopen
/// T-junction cracks along the changed edge.
pub struct CachedTerrainGeometry {
    pub geometry: PatchGeometry,
    pub(crate) surface: Option<PreparedPatchSurface>,
    stitch_mask: u8,
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
    half_fov_rad: f64,
    vertical_fov_rad: f64,
    viewport_height_px: f64,
}

#[derive(Default)]
struct TerrainStreamingCadence {
    last_reconcile_at_s: f64,
    focus_direction: Option<DVec3>,
    camera_position_m: Option<DVec3>,
    half_fov_rad: Option<f64>,
    max_focus_level: Option<u32>,
}

/// Streaming system: keep six root tiles available for the bound planet, refine
/// the rocket-facing neighborhood by projected geometric error, generate
/// deterministic geometry from the shared terrain source, and enforce the
/// memory budget. It only updates the streaming resource; it never writes
/// rendered geometry or the rocket's state.
#[expect(
    clippy::too_many_arguments,
    reason = "This streaming system coordinates independent ECS resources, events, and views."
)]
pub fn stream_terrain_patches(
    mut streaming: ResMut<TerrainStreamingResource>,
    planet_query: Query<(Entity, &PlanetComponent, &PlanetTerrain)>,
    rocket_query: Query<(&RocketPlanetBinding, &RocketPhysicsState), Without<SpentStage>>,
    mut ready_events: MessageWriter<TerrainPatchReady>,
    mut cached_events: MessageWriter<TerrainPatchCached>,
    mut evicted_events: MessageWriter<TerrainPatchEvicted>,
    config: Res<TerrainRenderConfig>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    time: Res<Time>,
    render_origin: Res<RenderOrigin>,
    camera_query: Query<(&Camera, &Transform, &Projection), With<Camera3d>>,
) {
    // No rocket yet: keep the manager tidy and return.
    let Some((binding, rocket)) = rocket_query.iter().next() else {
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
    let Some((planet_entity, _planet, planet_terrain)) = planet_query
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

    let (completed_batch_count, completed_batch_ms) = collect_completed_generation(&mut streaming);

    let radius_m = planet_query
        .iter()
        .find(|(_, planet, _)| planet.matches_body(&binding.planet_name))
        .map(|(_, planet, _)| planet.domain_planet.radius_km as f64 * 1000.0)
        .unwrap_or(6_371_000.0);

    let position_m = rocket.dynamics.position_m;
    let r = position_m.length();
    if r < 1e-6 {
        return;
    }
    // Terrain source coordinates are planet body-fixed geographic coordinates;
    // the rocket state remains planet-centered inertial everywhere else.
    let Some(orientation) =
        ephemeris_snapshot.orientation_for_catalog_body(&_planet.domain_planet.name)
    else {
        return;
    };
    let position_bf = planet_inertial_to_body_fixed(position_m, orientation);
    let dir = position_bf.normalize_or_zero();
    let viewport = terrain_viewport(&camera_query, &render_origin, orientation);
    let focus_direction = viewport_focus_direction(viewport.as_ref(), radius_m, dir);
    let lod_camera_position_m = viewport
        .as_ref()
        .map(|viewport| viewport.position_m)
        .unwrap_or(position_bf);
    let altitude_m = (lod_camera_position_m.length() - radius_m).max(0.0);

    if !should_reconcile_terrain(
        &streaming.cadence,
        time.elapsed_secs_f64(),
        focus_direction,
        lod_camera_position_m,
        viewport.as_ref().map(|viewport| viewport.half_fov_rad),
        completed_batch_count > 0,
    ) {
        return;
    }
    streaming.manager.tick();
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
    let mut errors = projected_errors_for_focus(
        focus_direction,
        max_focus_level,
        radius_m,
        CameraProjection {
            position_m: lod_camera_position_m,
            vertical_fov_rad: viewport
                .as_ref()
                .map_or(FOV_RAD, |viewport| viewport.vertical_fov_rad),
            viewport_height_px: viewport
                .as_ref()
                .map_or(SCREEN_HEIGHT_PX, |viewport| viewport.viewport_height_px),
        },
    );
    apply_selection_hysteresis(&mut errors, &streaming.target_leaves);
    retain_visible_detail_errors(
        &mut errors,
        &streaming.target_leaves,
        &streaming.generated,
        viewport.as_ref(),
        radius_m,
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
    streaming.target_leaves = selection.target_leaves.clone();

    // Keep every root resident as the complete source-authoritative fallback.
    // The recessed flight globe is only a bootstrap mesh while these async jobs
    // complete; it must not become exposed terrain between local patches.
    let mut requested: BTreeSet<_> = TerrainPatch::roots().into_iter().collect();
    for patch in selection
        .requested
        .iter()
        .copied()
        .filter(|patch| patch_intersects_viewport(*patch, viewport.as_ref(), radius_m))
    {
        add_viewport_lod_group(patch, &selection.requested, &mut requested);
    }

    // The camera can move before a queued refinement finishes. Cancel obsolete
    // unfinished tasks now; completed geometry follows the cache lifecycle below
    // so it can still be reused if the patch becomes visible again.
    let stale_inflight: Vec<_> = streaming
        .inflight
        .keys()
        .copied()
        .filter(|patch| !requested.contains(patch))
        .collect();
    for patch in stale_inflight {
        streaming.inflight.remove(&patch);
        streaming.manager.cancel_pending(&patch);
    }
    cancel_stale_requested(&mut streaming.manager, &requested);

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

    for patch in &requested {
        let size_bytes = estimated_patch_bytes(*patch, config.patch_resolution_for(*patch));
        streaming.manager.request(*patch, size_bytes);
    }

    let mut generation_order: Vec<_> = requested.iter().copied().collect();
    generation_order.sort_by_key(|patch| {
        let center = patch.center_direction();
        let distance_key = ((1.0 - center.dot(focus_direction)).max(0.0) * 1_000_000.0) as u64;
        (
            !patch_intersects_viewport(*patch, viewport.as_ref(), radius_m),
            patch.level,
            distance_key,
            patch.face,
            patch.tile_y,
            patch.tile_x,
        )
    });
    let task_pool = AsyncComputeTaskPool::get();
    let generation_limit = generation_capacity(task_pool.thread_num(), streaming.inflight.len());
    let batch = generation_batch(
        &generation_order,
        &streaming.manager,
        &streaming.generated,
        generation_limit,
    );
    for patch in batch {
        streaming.manager.begin_generation(&patch);
        let source = planet_terrain.source.clone();
        let stitch_edges = stitch_edges_for(patch, &selection.target_leaves);
        let stitch_mask = stitch_mask(&stitch_edges);
        let patch_resolution = config.patch_resolution_for(patch);
        let skirt_depth_m = config.skirt_depth_m;
        let task = task_pool.spawn(async move {
            let generation_started = Instant::now();
            // DEM loading and erosion baking are allowed only in this worker.
            let (lat, lon) = direction_to_lat_lon(patch.center_direction());
            source.prepare_sample(lat, lon);
            let geometry = build_streamed_patch_geometry(
                &patch,
                source.as_ref(),
                radius_m,
                patch_resolution,
                skirt_depth_m,
                &stitch_edges,
            );
            let surface = prepare_patch_surface(source.as_ref(), &patch, &geometry, radius_m);
            GeneratedTerrainPatch {
                geometry,
                surface,
                stitch_mask,
                generation_ms: generation_started.elapsed().as_secs_f64() * 1_000.0,
            }
        });
        streaming.inflight.insert(patch, task);
    }

    // Publish only a complete ready leaf cover. Cached child meshes are never
    // spawned until every sibling can replace the parent, preventing z-fighting
    // and blank-space transitions.
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
    for patch in arrived {
        streaming.manager.mark_visible(&patch);
        ready_events.write(TerrainPatchReady {
            patch,
            planet_entity,
        });
    }
    streaming.published = current_visible;

    let metrics_due =
        completed_batch_count > 0 && streaming.generated.len() >= streaming.next_metrics_report_at;
    if metrics_due {
        info!(
            target: "terrain_streaming",
            visible_tiles = streaming.published.len(),
            generated_tiles = streaming.generated.len(),
            resident_tiles = streaming.manager.ready_patch_count(),
            estimated_resident_mib = streaming.manager.resident_bytes() as f64 / (1024.0 * 1024.0),
            budget_mib = streaming.budget_bytes as f64 / (1024.0 * 1024.0),
            inflight_tiles = streaming.inflight.len(),
            completed_batch_tiles = completed_batch_count,
            completed_batch_ms = completed_batch_ms,
            "Terrain streaming metrics"
        );
        streaming.next_metrics_report_at =
            streaming.generated.len() + METRICS_GENERATED_TILE_INTERVAL;
    }

    let budget = streaming.budget_bytes;
    // The complete requested chain is progressive render fallback. It may
    // temporarily exceed the cache budget but must never be evicted mid-handoff.
    let protected = requested;
    let evicted = streaming
        .manager
        .enforce_memory_budget_protecting(budget, &protected);
    for patch in evicted {
        streaming.generated.remove(&patch);
        evicted_events.write(TerrainPatchEvicted {
            patch,
            planet_entity,
        });
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

fn cancel_stale_requested(manager: &mut TerrainPatchManager, requested: &BTreeSet<TerrainPatch>) {
    let stale: Vec<_> = manager
        .patch_states()
        .filter_map(|(patch, state)| {
            (state == PatchState::Requested && !requested.contains(&patch)).then_some(patch)
        })
        .collect();
    for patch in stale {
        manager.cancel_pending(&patch);
    }
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
    let camera_position_inertial = render_origin.origin + transform.translation.as_dvec3();
    let forward_inertial = transform.forward().as_vec3().as_dvec3();

    Some(TerrainViewport {
        position_m: body_to_inertial.inverse() * camera_position_inertial,
        forward: (body_to_inertial.inverse() * forward_inertial).normalize_or_zero(),
        half_fov_rad: vertical_fov_rad.max(horizontal_fov_rad) * 0.5,
        vertical_fov_rad,
        viewport_height_px,
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
) {
    let Some(viewport) = viewport else {
        return;
    };
    for leaf in previous_target_leaves {
        if leaf.level == 0
            || !generated.contains_key(leaf)
            || !patch_intersects_viewport(*leaf, Some(viewport), radius_m)
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
/// strand the parent forever and make close terrain arrive late.
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

/// Conservative bounding-sphere frustum test. It intentionally retains a small
/// margin for smooth camera motion; patches outside it are not requested,
/// rendered, or retained in the cache.
fn patch_intersects_viewport(
    patch: TerrainPatch,
    viewport: Option<&TerrainViewport>,
    radius_m: f64,
) -> bool {
    let Some(viewport) = viewport else {
        return true;
    };
    if viewport.forward.length_squared() < 0.5 {
        return true;
    }

    if patch_is_behind_horizon(patch, viewport.position_m, radius_m) {
        return false;
    }

    let center = patch.center_direction();
    let to_center = center * radius_m - viewport.position_m;
    let distance_m = to_center.length();
    let bounding_radius_m = patch_bounding_radius_m(patch, radius_m);
    if distance_m <= bounding_radius_m {
        return true;
    }
    let view_angle = viewport
        .forward
        .dot(to_center.normalize())
        .clamp(-1.0, 1.0)
        .acos();
    let sphere_angle_rad = (bounding_radius_m / distance_m).clamp(0.0, 1.0).asin();
    view_angle <= viewport.half_fov_rad + sphere_angle_rad + VIEWPORT_PREFETCH_MARGIN_RAD
}

fn patch_bounding_radius_m(patch: TerrainPatch, radius_m: f64) -> f64 {
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

    // The chord reaches the farthest patch corner. An arc-length estimate here
    // is too small and could cull a tile that still contributes to the limb.
    2.0 * radius_m * (patch_radius_rad * 0.5).sin() + HORIZON_CULL_HEIGHT_MARGIN_M
}

/// Reject a quadtree node only when its conservative bounding sphere lies
/// wholly behind the tangent plane of the planet as seen by the camera.
///
/// This runs before queueing a `TerrainSource` bake. Root coverage remains
/// unconditional, while refined nodes behind the limb never consume a task.
fn patch_is_behind_horizon(patch: TerrainPatch, camera_position_m: DVec3, radius_m: f64) -> bool {
    let camera_distance_m = camera_position_m.length();
    if camera_distance_m <= radius_m {
        return false;
    }

    let center = patch.center_direction();
    let bounding_radius_m = patch_bounding_radius_m(patch, radius_m);

    camera_position_m.dot(center * radius_m) + camera_distance_m * bounding_radius_m
        < radius_m * radius_m
}

fn patch_needs_geometry(state: Option<PatchState>, has_geometry: bool) -> bool {
    matches!(state, Some(PatchState::Requested)) && !has_geometry
}

fn collect_completed_generation(streaming: &mut TerrainStreamingResource) -> (usize, f64) {
    let completed: Vec<_> = streaming
        .inflight
        .iter_mut()
        .filter_map(|(patch, task)| {
            block_on(future::poll_once(task)).map(|generated| (*patch, generated))
        })
        .collect();
    let completed_count = completed.len();
    let mut generation_ms = 0.0;
    for (patch, generated) in completed {
        streaming.inflight.remove(&patch);
        streaming.generated.insert(
            patch,
            CachedTerrainGeometry {
                geometry: generated.geometry,
                surface: Some(generated.surface),
                stitch_mask: generated.stitch_mask,
            },
        );
        streaming.manager.mark_ready(&patch);
        generation_ms += generated.generation_ms;
    }
    (completed_count, generation_ms)
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

/// Populate only the camera neighborhood at each level. The pure selection
/// model still starts from all roots; absent entries intentionally stop
/// refinement so distant terrain remains inexpensive.
fn projected_errors_for_focus(
    focus_direction: DVec3,
    max_level: u32,
    radius_m: f64,
    camera: CameraProjection,
) -> BTreeMap<TerrainPatch, f64> {
    let mut errors = BTreeMap::new();
    let geometry_error = PatchGeometricError {
        elevation_range_m: 20_000.0,
        child_to_parent_deviation_m: 2_000.0,
    };
    for level in 0..max_level {
        let focus = TerrainPatch::for_direction(focus_direction, level);
        for patch in patch_neighborhood(focus) {
            errors.insert(
                patch,
                projected_patch_error_px(&patch, geometry_error, radius_m, camera),
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

    #[derive(Debug)]
    struct DivergentOverviewSource;

    impl TerrainSource for DivergentOverviewSource {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            125.0
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
        let mut manager = TerrainPatchManager::new();
        manager.request(keep, 1);
        manager.request(stale, 1);

        cancel_stale_requested(&mut manager, &BTreeSet::from([keep]));

        assert_eq!(manager.state_of(&keep), Some(PatchState::Requested));
        assert_eq!(manager.state_of(&stale), None);
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
        let viewport = TerrainViewport {
            position_m: DVec3::new(0.0, 0.0, radius_m + 1_000.0),
            forward: -DVec3::Z,
            half_fov_rad: 0.5,
            vertical_fov_rad: 0.8,
            viewport_height_px: 1080.0,
        };

        assert!(patch_intersects_viewport(
            TerrainPatch::root(CubeFace::PosZ),
            Some(&viewport),
            radius_m,
        ));
        assert!(!patch_intersects_viewport(
            TerrainPatch::root(CubeFace::PosX),
            Some(&viewport),
            radius_m,
        ));

        assert!(
            viewport_focus_direction(Some(&viewport), radius_m, DVec3::X)
                .abs_diff_eq(DVec3::Z, 1e-9)
        );
    }

    #[test]
    fn complete_root_coverage_is_retained_with_camera_local_refinement() {
        let parent = TerrainPatch::root(CubeFace::PosZ);
        let selected: BTreeSet<_> = parent.children().into_iter().collect();
        let mut requested: BTreeSet<_> = TerrainPatch::roots().into_iter().collect();

        add_viewport_lod_group(parent.children()[0], &selected, &mut requested);

        assert!(TerrainPatch::roots()
            .into_iter()
            .all(|root| requested.contains(&root)));
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
        let viewport = TerrainViewport {
            position_m: DVec3::Z * (radius_m + 1_000.0),
            forward: -DVec3::Z,
            half_fov_rad: 0.8,
            vertical_fov_rad: 0.8,
            viewport_height_px: 1_080.0,
        };
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
        let viewport = TerrainViewport {
            position_m: DVec3::Z * (radius_m + 1_000.0),
            forward: -DVec3::Z,
            half_fov_rad: 0.5,
            vertical_fov_rad: 0.8,
            viewport_height_px: 1_080.0,
        };
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
        let patch = TerrainPatch::root(CubeFace::PosZ);
        let grid_vertices = u64::from(resolution) * u64::from(resolution);
        let skirt_vertices = 4 * (u64::from(resolution) - 1);

        assert!(
            estimated_patch_bytes(patch, resolution)
                > (grid_vertices + skirt_vertices) * DOMAIN_BYTES_PER_VERTEX,
            "the budget must include the renderer copy and index buffers"
        );
    }

    #[test]
    fn vegetation_budget_only_applies_at_the_finest_detail_level() {
        let coarse = TerrainPatch::root(CubeFace::PosZ);
        let non_vegetated_refined =
            TerrainPatch::for_direction(DVec3::Z, VEGETATION_MIN_PATCH_LEVEL - 1);
        let vegetation_detail = TerrainPatch::for_direction(DVec3::Z, VEGETATION_MIN_PATCH_LEVEL);

        assert_eq!(
            estimated_patch_bytes(non_vegetated_refined, 33),
            estimated_patch_bytes(coarse, 33),
            "lower-detail patches must not reserve invisible vegetation"
        );
        assert_eq!(
            estimated_patch_bytes(vegetation_detail, 33),
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
            radius_m
        ));
        assert!(patch_is_behind_horizon(hidden, camera_position_m, radius_m));
    }
}
