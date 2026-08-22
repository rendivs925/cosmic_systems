//! Terrain rendering plugin (AGENTS.md sections 27-28).
//!
//! Spawns GPU meshes and materials for cube-sphere LOD terrain patches from the
//! streaming manager, with PBR shaders for planetary surfaces and a floating
//! origin for precision at planetary scale.

use crate::domain::services::cube_sphere::{face_uv_to_direction, direction_to_lat_lon, TerrainPatch, PatchGeometry};
use crate::domain::services::terrain_source::TerrainSource;
use crate::infrastructure::bevy_adapters::components::*;
use crate::infrastructure::bevy_adapters::terrain_streaming::TerrainStreamingResource;
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::ecs::message::Message;
use bevy::math::{DVec3, Vec3};
use bevy::prelude::*;
use bevy_mesh::{Indices, PrimitiveTopology};

/// Component tracking the render state of a terrain patch.
#[derive(Component, Debug, Clone)]
pub struct TerrainPatchRenderState {
    pub patch: TerrainPatch,
    pub mesh_handle: Handle<Mesh>,
    pub material_handle: Handle<StandardMaterial>,
    pub entity: Entity,
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
            skirt_depth_m: 50.0,
            patch_resolution: 8,
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

#[derive(Message, Debug, Clone)]
pub struct TerrainPatchEvicted {
    pub patch: TerrainPatch,
    pub planet_entity: Entity,
}

/// Plugin that registers terrain rendering systems for the rocket mode.
pub struct TerrainRenderPlugin;

impl Plugin for TerrainRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderOrigin>()
            .init_resource::<TerrainRenderConfig>()
            .add_message::<TerrainPatchReady>()
            .add_message::<TerrainPatchEvicted>()
            .add_systems(
                Update,
                (
                    spawn_patch_mesh_system,
                    despawn_patch_mesh_system,
                    update_render_origin_system,
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
    streaming: Res<TerrainStreamingResource>,
    config: Res<TerrainRenderConfig>,
    planet_query: Query<&PlanetTerrain>,
    planet_entities: Query<Entity, With<PlanetComponent>>,
) {
    for event in events.read() {
        let Some(planet_entity) = planet_entities.iter().find(|e| *e == event.planet_entity) else {
            continue;
        };
        let Ok(planet_terrain) = planet_query.get(planet_entity) else {
            continue;
        };

        let patch = event.patch;
        let Some(geometry) = streaming.generated.get(&patch) else {
            continue;
        };

        // Build Bevy mesh from PatchGeometry.
        let mesh = patch_geometry_to_mesh(geometry, &config);
        let mesh_handle = meshes.add(mesh);

        // Create material with biome-appropriate properties.
        let material = patch_material(&patch, planet_terrain.source.as_ref(), &config);
        let material_handle = materials.add(material);

        // Compute patch transform relative to planet center.
        // The patch geometry is already in planet-centered coordinates,
        // so we just need to position it at the planet center (origin).
        // The floating origin will adjust all patch transforms.
        let transform = Transform::from_translation(Vec3::ZERO);

        let entity = commands
            .spawn((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(material_handle.clone()),
                transform,
                TerrainPatchRenderState {
                    patch,
                    mesh_handle: mesh_handle.clone(),
                    material_handle: material_handle.clone(),
                    entity: Entity::PLACEHOLDER,
                },
                Name::new(format!("TerrainPatch_{:?}_{}_{}_{}", patch.face, patch.level, patch.tile_x, patch.tile_y)),
            ))
            .id();

        // Update the entity reference in the component.
        commands.entity(entity).insert(TerrainPatchRenderState {
            patch,
            mesh_handle,
            material_handle,
            entity,
        });
    }
}

/// System that despawns mesh entities when a terrain patch is evicted.
fn despawn_patch_mesh_system(
    mut commands: Commands,
    mut events: MessageReader<TerrainPatchEvicted>,
    render_query: Query<(Entity, &TerrainPatchRenderState)>,
) {
    for event in events.read() {
        for (entity, state) in render_query.iter() {
            if state.patch == event.patch {
                commands.entity(entity).despawn();
                break;
            }
        }
    }
}

/// System that updates the floating render origin when the camera moves
/// beyond the threshold, and adjusts all patch transforms accordingly.
fn update_render_origin_system(
    mut render_origin: ResMut<RenderOrigin>,
    config: Res<TerrainRenderConfig>,
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    mut patch_query: Query<&mut Transform, With<TerrainPatchRenderState>>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let camera_pos = DVec3::new(
        camera_transform.translation().x as f64,
        camera_transform.translation().y as f64,
        camera_transform.translation().z as f64,
    );

    let offset = camera_pos - render_origin.origin;
    let distance = offset.length();

    if distance > config.recenter_threshold_m {
        // Re-center the origin to the camera position.
        let delta = offset;
        render_origin.origin = camera_pos;
        render_origin.last_camera_pos = camera_pos;

        // Shift all patch transforms by -delta.
        for mut transform in patch_query.iter_mut() {
            let current = DVec3::new(
                transform.translation.x as f64,
                transform.translation.y as f64,
                transform.translation.z as f64,
            );
            let new_pos = current - delta;
            transform.translation = Vec3::new(
                new_pos.x as f32,
                new_pos.y as f32,
                new_pos.z as f32,
            );
        }
    }
}

/// Convert domain PatchGeometry to Bevy Mesh.
fn patch_geometry_to_mesh(geometry: &PatchGeometry, config: &TerrainRenderConfig) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);

