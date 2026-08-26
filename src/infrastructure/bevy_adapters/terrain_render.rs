//! Terrain rendering plugin (AGENTS.md sections 27-28).
//!
//! Spawns GPU meshes and materials for cube-sphere LOD terrain patches from the
//! streaming manager, with PBR shaders for planetary surfaces and a floating
//! origin for precision at planetary scale.

use crate::domain::services::cube_sphere::{
    direction_to_lat_lon, face_uv_to_direction, PatchGeometry, TerrainPatch,
};
use crate::domain::services::terrain_source::TerrainSource;
use crate::infrastructure::bevy_adapters::components::*;
use crate::infrastructure::bevy_adapters::terrain_streaming::TerrainStreamingResource;
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::ecs::message::Message;
use bevy::math::DVec3;
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
///
/// The render origin is fixed once at startup (`setup_rocket_camera_and_origin`)
/// to the rocket's physical position, and the streaming system keeps patches
/// generated around the rocket's current sub-point. The camera remains near the
/// rocket in flight units, so no auto-recentering is needed in rocket mode.
/// The legacy `update_render_origin_system` (solar-system camera frame) is not
/// registered here because it misinterprets the rocket-mode flight-unit camera
/// as physics meters and scatters the patches far from the vehicle.
pub struct TerrainRenderPlugin;

impl Plugin for TerrainRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderOrigin>()
            .init_resource::<TerrainRenderConfig>()
            .add_message::<TerrainPatchReady>()
            .add_message::<TerrainPatchEvicted>()
            .add_systems(
                Update,
                (spawn_patch_mesh_system, despawn_patch_mesh_system).chain(),
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
    render_origin: Res<RenderOrigin>,
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

        // Build Bevy mesh from PatchGeometry, rebasing the planet-centered
        // geometry into the rocket-local flight frame so f32 mesh vertices stay
        // small near the camera (avoids precision loss at ~6371 km magnitudes
        // that degrades the sphere into a flat plane with broken triangles).
        let mesh = patch_geometry_to_mesh(geometry, &config, &render_origin.origin);
        let mesh_handle = meshes.add(mesh);

        // Create material with biome-appropriate properties.
        let material = patch_material(&patch, planet_terrain.source.as_ref(), &config);
        let material_handle = materials.add(material);

        // Geometry is already in the rocket-local flight frame; the entity sits
        // at the origin (the rocket's render position).
        let transform = Transform::IDENTITY;

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
                Name::new(format!(
                    "TerrainPatch_{:?}_{}_{}_{}",
                    patch.face, patch.level, patch.tile_x, patch.tile_y
                )),
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

/// Convert domain PatchGeometry to Bevy Mesh, rebasing planet-centered positions
/// into the rocket-local flight frame (`positions - render_origin`). This keeps
/// f32 vertex magnitudes small near the camera, preserving the spherical surface
/// instead of collapsing it into a flat plane at ~6371 km magnitudes.
fn patch_geometry_to_mesh(
    geometry: &PatchGeometry,
    config: &TerrainRenderConfig,
    render_origin: &DVec3,
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
            let v = DVec3::from_array(*p) - *render_origin;
            [v.x as f32, v.y as f32, v.z as f32]
        })
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
            let x = p[0];
            let y = p[1];
            let z = p[2];
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
fn patch_material(
    patch: &TerrainPatch,
    source: &dyn TerrainSource,
    config: &TerrainRenderConfig,
) -> StandardMaterial {
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
        unlit: false,
        ..default()
    }
}

/// Biome properties from height. Pure function of altitude so the biome
/// classification is directly unit-testable (AGENTS.md section 45).
/// Bands, lower→higher: ocean, shoreline, plains, lowlands, hills/mountains,
/// rocky high mountains, and a snow line (spec "material variation by
/// altitude": above the snow line the albedo shifts toward white and roughness
/// decreases).
fn biome_properties(height_m: f64) -> ((f32, f32, f32), f32, f32) {
    if height_m > 4_500.0 {
        // Snow line: bright white with lower roughness (spec).
        ((0.92, 0.94, 0.97), 0.45, 0.0)
    } else if height_m > 3_000.0 {
        // High mountains: rocky grey.
        ((0.45, 0.42, 0.4), 0.9, 0.0)
    } else if height_m > 1_000.0 {
        // Mountains/hills: dark green slopes.
        ((0.28, 0.38, 0.22), 0.85, 0.0)
    } else if height_m > 100.0 {
        // Lowlands: grass.
        ((0.3, 0.45, 0.2), 0.8, 0.0)
    } else if height_m > 2.0 {
        // Plains / launch-site elevation: grassland green.
        ((0.32, 0.5, 0.22), 0.8, 0.0)
    } else if height_m > -2.0 {
        // Shoreline band: sand.
        ((0.76, 0.7, 0.5), 0.7, 0.0)
    } else {
        // Ocean: blue water.
        ((0.1, 0.25, 0.45), 0.15, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::cube_sphere::build_patch_geometry;

    #[test]
    fn ocean_plains_and_mountain_biomes_are_distinct() {
        // Spec: a patch in a mountain biome must use albedo distinct from
        // plains/ocean biomes (scenario "material variation by biome").
        let ocean = biome_properties(-1000.0);
        let plains = biome_properties(50.0);
        let mountains = biome_properties(2_000.0);
        assert_ne!(ocean.0, plains.0, "ocean and plains must differ");
        assert_ne!(plains.0, mountains.0, "plains and mountains must differ");
        // Plains are largely green; ocean largely blue.
        assert!(plains.0 .1 > plains.0 .2, "plains should be green-dominant");
        assert!(ocean.0 .2 > ocean.0 .0, "ocean should be blue-dominant");
    }

    #[test]
    fn snow_line_shifts_albedo_white_and_lowers_roughness() {
        // Spec scenario "material variation by altitude": above the snow line
        // albedo shifts toward white and roughness decreases.
        let rock = biome_properties(4_000.0);
        let snow = biome_properties(5_000.0);
        assert!(
            snow.0 .0 > 0.9 && snow.0 .1 > 0.9 && snow.0 .2 > 0.9,
            "snow must be near-white"
        );
        assert!(
            snow.1 < rock.1,
            "snow roughness {} must be below rocky {}",
            snow.1,
            rock.1
        );
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
                r < 6_371_000.0 + 2_500.0,
                "skirt vertex at radius {r} not extruded inward"
            );
        }
        // Skirt quads are present: more indices than a flat grid alone.
        assert!(geom.indices.len() > (res - 1) * (res - 1) * 6);
    }
}
