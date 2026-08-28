//! Terrain rendering plugin (AGENTS.md sections 27-28).
//!
//! Spawns GPU meshes and materials for cube-sphere LOD terrain patches from the
//! streaming manager, with PBR shaders for planetary surfaces and a floating
//! origin for precision at planetary scale.

use crate::application::texture_config::{get_planet_textures, load_texture};
use crate::domain::entities::planet::Planet;
use crate::domain::services::cube_sphere::{
    direction_to_lat_lon, face_uv_to_direction, PatchGeometry, TerrainPatch,
};
use crate::domain::services::reference_frames::body_fixed_to_inertial_rotation;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_source::{slope_deg_at, surface_appearance, TerrainSource};
use crate::infrastructure::bevy_adapters::components::*;
use crate::infrastructure::bevy_adapters::terrain_streaming::{
    TerrainStreamingResource, AUTHORITATIVE_TERRAIN_LEVEL,
};
use crate::infrastructure::bevy_adapters::terrain_surface::{
    build_vegetation_mesh, supports_vegetation,
};
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::ecs::message::Message;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use bevy::time::Fixed;
use bevy_mesh::{Indices, PrimitiveTopology};

/// Component tracking the render state of a terrain patch.
#[derive(Component, Debug, Clone)]
pub struct TerrainPatchRenderState {
    pub patch: TerrainPatch,
    pub mesh_handle: Handle<Mesh>,
    pub material_handle: Handle<StandardMaterial>,
    pub vegetation_mesh_handle: Option<Handle<Mesh>>,
    pub planet_entity: Entity,
    /// Body-fixed-to-inertial rotation used to bake this mesh's vertices.
    pub body_to_inertial_at_spawn: DQuat,
    /// Render origin used to bake this mesh's vertices.
    pub render_origin_at_spawn: DVec3,
}

/// Reusable render assets whose appearance is identical for every terrain
/// patch. Patch terrain materials stay independent because roughness is derived
/// from their geographic surface sample.
#[derive(Resource, Default)]
struct TerrainRenderAssets {
    vegetation_material: Option<Handle<StandardMaterial>>,
}

/// Resource for the floating render origin (AGENTS.md section 13).
/// When the camera moves far from the origin, we re-center to avoid f32
/// precision loss.
#[derive(Resource, Debug, Default)]
pub struct RenderOrigin {
    pub origin: DVec3,
    pub last_camera_pos: DVec3,
}

/// Configuration for terrain rendering.
#[derive(Resource, Debug, Clone)]
pub struct TerrainRenderConfig {
    /// Distance threshold (meters) beyond which the render origin re-centers.
    pub recenter_threshold_m: f64,
    /// Skirt depth for LOD crack hiding (meters).
    pub skirt_depth_m: f64,
    /// Patch resolution (vertices per side).
    pub patch_resolution: u32,
}

impl Default for TerrainRenderConfig {
    fn default() -> Self {
        Self {
            recenter_threshold_m: 10_000.0,
            skirt_depth_m: 5.0,
            // 2^n + 1 samples preserve parent/child boundary sample alignment.
            patch_resolution: 33,
        }
    }
}

/// Events emitted by the streaming system when patch lifecycle changes.
/// These are observed by the render system to spawn/despawn meshes.
#[derive(Message, Debug, Clone)]
pub struct TerrainPatchReady {
    pub patch: TerrainPatch,
    pub planet_entity: Entity,
}

/// Emitted when a generated patch leaves the active leaf cover but remains in
/// the streaming cache. Its GPU assets stay resident for a zero-regeneration
/// return to visibility.
#[derive(Message, Debug, Clone)]
pub struct TerrainPatchCached {
    pub patch: TerrainPatch,
    pub planet_entity: Entity,
}

#[derive(Message, Debug, Clone)]
pub struct TerrainPatchEvicted {
    pub patch: TerrainPatch,
    pub planet_entity: Entity,
}

/// Plugin that registers terrain rendering systems for the rocket mode.
///
/// The render origin follows the rocket's inertial physical position. Resident
/// patch roots are rebased when it moves, while physical coordinates remain f64.
pub struct TerrainRenderPlugin;

