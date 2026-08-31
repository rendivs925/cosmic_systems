//! Rocket-mode planet system.
//!
//! In rocket mode, the camera is at the rocket's position (flight units = meters).
//! This system spawns and positions the bound planet (Earth), its moons, and the
//! Sun in flight units using real textures. Their orbital presentation is
//! evaluated from the rocket simulation epoch rather than shared solar-map
//! transforms, whose display scale is not an authoritative flight frame.
//!
//! The existing display-scale conversion remains only for the current
//! parent-relative moon approximation.

use crate::application::material_factory::{create_planet_material, PlanetMaterialConfig};
use crate::application::solar_system_startup::solar_surface_luminance_nits;
use crate::application::texture_config::{get_planet_textures, load_texture};
use crate::components::rocket::{RocketPhysicsState, RocketPlanetBinding};
use crate::domain::services::ephemeris::NaifBodyId;
use crate::domain::services::physics::calculate_planet_position_f64;
use crate::domain::services::physics_orbital::MOON_ORBIT_SCALE;
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::services::reference_frames::body_fixed_to_planet_inertial_rotation;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::*;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::terrain_render::RenderOrigin;
use bevy::ecs::system::ParamSet;
use bevy::math::DVec3;
use bevy::prelude::*;

/// Component marking a planet entity managed by the rocket planet system.
#[derive(Component, Debug, Clone)]
pub struct RocketPlanet {
    pub name: String,
    pub is_bound_planet: bool,
    pub is_sun: bool,
}

/// Marks the local visual representation of the Sun. Its transform is camera
/// relative so the rocket camera retains its local depth range while the disc
/// keeps the Sun's true angular diameter.
#[derive(Component, Debug)]
pub struct RocketSunDisc;

const ROCKET_SUN_DISC_DISTANCE_M: f64 = 20_000.0;
const SUN_RADIUS_M: f64 = 696_340_000.0;
const SUN_MEAN_DISTANCE_M: f64 = 149_597_870_700.0;

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
#[expect(
    clippy::type_complexity,
    reason = "The query selects both planet and orbit presentation entities."
)]
pub fn isolate_rocket_presentation(
    mut solar_presentation: Query<
        &mut Visibility,
        Or<(With<PlanetComponent>, With<OrbitComponent>)>,
    >,
    mut solar_lights: Query<&mut PointLight>,
) {
    for mut visibility in solar_presentation.iter_mut() {
        *visibility = Visibility::Hidden;
    }
    for mut light in solar_lights.iter_mut() {
        light.intensity = 0.0;
    }
}

