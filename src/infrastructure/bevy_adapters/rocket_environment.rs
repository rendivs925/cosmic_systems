use crate::domain::services::physics_orbital::heliocentric_direction_to_sun_f64;
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use crate::infrastructure::bevy_adapters::rocket_planet::RocketBoundPlanet;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;

/// Spawns a directional sunlight source. The Sun's inertial direction comes
/// from the shared ephemeris; the rotating planet moves terrain through that
/// fixed direction to produce the physical day/night cycle.
pub fn setup_rocket_sun_light(
    mut commands: Commands,
    sim_time: Res<SimulationTime>,
    bound_planet: Res<RocketBoundPlanet>,
    planet_query: Query<&PlanetComponent>,
) {
    let sun_direction = bound_planet
        .0
        .as_deref()
        .and_then(|name| {
            planet_query
                .iter()
                .find(|planet| planet.domain_planet.name == name)
        })
        .and_then(|planet| {
            heliocentric_direction_to_sun_f64(&planet.domain_planet, sim_time.sim_time_s / 86_400.0)
        })
        .unwrap_or(bevy::math::DVec3::NEG_Z)
        .as_vec3();

    // Sky-blue ambient fill so shadowed faces read as sky-lit instead of black.
    commands.insert_resource(bevy::light::AmbientLight {
        color: Color::srgb(0.5, 0.6, 0.75),
        brightness: 400.0,
        ..default()
    });

    commands.spawn((
        bevy::light::DirectionalLight {
            illuminance: 100_000.0,             // bright daylight (lux)
            color: Color::srgb(1.0, 0.9, 0.75), // warm low-sun light
            shadows_enabled: true,
            ..default()
        },
        // Cascade shadow config tuned for the rocket flight scale (1 unit = 1 m).
        // The first cascade covers the immediate pad area; later cascades extend
        // to the horizon so distant terrain still casts visible shadows.
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 30.0,
            maximum_distance: 800.0,
            ..default()
        }
        .build(),
        // Light travels along local -Z toward the scene; orient it so the Sun
        // appears in its ephemeris direction.
        Transform::from_xyz(0.0, 0.0, 0.0).looking_at(-sun_direction, Vec3::Y),
        SunLight,
    ));
}

/// Tag component marking the sun directional light for day/night rotation.
#[derive(Component, Debug)]
pub struct SunLight;

/// Space is the clear color. Atmospheric haze is applied only to local geometry
/// through the camera fog; it must never turn the entire universe blue.
pub fn setup_rocket_sky_color(mut clear_color: ResMut<ClearColor>) {
    *clear_color = ClearColor(Color::srgb(0.002, 0.002, 0.006));
}

/// Kept as an update hook so future atmospheric scattering can drive a local
/// sky pass. ClearColor deliberately remains space-black at every altitude.
pub fn update_rocket_sky_color(mut clear_color: ResMut<ClearColor>) {
    *clear_color = ClearColor(Color::srgb(0.002, 0.002, 0.006));
}

/// Updates rocket-mode sunlight from the same ephemeris state used by the Sun
/// proxy. Planet and terrain rotation, rather than an artificial light orbit,
/// produces the local day/night cycle.
pub fn update_sun_day_night_cycle(
    sim_time: Res<SimulationTime>,
    bound_planet: Res<RocketBoundPlanet>,
    planet_query: Query<&PlanetComponent>,
    mut sun_query: Query<&mut Transform, With<SunLight>>,
) {
    let Some(planet) = planet_query
        .iter()
        .find(|planet| bound_planet.0.as_deref() == Some(planet.domain_planet.name.as_str()))
    else {
        return;
    };
    let Some(sun_direction) =
        heliocentric_direction_to_sun_f64(&planet.domain_planet, sim_time.sim_time_s / 86_400.0)
    else {
        return;
    };
    let sun_direction = sun_direction.as_vec3();

    for mut light_transform in sun_query.iter_mut() {
        *light_transform = Transform::from_xyz(0.0, 0.0, 0.0).looking_at(-sun_direction, Vec3::Y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::infrastructure::bevy_adapters::components::PlanetComponent;

    #[test]
    fn rocket_sunlight_matches_the_shared_ephemeris_direction() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let mut app = App::new();
        let mut simulation_time = SimulationTime::default();
        simulation_time.sim_time_s = 86_400.0;
        app.insert_resource(simulation_time);
        app.insert_resource(RocketBoundPlanet(Some("Earth".to_string())));
        app.world_mut().spawn(PlanetComponent {
            domain_planet: earth.clone(),
            material: default(),
            has_texture: false,
            base_reflectance: 0.0,
            base_roughness: 0.0,
        });
        let light = app.world_mut().spawn((SunLight, Transform::default())).id();
        app.add_systems(Update, update_sun_day_night_cycle);

        app.update();

        let expected = heliocentric_direction_to_sun_f64(&earth, 1.0)
            .unwrap()
            .as_vec3();
        let transform = app.world().entity(light).get::<Transform>().unwrap();
        let light_travel_direction = transform.rotation * -Vec3::Z;

        assert!((-light_travel_direction).distance(expected) < 1e-6);
    }
}