impl Plugin for TerrainRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderOrigin>()
            .init_resource::<TerrainRenderConfig>()
            .init_resource::<TerrainRenderAssets>()
            .add_message::<TerrainPatchReady>()
            .add_message::<TerrainPatchCached>()
            .add_message::<TerrainPatchEvicted>()
            .add_systems(
                Update,
                (
                    recenter_render_origin,
                    update_patch_transforms,
                    hide_cached_patch_mesh_system,
                    reveal_cached_patch_mesh_system,
                    spawn_patch_mesh_system,
                    despawn_patch_mesh_system,
                )
                    .chain(),
            );
    }
}

/// System that spawns Bevy mesh/material entities when a terrain patch
/// becomes ready in the streaming lifecycle.
fn spawn_patch_mesh_system(
    mut commands: Commands,
    mut events: MessageReader<TerrainPatchReady>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut render_assets: ResMut<TerrainRenderAssets>,
    asset_server: Res<AssetServer>,
    streaming: Res<TerrainStreamingResource>,
    _config: Res<TerrainRenderConfig>,
    render_origin: Res<RenderOrigin>,
    sim_time: Res<SimulationTime>,
    time: Res<Time<Fixed>>,
    planet_query: Query<(&PlanetTerrain, &PlanetComponent)>,
    existing_patches: Query<&TerrainPatchRenderState>,
) {
    for event in events.read() {
        if existing_patches
            .iter()
            .any(|state| state.patch == event.patch && state.planet_entity == event.planet_entity)
        {
            continue;
        }
        let Ok((planet_terrain, planet)) = planet_query.get(event.planet_entity) else {
            continue;
        };
        let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
        let source = planet_terrain.source.as_ref();

        let patch = event.patch;
        let Some(cached_geometry) = streaming.generated.get(&patch) else {
            continue;
        };
        let geometry = &cached_geometry.geometry;

        // Build Bevy mesh from PatchGeometry, rebasing the planet-centered
        // geometry into the rocket-local flight frame so f32 mesh vertices stay
        // small near the camera (avoids precision loss at ~6371 km magnitudes
        // that degrades the sphere into a flat plane with broken triangles).
        let body_to_inertial = interpolated_body_to_inertial_rotation(
            &planet.domain_planet,
            &sim_time,
            time.overstep_fraction() as f64,
        );
        let mesh = patch_geometry_to_mesh(
            geometry,
            &render_origin.origin,
            body_to_inertial,
            source,
            patch.level,
            radius_m,
        );
        let mesh_handle = meshes.add(mesh);

        let mut material = patch_material(&patch, source, patch.level);
        material.base_color_texture = load_texture(
            &asset_server,
            get_planet_textures(&planet.domain_planet.name).albedo,
        );
        let material_handle = materials.add(material);

        // Geometry is already in the rocket-local flight frame; the entity sits
        // at the origin (the rocket's render position).
        let transform = Transform::IDENTITY;

        let vegetation_mesh_handle = if supports_vegetation(patch.level) {
            build_vegetation_mesh(
                source,
                &patch,
                radius_m,
                &render_origin.origin,
                body_to_inertial,
            )
            .map(|mesh| meshes.add(mesh))
        } else {
            None
        };
        let vegetation_material = vegetation_mesh_handle.as_ref().map(|_| {
            render_assets
                .vegetation_material
                .get_or_insert_with(|| {
                    materials.add(StandardMaterial {
                        base_color: Color::WHITE,
                        perceptual_roughness: 0.9,
                        metallic: 0.0,
                        ..default()
                    })
                })
                .clone()
        });

        let entity = commands
            .spawn((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(material_handle.clone()),
                transform,
                TerrainPatchRenderState {
                    patch,
                    mesh_handle: mesh_handle.clone(),
                    material_handle: material_handle.clone(),
                    vegetation_mesh_handle: vegetation_mesh_handle.clone(),
                    planet_entity: event.planet_entity,
                    body_to_inertial_at_spawn: body_to_inertial,
                    render_origin_at_spawn: render_origin.origin,
                },
                Name::new(format!(
                    "TerrainPatch_{:?}_{}_{}_{}",
                    patch.face, patch.level, patch.tile_x, patch.tile_y
                )),
            ))
            .id();

        // Merged vegetation + scatter (trees, rocks) is one child draw and
        // shares an immutable material across every terrain tile.
        if let (Some(vegetation_mesh_handle), Some(vegetation_material)) =
            (vegetation_mesh_handle.clone(), vegetation_material)
        {
            commands.entity(entity).with_children(|parent| {
                parent.spawn((
                    Mesh3d(vegetation_mesh_handle),
                    MeshMaterial3d(vegetation_material),
                    Transform::IDENTITY,
                    Name::new(format!(
                        "Vegetation_{:?}_{}_{}_{}",
                        patch.face, patch.level, patch.tile_x, patch.tile_y
                    )),
                ));
            });
        }
    }
}

