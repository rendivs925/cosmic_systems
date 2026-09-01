use crate::application::solar_system_startup::SUN_ILLUMINANCE_AT_EARTH_LUX;
use crate::domain::services::ephemeris::NaifBodyId;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::rocket_planet::RocketBoundPlanet;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;

/// Spawns a directional sunlight source. The Sun's inertial direction comes
/// from the shared ephemeris; the rotating planet moves terrain through that
/// fixed direction to produce the physical day/night cycle.
pub fn setup_rocket_sun_light(
    mut commands: Commands,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
) {
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

    // The default DE440 epoch is civil twilight at KSC. This presentation-only
    // neutral fill keeps the launch terrain and vehicle inspectable without
    // tinting source-derived land and water albedo blue. Direct light still
    // follows the authoritative Sun direction and casts shadows.
    commands.insert_resource(bevy::light::AmbientLight {
        color: Color::srgb(0.65, 0.65, 0.62),
        brightness: 6_000.0,
        ..default()
    });

    commands.spawn((
        bevy::light::DirectionalLight {
            illuminance: SUN_ILLUMINANCE_AT_EARTH_LUX,
            color: Color::srgb(1.0, 1.0, 0.98),
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