/// Startup system: spawn moons and the Sun in flight units.
///
/// The streamed terrain renderer owns the bound planet's visible surface. A
/// second globe proxy cannot agree with local terrain elevation at launch and
/// must not become a presentation fallback beneath the vehicle.
pub fn setup_rocket_planets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    solar_params: Res<SolarSystemParameters>,
    rocket_query: Query<(&RocketPlanetBinding, &RocketPhysicsState)>,
    mut bound_planet_res: ResMut<RocketBoundPlanet>,
) {
    let Some((binding, _rocket)) = rocket_query.iter().next() else {
        return;
    };
    let planet_name = binding.planet_name.to_string();
    bound_planet_res.0 = Some(planet_name.clone());

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
    spawn_rocket_sun(
        &mut commands,
        &mut meshes,
        &mut materials,
        &asset_server,
        &solar_params,
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
    let mesh_handle = meshes.add(Sphere::new(radius_m));

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
) {
    let mesh_handle = meshes.add(Sphere::new(1.0));

    let textures = get_planet_textures("Sun");
    let albedo_handle = load_texture(asset_server, textures.albedo);

    let material = create_planet_material(PlanetMaterialConfig {
        base_color_texture: albedo_handle.clone(),
        normal_map_texture: None,
        emissive_texture: albedo_handle,
        base_color: Color::srgb(1.0, 1.0, 0.98),
        emissive: LinearRgba::new(
            solar_surface_luminance_nits(solar_params),
            solar_surface_luminance_nits(solar_params),
            solar_surface_luminance_nits(solar_params),
            1.0,
        ),
        // Match the solar-map material path so HDR emission participates in
        // the same tone mapping and bloom presentation.
        unlit: false,
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
        RocketSunDisc,
    ));
}

/// Update rocket celestial proxies from the authoritative rocket simulation epoch.
///
/// Rocket proxies must not consume shared solar-map transforms because the
/// flight presentation has a distinct meter-scale render origin. Both paths
/// derive their celestial state from the same `SimulationTime` epoch.
#[expect(
    clippy::too_many_arguments,
    reason = "This presentation system synchronizes independent shared and rocket-mode state."
)]
#[expect(
    clippy::type_complexity,
    reason = "The ParamSet keeps planet and moon presentation queries borrow-safe."
)]
pub fn update_rocket_planets(
    solar_params: Res<SolarSystemParameters>,
    physical_scale: Res<PhysicalScale>,
    render_origin: Res<RenderOrigin>,
    sim_time: Res<SimulationTime>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
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

    let Some(bound_body) = NaifBodyId::for_catalog_name(bound_planet_name) else {
        return;
    };
    let Some(bound_orientation) = ephemeris_snapshot.orientation(bound_body) else {
        return;
    };
    // Conversion: solar display units -> meters
    let display_to_meters = physical_scale.solar_meters_per_display_unit;

    // The bound planet remains centered in the flight frame (render_origin
    // tracks the rocket). The Sun disc is owned by update_rocket_sun_disc.
    // The planet center is at -render_origin.origin
    let planet_center_flight = -render_origin.origin.as_vec3();
    for (rocket_planet, mut transform, mut visibility) in query_set.p0().iter_mut() {
        if rocket_planet.is_bound_planet {
            transform.translation = planet_center_flight;
            *visibility = Visibility::Visible;
            transform.rotation =
                body_fixed_to_planet_inertial_rotation(bound_orientation).as_quat();
        }
    }

    // Moons: position relative to bound planet
    for (rocket_moon, mut transform) in query_set.p1().iter_mut() {
        if rocket_moon.parent_planet == *bound_planet_name {
            if let Some(moon_relative_to_bound) = NaifBodyId::for_catalog_name(&rocket_moon.name)
                .and_then(|body| ephemeris_snapshot.solar_inertial_relative_state(body, bound_body))
            {
                transform.translation =
                    (planet_center_flight.as_dvec3() + moon_relative_to_bound.position_m).as_vec3();
                continue;
            }
            let moon_solar = planet_query
                .iter()
                .find(|planet| planet.domain_planet.name == rocket_moon.name)
                .map(|planet| {
                    calculate_planet_position_f64(
                        &planet.domain_planet,
                        sim_time.sim_time_s / 86_400.0,
                        &solar_params,
                        DVec3::ZERO,
                        Some(bound_planet.domain_planet.axial_tilt_deg),
                    )
                });

            if let Some(moon_pos) = moon_solar {
                // Shared solar presentation intentionally exaggerates moon
                // orbits. Flight proxies must undo that visual-only scale.
                let rel = moon_pos * (display_to_meters / MOON_ORBIT_SCALE as f64);
                transform.translation = (planet_center_flight.as_dvec3() + rel).as_vec3();
            }
        }
    }
}

/// Keep the visual Sun inside the local rocket camera depth range while its
/// direction comes from the evaluated ephemeris snapshot. This is presentation
/// only; the directional light remains the illumination authority.
pub fn update_rocket_sun_disc(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
    camera_query: Query<&Transform, (With<Camera3d>, Without<RocketSunDisc>)>,
    mut sun_query: Query<(&mut Transform, &mut Visibility), With<RocketSunDisc>>,
) {
    let Some(bound_body) = bound_planet
        .0
        .as_deref()
        .and_then(NaifBodyId::for_catalog_name)
    else {
        return;
    };
    let Some(sun_direction) = ephemeris_snapshot
        .solar_inertial_relative_state(NaifBodyId::SUN, bound_body)
        .map(|state| state.position_m.normalize_or_zero())
        .filter(|direction| direction.length_squared() > 0.0)
    else {
        return;
    };
    let Some(camera) = camera_query.iter().next() else {
        return;
    };

    let radius_m = rocket_sun_disc_radius_m(ROCKET_SUN_DISC_DISTANCE_M);
    let translation =
        camera.translation + sun_direction.as_vec3() * ROCKET_SUN_DISC_DISTANCE_M as f32;
    for (mut transform, mut visibility) in sun_query.iter_mut() {
        transform.translation = translation;
        transform.scale = Vec3::splat(radius_m as f32);
        *visibility = Visibility::Visible;
    }
}

