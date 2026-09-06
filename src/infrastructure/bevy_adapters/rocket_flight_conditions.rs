//! Atmosphere and air-relative fixed-tick flight-condition adapters.

use crate::components::rocket::{RocketFlightConditions, RocketPhysicsState, RocketPlanetBinding};
use crate::domain::services::atmosphere::FlightConditions;
use crate::domain::services::reference_frames::surface_velocity_in_planet_inertial;
use crate::infrastructure::bevy_adapters::components::{PlanetAtmosphere, PlanetComponent};
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use bevy::prelude::{Query, Res};

/// Refresh each vehicle's sole atmosphere sample and air-relative motion at
/// the first fixed stage. All subsequent flight consumers read this component.
pub fn refresh_flight_conditions(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    planet_query: Query<(&PlanetComponent, &PlanetAtmosphere)>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &mut RocketFlightConditions,
    )>,
) {
    for (binding, rocket, mut conditions) in rocket_query.iter_mut() {
        let Some((planet, atmosphere)) = planet_query
            .iter()
            .find(|(planet, _)| planet.matches_body(&binding.planet_name))
        else {
            continue;
        };
        let radius_m = planet.domain_planet.radius_km as f64 * 1_000.0;
        let Some(orientation) =
            ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
        else {
            continue;
        };
        let altitude_m = (rocket.dynamics.position_m.length() - radius_m).max(0.0);
        let atmosphere_relative_velocity_mps = rocket.dynamics.velocity_mps
            - surface_velocity_in_planet_inertial(rocket.dynamics.position_m, orientation);
        conditions.replace_sample(FlightConditions::from_atmosphere(
            altitude_m,
            atmosphere.source.properties(altitude_m),
            atmosphere_relative_velocity_mps,
        ));
    }
}
