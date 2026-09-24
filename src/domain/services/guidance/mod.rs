//! Mission guidance: where the vehicle should go.
//!
//! Guidance is the first layer of the flight loop (AGENTS.md section 18):
//! mission → guidance → control → actuation → physics → state → guidance.
//! Guidance computes a target attitude (and phase transitions) from the
//! mission and the current state; it never commands actuators or writes the
//! vehicle's motion. All target generation is a pure function so it is
//! testable without Bevy.
//!
//! ## Ascent guidance
//!
//! A gravity-turn pitch-over: the vehicle holds the local vertical on the pad,
//! then pitches over toward the downrange direction by an angle that grows with
//! altitude, reaching [`ascent::AscentGuidanceProfile::max_turn_angle_rad`] by
//! [`ascent::AscentGuidanceProfile::turn_end_altitude_m`]. Orbit insertion
//! aligns the vehicle prograde (with the velocity vector).
//!
//! ## Descent guidance
//!
//! - Deorbit burn: computes retrograde burn to lower periapsis to entry interface.
//! - Reentry corridor: bank-angle profile to manage g-load, q, and heat flux.
//! - Powered descent: convex optimization for minimum-fuel landing.
//! - Unpowered descent: parafoil lateral acceleration tracking.
//!
//! The implementation is split into cohesive submodules: [`ascent`], [`entry`],
//! [`landing`], [`transfer`], [`boostback`], and the shared [`attitude`]
//! helpers. [`AutopilotMode`] is the one flight-computer mode enum.

mod ascent;
mod attitude;
mod boostback;
mod entry;
mod landing;
mod transfer;

pub use ascent::{
    advance_ascent_phase, ascent_pitch_gate_clear, gravity_turn_direction,
    gravity_turn_direction_gated, gravity_turn_pitch_angle, gravity_turn_pitch_angle_combined,
    gravity_turn_pitch_angle_gated, gravity_turn_pitch_angle_time,
    prograde_ascending_node_launch_heading, target_attitude_for_phase, AscendingNodeLaunchHeading,
    AscendingNodeLaunchHeadingError, AscentGuidanceProfile,
};
pub use attitude::{
    attitude_from_direction, banked_attitude_from_direction, pitch_axis_from_reference,
    prograde_attitude,
};
pub use boostback::{
    boostback_guidance, BoostbackCommand, BOOSTBACK_COMPLETE_DISTANCE_M,
    BOOSTBACK_COMPLETE_SPEED_MPS, BOOSTBACK_POSITION_GAIN_INV_S2, BOOSTBACK_VELOCITY_GAIN_INV_S,
};
pub use entry::{
    advance_descent_phase, reentry_bank_angle, reentry_bank_angle_enhanced,
    target_surface_range_errors_m, DescentGuidanceConfig, SurfaceRangeErrors,
};
pub use landing::{
    default_surface_landing_target, hover_slam_guidance, powered_descent_guidance,
    powered_descent_guidance_convex, suicide_burn_guidance, terminal_landing_guidance,
};
pub use transfer::{
    bielliptic_potentially_favorable, bielliptic_transfer, combined_maneuver_dv, deorbit_burn_dv,
    deorbit_burn_targeting, hohmann_transfer, transfer_burn_phase, BiellipticSolution,
    TransferBurnPhase, TransferSolution, BIELLIPTIC_FAVORABLE_RATIO,
};

