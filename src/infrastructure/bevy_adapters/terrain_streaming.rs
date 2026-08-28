//! Cube-sphere terrain streaming (AGENTS.md sections 22-23).
//!
//! A `TerrainPatchManager` resource is driven each tick by a streaming system
//! that keeps a complete six-face quadtree surface alive through the
//! requested → generating → ready → visible → cached → evicted lifecycle, and
//! enforces the configured memory budget by evicting least-recently-used cached
//! patches. Generated patch geometry is built deterministically from the shared
//! per-planet `TerrainSource`. Coarse roots cover the whole planet; only the
//! camera neighborhood is refined.

use crate::domain::services::cube_sphere::{
    build_patch_geometry_with_stitches, build_patch_overview_geometry_with_stitches,
    lod_for_distance, projected_patch_error_px, select_quadtree_leaves, CameraProjection,
    PatchEdge, PatchGeometricError, PatchGeometry, QuadtreePatchState, QuadtreeSelectionConfig,
    TerrainPatch,
};
use crate::domain::services::reference_frames::planet_inertial_to_body_fixed;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_patch_manager::{PatchState, TerrainPatchManager};
use crate::domain::services::terrain_source::TerrainSource;
use crate::infrastructure::bevy_adapters::components::*;
use crate::infrastructure::bevy_adapters::terrain_render::{
    TerrainPatchCached, TerrainPatchEvicted, TerrainPatchReady, TerrainRenderConfig,
};
#[cfg(test)]
use crate::infrastructure::bevy_adapters::terrain_surface::VEGETATION_MIN_PATCH_LEVEL;
use crate::infrastructure::bevy_adapters::terrain_surface::{
    supports_vegetation, MAX_VEGETATION_MESH_BYTES,
};
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use bevy::{math::DVec3, prelude::*};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Instant;

/// Camera/LOD constants.
/// The initial hierarchy keeps global roots inexpensive while allowing local
/// detail down to roughly 2.4 km patches at Earth scale. Higher-resolution DEM
/// and imagery work can raise this only after profiling the visible leaf budget.
const MAX_PATCH_LEVEL: u32 = 12;
const FOV_RAD: f64 = 1.0;
const SCREEN_HEIGHT_PX: f64 = 1080.0;
const SCREEN_ERROR_PX: f64 = 4.0;
/// Domain geometry retains f64 position/normal and two UV sets. The renderer
/// creates a second f32 mesh with vertex color data, so budget both copies.
const DOMAIN_BYTES_PER_VERTEX: u64 = 64;
const RENDER_BYTES_PER_VERTEX: u64 = 48;
const BYTES_PER_INDEX: u64 = 4;
const DEFAULT_BUDGET_BYTES: u64 = 128 * 1024 * 1024;
const METRICS_GENERATED_TILE_INTERVAL: usize = 32;
/// Global and continental tiles render the terrain source's inexpensive overview
/// representation. Local tiles at this level and finer use authoritative height
/// samples, matching collision and avoiding global erosion-cache initialization.
pub(crate) const AUTHORITATIVE_TERRAIN_LEVEL: u32 = 8;

/// Minimum distance for LOD calculation when on the ground.
/// Uses estimated camera-to-terrain distance (~150m) instead of orbital heuristic.
const SURFACE_LOD_DISTANCE_M: f64 = 150.0;
/// Altitude threshold below which surface LOD distance is used.
const SURFACE_LOD_ALTITUDE_THRESHOLD_M: f64 = 10_000.0;
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
    /// Background geometry jobs for requested refinement tiles. Coarse roots are
    /// generated synchronously once so the initial frame always has a complete
    /// fallback surface; all later work is polled without blocking rendering.
    inflight: BTreeMap<TerrainPatch, Task<GeneratedTerrainPatch>>,
    /// Leaves currently published to the renderer. Generated descendants remain
    /// cached until all siblings are ready, then replace their parent together.
    pub published: BTreeSet<TerrainPatch>,
    /// Planet that owns every entry in this cache. Patch coordinates alone are
    /// not sufficient when a rocket changes its bound celestial body.
    active_planet: Option<Entity>,
    /// Next generated-tile count at which streaming metrics are reported.
    next_metrics_report_at: usize,
}

impl Default for TerrainStreamingResource {
    fn default() -> Self {
        Self {
            manager: TerrainPatchManager::new(),
            budget_bytes: DEFAULT_BUDGET_BYTES,
            generated: HashMap::new(),
            inflight: BTreeMap::new(),
            published: BTreeSet::new(),
            active_planet: None,
            next_metrics_report_at: 0,
        }
    }
}

