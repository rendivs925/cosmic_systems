use super::components::{RocketFlightConditions, RocketPhysicsState};
use super::planet::{RocketBoundPlanet, RocketBoundPlanetCloud};
use crate::application::solar_system_startup::SUN_ILLUMINANCE_AT_EARTH_LUX;
use crate::domain::services::atmosphere::SEA_LEVEL_DENSITY_KG_M3;
use crate::domain::services::ephemeris::NaifBodyId;
use crate::domain::units::AU_IN_METERS;
use crate::domain::value_objects::atmospheric_optics::AtmosphericOptics;
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::infrastructure::bevy_adapters::entity_components::PlanetComponent;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

/// Rocket shadow cascades are sized to the near-flight scale: sharp vehicle and
/// pad shadows out to a few hundred meters, useful terrain shadows out to the
/// visible horizon range, well inside the flight camera's far plane.
const ROCKET_SHADOW_CASCADES: usize = 4;
const ROCKET_SHADOW_MIN_DISTANCE_M: f32 = 1.0;
const ROCKET_SHADOW_FIRST_CASCADE_FAR_M: f32 = 250.0;
const ROCKET_SHADOW_MAX_DISTANCE_M: f32 = 20_000.0;

/// Ratio of mean sky radiance to direct solar illuminance for a clear Earth
/// atmosphere. A single-scattering integration of the shared optics gives
/// roughly 1% at the zenith and 13% at the horizon, so the hemisphere average
/// is a few percent; that is the ambient fill used for shadows and the night
/// side. This keeps ambient and direct lighting in one radiometric scale.
const SKY_AMBIENT_FRACTION: f32 = 0.025;

