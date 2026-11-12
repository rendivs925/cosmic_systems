//! ECS adapters for autonomous drone-ship recovery.
//!
//! The domain recovery models own prediction and station-keeping control. This
//! module integrates their f64 state and applies a post-integration deck
//! constraint, analogous to terrain resting contact, without touching render
//! transforms.

use super::components::{
    DroneShip, DroneShipLandingTarget, GroundRest, LandingLegs, LandingScorecard,
    RecoveryDeckAnchor, RocketAutopilot, RocketGeometry, RocketMissionState, RocketPhysicsState,
    TerrainCollisionState,
};
use super::contact::record_scorecard;
use super::events::{CrashEvent, TouchdownEvent};
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::services::recovery::{DroneShip as DomainDroneShip, StationKeeper};
use crate::domain::services::reference_frames::{
    body_fixed_to_planet_inertial, body_fixed_to_planet_inertial_rotation,
    surface_velocity_in_planet_inertial,
};
use crate::domain::services::rocket_dynamics::orientation_from_up_and_heading;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_collision::{
    decompose_velocity, evaluate_touchdown, GroundContact, TouchdownCriteria,
};
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::domain::value_objects::recovery_deck::RecoveryDeckCoordinates;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::terrain::render::RenderOrigin;
use bevy::asset::Assets;
use bevy::color::Color;
use bevy::log::info;
use bevy::math::primitives::Cuboid;
use bevy::math::DVec3;
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::prelude::{
    Commands, Entity, Mesh, Mesh3d, MessageWriter, Name, Query, Res, ResMut, Resource, Transform,
    Without,
};

/// Requested body-fixed recovery deck, parsed from the launch options. The
/// latitude/longitude are terrain-radial; the deck is a floating platform on
/// the sea surface, not terrain.
#[derive(Resource, Debug, Clone, Copy)]
pub struct RecoveryDeckRequest(pub RecoveryDeckCoordinates);

/// The spawned deck entity, if a recovery deck was requested. Staging reads it
/// to attach the landing target to the recovering booster.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct RecoveryDeckEntity(pub Option<Entity>);

/// A 50 m square deck (Falcon 9 drone ships are roughly 91 m x 52 m; this is
/// the conservative inscribed square). The rocket must touch down inside it.
const RECOVERY_DECK_HALF_EXTENT_M: f64 = 25.0;
/// Representative loaded drone-ship mass; station keeping is not exercised by
/// a body-fixed deck, but the domain state needs a finite positive mass.
const RECOVERY_DECK_MASS_KG: f64 = 1.0e7;

/// Spawn the requested deck once. The authoritative position is set on the
/// first recovery tick by [`anchor_recovery_decks`], so startup does not depend
/// on the ephemeris evaluation order.
pub fn spawn_recovery_decks(mut commands: Commands, request: Option<Res<RecoveryDeckRequest>>) {
    let Some(request) = request else {
        return;
    };
    let Some(planet) = PlanetFactory::create_by_id(&CelestialBodyId::earth()) else {
        bevy::log::warn!("Recovery deck requires the Earth catalog entry; no deck spawned");
        return;
    };
    // Terrain heights and the launch pad are radial offsets from the catalog
    // mean radius, not WGS-84 ellipsoid heights. The deck coordinate is
    // terrain-radial, so the sea surface is exactly that fixed radius.
    let sea_surface_m = planet.radius_km as f64 * 1_000.0;
    let position_body_fixed_m = request.0.position_body_fixed(sea_surface_m);
    let entity = commands
        .spawn((
            DroneShip {
                state: DomainDroneShip {
                    position_m: DVec3::ZERO,
                    velocity_mps: DVec3::ZERO,
                    external_accel_mps2: DVec3::ZERO,
                    mass_kg: RECOVERY_DECK_MASS_KG,
                },
                station_target_position_m: DVec3::ZERO,
                station_keeper: StationKeeper {
                    kp: 0.05,
                    kd: 0.2,
                    max_thrust_n: 5.0e6,
                },
                deck_half_extent_m: RECOVERY_DECK_HALF_EXTENT_M,
            },
            RecoveryDeckAnchor {
                planet_name: CelestialBodyId::earth(),
                position_body_fixed_m,
            },
            Name::new("RecoveryDeck"),
        ))
        .id();
    commands.insert_resource(RecoveryDeckEntity(Some(entity)));
    info!(
        "Recovery deck spawned at body-fixed ({:.1}, {:.1}, {:.1}) m for {:.4}, {:.4}",
        position_body_fixed_m.x,
        position_body_fixed_m.y,
        position_body_fixed_m.z,
        request.0.latitude_deg,
        request.0.longitude_deg
    );
}

