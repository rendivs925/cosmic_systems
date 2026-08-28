//! Rocket-mode planet system.
//!
//! In rocket mode, the camera is at the rocket's position (flight units = meters).
//! This system spawns and positions the bound planet (Earth), its moons, and the
//! Sun in flight units using real textures. Their orbital presentation is
//! evaluated from the rocket simulation epoch rather than shared solar-map
//! transforms, whose clock is intentionally wall-clock driven.
//!
//! Conversion: solar system display units -> meters using PhysicalScale.

use crate::application::material_factory::{create_planet_material, PlanetMaterialConfig};
use crate::application::mesh_factory::create_flight_globe_mesh;
use crate::application::texture_config::{get_planet_textures, load_texture};
use crate::components::rocket::{RocketPhysicsState, RocketPlanetBinding};
use crate::domain::services::physics::calculate_planet_position;
use crate::domain::services::physics_orbital::MOON_ORBIT_SCALE;
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::services::reference_frames::body_fixed_to_inertial_rotation;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::*;
use crate::infrastructure::bevy_adapters::terrain_render::RenderOrigin;
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
    rocket_query: Query<(&RocketPlanetBinding, &RocketPhysicsState)>,
    mut bound_planet_res: ResMut<RocketBoundPlanet>,
) {
    let Some((binding, _rocket)) = rocket_query.iter().next() else {
        return;
    };
    let planet_name = binding.planet_name.to_string();
    bound_planet_res.0 = Some(planet_name.clone());

    let Some(bound_planet) = PlanetFactory::create_by_name(&planet_name) else {
        return;
    };
    spawn_rocket_bound_planet(
        &mut commands,
        &mut meshes,
        &mut materials,
        &asset_server,
        &bound_planet,
    );

    // The globe is an inner presentation fallback. Terrain streaming overlays
    // only the visible camera neighborhood, so no full-planet terrain bake is
    // required to avoid a hole at the horizon.
    for moon in PlanetFactory::get_moons_of(&planet_name) {
        spawn_rocket_moon(
            &mut commands,
            &mut meshes,
            &mut materials,
            &asset_server,
            &moon,
        );
    }

    // Spawn the Sun
    spawn_rocket_sun(&mut commands, &mut meshes, &mut materials, &asset_server);
}

fn spawn_rocket_bound_planet(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &AssetServer,
    planet: &crate::domain::entities::planet::Planet,
) {
    const TERRAIN_FALLBACK_DEPTH_M: f32 = 15_000.0;
    let radius_m = (planet.radius_km * 1_000.0 - TERRAIN_FALLBACK_DEPTH_M).max(1.0);
    let mesh_handle = create_flight_globe_mesh(meshes, radius_m);
    let textures = get_planet_textures(&planet.name);
    let material_handle = materials.add(create_planet_material(PlanetMaterialConfig {
        base_color_texture: load_texture(asset_server, textures.albedo),
        normal_map_texture: None,
        emissive_texture: load_texture(asset_server, textures.emissive),
        base_color: planet.color,
        emissive: LinearRgba::BLACK,
        unlit: false,
        metallic: 0.0,
        reflectance: 0.35,
        perceptual_roughness: 0.9,
    }));
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::default(),
        RocketPlanet {
            name: planet.name.clone(),
            is_bound_planet: true,
            is_sun: false,
        },
    ));
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

