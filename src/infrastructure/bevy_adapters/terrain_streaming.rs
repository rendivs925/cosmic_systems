//! Cube-sphere terrain streaming (AGENTS.md sections 22-23).
//!
//! A `TerrainPatchManager` resource is driven each tick by a streaming system
//! that keeps the quadtree patch set around the rocket/camera alive through the
//! requested → generating → ready → visible → cached → evicted lifecycle, and
//! enforces the configured memory budget by evicting least-recently-used cached
//! patches. Generated patch geometry is built deterministically from the shared
//! per-planet `TerrainSource`; only patches near the focus are ever requested,
//! never the whole planet.

use crate::domain::services::cube_sphere::{
    build_patch_geometry, lod_for_distance, patch_world_size_m, PatchGeometry, TerrainPatch,
};
use crate::domain::services::reference_frames::planet_inertial_to_body_fixed;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_patch_manager::{PatchState, TerrainPatchManager};
use crate::infrastructure::bevy_adapters::components::*;
use crate::infrastructure::bevy_adapters::terrain_render::{
    TerrainPatchEvicted, TerrainPatchReady, TerrainRenderConfig,
};
use bevy::prelude::*;
use std::collections::HashMap;

/// Camera/LOD constants.
/// MAX_PATCH_LEVEL=15 gives ~305 m patches near the launch site. The coarse
/// Earth presentation proxy supplies the continuous globe beyond this local
/// high-detail window.
const MAX_PATCH_LEVEL: u32 = 15;
const FOV_RAD: f64 = 1.0;
const SCREEN_HEIGHT_PX: f64 = 1080.0;
const SCREEN_ERROR_PX: f64 = 4.0;
/// Approximate bytes per generated patch vertex (f64 position + normal).
const BYTES_PER_VERTEX: u64 = 48;
const DEFAULT_BUDGET_BYTES: u64 = 128 * 1024 * 1024;

/// Minimum distance for LOD calculation when on the ground.
/// Uses estimated camera-to-terrain distance (~150m) instead of orbital heuristic.
const SURFACE_LOD_DISTANCE_M: f64 = 150.0;
/// Altitude threshold below which surface LOD distance is used.
const SURFACE_LOD_ALTITUDE_THRESHOLD_M: f64 = 10_000.0;
/// Above this altitude the coarse planet proxy is the continuous visual
/// surface. Rendering a 3x3 local patch window here produces a visibly flat
/// slab and obscures the actual planetary horizon.
pub(crate) const LOCAL_TERRAIN_MAX_ALTITUDE_M: f64 = 20_000.0;

pub(crate) fn local_terrain_is_required(altitude_m: f64) -> bool {
    altitude_m <= LOCAL_TERRAIN_MAX_ALTITUDE_M
}

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
    pub generated: HashMap<TerrainPatch, PatchGeometry>,
}

impl Default for TerrainStreamingResource {
    fn default() -> Self {
        Self {
            manager: TerrainPatchManager::new(),
            budget_bytes: DEFAULT_BUDGET_BYTES,
            generated: HashMap::new(),
        }
    }
}

