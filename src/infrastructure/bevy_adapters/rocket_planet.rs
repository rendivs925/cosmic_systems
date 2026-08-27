//! Rocket-mode planet system.
//!
//! In rocket mode, the camera is at the rocket's position (flight units = meters).
//! This system spawns and positions the bound planet (Earth), its moons, and the
//! Sun in flight units using real textures, driven by the solar system's
//! authoritative orbital state (which runs at AU scale in SharedSimulationPlugin).
//!
//! Conversion: solar system display units -> meters using PhysicalScale.

use crate::application::material_factory::{create_planet_material, PlanetMaterialConfig};
use crate::application::mesh_factory::create_flight_globe_mesh;
use crate::application::texture_config::{get_planet_textures, load_texture};
use crate::components::rocket::{RocketPhysicsState, RocketPlanetBinding};
use crate::domain::services::physics_orbital::MOON_ORBIT_SCALE;
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::services::reference_frames::body_fixed_to_inertial_rotation;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::*;
use crate::infrastructure::bevy_adapters::terrain_render::RenderOrigin;
use crate::infrastructure::bevy_adapters::terrain_streaming::local_terrain_is_required;
use bevy::ecs::system::ParamSet;
use bevy::prelude::*;

/// Component marking a planet entity managed by the rocket planet system.
#[derive(Component, Debug, Clone)]
pub struct RocketPlanet {
    pub name: String,
    pub is_bound_planet: bool,
    pub is_sun: bool,
}

/// Component marking a moon entity managed by the rocket planet system.
#[derive(Component, Debug, Clone)]
pub struct RocketMoon {
    pub name: String,
    pub parent_planet: String,
}

/// Resource storing the bound planet name for quick lookup.
#[derive(Resource, Debug, Default)]
pub struct RocketBoundPlanet(pub Option<String>);

/// Rocket Mode keeps shared celestial entities as the simulation authority, but
/// hides their solar-scale presentation. Flight-frame proxy meshes are the only
/// celestial visuals seen by the rocket camera.
pub fn isolate_rocket_presentation(
    mut planets: Query<&mut Visibility, With<PlanetComponent>>,
    mut solar_lights: Query<&mut PointLight>,
) {
    for mut visibility in planets.iter_mut() {
        *visibility = Visibility::Hidden;
    }
    for mut light in solar_lights.iter_mut() {
        light.intensity = 0.0;
    }
}

/// Startup system: spawn the bound planet, its moons, and the Sun in flight units.
pub fn setup_rocket_planets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    solar_params: Res<SolarSystemParameters>,
    physical_scale: Res<PhysicalScale>,
    rocket_query: Query<(&RocketPlanetBinding, &RocketPhysicsState)>,
    mut bound_planet_res: ResMut<RocketBoundPlanet>,
) {
    let Some((binding, rocket)) = rocket_query.iter().next() else {
        return;
    };
    let planet_name = binding.planet_name.to_string();
    bound_planet_res.0 = Some(planet_name.clone());

    // Create the bound planet (Earth) with true-scale radius in meters
    if let Some(planet) = PlanetFactory::create_by_name(&planet_name) {
        let radius_m = planet.radius_km as f64 * 1000.0;
        let show_planet_proxy =
            !local_terrain_is_required((rocket.dynamics.position_m.length() - radius_m).max(0.0));
        let mesh_handle = create_flight_globe_mesh(&mut meshes, radius_m as f32);

        let textures = get_planet_textures(&planet_name);
        let albedo_handle = load_texture(&asset_server, textures.albedo);
        let emissive_handle = load_texture(&asset_server, textures.emissive);

        let (metallic, reflectance, perceptual_roughness, base_color) = match planet_name.as_str() {
            "Mercury" => (0.1, 0.3, 0.8, planet.color),
            "Venus" => (0.1, 0.75, 0.25, planet.color),
            "Earth" => (0.05, 0.4, 0.45, planet.color),
            "Mars" => (0.05, 0.25, 0.6, planet.color),
            "Jupiter" => (0.0, 0.7, 0.15, planet.color),
            "Saturn" => (0.0, 0.65, 0.15, planet.color),
            "Uranus" => (0.0, 0.5, 0.25, planet.color),
            "Neptune" => (0.0, 0.6, 0.25, planet.color),
            _ => (0.0, 0.5, 0.7, planet.color),
        };

        let material = create_planet_material(PlanetMaterialConfig {
            base_color_texture: albedo_handle.clone(),
            normal_map_texture: None,
            emissive_texture: emissive_handle.clone(),
            base_color,
            emissive: if planet_name == "Sun" {
                LinearRgba::new(1.0, 1.0, 0.8, 1.0)
            } else if textures.albedo.is_some() {
                LinearRgba::new(0.35, 0.35, 0.35, 1.0)
            } else {
                LinearRgba::BLACK
            },
            unlit: planet_name == "Sun",
            metallic,
            reflectance,
            perceptual_roughness,
        });

        let material_handle = materials.add(material);
        let planet_entity = commands
            .spawn((
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle),
                Transform::default(),
                RocketPlanet {
                    name: planet_name.clone(),
                    is_bound_planet: true,
                    is_sun: false,
                },
                if show_planet_proxy {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                },
            ))
            .id();

        // The camera is only meters above a sphere whose center is thousands
        // of kilometers away. Its bounds can otherwise be rejected before its
        // nearby surface reaches the frustum, leaving terrain patches against
        // empty space instead of a continuous curved planet.
        commands
            .entity(planet_entity)
            .insert(bevy::camera::visibility::NoFrustumCulling);

        // Spawn moons of this planet
        let moons = PlanetFactory::get_moons_of(&planet_name);
        for moon in moons {
            spawn_rocket_moon(
                &mut commands,
                &mut meshes,
                &mut materials,
                &asset_server,
                &moon,
            );
        }
    }

    // Spawn the Sun
    spawn_rocket_sun(
        &mut commands,
        &mut meshes,
        &mut materials,
        &asset_server,
        &solar_params,
        &physical_scale,
    );
}

