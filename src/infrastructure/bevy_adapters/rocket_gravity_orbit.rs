//! Gravity and orbital-state adapters for the fixed rocket flight pipeline.

use crate::components::rocket::{
    GravityAcceleration, OrbitalElements, RocketMissionState, RocketPhysicsState,
    RocketPlanetBinding,
};
use crate::domain::services::ephemeris::NaifBodyId;
use crate::domain::services::gravity::{
    differential_gravitational_acceleration_from_mu, earth_j2_acceleration,
    gravitational_acceleration_from_mu, ForceModelConfig, ForceModelTerm,
};
use crate::domain::services::physics_orbital::orbital_elements_from_state_in_reference_frame;
use crate::domain::services::reference_frames::{
    planet_equatorial_reference_x_axis, planet_inertial_spin_axis,
};
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use bevy::math::DVec3;
use bevy::prelude::{Query, Res, Resource};

/// Read-only ECS composition boundary for the pure force-model configuration.
/// The selected tier is installed at rocket-mode startup and never mutated by
/// the fixed simulation pipeline.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActiveForceModel(pub ForceModelConfig);

/// Compute the authoritative gravitational acceleration from each rocket's
/// typed dominant-body binding.
pub fn update_rocket_gravity(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    force_model: Res<ActiveForceModel>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &mut GravityAcceleration,
    )>,
) {
    for (binding, rocket, mut gravity) in rocket_query.iter_mut() {
        if planet_query
            .iter()
            .all(|planet| !planet.matches_body(&binding.planet_name))
        {
            gravity.value = DVec3::ZERO;
            continue;
        }

        let Some(bound_body) = NaifBodyId::for_catalog_name(binding.planet_name.as_str()) else {
            gravity.value = DVec3::ZERO;
            continue;
        };
        let Some(bound_mu_m3_s2) = ephemeris_snapshot.gravitational_parameter_m3_s2(bound_body)
        else {
            gravity.value = DVec3::ZERO;
            continue;
        };
        let mut acceleration = gravitational_acceleration_from_mu(
            bound_mu_m3_s2,
            rocket.dynamics.position_m,
            DVec3::ZERO,
        );

        if bound_body == NaifBodyId::EARTH
            && force_model
                .0
                .active_terms()
                .contains(&ForceModelTerm::EarthJ2)
        {
            if let (Some(orientation), Some(j2_model)) = (
                ephemeris_snapshot.orientation(NaifBodyId::EARTH),
                ephemeris_snapshot.earth_j2_model(),
            ) {
                acceleration += earth_j2_acceleration(
                    bound_mu_m3_s2,
                    rocket.dynamics.position_m,
                    planet_inertial_spin_axis(orientation),
                    j2_model,
                );
            }
        }

        // The rocket state is planet-centered inertial, so every perturbing
        // body contributes only its acceleration relative to the bound planet.
        // Adding its full barycentric acceleration would double-count the
        // frame origin's own acceleration and eject local flights incorrectly.
        for (term, perturbing_body) in [
            (ForceModelTerm::LunarThirdBody, NaifBodyId::MOON),
            (ForceModelTerm::SolarThirdBody, NaifBodyId::SUN),
        ] {
            if !force_model.0.active_terms().contains(&term) {
                continue;
            }
            let relative_state =
                ephemeris_snapshot.solar_inertial_relative_state(perturbing_body, bound_body);
            let perturbing_mu_m3_s2 =
                ephemeris_snapshot.gravitational_parameter_m3_s2(perturbing_body);
            if let (Some(relative_state), Some(perturbing_mu_m3_s2)) =
                (relative_state, perturbing_mu_m3_s2)
            {
                acceleration += differential_gravitational_acceleration_from_mu(
                    perturbing_mu_m3_s2,
                    rocket.dynamics.position_m,
                    DVec3::ZERO,
                    relative_state.position_m,
                );
            }
        }

        gravity.value = acceleration;
    }
}