/// Refresh a body-fixed deck's inertial state from the shared ephemeris. The
/// deck is an ocean platform holding station, so its authoritative position
/// rotates with the body; guidance and deck contact read this state and never
/// re-derive their own deck coordinates.
pub fn anchor_recovery_decks(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    mut decks: Query<(&RecoveryDeckAnchor, &mut DroneShip)>,
) {
    for (anchor, mut deck) in &mut decks {
        let Some(orientation) =
            ephemeris_snapshot.orientation_for_catalog_body(anchor.planet_name.as_str())
        else {
            continue;
        };
        let position_m = body_fixed_to_planet_inertial(anchor.position_body_fixed_m, orientation);
        deck.state.position_m = position_m;
        deck.state.velocity_mps = surface_velocity_in_planet_inertial(position_m, orientation);
        deck.state.external_accel_mps2 = DVec3::ZERO;
        deck.station_target_position_m = position_m;
    }
}

/// Presentation asset for a recovery deck. The simulated deck is authoritative;
/// this mesh follows its body-fixed anchor through the same render-origin
/// boundary as terrain and the launch pad.
pub fn ensure_recovery_deck_presentation(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    decks: Query<(Entity, &RecoveryDeckAnchor), Without<Mesh3d>>,
) {
    let mesh = meshes.add(Cuboid::new(50.0, 0.5, 50.0));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.36, 0.38, 0.41),
        perceptual_roughness: 0.72,
        metallic: 0.2,
        ..Default::default()
    });
    for (entity, _) in &decks {
        commands.entity(entity).insert((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            Name::new("RecoveryDeckVisual"),
        ));
    }
}

/// Place the deck mesh at the sea surface and orient its top face along the
/// local up. The cuboid is half a metre thick, so its center sits a quarter
/// metre below the contact plane.
pub fn sync_recovery_deck_presentation(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    render_origin: Res<RenderOrigin>,
    physical_scale: Res<PhysicalScale>,
    mut decks: Query<(&RecoveryDeckAnchor, &mut Transform)>,
) {
    for (anchor, mut transform) in &mut decks {
        let Some(orientation) =
            ephemeris_snapshot.orientation_for_catalog_body(anchor.planet_name.as_str())
        else {
            continue;
        };
        let body_to_inertial = body_fixed_to_planet_inertial_rotation(orientation);
        let up_body_fixed = anchor.position_body_fixed_m.normalize_or_zero();
        let north_body_fixed = (DVec3::Y - up_body_fixed * up_body_fixed.y).normalize_or_zero();
        let Some(attitude) = orientation_from_up_and_heading(
            body_to_inertial * up_body_fixed,
            body_to_inertial * north_body_fixed,
        ) else {
            continue;
        };
        let local_m = body_to_inertial * anchor.position_body_fixed_m - render_origin.origin;
        transform.translation = DVec3::new(
            physical_scale.flight_meters_to_units(local_m.x),
            physical_scale.flight_meters_to_units(local_m.y),
            physical_scale.flight_meters_to_units(local_m.z),
        )
        .as_vec3()
            - (attitude * DVec3::Y).as_vec3() * 0.25;
        transform.rotation = attitude.as_quat();
    }
}