    // Positions (f32 for GPU).
    let positions: Vec<[f32; 3]> = geometry
        .positions
        .iter()
        .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);

    // Normals.
    let normals: Vec<[f32; 3]> = geometry
        .normals
        .iter()
        .map(|n| [n[0] as f32, n[1] as f32, n[2] as f32])
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);

    // UVs (simple planar projection for now; can be enhanced).
    let uvs: Vec<[f32; 2]> = geometry
        .positions
        .iter()
        .map(|p| {
            let x = p[0] as f64;
            let y = p[1] as f64;
            let z = p[2] as f64;
            // Spherical UV mapping.
            let u = (z.atan2(x) + std::f64::consts::PI) / (2.0 * std::f64::consts::PI);
            let v = (y / (x * x + y * y + z * z).sqrt()).asin() / std::f64::consts::PI + 0.5;
            [u as f32, v as f32]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);

    // Indices.
    mesh.insert_indices(Indices::U32(geometry.indices.clone()));

    mesh
}

/// Create a material for a terrain patch based on biome/altitude.
fn patch_material(patch: &TerrainPatch, source: &dyn TerrainSource, config: &TerrainRenderConfig) -> StandardMaterial {
    // Sample height at patch center for biome/altitude classification.
    let (u0, v0, u1, v1) = patch.uv_bounds();
    let u_mid = (u0 + u1) * 0.5;
    let v_mid = (v0 + v1) * 0.5;
    let dir = face_uv_to_direction(patch.face, u_mid, v_mid);
    let (lat, lon) = direction_to_lat_lon(dir);
    let height = source.height_m(lat, lon);

    // Biome classification based on height (simple for now).
    let (albedo, roughness, metallic) = biome_properties(height);

    StandardMaterial {
        base_color: Color::srgb(albedo.0, albedo.1, albedo.2),
        perceptual_roughness: roughness,
        metallic,
        ..default()
    }
}

/// Biome properties from height (placeholder - will be enhanced with actual biome system).
fn biome_properties(height_m: f64) -> ((f32, f32, f32), f32, f32) {
    if height_m > 3000.0 {
        // High mountains: rocky, rough.
        ((0.4, 0.35, 0.3), 0.9, 0.0)
    } else if height_m > 1000.0 {
        // Mountains/hills.
        ((0.45, 0.4, 0.35), 0.85, 0.0)
    } else if height_m > 100.0 {
        // Lowlands: grass/dirt.
        ((0.35, 0.4, 0.25), 0.8, 0.0)
    } else if height_m > -100.0 {
        // Near sea level: sand/coastal.
        ((0.5, 0.45, 0.3), 0.7, 0.0)
    } else {
        // Ocean: water.
        ((0.1, 0.2, 0.4), 0.1, 0.0)
    }
}