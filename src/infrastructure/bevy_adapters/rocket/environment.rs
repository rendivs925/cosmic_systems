use super::components::{RocketFlightConditions, RocketPhysicsState};
use super::planet::{RocketBoundPlanet, RocketBoundPlanetCloud};
use crate::application::solar_system_startup::SUN_ILLUMINANCE_AT_EARTH_LUX;
use crate::domain::services::ephemeris::NaifBodyId;
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use bevy::pbr::{DistanceFog, FogFalloff};
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
        .as_ref()
        .and_then(|id| sun_direction_for_bound_planet(&ephemeris_snapshot, id))
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
            // Terrain is assembled from rebased streamed patches. Do not let
            // directional shadow cascades introduce a planet-scale dark arc at
            // patch boundaries; ephemeris-driven direct lighting still models
            // the physical day/night cycle.
            shadows_enabled: false,
            ..default()
        },
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
    rocket_query: Query<(&RocketPhysicsState, &RocketFlightConditions)>,
    mut ambient: ResMut<AmbientLight>,
) {
    let Some(sun_direction) = bound_planet
        .0
        .as_ref()
        .and_then(|id| sun_direction_for_bound_planet(&ephemeris_snapshot, id))
    else {
        return;
    };
    let Some((rocket, conditions)) = rocket_query.iter().next() else {
        return;
    };
    let daylight = local_daylight(rocket.dynamics.position_m, sun_direction);
    let presentation = atmospheric_presentation(conditions, daylight);
    ambient.color = Color::srgb(0.56, 0.68, 0.82);
    // The atmosphere controls diffuse sky fill while a tiny floor preserves a
    // readable but genuinely dark vacuum/night presentation.
    ambient.brightness = 0.01 + presentation.ambient_unit * 34.99;
}

/// Tag component marking the sun directional light for day/night rotation.
#[derive(Component, Debug)]
pub struct SunLight;

/// Initialize rocket rendering with space-black until the first atmosphere
/// update evaluates the rocket's authoritative altitude and solar geometry.
pub fn setup_rocket_sky_color(mut clear_color: ResMut<ClearColor>) {
    *clear_color = ClearColor(Color::srgb(0.002, 0.002, 0.006));
}

/// Approximate local atmospheric presentation from the same flight conditions
/// and ephemeris Sun as terrain lighting. It fades to space by 100 km and does
/// not alter any atmospheric physics or the solar direction.
pub fn update_rocket_sky_color(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
    rocket_query: Query<(&RocketPhysicsState, &RocketFlightConditions)>,
    mut clear_color: ResMut<ClearColor>,
    mut fog_query: Query<&mut DistanceFog, With<Camera3d>>,
    cloud_query: Query<&MeshMaterial3d<StandardMaterial>, With<RocketBoundPlanetCloud>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(sun_direction) = bound_planet
        .0
        .as_ref()
        .and_then(|id| sun_direction_for_bound_planet(&ephemeris_snapshot, id))
    else {
        return;
    };
    let Some((rocket, conditions)) = rocket_query.iter().next() else {
        return;
    };
    let daylight = local_daylight(rocket.dynamics.position_m, sun_direction);
    let presentation = atmospheric_presentation(conditions, daylight);
    let sky = presentation.sky_unit;
    *clear_color = ClearColor(Color::srgb(
        0.002 + 0.3 * sky,
        0.002 + 0.52 * sky,
        0.006 + 0.79 * sky,
    ));

    for mut fog in fog_query.iter_mut() {
        fog.color = Color::srgba(0.52, 0.68, 0.9, sky);
        fog.falloff = FogFalloff::from_visibility(presentation.fog_visibility_m);
    }
    for material_handle in &cloud_query {
        if let Some(material) = materials.get_mut(&material_handle.0) {
            material.base_color = material
                .base_color
                .with_alpha(presentation.cloud_opacity_unit);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AtmosphericPresentation {
    sky_unit: f32,
    ambient_unit: f32,
    cloud_opacity_unit: f32,
    fog_visibility_m: f32,
}

/// Smooth render controls from the authoritative fixed-tick atmospheric sample.
/// The constants are visual thresholds only and never feed atmospheric physics.
fn atmospheric_presentation(
    conditions: &RocketFlightConditions,
    daylight: f32,
) -> AtmosphericPresentation {
    let density_unit = (conditions.density_kg_m3.max(0.0) / 1.225).clamp(0.0, 1.0) as f32;
    let altitude_unit = (conditions.altitude_m.max(0.0) / 100_000.0).clamp(0.0, 1.0) as f32;
    let atmosphere_unit = (density_unit * (1.0 - altitude_unit * 0.35)).clamp(0.0, 1.0);
    let daylight = daylight.clamp(0.0, 1.0);
    AtmosphericPresentation {
        sky_unit: daylight * atmosphere_unit,
        ambient_unit: daylight * atmosphere_unit,
        cloud_opacity_unit: atmosphere_unit * (0.18 + daylight * 0.62),
        fog_visibility_m: 25_000.0 / atmosphere_unit.max(0.02),
    }
}

fn local_daylight(rocket_position_m: bevy::math::DVec3, sun_direction: bevy::math::DVec3) -> f32 {
    let surface_normal = rocket_position_m.normalize_or_zero();
    let daylight = ((surface_normal.dot(sun_direction) + 0.12) / 0.32).clamp(0.0, 1.0);
    (daylight * daylight * (3.0 - 2.0 * daylight)) as f32
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
        .as_ref()
        .and_then(|id| sun_direction_for_bound_planet(&ephemeris_snapshot, id))
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
    bound_planet_id: &CelestialBodyId,
) -> Option<bevy::math::DVec3> {
    let bound_body = NaifBodyId::for_catalog_name(bound_planet_id.as_str())?;
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
            sun_direction_for_bound_planet(&snapshot, &CelestialBodyId::earth(),),
            Some(-DVec3::X)
        );
    }

    #[test]
    fn local_sky_presentation_follows_the_ephemeris_day_night_boundary() {
        assert_eq!(local_daylight(DVec3::X, DVec3::X), 1.0);
        assert_eq!(local_daylight(DVec3::X, -DVec3::X), 0.0);
    }

    #[test]
    fn atmospheric_presentation_continuously_fades_to_vacuum() {
        let dense = RocketFlightConditions::from_sample(
            crate::domain::services::atmosphere::FlightConditions {
                altitude_m: 0.0,
                density_kg_m3: 1.225,
                ..default()
            },
        );
        let thin = RocketFlightConditions::from_sample(
            crate::domain::services::atmosphere::FlightConditions {
                altitude_m: 80_000.0,
                density_kg_m3: 0.01,
                ..default()
            },
        );
        let dense = atmospheric_presentation(&dense, 1.0);
        let thin = atmospheric_presentation(&thin, 1.0);

        assert!((0.0..=1.0).contains(&dense.sky_unit));
        assert!(thin.sky_unit < dense.sky_unit);
        assert!(thin.cloud_opacity_unit < dense.cloud_opacity_unit);
        assert!(thin.fog_visibility_m > dense.fog_visibility_m);
    }
}
