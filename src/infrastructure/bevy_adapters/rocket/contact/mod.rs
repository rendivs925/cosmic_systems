//! Terrain-contact preparation and constraint adapters.

use super::components::{
    DroneShipLandingTarget, GroundRest, LandingLegs, LandingScorecard, RecoveringStage,
    RocketAutopilot, RocketFlightConditions, RocketGeometry, RocketMissionState,
    RocketPhysicsState, RocketPlanetBinding, RocketPropulsion, TerrainCollisionState, TipOverState,
};
use super::events::{CrashEvent, LiftoffEvent, SplashdownDetectedEvent, TouchdownEvent};
use crate::domain::services::reference_frames::{
    body_fixed_to_planet_inertial_rotation, body_fixed_to_terrain_lat_lon, enu_basis,
    geodetic_to_body_fixed, geodetic_to_terrain_lat_lon, planet_inertial_to_body_fixed,
    surface_velocity_in_planet_inertial,
};
use crate::domain::services::rocket_dynamics::orientation_from_up_and_heading;
use crate::domain::services::rocket_propulsion::stage_thrust_body;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_collision::{
    decompose_velocity, evaluate_touchdown, liftoff_from_rest, resolve_resting_contact,
    sample_surface, GroundContact, TouchdownCriteria,
};
use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
use crate::infrastructure::bevy_adapters::entity_components::{PlanetComponent, PlanetTerrain};
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use bevy::ecs::query::QueryData;

mod sample_cache;
mod topple;

use bevy::log::info;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::{Entity, MessageWriter, Query, Res};
pub use sample_cache::TerrainSurfaceSampleCache;
pub use topple::advance_topple;
pub(crate) use topple::{arm_topple_if_leaning, monitor_grounded_topple, record_scorecard};

/// Bundled state required by the post-integration ground-contact authority.
/// Landing gear is optional so gear-less vehicles retain rigid point contact.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct GroundContactAccess {
    pub entity: Entity,
    pub binding: &'static RocketPlanetBinding,
    pub launch_site: Option<&'static LaunchSiteCoordinates>,
    pub recovering_stage: Option<&'static RecoveringStage>,
    pub dynamics: &'static mut RocketPhysicsState,
    pub propulsion: &'static mut RocketPropulsion,
    pub conditions: Option<&'static RocketFlightConditions>,
    pub geometry: &'static RocketGeometry,
    pub collision: &'static mut TerrainCollisionState,
    pub rest: &'static mut GroundRest,
    pub mission_state: &'static mut RocketMissionState,
    pub legs: Option<&'static mut LandingLegs>,
    pub tip_over: &'static mut TipOverState,
    pub scorecard: &'static mut LandingScorecard,
    pub autopilot: &'static mut RocketAutopilot,
    pub drone_ship_target: Option<&'static DroneShipLandingTarget>,
}

/// Advance the one-way gear-deployment latch before terrain-contact resolution.
pub fn deploy_landing_legs(
    mut rocket_query: Query<(
        &TerrainCollisionState,
        &RocketPhysicsState,
        &GroundRest,
        &mut LandingLegs,
    )>,
) {
    for (collision, rocket, ground_rest, mut legs) in rocket_query.iter_mut() {
        if ground_rest.active || collision.ground_contact == GroundContact::Landed {
            continue;
        }
        let radius_m = rocket.dynamics.position_m.length();
        if radius_m < 1.0 {
            continue;
        }
        let up_dir = rocket.dynamics.position_m / radius_m;
        let vertical_speed_mps = rocket.dynamics.velocity_mps.dot(up_dir);
        let deploy_gate_altitude_m = legs.deploy_gate_altitude_m();
        if legs.deployment.update(
            deploy_gate_altitude_m,
            collision.radar_altitude_m,
            vertical_speed_mps,
        ) {
            info!(
                "Landing legs deployed at {:.0} m AGL",
                collision.radar_altitude_m
            );
        }
    }
}

