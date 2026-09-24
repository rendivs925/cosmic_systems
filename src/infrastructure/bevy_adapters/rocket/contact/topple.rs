//! Armed ground-contact topple: tip-over detection and attitude application.

use super::super::components::{
    LandingLegs, LandingScorecard, RocketGeometry, RocketMissionState, RocketPhysicsState,
    RocketPlanetBinding, TipOverState,
};
use crate::domain::services::landing_gear::{topple_critical_angle_rad, ToppleFall};
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::entity_components::PlanetComponent;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use bevy::log::info;
use bevy::math::{DMat3, DQuat, DVec3};
use bevy::prelude::{Query, Res};

/// Advance an armed ground-contact topple and apply its attitude to the
/// authoritative simulation state. This runs after contact resolution.
pub fn advance_topple(
    sim_time: Res<SimulationTime>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &mut RocketPhysicsState,
        &mut TipOverState,
        &mut RocketMissionState,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (binding, mut rocket, mut tip_over, mut mission_state) in rocket_query.iter_mut() {
        if tip_over.fall.is_none() {
            continue;
        }
        let com_height_m = tip_over.com_height_m;
        let Some(planet) = planet_query
            .iter()
            .find(|planet| planet.matches_body(&binding.planet_name))
        else {
            continue;
        };
        let Some(mu_m3_s2) =
            ephemeris_snapshot.gravitational_parameter_for_catalog_body(&planet.domain_planet.name)
        else {
            continue;
        };
        let radius_m = rocket.dynamics.position_m.length();
        if radius_m < 1.0 {
            continue;
        }
        let up_dir = rocket.dynamics.position_m / radius_m;
        let body_y = rocket.dynamics.orientation * DVec3::Y;
        let fall_dir_h = (body_y - up_dir * body_y.dot(up_dir)).normalize_or_zero();
        if fall_dir_h.length_squared() < 0.5 {
            continue;
        }

        let gravity_mps2 = mu_m3_s2 / radius_m.powi(2);
        let fall = tip_over.fall.as_mut().expect("armed above");
        let completed = fall.advance(gravity_mps2, com_height_m, dt);

        let y_new = up_dir * fall.tilt_rad.cos() + fall_dir_h * fall.tilt_rad.sin();
        let x_old = body_y.cross(y_new).cross(body_y).normalize_or_zero();
        let x_new = if x_old.length_squared() > 0.5 {
            x_old
        } else {
            fall_dir_h.cross(up_dir).normalize_or_zero()
        };
        if x_new.length_squared() < 0.5 {
            continue;
        }
        let z_new = x_new.cross(y_new);
        rocket.dynamics.orientation = DQuat::from_mat3(&DMat3::from_cols(
            x_new.normalize(),
            y_new,
            z_new.normalize(),
        ));

        if completed && *mission_state != RocketMissionState::Crashed {
            *mission_state = RocketMissionState::Crashed;
            info!("Vehicle toppled over; mission lost");
        }
    }
}

/// Center-of-mass height above the foot plane while grounded.
pub(crate) fn com_height_on_ground(legs: Option<&LandingLegs>, geometry: &RocketGeometry) -> f64 {
    match legs.filter(|legs| legs.deployed()) {
        Some(legs) => legs.gear.com_height_on_gear_m(geometry.height_m as f64),
        None => geometry.height_m as f64 / 2.0,
    }
}

/// Record the one-shot landing scorecard from the contact verdict.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_scorecard(
    scorecard: &mut LandingScorecard,
    descent_speed_mps: f64,
    lateral_speed_mps: f64,
    tilt_deg: f64,
    slope_deg: f64,
    position_m: DVec3,
    planet_radius_m: f64,
    target_position_m: DVec3,
    over_water: bool,
) {
    let sub_point = position_m.normalize_or_zero() * planet_radius_m;
    let distance_to_target_m = if target_position_m.length_squared() > 1.0 {
        (sub_point - target_position_m.normalize_or_zero() * planet_radius_m).length()
    } else {
        0.0
    };
    *scorecard = LandingScorecard {
        touchdown_vertical_speed_mps: descent_speed_mps,
        touchdown_lateral_speed_mps: lateral_speed_mps,
        touchdown_tilt_deg: tilt_deg,
        touchdown_slope_deg: slope_deg,
        distance_to_target_m,
        leg_compression_peak_m: scorecard.leg_compression_peak_m,
        over_water,
        recorded: true,
    };
}

/// Arm a topple immediately for a crashed vehicle beyond its critical lean.
pub(crate) fn arm_topple_if_leaning(
    tip_over: &mut TipOverState,
    legs: Option<&LandingLegs>,
    geometry: &RocketGeometry,
    tilt_deg: f64,
) -> bool {
    if tip_over.is_toppling() {
        return false;
    }
    let critical_rad = topple_critical_angle_rad(
        legs.filter(|legs| legs.deployed())
            .map(|legs| legs.gear.spec.base_radius_m)
            .unwrap_or(geometry.radius_m as f64),
        com_height_on_ground(legs, geometry),
    );
    let lean_rad = tilt_deg.to_radians();
    if critical_rad <= 0.0 || lean_rad <= critical_rad {
        return false;
    }
    tip_over.com_height_m = com_height_on_ground(legs, geometry);
    tip_over.fall = Some(ToppleFall::from_tilt(lean_rad));
    true
}

/// Arm a topple when a grounded vehicle sustains a beyond-critical lean.
pub(crate) fn monitor_grounded_topple(
    tip_over: &mut TipOverState,
    legs: Option<&LandingLegs>,
    geometry: &RocketGeometry,
    tilt_deg: f64,
    dt: f64,
) {
    const SUSTAINED_LEAN_DURATION_S: f64 = 0.5;

    if tip_over.is_toppling() {
        return;
    }
    let critical_rad = topple_critical_angle_rad(
        legs.filter(|legs| legs.deployed())
            .map(|legs| legs.gear.spec.base_radius_m)
            .unwrap_or(geometry.radius_m as f64),
        com_height_on_ground(legs, geometry),
    );
    let lean_rad = tilt_deg.to_radians();
    if critical_rad <= 0.0 || lean_rad <= critical_rad {
        tip_over.exceeded_for_s = 0.0;
        return;
    }

    tip_over.exceeded_for_s += dt;
    if tip_over.exceeded_for_s < SUSTAINED_LEAN_DURATION_S {
        return;
    }
    tip_over.com_height_m = com_height_on_ground(legs, geometry);
    tip_over.fall = Some(ToppleFall::from_tilt(lean_rad));
    info!(
        "Vehicle leaning {:.1} deg beyond the {:.1} deg critical angle; toppling",
        tilt_deg,
        critical_rad.to_degrees()
    );
}
