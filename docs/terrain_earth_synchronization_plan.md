# Terrain-Earth Synchronization Plan for Orbital Dynamics

## Overview
This plan outlines the implementation to synchronize terrain positioning with Earth's orbital dynamics, rotation, and motion for accurate orbital dynamics and rocket launch simulations.

## Current Problems

1. **Static Terrain Positioning**: Terrain patches are positioned at fixed offsets from planet centers, but planets move in their orbits around the Sun
2. **No Earth Rotation Sync**: Terrain doesn't account for Earth's axial rotation (23.439° tilt) or daily rotation
3. **Disconnected Rocket Simulation**: Rockets don't interact with terrain surfaces or account for Earth's movement during flight
4. **Camera-Based Visibility**: Terrain only shows in specific camera modes, not based on actual proximity

## Proposed Solution Architecture

### 1. Terrain Orbital Synchronization System
**File**: `src/infrastructure/bevy_adapters/terrain_systems.rs`
**Function**: `update_terrain_orbital_positions()`

```rust
pub fn update_terrain_orbital_positions(
    mut terrain_query: Query<(&mut Transform, &TerrainComponent)>,
    planet_query: Query<(&Transform, &PlanetComponent)>,
) {
    for (mut terrain_transform, terrain_comp) in terrain_query.iter_mut() {
        // Find the parent planet
        if let Ok((planet_transform, planet_comp)) = planet_query.get(terrain_comp.planet_entity) {
            // Calculate terrain position relative to planet's current orbital position
            // Apply planet's rotation to terrain orientation
            let planet_rotation = planet_transform.rotation;
            let rotated_offset = planet_rotation * terrain_comp.position_offset;

            terrain_transform.translation = planet_transform.translation + rotated_offset;
            terrain_transform.rotation = planet_rotation;
        }
    }
}
```

### 2. Rocket-Terrain Integration System
**File**: `src/infrastructure/bevy_adapters/rocket_systems.rs`
**Function**: `update_rocket_terrain_interaction()`

```rust
pub fn update_rocket_terrain_interaction(
    mut rocket_query: Query<(&mut RocketComponent, &Transform)>,
    terrain_query: Query<(&TerrainComponent, &Transform)>,
    images: Res<Assets<Image>>,
) {
    for (mut rocket, rocket_transform) in rocket_query.iter_mut() {
        // Find nearest terrain
        for (terrain, terrain_transform) in terrain_query.iter() {
            let distance_to_terrain = rocket_transform.translation.distance(terrain_transform.translation);

            if distance_to_terrain < terrain.size_km * 500.0 { // Within terrain influence
                // Sample terrain height at rocket position
                let terrain_height = sample_terrain_height(
                    rocket_transform.translation - terrain_transform.translation,
                    terrain,
                    &images
                );

                // Apply terrain effects (ground effect, collision detection, etc.)
                // Update rocket position relative to terrain
            }
        }
    }
}
```

### 3. Time-Based Terrain Synchronization
**File**: `src/infrastructure/bevy_adapters/terrain_systems.rs`
**Function**: `update_terrain_time_synchronization()`

```rust
pub fn update_terrain_time_synchronization(
    time: Res<Time>,
    solar_params: Res<SolarSystemParameters>,
    mut terrain_query: Query<(&mut Transform, &TerrainComponent)>,
    planet_query: Query<(&Transform, &PlanetComponent), With<EarthMarker>>,
) {
    let elapsed_seconds = time.elapsed_secs();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    // Calculate Earth's rotation angle
    let earth_rotation = calculate_planet_rotation(&earth_planet, time_days);

    for (mut terrain_transform, terrain_comp) in terrain_query.iter_mut() {
        if terrain_comp.planet_name == "Earth" {
            // Apply Earth's axial rotation to terrain
            terrain_transform.rotation = Quat::from_rotation_y(earth_rotation);
        }
    }
}
```

### 4. Proximity-Based Terrain Visibility
**File**: `src/infrastructure/bevy_adapters/terrain_systems.rs`
**Function**: `update_terrain_proximity_visibility()`

```rust
pub fn update_terrain_proximity_visibility(
    camera_query: Query<&Transform, With<CameraController>>,
    mut terrain_query: Query<(&mut Visibility, &Transform, &TerrainComponent)>,
) {
    let camera_pos = camera_query.single().unwrap().translation;

    for (mut visibility, terrain_transform, terrain_comp) in terrain_query.iter_mut() {
        let distance = camera_pos.distance(terrain_transform.translation);
        let max_visible_distance = terrain_comp.size_km * 1000.0 * 2.0; // 2x terrain size

        *visibility = if distance < max_visible_distance {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}
```