/// Integrate each vessel from its bounded station-keeping thrust. The domain
/// controller supplies the command; this adapter owns only ECS state updates.
/// Body-fixed decks are skipped: they are anchored by the rotating surface, and
/// integrating an inertial station-keeper against a rotating target would
/// command a spurious hundreds-of-metres-per-second correction.
pub fn station_keep_drone_ships(
    sim_time: Res<SimulationTime>,
    mut ships: Query<&mut DroneShip, Without<RecoveryDeckAnchor>>,
) {
    let dt = sim_time.fixed_timestep();
    for mut ship in &mut ships {
        if ship.state.mass_kg <= 0.0 || !ship.state.mass_kg.is_finite() {
            continue;
        }
        let thrust_n = ship
            .station_keeper
            .thrust(&ship.state, ship.station_target_position_m);
        let acceleration_mps2 = ship.state.external_accel_mps2 + thrust_n / ship.state.mass_kg;
        ship.state.velocity_mps += acceleration_mps2 * dt;
        let velocity_mps = ship.state.velocity_mps;
        ship.state.position_m += velocity_mps * dt;
    }
}

/// Resolve a stage against a drone ship's moving deck after 6-DOF integration.
/// The contact verdict is entirely deck-relative: both velocity and lateral
/// error are measured in the deck frame, then the normal constraint latches
/// the authoritative rocket state to the vessel rather than terrain.
#[expect(
    clippy::type_complexity,
    reason = "The recovery query combines cohesive rocket state for landing-target guidance."
)]
pub fn resolve_drone_ship_deck_contact(
    sim_time: Res<SimulationTime>,
    mut touchdown_writer: MessageWriter<TouchdownEvent>,
    mut crash_writer: MessageWriter<CrashEvent>,
    ships: Query<&DroneShip>,
    mut rockets: Query<(
        Entity,
        &mut DroneShipLandingTarget,
        &mut RocketPhysicsState,
        &RocketGeometry,
        &mut TerrainCollisionState,
        &mut GroundRest,
        &mut RocketMissionState,
        Option<&LandingLegs>,
        &mut LandingScorecard,
        &RocketAutopilot,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (
        rocket_entity,
        mut target,
        mut rocket,
        geometry,
        mut collision,
        mut rest,
        mut mission,
        legs,
        mut scorecard,
        autopilot,
    ) in &mut rockets
    {
        let Ok(ship) = ships.get(target.drone_ship) else {
            continue;
        };
        let normal = ship.state.position_m.normalize_or_zero();
        if normal.length_squared() < 0.5 {
            continue;
        }
        let lower_offset_world_m = rocket.dynamics.orientation * geometry.lower_extent_body_m();
        let contact_position_m = rocket.dynamics.position_m + lower_offset_world_m;
        let relative_position_m = contact_position_m - ship.state.position_m;
        let deck_altitude_m = relative_position_m.dot(normal);
        let deck_lateral_m = relative_position_m - normal * deck_altitude_m;
        let relative_velocity_mps = rocket.dynamics.velocity_mps
            + (rocket.dynamics.orientation * rocket.dynamics.angular_velocity_radps)
                .cross(lower_offset_world_m)
            - ship.state.velocity_mps;

        if target.deck_contact {
            // The deck is a moving resting-contact frame. Remove normal motion
            // and damp lateral slip deterministically while following its drift.
            rocket.dynamics.position_m -= normal * deck_altitude_m;
            let normal_speed_mps = relative_velocity_mps.dot(normal);
            let tangential_mps = relative_velocity_mps - normal * normal_speed_mps;
            rocket.dynamics.velocity_mps =
                ship.state.velocity_mps + tangential_mps * (-12.0 * dt).exp();
            rocket.dynamics.angular_velocity_radps = DVec3::ZERO;
            rocket.dynamics.angular_acceleration_radps2 = DVec3::ZERO;
            rest.active = true;
            collision.radar_altitude_m = 0.0;
            collision.slope_deg = 0.0;
            collision.over_water = false;
            collision.ground_contact = GroundContact::Landed;
            continue;
        }

        if deck_lateral_m
            .x
            .abs()
            .max(deck_lateral_m.y.abs())
            .max(deck_lateral_m.z.abs())
            > ship.deck_half_extent_m
            || deck_altitude_m > 0.0
        {
            continue;
        }

        let components = decompose_velocity(relative_velocity_mps, normal);
        if components.normal_mps > 0.0 {
            continue;
        }
        let tilt_deg = (rocket.dynamics.orientation * DVec3::Y)
            .angle_between(normal)
            .to_degrees();
        let criteria = match legs.filter(|legs| legs.deployed()) {
            Some(legs) => legs
                .gear
                .touchdown_criteria(TouchdownCriteria::default(), geometry.height_m as f64),
            None => TouchdownCriteria::default(),
        };
        let mut verdict = evaluate_touchdown(
            -components.normal_mps,
            components.lateral_mps,
            0.0,
            tilt_deg,
            &criteria,
        );
        if verdict == GroundContact::Landed
            && legs.filter(|legs| legs.deployed()).is_some_and(|legs| {
                !legs
                    .gear
                    .supports_touchdown(rocket.dynamics.mass_kg, (-components.normal_mps).max(0.0))
            })
        {
            bevy::log::warn!(
                "Landing gear capacity exceeded at drone-ship touchdown: mass {:.0} kg, descent {:.2} m/s",
                rocket.dynamics.mass_kg,
                -components.normal_mps
            );
            verdict = GroundContact::Crash;
        }
        collision.ground_contact = verdict;
        match verdict {
            GroundContact::Landed => {
                rocket.dynamics.position_m -= normal * deck_altitude_m;
                rocket.dynamics.velocity_mps = ship.state.velocity_mps + relative_velocity_mps
                    - normal * relative_velocity_mps.dot(normal);
                rocket.dynamics.angular_velocity_radps = DVec3::ZERO;
                rocket.dynamics.angular_acceleration_radps2 = DVec3::ZERO;
                rest.active = true;
                target.deck_contact = true;
                collision.radar_altitude_m = 0.0;
                collision.slope_deg = 0.0;
                collision.over_water = false;
                record_scorecard(
                    &mut scorecard,
                    -components.normal_mps,
                    components.lateral_mps,
                    tilt_deg,
                    0.0,
                    rocket.dynamics.position_m,
                    ship.state.position_m.length(),
                    autopilot.target_landing_position_m,
                    false,
                );
                if matches!(
                    *mission,
                    RocketMissionState::PoweredDescent
                        | RocketMissionState::UnpoweredDescent
                        | RocketMissionState::Landing
                        | RocketMissionState::ReentryCorridor
                ) {
                    *mission = RocketMissionState::Landed;
                    touchdown_writer.write(TouchdownEvent {
                        rocket: rocket_entity,
                        position_m: rocket.dynamics.position_m,
                        vertical_speed_mps: -components.normal_mps,
                        lateral_speed_mps: components.lateral_mps,
                        tilt_deg,
                        slope_deg: 0.0,
                    });
                }
            }
            GroundContact::Crash => {
                if *mission != RocketMissionState::PreLaunch
                    && *mission != RocketMissionState::Crashed
                {
                    info!(
                        "Deck crash: descent {:.2} m/s, lateral {:.2} m/s, tilt {:.1} deg",
                        -components.normal_mps, components.lateral_mps, tilt_deg
                    );
                    record_scorecard(
                        &mut scorecard,
                        -components.normal_mps,
                        components.lateral_mps,
                        tilt_deg,
                        0.0,
                        rocket.dynamics.position_m,
                        ship.state.position_m.length(),
                        autopilot.target_landing_position_m,
                        false,
                    );
                    *mission = RocketMissionState::Crashed;
                    crash_writer.write(CrashEvent {
                        rocket: rocket_entity,
                        position_m: rocket.dynamics.position_m,
                        vertical_speed_mps: -components.normal_mps,
                        tilt_deg,
                    });
                }
            }
            GroundContact::None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::body_orientation::BodyOrientation;
    use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
    use crate::domain::services::gravity::gravitational_parameter;
    use crate::domain::services::simulation_time::DEFAULT_FIXED_TIMESTEP_S;
    use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
    use bevy::math::DQuat;
    use bevy::prelude::{App, MinimalPlugins, Startup, Update};

    fn earth_orientation_at(rotation: DQuat) -> BodyOrientation {
        BodyOrientation::from_kernel(
            NaifBodyId::EARTH,
            TdbEpoch::j2000(),
            "recovery-deck-test".to_owned(),
            rotation,
            DVec3::Z * (std::f64::consts::TAU / (23.934 * 3_600.0)),
        )
    }

    fn earth_snapshot(orientation: BodyOrientation) -> EphemerisSnapshot {
        let earth_mass_kg = PlanetFactory::create_by_name("Earth")
            .expect("Earth exists")
            .mass_kg;
        EphemerisSnapshot::from_states_orientations_and_gravitational_parameters(
            Vec::new(),
            vec![orientation],
            vec![(NaifBodyId::EARTH, gravitational_parameter(earth_mass_kg))],
        )
    }

    #[test]
    fn body_fixed_deck_state_rotates_with_the_shared_ephemeris() {
        let orientation = earth_orientation_at(DQuat::IDENTITY);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(earth_snapshot(orientation.clone()));
        app.insert_resource(SimulationTime::new(DEFAULT_FIXED_TIMESTEP_S));
        app.add_systems(Update, anchor_recovery_decks);
        let position_bf = DVec3::new(6_378_137.0, 0.0, 0.0);
        app.world_mut().spawn((
            DroneShip {
                state: DomainDroneShip {
                    position_m: DVec3::ZERO,
                    velocity_mps: DVec3::ZERO,
                    external_accel_mps2: DVec3::ZERO,
                    mass_kg: 1.0e7,
                },
                station_target_position_m: DVec3::ZERO,
                station_keeper: StationKeeper {
                    kp: 0.05,
                    kd: 0.2,
                    max_thrust_n: 5.0e6,
                },
                deck_half_extent_m: 25.0,
            },
            RecoveryDeckAnchor {
                planet_name: CelestialBodyId::earth(),
                position_body_fixed_m: position_bf,
            },
        ));

        app.update();
        let first = {
            let world = app.world_mut();
            let mut query = world.query::<&DroneShip>();
            query.single(world).expect("one deck").state.position_m
        };
        let expected = body_fixed_to_planet_inertial(position_bf, &orientation);
        assert!((first - expected).length() < 1.0);

        let rotated = earth_orientation_at(DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2));
        app.insert_resource(earth_snapshot(rotated.clone()));
        app.update();
        let second = {
            let world = app.world_mut();
            let mut query = world.query::<&DroneShip>();
            query.single(world).expect("one deck").state.position_m
        };
        let expected_rotated = body_fixed_to_planet_inertial(position_bf, &rotated);
        assert!((second - expected_rotated).length() < 1.0);
        assert!(
            (second - first).length() > 6_000_000.0,
            "a quarter turn must carry the deck around the body"
        );
    }

    #[test]
    fn requested_deck_is_spawned_with_a_body_fixed_anchor() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(RecoveryDeckRequest(
            RecoveryDeckCoordinates::try_new(25.0, -90.0).expect("valid deck"),
        ));
        app.add_systems(Startup, spawn_recovery_decks);
        app.update();
        let deck = app
            .world()
            .resource::<RecoveryDeckEntity>()
            .0
            .expect("deck spawns");
        let anchor = app
            .world()
            .get::<RecoveryDeckAnchor>(deck)
            .expect("deck keeps its body-fixed anchor");
        assert!(anchor.position_body_fixed_m.length() > 6_000_000.0);
    }
}
