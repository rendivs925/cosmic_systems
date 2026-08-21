use crate::domain::services::physics_orbital::calculate_planet_position;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::*;
use bevy::prelude::*;

// Re-export terrain functionality from split modules
pub use super::terrain_heightmaps::*;
pub use super::terrain_mesh::*;
pub use super::terrain_textures::*;
pub use super::terrain_utils::*;
pub use super::terrain_visibility::*;

/// System to synchronize terrain positions with their parent planet's orbital motion
/// This ensures terrain moves with Earth around the Sun and rotates with Earth's axial tilt
pub fn update_terrain_orbital_positions(
    mut terrain_query: Query<(&mut Transform, &TerrainComponent)>,
    planet_query: Query<(&Transform, &PlanetComponent)>,
    solar_params: Res<SolarSystemParameters>,
    time: Res<Time>,
) {
    let elapsed_seconds = time.elapsed_secs();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    for (mut terrain_transform, terrain_comp) in terrain_query.iter_mut() {
        // Find the parent planet
        if let Ok((planet_transform, planet_comp)) = planet_query.get(terrain_comp.planet_entity) {
            // Calculate planet's current orbital position
            let planet_position = calculate_planet_position(
                &planet_comp.domain_planet,
                time_days,
                &solar_params,
                Vec3::ZERO, // Sun at origin
                None,       // No parent for Earth
            );

            // Calculate terrain position relative to planet's current orbital position
            // Apply planet's rotation to terrain orientation
            let planet_rotation = planet_transform.rotation;
            let rotated_offset = planet_rotation * terrain_comp.position_offset;

            terrain_transform.translation = planet_position + rotated_offset;
            terrain_transform.rotation = planet_rotation;
        }
    }
}

/// Combined terrain synchronization system that handles both orbital positioning and time-based rotation
/// This prevents query conflicts by querying specific planet entities
pub fn update_terrain_synchronization(
    time: Res<Time>,
    solar_params: Res<SolarSystemParameters>,
    mut terrain_query: Query<(&mut Transform, &TerrainComponent)>,
    planet_query: Query<&PlanetComponent>,
    transform_query: Query<&Transform, Without<TerrainComponent>>,
) {
    let elapsed_seconds = time.elapsed_secs();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    // Update all terrain positions and rotations
    for (mut terrain_transform, terrain_comp) in terrain_query.iter_mut() {
        // Get the specific planet for this terrain
        if let Ok(planet_comp) = planet_query.get(terrain_comp.planet_entity) {
            // Calculate planet's current orbital position
            let planet_position = calculate_planet_position(
                &planet_comp.domain_planet,
                time_days,
                &solar_params,
                Vec3::ZERO, // Sun at origin
                None,       // No parent for Earth
            );

            // Get planet's current transform
            if let Ok(planet_transform) = transform_query.get(terrain_comp.planet_entity) {
                // Calculate terrain position relative to planet's current orbital position
                let planet_rotation = planet_transform.rotation;
                let rotated_offset = planet_rotation * terrain_comp.position_offset;

                terrain_transform.translation = planet_position + rotated_offset;

                // Apply additional rotation for Earth (axial tilt + daily rotation)
                if terrain_comp.planet_name == "Earth" {
                    use crate::domain::services::physics_utils::calculate_planet_rotation;
                    let earth_rotation_angle =
                        calculate_planet_rotation(&planet_comp.domain_planet, time_days);
                    let axial_rotation = Quat::from_rotation_y(earth_rotation_angle);
                    terrain_transform.rotation = planet_transform.rotation * axial_rotation;
                } else {
                    terrain_transform.rotation = planet_rotation;
                }
            }
        }
    }
}