/// Spawn a moon in flight units.
fn spawn_rocket_moon(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &AssetServer,
    moon: &crate::domain::entities::planet::Planet,
) {
    let radius_m = moon.radius_km * 1000.0;
    let mesh_handle = meshes.add(Sphere::new(radius_m as f32));

    let textures = get_planet_textures(&moon.name);
    let albedo_handle = load_texture(asset_server, textures.albedo);
    let emissive_handle = load_texture(asset_server, textures.emissive);

    let (metallic, reflectance, perceptual_roughness, base_color) = match moon.name.as_str() {
        "Moon" => (0.05, 0.12, 0.9, moon.color),
        "Phobos" | "Deimos" => (0.05, 0.1, 0.85, moon.color),
        "Io" => (0.1, 0.3, 0.7, moon.color),
        "Europa" => (0.05, 0.5, 0.3, moon.color),
        "Ganymede" => (0.05, 0.4, 0.5, moon.color),
        "Callisto" => (0.05, 0.3, 0.6, moon.color),
        "Titan" => (0.05, 0.5, 0.4, moon.color),
        _ => (0.05, 0.5, 0.7, moon.color),
    };

    let material = create_planet_material(PlanetMaterialConfig {
        base_color_texture: albedo_handle.clone(),
        normal_map_texture: None,
        emissive_texture: emissive_handle.clone(),
        base_color,
        emissive: if textures.albedo.is_some() {
            LinearRgba::new(0.35, 0.35, 0.35, 1.0)
        } else {
            LinearRgba::BLACK
        },
        unlit: false,
        metallic,
        reflectance,
        perceptual_roughness,
    });

    let material_handle = materials.add(material);
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::default(),
        RocketMoon {
            name: moon.name.clone(),
            parent_planet: moon.parent_entity.as_ref().unwrap().clone(),
        },
    ));
}

/// Spawn the Sun in flight units.
fn spawn_rocket_sun(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &AssetServer,
    solar_params: &SolarSystemParameters,
    physical_scale: &PhysicalScale,
) {
    // True Sun radius in meters
    let sun_radius_m = 696_340_000.0;
    let mesh_handle = meshes.add(Sphere::new(sun_radius_m as f32));

    let textures = get_planet_textures("Sun");
    let albedo_handle = load_texture(asset_server, textures.albedo);

    let material = create_planet_material(PlanetMaterialConfig {
        base_color_texture: albedo_handle.clone(),
        normal_map_texture: None,
        emissive_texture: albedo_handle,
        base_color: Color::srgb(1.0, 1.0, 0.98),
        emissive: LinearRgba::new(1.0, 1.0, 0.8, 1.0),
        unlit: true,
        metallic: 0.0,
        reflectance: 0.0,
        perceptual_roughness: 0.0,
    });

    let material_handle = materials.add(material);
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::default(),
        RocketPlanet {
            name: "Sun".to_string(),
            is_bound_planet: false,
            is_sun: true,
        },
    ));
}