/// Autopilot mode for the flight computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutopilotMode {
    #[default]
    Off,
    /// Gravity-turn ascent to orbit insertion.
    Ascent,
    /// Circularization burn at apoapsis.
    OrbitInsertion,
    /// Retrograde burn to lower periapsis for entry.
    Deorbit,
    /// Bank-angle management for atmospheric entry.
    Reentry,
    /// Powered descent with convex optimization (suicide burn / hover-slam).
    PoweredDescent,
    /// Booster flyback skeleton: retrograde pitch-over burn targeting
    /// return-to-launch-site downrange zeroing; hands off to
    /// [`AutopilotMode::Landing`] (suicide burn / hover-slam) for touchdown.
    Boostback,
    /// Terminal landing guidance.
    Landing,
    /// Two-impulse orbit transfer (Hohmann, or bi-elliptic when favorable):
    /// departure burn → coast to apsis → arrival burn. Target radius comes
    /// from [`crate::infrastructure::bevy_adapters::rocket::components::RocketAutopilot::
    /// transfer_target_radius_m`].
    Transfer,
    /// Station keeping / orbital maintenance.
    StationKeep,
    /// Rendezvous with target vehicle (future).
    Rendezvous,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::rocket::RocketMissionState;
    use crate::domain::math::{DQuat, DVec3};
    use crate::domain::services::gravity::{
        circular_orbit_speed_mps, gravitational_acceleration, gravitational_parameter,
    };
    use crate::domain::services::physics_orbital::plane_change_dv;
    use crate::domain::services::reference_frames::{
        planet_inertial_enu_basis, PlanetInertialEnuError,
    };

    fn profile() -> AscentGuidanceProfile {
        AscentGuidanceProfile::new(0.0, 0.0, 80_000.0, 80.0_f64.to_radians())
    }

    fn up_dir() -> DVec3 {
        DVec3::new(0.0, 1.0, 0.0)
    }

    #[test]
    fn equatorial_orbit_from_equator_launches_due_east() {
        let spin_axis = DVec3::Z;
        let position_m = DVec3::X * 6_378_137.0;
        let basis = planet_inertial_enu_basis(position_m, spin_axis).unwrap();
        let heading = prograde_ascending_node_launch_heading(position_m, spin_axis, 0.0).unwrap();

        assert!((heading.azimuth_east_of_north_rad - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!((heading.direction_pci - basis.east).length() < 1e-12);
        assert!(heading.direction_pci.dot(basis.up).abs() < 1e-12);
    }

    #[test]
    fn ksc_28_5_degree_target_uses_the_eastward_ascending_solution() {
        use crate::domain::services::body_orientation::BodyOrientation;
        use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
        use crate::domain::services::planet_factory::PlanetFactory;
        use crate::domain::services::reference_frames::{
            body_fixed_to_planet_inertial, geodetic_to_body_fixed, planet_inertial_spin_axis,
        };
        use crate::domain::value_objects::launch_site_coordinates::predefined_sites;

        let orientation = BodyOrientation::from_kernel(
            NaifBodyId::EARTH,
            TdbEpoch::j2000(),
            "guidance-heading-test".to_owned(),
            DQuat::IDENTITY,
            DVec3::Z,
        );
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let position_m = body_fixed_to_planet_inertial(
            geodetic_to_body_fixed(&predefined_sites::kennedy_space_center(), &earth),
            &orientation,
        );
        let spin_axis = planet_inertial_spin_axis(&orientation);
        let basis = planet_inertial_enu_basis(position_m, spin_axis).unwrap();
        let target_inclination_rad = 28.5_f64.to_radians();
        let heading =
            prograde_ascending_node_launch_heading(position_m, spin_axis, target_inclination_rad)
                .unwrap();

        assert!(heading.azimuth_east_of_north_rad.to_degrees() > 80.0);
        assert!(heading.direction_pci.dot(basis.east) > 0.98);
        assert!(
            (target_inclination_rad.cos()
                - (1.0 - spin_axis.dot(basis.up).powi(2)).sqrt()
                    * heading.azimuth_east_of_north_rad.sin())
            .abs()
                < 1e-12
        );
    }

    #[test]
    fn polar_orbit_from_equator_launches_northbound() {
        let spin_axis = DVec3::Z;
        let position_m = DVec3::X;
        let basis = planet_inertial_enu_basis(position_m, spin_axis).unwrap();
        let heading = prograde_ascending_node_launch_heading(
            position_m,
            spin_axis,
            std::f64::consts::FRAC_PI_2,
        )
        .unwrap();

        assert!(heading.azimuth_east_of_north_rad.abs() < 1e-12);
        assert!((heading.direction_pci - basis.north).length() < 1e-12);
    }

    #[test]
    fn arbitrary_tilted_spin_axis_defines_the_local_heading_frame() {
        let spin_axis = DVec3::new(0.2, 0.7, 0.68).normalize();
        let position_m = (DVec3::Y - spin_axis * spin_axis.y).normalize() * 7_000_000.0;
        let basis = planet_inertial_enu_basis(position_m, spin_axis).unwrap();
        let inclination_rad = 45.0_f64.to_radians();
        let heading =
            prograde_ascending_node_launch_heading(position_m, spin_axis, inclination_rad).unwrap();

        let expected =
            (basis.north * inclination_rad.cos() + basis.east * inclination_rad.sin()).normalize();
        assert!((heading.direction_pci - expected).length() < 1e-12);
        assert!(heading.direction_pci.dot(basis.up).abs() < 1e-12);
    }

    #[test]
    fn target_below_launch_site_latitude_is_rejected() {
        let spin_axis = DVec3::Z;
        let latitude_rad = 30.0_f64.to_radians();
        let position_m = DVec3::X * latitude_rad.cos() + spin_axis * latitude_rad.sin();

        assert_eq!(
            prograde_ascending_node_launch_heading(position_m, spin_axis, 10.0_f64.to_radians(),),
            Err(AscendingNodeLaunchHeadingError::UnreachableInclination)
        );
    }

    #[test]
    fn polar_launch_site_returns_an_explicit_safe_error() {
        assert_eq!(
            prograde_ascending_node_launch_heading(DVec3::Z, DVec3::Z, std::f64::consts::FRAC_PI_2),
            Err(AscendingNodeLaunchHeadingError::InvalidLocalFrame(
                PlanetInertialEnuError::PolarPosition
            ))
        );
    }

    #[test]
    fn gravity_turn_ramps_pitch_with_altitude() {
        let p = profile();
        assert_eq!(gravity_turn_pitch_angle(&p, 0.0), 0.0);
        assert_eq!(
            gravity_turn_pitch_angle(&p, 40_000.0),
            40.0_f64.to_radians()
        );
        assert!((gravity_turn_pitch_angle(&p, 80_000.0) - 80.0_f64.to_radians()).abs() < 1e-12);
        // Clamps beyond the profile.
        assert!((gravity_turn_pitch_angle(&p, 200_000.0) - 80.0_f64.to_radians()).abs() < 1e-12);
    }

    #[test]
    fn gravity_turn_direction_pitches_toward_azimuth() {
        let p = profile();
        let axis = pitch_axis_from_reference(up_dir(), DVec3::Z).unwrap();
        let dir = gravity_turn_direction(&p, up_dir(), axis, 80_000.0);
        // Rotated 80° from vertical toward the horizontal plane.
        assert!((dir.dot(up_dir()) - 80.0_f64.to_radians().cos()).abs() < 1e-9);
        // Horizontal component grows with altitude.
        let vertical = gravity_turn_direction(&p, up_dir(), axis, 0.0);
        assert!((vertical - up_dir()).length() < 1e-9);
    }

    #[test]
    fn pitch_axis_is_horizontal_and_normalized() {
        let axis = pitch_axis_from_reference(up_dir(), DVec3::Z).unwrap();
        assert!((axis.length() - 1.0).abs() < 1e-12);
        assert!(axis.dot(up_dir()).abs() < 1e-12);
        assert!(pitch_axis_from_reference(DVec3::Z, DVec3::Z).is_none());
    }

    #[test]
    fn gated_pitch_holds_vertical_until_gate_passes() {
        let p = profile();
        // Well past the time schedule start (10 s) but low and slow: the
        // gate must keep the vehicle exactly vertical (electron tower-tip
        // regression).
        assert!((gravity_turn_pitch_angle_gated(&p, 65.0, 30.0, 13.0)).abs() < 1e-12);
        assert!((gravity_turn_pitch_angle_gated(&p, 0.0, 60.0, 0.0)).abs() < 1e-12);
        // High enough but still slow: altitude condition alone is not enough.
        assert!((gravity_turn_pitch_angle_gated(&p, 500.0, 30.0, 10.0)).abs() < 1e-12);
        // Fast but too low: vertical-speed condition alone is not enough.
        assert!((gravity_turn_pitch_angle_gated(&p, 100.0, 30.0, 80.0)).abs() < 1e-12);
    }

    #[test]
    fn gated_pitch_engages_combined_schedule_once_gate_clears() {
        let p = profile();
        let alt = 2_000.0;
        let t = 20.0;
        let vs = 100.0;
        assert!(ascent_pitch_gate_clear(&p, alt, vs));
        let gated = gravity_turn_pitch_angle_gated(&p, alt, t, vs);
        let combined = gravity_turn_pitch_angle_combined(&p, alt, t);
        assert!((gated - combined).abs() < 1e-12);

        // Inclusive thresholds: a gate met exactly engages the turn.
        assert!(ascent_pitch_gate_clear(
            &p,
            p.pitch_gate_min_altitude_m,
            p.pitch_gate_min_vertical_speed_mps
        ));
    }

    #[test]
    fn gated_direction_matches_gated_pitch() {
        let p = profile();
        let axis = pitch_axis_from_reference(up_dir(), DVec3::Z).unwrap();
        // Below the gate: direction is exactly the local vertical.
        let dir = gravity_turn_direction_gated(&p, up_dir(), axis, 50.0, 30.0, 5.0);
        assert!((dir - up_dir()).length() < 1e-9);
        // Above the gate: tilted away from vertical by the schedule angle.
        let angle = gravity_turn_pitch_angle_gated(&p, 40_000.0, 90.0, 300.0);
        let dir = gravity_turn_direction_gated(&p, up_dir(), axis, 40_000.0, 90.0, 300.0);
        assert!((dir.dot(up_dir()) - angle.cos()).abs() < 1e-9);
    }

    #[test]
    fn attitude_points_body_y_along_direction() {
        let dir = DVec3::new(0.0, 1.0, 0.0);
        let q = attitude_from_direction(dir);
        let body_y = q * DVec3::Y;
        assert!((body_y - dir).length() < 1e-9);
    }

    #[test]
    fn prograde_aligns_with_velocity() {
        let vel = DVec3::new(7_600.0, 0.0, 0.0);
        let q = prograde_attitude(vel);
        assert!((q * DVec3::Y - DVec3::X).length() < 1e-9);
        assert_eq!(prograde_attitude(DVec3::ZERO), DQuat::IDENTITY);
    }

    #[test]
    fn phase_selects_distinct_targets() {
        let p = profile();
        let axis = pitch_axis_from_reference(up_dir(), DVec3::Z).unwrap();
        let vel = DVec3::new(7_600.0, 0.0, 0.0);

        let launch =
            target_attitude_for_phase(RocketMissionState::Launch, &p, up_dir(), axis, 0.0, vel);
        let ascent = target_attitude_for_phase(
            RocketMissionState::Ascent,
            &p,
            up_dir(),
            axis,
            80_000.0,
            vel,
        );
        let orbit =
            target_attitude_for_phase(RocketMissionState::Orbit, &p, up_dir(), axis, 80_000.0, vel);

        assert!((launch * DVec3::Y - up_dir()).length() < 1e-9);
        // Ascent tilts from vertical.
        assert!((ascent * DVec3::Y).dot(up_dir()) < 1.0 - 1e-3);
        // Orbit aligns with the velocity.
        assert!((orbit * DVec3::Y - DVec3::X).length() < 1e-9);
    }

    #[test]
    fn phase_advances_launch_ascent_orbit() {
        assert_eq!(
            advance_ascent_phase(RocketMissionState::Launch, 1_000.0, 5_000.0, false,),
            RocketMissionState::Launch
        );
        assert_eq!(
            advance_ascent_phase(RocketMissionState::Launch, 6_000.0, 5_000.0, false,),
            RocketMissionState::Ascent
        );
        assert_eq!(
            advance_ascent_phase(RocketMissionState::Ascent, 200_000.0, 5_000.0, true,),
            RocketMissionState::Orbit
        );
        // An unsafe or incomplete target state stays in ascent.
        assert_eq!(
            advance_ascent_phase(RocketMissionState::Ascent, 200_000.0, 5_000.0, false,),
            RocketMissionState::Ascent
        );
    }

    #[test]
    fn deorbit_burn_dv_positive_for_lower_periapsis() {
        let mu = 3.986e14; // Earth
        let orbit_r = 6_771_000.0; // 400 km altitude
        let target_peri = 6_471_000.0; // 100 km periapsis
        let dv = deorbit_burn_dv(orbit_r, target_peri, mu);
        assert!(dv > 0.0);
        assert!(dv < 200.0); // Reasonable deorbit burn
    }

    #[test]
    fn hohmann_matches_reference_values() {
        let mu = 3.986e14; // Earth
        let leo = 6_678_000.0; // ~300 km altitude
        let geo = 42_164_000.0;
        let t = hohmann_transfer(leo, geo, mu);
        // Classical LEO→GEO figures: Δv ≈ 2.43 + 1.47 km/s over ≈ 5.27 h.
        assert!(
            (t.departure_dv_mps - 2_430.0).abs() < 60.0,
            "departure dv {}",
            t.departure_dv_mps
        );
        assert!(
            (t.arrival_dv_mps - 1_470.0).abs() < 60.0,
            "arrival dv {}",
            t.arrival_dv_mps
        );
        assert!(
            (t.transfer_time_s - 18_930.0).abs() < 300.0,
            "transfer time {}",
            t.transfer_time_s
        );

        // Direction-symmetric: lowering costs the same impulses.
        let back = hohmann_transfer(geo, leo, mu);
        assert!((back.total_dv_mps() - t.total_dv_mps()).abs() < 1e-9);
        assert!((back.transfer_time_s - t.transfer_time_s).abs() < 1e-6);

        // Degenerate: same orbit → zero cost.
        let none = hohmann_transfer(leo, leo, mu);
        assert!(none.total_dv_mps() < 1e-9);
    }

    #[test]
    fn bielliptic_ratio_boundary_and_budget() {
        // Below the classical ratio the Hohmann wins (or ties) by rule.
        assert!(!bielliptic_potentially_favorable(7_000_000.0, 80_000_000.0));
        assert!(bielliptic_potentially_favorable(7_000_000.0, 90_000_000.0));

        // A tall bi-elliptic for a >11.94 ratio must be a valid maneuver:
        // positive finite impulses and a longer coast than the Hohmann.
        let mu = 3.986e14;
        let r1 = 7_000_000.0;
        let r2 = 100_000_000.0;
        let rb = 500_000_000.0;
        let b = bielliptic_transfer(r1, r2, rb, mu);
        assert!(b.departure_dv_mps > 0.0 && b.mid_dv_mps.is_finite());
        assert!(b.arrival_dv_mps > 0.0 && b.arrival_dv_mps < 1_000.0);
        let h = hohmann_transfer(r1, r2, mu);
        assert!(
            b.transfer_time_s > h.transfer_time_s * 5.0,
            "bi-elliptic via {} m must take far longer",
            rb
        );
    }

    #[test]
    fn plane_change_and_combined_identity() {
        // Pure rotation: Δv = 2 v sin(i/2).
        let speed = 7_600.0;
        let i = 30.0_f64.to_radians();
        let expected = 2.0 * speed * (i / 2.0).sin();
        assert!((plane_change_dv(speed, i) - expected).abs() < 1e-9);
        // Zero change costs nothing.
        assert_eq!(plane_change_dv(speed, 0.0), 0.0);

        // The combined-maneuver identity is the law of cosines: at a right
        // angle it degenerates to the hypotenuse; with equal magnitudes and
        // a nearly-opposed angle it stays below the sequential sum.
        let (a, b) = (300.0_f64, 400.0_f64);
        let right_angle = std::f64::consts::FRAC_PI_2;
        assert!((combined_maneuver_dv(a, b, right_angle) - 500.0).abs() < 1e-9);

        let v = 1_000.0_f64;
        let almost_opposed = 170.0_f64.to_radians();
        let combined = combined_maneuver_dv(v, v, almost_opposed);
        assert!(
            combined < 2.0 * v && combined > std::f64::consts::SQRT_2 * v * 0.99,
            "combined {combined} outside the geometric expectation"
        );
        // Zero angle between identical vectors cancels completely.
        assert_eq!(combined_maneuver_dv(v, v, 0.0), 0.0);
    }

    /// Scenario `hohmann_simulated` (Phase 17): apply both solver burns to a
    /// real two-body integration (authoritative gravity + the production
    /// semi-implicit Euler, dt = 1 s) and verify the vehicle actually arrives
    /// on the target circle. Tolerances: arrival radius 0.1 % of r2 (bounded
    /// integration error at 1 s steps), final speed 0.1 %.
    #[test]
    fn hohmann_burns_simulate_to_a_circular_arrival() {
        let earth_mass_kg = 5.97237e24;
        let mu = gravitational_parameter(earth_mass_kg);
        let (r1, r2) = (6_678_000.0_f64, 42_164_000.0_f64);
        let t = hohmann_transfer(r1, r2, mu);

        let dt = 1.0;
        let mut pos = DVec3::new(r1, 0.0, 0.0);
        let mut vel = DVec3::new(0.0, 0.0, (mu / r1).sqrt());

        // Departure burn: prograde (tangential +Z here).
        vel += DVec3::new(0.0, 0.0, t.departure_dv_mps);

        // Coast half an ellipse.
        let coast_steps = (t.transfer_time_s / dt).round() as u32;
        for _ in 0..coast_steps {
            vel += gravitational_acceleration(earth_mass_kg, pos, DVec3::ZERO) * dt;
            pos += vel * dt;
        }
        assert!(
            ((pos.length() - r2) / r2).abs() < 1e-3,
            "transfer did not arrive at apoapsis r2: {}",
            pos.length()
        );

        // Arrival burn: prograde along the local tangential direction.
        let radial = pos.normalize();
        let tangential = DVec3::new(-radial.z, 0.0, radial.x);
        vel += tangential * t.arrival_dv_mps;

        // One full revolution later the orbit must still be the target circle.
        let period2 = std::f64::consts::PI * 2.0 * (r2 * r2 * r2 / mu).sqrt();
        let mut worst = 0.0_f64;
        for _ in 0..((period2 / dt) as u32) {
            vel += gravitational_acceleration(earth_mass_kg, pos, DVec3::ZERO) * dt;
            pos += vel * dt;
            worst = worst.max(((pos.length() - r2) / r2).abs());
        }
        assert!(
            worst < 1e-3,
            "arrival orbit not circular: worst drift {worst}"
        );
        assert!(
            ((vel.length() - (mu / r2).sqrt()) / (mu / r2).sqrt()).abs() < 1e-3,
            "arrival speed {} vs circular {}",
            vel.length(),
            (mu / r2).sqrt()
        );
    }

    #[test]
    fn transfer_phase_classification_walks_burn_coast_burn() {
        let r_now = 7_000_000.0_f64;
        let target = 10_000_000.0_f64;

        // Still in the parking orbit: apoapsis not yet raised.
        assert_eq!(
            transfer_burn_phase(r_now, target, r_now + 50_000.0, 0.007),
            TransferBurnPhase::Departure
        );
        // Apoapsis at the target but still elliptical: coasting.
        assert_eq!(
            transfer_burn_phase(r_now, target, target, 0.15),
            TransferBurnPhase::Coast
        );
        // Circularized at the target: done.
        assert_eq!(
            transfer_burn_phase(target, target, target, 0.001),
            TransferBurnPhase::Done
        );
    }

    #[test]
    fn deorbit_burn_targeting_returns_retrograde() {
        let pos = DVec3::new(6_771_000.0, 0.0, 0.0);
        let vel = DVec3::new(0.0, 7_600.0, 0.0);
        let mu = 3.986e14;
        let (dv, att) = deorbit_burn_targeting(pos, vel, 6_471_000.0, mu);
        assert!(dv > 0.0);
        // Attitude should point opposite to velocity (retrograde).
        let body_y = att * DVec3::Y;
        let retrograde = -vel.normalize();
        assert!((body_y - retrograde).length() < 1e-6);
    }

    #[test]
    fn boostback_burns_opposing_downrange_velocity() {
        // Vehicle ~100 km downrange of the pad, flying further away.
        let radius = 6_371_000.0;
        let site = DVec3::new(radius, 0.0, 0.0);
        let theta = 100_000.0 / radius; // central angle for ~100 km arc
        let position = DVec3::new(
            radius * theta.cos() * (radius + 200_000.0) / radius,
            radius * theta.sin() * (radius + 200_000.0) / radius,
            0.0,
        )
        .normalize()
            * (radius + 200_000.0);
        // Tangential unit vector at the vehicle (direction of increasing θ).
        let tangent = DVec3::new(-position.y, position.x, 0.0).normalize();
        let velocity = tangent * 300.0;

        let cmd = boostback_guidance(position, velocity, site, 25_000.0, 1_000_000.0);

        assert!(!cmd.complete, "far and fast must not be complete");
        assert!(cmd.throttle > 0.0, "must command a burn");
        // Thrust direction opposes the receding horizontal velocity.
        let thrust_dir = cmd.attitude * DVec3::Y;
        assert!(
            thrust_dir.dot(tangent) < 0.0,
            "must burn back toward pad (against tangent)"
        );
    }

    #[test]
    fn boostback_completes_over_the_pad_when_slow() {
        let radius = 6_371_000.0;
        let site = DVec3::new(radius, 0.0, 0.0);
        // Radially above the pad at 200 km with a ~100 m tangential offset.
        let position = site + DVec3::X * 200_000.0 + DVec3::Y * 100.0;
        let velocity = DVec3::new(-5.0, 0.0, 0.0); // nearly null horizontally
        let cmd = boostback_guidance(position, velocity, site, 25_000.0, 1_000_000.0);
        assert!(cmd.complete);
    }

    #[test]
    fn boostback_zero_horizontal_error_gives_no_burn() {
        let site = DVec3::new(6_371_000.0, 0.0, 0.0);
        // Vehicle radially above the pad: no horizontal error.
        let up = site.normalize();
        let pos = site + up * 200_000.0;
        let cmd = boostback_guidance(pos, DVec3::ZERO, site, 25_000.0, 1_000_000.0);
        assert_eq!(cmd.throttle, 0.0, "no horizontal state error → coast");
        // Pad is directly below: boostback hands off to the landing leg.
        assert!(cmd.complete);
    }

    #[test]
    fn reentry_bank_angle_zero_when_within_corridor() {
        let config = DescentGuidanceConfig::default();
        let bank = reentry_bank_angle(80_000.0, 3000.0, 10_000.0, 100_000.0, 2.0, &config, 0.0);
        assert!((bank - 0.0).abs() < 1e-6);
    }

    #[test]
    fn reentry_bank_angle_90_deg_when_violating_constraints() {
        let config = DescentGuidanceConfig::default();
        let bank = reentry_bank_angle(50_000.0, 5000.0, 100_000.0, 2_000_000.0, 6.0, &config, 0.0);
        assert!((bank.abs() - 90.0_f64.to_radians()).abs() < 1e-6);
    }

    #[test]
    fn surface_range_errors_follow_the_local_flight_frame() {
        let radius_m = 6_371_000.0;
        let position = DVec3::new(radius_m, 0.0, 0.0);
        let velocity = DVec3::Y * 1_000.0;

        // A target ahead along +Y has positive downrange and no crossrange.
        let ahead = DVec3::new(radius_m, 100_000.0, 0.0).normalize() * radius_m;
        let range_errors = target_surface_range_errors_m(position, velocity, ahead, radius_m);
        assert!(range_errors.crossrange_m.abs() < 1e-6);
        assert!(range_errors.downrange_m > 99_000.0 && range_errors.downrange_m < 100_000.0);

        // At this location the vehicle's left is -Z, matching the bank sign
        // convention used by `reentry_bank_angle_enhanced`.
        let left = DVec3::new(radius_m, 0.0, -100_000.0).normalize() * radius_m;
        let range_errors = target_surface_range_errors_m(position, velocity, left, radius_m);
        assert!(range_errors.crossrange_m > 99_000.0 && range_errors.crossrange_m < 100_000.0);
        assert!(range_errors.downrange_m.abs() < 1e-6);
    }

    #[test]
    fn banked_attitude_rolls_about_the_longitudinal_axis() {
        let direction = DVec3::new(0.2, 0.9, 0.3).normalize();
        let bank = 30.0_f64.to_radians();
        let attitude = banked_attitude_from_direction(direction, bank);
        assert!((attitude * DVec3::Y).dot(direction) > 1.0 - 1e-12);
        let unbanked = attitude_from_direction(direction) * DVec3::X;
        let banked = attitude * DVec3::X;
        assert!(unbanked.angle_between(banked) > 0.1);
    }

    #[test]
    fn default_landing_target_is_on_surface_directly_below_vehicle() {
        let position = DVec3::new(6_371_000.0 + 5_000.0, 0.0, 0.0);
        let target = default_surface_landing_target(position, 6_371_000.0);
        assert!((target.length() - 6_371_000.0).abs() < 1e-9);
        assert!(target.normalize().dot(position.normalize()) > 1.0 - 1e-12);
    }

    #[test]
    fn descent_phase_transitions() {
        let config = DescentGuidanceConfig::default();

        // Orbit -> DeorbitBurn (external)
        let p = advance_descent_phase(
            RocketMissionState::Orbit,
            400_000.0,
            7600.0,
            0.0,
            false,
            true,
            &config,
        );
        assert_eq!(p, RocketMissionState::Orbit); // External command needed

        // DeorbitBurn -> ReentryCorridor
        let p = advance_descent_phase(
            RocketMissionState::DeorbitBurn,
            100_000.0,
            7000.0,
            1000.0,
            true,
            true,
            &config,
        );
        assert_eq!(p, RocketMissionState::ReentryCorridor);

        // ReentryCorridor -> PoweredDescent
        let p = advance_descent_phase(
            RocketMissionState::ReentryCorridor,
            5_000.0,
            200.0,
            1000.0,
            true,
            true,
            &config,
        );
        assert_eq!(p, RocketMissionState::PoweredDescent);

        // PoweredDescent -> Landing
        let p = advance_descent_phase(
            RocketMissionState::PoweredDescent,
            50.0,
            1.0,
            100.0,
            true,
            true,
            &config,
        );
        assert_eq!(p, RocketMissionState::Landing);

        // ReentryCorridor -> UnpoweredDescent (no engines)
        let p = advance_descent_phase(
            RocketMissionState::ReentryCorridor,
            5_000.0,
            200.0,
            1000.0,
            true,
            false,
            &config,
        );
        assert_eq!(p, RocketMissionState::UnpoweredDescent);

        // A failed ascent below the entry interface must no longer keep its
        // ascent phase and full-throttle command while descending.
        let p = advance_descent_phase(
            RocketMissionState::Ascent,
            12_000.0,
            220.0,
            8_000.0,
            true,
            false,
            &config,
        );
        assert_eq!(p, RocketMissionState::UnpoweredDescent);
    }

    /// Item 2.9: a domain-level integration of the whole descent chain —
    /// deorbit targeting → reentry-corridor bank management → powered descent →
    /// terminal hover-slam/suicide-burn. Not a full 6-DOF flight, but it drives
    /// one representative state through every guidance phase and checks the
    /// physical ordering the phase logic depends on.
    #[test]
    fn full_descent_chain_deorbit_reentry_terminal() {
        const EARTH_MASS_KG: f64 = 5.97237e24;
        const EARTH_RADIUS_M: f64 = 6_371_000.0;
        let mu = gravitational_parameter(EARTH_MASS_KG);
        let config = DescentGuidanceConfig::default();
        let pad = DVec3::new(EARTH_RADIUS_M, 0.0, 0.0);
        let up = pad.normalize();

        // 1) Deorbit: circular LEO at 200 km. Burn must be positive delta-v and
        // retrograde (body +Y opposite the velocity vector).
        let r_orbit = EARTH_RADIUS_M + 200_000.0;
        let v_orbit = circular_orbit_speed_mps(EARTH_MASS_KG, r_orbit);
        let pos = DVec3::new(r_orbit, 0.0, 0.0);
        let vel = DVec3::new(0.0, 0.0, v_orbit);
        let target_periapsis = EARTH_RADIUS_M + config.entry_interface_altitude_m;
        let (dv, attitude) = deorbit_burn_targeting(pos, vel, target_periapsis, mu);
        assert!(dv > 0.0, "deorbit burn delta-v must be positive");
        let body_y = attitude * DVec3::Y;
        assert!(body_y.dot(vel) < 0.0, "deorbit burn must be retrograde");

        // 2) Reentry corridor: nominal state in the corridor ⇒ small crossrange
        // bank; violating g-load ⇒ bank to ~90° with the crossrange sign.
        let nominal = reentry_bank_angle(
            80_000.0, 7_000.0, 20_000.0, 300_000.0, 2.0, &config, 10_000.0,
        );
        assert!(nominal.abs() <= 30.0_f64.to_radians());

        let violating_g = reentry_bank_angle(
            80_000.0,
            7_000.0,
            20_000.0,
            300_000.0,
            config.max_g_load + 1.0,
            &config,
            10_000.0,
        );
        assert!(
            (violating_g.abs() - 90.0_f64.to_radians()).abs() < 1e-9,
            "violating g-load must bank to 90°, got {}°",
            violating_g.to_degrees()
        );
        assert!(
            violating_g < 0.0,
            "positive crossrange ⇒ left (negative) bank"
        );

        // 3) Powered descent (convex): a plausible initiation state — subsonic,
        // descending fast enough that braking is required. The command must be
        // bounded to the engine envelope and finite.
        let descent_pos = DVec3::new(EARTH_RADIUS_M + 1_500.0, 0.0, 0.0);
        let descent_vel = -up * 80.0; // falling at 80 m/s
        let (thrust, _) = powered_descent_guidance_convex(
            descent_pos,
            descent_vel,
            pad,
            40_000.0,
            8.0e5,
            3.0e5,
            20.0_f64.to_radians(),
            9.81,
            12.0,
        );
        assert!(thrust.length().is_finite());
        assert!(
            thrust.length() >= 3.0e5 - 1.0 && thrust.length() <= 8.0e5 + 1.0,
            "thrust {} outside engine envelope",
            thrust.length()
        );

        // 4) Terminal hover-slam brake: descending well below the target rate,
        // the commanded thrust must point up (brake the fall) and contain a
        // component opposing any horizontal drift (nulls it).
        let (h_thrust, _) = hover_slam_guidance(
            descent_pos,
            -up * 30.0 + DVec3::new(0.0, 0.0, 25.0), // descend + drift +Z
            pad,
            40_000.0,
            8.0e5,
            9.81,
            -1.0, // target descent rate
        );
        assert!(h_thrust.dot(up) > 0.0, "hover-slam must brake the fall");
        let horizontal_thrust = h_thrust - up * h_thrust.dot(up);
        assert!(
            horizontal_thrust.dot(DVec3::Z) < 0.0,
            "hover-slam must oppose the horizontal drift"
        );

        // 5) Suicide burn: gates on the computed arrest altitude — too high, no
        // burn; within it, burn. `up` is the radial (+X) direction at the pad.
        let (_, _, _, should_burn_high) =
            suicide_burn_guidance(pad + up * 60_000.0, -up * 100.0, pad, 40_000.0, 8.0e5, 9.81);
        assert!(
            !should_burn_high,
            "must not burn while far above the suicide altitude"
        );

        // ~100 m up at 50 m/s: 50²/(2·(20−9.81)) ≈ 123 m arrest altitude, so a
        // 100 m start is inside it and the burn must ignite.
        let (_, _, _, should_burn_near) =
            suicide_burn_guidance(pad + up * 100.0, -up * 50.0, pad, 40_000.0, 8.0e5, 9.81);
        assert!(
            should_burn_near,
            "must ignite once inside the suicide burn altitude"
        );
    }

    #[test]
    fn terminal_landing_guidance_keeps_main_thrust_above_the_horizon() {
        let up = DVec3::X;
        let target = up * 6_371_000.0;
        let position = target + up * 100.0;
        let (thrust, attitude) = terminal_landing_guidance(
            position,
            up * 12.0 + DVec3::Z * 8.0,
            target,
            100.0,
            40_000.0,
            800_000.0,
            9.81,
        );

        assert!(thrust.dot(up) > 0.0, "terminal thrust must remain upward");
        assert!(
            (attitude * DVec3::Y).angle_between(up) <= 12.0_f64.to_radians() + 1e-9,
            "terminal attitude must remain inside the landing tilt envelope"
        );
    }
}
