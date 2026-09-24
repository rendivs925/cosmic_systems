# Solar System Simulation Plan

## Overview
Implementation plan for adding a solar system simulation to the Cosmic Frontier Simulator. Planets will be represented as colored spheres with realistic relative sizes and orbital mechanics.

## Current Codebase Analysis

The project uses a clean DDD architecture with:
- **Domain layer**: Pure business logic (entities, services, value objects)
- **Application layer**: Use cases and orchestration  
- **Infrastructure layer**: Bevy integration (components, systems)
- **Presentation layer**: UI and rendering

The current "space simulation" mode just renders a single blue sphere. This provides a perfect foundation for expansion.

## Solar System Data Requirements

From NASA planetary fact sheets and astronomical data:

**Diameters (km):**
- Sun: 1,392,684
- Jupiter: 142,984  
- Saturn: 120,536
- Uranus: 51,118
- Neptune: 49,528
- Earth: 12,756
- Venus: 12,104
- Mars: 6,792
- Mercury: 4,879

**Realistic Colors (Bevy Color::srgb):**
- Mercury: `Color::srgb(0.5, 0.5, 0.5)` (gray rocky)
- Venus: `Color::srgb(0.9, 0.8, 0.6)` (yellowish cloudy)
- Earth: `Color::srgb(0.2, 0.4, 0.8)` (blue with green continents)
- Mars: `Color::srgb(0.8, 0.3, 0.1)` (red/orange)
- Jupiter: `Color::srgb(0.8, 0.6, 0.4)` (orange/brown bands)
- Saturn: `Color::srgb(0.9, 0.8, 0.5)` (golden rings)
- Uranus: `Color::srgb(0.6, 0.8, 0.9)` (pale cyan)
- Neptune: `Color::srgb(0.3, 0.5, 0.9)` (deep azure)
- Sun: `Color::srgb(1.0, 1.0, 0.9)` (bright yellowish-white)

## Implementation Plan

### 1. Domain Layer Updates

**New Entity: Planet**
```rust
#[derive(Clone, Debug)]
pub struct Planet {
    pub name: String,
    pub radius_km: f32,
    pub mass_kg: f64,
    pub color: Color,
    pub orbital_distance_au: f32,  // Average distance from Sun in AU
    pub orbital_period_days: f32,
    pub rotation_period_hours: f32,
}
```

**New Value Object: SolarSystemParameters**
```rust
#[derive(Resource, Clone, Debug)]
pub struct SolarSystemParameters {
    pub sun_radius_km: f32,
    pub scale_factor: f32,  // For visualization (e.g., 1 AU = 100 units)
    pub time_scale: f32,    // Simulation speed multiplier
    pub show_orbits: bool,
}
```

**New Service: OrbitalMechanics**
```rust
pub fn calculate_position(planet: &Planet, time_days: f32) -> Vec3 {
    // Kepler's laws implementation
    let angle = 2.0 * PI * time_days / planet.orbital_period_days;
    let distance = planet.orbital_distance_au * AU_TO_UNITS;
    Vec3::new(distance * angle.cos(), 0.0, distance * angle.sin())
}
```

### 2. Application Layer Updates

**Update SimulationService**
- Add planet creation methods
- Add orbital calculation orchestration
- Integrate with existing physics services

**New PlanetFactory**
```rust
impl PlanetFactory {
    pub fn create_solar_system() -> Vec<Planet> {
        vec![
            Planet { name: "Mercury".to_string(), radius_km: 4879.0, /* ... */ },
            Planet { name: "Venus".to_string(), radius_km: 12104.0, /* ... */ },
            // ... all planets
        ]
    }
}
```

### 3. Infrastructure Layer Updates

**New Bevy Component: PlanetComponent**
```rust
#[derive(Component)]
pub struct PlanetComponent {
    pub domain_planet: Planet,
}
```

**New Bevy Component: OrbitComponent** 
```rust
#[derive(Component)]
pub struct OrbitComponent {
    pub radius: f32,
    pub planet_entity: Entity,
}
```

**Update Systems:**
- `planet_update_system`: Handle orbital motion and rotation
- `planet_render_system`: Scale and color planets appropriately
- `orbit_render_system`: Draw orbital paths (optional)

### 4. Presentation Layer Updates

**Update setup_space function** to:
- Create Sun (scaled appropriately)
- Create all 8 planets with correct relative sizes and colors
- Set up orbital mechanics
- Position camera for good solar system view

### 5. Scaling Considerations

Given the huge size differences, implement logarithmic scaling:
- Sun: Actual relative size (but still needs scaling for visibility)
- Planets: Scaled so Mercury is visible but Jupiter dominates
- Orbits: Scaled to fit in view (e.g., 1 AU = 50-100 units)

### 6. Camera & Controls

- Orbit camera around solar system center
- Zoom controls to see planets up close
- Time controls (pause/play/speed up orbital motion)
- Planet labels (optional UI)

### 7. Implementation Steps

1. **Create domain entities** for Planet and orbital parameters
2. **Add orbital mechanics calculations** to physics service  
3. **Create Bevy components** for planets and orbits
4. **Update setup_space** to spawn all celestial bodies
5. **Implement planet motion systems** for realistic orbits
6. **Add visual scaling** to make simulation viewable
7. **Test and balance** sizes/colors for visual appeal

### 8. Technical Considerations

- **Performance**: Use instancing for orbital path rendering if needed
- **Accuracy**: Implement simplified Keplerian orbits (circular for starters)
- **Visuals**: Add basic lighting, textures could be added later
- **Extensibility**: Design so moons, asteroids, etc. can be added easily

This approach maintains your clean architecture while building on the existing space simulation foundation.