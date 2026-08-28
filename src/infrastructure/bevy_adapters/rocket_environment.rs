use crate::components::rocket::*;
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::math::Vec3;
use bevy::prelude::*;

/// Spawns a directional sun light for rocket mode. The solar simulation uses
/// a PointLight at the origin, but in the flight frame the sun should be a
/// directional light at infinity. The sun is placed well above the LOCAL horizon
/// (the rocket's radial up direction) so the pad and terrain are brightly lit:
/// a fixed world-space direction would sit only a few degrees above the KSC
/// horizon because the flight frame's axes are not the planet's local frame.
pub fn setup_rocket_sun_light(mut commands: Commands, rocket_query: Query<&RocketPhysicsState>) {
    // Radial up at the pad (the rocket's body +Y at spawn).
    let up = rocket_query
        .iter()
        .next()
        .map(|r| r.dynamics.position_m.normalize_or_zero().as_vec3())
        .filter(|v| v.length_squared() > 0.5)
        .unwrap_or(Vec3::Y);
    // A fixed horizontal reference perpendicular to the local up.
    let east = if up.z.abs() < 0.9 {
        up.cross(Vec3::Z).normalize()
    } else {
        up.cross(Vec3::X).normalize()
    };
    // Sun ~20 deg above the local horizon (morning golden hour): long shadows
    // and warm light like the launch-pad reference footage.
    let sun_dir = (up * 0.342 + east * 0.94).normalize();

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
        // Light travels along the light's -Z toward the scene; orient it so the
        // sun appears in the `sun_dir` direction.
        Transform::from_xyz(0.0, 0.0, 0.0).looking_at(-sun_dir, Vec3::Y),
        // Tag component so the day/night system can find and rotate this light.
        SunLight,
        // Store the computed sun direction so the day/night rotation starts from
        // the correct horizon angle (not a generic default).
        SunLightState {
            initial_direction: sun_dir,
        },
    ));
}

/// Tag component marking the sun directional light for day/night rotation.
#[derive(Component, Debug)]
pub struct SunLight;

/// Component storing the sun's initial direction so the day/night system can
/// rotate it around the planet's north pole each frame.
#[derive(Component, Debug)]
pub struct SunLightState {
    pub initial_direction: Vec3,
}

impl Default for SunLightState {
    fn default() -> Self {
        Self {
            initial_direction: Vec3::new(0.0, 0.26, 0.97).normalize(), // ~15 deg above horizon
        }
    }
}

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

/// Day/night cycle: rotates the sun light direction around the planet's rotation
/// axis (Y in the flight frame) as simulation time advances. The planet's angular
/// velocity comes from the Earth planet definition. The sun makes one full
/// revolution per planet rotation period (~24 hours for Earth).
pub fn update_sun_day_night_cycle(
    sim_time: Res<SimulationTime>,
    planet_query: Query<&PlanetComponent>,
    mut sun_query: Query<(&mut Transform, &SunLightState), With<SunLight>>,
) {
    // Find Earth planet for its rotation period, then compute angular velocity.
    // omega = 2π / period_seconds.
    let earth_rotation_rad_s = planet_query
        .iter()
        .find(|p| p.domain_planet.name == "Earth")
        .map(|p| {
            let period_s = p.domain_planet.rotation_period_hours as f64 * 3600.0;
            if period_s > 0.0 {
                std::f64::consts::TAU / period_s
            } else {
                7.2921159e-5 // Earth sidereal rotation rate rad/s
            }
        })
        .unwrap_or(7.2921159e-5_f64);

    let total_time_s = sim_time.sim_time_s;
    let rotation_angle = (total_time_s * earth_rotation_rad_s) as f32;

    for (mut light_transform, sun_state) in sun_query.iter_mut() {
        // Rotate initial sun direction around the Y axis (planet rotation axis).
        // The planet's north pole points along +Y in the flight frame.
        let cos_a = rotation_angle.cos();
        let sin_a = rotation_angle.sin();
        let dir = sun_state.initial_direction;
        let rotated = Vec3::new(
            cos_a * dir.x - sin_a * dir.z,
            dir.y,
            sin_a * dir.x + cos_a * dir.z,
        )
        .normalize();

        // Update the light's look-direction so the sun travels across the sky.
        *light_transform = Transform::from_xyz(0.0, 0.0, 0.0).looking_at(-rotated, Vec3::Y);
    }
}