/// Update rocket celestial proxies from the authoritative rocket simulation epoch.
///
/// Shared solar-map transforms advance using display wall-clock time for normal
/// and craft modes. Rocket proxies must not consume those transforms because
/// replay, pause, and warp advance `SimulationTime` independently.
pub fn update_rocket_planets(
    solar_params: Res<SolarSystemParameters>,
    physical_scale: Res<PhysicalScale>,
    render_origin: Res<RenderOrigin>,
    sim_time: Res<SimulationTime>,
    rocket_query: Query<(), With<RocketPhysicsState>>,
    planet_query: Query<&PlanetComponent, (Without<RocketPlanet>, Without<RocketMoon>)>,
    mut query_set: ParamSet<(
        Query<(&RocketPlanet, &mut Transform, &mut Visibility)>,
        Query<(&RocketMoon, &mut Transform)>,
    )>,
    bound_planet_res: Res<RocketBoundPlanet>,
) {
    let Some(bound_planet_name) = &bound_planet_res.0 else {
        return;
    };
    if rocket_query.is_empty() {
        return;
    }
    let Some(bound_planet) = planet_query
        .iter()
        .find(|planet| planet.domain_planet.name == *bound_planet_name)
    else {
        return;
    };

    let time_days = (sim_time.sim_time_s / 86_400.0) as f32;
    let bound_planet_pos = calculate_planet_position(
        &bound_planet.domain_planet,
        time_days,
        &solar_params,
        Vec3::ZERO,
        None,
    );
    // Conversion: solar display units -> meters
    let display_to_meters = physical_scale.solar_meters_per_display_unit as f64;

    // Bound planet and Sun: always at origin in flight frame (render_origin tracks rocket)
    // The planet center is at -render_origin.origin
    let planet_center_flight = -render_origin.origin.as_vec3();
    for (rocket_planet, mut transform, mut visibility) in query_set.p0().iter_mut() {
        if rocket_planet.is_bound_planet {
            transform.translation = planet_center_flight;
            *visibility = Visibility::Visible;
            if let Some(planet) = planet_query
                .iter()
                .find(|planet| planet.domain_planet.name == rocket_planet.name)
            {
                transform.rotation = body_fixed_to_inertial_rotation(
                    &planet.domain_planet,
                    (sim_time.sim_time_s / 86_400.0) as f32,
                )
                .as_quat();
            }
        } else if rocket_planet.is_sun {
            // Sun position relative to the bound planet, converted to meters.
            let sun_solar = planet_query
                .iter()
                .find(|planet| planet.domain_planet.name == "Sun")
                .map(|planet| {
                    calculate_planet_position(
                        &planet.domain_planet,
                        time_days,
                        &solar_params,
                        Vec3::ZERO,
                        None,
                    )
                });

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
                .find(|planet| planet.domain_planet.name == rocket_moon.name)
                .map(|planet| {
                    calculate_planet_position(
                        &planet.domain_planet,
                        time_days,
                        &solar_params,
                        bound_planet_pos,
                        Some(bound_planet.domain_planet.axial_tilt_deg),
                    )
                });

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
    use crate::domain::services::physics::calculate_planet_position;
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::value_objects::physical_scale::PhysicalScale;
    use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
    use crate::infrastructure::bevy_adapters::components::PlanetComponent;
    use bevy::math::{DMat3, DQuat, DVec3};

    #[test]
    fn test_physical_scale_conversion() {
        let solar = SolarSystemParameters::for_visualization();
        let scale = PhysicalScale::from_solar_parameters(&solar);
        // 1 AU in meters should map to scale_factor display units
        let au_display = scale.solar_meters_to_units(149_597_870_700.0);
        assert!((au_display - solar.scale_factor as f64).abs() < 1.0);
    }

    #[test]
    fn rocket_proxies_follow_simulation_time_not_shared_transforms() {
        let solar = SolarSystemParameters::for_visualization();
        let scale = PhysicalScale::from_solar_parameters(&solar);
        let sim_time_s = 86_400.0;
        let time_days = (sim_time_s / 86_400.0) as f32;
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let moon = PlanetFactory::create_by_name("Moon").unwrap();
        let sun = PlanetFactory::create_by_name("Sun").unwrap();

        let mut app = App::new();
        app.insert_resource(solar.clone());
        app.insert_resource(scale.clone());
        app.insert_resource(RenderOrigin::default());
        app.insert_resource(SimulationTime {
            sim_time_s,
            ..SimulationTime::default()
        });
        app.insert_resource(RocketBoundPlanet(Some("Earth".to_string())));
        app.world_mut().spawn(RocketPhysicsState {
            dynamics: RocketDynamicsState::new(
                DVec3::ZERO,
                DVec3::ZERO,
                DQuat::IDENTITY,
                1.0,
                DMat3::IDENTITY,
                DVec3::ZERO,
            ),
        });
        app.world_mut().spawn(PlanetComponent {
            domain_planet: earth.clone(),
            material: default(),
            has_texture: false,
            base_reflectance: 0.0,
            base_roughness: 0.0,
        });
        app.world_mut().spawn(PlanetComponent {
            domain_planet: moon.clone(),
            material: default(),
            has_texture: false,
            base_reflectance: 0.0,
            base_roughness: 0.0,
        });
        // This deliberately incorrect shared-presentation transform must not
        // affect rocket proxy placement.
        app.world_mut().spawn((
            PlanetComponent {
                domain_planet: sun.clone(),
                material: default(),
                has_texture: false,
                base_reflectance: 0.0,
                base_roughness: 0.0,
            },
            Transform::from_translation(Vec3::splat(123_456.0)),
        ));
        let rocket_sun = app
            .world_mut()
            .spawn((
                RocketPlanet {
                    name: "Sun".to_string(),
                    is_bound_planet: false,
                    is_sun: true,
                },
                Transform::default(),
                Visibility::Visible,
            ))
            .id();
        let rocket_moon = app
            .world_mut()
            .spawn((
                RocketMoon {
                    name: "Moon".to_string(),
                    parent_planet: "Earth".to_string(),
                },
                Transform::default(),
            ))
            .id();
        app.add_systems(Update, update_rocket_planets);

        app.update();

        let earth_pos = calculate_planet_position(&earth, time_days, &solar, Vec3::ZERO, None);
        let expected_sun =
            (-earth_pos.as_dvec3() * scale.solar_meters_per_display_unit as f64).as_vec3();
        let moon_pos = calculate_planet_position(
            &moon,
            time_days,
            &solar,
            earth_pos,
            Some(earth.axial_tilt_deg),
        );
        let expected_moon = ((moon_pos - earth_pos).as_dvec3()
            * (scale.solar_meters_per_display_unit as f64 / MOON_ORBIT_SCALE as f64))
            .as_vec3();

        assert_eq!(
            app.world()
                .entity(rocket_sun)
                .get::<Transform>()
                .unwrap()
                .translation,
            expected_sun
        );
        assert_eq!(
            app.world()
                .entity(rocket_moon)
                .get::<Transform>()
                .unwrap()
                .translation,
            expected_moon
        );
    }
}