/// Hide cached tile entities without destroying their mesh/material assets.
/// The ready handler restores these entities instead of rebuilding them.
fn hide_cached_patch_mesh_system(
    mut events: MessageReader<TerrainPatchCached>,
    mut render_query: Query<(&TerrainPatchRenderState, &mut Visibility)>,
) {
    for event in events.read() {
        if let Some((_, mut visibility)) = render_query.iter_mut().find(|(state, _)| {
            state.patch == event.patch && state.planet_entity == event.planet_entity
        }) {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Restore a cached tile before the ready handler considers creating new GPU
/// assets. A cache hit therefore performs no mesh conversion or asset upload.
fn reveal_cached_patch_mesh_system(
    mut events: MessageReader<TerrainPatchReady>,
    sim_time: Res<SimulationTime>,
    render_origin: Res<RenderOrigin>,
    time: Res<Time<Fixed>>,
    planet_query: Query<&PlanetComponent>,
    mut render_query: Query<(&TerrainPatchRenderState, &mut Transform, &mut Visibility)>,
) {
    for event in events.read() {
        for (state, mut transform, mut visibility) in render_query.iter_mut() {
            if state.patch != event.patch || state.planet_entity != event.planet_entity {
                continue;
            }
            if let Ok(planet) = planet_query.get(state.planet_entity) {
                update_patch_transform(
                    &mut transform,
                    state,
                    &planet.domain_planet,
                    &sim_time,
                    time.overstep_fraction() as f64,
                    render_origin.origin,
                );
            }
            *visibility = Visibility::Visible;
            break;
        }
    }
}

/// System that despawns mesh entities when a terrain patch is evicted.
fn despawn_patch_mesh_system(
    mut commands: Commands,
    mut events: MessageReader<TerrainPatchEvicted>,
    render_query: Query<(Entity, &TerrainPatchRenderState)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for event in events.read() {
        for (entity, state) in render_query.iter() {
            if state.patch == event.patch && state.planet_entity == event.planet_entity {
                commands.entity(entity).despawn();
                release_patch_render_assets(state, &mut meshes, &mut materials);
                break;
            }
        }
    }
}

fn release_patch_render_assets(
    state: &TerrainPatchRenderState,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    meshes.remove(state.mesh_handle.id());
    materials.remove(state.material_handle.id());
    if let Some(vegetation_mesh_handle) = &state.vegetation_mesh_handle {
        meshes.remove(vegetation_mesh_handle.id());
    }
}

/// Convert domain PatchGeometry to Bevy Mesh, rebasing planet-centered positions
/// into the rocket-local flight frame (`positions - render_origin`). This keeps
/// f32 vertex magnitudes small near the camera, preserving the spherical surface
/// instead of collapsing it into a flat plane at ~6371 km magnitudes.
fn patch_geometry_to_mesh(
    geometry: &PatchGeometry,
    render_origin: &DVec3,
    body_to_inertial: bevy::math::DQuat,
    source: &dyn TerrainSource,
    patch_level: u32,
    planet_radius_m: f64,
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );

    // Positions rebased to flight frame (f32 for GPU).
    let positions: Vec<[f32; 3]> = geometry
        .positions
        .iter()
        .map(|p| {
            let v = body_to_inertial * DVec3::from_array(*p) - *render_origin;
            [v.x as f32, v.y as f32, v.z as f32]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);

    // Normals.
    let normals: Vec<[f32; 3]> = geometry
        .normals
        .iter()
        .map(|n| {
            let n = body_to_inertial * DVec3::from_array(*n);
            [n.x as f32, n.y as f32, n.z as f32]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);

    // UV0 is geographic equirectangular UV so every tile, including roots,
    // samples the same global Earth imagery. The active StandardMaterial has no
    // tile-local UV consumer, so UV1 remains domain data until a custom material
    // actually needs it.
    let uvs = geometry.uvs.to_vec();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        terrain_vertex_colors(geometry, source, patch_level, planet_radius_m),
    );

    // Indices.
    mesh.insert_indices(Indices::U32(geometry.indices.clone()));

    mesh
}

/// Blend procedural biome tint in only after close-range refinement. Global
/// imagery remains the base layer at every level; vertex colors provide a
/// stable renderer-supported fallback until terrain uses a custom material.
fn terrain_vertex_colors(
    geometry: &PatchGeometry,
    source: &dyn TerrainSource,
    patch_level: u32,
    planet_radius_m: f64,
) -> Vec<[f32; 4]> {
    let detail = ((patch_level as f32 - 5.0) / 3.0).clamp(0.0, 1.0);
    if detail == 0.0 {
        return vec![[1.0, 1.0, 1.0, 1.0]; geometry.positions.len()];
    }
    geometry
        .positions
        .iter()
        .zip(&geometry.normals)
        .map(|(position, normal)| {
            let (lat, lon) = direction_to_lat_lon(DVec3::from_array(*position));
            // Geometry was sampled from this same surface already. Reuse its
            // radial height and normal-derived slope instead of repeating five
            // terrain-source calls for every render vertex.
            let radial = DVec3::from_array(*position).normalize();
            let height_m = DVec3::from_array(*position).length() - planet_radius_m;
            let slope_deg = DVec3::from_array(*normal)
                .normalize()
                .dot(radial)
                .clamp(-1.0, 1.0)
                .acos()
                .to_degrees();
            let appearance = if patch_level < AUTHORITATIVE_TERRAIN_LEVEL {
                surface_appearance(
                    height_m,
                    source.overview_moisture(lat, lon),
                    source.zone_lat(lat),
                    slope_deg,
                )
            } else {
                surface_appearance(
                    height_m,
                    source.moisture(lat, lon),
                    source.zone_lat(lat),
                    slope_deg,
                )
            };
            [
                1.0 + (appearance.albedo[0] - 1.0) * detail * 0.25,
                1.0 + (appearance.albedo[1] - 1.0) * detail * 0.25,
                1.0 + (appearance.albedo[2] - 1.0) * detail * 0.25,
                1.0,
            ]
        })
        .collect()
}

/// Shift only presentation coordinates when the rocket has moved far enough
/// from the current local origin. Existing terrain mesh vertices stay valid;
/// their root transforms preserve world placement until regenerated.
pub fn recenter_render_origin(
    config: Res<TerrainRenderConfig>,
    rocket_query: Query<&RocketPhysicsState>,
    mut render_origin: ResMut<RenderOrigin>,
) {
    let Some(rocket) = rocket_query.iter().next() else {
        return;
    };
    let new_origin = rocket.dynamics.position_m;
    if (new_origin - render_origin.origin).length() < config.recenter_threshold_m {
        return;
    }
    render_origin.origin = new_origin;
    render_origin.last_camera_pos = new_origin;
}

/// Keep visible terrain meshes attached to the rotating planet after generation.
/// Cached meshes are refreshed immediately before reveal, avoiding transform
/// writes for geometry that is not currently rendered.
fn update_patch_transforms(
    sim_time: Res<SimulationTime>,
    render_origin: Res<RenderOrigin>,
    time: Res<Time<Fixed>>,
    planet_query: Query<&PlanetComponent>,
    mut patch_query: Query<(&TerrainPatchRenderState, &mut Transform, &Visibility)>,
) {
    let alpha = time.overstep_fraction() as f64;
    for (state, mut transform, visibility) in patch_query.iter_mut() {
        if *visibility == Visibility::Hidden {
            continue;
        }
        let Ok(planet) = planet_query.get(state.planet_entity) else {
            continue;
        };
        update_patch_transform(
            &mut transform,
            state,
            &planet.domain_planet,
            &sim_time,
            alpha,
            render_origin.origin,
        );
    }
}

fn update_patch_transform(
    transform: &mut Transform,
    state: &TerrainPatchRenderState,
    planet: &Planet,
    sim_time: &SimulationTime,
    alpha: f64,
    render_origin: DVec3,
) {
    let body_to_inertial = interpolated_body_to_inertial_rotation(planet, sim_time, alpha);
    let (rotation, translation) = patch_transform_components(
        state.body_to_inertial_at_spawn,
        state.render_origin_at_spawn,
        body_to_inertial,
        render_origin,
    );
    transform.rotation = rotation.as_quat();
    transform.translation = translation.as_vec3();
}

/// Match the previous/current fixed-step body poses used by rocket presentation.
fn interpolated_body_to_inertial_rotation(
    planet: &Planet,
    sim_time: &SimulationTime,
    alpha: f64,
) -> DQuat {
    let previous_time_s = (sim_time.sim_time_s - sim_time.fixed_timestep()).max(0.0);
    let previous = body_fixed_to_inertial_rotation(planet, (previous_time_s / 86_400.0) as f32);
    let current = body_fixed_to_inertial_rotation(planet, (sim_time.sim_time_s / 86_400.0) as f32);
    previous.slerp(current, alpha)
}

/// Return the presentation-only transform taking a baked terrain patch into the
/// current interpolated body pose and render-origin frame.
fn patch_transform_components(
    body_to_inertial_at_spawn: DQuat,
    render_origin_at_spawn: DVec3,
    body_to_inertial: DQuat,
    render_origin: DVec3,
) -> (DQuat, DVec3) {
    let rotation = body_to_inertial * body_to_inertial_at_spawn.conjugate();
    let translation = rotation * render_origin_at_spawn - render_origin;
    (rotation, translation)
}

/// Create the base terrain material. The albedo and normal map are supplied per
/// patch by `build_patch_surfaces` (set by the caller); this only provides the
/// representative roughness from the shared `surface_appearance` law (AGENTS.md
/// 50: one authoritative appearance law).
fn patch_material(
    patch: &TerrainPatch,
    source: &dyn TerrainSource,
    patch_level: u32,
) -> StandardMaterial {
    let (u0, v0, u1, v1) = patch.uv_bounds();
    let u_mid = (u0 + u1) * 0.5;
    let v_mid = (v0 + v1) * 0.5;
    let dir = face_uv_to_direction(patch.face, u_mid, v_mid);
    let (lat, lon) = direction_to_lat_lon(dir);
    let (height, moisture, slope) = if patch_level < AUTHORITATIVE_TERRAIN_LEVEL {
        (
            source.overview_height_m(lat, lon),
            source.overview_moisture(lat, lon),
            source.overview_slope_deg(lat, lon),
        )
    } else {
        (
            source.height_m(lat, lon),
            source.moisture(lat, lon),
            slope_deg_at(source, lat, lon),
        )
    };
    let zone = source.zone_lat(lat);
    let appearance = surface_appearance(height, moisture, zone, slope);

    StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: appearance.roughness,
        metallic: appearance.metallic,
        unlit: false,
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::cube_sphere::build_patch_geometry;
    use crate::domain::services::planet_factory::PlanetFactory;
    use bevy::ecs::message::Messages;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Default)]
    struct CountingTerrainSource {
        authoritative_height_calls: AtomicUsize,
        overview_height_calls: AtomicUsize,
    }

    impl TerrainSource for CountingTerrainSource {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            self.authoritative_height_calls
                .fetch_add(1, Ordering::Relaxed);
            0.0
        }

        fn overview_height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            self.overview_height_calls.fetch_add(1, Ordering::Relaxed);
            0.0
        }

        fn moisture(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            0.5
        }

        fn overview_moisture(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            0.5
        }

        fn overview_slope_deg(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            0.0
        }
    }

    fn terrain_position_in_render_frame(
        body_fixed_position_m: DVec3,
        body_to_inertial_at_spawn: DQuat,
        render_origin_at_spawn: DVec3,
        body_to_inertial: DQuat,
        render_origin: DVec3,
    ) -> DVec3 {
        let baked_position =
            body_to_inertial_at_spawn * body_fixed_position_m - render_origin_at_spawn;
        let (rotation, translation) = patch_transform_components(
            body_to_inertial_at_spawn,
            render_origin_at_spawn,
            body_to_inertial,
            render_origin,
        );
        rotation * baked_position + translation
    }

    #[test]
    fn patch_geometry_emits_skirt_ring_for_crack_free_lod() {
        // Spec scenario "skirt geometry stitches edges": adjacent patches at
        // different LOD must not gap. The geometry-level guarantee is a
        // boundary skirt ring: for a res×res patch, every boundary vertex gets
        // an extra extruded vertex and a skirt quad is emitted per segment.
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 2);
        let res = 5usize;
        let geom = build_patch_geometry(
            &patch,
            &crate::domain::services::terrain_source::ProceduralTerrainSource::new(
                99, 2_000.0, 800.0, 0,
            ),
            6_371_000.0,
            res as u32,
            40.0,
        );
        // Platform vertices + one skirt vertex per boundary vertex.
        let boundary_count = 4 * (res - 1); // res*res grid has 4(res-1) boundary verts
        assert_eq!(
            geom.positions.len(),
            res * res + boundary_count,
            "expected skirt ring appended"
        );
        // Skirt vertices must be extruded downward (closer to the planet than a
        // corresponding grid vertex, so the crack is hidden rather than opened).
        let non_skirt = res * res;
        for pos in geom.positions.iter().skip(non_skirt) {
            let r = DVec3::from_array(*pos).length();
            assert!(
                r < 6_371_000.0 + 2_900.0,
                "skirt vertex at radius {r} not extruded inward"
            );
        }
        // Skirt quads are present: more indices than a flat grid alone.
        assert!(geom.indices.len() > (res - 1) * (res - 1) * 6);
    }

    #[test]
    fn interpolated_terrain_matches_surface_fixed_rocket_across_fixed_overstep() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let mut sim_time = SimulationTime::new(0.25);
        sim_time.sim_time_s = 12_345.0;
        let surface_position_m = DVec3::new(earth.radius_km as f64 * 1_000.0, 0.0, 0.0);
        let body_to_inertial_at_spawn = body_fixed_to_inertial_rotation(&earth, 0.1);
        let render_origin_at_spawn = DVec3::new(100.0, -200.0, 300.0);
        let current_body_to_inertial =
            body_fixed_to_inertial_rotation(&earth, (sim_time.sim_time_s / 86_400.0) as f32);
        let render_origin = current_body_to_inertial * surface_position_m;

        for alpha in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let body_to_inertial = interpolated_body_to_inertial_rotation(&earth, &sim_time, alpha);
            let terrain_position = terrain_position_in_render_frame(
                surface_position_m,
                body_to_inertial_at_spawn,
                render_origin_at_spawn,
                body_to_inertial,
                render_origin,
            );
            let rocket_position = body_to_inertial * surface_position_m - render_origin;

            assert!(
                terrain_position.distance(rocket_position) < 1e-7,
                "terrain diverged from a surface-fixed rocket at alpha {alpha}"
            );
        }
    }

    #[test]
    fn newly_spawned_patch_uses_the_interpolated_body_pose() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let mut sim_time = SimulationTime::new(0.25);
        sim_time.sim_time_s = 12_345.0;
        let alpha = 0.5;
        let body_to_inertial = interpolated_body_to_inertial_rotation(&earth, &sim_time, alpha);
        let surface_position_m = DVec3::new(0.0, earth.radius_km as f64 * 1_000.0, 0.0);
        let render_origin = DVec3::new(10.0, 20.0, 30.0);

        let (rotation, translation) = patch_transform_components(
            body_to_inertial,
            render_origin,
            body_to_inertial,
            render_origin,
        );
        let terrain_position = terrain_position_in_render_frame(
            surface_position_m,
            body_to_inertial,
            render_origin,
            body_to_inertial,
            render_origin,
        );

        assert!(rotation.abs_diff_eq(DQuat::IDENTITY, 1e-12));
        assert!(translation.abs_diff_eq(DVec3::ZERO, 1e-12));
        assert!(terrain_position
            .abs_diff_eq(body_to_inertial * surface_position_m - render_origin, 1e-7));
    }

    #[test]
    fn global_imagery_coordinates_keep_coarse_tiles_neutral_and_fine_tiles_tinted() {
        let source = crate::domain::services::terrain_source::ProceduralTerrainSource::new(
            99, 2_000.0, 800.0, 0,
        );
        let patch = TerrainPatch::for_direction(DVec3::new(0.3, 0.4, 1.0).normalize(), 2);
        let geometry = build_patch_geometry(&patch, &source, 6_371_000.0, 5, 40.0);
        let coarse = terrain_vertex_colors(&geometry, &source, 0, 6_371_000.0);
        let fine = terrain_vertex_colors(&geometry, &source, 8, 6_371_000.0);

        assert!(coarse.iter().all(|color| *color == [1.0, 1.0, 1.0, 1.0]));
        assert!(fine.iter().any(|color| *color != [1.0, 1.0, 1.0, 1.0]));
        assert_eq!(geometry.uvs.len(), geometry.local_uvs.len());
    }

    #[test]
    fn vertex_colors_reuse_geometry_without_resampling_height_or_slope() {
        let source = CountingTerrainSource::default();
        let geometry = PatchGeometry {
            positions: vec![(DVec3::X * 6_371_000.0).to_array()],
            normals: vec![DVec3::X.to_array()],
            uvs: vec![[0.0, 0.0]],
            local_uvs: vec![[0.0, 0.0]],
            indices: Vec::new(),
        };

        terrain_vertex_colors(
            &geometry,
            &source,
            AUTHORITATIVE_TERRAIN_LEVEL - 1,
            6_371_000.0,
        );
        assert_eq!(
            source.authoritative_height_calls.load(Ordering::Relaxed),
            0,
            "vertex colors must reuse the height stored in patch geometry"
        );
        assert_eq!(source.overview_height_calls.load(Ordering::Relaxed), 0);

        terrain_vertex_colors(&geometry, &source, AUTHORITATIVE_TERRAIN_LEVEL, 6_371_000.0);
        assert_eq!(source.authoritative_height_calls.load(Ordering::Relaxed), 0);
        assert_eq!(source.overview_height_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn cached_patch_reuses_its_render_entity_when_republished() {
        let mut app = App::new();
        app.insert_resource(SimulationTime::default())
            .insert_resource(RenderOrigin::default())
            .insert_resource(Time::<Fixed>::default())
            .add_message::<TerrainPatchCached>()
            .add_message::<TerrainPatchReady>()
            .add_systems(
                Update,
                (
                    hide_cached_patch_mesh_system,
                    reveal_cached_patch_mesh_system,
                )
                    .chain(),
            );

        let patch = TerrainPatch::for_direction(DVec3::X, 2);
        let planet_entity = Entity::PLACEHOLDER;
        let entity = app
            .world_mut()
            .spawn((
                TerrainPatchRenderState {
                    patch,
                    mesh_handle: Handle::default(),
                    material_handle: Handle::default(),
                    vegetation_mesh_handle: None,
                    planet_entity,
                    body_to_inertial_at_spawn: DQuat::IDENTITY,
                    render_origin_at_spawn: DVec3::ZERO,
                },
                Transform::IDENTITY,
                Visibility::Visible,
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<TerrainPatchCached>>()
            .write(TerrainPatchCached {
                patch,
                planet_entity,
            });
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );

        app.world_mut()
            .resource_mut::<Messages<TerrainPatchReady>>()
            .write(TerrainPatchReady {
                patch,
                planet_entity,
            });
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Visible
        );
        assert!(app.world().get_entity(entity).is_ok());
    }

    #[test]
    fn evicting_a_patch_releases_its_unique_render_assets() {
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mesh_handle = meshes.add(Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        ));
        let vegetation_mesh_handle = meshes.add(Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        ));
        let material_handle = materials.add(StandardMaterial::default());
        let shared_vegetation_material = materials.add(StandardMaterial::default());
        let state = TerrainPatchRenderState {
            patch: TerrainPatch::for_direction(DVec3::X, 0),
            mesh_handle: mesh_handle.clone(),
            material_handle: material_handle.clone(),
            vegetation_mesh_handle: Some(vegetation_mesh_handle.clone()),
            planet_entity: Entity::PLACEHOLDER,
            body_to_inertial_at_spawn: DQuat::IDENTITY,
            render_origin_at_spawn: DVec3::ZERO,
        };

        release_patch_render_assets(&state, &mut meshes, &mut materials);

        assert!(meshes.get(mesh_handle.id()).is_none());
        assert!(meshes.get(vegetation_mesh_handle.id()).is_none());
        assert!(materials.get(material_handle.id()).is_none());
        assert!(materials.get(shared_vegetation_material.id()).is_some());
    }
}