fn rocket_sun_disc_radius_m(distance_m: f64) -> f64 {
    distance_m * (SUN_RADIUS_M / SUN_MEAN_DISTANCE_M)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::body_orientation::BodyOrientation;
    use crate::domain::services::ephemeris::{BodyState, TdbEpoch};
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::value_objects::physical_scale::PhysicalScale;
    use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
    use crate::infrastructure::bevy_adapters::components::PlanetComponent;
    use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
    use bevy::math::{DMat3, DQuat, DVec3};

    fn earth_orientation(epoch: TdbEpoch) -> BodyOrientation {
        BodyOrientation::from_kernel(
            NaifBodyId::EARTH,
            epoch,
            "test-orientation".to_string(),
            DQuat::IDENTITY,
            DVec3::Z * (std::f64::consts::TAU / (23.934 * 3_600.0)),
        )
    }

    #[test]
    fn test_physical_scale_conversion() {
        let solar = SolarSystemParameters::for_visualization();
        let scale = PhysicalScale::from_solar_parameters(&solar);
        // 1 AU in meters should map to scale_factor display units
        let au_display = scale.solar_meters_to_units(149_597_870_700.0);
        assert!((au_display - solar.scale_factor as f64).abs() < 1.0);
    }

    #[test]
    fn rocket_moon_proxy_follows_simulation_time_not_shared_transforms() {
        let solar = SolarSystemParameters::for_visualization();
        let scale = PhysicalScale::from_solar_parameters(&solar);
        let sim_time_s = 86_400.0;
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let moon = PlanetFactory::create_by_name("Moon").unwrap();

        let mut app = App::new();
        app.insert_resource(solar.clone());
        app.insert_resource(scale);
        app.insert_resource(RenderOrigin::default());
        let mut simulation_time = SimulationTime::default();
        simulation_time.sim_time_s = sim_time_s;
        app.insert_resource(simulation_time);
        app.insert_resource(RocketBoundPlanet(Some("Earth".to_string())));
        let epoch = TdbEpoch::j2000();
        app.insert_resource(EphemerisSnapshot::from_states_and_orientations(
            vec![
                BodyState {
                    target: NaifBodyId::EARTH,
                    center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                    epoch,
                    position_m: DVec3::ZERO,
                    velocity_mps: DVec3::ZERO,
                },
                BodyState {
                    target: NaifBodyId::SUN,
                    center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                    epoch,
                    position_m: -DVec3::X * 149_597_870_700.0,
                    velocity_mps: DVec3::ZERO,
                },
                BodyState {
                    target: NaifBodyId::MOON,
                    center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                    epoch,
                    position_m: DVec3::X * 384_400_000.0,
                    velocity_mps: DVec3::ZERO,
                },
            ],
            vec![earth_orientation(epoch)],
        ));
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

        let expected_moon = (DVec3::X * 384_400_000.0).as_vec3();

        assert_eq!(
            app.world()
                .entity(rocket_moon)
                .get::<Transform>()
                .unwrap()
                .translation,
            expected_moon
        );
    }

    #[test]
    fn rocket_sun_disc_preserves_the_mean_solar_angular_radius() {
        let radius_m = rocket_sun_disc_radius_m(ROCKET_SUN_DISC_DISTANCE_M);
        let angular_radius_rad = (radius_m / ROCKET_SUN_DISC_DISTANCE_M).asin();
        let expected_angular_radius_rad = (SUN_RADIUS_M / SUN_MEAN_DISTANCE_M).asin();

        assert!((angular_radius_rad - expected_angular_radius_rad).abs() < 1e-12);
    }
}
