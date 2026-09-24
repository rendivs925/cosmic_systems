//! Orbital mechanics in two layers: solar-map display geometry and the
//! state-vector orbital-element/transfer math shared by flight systems.
//!
//! The implementation is split into cohesive submodules: [`solar_map`]
//! (approximate parent-relative map projection) and [`state_orbits`]
//! (frame-relative orbital elements, apsis geometry, and transfer speeds).

mod solar_map;
mod state_orbits;

pub use solar_map::{
    calculate_orbit_radius_units, calculate_planet_position_f64, orbit_point_f64, orbit_shape_for,
    orbit_shape_for_at_time, orbital_elements_for, transform_orbital_point_f64, OrbitShape,
    OrbitalElements, MOON_ORBIT_SCALE,
};
pub use state_orbits::{
    apsis_endpoints_from_state, circular_speed_mps, circularize_burn_dv, hohmann_transfer_dv,
    orbital_elements_from_state, orbital_elements_from_state_in_reference_frame, plane_change_dv,
    specific_orbital_energy, vis_viva_speed_mps, ApsisEndpoints, LowEarthOrbitTarget,
    StateVectorOrbitalElements, APSIS_ECCENTRICITY_EPSILON,
};

#[cfg(test)]
mod tests {
    use super::solar_map::{orbital_eccentric_anomaly, true_anomaly};
    use super::*;
    use crate::domain::math::DQuat;
    use crate::domain::math::DVec3;
    use crate::domain::services::physics_utils::calculate_visual_radius;
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::value_objects::solar_system_params::SolarSystemParameters;

    const EARTH_MU: f64 = 3.986004418e14; // m^3/s^2
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    #[test]
    fn circular_orbit_elements() {
        let r = 6_771_000.0; // 400 km altitude
        let v_circular = (EARTH_MU / r).sqrt();
        let pos = DVec3::new(r, 0.0, 0.0);
        let vel = DVec3::new(0.0, v_circular, 0.0);

        let elements = orbital_elements_from_state(pos, vel, EARTH_MU);

        assert!((elements.eccentricity - 0.0).abs() < 1e-6);
        assert!((elements.semi_major_axis_m - r).abs() < 1.0);
        assert!((elements.inclination_rad - 0.0).abs() < 1e-6);
        assert!((elements.apoapsis_m - r).abs() < 1.0);
        assert!((elements.periapsis_m - r).abs() < 1.0);
    }

    #[test]
    fn elliptical_orbit_elements() {
        let r_p = 6_671_000.0; // 300 km periapsis
        let r_a = 7_071_000.0; // 700 km apoapsis
        let a = (r_p + r_a) / 2.0;
        let v_p = (EARTH_MU * (2.0 / r_p - 1.0 / a)).sqrt(); // Periapsis velocity

        let pos = DVec3::new(r_p, 0.0, 0.0);
        let vel = DVec3::new(0.0, v_p, 0.0);

        let elements = orbital_elements_from_state(pos, vel, EARTH_MU);

        assert!((elements.eccentricity - (r_a - r_p) / (r_a + r_p)).abs() < 1e-4);
        assert!((elements.semi_major_axis_m - a).abs() < 10.0);
        assert!((elements.periapsis_m - r_p).abs() < 10.0);
        assert!((elements.apoapsis_m - r_a).abs() < 10.0);
    }

    #[test]
    fn analytic_apsides_match_the_eccentricity_vector() {
        let periapsis_m = 6_671_000.0;
        let apoapsis_m = 7_071_000.0;
        let semi_major_axis_m = (periapsis_m + apoapsis_m) * 0.5;
        let velocity_mps = (EARTH_MU * (2.0 / periapsis_m - 1.0 / semi_major_axis_m)).sqrt();
        let apsides = apsis_endpoints_from_state(
            DVec3::new(periapsis_m, 0.0, 0.0),
            DVec3::new(0.0, velocity_mps, 0.0),
            EARTH_MU,
        )
        .expect("elliptical orbit has unique apsides");

        assert!((apsides.periapsis_position_m.length() - periapsis_m).abs() < 1e-6);
        assert!((apsides.apoapsis_position_m.length() - apoapsis_m).abs() < 1e-6);
        assert!(apsides.periapsis_position_m.x > 0.0);
        assert!(apsides.apoapsis_position_m.x < 0.0);
    }