/// Authoritative rocket-terrain contact. Runs POST-integration in
/// [`RocketSet::GroundContact`], so verdicts and constraints act on the
/// just-integrated state: samples collision terrain, refreshes the
/// [`TerrainCollisionState`] sensors, evaluates multi-criteria touchdown,
/// enforces the resting-contact constraint (`resolve_resting_contact`:
/// penetration clamp + normal-velocity removal + tangential damping),
/// releases rest when thrust exceeds weight, and emits splashdown on water
/// touchdowns exactly as before.
#[expect(
    clippy::too_many_arguments,
    reason = "Post-integration contact resolution reads shared time, ephemeris, surface cache, the four domain message channels, and the cohesive ground-contact query."
)]
pub fn resolve_ground_contact(
    sim_time: Res<SimulationTime>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    surface_cache: Option<Res<TerrainSurfaceSampleCache>>,
    mut splashdown_writer: MessageWriter<SplashdownDetectedEvent>,
    mut liftoff_writer: MessageWriter<LiftoffEvent>,
    mut touchdown_writer: MessageWriter<TouchdownEvent>,
    mut crash_writer: MessageWriter<CrashEvent>,
    planet_query: Query<(Entity, &PlanetComponent, &PlanetTerrain)>,
    mut rocket_query: Query<GroundContactAccess>,
) {
    let dt = sim_time.fixed_timestep();

    for mut access in rocket_query.iter_mut() {
        let rocket_entity = access.entity;
        let binding = access.binding;
        let propulsion = &mut *access.propulsion;
        let ambient_pressure_pa = access
            .conditions
            .map(|conditions| conditions.ambient_pressure_pa)
            .unwrap_or(0.0);
        let geometry = access.geometry;
        let autopilot = &mut *access.autopilot;
        if access
            .drone_ship_target
            .is_some_and(|target| target.deck_contact)
        {
            // Moving-deck contact is resolved by rocket_recovery in the deck's
            // own frame. Terrain must not re-apply a static-world constraint.
            continue;
        }
        let rocket = &mut *access.dynamics;
        let collision = &mut *access.collision;
        let rest = &mut *access.rest;
        let mission_state = &mut *access.mission_state;
        let mut legs = access.legs.as_deref_mut();
        let tip_over = &mut *access.tip_over;
        let scorecard = &mut *access.scorecard;
        let Some((planet_entity, planet, planet_terrain)) = planet_query
            .iter()
            .find(|(_, planet, _)| planet.matches_body(&binding.planet_name))
        else {
            continue;
        };
        let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
        let Some(mu_m3_s2) =
            ephemeris_snapshot.gravitational_parameter_for_catalog_body(&planet.domain_planet.name)
        else {
            continue;
        };

        let lower_extent_body_m = geometry.lower_extent_body_m();
        let lower_offset_world_m = rocket.dynamics.orientation * lower_extent_body_m;
        let contact_position_m = rocket.dynamics.position_m + lower_offset_world_m;
        let rotating_surface = access.launch_site.is_some() || access.recovering_stage.is_some();
        let orientation =
            ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name);
        if rotating_surface && orientation.is_none() {
            continue;
        }
        let position_bf = orientation
            .filter(|_| rotating_surface)
            .map_or(contact_position_m, |orientation| {
                planet_inertial_to_body_fixed(contact_position_m, orientation)
            });
        let dir_bf = position_bf.normalize_or_zero();
        if dir_bf.length_squared() < 1e-12 {
            continue;
        }
        let (lat, lon) = body_fixed_to_terrain_lat_lon(dir_bf);
        let sample = surface_cache.as_deref().map_or_else(
            || sample_surface(planet_terrain.source.as_ref(), lat, lon, radius_m),
            |cache| {
                cache.sample(
                    planet_entity,
                    planet_terrain.source.as_ref(),
                    lat,
                    lon,
                    radius_m,
                )
            },
        );
        let surface_radius_m = radius_m + sample.height_m;
        let signed_altitude_m = contact_position_m.length() - surface_radius_m;

        collision.radar_altitude_m = signed_altitude_m.max(0.0);
        collision.slope_deg = sample.slope_deg;
        collision.over_water = planet.domain_planet.has_ocean && sample.height_m < 0.0;

        let body_to_inertial = orientation
            .filter(|_| rotating_surface)
            .map_or(DQuat::IDENTITY, body_fixed_to_planet_inertial_rotation);
        let normal = if sample.normal.length_squared() > 1e-12 {
            body_to_inertial * sample.normal
        } else {
            body_to_inertial * dir_bf
        };
        let tilt_deg = (rocket.dynamics.orientation * DVec3::Y)
            .angle_between(normal)
            .to_degrees();
        let surface_velocity = orientation
            .filter(|_| rotating_surface)
            .map_or(DVec3::ZERO, |orientation| {
                surface_velocity_in_planet_inertial(contact_position_m, orientation)
            });
        let angular_velocity_world_radps =
            rocket.dynamics.orientation * rocket.dynamics.angular_velocity_radps;
        let velocity = rocket.dynamics.velocity_mps
            + angular_velocity_world_radps.cross(lower_offset_world_m)
            - surface_velocity;
        let components = decompose_velocity(velocity, normal);

        if *mission_state == RocketMissionState::PreLaunch {
            if let Some(launch_site) = access.launch_site {
                let pad_direction_bf =
                    geodetic_to_body_fixed(launch_site, &planet.domain_planet).normalize();
                let (pad_latitude_deg, pad_longitude_deg) =
                    geodetic_to_terrain_lat_lon(launch_site, &planet.domain_planet);
                let pad_sample = surface_cache.as_deref().map_or_else(
                    || {
                        sample_surface(
                            planet_terrain.source.as_ref(),
                            pad_latitude_deg,
                            pad_longitude_deg,
                            radius_m,
                        )
                    },
                    |cache| {
                        cache.sample(
                            planet_entity,
                            planet_terrain.source.as_ref(),
                            pad_latitude_deg,
                            pad_longitude_deg,
                            radius_m,
                        )
                    },
                );
                let pad_position_m =
                    body_to_inertial * (pad_direction_bf * (radius_m + pad_sample.height_m));
                let pad_normal = (body_to_inertial * pad_sample.normal).normalize_or_zero();

                let (_, pad_north_bf, _) =
                    enu_basis(launch_site.latitude_deg, launch_site.longitude_deg);
                let pad_orientation =
                    orientation_from_up_and_heading(pad_normal, body_to_inertial * pad_north_bf)
                        .expect("nonpolar launch pad must define a surface heading");
                rocket.dynamics.position_m = pad_position_m - pad_orientation * lower_extent_body_m;
                rocket.dynamics.velocity_mps = surface_velocity_in_planet_inertial(
                    rocket.dynamics.position_m,
                    orientation.expect("launch site requires orientation"),
                );
                rocket.dynamics.orientation = pad_orientation;
                rocket.dynamics.angular_velocity_radps = DVec3::ZERO;
                rest.active = true;
                collision.radar_altitude_m = 0.0;
                collision.slope_deg = pad_sample.slope_deg;
                collision.over_water = planet.domain_planet.has_ocean && pad_sample.height_m < 0.0;
                collision.ground_contact = GroundContact::Landed;
                tip_over.exceeded_for_s = 0.0;
                tip_over.fall = None;
                continue;
            }
        }

        let criteria = match legs.as_ref() {
            Some(legs) if legs.deployed() => legs
                .gear
                .touchdown_criteria(TouchdownCriteria::default(), geometry.height_m as f64),
            _ => TouchdownCriteria::default(),
        };

        if rest.active {
            let gravity_mps2 = mu_m3_s2 / rocket.dynamics.position_m.length().powi(2);
            let weight_n = rocket.dynamics.mass_kg * gravity_mps2;
            let upward_thrust_n = propulsion
                .running_core_stage()
                .map(|(active_core_stage, throttle)| {
                    let thrust_body = stage_thrust_body(
                        &active_core_stage.stage().engines,
                        throttle,
                        ambient_pressure_pa,
                    )
                    .0;
                    (rocket.dynamics.orientation * thrust_body)
                        .dot(normal)
                        .max(0.0)
                })
                .unwrap_or(0.0);
            if liftoff_from_rest(upward_thrust_n, weight_n) {
                rest.active = false;
                collision.ground_contact = GroundContact::None;
                liftoff_writer.write(LiftoffEvent {
                    rocket: rocket_entity,
                    position_m: contact_position_m,
                    upward_thrust_n,
                    weight_n,
                });
                bevy::log::info!(
                    "Liftoff: upward thrust {:.0} N exceeds weight {:.0} N, released from surface",
                    upward_thrust_n,
                    weight_n
                );
                continue;
            }
        }

        if rest.active {
            match legs.as_mut().filter(|legs| legs.deployed()).map(|legs| {
                let penetration_m = (-signed_altitude_m).max(0.0);
                (
                    legs.gear.resolve_contact_step(
                        velocity,
                        normal,
                        penetration_m,
                        rocket.dynamics.mass_kg,
                        dt,
                    ),
                    legs,
                )
            }) {
                Some((outcome, legs)) if outcome.bottomed_out => {
                    bevy::log::warn!("Landing gear bottomed out; rigid contact engaged");
                    let res = resolve_resting_contact(
                        contact_position_m,
                        velocity,
                        surface_radius_m,
                        normal,
                        dt,
                    );
                    rocket.dynamics.position_m = res.position_m - lower_offset_world_m;
                    rocket.dynamics.velocity_mps = res.velocity_mps + surface_velocity
                        - angular_velocity_world_radps.cross(lower_offset_world_m);
                    legs.compression_m = legs.gear.spec.stroke_m;
                }
                Some((outcome, legs)) => {
                    rocket.dynamics.velocity_mps = outcome.velocity_mps + surface_velocity;
                    legs.compression_m = outcome.compression_m;
                    scorecard.leg_compression_peak_m =
                        scorecard.leg_compression_peak_m.max(outcome.compression_m);
                }
                None => {
                    let res = resolve_resting_contact(
                        contact_position_m,
                        velocity,
                        surface_radius_m,
                        normal,
                        dt,
                    );
                    rocket.dynamics.position_m = res.position_m - lower_offset_world_m;
                    rocket.dynamics.velocity_mps = res.velocity_mps + surface_velocity
                        - angular_velocity_world_radps.cross(lower_offset_world_m);
                }
            }
            collision.ground_contact = GroundContact::Landed;
            if !tip_over.is_toppling() {
                rocket.dynamics.angular_velocity_radps = DVec3::ZERO;
                rocket.dynamics.angular_acceleration_radps2 = DVec3::ZERO;
            }
            monitor_grounded_topple(tip_over, legs.as_deref(), geometry, tilt_deg, dt);
            continue;
        }

        if signed_altitude_m < 0.0 {
            let radial_dir = contact_position_m.normalize_or_zero();
            rocket.dynamics.position_m = radial_dir * surface_radius_m - lower_offset_world_m;
            let into_ground = velocity.dot(normal).min(0.0);
            rocket.dynamics.velocity_mps = velocity - normal * into_ground + surface_velocity
                - angular_velocity_world_radps.cross(lower_offset_world_m);
        }

        // A terminal verdict is valid only at the sampled contact plane. The
        // previous three-metre band could mark a descending vehicle as Landed
        // while it was still airborne; fixed-step penetration is projected onto
        // this plane immediately above before the verdict is evaluated.
        if signed_altitude_m > 0.0 || components.normal_mps > 0.0 {
            collision.ground_contact = GroundContact::None;
            continue;
        }

        let mut verdict = evaluate_touchdown(
            -components.normal_mps,
            components.lateral_mps,
            sample.slope_deg,
            tilt_deg,
            &criteria,
        );
        if verdict == GroundContact::Landed
            && legs
                .as_ref()
                .filter(|legs| legs.deployed())
                .is_some_and(|legs| {
                    !legs.gear.supports_touchdown(
                        rocket.dynamics.mass_kg,
                        (-components.normal_mps).max(0.0),
                    )
                })
        {
            bevy::log::warn!(
                "Landing gear capacity exceeded at touchdown: mass {:.0} kg, descent {:.2} m/s",
                rocket.dynamics.mass_kg,
                -components.normal_mps
            );
            verdict = GroundContact::Crash;
        }
        collision.ground_contact = verdict;

        match verdict {
            GroundContact::Landed => {
                rest.active = true;
                bevy::log::info!(
                    "Touchdown at ({lat:.2}, {lon:.2}): descent {:.2} m/s, lateral {:.2} m/s, slope {:.1} deg, tilt {:.1} deg{}",
                    -components.normal_mps,
                    components.lateral_mps,
                    sample.slope_deg,
                    tilt_deg,
                    if collision.over_water { " (water)" } else { "" }
                );
                record_scorecard(
                    scorecard,
                    -components.normal_mps,
                    components.lateral_mps,
                    tilt_deg,
                    sample.slope_deg,
                    contact_position_m,
                    radius_m,
                    autopilot.target_landing_position_m,
                    collision.over_water,
                );
                if legs.as_ref().filter(|legs| legs.deployed()).is_none() {
                    let res = resolve_resting_contact(
                        contact_position_m,
                        velocity,
                        surface_radius_m,
                        normal,
                        dt,
                    );
                    rocket.dynamics.position_m = res.position_m - lower_offset_world_m;
                    rocket.dynamics.velocity_mps = res.velocity_mps + surface_velocity
                        - angular_velocity_world_radps.cross(lower_offset_world_m);
                }
                // Point-contact terrain has no contact-torque model. Arresting
                // free rotation on an accepted supported landing prevents the
                // integrator from rotating a resting vehicle through the ground.
                // A beyond-support lean still enters the existing topple model.
                rocket.dynamics.angular_velocity_radps = DVec3::ZERO;
                rocket.dynamics.angular_acceleration_radps2 = DVec3::ZERO;

                if matches!(
                    *mission_state,
                    RocketMissionState::PoweredDescent
                        | RocketMissionState::UnpoweredDescent
                        | RocketMissionState::Landing
                        | RocketMissionState::ReentryCorridor
                ) {
                    *mission_state = RocketMissionState::Landed;
                    autopilot.integral = DVec3::ZERO;
                    propulsion.throttle = 0.0;
                    propulsion.gimbal_pitch_rad = 0.0;
                    propulsion.gimbal_yaw_rad = 0.0;
                    if collision.over_water {
                        splashdown_writer.write(SplashdownDetectedEvent {
                            rocket: rocket_entity,
                            position_m: contact_position_m,
                            touchdown_vertical_speed_mps: -components.normal_mps,
                        });
                        bevy::log::info!(
                            "Splashdown detected at ({lat:.2}, {lon:.2}), vertical speed {:.1} m/s",
                            -components.normal_mps
                        );
                    } else {
                        touchdown_writer.write(TouchdownEvent {
                            rocket: rocket_entity,
                            position_m: contact_position_m,
                            vertical_speed_mps: -components.normal_mps,
                            lateral_speed_mps: components.lateral_mps,
                            tilt_deg,
                            slope_deg: sample.slope_deg,
                        });
                    }
                }
            }
            GroundContact::Crash => {
                if *mission_state != RocketMissionState::PreLaunch {
                    *mission_state = RocketMissionState::Crashed;
                    crash_writer.write(CrashEvent {
                        rocket: rocket_entity,
                        position_m: contact_position_m,
                        vertical_speed_mps: -components.normal_mps,
                        tilt_deg,
                    });
                    record_scorecard(
                        scorecard,
                        -components.normal_mps,
                        components.lateral_mps,
                        tilt_deg,
                        sample.slope_deg,
                        contact_position_m,
                        radius_m,
                        autopilot.target_landing_position_m,
                        collision.over_water,
                    );
                    arm_topple_if_leaning(tip_over, legs.as_deref(), geometry, tilt_deg);
                }
            }
            GroundContact::None => {}
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::items_after_test_module,
    reason = "Ground-contact regression tests are kept beside the contact resolver they exercise."
)]
mod tests {
    use super::super::components::RocketRenderState;
    use super::*;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::terrain_collision::decompose_velocity;
    use crate::domain::services::terrain_source::ProceduralTerrainSource;
    use bevy::math::{DMat3, DQuat, DVec3};

