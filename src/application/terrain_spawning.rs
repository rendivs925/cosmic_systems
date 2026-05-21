use crate::infrastructure::bevy_adapters::{components::*, entity_components::LaunchSiteType};
use bevy::prelude::*;
use std::collections::HashMap;

pub fn spawn_terrain_patches(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    entity_map: &HashMap<String, Entity>,
) {
    use crate::infrastructure::bevy_adapters::terrain_heightmaps::generate_launch_site_heightmap;
    use crate::infrastructure::bevy_adapters::terrain_textures::generate_terrain_textures;

    // Create Kennedy Space Center terrain for Earth
    if let Some(earth_entity) = entity_map.get("Earth") {
        println!("Creating Kennedy Space Center terrain for Earth");
        let site_type = LaunchSiteType::KennedySpaceCenter;

        // Generate heightmap and textures
        println!("Generating KSC heightmap and textures...");
        let heightmap = generate_launch_site_heightmap(site_type, 10.0, 256);
        let (diffuse_texture, normal_texture) = generate_terrain_textures(site_type, 256);

        let heightmap_handle = images.add(heightmap);
        let diffuse_handle = images.add(diffuse_texture);
        let normal_handle = images.add(normal_texture);

        println!("Created KSC terrain assets: heightmap={:?}, diffuse={:?}, normal={:?}",
                heightmap_handle, diffuse_handle, normal_handle);

        println!("Spawning KSC terrain entity...");
        commands.spawn((
            TerrainComponent {
                planet_entity: *earth_entity,
                planet_name: "Earth".to_string(),
                position_offset: Vec3::new(0.0, -6371.0, 0.0), // On Earth's surface
                scale: 1.0,
                heightmap: heightmap_handle,
                surface_texture: diffuse_handle,
                normal_texture: normal_handle,
                size_km: 10.0,
                resolution: 256,
                launch_site_type: site_type,
            },
            Transform::from_translation(Vec3::new(0.0, -6371.0, 0.0)),
            Visibility::Hidden, // Hidden by default
            Selectable {
                name: "Kennedy Space Center".to_string(),
                selected: false,
            },
        ));
        println!("KSC terrain entity spawned successfully");
    }

    // Create RTLS landing pad (adjacent to KSC)
    if let Some(earth_entity) = entity_map.get("Earth") {
        let site_type = LaunchSiteType::RtlsLandingPad;

        // Generate RTLS terrain (smaller area, higher detail)
        let heightmap = generate_launch_site_heightmap(site_type, 2.0, 128);
        let (diffuse_texture, normal_texture) = generate_terrain_textures(site_type, 128);

        let heightmap_handle = images.add(heightmap);
        let diffuse_handle = images.add(diffuse_texture);
        let normal_handle = images.add(normal_texture);

        // Position RTLS pad ~10km from KSC
        let rtls_position = Vec3::new(10000.0, -6371.0, 0.0);

        commands.spawn((
            TerrainComponent {
                planet_entity: *earth_entity,
                planet_name: "Earth".to_string(),
                position_offset: rtls_position,
                scale: 1.0,
                heightmap: heightmap_handle,
                surface_texture: diffuse_handle,
                normal_texture: normal_handle,
                size_km: 2.0,
                resolution: 128,
                launch_site_type: site_type,
            },
            Transform::from_translation(rtls_position),
            Visibility::Hidden, // Hidden by default
            Selectable {
                name: "RTLS Landing Pad".to_string(),
                selected: false,
            },
        ));

        // Create drone ship landing zone (ocean)
        let site_type = LaunchSiteType::DroneShip;

        // Generate ocean terrain
        let heightmap = generate_launch_site_heightmap(site_type, 3.0, 128);
        let (diffuse_texture, normal_texture) = generate_terrain_textures(site_type, 128);

        let heightmap_handle = images.add(heightmap);
        let diffuse_handle = images.add(diffuse_texture);
        let normal_handle = images.add(normal_texture);

        // Position drone ship ~50km offshore
        let drone_ship_position = Vec3::new(50000.0, -6371.0, 10000.0);

        commands.spawn((
            TerrainComponent {
                planet_entity: *earth_entity,
                planet_name: "Earth".to_string(),
                position_offset: drone_ship_position,
                scale: 1.0,
                heightmap: heightmap_handle,
                surface_texture: diffuse_handle,
                normal_texture: normal_handle,
                size_km: 3.0,
                resolution: 128,
                launch_site_type: site_type,
            },
            Transform::from_translation(drone_ship_position),
            Visibility::Hidden, // Hidden by default
            Selectable {
                name: "Drone Ship Landing Zone".to_string(),
                selected: false,
            },
        ));
    }

    // Create lunar landing site for Moon
    if let Some(moon_entity) = entity_map.get("Moon") {
        let site_type = LaunchSiteType::LunarLanding;

        // Generate lunar terrain
        let heightmap = generate_launch_site_heightmap(site_type, 5.0, 128);
        let (diffuse_texture, normal_texture) = generate_terrain_textures(site_type, 128);

        let heightmap_handle = images.add(heightmap);
        let diffuse_handle = images.add(diffuse_texture);
        let normal_handle = images.add(normal_texture);

        commands.spawn((
            TerrainComponent {
                planet_entity: *moon_entity,
                planet_name: "Moon".to_string(),
                position_offset: Vec3::new(0.0, -1737.4, 0.0), // On Moon's surface
                scale: 1.0,
                heightmap: heightmap_handle,
                surface_texture: diffuse_handle,
                normal_texture: normal_handle,
                size_km: 5.0,
                resolution: 128,
                launch_site_type: site_type,
            },
            Transform::from_translation(Vec3::new(0.0, -1737.4, 0.0)),
            Visibility::Hidden, // Hidden by default
            Selectable {
                name: "Lunar Landing Site".to_string(),
                selected: false,
            },
        ));
    }
}