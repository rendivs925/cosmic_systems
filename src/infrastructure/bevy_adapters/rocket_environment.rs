use crate::application::solar_system_startup::SUN_ILLUMINANCE_AT_EARTH_LUX;
use crate::domain::services::ephemeris::NaifBodyId;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::rocket_planet::RocketBoundPlanet;
use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap};
use bevy::prelude::*;

/// Spawns a directional sunlight source. The Sun's inertial direction comes
/// from the shared ephemeris; the rotating planet moves terrain through that
/// fixed direction to produce the physical day/night cycle.
pub fn setup_rocket_sun_light(
    mut commands: Commands,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
) {
    // The flight camera needs a denser near-field map than the solar overview;
    // this is still one ephemeris-driven directional Sun, not a fill light.
    commands.insert_resource(DirectionalLightShadowMap { size: 4096 });
    let Some(sun_direction) = bound_planet
        .0
        .as_deref()
        .and_then(|name| sun_direction_for_bound_planet(&ephemeris_snapshot, name))
    else {
        bevy::log::error!(
            "cannot initialize rocket sunlight without a bound-planet ephemeris state"
        );
        return;
    };
    let sun_direction = sun_direction.as_vec3();

    commands.spawn((
        bevy::light::DirectionalLight {
            illuminance: SUN_ILLUMINANCE_AT_EARTH_LUX,
            color: Color::srgb(1.0, 1.0, 0.98),
            shadows_enabled: true,
            shadow_depth_bias: 0.015,
            shadow_normal_bias: 0.8,
            ..default()
        },
        // Cascade shadow config tuned for the rocket flight scale (1 unit = 1 m).
        // The first cascade covers the immediate pad area; later cascades extend
        // to the horizon so distant terrain still casts visible shadows.
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 45.0,
            maximum_distance: 1_200.0,
            overlap_proportion: 0.25,
            ..default()
        }
        .build(),
        // Light travels along local -Z toward the scene; orient it so the Sun
        // appears in its ephemeris direction.
        Transform::from_xyz(0.0, 0.0, 0.0).looking_at(-sun_direction, Vec3::Y),
        SunLight,
    ));
}

/// Sky fill is derived from the same Sun and local surface normal as direct
/// lighting. It keeps sunlit shadows readable without making the night side a
/// permanently lit gray scene.
pub fn update_rocket_sky_ambient_light(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
    rocket_query: Query<&crate::components::rocket::RocketPhysicsState>,
    mut ambient: ResMut<AmbientLight>,
) {
    let Some(sun_direction) = bound_planet
        .0
        .as_deref()
        .and_then(|name| sun_direction_for_bound_planet(&ephemeris_snapshot, name))
    else {
        return;
    };
    let Some(rocket) = rocket_query.iter().next() else {
        return;
    };
    let surface_normal = rocket.dynamics.position_m.normalize_or_zero();
    let daylight = ((surface_normal.dot(sun_direction) + 0.12) / 0.32).clamp(0.0, 1.0);
    let daylight = daylight * daylight * (3.0 - 2.0 * daylight);
    ambient.color = Color::srgb(0.56, 0.68, 0.82);
    // 35 cd/m² is a restrained sky contribution beside 127 klux direct sun;
    // the 0.02 cd/m² floor preserves a genuinely dark night side.
    ambient.brightness = 0.02 + daylight as f32 * 34.98;
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
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
    mut sun_query: Query<&mut Transform, With<SunLight>>,
) {
    let Some(sun_direction) = bound_planet
        .0
        .as_deref()
        .and_then(|name| sun_direction_for_bound_planet(&ephemeris_snapshot, name))
    else {
        return;
    };
    let sun_direction = sun_direction.as_vec3();

    for mut light_transform in sun_query.iter_mut() {
        *light_transform = Transform::from_xyz(0.0, 0.0, 0.0).looking_at(-sun_direction, Vec3::Y);
    }
}

/// Direction from the bound planet toward the Sun in the existing
/// planet-centered inertial axes. The input snapshot is SSB/ICRF; the reference
/// frame service performs the one explicit ICRF-to-solar-inertial conversion.
fn sun_direction_for_bound_planet(
    ephemeris_snapshot: &EphemerisSnapshot,
    bound_planet_name: &str,
) -> Option<bevy::math::DVec3> {
    let bound_body = NaifBodyId::for_catalog_name(bound_planet_name)?;
    let direction = ephemeris_snapshot
        .solar_inertial_relative_state(NaifBodyId::SUN, bound_body)?
        .position_m;
    let length = direction.length();
    (length.is_finite() && length > 0.0).then_some(direction / length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::ephemeris::{BodyState, TdbEpoch};
    use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
    use bevy::math::DVec3;

    #[test]
    fn rocket_sunlight_matches_the_shared_ephemeris_direction() {
        let epoch = TdbEpoch::j2000();
        let snapshot = EphemerisSnapshot::from_states(vec![
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
                position_m: -DVec3::X,
                velocity_mps: DVec3::ZERO,
            },
        ]);

        assert_eq!(
            sun_direction_for_bound_planet(&snapshot, "Earth"),
            Some(-DVec3::X)
        );
    }
}