/// Compute post-integration orbital elements for telemetry and next-tick
/// guidance from the authoritative planet-centered inertial state.
pub fn update_orbital_elements(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketMissionState,
        &mut OrbitalElements,
    )>,
) {
    for (binding, rocket, mission, mut elements) in rocket_query.iter_mut() {
        if *mission == RocketMissionState::PreLaunch {
            *elements = OrbitalElements::default();
            continue;
        }
        let Some(planet) = planet_query
            .iter()
            .find(|planet| planet.matches_body(&binding.planet_name))
        else {
            continue;
        };
        let Some(orientation) =
            ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
        else {
            continue;
        };
        let Some(mu_m3_s2) =
            ephemeris_snapshot.gravitational_parameter_for_catalog_body(&planet.domain_planet.name)
        else {
            continue;
        };

        let state = orbital_elements_from_state_in_reference_frame(
            rocket.dynamics.position_m,
            rocket.dynamics.velocity_mps,
            mu_m3_s2,
            planet_inertial_spin_axis(orientation),
            planet_equatorial_reference_x_axis(orientation),
        );
        elements.semi_major_axis_m = state.semi_major_axis_m;
        elements.eccentricity = state.eccentricity;
        elements.inclination_rad = state.inclination_rad;
        elements.longitude_ascending_node_rad = state.longitude_ascending_node_rad;
        elements.argument_of_periapsis_rad = state.argument_of_periapsis_rad;
        elements.true_anomaly_rad = state.true_anomaly_rad;
        elements.mean_anomaly_rad = state.mean_anomaly_rad;
        elements.orbital_period_s = state.orbital_period_s;
        elements.apoapsis_m = state.apoapsis_m;
        elements.periapsis_m = state.periapsis_m;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::ephemeris::{BodyState, TdbEpoch};
    use crate::domain::services::gravity::{
        differential_gravitational_acceleration, earth_j2_acceleration, gravitational_acceleration,
        gravitational_parameter, EarthJ2GravityModel, ForceModelTier,
    };
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
    use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
    use bevy::math::{DMat3, DQuat};
    use bevy::prelude::{default, App, Update};

    fn planet_component(name: &str) -> PlanetComponent {
        PlanetComponent {
            domain_planet: PlanetFactory::create_by_name(name).unwrap(),
            material: default(),
            has_texture: false,
            base_reflectance: 0.0,
            base_roughness: 0.0,
        }
    }

    #[test]
    fn primary_bound_rocket_adds_solar_differential_gravity() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let sun = PlanetFactory::create_by_name("Sun").unwrap();
        let rocket_position_m = DVec3::X * (earth.radius_km as f64 * 1_000.0 + 400_000.0);
        let mut app = App::new();
        let epoch = TdbEpoch::j2000();
        let earth_state = BodyState {
            target: NaifBodyId::EARTH,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: DVec3::ZERO,
            velocity_mps: DVec3::ZERO,
        };
        let sun_state = BodyState {
            target: NaifBodyId::SUN,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: -DVec3::X * 149_597_870_700.0,
            velocity_mps: DVec3::ZERO,
        };
        app.insert_resource(
            EphemerisSnapshot::from_states_with_gravitational_parameters(
                vec![earth_state, sun_state],
                vec![
                    (NaifBodyId::EARTH, gravitational_parameter(earth.mass_kg)),
                    (NaifBodyId::SUN, gravitational_parameter(sun.mass_kg)),
                ],
            ),
        );
        app.insert_resource(ActiveForceModel::default());
        app.world_mut().spawn(planet_component("Earth"));
        app.world_mut().spawn(planet_component("Sun"));
        let rocket = app
            .world_mut()
            .spawn((
                RocketPlanetBinding {
                    planet_name: CelestialBodyId::earth(),
                },
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        rocket_position_m,
                        DVec3::ZERO,
                        DQuat::IDENTITY,
                        1_000.0,
                        DMat3::IDENTITY,
                        DVec3::ZERO,
                    ),
                },
                GravityAcceleration::default(),
            ))
            .id();
        app.add_systems(Update, update_rocket_gravity);

        app.update();

        let expected = gravitational_acceleration(earth.mass_kg, rocket_position_m, DVec3::ZERO)
            + differential_gravitational_acceleration(
                sun.mass_kg,
                rocket_position_m,
                DVec3::ZERO,
                sun_state.position_m - earth_state.position_m,
            );
        let actual = app
            .world()
            .get::<GravityAcceleration>(rocket)
            .unwrap()
            .value;

        assert_eq!(actual, expected);
        assert!(
            (actual - gravitational_acceleration(earth.mass_kg, rocket_position_m, DVec3::ZERO))
                .length()
                > 1.0e-7
        );
    }

    #[test]
    fn earth_moon_sun_tier_adds_same_epoch_lunar_and_solar_differential_gravity() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let moon = PlanetFactory::create_by_name("Moon").unwrap();
        let sun = PlanetFactory::create_by_name("Sun").unwrap();
        let rocket_position_m = DVec3::X * (earth.radius_km as f64 * 1_000.0 + 400_000.0);
        let epoch = TdbEpoch::j2000();
        let earth_state = BodyState {
            target: NaifBodyId::EARTH,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: DVec3::ZERO,
            velocity_mps: DVec3::ZERO,
        };
        let moon_state = BodyState {
            target: NaifBodyId::MOON,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: DVec3::X * 384_400_000.0,
            velocity_mps: DVec3::ZERO,
        };
        let sun_state = BodyState {
            target: NaifBodyId::SUN,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: -DVec3::X * 149_597_870_700.0,
            velocity_mps: DVec3::ZERO,
        };
        let mut app = App::new();
        app.insert_resource(
            EphemerisSnapshot::from_states_with_gravitational_parameters(
                vec![earth_state, moon_state, sun_state],
                vec![
                    (NaifBodyId::EARTH, gravitational_parameter(earth.mass_kg)),
                    (NaifBodyId::MOON, gravitational_parameter(moon.mass_kg)),
                    (NaifBodyId::SUN, gravitational_parameter(sun.mass_kg)),
                ],
            ),
        );
        app.insert_resource(ActiveForceModel(ForceModelConfig::new(
            ForceModelTier::EarthMoonSun,
        )));
        app.world_mut().spawn(planet_component("Earth"));
        let rocket = app
            .world_mut()
            .spawn((
                RocketPlanetBinding {
                    planet_name: CelestialBodyId::earth(),
                },
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        rocket_position_m,
                        DVec3::ZERO,
                        DQuat::IDENTITY,
                        1_000.0,
                        DMat3::IDENTITY,
                        DVec3::ZERO,
                    ),
                },
                GravityAcceleration::default(),
            ))
            .id();
        app.add_systems(Update, update_rocket_gravity);

        app.update();

        let expected = gravitational_acceleration(earth.mass_kg, rocket_position_m, DVec3::ZERO)
            + differential_gravitational_acceleration(
                moon.mass_kg,
                rocket_position_m,
                DVec3::ZERO,
                moon_state.position_m - earth_state.position_m,
            )
            + differential_gravitational_acceleration(
                sun.mass_kg,
                rocket_position_m,
                DVec3::ZERO,
                sun_state.position_m - earth_state.position_m,
            );
        let actual = app
            .world()
            .get::<GravityAcceleration>(rocket)
            .unwrap()
            .value;

        assert_eq!(actual, expected);
    }

    #[test]
    fn planet_sun_tier_excludes_lunar_differential_gravity() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let moon = PlanetFactory::create_by_name("Moon").unwrap();
        let sun = PlanetFactory::create_by_name("Sun").unwrap();
        let rocket_position_m = DVec3::X * (earth.radius_km as f64 * 1_000.0 + 400_000.0);
        let epoch = TdbEpoch::j2000();
        let earth_state = BodyState {
            target: NaifBodyId::EARTH,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: DVec3::ZERO,
            velocity_mps: DVec3::ZERO,
        };
        let moon_state = BodyState {
            target: NaifBodyId::MOON,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: DVec3::X * 384_400_000.0,
            velocity_mps: DVec3::ZERO,
        };
        let sun_state = BodyState {
            target: NaifBodyId::SUN,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: -DVec3::X * 149_597_870_700.0,
            velocity_mps: DVec3::ZERO,
        };
        let mut app = App::new();
        app.insert_resource(
            EphemerisSnapshot::from_states_with_gravitational_parameters(
                vec![earth_state, moon_state, sun_state],
                vec![
                    (NaifBodyId::EARTH, gravitational_parameter(earth.mass_kg)),
                    (NaifBodyId::MOON, gravitational_parameter(moon.mass_kg)),
                    (NaifBodyId::SUN, gravitational_parameter(sun.mass_kg)),
                ],
            ),
        );
        app.insert_resource(ActiveForceModel::default());
        app.world_mut().spawn(planet_component("Earth"));
        let rocket = app
            .world_mut()
            .spawn((
                RocketPlanetBinding {
                    planet_name: CelestialBodyId::earth(),
                },
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        rocket_position_m,
                        DVec3::ZERO,
                        DQuat::IDENTITY,
                        1_000.0,
                        DMat3::IDENTITY,
                        DVec3::ZERO,
                    ),
                },
                GravityAcceleration::default(),
            ))
            .id();
        app.add_systems(Update, update_rocket_gravity);

        app.update();

        let expected = gravitational_acceleration(earth.mass_kg, rocket_position_m, DVec3::ZERO)
            + differential_gravitational_acceleration(
                sun.mass_kg,
                rocket_position_m,
                DVec3::ZERO,
                sun_state.position_m - earth_state.position_m,
            );
        let actual = app
            .world()
            .get::<GravityAcceleration>(rocket)
            .unwrap()
            .value;

        assert_eq!(actual, expected);
        assert!(
            differential_gravitational_acceleration(
                moon.mass_kg,
                rocket_position_m,
                DVec3::ZERO,
                moon_state.position_m - earth_state.position_m,
            )
            .length()
                > 1.0e-8
        );
    }

    #[test]
    fn earth_j2_tier_uses_the_snapshot_orientation_and_model() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let radius_m = earth.radius_km as f64 * 1_000.0 + 400_000.0;
        let orientation = crate::domain::services::body_orientation::BodyOrientation::from_kernel(
            NaifBodyId::EARTH,
            TdbEpoch::j2000(),
            "test-orientation".to_string(),
            DQuat::IDENTITY,
            DVec3::Z,
        );
        let j2_model = EarthJ2GravityModel {
            model_id: "test".to_string(),
            reference_radius_m: 6_378_136.3,
            j2: 1.082_626_173_852_222_7e-3,
        };
        let mut app = App::new();
        app.insert_resource(
            EphemerisSnapshot::from_states_orientations_and_gravitational_parameters(
                Vec::new(),
                vec![orientation.clone()],
                vec![(NaifBodyId::EARTH, gravitational_parameter(earth.mass_kg))],
            )
            .with_earth_j2_model(j2_model.clone()),
        );
        app.insert_resource(ActiveForceModel(ForceModelConfig::new(
            ForceModelTier::EarthJ2,
        )));
        app.world_mut().spawn(planet_component("Earth"));
        let rocket = app
            .world_mut()
            .spawn((
                RocketPlanetBinding {
                    planet_name: CelestialBodyId::earth(),
                },
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        DVec3::X * radius_m,
                        DVec3::ZERO,
                        DQuat::IDENTITY,
                        1_000.0,
                        DMat3::IDENTITY,
                        DVec3::ZERO,
                    ),
                },
                GravityAcceleration::default(),
            ))
            .id();
        app.add_systems(Update, update_rocket_gravity);

        app.update();

        let mu_m3_s2 = gravitational_parameter(earth.mass_kg);
        let expected =
            gravitational_acceleration_from_mu(mu_m3_s2, DVec3::X * radius_m, DVec3::ZERO)
                + earth_j2_acceleration(mu_m3_s2, DVec3::X * radius_m, DVec3::Z, &j2_model);
        let actual = app
            .world()
            .get::<GravityAcceleration>(rocket)
            .unwrap()
            .value;
        assert_eq!(actual, expected);
    }
}
