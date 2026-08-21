use crate::infrastructure::bevy_adapters::{components::*, entity_components::LaunchSiteType};
use bevy::prelude::*;

/// Sample terrain height at a world position
pub fn sample_terrain_height(
    world_pos: Vec3,
    terrain: &TerrainComponent,
    images: &Assets<Image>,
) -> f32 {
    // Convert world position to local terrain coordinates
    let local_pos = world_pos - terrain.position_offset;

    // Terrain size in meters
    let terrain_size_m = terrain.size_km * 1000.0;

    // Convert to UV coordinates (0-1 range)
    let uv_x = (local_pos.x / terrain_size_m + 0.5).clamp(0.0, 1.0);
    let uv_z = (local_pos.z / terrain_size_m + 0.5).clamp(0.0, 1.0);

    // Convert to pixel coordinates
    let pixel_x = (uv_x * (terrain.resolution - 1) as f32) as usize;
    let pixel_y = (uv_z * (terrain.resolution - 1) as f32) as usize;

    // Get height from heightmap (if available)
    if let Some(heightmap_image) = images.get(&terrain.heightmap) {
        if let Some(data) = &heightmap_image.data {
            // For R8Unorm format, each pixel is a single u8 value
            let pixel_index = pixel_y * terrain.resolution as usize + pixel_x;
            if pixel_index < data.len() {
                let height_normalized = data[pixel_index] as f32 / 255.0;

                // Convert back to actual height based on terrain type
                match terrain.launch_site_type {
                    LaunchSiteType::KennedySpaceCenter => height_normalized * 20.0 - 10.0, // -10m to +10m for Earth
                    LaunchSiteType::RtlsLandingPad => height_normalized * 15.0 - 7.5, // -7.5m to +7.5m
                    LaunchSiteType::DroneShip => height_normalized * 4.0 - 2.0, // -2m to +2m for ocean
                    LaunchSiteType::LunarLanding => height_normalized * 20.0 - 10.0, // -10m to +10m for Moon
                }
            } else {
                0.0 // Default height
            }
        } else {
            0.0 // No heightmap data
        }
    } else {
        0.0 // No heightmap available
    }
}

/// Get terrain surface normal at a world position
pub fn get_terrain_normal(
    _world_pos: Vec3,
    _terrain: &TerrainComponent,
    _images: &Assets<Image>,
) -> Vec3 {
    // For now, return up vector. In a full implementation, you'd sample
    // the normal map or calculate normals from heightmap
    Vec3::Y
}

/// Landing zone clearance information
#[derive(Debug, Clone)]
pub struct LandingClearance {
    pub is_clear: bool,
    pub max_terrain_height: f32,
    pub min_terrain_height: f32,
    pub slope_degrees: f32,
    pub recommended_touchdown_height: f32,
}

/// Check if landing zone is clear for rocket touchdown
pub fn check_landing_zone_clearance(
    rocket_position: Vec3,
    landing_gear_positions: &[Vec3],
    terrain: &TerrainComponent,
    images: &Assets<Image>,
) -> LandingClearance {
    let mut max_terrain_height = f32::NEG_INFINITY;
    let mut min_terrain_height = f32::INFINITY;
    let mut landing_gear_clear = true;

    // Check terrain height at each landing gear position
    for gear_pos in landing_gear_positions {
        let terrain_height = sample_terrain_height(rocket_position + *gear_pos, terrain, images);
        max_terrain_height = max_terrain_height.max(terrain_height);
        min_terrain_height = min_terrain_height.min(terrain_height);

        // Check if gear would be below terrain (collision)
        if rocket_position.y + gear_pos.y < terrain_height {
            landing_gear_clear = false;
        }
    }

    // Calculate landing slope (difference between highest and lowest points)
    let landing_slope = max_terrain_height - min_terrain_height;

    LandingClearance {
        is_clear: landing_gear_clear,
        max_terrain_height,
        min_terrain_height,
        slope_degrees: landing_slope.to_degrees(),
        recommended_touchdown_height: max_terrain_height + 2.0, // 2m safety margin
    }
}

/// Calculate ground effect on rocket thrust near terrain
pub fn calculate_ground_effect(
    thrust_vector: Vec3,
    distance_to_ground: f32,
    terrain_type: LaunchSiteType,
) -> f32 {
    let _ = thrust_vector; // Unused parameter

    if distance_to_ground > 50.0 {
        return 1.0; // No ground effect beyond 50m
    }

    let ground_effect_strength = match terrain_type {
        LaunchSiteType::KennedySpaceCenter => 0.15, // Concrete pad has moderate ground effect
        LaunchSiteType::RtlsLandingPad => 0.12,     // Similar to launch pad
        LaunchSiteType::DroneShip => 0.08,          // Water surface has less ground effect
        LaunchSiteType::LunarLanding => 0.05,       // Lunar regolith has minimal ground effect
    };

    // Ground effect increases thrust as you get closer to ground
    // Simplified model: thrust multiplier = 1 + (strength / distance)
    let distance_factor = (50.0 - distance_to_ground).max(0.1) / 50.0;
    1.0 + ground_effect_strength * distance_factor
}

/// Terrain properties for physics calculations
#[derive(Debug, Clone)]
pub struct TerrainProperties {
    pub friction_coefficient: f32,
    pub restitution: f32,      // Bounciness
    pub surface_hardness: f32, // For impact calculations
}

pub fn get_terrain_properties(terrain_type: LaunchSiteType) -> TerrainProperties {
    match terrain_type {
        LaunchSiteType::KennedySpaceCenter => TerrainProperties {
            friction_coefficient: 0.7, // Concrete has good friction
            restitution: 0.1,          // Low bounce
            surface_hardness: 0.9,     // Hard surface
        },
        LaunchSiteType::RtlsLandingPad => TerrainProperties {
            friction_coefficient: 0.8, // Clean concrete
            restitution: 0.05,         // Very low bounce
            surface_hardness: 0.95,    // Very hard
        },
        LaunchSiteType::DroneShip => TerrainProperties {
            friction_coefficient: 0.3, // Wet steel is slippery
            restitution: 0.2,          // Some bounce on water
            surface_hardness: 0.8,     // Steel surface
        },
        LaunchSiteType::LunarLanding => TerrainProperties {
            friction_coefficient: 0.6, // Regolith has moderate friction
            restitution: 0.02,         // Almost no bounce
            surface_hardness: 0.3,     // Soft regolith
        },
    }
}
