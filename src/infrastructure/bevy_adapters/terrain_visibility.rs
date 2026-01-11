use crate::infrastructure::bevy_adapters::components::*;
use bevy::prelude::*;

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

    println!("👁️ Terrain visibility check - Camera mode: {:?}, Selected planet: {:?}, Camera pos: {:?}",
             camera_controller.mode, selected_planet.name, camera_pos);

    let mut terrain_count = 0;
    for (mut visibility, terrain) in terrain_query.iter_mut() {
        // Show terrain when in TerrainView mode AND Earth is selected
        // OR when Earth is selected (temporary for testing)
        let should_show = (camera_controller.mode == CameraMode::TerrainView
            && selected_planet.name.as_ref() == Some(&terrain.planet_name))
            || (selected_planet.name.as_ref() == Some(&terrain.planet_name) && camera_controller.mode == CameraMode::FreeFlight);

        let new_visibility = if should_show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };

        if *visibility != new_visibility {
            println!("🌍 Terrain visibility change for {}: {:?} -> {:?} (mode: {:?}, selected: {:?})",
                     terrain.planet_name, *visibility, new_visibility,
                     camera_controller.mode, selected_planet.name);
        }

        *visibility = new_visibility;
        terrain_count += 1;
    }

    if terrain_count == 0 {
        println!("⚠️ No terrain entities found in the world!");
    } else {
        println!("📊 Found {} terrain entities", terrain_count);
    }
}

// System to update terrain level of detail based on distance
pub fn update_terrain_lod(
    camera_query: Query<&Transform, With<Camera>>,
    mut terrain_query: Query<(&mut TerrainComponent, &GlobalTransform)>,
) {
    let camera_transform = match camera_query.single().ok() {
        Some(transform) => transform,
        None => return,
    };

    let camera_pos = camera_transform.translation;

    for (mut terrain, terrain_transform) in terrain_query.iter_mut() {
        let terrain_pos = terrain_transform.translation();
        let distance = camera_pos.distance(terrain_pos);

        // Adjust LOD based on distance
        let lod_factor = if distance < 1000.0 {
            1.0 // High detail close up
        } else if distance < 5000.0 {
            0.75 // Medium detail
        } else if distance < 15000.0 {
            0.5 // Low detail
        } else {
            0.25 // Very low detail for distant terrain
        };

        // Update terrain scale based on LOD
        terrain.scale = lod_factor;
    }
}

// Initialize terrain LOD settings
pub fn initialize_terrain_lod(mut terrain_query: Query<&mut TerrainComponent>) {
    for mut terrain in terrain_query.iter_mut() {
        // Set initial LOD scale
        terrain.scale = 1.0;
    }
}