/// Update system: position rocket-mode planets from solar system state.
/// Runs after solar system planet positions are updated.
pub fn update_rocket_planets(
    solar_params: Res<SolarSystemParameters>,
    physical_scale: Res<PhysicalScale>,
    render_origin: Res<RenderOrigin>,
    sim_time: Res<SimulationTime>,
    rocket_query: Query<(&RocketPlanetBinding, &RocketPhysicsState)>,
    planet_query: Query<
        (&PlanetComponent, &Transform),
        (Without<RocketPlanet>, Without<RocketMoon>),
    >,
    mut query_set: ParamSet<(
        Query<(&RocketPlanet, &mut Transform, &mut Visibility)>,
        Query<(&RocketMoon, &mut Transform)>,
    )>,
    bound_planet_res: Res<RocketBoundPlanet>,
) {
    let Some(bound_planet_name) = &bound_planet_res.0 else {
        return;
    };
    let Some((binding, rocket)) = rocket_query.iter().next() else {
        return;
    };

    // Find the bound planet's solar system transform
    let bound_planet_transform = planet_query
        .iter()
        .find(|(p, _)| p.domain_planet.name == *bound_planet_name)
        .map(|(_, t)| t.translation);

    let Some(bound_planet_pos) = bound_planet_transform else {
        return;
    };
    let bound_planet_radius_m = planet_query
        .iter()
        .find(|(planet, _)| planet.domain_planet.name == binding.planet_name)
        .map(|(planet, _)| planet.domain_planet.radius_km as f64 * 1000.0)
        .unwrap_or(6_371_000.0);
    let show_bound_planet_proxy = !local_terrain_is_required(
        (rocket.dynamics.position_m.length() - bound_planet_radius_m).max(0.0),
    );

    // Conversion: solar display units -> meters
    let display_to_meters = physical_scale.solar_meters_per_display_unit as f64;

    // Bound planet and Sun: always at origin in flight frame (render_origin tracks rocket)
    // The planet center is at -render_origin.origin
    let planet_center_flight = -render_origin.origin.as_vec3();
    for (rocket_planet, mut transform, mut visibility) in query_set.p0().iter_mut() {
        if rocket_planet.is_bound_planet {
            transform.translation = planet_center_flight;
            *visibility = if show_bound_planet_proxy {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            if let Some((planet, _)) = planet_query
                .iter()
                .find(|(planet, _)| planet.domain_planet.name == rocket_planet.name)
            {
                transform.rotation = body_fixed_to_inertial_rotation(
                    &planet.domain_planet,
                    (sim_time.sim_time_s / 86_400.0) as f32,
                )
                .as_quat();
            }
        } else if rocket_planet.is_sun {
            // Sun position: solar_pos - bound_planet_pos (relative), converted to meters
            let sun_solar = planet_query
                .iter()
                .find(|(p, _)| p.domain_planet.name == "Sun")
                .map(|(_, t)| t.translation);

            if let Some(sun_pos) = sun_solar {
                let rel = (sun_pos - bound_planet_pos).as_dvec3() * display_to_meters;
                transform.translation = (planet_center_flight.as_dvec3() + rel).as_vec3();
            }
        }
    }

    // Moons: position relative to bound planet
    for (rocket_moon, mut transform) in query_set.p1().iter_mut() {
        if rocket_moon.parent_planet == *bound_planet_name {
            let moon_solar = planet_query
                .iter()
                .find(|(p, _)| p.domain_planet.name == rocket_moon.name)
                .map(|(_, t)| t.translation);

            if let Some(moon_pos) = moon_solar {
                // Shared solar presentation intentionally exaggerates moon
                // orbits. Flight proxies must undo that visual-only scale.
                let rel = (moon_pos - bound_planet_pos).as_dvec3()
                    * (display_to_meters / MOON_ORBIT_SCALE as f64);
                transform.translation = (planet_center_flight.as_dvec3() + rel).as_vec3();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::value_objects::physical_scale::PhysicalScale;
    use crate::domain::value_objects::solar_system_params::SolarSystemParameters;

    #[test]
    fn test_physical_scale_conversion() {
        let solar = SolarSystemParameters::for_visualization();
        let scale = PhysicalScale::from_solar_parameters(&solar);
        // 1 AU in meters should map to scale_factor display units
        let au_display = scale.solar_meters_to_units(149_597_870_700.0);
        assert!((au_display - solar.scale_factor as f64).abs() < 1.0);
    }
}