    #[test]
    fn analytic_apsides_preserve_an_arbitrary_orbital_orientation() {
        let periapsis_m = 6_671_000.0;
        let apoapsis_m = 7_071_000.0;
        let semi_major_axis_m = (periapsis_m + apoapsis_m) * 0.5;
        let velocity_mps = (EARTH_MU * (2.0 / periapsis_m - 1.0 / semi_major_axis_m)).sqrt();
        let rotation = DQuat::from_rotation_arc(DVec3::Y, DVec3::new(0.3, 0.8, 0.5).normalize());
        let apsides = apsis_endpoints_from_state(
            rotation * DVec3::new(periapsis_m, 0.0, 0.0),
            rotation * DVec3::new(0.0, velocity_mps, 0.0),
            EARTH_MU,
        )
        .expect("rotated ellipse has unique apsides");

        assert!((apsides.periapsis_position_m - rotation * DVec3::X * periapsis_m).length() < 1e-6);
        assert!((apsides.apoapsis_position_m + rotation * DVec3::X * apoapsis_m).length() < 1e-6);
    }

    #[test]
    fn analytic_apsides_remain_exact_for_a_long_period_orbit() {
        let periapsis_m = EARTH_RADIUS_M + 200_000.0;
        let apoapsis_m = 250_000_000.0;
        let semi_major_axis_m = (periapsis_m + apoapsis_m) * 0.5;
        let velocity_mps = (EARTH_MU * (2.0 / periapsis_m - 1.0 / semi_major_axis_m)).sqrt();
        let apsides = apsis_endpoints_from_state(
            DVec3::new(periapsis_m, 0.0, 0.0),
            DVec3::new(0.0, velocity_mps, 0.0),
            EARTH_MU,
        )
        .expect("long-period ellipse has unique apsides");

        assert!((apsides.periapsis_position_m.length() - periapsis_m).abs() < periapsis_m * 1e-12);
        assert!((apsides.apoapsis_position_m.length() - apoapsis_m).abs() < apoapsis_m * 1e-12);
    }

    #[test]
    fn circular_orbit_has_no_unique_analytic_apsides() {
        let radius_m = 6_771_000.0;
        let velocity_mps = (EARTH_MU / radius_m).sqrt();
        assert!(apsis_endpoints_from_state(
            DVec3::new(radius_m, 0.0, 0.0),
            DVec3::new(0.0, velocity_mps, 0.0),
            EARTH_MU,
        )
        .is_none());
    }

    #[test]
    fn inclined_orbit_elements() {
        let r = 6_771_000.0;
        let v_circular = (EARTH_MU / r).sqrt();
        let inclination = 28.5_f64.to_radians(); // KSC inclination

        let pos = DVec3::new(r, 0.0, 0.0);
        let vel = DVec3::new(
            0.0,
            v_circular * inclination.cos(),
            v_circular * inclination.sin(),
        );

        let elements = orbital_elements_from_state(pos, vel, EARTH_MU);

        assert!((elements.inclination_rad - inclination).abs() < 1e-4);
    }

    #[test]
    fn local_equatorial_elements_respect_a_tilted_spin_axis() {
        let radius_m = 6_771_000.0;
        let circular_speed_mps = (EARTH_MU / radius_m).sqrt();
        let tilt = 23.44_f64.to_radians();
        let spin_axis = DQuat::from_rotation_z(tilt) * DVec3::Y;
        let reference_x = DQuat::from_rotation_z(tilt) * DVec3::X;
        let position_m = reference_x * radius_m;
        let velocity_mps = spin_axis.cross(position_m).normalize() * circular_speed_mps;

        let elements = orbital_elements_from_state_in_reference_frame(
            position_m,
            velocity_mps,
            EARTH_MU,
            spin_axis,
            reference_x,
        );

        assert!(elements.inclination_rad.abs() < 1e-12);
    }