/// Streaming system: request the patch set around the rocket (falling back to
/// the camera view for the dominant body), drive lifecycle, generate
/// deterministic geometry from the shared terrain source, and enforce the
/// memory budget. It only updates the streaming resource; it never writes
/// rendered geometry or the rocket's state.
pub fn stream_terrain_patches(
    mut streaming: ResMut<TerrainStreamingResource>,
    planet_query: Query<(Entity, &PlanetComponent, &PlanetTerrain)>,
    rocket_query: Query<(&RocketPlanetBinding, &RocketPhysicsState)>,
    mut ready_events: MessageWriter<TerrainPatchReady>,
    mut evicted_events: MessageWriter<TerrainPatchEvicted>,
    config: Res<TerrainRenderConfig>,
    sim_time: Res<SimulationTime>,
) {
    streaming.manager.tick();

    // No rocket yet: keep the manager tidy and return.
    let Some((binding, rocket)) = rocket_query.iter().next() else {
        let budget = streaming.budget_bytes;
        let evicted = streaming.manager.enforce_memory_budget(budget);
        for patch in evicted {
            evicted_events.write(TerrainPatchEvicted {
                patch,
                planet_entity: Entity::PLACEHOLDER, // No planet when no rocket
            });
        }
        return;
    };
    let Some((planet_entity, _planet, planet_terrain)) = planet_query
        .iter()
        .find(|(_, planet, _)| planet.matches_body(&binding.planet_name))
    else {
        return;
    };

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

    if !local_terrain_is_required(altitude_m) {
        // Local terrain is presentation detail, not the planetary base mesh.
        // Release all resident patches when the vehicle enters a planetary
        // view so only the true-scale globe defines the horizon.
        let visible: Vec<TerrainPatch> = streaming.manager.visible_patches().copied().collect();
        for patch in visible {
            streaming.manager.mark_cached(&patch);
        }
        for patch in streaming.manager.enforce_memory_budget(0) {
            streaming.generated.remove(&patch);
            evicted_events.write(TerrainPatchEvicted {
                patch,
                planet_entity,
            });
        }
        return;
    }

    // Terrain source coordinates are planet body-fixed geographic coordinates;
    // the rocket state remains planet-centered inertial everywhere else.
    let position_bf = planet_inertial_to_body_fixed(
        position_m,
        &_planet.domain_planet,
        (sim_time.sim_time_s / 86_400.0) as f32,
    );
    let dir = position_bf.normalize_or_zero();

    // Request the ring of patches around the focus direction at the LOD
    // appropriate for the current altitude. The LOD distance is blended
    // continuously between the "on the ground" camera-to-terrain estimate and
    // the orbital heuristic over a band around the threshold, so the LOD level
    // steps one level at a time instead of jumping several at once when the
    // rocket passes 10 km (avoids the visible full-window pop flicker).
    let ground_distance_m = SURFACE_LOD_DISTANCE_M;
    let orbital_distance_m = (altitude_m + radius_m * 0.05).max(10_000.0);
    let blend = smoothstep(
        SURFACE_LOD_ALTITUDE_THRESHOLD_M * 0.5,
        SURFACE_LOD_ALTITUDE_THRESHOLD_M,
        altitude_m,
    );
    let lod_distance_m =
        ground_distance_m + (orbital_distance_m.max(ground_distance_m) - ground_distance_m) * blend;

    let level = lod_for_distance(
        lod_distance_m,
        radius_m,
        FOV_RAD,
        SCREEN_HEIGHT_PX,
        SCREEN_ERROR_PX,
        MAX_PATCH_LEVEL,
    );
    let focus = TerrainPatch::for_direction(dir, level);

    // Track patches that were already visible to detect newly ready ones.
    let previously_visible: Vec<TerrainPatch> =
        streaming.manager.visible_patches().copied().collect();

    // Drive lifecycle for the focus window and generate geometry.
    let window = surrounding_patches(&focus);
    for patch in window.iter() {
        let resolution = config.patch_resolution as u64;
        let size_bytes = BYTES_PER_VERTEX * resolution * resolution;
        streaming.manager.request(*patch, size_bytes);

        let was_ready = matches!(
            streaming.manager.state_of(patch),
            Some(PatchState::Ready) | Some(PatchState::Visible)
        );

        streaming.manager.begin_generation(patch);
        let geometry = build_patch_geometry(
            patch,
            planet_terrain.source.as_ref(),
            radius_m,
            config.patch_resolution,
            config.skirt_depth_m,
        );
        streaming.generated.insert(*patch, geometry);
        streaming.manager.mark_ready(patch);
        streaming.manager.mark_visible(patch);

        // Emit ready event for newly ready patches.
        if !was_ready {
            ready_events.write(TerrainPatchReady {
                patch: *patch,
                planet_entity,
            });
        }
    }

    // Move departed patches to the cache. They remain renderable and can be
    // reused until the manager evicts them under the configured memory budget.
    for patch in previously_visible {
        if !window.contains(&patch) {
            streaming.manager.mark_cached(&patch);
        }
    }

    let budget = streaming.budget_bytes;
    let evicted = streaming.manager.enforce_memory_budget(budget);
    for patch in evicted {
        streaming.generated.remove(&patch);
        evicted_events.write(TerrainPatchEvicted {
            patch,
            planet_entity,
        });
    }
}

/// The focus patch plus its neighbors at the same level (a 3×3 patch window
/// clipped to the face bounds).
fn surrounding_patches(focus: &TerrainPatch) -> Vec<TerrainPatch> {
    let span = (1u64 << focus.level) as i64;
    let cx = focus.tile_x as i64;
    let cy = focus.tile_y as i64;
    let mut out = Vec::with_capacity(9);
    for dy in -1..=1i64 {
        for dx in -1..=1i64 {
            let tx = cx + dx;
            let ty = cy + dy;
            if tx < 0 || tx >= span || ty < 0 || ty >= span {
                continue; // across a face edge; neighbor faces are out of scope here
            }
            out.push(TerrainPatch {
                face: focus.face,
                level: focus.level,
                tile_x: tx as u32,
                tile_y: ty as u32,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::cube_sphere::CubeFace;

    #[test]
    fn local_terrain_is_limited_to_near_surface_views() {
        assert!(local_terrain_is_required(0.0));
        assert!(local_terrain_is_required(LOCAL_TERRAIN_MAX_ALTITUDE_M));
        assert!(!local_terrain_is_required(
            LOCAL_TERRAIN_MAX_ALTITUDE_M + 1.0
        ));
    }

    #[test]
    fn focus_window_has_neighbors_within_face() {
        let focus = TerrainPatch {
            face: CubeFace::PosZ,
            level: 2,
            tile_x: 2,
            tile_y: 2,
        };
        let window = surrounding_patches(&focus);
        // Interior 3×3 window.
        assert_eq!(window.len(), 9);
        assert!(window.contains(&focus));
    }

    #[test]
    fn focus_window_clips_at_face_edge() {
        let focus = TerrainPatch {
            face: CubeFace::PosX,
            level: 1,
            tile_x: 0,
            tile_y: 0,
        };
        let window = surrounding_patches(&focus);
        // Corner tile: 2 of the 8 neighbors are outside the face.
        assert_eq!(window.len(), 4);
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
}
