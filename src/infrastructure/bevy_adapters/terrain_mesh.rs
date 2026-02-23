use crate::infrastructure::bevy_adapters::{components::*, entity_components::LaunchSiteType};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy_mesh::{Indices, PrimitiveTopology};

// System to generate terrain mesh from heightmap
pub fn generate_terrain_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    terrain_query: Query<(Entity, &TerrainComponent), Added<TerrainComponent>>,
    images: Res<Assets<Image>>,
) {
    for (entity, terrain) in terrain_query.iter() {
        // Create terrain mesh from heightmap
        let mesh = create_terrain_mesh(&terrain, &images);
        let mesh_handle = meshes.add(mesh);

        // Create terrain material with normal mapping
        let material = StandardMaterial {
            base_color_texture: Some(terrain.surface_texture.clone()),
            normal_map_texture: Some(terrain.normal_texture.clone()),
            perceptual_roughness: 0.8,
            metallic: 0.0,
            ..default()
        };
        let material_handle = materials.add(material);

        // Update the entity with mesh and material
        commands
            .entity(entity)
            .insert((Mesh3d(mesh_handle), MeshMaterial3d(material_handle)));
    }
}

fn create_terrain_mesh(terrain: &TerrainComponent, images: &Assets<Image>) -> Mesh {
    let size = terrain.size_km * 1000.0; // Convert km to meters
    let resolution = terrain.resolution as usize;

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    let half_size = size / 2.0;
    let step = size / (resolution - 1) as f32;

    // Get height range based on terrain type
    let (height_min, height_max) = match terrain.launch_site_type {
        LaunchSiteType::KennedySpaceCenter => (-10.0, 10.0),
        LaunchSiteType::RtlsLandingPad => (-8.0, 8.0),
        LaunchSiteType::DroneShip => (-2.0, 2.0),
        LaunchSiteType::LunarLanding => (-10.0, 10.0),
    };

    // Generate vertices
    for z in 0..resolution {
        for x in 0..resolution {
            let x_pos = x as f32 * step - half_size;
            let z_pos = z as f32 * step - half_size;

            // Sample height from heightmap
            let mut y_pos = 0.0;
            if let Some(heightmap_image) = images.get(&terrain.heightmap) {
                if let Some(data) = &heightmap_image.data {
                    let pixel_index = z * resolution + x;
                    if pixel_index < data.len() {
                        let height_normalized = data[pixel_index] as f32 / 255.0;
                        y_pos = height_min + height_normalized * (height_max - height_min);
                    }
                }
            }

            positions.push([x_pos, y_pos, z_pos]);
            // Normals will be calculated later
            normals.push([0.0, 1.0, 0.0]); // Temporary up normal
            uvs.push([
                x as f32 / (resolution - 1) as f32,
                z as f32 / (resolution - 1) as f32,
            ]);
        }
    }

    // Calculate normals based on height differences
    for z in 0..resolution {
        for x in 0..resolution {
            let idx = z * resolution + x;

            // Calculate normal using central differences
            let height_center = positions[idx][1];

            let height_left = if x > 0 {
                positions[z * resolution + (x - 1)][1]
            } else {
                height_center
            };

            let height_right = if x < resolution - 1 {
                positions[z * resolution + (x + 1)][1]
            } else {
                height_center
            };

            let height_up = if z > 0 {
                positions[(z - 1) * resolution + x][1]
            } else {
                height_center
            };

            let height_down = if z < resolution - 1 {
                positions[(z + 1) * resolution + x][1]
            } else {
                height_center
            };

            // Calculate gradients
            let dx = (height_right - height_left) / (2.0 * step);
            let dz = (height_down - height_up) / (2.0 * step);

            // Normal vector (negate dz for correct orientation)
            let normal = Vec3::new(-dx, 1.0, -dz).normalize();
            normals[idx] = [normal.x, normal.y, normal.z];
        }
    }

    // Generate indices
    for z in 0..resolution - 1 {
        for x in 0..resolution - 1 {
            let top_left = (z * resolution + x) as u32;
            let top_right = (z * resolution + x + 1) as u32;
            let bottom_left = ((z + 1) * resolution + x) as u32;
            let bottom_right = ((z + 1) * resolution + x + 1) as u32;

            // First triangle
            indices.push(top_left);
            indices.push(bottom_left);
            indices.push(top_right);

            // Second triangle
            indices.push(top_right);
            indices.push(bottom_left);
            indices.push(bottom_right);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));

    mesh
}