    #[test]
    fn cached_surface_samples_are_exactly_identical_to_the_authoritative_source() {
        let source = ProceduralTerrainSource::new(7, 1_000.0, 500.0, 0);
        let cache = TerrainSurfaceSampleCache::default();
        let latitude_deg = 28.5721;
        let longitude_deg = -80.6480;
        let radius_m = 6_371_000.0;
        let direct = sample_surface(&source, latitude_deg, longitude_deg, radius_m);
        let cached = cache.sample(
            Entity::PLACEHOLDER,
            &source,
            latitude_deg,
            longitude_deg,
            radius_m,
        );
        let repeated = cache.sample(
            Entity::PLACEHOLDER,
            &source,
            latitude_deg,
            longitude_deg,
            radius_m,
        );

        assert_eq!(cached, direct);
        assert_eq!(repeated, direct);
    }

    #[test]
    fn interpolation_leaves_fixed_contact_verdict_deterministic() {
        let previous = RocketDynamicsState::new(
            DVec3::new(0.0, 100.0, 0.0),
            DVec3::new(1.0, -2.0, 0.0),
            DQuat::IDENTITY,
            1_000.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let current = RocketDynamicsState::new(
            DVec3::new(0.0, 99.0, 0.0),
            DVec3::new(1.0, -4.0, 0.0),
            DQuat::IDENTITY,
            1_000.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let render = RocketRenderState {
            prev: previous,
            current,
        };
        let velocity = decompose_velocity(render.current.velocity_mps, DVec3::Y);
        let criteria = TouchdownCriteria::default();

        assert_eq!(
            evaluate_touchdown(
                -velocity.normal_mps,
                velocity.lateral_mps,
                0.0,
                0.0,
                &criteria
            ),
            GroundContact::Landed
        );
        assert_eq!(render.current, current);
    }
}