    #[test]
    fn highly_eccentric_true_anomaly_remains_finite_at_apoapsis() {
        let anomaly = true_anomaly(std::f32::consts::PI, 0.7512);

        assert!(anomaly.is_finite());
        assert!((anomaly.abs() - std::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn moon_orbit_shape_uses_the_catalog_semimajor_axis() {
        let solar = SolarSystemParameters::for_visualization();
        let nereid = PlanetFactory::create_by_name("Nereid").unwrap();
        let shape = orbit_shape_for(&nereid, &solar);

        assert!(
            (shape.semi_major_axis_units - nereid.orbital_distance_au * solar.scale_factor).abs()
                < 1e-3
        );
    }

    #[test]
    fn triton_parent_relative_projection_preserves_outer_moon_precision() {
        let solar = SolarSystemParameters::for_visualization();
        let neptune = PlanetFactory::create_by_name("Neptune").unwrap();
        let triton = PlanetFactory::create_by_name("Triton").unwrap();
        let time_days = 123.456_789;
        let neptune_position = DVec3::new(2_250_000.0, -100_000.0, 500_000.0);
        let triton_position = calculate_planet_position_f64(
            &triton,
            time_days,
            &solar,
            neptune_position,
            Some(neptune.axial_tilt_deg),
        );

        let relative_position = triton_position - neptune_position;
        let rebased_error = DVec3::from(relative_position.as_vec3()).distance(relative_position);
        let absolute_error = DVec3::from(triton_position.as_vec3()).distance(triton_position);

        assert!(relative_position.length() > 100.0);
        assert!(rebased_error < 0.000_1, "rebased error was {rebased_error}");
        assert!(
            absolute_error > rebased_error * 100.0,
            "absolute error {absolute_error} was not materially worse than rebased error {rebased_error}"
        );
    }

    #[test]
    fn moon_states_match_their_parent_relative_epoch_shape_projection() {
        let solar = SolarSystemParameters::for_visualization();
        let time_days = 123.456_789;

        for (index, moon) in PlanetFactory::get_moons().into_iter().enumerate() {
            let parent = PlanetFactory::create_by_name(
                moon.parent_entity
                    .as_deref()
                    .expect("moon catalog entries have a parent"),
            )
            .expect("moon parent exists in the catalog");
            let parent_position =
                DVec3::new(1_000_000.0 + index as f64 * 10_000.0, -50_000.0, 25_000.0);
            let orbit_shape = orbit_shape_for_at_time(&moon, &solar, time_days);
            let local_orbit_point = orbit_point_f64(
                &orbit_shape,
                orbital_eccentric_anomaly(&moon, &orbit_shape, time_days),
            );
            let tilt_rad = (parent.axial_tilt_deg as f64).to_radians();
            let expected = parent_position
                + DVec3::new(
                    local_orbit_point.x * tilt_rad.cos() - local_orbit_point.y * tilt_rad.sin(),
                    local_orbit_point.x * tilt_rad.sin() + local_orbit_point.y * tilt_rad.cos(),
                    local_orbit_point.z,
                );
            let propagated = calculate_planet_position_f64(
                &moon,
                time_days,
                &solar,
                parent_position,
                Some(parent.axial_tilt_deg),
            );

            assert!(
                propagated.distance(expected) < 1e-9,
                "{} departed from its parent-relative ribbon geometry",
                moon.name
            );
        }
    }

    #[test]
    fn time_warp_changes_do_not_teleport_a_parent_relative_moon() {
        let mut solar = SolarSystemParameters::for_visualization();
        let elapsed_seconds = 12_345.678_9;
        let moon = PlanetFactory::create_by_name("Moon").unwrap();
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let earth_position = DVec3::new(75_000.0, 0.0, 0.0);
        let positions_at = |params: &SolarSystemParameters| {
            let time_days = params.time_to_days_f64(elapsed_seconds);
            calculate_planet_position_f64(
                &moon,
                time_days,
                params,
                earth_position,
                Some(earth.axial_tilt_deg),
            )
        };

        let moon_before = positions_at(&solar);
        solar.set_time_scale_at(elapsed_seconds, 1.0);
        let moon_realtime = positions_at(&solar);
        solar.set_time_scale_at(elapsed_seconds, 10_000.0);
        let moon_high_warp = positions_at(&solar);

        assert!((moon_realtime - moon_before).length() < 1e-8);
        assert!((moon_high_warp - moon_before).length() < 1e-8);
    }

    #[test]
    fn every_moon_clears_its_parent_at_periapsis_on_the_solar_map() {
        let solar = SolarSystemParameters::for_visualization();

        for moon in PlanetFactory::get_moons() {
            let parent_name = moon.parent_entity.as_deref().unwrap();
            let parent = PlanetFactory::create_by_name(parent_name).unwrap();
            let orbit = orbit_shape_for(&moon, &solar);
            let periapsis_units = orbit.semi_major_axis_units * (1.0 - orbit.eccentricity);
            let clearance_units = periapsis_units
                - calculate_visual_radius(&parent, &solar)
                - calculate_visual_radius(&moon, &solar);

            assert!(
                clearance_units > 0.0,
                "{} intersects {} by {} solar-map units at periapsis",
                moon.name,
                parent.name,
                -clearance_units
            );
        }
    }

    #[test]
    fn low_earth_target_accepts_a_safe_inclined_circular_orbit() {
        let target = LowEarthOrbitTarget::default();
        let radius_m = EARTH_RADIUS_M + 200_000.0;
        let circular_speed_mps = (EARTH_MU / radius_m).sqrt();
        let velocity_mps = DVec3::new(
            0.0,
            circular_speed_mps * target.target_inclination_rad.cos(),
            circular_speed_mps * target.target_inclination_rad.sin(),
        );

        assert!(target.matches_state(
            DVec3::new(radius_m, 0.0, 0.0),
            velocity_mps,
            EARTH_MU,
            EARTH_RADIUS_M,
        ));
    }

    #[test]
    fn low_earth_target_rejects_near_orbital_speed_state_with_unsafe_periapsis() {
        let target = LowEarthOrbitTarget::default();
        let radius_m = EARTH_RADIUS_M + 200_000.0;
        let circular_speed_mps = (EARTH_MU / radius_m).sqrt();
        let velocity_mps = DVec3::new(
            0.0,
            circular_speed_mps * 0.98 * target.target_inclination_rad.cos(),
            circular_speed_mps * 0.98 * target.target_inclination_rad.sin(),
        );

        assert!(!target.matches_state(
            DVec3::new(radius_m, 0.0, 0.0),
            velocity_mps,
            EARTH_MU,
            EARTH_RADIUS_M,
        ));
    }

    #[test]
    fn low_earth_target_rejects_unbound_or_wrong_plane_states() {
        let target = LowEarthOrbitTarget::default();
        let radius_m = EARTH_RADIUS_M + 200_000.0;
        let circular_speed_mps = (EARTH_MU / radius_m).sqrt();

        assert!(!target.matches_state(
            DVec3::new(radius_m, 0.0, 0.0),
            DVec3::new(0.0, circular_speed_mps * 2.0, 0.0),
            EARTH_MU,
            EARTH_RADIUS_M,
        ));
        assert!(!target.matches_state(
            DVec3::new(radius_m, 0.0, 0.0),
            DVec3::new(0.0, circular_speed_mps, 0.0),
            EARTH_MU,
            EARTH_RADIUS_M,
        ));
    }

    #[test]
    fn circularize_burn_positive_for_suborbital() {
        let r = 6_771_000.0;
        let v_suborbital = (EARTH_MU / r).sqrt() * 0.8; // 80% of circular
        let pos = DVec3::new(r, 0.0, 0.0);
        let vel = DVec3::new(0.0, v_suborbital, 0.0);

        let (dv, target_r) = circularize_burn_dv(pos, vel, EARTH_MU);

        assert!(dv > 0.0);
        assert!((target_r - r).abs() < 1.0);
    }

    #[test]
    fn hohmann_transfer_to_higher_orbit() {
        let r1 = 6_771_000.0; // 400 km
        let r2 = 42_164_000.0; // GEO
        let (dv1, dv2) = hohmann_transfer_dv(r1, r2, EARTH_MU);

        assert!(dv1 > 0.0);
        assert!(dv2 > 0.0);
        // Total ~3.9 km/s for LEO to GEO
        assert!((dv1 + dv2 - 3900.0).abs() < 100.0);
    }

    /// Regression pin: the period derived from state vectors must equal the
    /// analytic two-body result T = 2π√(a³/μ) for a circular orbit.
    #[test]
    fn circular_orbit_period_matches_analytic() {
        let r = 6_971_000.0; // 600 km altitude
        let v_circular = (EARTH_MU / r).sqrt();
        let pos = DVec3::new(r, 0.0, 0.0);
        let vel = DVec3::new(0.0, v_circular, 0.0);

        let elements = orbital_elements_from_state(pos, vel, EARTH_MU);
        let analytic_period = 2.0 * std::f64::consts::PI * (r.powi(3) / EARTH_MU).sqrt();

        assert!(
            (elements.orbital_period_s - analytic_period).abs() < analytic_period * 1e-6,
            "period from state {} s vs analytic {} s",
            elements.orbital_period_s,
            analytic_period
        );
        // Sanity: a 600 km LEO orbit is ~96.5 minutes (analytic value).
        assert!((analytic_period / 60.0 - 96.5).abs() < 0.5);
    }
}