### 5. Launch Site Coordinate System
**File**: `src/domain/value_objects/launch_site_coordinates.rs`
**Struct**: `LaunchSiteCoordinates`

```rust
#[derive(Component, Debug, Clone)]
pub struct LaunchSiteCoordinates {
    pub planet_name: String,
    pub latitude_deg: f32,    // -90 to 90
    pub longitude_deg: f32,   // -180 to 180
    pub altitude_m: f32,      // Height above reference ellipsoid
}

impl LaunchSiteCoordinates {
    pub fn to_planet_relative_position(&self, planet: &Planet) -> Vec3 {
        // Convert lat/lon/alt to planet-centered Cartesian coordinates
        // Account for planet's ellipsoidal shape, not just spherical
        let planet_radius = planet.radius_km * 1000.0;

        let lat_rad = self.latitude_deg.to_radians();
        let lon_rad = self.longitude_deg.to_radians();

        let x = (planet_radius + self.altitude_m) * lat_rad.cos() * lon_rad.cos();
        let y = (planet_radius + self.altitude_m) * lat_rad.sin();
        let z = (planet_radius + self.altitude_m) * lat_rad.cos() * lon_rad.sin();

        Vec3::new(x, y, z)
    }
}
```

### 6. Orbital Mechanics Integration
**File**: `src/domain/services/physics.rs`
**Function**: `calculate_terrain_orbital_position()`

```rust
pub fn calculate_terrain_orbital_position(
    terrain_coords: &LaunchSiteCoordinates,
    planet: &Planet,
    time_days: f32,
    solar_params: &SolarSystemParameters,
) -> Vec3 {
    // First, get planet's orbital position
    let planet_position = calculate_planet_position(planet, time_days, solar_params, Vec3::ZERO, None);

    // Calculate Earth's rotation at this time
    let earth_rotation = calculate_planet_rotation(planet, time_days);

    // Convert launch site coordinates to position relative to planet center
    let relative_position = terrain_coords.to_planet_relative_position(planet);

    // Apply Earth's rotation
    let rotated_position = Quat::from_rotation_y(earth_rotation) * relative_position;

    // Add to planet's orbital position
    planet_position + rotated_position
}
```

## Implementation Priority

### High Priority (Core Functionality)
1. Terrain orbital synchronization system
2. Proximity-based terrain visibility

### Medium Priority (Enhanced Realism)
3. Rocket-terrain interaction system
4. Time-based synchronization (Earth rotation)

### Low Priority (Advanced Features)
5. Full launch site coordinate system with lat/lon/alt
6. Complete orbital-terrain coupling

## Files to Modify

### Infrastructure Layer
- `src/infrastructure/bevy_adapters/terrain_systems.rs` - Add orbital sync and proximity visibility
- `src/infrastructure/bevy_adapters/rocket_systems.rs` - Add terrain interaction
- `src/infrastructure/bevy_adapters/systems.rs` - Register new systems

### Domain Layer
- `src/domain/value_objects/launch_site_coordinates.rs` - New coordinate system (create)
- `src/domain/services/physics.rs` - Extend orbital calculations
- `src/domain/entities/terrain.rs` - Update terrain component (if needed)

### Application Layer
- `src/application/solar_system_startup.rs` - Update system registration
- `src/application/terrain_startup.rs` - Update terrain spawning with new components

## Testing and Validation

1. **Orbital Synchronization Test**: Verify terrain moves with Earth around the Sun
2. **Rotation Test**: Confirm terrain rotates with Earth's axial rotation
3. **Rocket Interaction Test**: Test rocket launch/landing on moving terrain
4. **Performance Test**: Ensure synchronization doesn't impact frame rate
5. **Edge Cases**: Test with multiple terrain patches, different planets

## Dependencies

- Requires existing `PlanetComponent`, `TerrainComponent`, `RocketComponent`
- Uses `SolarSystemParameters` for time calculations
- Integrates with existing orbital mechanics in `physics.rs`
- Depends on Bevy's `Transform`, `Visibility`, and `Query` systems

## Success Criteria

- Terrain patches follow Earth's orbital motion around the Sun
- Terrain rotates with Earth's daily rotation and axial tilt
- Rockets can launch from and land on terrain surfaces
- Terrain visibility based on camera proximity, not mode
- Launch sites use accurate lat/lon coordinates
- No performance degradation in simulation

## Future Enhancements

- Terrain deformation during rocket launches/landings
- Atmospheric effects on terrain visibility
- Multi-planet terrain synchronization (Mars, Moon, etc.)
- Real-time weather effects on terrain conditions
- Dynamic terrain generation based on orbital position