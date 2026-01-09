use crate::infrastructure::bevy_adapters::components::*;
use bevy::prelude::*;
use bevy_mesh::{Indices, PrimitiveTopology};

// System to update terrain patches based on camera proximity and planet selection
pub fn update_terrain_visibility(
    mut terrain_query: Query<(&mut Visibility, &TerrainComponent)>,
    camera_query: Query<(&CameraController, &Transform), With<Camera>>,
    selected_planet: Res<SelectedPlanet>,
) {
    let (camera_controller, camera_transform) = match camera_query.single().ok() {
        Some(data) => data,
        None => return,
    };

    let camera_pos = camera_transform.translation;

    for (mut visibility, terrain) in terrain_query.iter_mut() {
        // Show terrain only when in TerrainView mode and Earth is selected
        let should_show = camera_controller.mode == CameraMode::TerrainView
            && selected_planet.name.as_ref() == Some(&terrain.planet_name);

        *visibility = if should_show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

// System to generate terrain mesh from heightmap
pub fn generate_terrain_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    terrain_query: Query<(Entity, &TerrainComponent), Added<TerrainComponent>>,
    asset_server: Res<AssetServer>,
) {
    for (entity, terrain) in terrain_query.iter() {
        // Create terrain mesh from heightmap
        let mesh = create_terrain_mesh(terrain);
        let mesh_handle = meshes.add(mesh);

        // Create terrain material
        let material = StandardMaterial {
            base_color_texture: Some(terrain.surface_texture.clone()),
            perceptual_roughness: 0.8,
            metallic: 0.0,
            ..default()
        };
        let material_handle = materials.add(material);

        // Update the entity with mesh and material
        commands.entity(entity).insert((
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
        ));
    }
}

fn create_terrain_mesh(terrain: &TerrainComponent) -> Mesh {
    // Create a simple plane mesh for now - will be replaced with heightmap-based terrain
    let size = terrain.size_km * 1000.0; // Convert km to meters
    let resolution = terrain.resolution as usize;

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    let half_size = size / 2.0;
    let step = size / (resolution - 1) as f32;

    // Generate vertices
    for z in 0..resolution {
        for x in 0..resolution {
            let x_pos = x as f32 * step - half_size;
            let z_pos = z as f32 * step - half_size;
            let y_pos = 0.0; // Flat terrain for now - will use heightmap later

            positions.push([x_pos, y_pos, z_pos]);
            normals.push([0.0, 1.0, 0.0]); // Up normal
            uvs.push([x as f32 / (resolution - 1) as f32, z as f32 / (resolution - 1) as f32]);
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