/// Spawns a directional sunlight source. The Sun's inertial direction comes
/// from the shared ephemeris; the rotating planet moves terrain through that
/// fixed direction to produce the physical day/night cycle.
pub fn setup_rocket_sun_light(
    mut commands: Commands,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
) {
    let Some(sun) = bound_planet
        .0
        .as_ref()
        .and_then(|id| bound_planet_sun(&ephemeris_snapshot, id))
    else {
        bevy::log::error!(
            "cannot initialize rocket sunlight without a bound-planet ephemeris state"
        );
        return;
    };
    let sun_direction = sun.direction.as_vec3();

    commands.spawn((
        bevy::light::DirectionalLight {
            illuminance: solar_illuminance_lux(sun.distance_m),
            color: Color::srgb(1.0, 1.0, 0.98),
            // Directional shadow cascades are configured with the light and the
            // enclosing far-field globe is excluded as a caster, so terrain
            // self-shadowing does not produce a planet-scale dark arc.
            shadows_enabled: true,
            ..default()
        },
        // Light travels along local -Z toward the scene; orient it so the Sun
        // appears in its ephemeris direction.
        Transform::from_xyz(0.0, 0.0, 0.0).looking_at(-sun_direction, Vec3::Y),
        bevy::light::CascadeShadowConfig::from(bevy::light::CascadeShadowConfigBuilder {
            num_cascades: ROCKET_SHADOW_CASCADES,
            minimum_distance: ROCKET_SHADOW_MIN_DISTANCE_M,
            first_cascade_far_bound: ROCKET_SHADOW_FIRST_CASCADE_FAR_M,
            maximum_distance: ROCKET_SHADOW_MAX_DISTANCE_M,
            ..default()
        }),
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
    let Some(sun) = bound_planet
        .0
        .as_ref()
        .and_then(|id| bound_planet_sun(&ephemeris_snapshot, id))
    else {
        return;
    };
    let Some((rocket, conditions)) = rocket_query.iter().next() else {
        return;
    };
    let daylight = twilight_daylight_unit(solar_altitude_rad(
        rocket.dynamics.position_m,
        sun.direction,
    ));
    let presentation = atmospheric_presentation(conditions, daylight);
    ambient.color = Color::srgb(0.56, 0.68, 0.82);
    // `AmbientLight::brightness` is a sky radiance in cd/m^2, so it must scale
    // with the same solar illuminance that drives the directional light. A
    // 127 klx sun needs a few percent of that as sky fill (~10% of the sunlit
    // surface radiance); the previous fixed 35 cd/m^2 was ~100x too dark and
    // crushed every shadow to black. A tiny floor preserves a genuinely dark
    // vacuum/night presentation.
    let sky_ambient_cd_m2 = solar_illuminance_lux(sun.distance_m) * SKY_AMBIENT_FRACTION;
    ambient.brightness = 0.01 + sky_ambient_cd_m2 * presentation.ambient_unit;
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
/// and ephemeris Sun as terrain lighting. Aerial perspective is derived from the
/// bound body's authoritative atmospheric optics, so distant terrain recedes
/// through the same Rayleigh/Mie/ozone model as the sky. It does not alter any
/// atmospheric physics or the solar direction.
#[expect(
    clippy::too_many_arguments,
    reason = "This presentation system synchronizes independent camera, atmosphere, and cloud state."
)]
pub fn update_rocket_sky_color(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
    rocket_query: Query<(&RocketPhysicsState, &RocketFlightConditions)>,
    planet_query: Query<&PlanetComponent>,
    mut clear_color: ResMut<ClearColor>,
    mut fog_query: Query<&mut DistanceFog, With<Camera3d>>,
    cloud_query: Query<&MeshMaterial3d<StandardMaterial>, With<RocketBoundPlanetCloud>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(sun) = bound_planet
        .0
        .as_ref()
        .and_then(|id| bound_planet_sun(&ephemeris_snapshot, id))
    else {
        return;
    };
    let Some((rocket, conditions)) = rocket_query.iter().next() else {
        return;
    };
    let daylight = twilight_daylight_unit(solar_altitude_rad(
        rocket.dynamics.position_m,
        sun.direction,
    ));
    let presentation = atmospheric_presentation(conditions, daylight);
    let sky = presentation.sky_unit;
    *clear_color = ClearColor(Color::srgb(
        0.002 + 0.3 * sky,
        0.002 + 0.52 * sky,
        0.006 + 0.79 * sky,
    ));

    let optics = bound_planet
        .0
        .as_ref()
        .and_then(|id| {
            planet_query
                .iter()
                .find(|planet| planet.matches_body(id))
                .map(|planet| (id.as_str(), planet.domain_planet.radius_km * 1_000.0))
        })
        .and_then(|(name, radius_m)| AtmosphericOptics::for_body(name, radius_m));

    for mut fog in fog_query.iter_mut() {
        apply_aerial_perspective(&mut fog, optics.as_ref(), conditions.altitude_m, daylight);
    }
    for material_handle in &cloud_query {
        if let Some(material) = materials.get_mut(&material_handle.0) {
            material.base_color = material
                .base_color
                .with_alpha(presentation.cloud_opacity_unit);
        }
    }
}

/// Configure per-fragment aerial perspective from the body's atmospheric
/// optics. Bevy's atmospheric fog applies per-channel optical depth, so the
/// transmittance and sky-coloured in-scattering match the modelled atmosphere.
fn apply_aerial_perspective(
    fog: &mut DistanceFog,
    optics: Option<&AtmosphericOptics>,
    altitude_m: f64,
    daylight: f32,
) {
    let daylight = daylight.clamp(0.0, 1.0);
    // Rayleigh-dominated airlight: blue-biased base with a warm sun lobe for Mie
    // forward scattering. Bevy adds `pow(NdotL, exponent) * light.color *
    // exposure` on top of the base, and `light.color` is premultiplied by the
    // solar illuminance, so the lobe alpha must stay small or the haze saturates
    // to white. The base color alone already matches the sky's radiance scale.
    fog.color = Color::srgba(
        0.45 * daylight + 0.002,
        0.55 * daylight + 0.002,
        0.72 * daylight + 0.006,
        daylight,
    );
    fog.directional_light_color = Color::srgba(1.0, 0.94, 0.82, 0.04 * daylight);
    fog.directional_light_exponent = 16.0;
    fog.falloff = match optics {
        Some(optics) => {
            let altitude_m = altitude_m.max(0.0) as f32;
            FogFalloff::Atmospheric {
                extinction: Vec3::from(optics.extinction_per_m(altitude_m)),
                inscattering: Vec3::from(optics.scattering_per_m(altitude_m)),
            }
        }
        None => FogFalloff::from_visibility(25_000.0),
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AtmosphericPresentation {
    sky_unit: f32,
    ambient_unit: f32,
    cloud_opacity_unit: f32,
}

/// Smooth render controls from the authoritative fixed-tick atmospheric sample.
/// The constants are visual thresholds only and never feed atmospheric physics.
fn atmospheric_presentation(
    conditions: &RocketFlightConditions,
    daylight: f32,
) -> AtmosphericPresentation {
    let density_unit =
        (conditions.density_kg_m3.max(0.0) / SEA_LEVEL_DENSITY_KG_M3).clamp(0.0, 1.0) as f32;
    let altitude_unit = (conditions.altitude_m.max(0.0) / 100_000.0).clamp(0.0, 1.0) as f32;
    let atmosphere_unit = (density_unit * (1.0 - altitude_unit * 0.35)).clamp(0.0, 1.0);
    let daylight = daylight.clamp(0.0, 1.0);
    AtmosphericPresentation {
        sky_unit: daylight * atmosphere_unit,
        ambient_unit: daylight * atmosphere_unit,
        cloud_opacity_unit: atmosphere_unit * (0.18 + daylight * 0.62),
    }
}

/// Solar altitude of the observer above its own local horizon, in radians.
/// Positive is day, negative is night. The observer's planet-centered position
/// normal is the local vertical in the planet-centered inertial frame.
fn solar_altitude_rad(
    observer_position_m: bevy::math::DVec3,
    sun_direction: bevy::math::DVec3,
) -> f64 {
    let local_up = observer_position_m.normalize_or_zero();
    local_up.dot(sun_direction).clamp(-1.0, 1.0).asin()
}

/// Fraction of full daylight used only for non-direct sky, ambient, and cloud
/// presentation. It follows the civil/nautical/astronomical twilight bands and
/// reaches zero at -18 degrees solar altitude. Direct PBR lighting terminates
/// geometrically and is unaffected by this curve.
fn twilight_daylight_unit(solar_altitude_rad: f64) -> f32 {
    let astronomical_twilight_rad = -18.0_f64.to_radians();
    let t = ((solar_altitude_rad - astronomical_twilight_rad) / -astronomical_twilight_rad)
        .clamp(0.0, 1.0);
    (t * t * (3.0 - 2.0 * t)) as f32
}

/// Updates rocket-mode sunlight from the same ephemeris state used by the Sun
/// proxy. Planet and terrain rotation, rather than an artificial light orbit,
/// produces the local day/night cycle. Direct illuminance tracks the true
/// planet-Sun distance so bodies other than the reference distance receive the
/// inverse-square-correct radiance.
pub fn update_sun_day_night_cycle(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
    mut sun_query: Query<(&mut Transform, &mut bevy::light::DirectionalLight), With<SunLight>>,
) {
    let Some(sun) = bound_planet
        .0
        .as_ref()
        .and_then(|id| bound_planet_sun(&ephemeris_snapshot, id))
    else {
        return;
    };
    let sun_direction = sun.direction.as_vec3();
    let illuminance = solar_illuminance_lux(sun.distance_m);

    for (mut light_transform, mut light) in sun_query.iter_mut() {
        *light_transform = Transform::from_xyz(0.0, 0.0, 0.0).looking_at(-sun_direction, Vec3::Y);
        light.illuminance = illuminance;
    }
}

/// Direct normal solar illuminance at a planet-Sun distance, scaled from the
/// single calibrated reference value by the inverse square of the distance.
pub(super) fn solar_illuminance_lux(planet_sun_distance_m: f64) -> f32 {
    if !planet_sun_distance_m.is_finite() || planet_sun_distance_m <= 0.0 {
        return 0.0;
    }
    let distance_ratio = AU_IN_METERS / planet_sun_distance_m;
    (SUN_ILLUMINANCE_AT_EARTH_LUX as f64 * distance_ratio * distance_ratio) as f32
}

/// Bound planet Sun geometry in the existing planet-centered inertial axes: the
/// unit direction from the planet toward the Sun and the planet-Sun range in
/// meters. The input snapshot is SSB/ICRF; the reference frame service performs
/// the one explicit ICRF-to-solar-inertial conversion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BoundPlanetSun {
    pub(super) direction: bevy::math::DVec3,
    pub(super) distance_m: f64,
}

pub(super) fn bound_planet_sun(
    ephemeris_snapshot: &EphemerisSnapshot,
    bound_planet_id: &CelestialBodyId,
) -> Option<BoundPlanetSun> {
    let bound_body = NaifBodyId::for_catalog_name(bound_planet_id.as_str())?;
    let position_m = ephemeris_snapshot
        .solar_inertial_relative_state(NaifBodyId::SUN, bound_body)?
        .position_m;
    let distance_m = position_m.length();
    (distance_m.is_finite() && distance_m > 0.0).then_some(BoundPlanetSun {
        direction: position_m / distance_m,
        distance_m,
    })
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
            bound_planet_sun(&snapshot, &CelestialBodyId::earth()).map(|sun| sun.direction),
            Some(-DVec3::X)
        );
    }

    #[test]
    fn direct_illuminance_follows_the_inverse_square_law() {
        let reference = solar_illuminance_lux(AU_IN_METERS);
        assert!((reference as f64 - SUN_ILLUMINANCE_AT_EARTH_LUX as f64).abs() < 0.1);

        let twice_as_far = solar_illuminance_lux(2.0 * AU_IN_METERS);
        assert!((twice_as_far as f64 - reference as f64 / 4.0).abs() < 0.1);
        assert!(solar_illuminance_lux(1.52 * AU_IN_METERS) < reference);
    }

    #[test]
    fn solar_altitude_is_positive_by_day_and_negative_by_night() {
        let day = solar_altitude_rad(DVec3::X, DVec3::X);
        let night = solar_altitude_rad(DVec3::X, -DVec3::X);
        let horizon = solar_altitude_rad(DVec3::X, DVec3::Y);

        assert!((day - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!((night + std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!(horizon.abs() < 1e-12);
    }

    #[test]
    fn twilight_daylight_reaches_night_beyond_astronomical_twilight() {
        assert_eq!(twilight_daylight_unit(0.0), 1.0);
        assert_eq!(twilight_daylight_unit(-18.0_f64.to_radians()), 0.0);
        assert!(twilight_daylight_unit(-6.0_f64.to_radians()) < 1.0);
        assert!(twilight_daylight_unit(0.5) == 1.0);
    }

    #[test]
    fn atmospheric_presentation_continuously_fades_to_vacuum() {
        let dense = RocketFlightConditions::from_sample(
            crate::domain::services::atmosphere::FlightConditions {
                altitude_m: 0.0,
                density_kg_m3: SEA_LEVEL_DENSITY_KG_M3,
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
    }

    #[test]
    fn aerial_perspective_uses_per_channel_optics_and_sun_glow() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        let mut fog = DistanceFog::default();
        apply_aerial_perspective(&mut fog, Some(&optics), 0.0, 1.0);

        match fog.falloff {
            FogFalloff::Atmospheric {
                extinction,
                inscattering,
            } => {
                assert!(extinction.z > extinction.x);
                assert!(inscattering.z > inscattering.x);
                assert!(inscattering.length() > 0.0);
            }
            other => panic!("expected atmospheric falloff, got {other:?}"),
        }
        assert!(fog.directional_light_color.to_srgba().alpha > 0.0);
    }

    #[test]
    fn vacuum_aerial_perspective_falls_back_without_sun_glow() {
        let mut fog = DistanceFog::default();
        apply_aerial_perspective(&mut fog, None, 0.0, 0.0);

        assert!(matches!(fog.falloff, FogFalloff::Exponential { .. }));
        assert_eq!(fog.directional_light_color.to_srgba().alpha, 0.0);
    }
}