/// Generated geometry is valid only for the LOD stitch pattern used to build
/// its index buffer. Reusing it with a different neighboring LOD would reopen
/// T-junction cracks along the changed edge.
pub struct CachedTerrainGeometry {
    pub geometry: PatchGeometry,
    stitch_mask: u8,
}

struct GeneratedTerrainPatch {
    geometry: PatchGeometry,
    stitch_mask: u8,
    generation_ms: f64,
}

/// Streaming system: keep six root tiles available for the bound planet, refine
/// the rocket-facing neighborhood by projected geometric error, generate
/// deterministic geometry from the shared terrain source, and enforce the
/// memory budget. It only updates the streaming resource; it never writes
/// rendered geometry or the rocket's state.
pub fn stream_terrain_patches(
    mut streaming: ResMut<TerrainStreamingResource>,
    planet_query: Query<(Entity, &PlanetComponent, &PlanetTerrain)>,
    rocket_query: Query<(&RocketPlanetBinding, &RocketPhysicsState), Without<SpentStage>>,
    mut ready_events: MessageWriter<TerrainPatchReady>,
    mut cached_events: MessageWriter<TerrainPatchCached>,
    mut evicted_events: MessageWriter<TerrainPatchEvicted>,
    config: Res<TerrainRenderConfig>,
    sim_time: Res<SimulationTime>,
) {
    streaming.manager.tick();

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

    let (mut completed_batch_count, mut completed_batch_ms) =
        collect_completed_generation(&mut streaming);

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
    let altitude_m = (r - radius_m).max(0.0);

    // Terrain source coordinates are planet body-fixed geographic coordinates;
    // the rocket state remains planet-centered inertial everywhere else.
    let position_bf = planet_inertial_to_body_fixed(
        position_m,
        &_planet.domain_planet,
        (sim_time.sim_time_s / 86_400.0) as f32,
    );
    let dir = position_bf.normalize_or_zero();

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

    let max_focus_level = lod_for_distance(
        lod_distance_m,
        radius_m,
        FOV_RAD,
        SCREEN_HEIGHT_PX,
        SCREEN_ERROR_PX,
        MAX_PATCH_LEVEL,
    );
    let errors = projected_errors_for_focus(
        dir,
        max_focus_level,
        radius_m,
        CameraProjection {
            position_m: position_bf,
            vertical_fov_rad: FOV_RAD,
            viewport_height_px: SCREEN_HEIGHT_PX,
        },
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

    // Geometry caches include the stitch index pattern. Discard a cached or
    // unpublished variant when its desired neighboring LOD changes; the
    // selection below falls back to its ready parent until the replacement is
    // generated.
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
    if invalidated_stitch_variants {
        streaming.manager.sweep_evicted();
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

    let mut requested: BTreeSet<_> = TerrainPatch::roots().into_iter().collect();
    requested.extend(selection.requested.iter().copied());

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
        let size_bytes = estimated_patch_bytes(*patch, config.patch_resolution);
        streaming.manager.request(*patch, size_bytes);
    }

    let mut generation_order: Vec<_> = requested.into_iter().collect();
    generation_order.sort_by_key(|patch| {
        let center = patch.center_direction();
        let distance_key = ((1.0 - center.dot(dir)).max(0.0) * 1_000_000.0) as u64;
        (
            patch.level,
            distance_key,
            patch.face,
            patch.tile_y,
            patch.tile_x,
        )
    });
    let task_pool = AsyncComputeTaskPool::get();
    let generation_limit = generation_capacity(
        streaming.generated.is_empty() && streaming.inflight.is_empty(),
        task_pool.thread_num(),
        streaming.inflight.len(),
    );
    let batch = generation_batch(
        &generation_order,
        &streaming.manager,
        &streaming.generated,
        generation_limit,
    );
    if streaming.generated.is_empty() && streaming.inflight.is_empty() {
        let generation_started = Instant::now();
        for patch in batch {
            streaming.manager.begin_generation(&patch);
            let stitch_edges = stitch_edges_for(patch, &selection.target_leaves);
            let stitch_mask = stitch_mask(&stitch_edges);
            let geometry = generate_patch_geometry(
                patch,
                planet_terrain.source.as_ref(),
                radius_m,
                &config,
                &stitch_edges,
            );
            streaming.generated.insert(
                patch,
                CachedTerrainGeometry {
                    geometry,
                    stitch_mask,
                },
            );
            streaming.manager.mark_ready(&patch);
            completed_batch_count += 1;
        }
        completed_batch_ms += generation_started.elapsed().as_secs_f64() * 1_000.0;
    } else {
        for patch in batch {
            streaming.manager.begin_generation(&patch);
            let source = planet_terrain.source.clone();
            let stitch_edges = stitch_edges_for(patch, &selection.target_leaves);
            let stitch_mask = stitch_mask(&stitch_edges);
            let patch_resolution = config.patch_resolution;
            let skirt_depth_m = config.skirt_depth_m;
            let task = task_pool.spawn(async move {
                let generation_started = Instant::now();
                let geometry = if patch.level < AUTHORITATIVE_TERRAIN_LEVEL {
                    build_patch_overview_geometry_with_stitches(
                        &patch,
                        source.as_ref(),
                        radius_m,
                        patch_resolution,
                        skirt_depth_m,
                        &stitch_edges,
                    )
                } else {
                    build_patch_geometry_with_stitches(
                        &patch,
                        source.as_ref(),
                        radius_m,
                        patch_resolution,
                        skirt_depth_m,
                        &stitch_edges,
                    )
                };
                GeneratedTerrainPatch {
                    geometry,
                    stitch_mask,
                    generation_ms: generation_started.elapsed().as_secs_f64() * 1_000.0,
                }
            });
            streaming.inflight.insert(patch, task);
        }
    }

    // Publish only a complete ready leaf cover. Cached child meshes are never
    // spawned until every sibling can replace the parent, preventing z-fighting
    // and blank-space transitions.
    let current_visible: BTreeSet<_> = selection.visible_leaves;
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
    let protected: BTreeSet<_> = TerrainPatch::roots().into_iter().collect();
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

/// Keep every available asynchronous terrain worker busy without creating a
/// backlog that would delay higher-priority camera movement. The root cover is
/// the one exception: it is built synchronously as an atomic initial fallback.
fn generation_capacity(cache_is_empty: bool, worker_count: usize, inflight_count: usize) -> usize {
    if cache_is_empty {
        TerrainPatch::roots().len()
    } else {
        worker_count.saturating_sub(inflight_count)
    }
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
    evicted
}

fn generate_patch_geometry(
    patch: TerrainPatch,
    source: &dyn TerrainSource,
    radius_m: f64,
    config: &TerrainRenderConfig,
    stitch_edges: &[PatchEdge],
) -> PatchGeometry {
    if patch.level < AUTHORITATIVE_TERRAIN_LEVEL {
        build_patch_overview_geometry_with_stitches(
            &patch,
            source,
            radius_m,
            config.patch_resolution,
            config.skirt_depth_m,
            stitch_edges,
        )
    } else {
        build_patch_geometry_with_stitches(
            &patch,
            source,
            radius_m,
            config.patch_resolution,
            config.skirt_depth_m,
            stitch_edges,
        )
    }
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
    let terrain_bytes = vertices * (DOMAIN_BYTES_PER_VERTEX + RENDER_BYTES_PER_VERTEX)
        + indices * 2 * BYTES_PER_INDEX;
    if supports_vegetation(patch.level) {
        terrain_bytes + MAX_VEGETATION_MESH_BYTES
    } else {
        terrain_bytes
    }
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
        .filter(|patch| !streaming.published.contains(patch))
        .filter(|patch| {
            matches!(
                streaming.manager.state_of(patch),
                Some(PatchState::Ready | PatchState::Cached)
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
    fn cached_geometry_is_regenerated_when_its_stitch_pattern_changes() {
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
        streaming.manager.mark_cached(&patch);
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
                stitch_mask: stitch_mask(&stitch_edges_for(
                    patch,
                    &BTreeSet::from([patch, equal_neighbor]),
                )),
            },
        );

        assert_eq!(
            stale_cached_stitch_variants(&streaming, &BTreeSet::from([patch, coarser_neighbor])),
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
            estimated_patch_bytes(coarse, 33) + MAX_VEGETATION_MESH_BYTES
        );
    }

    #[test]
    fn generation_capacity_bootstraps_roots_then_fills_all_available_workers() {
        let roots = TerrainPatch::roots().to_vec();
        let mut manager = TerrainPatchManager::new();
        for patch in &roots {
            manager.request(*patch, 1);
        }

        let generated = HashMap::new();
        let bootstrap_batch = generation_batch(
            &roots,
            &manager,
            &generated,
            generation_capacity(generated.is_empty(), 4, 0),
        );
        assert_eq!(bootstrap_batch, roots);

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
                stitch_mask: 0,
            },
        );
        manager.begin_generation(&focus);
        manager.mark_ready(&focus);
        manager.mark_visible(&focus);
        let later_batch = generation_batch(
            &roots,
            &manager,
            &generated,
            generation_capacity(generated.is_empty(), 4, 0),
        );
        assert_eq!(later_batch.len(), 4);
        assert_ne!(later_batch, vec![focus]);

        assert_eq!(generation_capacity(false, 4, 4), 0);
        assert_eq!(generation_capacity(false, 8, 3), 5);
    }
}
