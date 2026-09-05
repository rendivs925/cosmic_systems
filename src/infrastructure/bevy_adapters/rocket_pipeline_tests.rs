#[cfg(test)]
use crate::components::rocket::*;
#[cfg(test)]
use crate::domain::services::body_orientation::BodyOrientation;
#[cfg(test)]
use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
#[cfg(test)]
use crate::domain::services::gravity::gravitational_parameter;
#[cfg(test)]
use crate::domain::services::planet_factory::PlanetFactory;
#[cfg(test)]
use crate::domain::services::rocket_dynamics::RocketDynamicsState;
#[cfg(test)]
use crate::domain::services::rocket_propulsion::stage_thrust_body;
#[cfg(test)]
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
#[cfg(test)]
use crate::infrastructure::bevy_adapters::components::{RocketAutopilot, RocketCommands};
#[cfg(test)]
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;

#[cfg(test)]
use bevy::math::{DMat3, DQuat, DVec3};
#[cfg(test)]
use bevy::prelude::*;

#[cfg(test)]
fn test_earth_orientation() -> BodyOrientation {
    BodyOrientation::from_kernel(
        NaifBodyId::EARTH,
        TdbEpoch::j2000(),
        "test-orientation".to_string(),
        DQuat::IDENTITY,
        DVec3::Z * (std::f64::consts::TAU / (23.934 * 3_600.0)),
    )
}

#[cfg(test)]
mod engine_lifecycle_pipeline_tests {
    use super::*;
    use crate::domain::entities::rocket::{
        EngineState, ParallelBoosters, Rocket, RocketEngine, RocketStage, ThrustReference,
    };
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::simulation_time::SimulationTime;
    use crate::infrastructure::bevy_adapters::rocket_control::actuation_system;
    use crate::infrastructure::bevy_adapters::rocket_propulsion::{
        propulsion_consumption, propulsion_thrust,
    };

    fn engine(max_ignitions: u32) -> RocketEngine {
        RocketEngine {
            position_m: bevy::math::Vec3::ZERO,
            thrust_axis: bevy::math::Vec3::Y,
            isp_sea_level: 250.0,
            isp_vacuum: 250.0,
            gimbal_range_deg: 0.0,
            rated_thrust_kn: 100.0,
            thrust_reference: ThrustReference::SeaLevel,
            throttle_min: 0.0,
            throttle_max: 1.0,
            max_ignitions,
            ignition_count: 0,
            state: EngineState::Off,
        }
    }

    fn stage(name: &str, max_ignitions: u32) -> RocketStage {
        RocketStage {
            name: name.into(),
            diameter_m: 1.0,
            height_m: 4.0,
            dry_mass_kg: 100.0,
            propellant_mass_kg: 100.0,
            recovery_propellant_reserve_kg: None,
            landing_gear: None,
            fairing_dry_mass_kg: None,
            engines: vec![engine(max_ignitions)],
        }
    }

    fn lifecycle_app(with_parallel_boosters: bool) -> (App, Entity) {
        let mut app = App::new();
        app.insert_resource(SimulationTime::new(0.25));
        app.add_systems(
            FixedUpdate,
            (actuation_system, propulsion_thrust, propulsion_consumption).chain(),
        );
        let boosters = with_parallel_boosters.then(|| {
            ParallelBoosters::new(
                stage("Booster", 1),
                vec![
                    bevy::math::Vec3::new(-2.0, 0.0, 0.0),
                    bevy::math::Vec3::new(2.0, 0.0, 0.0),
                ],
            )
        });
        let vehicle = Rocket {
            name: "Lifecycle test".into(),
            diameter_m: 1.0,
            height_m: 8.0,
            stages: vec![stage("S1", 2), stage("S2", 1)],
            parallel_boosters: boosters,
        };
        let entity = app
            .world_mut()
            .spawn((
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        DVec3::ZERO,
                        DVec3::ZERO,
                        DQuat::IDENTITY,
                        500.0,
                        DMat3::IDENTITY,
                        DVec3::ZERO,
                    ),
                },
                RocketGeometry {
                    radius_m: 0.5,
                    height_m: 8.0,
                    lower_extent_y_m: -4.0,
                },
                RocketFlightConditions::default(),
                RocketPropulsion {
                    vehicle,
                    active_stage: 0,
                    propellant_remaining_kg: vec![100.0, 100.0],
                    booster_propellant_remaining_kg: if with_parallel_boosters {
                        vec![100.0, 100.0]
                    } else {
                        Vec::new()
                    },
                    boosters_attached: with_parallel_boosters,
                    throttle: 0.0,
                    gimbal_pitch_rad: 0.0,
                    gimbal_yaw_rad: 0.0,
                    time_since_separation_s: 2.0,
                    ullage_settle_time_s: 2.0,
                    separations_count: 0,
                    attached_payload_kg: 0.0,
                },
                RocketCommands {
                    throttle_cmd: 1.0,
                    ..default()
                },
                RocketAutopilot::default(),
                RocketMissionState::Launch,
                RetroPropulsionEffect::default(),
                ForceAccumulator::default(),
                TorqueAccumulator::default(),
            ))
            .id();
        (app, entity)
    }

    #[test]
    fn cutoff_stops_thrust_and_flow_then_budget_exhaustion_is_terminal() {
        let (mut app, entity) = lifecycle_app(false);
        app.world_mut().run_schedule(FixedUpdate);
        let after_start = app.world().get::<RocketPropulsion>(entity).unwrap();
        let propellant_after_start = after_start.propellant_remaining_kg[0];
        let engine = &after_start.vehicle.stages[0].engines[0];
        assert_eq!(engine.state, EngineState::Running);
        assert_eq!(engine.ignition_count, 1);
        assert!(app.world().get::<ForceAccumulator>(entity).unwrap().0.y > 0.0);

        {
            let mut entity_mut = app.world_mut().entity_mut(entity);
            entity_mut.get_mut::<RocketCommands>().unwrap().throttle_cmd = 0.0;
            *entity_mut.get_mut::<ForceAccumulator>().unwrap() = ForceAccumulator::default();
        }
        app.world_mut().run_schedule(FixedUpdate);
        let after_cutoff = app.world().get::<RocketPropulsion>(entity).unwrap();
        assert_eq!(
            after_cutoff.vehicle.stages[0].engines[0].state,
            EngineState::Off
        );
        assert_eq!(
            after_cutoff.propellant_remaining_kg[0],
            propellant_after_start
        );
        assert_eq!(
            app.world().get::<ForceAccumulator>(entity).unwrap().0,
            DVec3::ZERO
        );

        {
            let mut entity_mut = app.world_mut().entity_mut(entity);
            entity_mut.get_mut::<RocketCommands>().unwrap().throttle_cmd = 1.0;
        }
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world()
                .get::<RocketPropulsion>(entity)
                .unwrap()
                .vehicle
                .stages[0]
                .engines[0]
                .ignition_count,
            2
        );
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<RocketCommands>()
            .unwrap()
            .throttle_cmd = 0.0;
        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<RocketCommands>()
            .unwrap()
            .throttle_cmd = 1.0;
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<ForceAccumulator>()
            .unwrap()
            .0 = DVec3::ZERO;
        let propellant_before_denied_restart = app
            .world()
            .get::<RocketPropulsion>(entity)
            .unwrap()
            .propellant_remaining_kg[0];
        app.world_mut().run_schedule(FixedUpdate);
        let propulsion = app.world().get::<RocketPropulsion>(entity).unwrap();
        assert_eq!(
            propulsion.vehicle.stages[0].engines[0].state,
            EngineState::Depleted
        );
        assert_eq!(
            propulsion.propellant_remaining_kg[0],
            propellant_before_denied_restart
        );
        assert_eq!(
            app.world().get::<ForceAccumulator>(entity).unwrap().0,
            DVec3::ZERO
        );
    }

    #[test]
    fn serial_upper_stage_waits_for_ullage_before_its_first_start() {
        let (mut app, entity) = lifecycle_app(false);
        {
            let mut propulsion = app.world_mut().get_mut::<RocketPropulsion>(entity).unwrap();
            propulsion.active_stage = 1;
            propulsion.time_since_separation_s = 0.0;
            propulsion.separations_count = 1;
        }
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            app.world()
                .get::<RocketPropulsion>(entity)
                .unwrap()
                .vehicle
                .stages[1]
                .engines[0]
                .state,
            EngineState::Off
        );
        app.world_mut()
            .get_mut::<RocketPropulsion>(entity)
            .unwrap()
            .time_since_separation_s = 2.0;
        app.world_mut().run_schedule(FixedUpdate);
        let upper = &app
            .world()
            .get::<RocketPropulsion>(entity)
            .unwrap()
            .vehicle
            .stages[1]
            .engines[0];
        assert_eq!(upper.state, EngineState::Running);
        assert_eq!(upper.ignition_count, 1);
    }

    #[test]
    fn serial_and_parallel_booster_engines_share_the_same_start_lifecycle() {
        let (mut serial, serial_entity) = lifecycle_app(false);
        serial.world_mut().run_schedule(FixedUpdate);
        assert_eq!(
            serial
                .world()
                .get::<RocketPropulsion>(serial_entity)
                .unwrap()
                .vehicle
                .stages[0]
                .engines[0]
                .state,
            EngineState::Running
        );

        let (mut parallel, parallel_entity) = lifecycle_app(true);
        parallel.world_mut().run_schedule(FixedUpdate);
        let propulsion = parallel
            .world()
            .get::<RocketPropulsion>(parallel_entity)
            .unwrap();
        assert_eq!(propulsion.vehicle.stages[0].engines[0].ignition_count, 1);
        let booster = &propulsion
            .vehicle
            .parallel_boosters
            .as_ref()
            .unwrap()
            .stage
            .engines[0];
        assert_eq!(booster.state, EngineState::Running);
        assert_eq!(booster.ignition_count, 1);
    }
}

#[cfg(test)]
fn test_ephemeris_snapshot() -> EphemerisSnapshot {
    let earth_mass_kg = PlanetFactory::create_by_name("Earth").unwrap().mass_kg;
    EphemerisSnapshot::from_states_orientations_and_gravitational_parameters(
        Vec::new(),
        vec![test_earth_orientation()],
        vec![(NaifBodyId::EARTH, gravitational_parameter(earth_mass_kg))],
    )
}

#[cfg(test)]
fn ksc_terrain_coordinates() -> (f64, f64) {
    let earth = PlanetFactory::create_by_name("Earth").expect("Earth exists");
    crate::domain::services::reference_frames::geodetic_to_terrain_lat_lon(
        &crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates::default(),
        &earth,
    )
}

#[cfg(test)]
mod ground_contact_tests {
    use super::*;
    use crate::domain::entities::rocket::{
        EngineState, Rocket, RocketEngine, RocketStage, ThrustReference,
    };
    use crate::domain::events::SplashdownDetectedEvent;
    use crate::domain::services::landing_gear::{LandingGear, LandingGearSpec};
    use crate::domain::services::reference_frames::{
        geodetic_to_body_fixed, planet_inertial_to_body_fixed, surface_velocity_in_planet_inertial,
    };
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::simulation_time::SimulationTime;
    use crate::domain::services::terrain_collision::{radial_direction, GroundContact};
    use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
    use crate::infrastructure::bevy_adapters::components::{
        PlanetComponent, TerrainCollisionState,
    };
    use crate::infrastructure::bevy_adapters::rocket_contact::{
        advance_topple, deploy_landing_legs, resolve_ground_contact,
    };
    use crate::infrastructure::bevy_adapters::rocket_dynamics::{
        accumulate_forces, integrate_6dof,
    };
    use crate::infrastructure::bevy_adapters::rocket_lifecycle::{
        apply_relaunch_requests, RelaunchCommandQueue,
    };
    use crate::infrastructure::bevy_adapters::rocket_telemetry::FlightRecorder;
    use bevy::math::DQuat;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    const DT: f64 = 1.0 / 64.0;
    const G0: f64 = 9.80665;

    /// One-engine test vehicle with a sea-level rated thrust, +Y body axis.
    fn test_vehicle(rated_thrust_kn: f32) -> Rocket {
        Rocket {
            name: "Test".into(),
            diameter_m: 1.0,
            height_m: 10.0,
            stages: vec![RocketStage {
                name: "S1".into(),
                diameter_m: 1.0,
                height_m: 10.0,
                dry_mass_kg: 400.0,
                propellant_mass_kg: 600.0,
                recovery_propellant_reserve_kg: None,
                landing_gear: None,
                fairing_dry_mass_kg: None,
                engines: vec![RocketEngine {
                    position_m: bevy::math::Vec3::new(0.0, -5.0, 0.0),
                    thrust_axis: bevy::math::Vec3::Y,
                    isp_sea_level: 250.0,
                    isp_vacuum: 300.0,
                    gimbal_range_deg: 0.0,
                    rated_thrust_kn,
                    thrust_reference: ThrustReference::SeaLevel,
                    throttle_min: 0.0,
                    throttle_max: 1.0,
                    max_ignitions: 2,
                    ignition_count: 1,
                    state: EngineState::Running,
                }],
            }],
            parallel_boosters: None,
        }
    }

    /// Spawn a rocket standing exactly on the terrain at (lat, lon), plus the
    /// Earth planet entity, and run only the tail of the fixed pipeline:
    /// force writer → accumulate → integrate → ground contact.
    fn pad_app(throttle: f32, rated_thrust_kn: f32) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<SplashdownDetectedEvent>();
        app.insert_resource(SimulationTime::new(DT));
        app.insert_resource(test_ephemeris_snapshot());
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            DT,
        )));

        let planet =
            crate::domain::services::planet_factory::PlanetFactory::create_by_name("Earth")
                .expect("Earth exists");
        app.world_mut().spawn((
            PlanetComponent {
                domain_planet: planet,
                material: Handle::default(),
                has_texture: false,
                base_reflectance: 1.0,
                base_roughness: 1.0,
            },
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::earth(),
        ));

        let source =
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::earth().source;
        let (lat, lon) = ksc_terrain_coordinates();
        let h = source.height_m(lat, lon);
        let up = radial_direction(lat, lon);
        let surface_radius = EARTH_RADIUS_M + h;

        let vehicle = test_vehicle(rated_thrust_kn);
        let propellant = vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        let (inertia, com) =
            crate::domain::services::rocket_dynamics::rocket_inertia_tensor(1000.0, 0.0, 0.5, 10.0);
        app.world_mut().spawn((
            RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    up * (surface_radius + 5.0),
                    DVec3::ZERO,
                    DQuat::from_rotation_arc(DVec3::Y, up),
                    1000.0,
                    inertia,
                    com,
                ),
            },
            RocketGeometry {
                radius_m: 0.5,
                height_m: 10.0,
                lower_extent_y_m: -5.0,
            },
            RocketPropulsion {
                vehicle,
                active_stage: 0,
                propellant_remaining_kg: propellant,
                booster_propellant_remaining_kg: Vec::new(),
                boosters_attached: false,
                throttle,
                gimbal_pitch_rad: 0.0,
                gimbal_yaw_rad: 0.0,
                time_since_separation_s: 10.0,
                ullage_settle_time_s: 2.0,
                separations_count: 0,
                attached_payload_kg: 0.0,
            },
            TerrainCollisionState::default(),
            GroundRest { active: true },
            TipOverState::default(),
            LandingScorecard::default(),
            RocketAutopilot::default(),
            FlightRecorder::new(64, 1.0),
            RocketMissionState::PreLaunch,
            GravityAcceleration { value: -up * G0 },
            ForceAccumulator::default(),
            TorqueAccumulator::default(),
            RocketPlanetBinding {
                planet_name: CelestialBodyId::earth(),
            },
        ));

        // Mirror production force writers: non-gravity forces only —
        // accumulate_forces contributes the single gravity term.
        fn write_flight_forces(
            mut query: Query<(
                &RocketPhysicsState,
                &RocketPropulsion,
                &mut ForceAccumulator,
            )>,
        ) {
            for (rocket, propulsion, mut force) in query.iter_mut() {
                if let Some(stage) = propulsion.vehicle.stages.get(propulsion.active_stage) {
                    let (body_thrust, _) =
                        stage_thrust_body(&stage.engines, propulsion.throttle, 0.0);
                    force.0 += rocket.dynamics.orientation * body_thrust;
                }
            }
        }
        app.add_systems(
            FixedUpdate,
            (
                write_flight_forces,
                accumulate_forces,
                integrate_6dof,
                resolve_ground_contact,
            )
                .chain(),
        );
        app
    }

    /// Pad hold is real physics now: a throttled-down vehicle must stay pinned
    /// to the pad instead of sinking under accumulated gravity.
    #[test]
    fn pad_hold_survives_gravity_without_sinking() {
        let mut app = pad_app(0.0, 20.0);

        for _ in 0..96 {
            app.update();
        }

        let world = app.world_mut();
        let mut q = world.query::<(&RocketPhysicsState, &GroundRest, &TerrainCollisionState)>();
        let (rocket, rest, collision) = q.single(world).unwrap();

        assert!(rest.active, "vehicle must still be resting on the pad");
        let (lat, lon) = ksc_terrain_coordinates();
        let h = crate::infrastructure::bevy_adapters::components::PlanetTerrain::earth()
            .source
            .height_m(lat, lon);
        let expected_r = EARTH_RADIUS_M + h + 5.0;
        assert!(
            (rocket.dynamics.position_m.length() - expected_r).abs() < 0.05,
            "position drifted off the pad: |r|={}",
            rocket.dynamics.position_m.length()
        );
        assert!(
            rocket.dynamics.velocity_mps.length() < 0.5,
            "residual velocity too large: {}",
            rocket.dynamics.velocity_mps.length()
        );
        assert_eq!(collision.ground_contact, GroundContact::Landed);
    }

    #[test]
    fn prelaunch_vehicle_remains_at_its_rotating_launch_site() {
        let mut app = pad_app(0.0, 20.0);
        let launch_site = LaunchSiteCoordinates::default();
        let entity = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            query.single(world).unwrap()
        };
        {
            let world = app.world_mut();
            world.entity_mut(entity).insert(launch_site.clone());
            let mut rocket = world.get_mut::<RocketPhysicsState>(entity).unwrap();
            rocket.dynamics.velocity_mps = DVec3::new(500.0, -100.0, 250.0);
            rocket.dynamics.angular_velocity_radps = DVec3::new(0.1, 0.2, 0.3);
        }

        for _ in 0..96 {
            app.update();
        }

        let earth = crate::domain::services::planet_factory::PlanetFactory::create_by_name("Earth")
            .unwrap();
        let orientation = test_earth_orientation();
        let world = app.world_mut();
        let rocket = world.get::<RocketPhysicsState>(entity).unwrap();
        let position_bf = planet_inertial_to_body_fixed(rocket.dynamics.position_m, &orientation);
        let expected_direction_bf = geodetic_to_body_fixed(&launch_site, &earth).normalize();
        let expected_surface_velocity =
            surface_velocity_in_planet_inertial(rocket.dynamics.position_m, &orientation);

        assert!(
            position_bf.normalize().dot(expected_direction_bf) > 1.0 - 1e-12,
            "pre-launch position drifted away from the launch site"
        );
        assert!(
            (rocket.dynamics.velocity_mps - expected_surface_velocity).length() < 1e-9,
            "pre-launch velocity must match the rotating launch pad"
        );
        assert!(
            rocket.dynamics.angular_velocity_radps.length() < 1e-12,
            "pre-launch vehicle must not rotate on the pad"
        );
    }

    /// Takeoff from resting contact: throttle above TWR 1 releases the
    /// constraint and the vehicle climbs.
    #[test]
    fn takeoff_releases_rest_and_climbs() {
        let mut app = pad_app(1.0, 200.0);
        let start_r;

        {
            let world = app.world_mut();
            let mut q = world.query::<&RocketPhysicsState>();
            start_r = q.single(world).unwrap().dynamics.position_m.length();
        }

        for _ in 0..128 {
            app.update();
        }

        let world = app.world_mut();
        let mut q = world.query::<(&RocketPhysicsState, &GroundRest, &RocketMissionState)>();
        let (rocket, rest, mission) = q.single(world).unwrap();

        assert!(!rest.active, "constraint must release above TWR 1");
        assert!(
            rocket.dynamics.position_m.length() > start_r + 1.0,
            "vehicle did not climb: Δr={}",
            rocket.dynamics.position_m.length() - start_r
        );
        assert_ne!(
            *mission,
            RocketMissionState::Crashed,
            "liftoff must not be judged a crash"
        );
    }

    /// Deployed landing gear absorb a gentle touchdown through strut
    /// compression instead of the rigid point-contact snap.
    #[test]
    fn deployed_legs_absorb_touchdown_softly() {
        let surface_r;
        let mut app = {
            let mut app = pad_app(0.0, 20.0); // engines off
            let world = app.world_mut();
            let (lat, lon) = ksc_terrain_coordinates();
            let h = crate::infrastructure::bevy_adapters::components::PlanetTerrain::earth()
                .source
                .height_m(lat, lon);
            surface_r = EARTH_RADIUS_M + h;

            let (entity, up, mass_kg, inertia_body, com) = {
                let mut q = world.query::<(Entity, &RocketPhysicsState)>();
                let (entity, rocket) = q.single(world).unwrap();
                let up = rocket.dynamics.position_m.normalize();
                (
                    entity,
                    up,
                    rocket.dynamics.mass_kg,
                    rocket.dynamics.inertia_body,
                    rocket.dynamics.center_of_mass_m,
                )
            };
            // Start airborne 2 m above the pad, descending at 2 m/s, gear
            // down, released from the pad hold. A terminal verdict must wait
            // until this descent crosses the sampled contact plane.
            world.entity_mut(entity).insert(RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    up * (surface_r + 7.0),
                    -up * 2.0,
                    DQuat::from_rotation_arc(DVec3::Y, up),
                    mass_kg,
                    inertia_body,
                    com,
                ),
            });
            world.get_mut::<GroundRest>(entity).unwrap().active = false;
            world
                .get_mut::<RocketPhysicsState>(entity)
                .unwrap()
                .dynamics
                .angular_velocity_radps = DVec3::new(0.03, -0.02, 0.01);
            // Gear down: pre-latch deployment (the latch itself is covered
            // by domain tests; this test exercises the contact path).
            let gear = LandingGear::new(
                LandingGearSpec {
                    count: 4,
                    base_radius_m: 4.5,
                    stroke_m: 3.0,
                    max_landing_mass_kg: Some(2_000.0),
                    deploy_altitude_m: 100.0,
                },
                1_000.0, // gross vehicle mass of this test vehicle
            );
            let mut legs = LandingLegs::new(gear);
            legs.deployment.deployed = true;
            world.entity_mut(entity).insert(legs);
            app
        };

        // The first Update initializes the manual fixed clock; the second
        // performs the first 1/64 s simulation tick.
        app.update();
        app.update();
        {
            let world = app.world_mut();
            let mut q = world.query::<(&GroundRest, &LandingScorecard)>();
            let (rest, scorecard) = q.single(world).unwrap();
            assert!(
                !rest.active,
                "vehicle must not land while still above terrain"
            );
            assert!(
                !scorecard.recorded,
                "touchdown must wait for the contact plane"
            );
        }
        for _ in 0..510 {
            app.update();
        }

        let world = app.world_mut();
        let mut q = world.query::<(
            &RocketPhysicsState,
            &GroundRest,
            &RocketMissionState,
            &LandingLegs,
            &LandingScorecard,
        )>();
        let (rocket, rest, mission, legs, scorecard) = q.single(world).unwrap();

        assert!(rest.active, "must be resting on gear");
        assert_ne!(*mission, RocketMissionState::Crashed);
        assert!(scorecard.recorded, "touchdown must be recorded");
        assert!(
            scorecard.touchdown_vertical_speed_mps > 0.0
                && scorecard.touchdown_vertical_speed_mps <= 5.0,
            "scorecard descent {} exceeds the accepted gear touchdown limit",
            scorecard.touchdown_vertical_speed_mps
        );
        assert!(
            scorecard.leg_compression_peak_m > 0.0
                && scorecard.leg_compression_peak_m <= legs.gear.spec.stroke_m,
            "compression peak {} outside (0, stroke]",
            scorecard.leg_compression_peak_m
        );
        assert!(
            legs.compression_m > 0.0 && legs.compression_m <= legs.gear.spec.stroke_m,
            "strut compression {} outside (0, stroke]",
            legs.compression_m
        );
        // Soft contact: the hull rides below the point-contact radius by at
        // most one stroke (documented approximation).
        let lower_contact_radius_m = (rocket.dynamics.position_m
            + rocket.dynamics.orientation * DVec3::NEG_Y * 5.0)
            .length();
        let sink = surface_r - lower_contact_radius_m;
        assert!(
            sink >= -0.5 && sink <= legs.gear.spec.stroke_m + 0.5,
            "sink depth {sink} m outside strut range"
        );
        assert!(
            rocket.dynamics.velocity_mps.length() < 0.3,
            "not settled: {}",
            rocket.dynamics.velocity_mps.length()
        );
        assert!(
            rocket.dynamics.angular_velocity_radps.length() < 1e-12,
            "supported landing must not retain free angular motion"
        );
    }

    #[test]
    fn landing_gear_rating_failure_is_a_crash() {
        let mut app = pad_app(0.0, 20.0);
        let entity = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            query.single(world).unwrap()
        };
        let gear = LandingGear::new(
            LandingGearSpec {
                count: 4,
                base_radius_m: 4.5,
                stroke_m: 3.0,
                max_landing_mass_kg: Some(500.0),
                deploy_altitude_m: 100.0,
            },
            1_000.0,
        );
        let mut legs = LandingLegs::new(gear);
        legs.deployment.deployed = true;
        let world = app.world_mut();
        world.entity_mut(entity).insert(legs);
        *world.get_mut::<RocketMissionState>(entity).unwrap() = RocketMissionState::Landing;
        world.get_mut::<GroundRest>(entity).unwrap().active = false;

        // The first Update initializes the manual fixed clock; the second
        // performs the first 1/64 s simulation tick.
        app.update();
        app.update();

        let world = app.world();
        assert_eq!(
            *world.get::<RocketMissionState>(entity).unwrap(),
            RocketMissionState::Crashed,
            "a vehicle above its configured landing-gear mass rating must not land"
        );
        assert_eq!(
            world
                .get::<TerrainCollisionState>(entity)
                .unwrap()
                .ground_contact,
            GroundContact::Crash
        );
    }

    /// Relaunch (Phase 14): one command restores a flown, drained vehicle to
    /// a fresh pad state and clears its jettisoned debris.
    #[test]
    fn relaunch_restores_fresh_rotating_pad_state() {
        let mut app = pad_app(0.0, 20.0);
        app.init_resource::<RelaunchCommandQueue>();
        app.add_systems(FixedUpdate, apply_relaunch_requests);

        let debris_entity;
        let rocket_entity;
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            rocket_entity = q.single(world).unwrap();

            debris_entity = world
                .spawn(SpentStage {
                    parent_rocket: rocket_entity,
                    kind: SpentStageKind::Booster,
                })
                .id();
        }

        {
            // Fly the vehicle into a post-flight state: stage 2 active,
            // tanks empty, airborne, landed mission, gear down.
            let world = app.world_mut();
            let mut state = world.get_mut::<RocketPhysicsState>(rocket_entity).unwrap();
            state.dynamics.position_m += DVec3::X * 1_000.0;
            state.dynamics.velocity_mps = DVec3::new(50.0, -10.0, 0.0);
            let mut propulsion = world.get_mut::<RocketPropulsion>(rocket_entity).unwrap();
            propulsion.vehicle.stages[0].fairing_dry_mass_kg = Some(25.0);
            propulsion.propellant_remaining_kg = vec![0.0, 0.0];
            propulsion.active_stage = 1;
            propulsion.separations_count = 1;
            propulsion.throttle = 0.4;
            propulsion.attached_payload_kg = 0.0;
            let engine = &mut propulsion.vehicle.stages[0].engines[0];
            engine.max_ignitions = 2;
            engine.ignition_count = 2;
            engine.state = EngineState::Depleted;
            let mut mission = world.get_mut::<RocketMissionState>(rocket_entity).unwrap();
            *mission = RocketMissionState::Landed;
            world
                .entity_mut(rocket_entity)
                .insert(LaunchSiteCoordinates::default());
            world
                .entity_mut(rocket_entity)
                .insert(InitialPayloadFairing { dry_mass_kg: 25.0 });
        }

        app.world_mut()
            .resource_mut::<RelaunchCommandQueue>()
            .0
            .push(rocket_entity);

        app.update();
        app.update();
        app.world_mut().flush();

        let world = app.world_mut();
        assert!(
            world.get_entity(debris_entity).is_err(),
            "jettisoned debris must be despawned"
        );
        let mut q = world.query::<(
            &RocketPhysicsState,
            &RocketPropulsion,
            &RocketMissionState,
            &GroundRest,
        )>();
        let (rocket, propulsion, mission, rest) = q.single(world).unwrap();

        assert_eq!(*mission, RocketMissionState::PreLaunch);
        assert!(rest.active, "pad-hold must re-engage");
        assert_eq!(propulsion.active_stage, 0);
        assert_eq!(propulsion.separations_count, 0);
        assert_eq!(propulsion.throttle, 0.0);
        assert_eq!(propulsion.attached_payload_kg, 25.0);
        assert_eq!(
            world
                .get::<PayloadFairing>(rocket_entity)
                .expect("relaunch restores one attached fairing")
                .dry_mass_kg,
            25.0
        );
        for stage in &propulsion.vehicle.stages {
            for engine in &stage.engines {
                assert_eq!(engine.state, EngineState::Off);
                assert_eq!(engine.ignition_count, 0);
            }
        }
        for (stage, remaining) in propulsion
            .vehicle
            .stages
            .iter()
            .zip(&propulsion.propellant_remaining_kg)
        {
            assert!((remaining - stage.propellant_mass_kg).abs() < 1e-3);
        }
        let expected_surface_velocity = surface_velocity_in_planet_inertial(
            rocket.dynamics.position_m,
            &test_earth_orientation(),
        );
        assert!(
            (rocket.dynamics.velocity_mps - expected_surface_velocity).length() < 1e-9,
            "a KSC-equivalent relaunch must co-move with its rotating pad"
        );
        assert!(
            (400.0..420.0).contains(&rocket.dynamics.velocity_mps.length()),
            "the reset velocity must preserve KSC's inertial surface speed"
        );
        assert!(
            (rocket.dynamics.mass_kg
                - (propulsion.vehicle.total_mass_kg() + propulsion.attached_payload_kg) as f64)
                .abs()
                < 1e-6,
            "dynamics mass must match the refueled vehicle inventory"
        );
    }

    #[test]
    fn relaunch_restores_only_the_first_active_stage_gear() {
        let mut app = pad_app(0.0, 20.0);
        app.init_resource::<RelaunchCommandQueue>();
        app.add_systems(FixedUpdate, apply_relaunch_requests);
        let rocket_entity = {
            let world = app.world_mut();
            world
                .query_filtered::<Entity, With<RocketPhysicsState>>()
                .single(world)
                .unwrap()
        };

        let vehicle = Rocket::falcon9_test_fixture();
        let expected_gear = vehicle.stages[0].landing_gear.expect("stage 1 has gear");
        let stale_upper_gear = LandingGearSpec {
            count: 4,
            base_radius_m: 1.0,
            stroke_m: 1.0,
            max_landing_mass_kg: Some(1_000.0),
            deploy_altitude_m: 50.0,
        };
        {
            let world = app.world_mut();
            let mut propulsion = world.get_mut::<RocketPropulsion>(rocket_entity).unwrap();
            propulsion.vehicle = vehicle;
            propulsion.active_stage = 1;
            propulsion.propellant_remaining_kg = vec![0.0, 0.0];
            propulsion.separations_count = 1;
            world
                .entity_mut(rocket_entity)
                .insert(LandingLegs::new(LandingGear::new(
                    stale_upper_gear,
                    1_000.0,
                )));
        }
        app.world_mut()
            .resource_mut::<RelaunchCommandQueue>()
            .0
            .push(rocket_entity);

        app.update();
        app.update();

        let world = app.world();
        let propulsion = world.get::<RocketPropulsion>(rocket_entity).unwrap();
        let legs = world.get::<LandingLegs>(rocket_entity).unwrap();
        assert_eq!(propulsion.active_stage, 0);
        assert_eq!(legs.gear.spec, expected_gear);
        assert_ne!(legs.gear.spec, stale_upper_gear);
    }

    /// Phase 17 scenario `tip_over` (app-level wiring): a resting vehicle
    /// tilted beyond its critical angle must arm the topple model inside
    /// GroundContact, fall under gravity, and end Crashed exactly once.
    /// The domain pendulum is covered by landing_gear tests; this pins the
    /// arm → advance → mission-lost pipeline ordering.
    #[test]
    fn leaning_past_critical_angle_topples_to_crashed() {
        let mut app = pad_app(0.0, 20.0);
        // Production order: contact first, topple advance after it.
        app.add_systems(FixedUpdate, advance_topple.after(resolve_ground_contact));

        let rocket_entity = {
            let world = app.world_mut();
            let mut q = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            q.single(world).unwrap()
        };
        {
            // Tilt 30 deg about Z: well past the gear-less critical angle
            // (atan(0.6 m base / ~5 m com height) ≈ 7 deg for this vehicle).
            let world = app.world_mut();
            let up = world
                .get::<RocketPhysicsState>(rocket_entity)
                .unwrap()
                .dynamics
                .position_m
                .normalize();
            let upright = DQuat::from_rotation_arc(DVec3::Y, up);
            let leaned = upright * DQuat::from_rotation_z(30.0_f64.to_radians());
            let mut state = world.get_mut::<RocketPhysicsState>(rocket_entity).unwrap();
            state.dynamics.orientation = leaned;
            state.dynamics.angular_velocity_radps = DVec3::ZERO;
        }

        // The fall from 30 deg to 90 deg at g=9.81, l≈5 m takes a few
        // seconds; run well past that.
        for _ in 0..1_500 {
            app.update();
        }

        let world = app.world_mut();
        let mut q = world.query::<(&TipOverState, &RocketMissionState)>();
        let (tip_over, mission) = q.single(world).unwrap();
        assert!(
            tip_over.fall.is_some() || *mission == RocketMissionState::Crashed,
            "topple never armed"
        );
        assert_eq!(*mission, RocketMissionState::Crashed);
        drop(q);
        // Once crashed it stays crashed (one-way transition).
        for _ in 0..64 {
            app.update();
        }
        let world = app.world_mut();
        let mut q = world.query::<&RocketMissionState>();
        assert_eq!(*q.single(world).unwrap(), RocketMissionState::Crashed);
    }

    /// Phase 17 scenario `gear_deployment_gate` (app-level wiring): an
    /// undeployed descending vehicle must latch its legs when passing the
    /// radar-altitude gate with negative vertical speed — and only then.
    #[test]
    fn descending_through_gate_deploys_landing_legs() {
        use crate::domain::services::landing_gear::LandingGearSpec;

        let mut app = pad_app(0.0, 20.0);
        app.add_systems(
            FixedUpdate,
            (deploy_landing_legs, resolve_ground_contact).chain(),
        );

        let surface_r = {
            let world = app.world_mut();
            let mut q = world.query::<&RocketPhysicsState>();
            let rocket = q.single(world).unwrap();
            rocket.dynamics.position_m.length()
        };

        let rocket_entity = {
            let world = app.world_mut();
            let mut q = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            q.single(world).unwrap()
        };
        {
            // Airborne 150 m above the pad, descending slowly; legs present
            // but NOT deployed, gate at 100 m. TerrainCollisionState is
            // initialized consistently (radar 150 m) because in production
            // GroundContact has already sampled the surface before the
            // descent begins.
            let world = app.world_mut();
            let up = world
                .get::<RocketPhysicsState>(rocket_entity)
                .unwrap()
                .dynamics
                .position_m
                .normalize();
            world.entity_mut(rocket_entity).insert(RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    up * (surface_r + 150.0),
                    -up * 5.0,
                    DQuat::from_rotation_arc(DVec3::Y, up),
                    1_000.0,
                    bevy::math::DMat3::from_diagonal(DVec3::splat(1e4)),
                    DVec3::ZERO,
                ),
            });
            world
                .entity_mut(rocket_entity)
                .insert(TerrainCollisionState {
                    radar_altitude_m: 150.0,
                    ..TerrainCollisionState::default()
                });
            // Release the pad hold so the vehicle actually descends.
            world.get_mut::<GroundRest>(rocket_entity).unwrap().active = false;
            let gear = LandingGear::new(
                LandingGearSpec {
                    count: 4,
                    base_radius_m: 4.5,
                    stroke_m: 3.0,
                    max_landing_mass_kg: Some(2_000.0),
                    deploy_altitude_m: 100.0,
                },
                1_000.0,
            );
            world
                .entity_mut(rocket_entity)
                .insert(LandingLegs::new(gear));
        }

        // Above the gate nothing may deploy.
        for _ in 0..32 {
            app.update();
        }
        {
            let world = app.world_mut();
            let legs = world.get::<LandingLegs>(rocket_entity).unwrap();
            assert!(!legs.deployment.deployed, "deployed above the gate");
        }

        // Fall through the gate (150 m at 5 m/s ≈ 10 s); latch must trip.
        for _ in 0..800 {
            app.update();
        }
        let world = app.world_mut();
        let legs = world.get::<LandingLegs>(rocket_entity).unwrap();
        assert!(legs.deployment.deployed, "gate crossing must deploy legs");
    }
}

/// Recovery regression coverage uses the real fixed-tick guidance, control,
/// actuation, thrust, integration, and contact systems. The stages start from
/// finite descent states; neither test writes a Transform or forces a mission
/// completion verdict.
#[cfg(test)]
mod recovery_pipeline_tests {
    use super::*;
    use crate::domain::entities::rocket::{
        EngineState, Rocket, RocketEngine, RocketStage, ThrustReference,
    };
    use crate::domain::events::SplashdownDetectedEvent;
    use crate::domain::services::guidance::AutopilotMode;
    use crate::domain::services::recovery::{DroneShip as DomainDroneShip, StationKeeper};
    use crate::domain::services::rocket_dynamics::{rocket_inertia_tensor, RocketDynamicsState};
    use crate::domain::services::simulation_time::SimulationTime;
    use crate::domain::services::terrain_collision::radial_direction;
    use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
    use crate::infrastructure::bevy_adapters::components::{PlanetComponent, PlanetTerrain};
    use crate::infrastructure::bevy_adapters::rocket_contact::resolve_ground_contact;
    use crate::infrastructure::bevy_adapters::rocket_control::{actuation_system, control_system};
    use crate::infrastructure::bevy_adapters::rocket_dynamics::{
        accumulate_forces, integrate_6dof,
    };
    use crate::infrastructure::bevy_adapters::rocket_guidance::{
        guidance_system, update_drone_ship_landing_targets,
    };
    use crate::infrastructure::bevy_adapters::rocket_propulsion::{
        propulsion_consumption, propulsion_thrust,
    };
    use crate::infrastructure::bevy_adapters::rocket_recovery::{
        resolve_drone_ship_deck_contact, station_keep_drone_ships,
    };
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    const DT: f64 = 1.0 / 64.0;

    fn recovery_stage() -> Rocket {
        Rocket {
            name: "Recovery test stage".into(),
            diameter_m: 1.0,
            height_m: 10.0,
            stages: vec![RocketStage {
                name: "Booster".into(),
                diameter_m: 1.0,
                height_m: 10.0,
                dry_mass_kg: 400.0,
                propellant_mass_kg: 600.0,
                recovery_propellant_reserve_kg: None,
                landing_gear: None,
                fairing_dry_mass_kg: None,
                engines: vec![RocketEngine {
                    position_m: bevy::math::Vec3::new(0.0, -5.0, 0.0),
                    thrust_axis: bevy::math::Vec3::Y,
                    isp_sea_level: 250.0,
                    isp_vacuum: 300.0,
                    gimbal_range_deg: 4.0,
                    rated_thrust_kn: 20.0,
                    thrust_reference: ThrustReference::SeaLevel,
                    throttle_min: 0.0,
                    throttle_max: 1.0,
                    max_ignitions: 3,
                    ignition_count: 1,
                    state: EngineState::Running,
                }],
            }],
            parallel_boosters: None,
        }
    }

    fn recovery_app(deck_altitude_m: f64) -> (App, DVec3) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<SplashdownDetectedEvent>();
        app.insert_resource(SimulationTime::new(DT));
        app.insert_resource(test_ephemeris_snapshot());
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            DT,
        )));

        let planet =
            crate::domain::services::planet_factory::PlanetFactory::create_by_name("Earth")
                .expect("Earth exists");
        let terrain = PlanetTerrain::earth();
        let terrain_source = terrain.source.clone();
        app.world_mut().spawn((
            PlanetComponent {
                domain_planet: planet,
                material: Handle::default(),
                has_texture: false,
                base_reflectance: 1.0,
                base_roughness: 1.0,
            },
            terrain,
        ));

        let (lat, lon) = ksc_terrain_coordinates();
        let up = radial_direction(lat, lon);
        let terrain_surface_m = EARTH_RADIUS_M + terrain_source.height_m(lat, lon);
        let deck_center_m = up * (terrain_surface_m + deck_altitude_m);
        let vehicle = recovery_stage();
        let propellant = vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        let (inertia, center_of_mass_m) = rocket_inertia_tensor(400.0, 600.0, 0.5, 10.0);

        let stage = app
            .world_mut()
            .spawn((
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        // Physics positions are cylinder centers. Keep the
                        // lower extent 2 m clear of the deck at test start.
                        deck_center_m + up * 7.0,
                        -up * 2.0,
                        DQuat::from_rotation_arc(DVec3::Y, up),
                        1_000.0,
                        inertia,
                        center_of_mass_m,
                    ),
                },
                RocketGeometry {
                    radius_m: 0.5,
                    height_m: 10.0,
                    lower_extent_y_m: -5.0,
                },
                RocketFlightConditions::default(),
                RocketMissionState::Landing,
                RocketPropulsion {
                    vehicle,
                    active_stage: 0,
                    propellant_remaining_kg: propellant,
                    booster_propellant_remaining_kg: Vec::new(),
                    boosters_attached: false,
                    throttle: 0.0,
                    gimbal_pitch_rad: 0.0,
                    gimbal_yaw_rad: 0.0,
                    time_since_separation_s: 10.0,
                    ullage_settle_time_s: 0.0,
                    separations_count: 0,
                    attached_payload_kg: 0.0,
                },
            ))
            .id();
        app.world_mut().entity_mut(stage).insert((
            RocketCommands::default(),
            RocketAutopilot {
                mode: AutopilotMode::Boostback,
                target_landing_position_m: deck_center_m,
                ..Default::default()
            },
            OrbitalElements::default(),
            TerrainCollisionState::default(),
            GroundRest { active: false },
            TipOverState::default(),
            LandingScorecard::default(),
            RocketPlanetBinding {
                planet_name: CelestialBodyId::earth(),
            },
            GravityAcceleration {
                value: -up * 9.80665,
            },
            ForceAccumulator::default(),
            TorqueAccumulator::default(),
            RetroPropulsionEffect::default(),
        ));

        app.add_systems(
            FixedUpdate,
            (
                station_keep_drone_ships,
                update_drone_ship_landing_targets,
                guidance_system,
                control_system,
                actuation_system,
                propulsion_thrust,
                accumulate_forces,
                integrate_6dof,
                resolve_drone_ship_deck_contact,
                resolve_ground_contact,
                propulsion_consumption,
            )
                .chain(),
        );
        (app, deck_center_m)
    }

    #[test]
    fn rtls_recovery_stage_hands_off_and_lands_through_fixed_systems() {
        let (mut app, pad_position_m) = recovery_app(0.0);
        let stage = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            query.single(world).unwrap()
        };

        // The first physical tick must execute real RTLS boostback guidance and
        // hand its finite, on-target state to terminal guidance.
        app.update();
        app.update();
        assert_eq!(
            app.world().get::<RocketAutopilot>(stage).unwrap().mode,
            AutopilotMode::Landing,
            "boostback did not hand off to terminal recovery"
        );
        for _ in 0..512 {
            app.update();
        }

        let world = app.world();
        let rocket = world.get::<RocketPhysicsState>(stage).unwrap();
        let mission = world.get::<RocketMissionState>(stage).unwrap();
        let scorecard = world.get::<LandingScorecard>(stage).unwrap();
        assert_eq!(*mission, RocketMissionState::Landed);
        assert!(
            scorecard.recorded,
            "terrain contact must record recovery touchdown"
        );
        assert!(scorecard.touchdown_vertical_speed_mps <= 5.0);
        assert!(
            (rocket.dynamics.position_m - pad_position_m).length() < 5.0,
            "RTLS stage missed its explicit recovery target"
        );
    }

    fn droneship_outcome() -> (DVec3, DVec3, DVec3, LandingScorecard, bool, f64, bool) {
        let (mut app, deck_center_m) = recovery_app(20.0);
        let ship = app
            .world_mut()
            .spawn(DroneShip {
                state: DomainDroneShip {
                    position_m: deck_center_m,
                    velocity_mps: DVec3::new(0.0, 0.4, 0.0),
                    external_accel_mps2: DVec3::new(0.0, 0.05, 0.0),
                    mass_kg: 4.0e6,
                },
                station_target_position_m: deck_center_m,
                station_keeper: StationKeeper {
                    kp: 0.05,
                    kd: 0.2,
                    max_thrust_n: 2.0e6,
                },
                deck_half_extent_m: 25.0,
            })
            .id();
        let stage = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            query.single(world).unwrap()
        };
        app.world_mut()
            .entity_mut(stage)
            .insert(DroneShipLandingTarget {
                drone_ship: ship,
                prediction_horizon_s: 2.0,
                deck_contact: false,
            });

        // Touchdown must wait for the physical deck plane rather than use the
        // precontact band as a terminal-contact shortcut.
        app.update();
        app.update();
        let latched_above_deck = app
            .world()
            .get::<DroneShipLandingTarget>(stage)
            .unwrap()
            .deck_contact;
        for _ in 0..510 {
            app.update();
        }
        let world = app.world();
        let rocket = world.get::<RocketPhysicsState>(stage).unwrap();
        let autopilot = world.get::<RocketAutopilot>(stage).unwrap();
        let scorecard = world.get::<LandingScorecard>(stage).unwrap().clone();
        let target = world.get::<DroneShipLandingTarget>(stage).unwrap();
        let ship_state = &world.get::<DroneShip>(ship).unwrap().state;
        let deck_normal = ship_state.position_m.normalize_or_zero();
        let geometry = world.get::<RocketGeometry>(stage).unwrap();
        let lower_contact_m = rocket.dynamics.position_m
            + rocket.dynamics.orientation * geometry.lower_extent_body_m();
        let deck_separation_m = (lower_contact_m - ship_state.position_m).dot(deck_normal);
        (
            rocket.dynamics.position_m,
            rocket.dynamics.velocity_mps,
            autopilot.target_landing_position_m,
            scorecard,
            target.deck_contact,
            deck_separation_m,
            latched_above_deck,
        )
    }

    #[test]
    fn moving_droneship_recovery_is_deck_relative_and_deterministic() {
        let first = droneship_outcome();
        let second = droneship_outcome();

        assert!(
            first.4,
            "deck-relative contact must latch a successful recovery"
        );
        assert!(
            first.3.recorded,
            "deck touchdown must populate the landing scorecard"
        );
        assert!(first.3.touchdown_vertical_speed_mps <= 5.0);
        assert!(
            !first.6,
            "stage must not latch to a drone-ship deck while still airborne"
        );
        assert!(
            first.5.abs() < 1e-9,
            "deck contact must project to the deck plane, separation {} m",
            first.5
        );
        assert!(
            first.2.y.abs() > 0.01,
            "guidance must consume the moving ship's predicted target, not its static origin"
        );
        assert_eq!(
            first.0.to_array().map(f64::to_bits),
            second.0.to_array().map(f64::to_bits)
        );
        assert_eq!(
            first.1.to_array().map(f64::to_bits),
            second.1.to_array().map(f64::to_bits)
        );
        assert_eq!(
            first.2.to_array().map(f64::to_bits),
            second.2.to_array().map(f64::to_bits)
        );
        assert_eq!(
            first.3.touchdown_vertical_speed_mps.to_bits(),
            second.3.touchdown_vertical_speed_mps.to_bits()
        );
        assert_eq!(
            first.3.distance_to_target_m.to_bits(),
            second.3.distance_to_target_m.to_bits()
        );
        assert_eq!(first.6, second.6);
    }
}

/// Ascent-pipeline regression tests: the real Guidance → Control → Actuation
/// → Gravity → Forces → Integrate → GroundContact chain driving a low-thrust
/// (electron-class) vehicle off the pad. Pins two Phase 12 behaviors: the
/// throttle slew must reach the commanded maximum shortly after launch (no
/// hidden writer may cap it at the envelope floor), and the pitch-over must
/// stay gated until the vehicle clears the tower.
#[cfg(test)]
mod ascent_pipeline_tests {
    use super::*;
    use crate::domain::entities::rocket::{
        EngineState, Rocket, RocketEngine, RocketStage, ThrustReference,
    };
    use crate::domain::events::{SplashdownDetectedEvent, StageSeparatedEvent};
    use crate::domain::services::guidance::AutopilotMode;
    use crate::domain::services::physics_orbital::LowEarthOrbitTarget;
    use crate::domain::services::reference_frames::{
        planet_equatorial_reference_x_axis, planet_inertial_spin_axis,
    };
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::simulation_time::SimulationTime;
    use crate::domain::services::terrain_collision::radial_direction;
    use crate::infrastructure::bevy_adapters::components::PlanetComponent;
    use crate::infrastructure::bevy_adapters::rocket_contact::resolve_ground_contact;
    use crate::infrastructure::bevy_adapters::rocket_control::{actuation_system, control_system};
    use crate::infrastructure::bevy_adapters::rocket_dynamics::{
        accumulate_forces, integrate_6dof,
    };
    use crate::infrastructure::bevy_adapters::rocket_guidance::guidance_system;
    use crate::infrastructure::bevy_adapters::rocket_propulsion::{
        propulsion_consumption, propulsion_staging,
    };
    use bevy::math::{DQuat, DVec3};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    const DT: f64 = 1.0 / 64.0;

    /// Electron-class vehicle: nine 25.8 kN engines with a 0.6 throttle
    /// floor (stage envelope [0.6, 1.0]), ~13 t gross.
    pub(super) fn electron_like() -> Rocket {
        let engines = (0..9)
            .map(|i| {
                let angle = i as f32 * std::f32::consts::TAU / 9.0;
                RocketEngine {
                    position_m: bevy::math::Vec3::new(
                        0.45 * angle.cos(),
                        -6.05,
                        0.45 * angle.sin(),
                    ),
                    thrust_axis: bevy::math::Vec3::Y,
                    isp_sea_level: 303.0,
                    isp_vacuum: 311.0,
                    gimbal_range_deg: 4.0,
                    rated_thrust_kn: 25.8,
                    thrust_reference: ThrustReference::SeaLevel,
                    throttle_min: 0.6,
                    throttle_max: 1.0,
                    max_ignitions: 2,
                    ignition_count: 1,
                    state: EngineState::Running,
                }
            })
            .collect();
        Rocket {
            name: "Electron".into(),
            diameter_m: 1.2,
            height_m: 18.0,
            stages: vec![
                RocketStage {
                    name: "S1".into(),
                    diameter_m: 1.2,
                    height_m: 12.1,
                    dry_mass_kg: 950.0,
                    propellant_mass_kg: 9_250.0,
                    recovery_propellant_reserve_kg: None,
                    landing_gear: None,
                    fairing_dry_mass_kg: None,
                    engines,
                },
                RocketStage {
                    name: "S2".into(),
                    diameter_m: 1.2,
                    height_m: 2.4,
                    dry_mass_kg: 250.0,
                    propellant_mass_kg: 2_050.0,
                    recovery_propellant_reserve_kg: None,
                    landing_gear: None,
                    fairing_dry_mass_kg: None,
                    engines: vec![RocketEngine {
                        position_m: bevy::math::Vec3::new(0.0, -1.2, 0.0),
                        thrust_axis: bevy::math::Vec3::Y,
                        isp_sea_level: 311.0,
                        isp_vacuum: 343.0,
                        gimbal_range_deg: 4.0,
                        rated_thrust_kn: 25.8,
                        thrust_reference: ThrustReference::Vacuum,
                        throttle_min: 0.6,
                        throttle_max: 1.0,
                        max_ignitions: 2,
                        ignition_count: 1,
                        state: EngineState::Running,
                    }],
                },
            ],
            parallel_boosters: None,
        }
    }

    pub(super) fn ascent_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<SplashdownDetectedEvent>();
        app.insert_resource(SimulationTime::new(DT));
        app.insert_resource(test_ephemeris_snapshot());
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            DT,
        )));

        let planet =
            crate::domain::services::planet_factory::PlanetFactory::create_by_name("Earth")
                .expect("Earth exists");
        app.world_mut().spawn((
            PlanetComponent {
                domain_planet: planet,
                material: Handle::default(),
                has_texture: false,
                base_reflectance: 1.0,
                base_roughness: 1.0,
            },
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::earth(),
        ));

        let vehicle = electron_like();
        let propellant = vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        let total_mass_kg = vehicle.total_mass_kg() as f64;
        let (inertia, com) = crate::domain::services::rocket_dynamics::rocket_inertia_tensor(
            1_250.0, 11_300.0, 0.6, 18.0,
        );

        let (lat, lon) = ksc_terrain_coordinates();
        let h = crate::infrastructure::bevy_adapters::components::PlanetTerrain::earth()
            .source
            .height_m(lat, lon);
        let up = radial_direction(lat, lon);
        let surface_radius = EARTH_RADIUS_M + h;

        let vehicle_entity = app
            .world_mut()
            .spawn((
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        up * surface_radius,
                        DVec3::ZERO,
                        DQuat::from_rotation_arc(DVec3::Y, up),
                        total_mass_kg,
                        inertia,
                        com,
                    ),
                },
                RocketGeometry {
                    radius_m: 0.6,
                    height_m: 18.0,
                    lower_extent_y_m: -9.0,
                },
                RocketFlightConditions::default(),
                RocketMissionState::Launch,
                RocketPropulsion {
                    vehicle,
                    active_stage: 0,
                    propellant_remaining_kg: propellant,
                    booster_propellant_remaining_kg: Vec::new(),
                    boosters_attached: false,
                    throttle: 0.0,
                    gimbal_pitch_rad: 0.0,
                    gimbal_yaw_rad: 0.0,
                    time_since_separation_s: 10.0,
                    ullage_settle_time_s: 2.0,
                    separations_count: 0,
                    attached_payload_kg: 50.0,
                },
            ))
            .id();
        // Second insert: Bevy bundle tuples cap at 15 items.
        app.world_mut().entity_mut(vehicle_entity).insert((
            RocketCommands::default(),
            RocketAutopilot::default(),
            OrbitalElements::default(),
            TerrainCollisionState::default(),
            GroundRest { active: true },
            TipOverState::default(),
            LandingScorecard::default(),
            RocketPlanetBinding {
                planet_name: CelestialBodyId::earth(),
            },
            GravityAcceleration { value: -up * 9.81 },
            ForceAccumulator::default(),
            TorqueAccumulator::default(),
        ));

        // Mirror the production force writers: non-gravity forces only —
        // accumulate_forces owns the single gravity contribution.
        fn write_flight_forces(
            mut query: Query<(
                &RocketPhysicsState,
                &RocketPropulsion,
                &mut ForceAccumulator,
            )>,
        ) {
            for (rocket, propulsion, mut force) in query.iter_mut() {
                if let Some(stage) = propulsion.vehicle.stages.get(propulsion.active_stage) {
                    let (body_thrust, _) =
                        stage_thrust_body(&stage.engines, propulsion.throttle, 0.0);
                    force.0 += rocket.dynamics.orientation * body_thrust;
                }
            }
        }

        app.add_systems(
            FixedUpdate,
            (
                guidance_system,
                control_system,
                actuation_system,
                write_flight_forces,
                accumulate_forces,
                integrate_6dof,
                resolve_ground_contact,
            )
                .chain(),
        );
        app
    }

    #[test]
    fn throttle_slew_reaches_full_command_shortly_after_launch() {
        let mut app = ascent_app();

        // Sweep tick-by-tick and find the first tick where the effective
        // throttle reaches the commanded maximum (1.0). The slew limiter is
        // 2.0/s, so from the 0.6 envelope floor this must complete well
        // inside 1 s of the Launch transition.
        let mut ticks_to_full = None;
        for tick in 1..=64 {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<&RocketPropulsion>();
            let propulsion = q.single(world).unwrap();
            if propulsion.throttle >= 0.999 {
                ticks_to_full = Some(tick);
                break;
            }
        }

        let ticks = ticks_to_full.expect("throttle must reach full command");
        assert!(
            ticks <= 32,
            "slew reached full throttle at tick {ticks} ({:.2} s), expected within 0.5 s",
            ticks as f64 * DT
        );

        // Steady-state thrust must be the FULL 232 kN (nine engines at 100%),
        // not the 139 kN envelope-floor value: nothing downstream may cap the
        // command once the slew has caught up.
        let world = app.world_mut();
        let mut q = world.query::<(&RocketPropulsion, &RocketFlightConditions, &GroundRest)>();
        let (propulsion, conditions, _rest) = q.single(world).unwrap();
        let Some(stage) = propulsion.vehicle.stages.first() else {
            panic!("stage 1 missing");
        };
        let (thrust_body, _) = stage_thrust_body(
            &stage.engines,
            propulsion.throttle,
            conditions.ambient_pressure_pa,
        );
        assert!(
            thrust_body.length() > 230_000.0,
            "steady-state thrust {} N is not the expected full-throttle output",
            thrust_body.length()
        );
    }

    #[test]
    fn ascent_holds_vertical_through_gate_then_pitches_over() {
        let mut app = ascent_app();

        // First 5 s of simulated flight: below the gate the whole way
        // (altitude crosses 150 m around t≈5.5 s), so the guidance TARGET
        // must stay exactly on the local vertical.
        for _ in 0..320 {
            app.update();
        }
        {
            let world = app.world_mut();
            let mut q = world.query::<(&RocketCommands, &RocketPhysicsState)>();
            let (commands, rocket) = q.single(world).unwrap();
            let up = rocket.dynamics.position_m.normalize();
            let body_y_world = commands.target_attitude * DVec3::Y;
            assert!(
                (body_y_world.dot(up) - 1.0).abs() < 1e-9,
                "target attitude left vertical before the gate cleared: dot={}",
                body_y_world.dot(up)
            );
        }

        // Run well past the time-schedule start (t = 10 s): with the vehicle
        // high and fast the gate is clear and the combined schedule must have
        // produced a real pitch-over by t ≈ 20 s.
        for _ in 0..960 {
            app.update();
        }
        let world = app.world_mut();
        let mut q = world.query::<(
            &RocketCommands,
            &RocketPhysicsState,
            &GroundRest,
            &RocketPropulsion,
            &RocketMissionState,
            &TerrainCollisionState,
            &RocketAutopilot,
        )>();
        let (commands, rocket, rest, propulsion, mission, collision, autopilot) =
            q.single(world).unwrap();
        assert!(!rest.active, "vehicle must have lifted off");
        assert!(
            (propulsion.throttle - 1.0).abs() < 1e-3,
            "throttle must remain fully commanded in ascent: {}",
            propulsion.throttle
        );
        let radius = rocket.dynamics.position_m.length();
        let altitude = radius - EARTH_RADIUS_M;
        let vertical_speed = rocket
            .dynamics
            .velocity_mps
            .dot(rocket.dynamics.position_m / radius);
        let up = rocket.dynamics.position_m.normalize();
        let body_y_world = commands.target_attitude * DVec3::Y;
        assert!(
            body_y_world.dot(up) < 1.0 - 1e-4,
            "gravity turn never engaged after the gate cleared: dot={} \
             alt={altitude:.1} m, vs={vertical_speed:.1} m/s, t={:.2} s, \
             mission={mission:?}, contact={:?}, radar={:.1} m",
            body_y_world.dot(up),
            autopilot.time_since_liftoff_s,
            collision.ground_contact,
            collision.radar_altitude_m,
        );
    }

    /// A completed insertion is a property of the authoritative orbital state,
    /// not a speed threshold. A circular target state completes the ascent,
    /// while a nearly orbital state with an Earth-intersecting periapsis does
    /// not, even though both are evaluated by the same guidance system.
    #[test]
    fn target_orbit_predicate_controls_ascent_completion() {
        let target = LowEarthOrbitTarget::default();
        let radius_m = EARTH_RADIUS_M + target.target_apoapsis_altitude_m;
        let circular_speed_mps = (gravitational_parameter(5.972e24) / radius_m).sqrt();
        let orientation = test_earth_orientation();
        let reference_normal = planet_inertial_spin_axis(&orientation);
        let reference_x_axis = planet_equatorial_reference_x_axis(&orientation);
        let position_m = reference_x_axis * radius_m;
        let orbit_normal = reference_normal * target.target_inclination_rad.cos()
            - reference_normal.cross(reference_x_axis) * target.target_inclination_rad.sin();
        let circular_velocity_mps = orbit_normal.cross(position_m).normalize() * circular_speed_mps;

        let mut safe_app = ascent_app();
        let safe_entity = {
            let world = safe_app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            query.single(world).unwrap()
        };
        {
            let world = safe_app.world_mut();
            let mut rocket = world.get_mut::<RocketPhysicsState>(safe_entity).unwrap();
            rocket.dynamics.position_m = position_m;
            rocket.dynamics.velocity_mps = circular_velocity_mps;
            *world.get_mut::<RocketMissionState>(safe_entity).unwrap() = RocketMissionState::Ascent;
            world.get_mut::<RocketAutopilot>(safe_entity).unwrap().mode = AutopilotMode::Ascent;
        }
        // The first app update initializes Bevy's fixed-time clock; the
        // second executes the configured fixed-step flight pipeline.
        safe_app.update();
        safe_app.update();
        {
            let world = safe_app.world_mut();
            assert_eq!(
                *world.get::<RocketMissionState>(safe_entity).unwrap(),
                RocketMissionState::Orbit
            );
            assert_eq!(
                world
                    .get::<RocketCommands>(safe_entity)
                    .unwrap()
                    .throttle_cmd,
                0.0
            );
        }

        let mut unsafe_app = ascent_app();
        let unsafe_entity = {
            let world = unsafe_app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            query.single(world).unwrap()
        };
        {
            let world = unsafe_app.world_mut();
            let mut rocket = world.get_mut::<RocketPhysicsState>(unsafe_entity).unwrap();
            rocket.dynamics.position_m = position_m;
            rocket.dynamics.velocity_mps = circular_velocity_mps * 0.98;
            *world.get_mut::<RocketMissionState>(unsafe_entity).unwrap() =
                RocketMissionState::Ascent;
            world
                .get_mut::<RocketAutopilot>(unsafe_entity)
                .unwrap()
                .mode = AutopilotMode::Ascent;
        }
        unsafe_app.update();
        unsafe_app.update();
        assert_ne!(
            *unsafe_app
                .world()
                .get::<RocketMissionState>(unsafe_entity)
                .unwrap(),
            RocketMissionState::Orbit,
            "an unsafe periapsis must not complete ascent"
        );
        assert!(
            unsafe_app
                .world()
                .get::<RocketCommands>(unsafe_entity)
                .unwrap()
                .throttle_cmd
                > 0.0,
            "an unsafe periapsis must remain under ascent thrust rather than coasting into impact"
        );
    }

    #[test]
    fn orbital_coast_holds_attitude_and_damps_all_body_axes() {
        let target = LowEarthOrbitTarget::default();
        let radius_m = EARTH_RADIUS_M + target.target_apoapsis_altitude_m;
        let circular_speed_mps = (gravitational_parameter(5.972e24) / radius_m).sqrt();
        let initial_rate = DVec3::new(0.2, 0.0, -0.15);
        let mut app = ascent_app();
        let entity = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            query.single(world).unwrap()
        };
        {
            let world = app.world_mut();
            let mut rocket = world.get_mut::<RocketPhysicsState>(entity).unwrap();
            rocket.dynamics.position_m = DVec3::new(radius_m, 0.0, 0.0);
            rocket.dynamics.velocity_mps = DVec3::new(
                0.0,
                circular_speed_mps * target.target_inclination_rad.cos(),
                circular_speed_mps * target.target_inclination_rad.sin(),
            );
            rocket.dynamics.angular_velocity_radps = initial_rate;
            *world.get_mut::<RocketMissionState>(entity).unwrap() = RocketMissionState::Ascent;
        }

        // The first update initializes fixed time. The following two updates
        // complete insertion, enter the coast hold, and apply its RCS damping.
        app.update();
        app.update();
        app.update();

        let world = app.world();
        let rocket = world.get::<RocketPhysicsState>(entity).unwrap();
        let commands = world.get::<RocketCommands>(entity).unwrap();
        let autopilot = world.get::<RocketAutopilot>(entity).unwrap();
        assert_eq!(autopilot.mode, AutopilotMode::Off);
        assert_eq!(autopilot.integral, DVec3::ZERO);
        assert!(
            rocket.dynamics.angular_velocity_radps.x.abs() < initial_rate.x.abs()
                && rocket.dynamics.angular_velocity_radps.z.abs() < initial_rate.z.abs(),
            "orbital coast must damp residual pitch/yaw rates: {}",
            rocket.dynamics.angular_velocity_radps
        );
        assert!(
            commands
                .target_attitude
                .dot(rocket.dynamics.orientation)
                .abs()
                > 0.99,
            "orbital coast must hold the current attitude"
        );
    }

    /// Control allocates attitude torque only. It must retain the throttle
    /// target supplied by guidance for the subsequent actuation stage.
    #[test]
    fn control_preserves_guidance_throttle_command() {
        let mut app = ascent_app();
        app.world_mut()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        let entity = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketCommands>>();
            query.single(world).unwrap()
        };
        app.world_mut()
            .get_mut::<RocketCommands>(entity)
            .unwrap()
            .throttle_cmd = 0.37;
        app.add_systems(Update, control_system);
        app.update();

        assert_eq!(
            app.world()
                .get::<RocketCommands>(entity)
                .unwrap()
                .throttle_cmd,
            0.37
        );
    }

    /// Shared minimal burn rig (Phase 15/17): electron-class vehicle in
    /// vacuum, throttle forced fully open, only consumption/staging systems
    /// running so guidance policy cannot mask machinery behavior. Each
    /// FixedUpdate is scheduled at the requested warp frequency while every
    /// authoritative step remains `DT` simulation seconds.
    fn burn_rig_app(acceleration: f64) -> App {
        use bevy::asset::{AssetApp, AssetPlugin};

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.add_message::<StageSeparatedEvent>();
        let mut sim_time = SimulationTime::new(DT);
        sim_time.set_time_acceleration(acceleration);
        app.insert_resource(sim_time);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            DT / acceleration,
        )));

        let vehicle = electron_like();
        let propellant = vehicle
            .stages
            .iter()
            .map(|stage| stage.propellant_mass_kg)
            .collect();
        let total_mass_kg = vehicle.total_mass_kg() as f64;
        let (inertia, com) = crate::domain::services::rocket_dynamics::rocket_inertia_tensor(
            1_250.0, 11_300.0, 0.6, 18.0,
        );
        app.world_mut().spawn((
            RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    DVec3::new(EARTH_RADIUS_M + 100.0, 0.0, 0.0),
                    DVec3::new(0.0, 2_000.0, 0.0),
                    DQuat::IDENTITY,
                    total_mass_kg,
                    inertia,
                    com,
                ),
            },
            RocketGeometry {
                radius_m: 0.6,
                height_m: 18.0,
                lower_extent_y_m: -9.0,
            },
            RocketFlightConditions::default(),
            RocketPropulsion {
                vehicle,
                active_stage: 0,
                propellant_remaining_kg: propellant,
                booster_propellant_remaining_kg: Vec::new(),
                boosters_attached: false,
                throttle: 0.0,
                gimbal_pitch_rad: 0.0,
                gimbal_yaw_rad: 0.0,
                time_since_separation_s: 10.0,
                ullage_settle_time_s: 2.0,
                separations_count: 0,
                attached_payload_kg: 50.0,
            },
            RocketPlanetBinding {
                planet_name: CelestialBodyId::earth(),
            },
        ));

        // Burn rig: hold full commanded throttle, consume, stage.
        fn force_full_throttle(mut q: Query<&mut RocketPropulsion>) {
            for mut p in q.iter_mut() {
                p.throttle = 1.0;
            }
        }
        app.add_systems(
            FixedUpdate,
            (
                force_full_throttle,
                propulsion_consumption,
                propulsion_staging,
            )
                .chain(),
        );
        app.add_systems(
            Update,
            crate::domain::services::simulation_time::sync_fixed_timestep,
        );
        app
    }

    /// With time acceleration at 100×, propulsion still advances through
    /// bounded `DT` steps: one clean separation, drained first stage,
    /// conserved mass, and finite dynamics.
    /// A minimal burn rig drives the throttle directly so guidance policy
    /// (which legitimately coasts into insertion under coarse steps) cannot
    /// mask the machinery being tested.
    #[test]
    fn staging_and_consumption_stable_at_100x_acceleration() {
        let mut app = burn_rig_app(100.0);

        // Stage 1 drains in ~119 s; run well past that at bounded timesteps.
        for _ in 0..9_000 {
            app.world_mut().run_schedule(FixedUpdate);
        }

        let world = app.world_mut();
        let mut q = world.query::<(&RocketPhysicsState, &RocketPropulsion, &RocketGeometry)>();
        let (rocket, propulsion, geometry) = q.single(world).unwrap();

        assert_eq!(
            propulsion.separations_count, 1,
            "stage 1 must separate exactly once at 100×"
        );
        assert_eq!(propulsion.active_stage, 1);
        assert_eq!(propulsion.propellant_remaining_kg[0], 0.0);
        assert!(propulsion.propellant_remaining_kg[1] > 0.0);
        // Active-vehicle mass must equal its live components exactly:
        // upper-stage structure + whatever stage-2 propellant remains +
        // attached payload. (The rig keeps burning stage 2 after
        // separation, so absolute amounts are policy-free.)
        let expected_mass = propulsion.vehicle.stages[1].dry_mass_kg as f64
            + propulsion.propellant_remaining_kg[1] as f64
            + propulsion.attached_payload_kg as f64;
        assert!(
            (rocket.dynamics.mass_kg - expected_mass).abs() < 1.0,
            "mass {} inconsistent with stage bookkeeping {expected_mass}",
            rocket.dynamics.mass_kg
        );
        assert!(
            propulsion.propellant_remaining_kg[1] < propulsion.vehicle.stages[1].propellant_mass_kg,
            "stage 2 must also have consumed propellant post-staging"
        );
        assert!(rocket.dynamics.mass_kg.is_finite() && rocket.dynamics.mass_kg > 0.0);
        assert_eq!(geometry.radius_m, 0.6);
        assert_eq!(geometry.height_m, 2.4);

        // Stage separation must replace every geometry-dependent model input:
        // the detached first stage retains its own exterior while the live
        // upper stage supplies aero area, center of pressure, and inertia.
        let mut spent = world.query_filtered::<&RocketGeometry, With<SpentStage>>();
        let spent_geometry = spent.single(world).expect("separated first stage");
        assert_eq!(spent_geometry.radius_m, 0.6);
        assert_eq!(spent_geometry.height_m, 12.1);
        let active = world
            .query_filtered::<Entity, With<RocketPropulsion>>()
            .single(world)
            .unwrap();
        assert!(world.get::<LandingLegs>(active).is_none());
        assert_eq!(
            world
                .query_filtered::<&LandingLegs, With<SpentStage>>()
                .iter(world)
                .count(),
            0,
            "gear-less serial stages retain point-contact behavior"
        );
    }

    #[test]
    fn reserved_booster_enters_existing_recovery_pipeline_at_separation() {
        let mut app = burn_rig_app(1.0);
        {
            let world = app.world_mut();
            let mut propulsion = world
                .query::<&mut RocketPropulsion>()
                .single_mut(world)
                .unwrap();
            propulsion.vehicle.stages[0].recovery_propellant_reserve_kg = Some(100.0);
        }

        for _ in 0..20_000 {
            app.world_mut().run_schedule(FixedUpdate);
            let world = app.world_mut();
            let mut recovering = world.query_filtered::<(
                &RocketPropulsion,
                &RocketMissionState,
                &RocketAutopilot,
                &TerrainCollisionState,
            ), With<RecoveringStage>>();
            if let Ok((propulsion, mission, autopilot, _collision)) = recovering.single(world) {
                assert_eq!(*mission, RocketMissionState::Landing);
                assert_eq!(autopilot.mode, AutopilotMode::Boostback);
                assert_eq!(propulsion.propellant_remaining_kg, vec![100.0]);
                assert!(propulsion.vehicle.stages[0]
                    .recovery_propellant_reserve_kg
                    .is_none());
                return;
            }
        }

        panic!("a stage with a recovery reserve must enter the recovery flight pipeline");
    }

    #[test]
    fn falcon9_separation_moves_stage_local_gear_to_the_recovering_booster() {
        use crate::domain::services::landing_gear::LandingGear;

        let mut app = App::new();
        app.insert_resource(SimulationTime::new(DT));
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.add_message::<StageSeparatedEvent>();
        app.add_systems(FixedUpdate, propulsion_staging);

        let mut vehicle = Rocket::falcon9_test_fixture();
        vehicle.stages[1].fairing_dry_mass_kg = Some(50.0);
        let expected_gear = vehicle.stages[0]
            .landing_gear
            .expect("fixture stage 1 gear");
        let reserve_kg = vehicle.stages[0]
            .recovery_propellant_reserve_kg
            .expect("fixture stage 1 recovery reserve");
        let (inertia, center_of_mass_m) =
            crate::domain::services::rocket_dynamics::rocket_inertia_tensor(
                vehicle.total_dry_mass_kg() as f64,
                45_000.0,
                vehicle.diameter_m as f64 * 0.5,
                vehicle.height_m as f64,
            );
        let parent = app
            .world_mut()
            .spawn((
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        DVec3::new(EARTH_RADIUS_M + 1_000.0, 0.0, 0.0),
                        DVec3::new(0.0, 2_000.0, 0.0),
                        DQuat::IDENTITY,
                        67_250.0,
                        inertia,
                        center_of_mass_m,
                    ),
                },
                RocketGeometry {
                    radius_m: vehicle.diameter_m * 0.5,
                    height_m: vehicle.height_m,
                    lower_extent_y_m: vehicle.lower_extent_in_stack_m(),
                },
                RocketPropulsion {
                    vehicle,
                    active_stage: 0,
                    propellant_remaining_kg: vec![reserve_kg, 30_000.0],
                    booster_propellant_remaining_kg: Vec::new(),
                    boosters_attached: false,
                    throttle: 0.0,
                    gimbal_pitch_rad: 0.0,
                    gimbal_yaw_rad: 0.0,
                    time_since_separation_s: 1.0,
                    ullage_settle_time_s: 0.0,
                    separations_count: 0,
                    attached_payload_kg: 50.0,
                },
                RocketPlanetBinding {
                    planet_name: CelestialBodyId::earth(),
                },
                LandingLegs::new(LandingGear::new(expected_gear, 67_250.0)),
            ))
            .id();

        app.world_mut().run_schedule(FixedUpdate);

        let world = app.world_mut();
        let upper = world.get::<RocketPropulsion>(parent).unwrap();
        assert_eq!(upper.active_stage, 1);
        assert_eq!(upper.attached_payload_kg, 50.0);
        assert_eq!(upper.vehicle.stages[1].fairing_dry_mass_kg, Some(50.0));
        assert!(upper.vehicle.stages[1].landing_gear.is_none());
        assert!(world.get::<LandingLegs>(parent).is_none());
        assert!(world.get::<Children>(parent).is_none());

        let mut recovering = world.query_filtered::<(
            Entity,
            &RocketPhysicsState,
            &RocketPropulsion,
            &LandingLegs,
        ), With<RecoveringStage>>();
        let (spent, physics, propulsion, legs) = recovering.single(world).unwrap();
        assert_eq!(legs.gear.spec, expected_gear);
        assert_eq!(
            propulsion.vehicle.stages[0].landing_gear,
            Some(expected_gear)
        );
        assert!((physics.dynamics.mass_kg - 33_000.0).abs() < 1e-6);
        assert_eq!(propulsion.attached_payload_kg, 0.0);
        assert!(propulsion.vehicle.stages[0].fairing_dry_mass_kg.is_none());
        assert!(physics.dynamics.inertia_body.is_finite());
        assert!(world.get::<Children>(spent).is_some());
    }

    /// Phase 17 scenario `time_warp_burn`: the SAME burn executed at 1×, 10×
    /// and 100× must produce identical staging decisions and consistent mass
    /// bookkeeping at equal SIMULATED time. Consumption is linear in sim
    /// seconds, so staging and mass bookkeeping must match exactly.
    #[test]
    fn burn_rig_invariants_hold_across_time_warp_factors() {
        let mut expected_post_burn: Option<(f32, f64)> = None;

        for acceleration in [1.0, 10.0, 100.0] {
            let mut app = burn_rig_app(acceleration);

            // Same simulated duration for every factor: T = 140 s > the
            // ~119 s first-stage drain. Each fixed tick advances by DT.
            let steps = (140.0 / DT).ceil() as usize + 2;
            for _ in 0..steps {
                app.world_mut().run_schedule(FixedUpdate);
            }

            let world = app.world_mut();
            let mut q = world.query::<(&RocketPhysicsState, &RocketPropulsion)>();
            let (rocket, propulsion) = q.single(world).unwrap();

            assert_eq!(
                propulsion.separations_count, 1,
                "staging decision must not depend on time-warp factor"
            );
            assert_eq!(propulsion.active_stage, 1);
            assert_eq!(propulsion.propellant_remaining_kg[0], 0.0);
            assert!(propulsion.propellant_remaining_kg[1] > 0.0);
            let expected_mass = propulsion.vehicle.stages[1].dry_mass_kg as f64
                + propulsion.propellant_remaining_kg[1] as f64
                + propulsion.attached_payload_kg as f64;
            assert!(
                (rocket.dynamics.mass_kg - expected_mass).abs() < 1.0,
                "mass bookkeeping diverged at {acceleration}x: {} vs {expected_mass}",
                rocket.dynamics.mass_kg
            );
            assert!(rocket.dynamics.mass_kg.is_finite());

            let post_burn = (
                propulsion.propellant_remaining_kg[1],
                rocket.dynamics.mass_kg,
            );
            if let Some(expected) = expected_post_burn {
                assert!(
                    (post_burn.0 - expected.0).abs() < f32::EPSILON,
                    "stage-2 propellant diverged at {acceleration}x: {} vs {}",
                    post_burn.0,
                    expected.0
                );
                assert!(
                    (post_burn.1 - expected.1).abs() < f64::EPSILON,
                    "mass diverged at {acceleration}x: {} vs {}",
                    post_burn.1,
                    expected.1
                );
            } else {
                expected_post_burn = Some(post_burn);
            }
        }
    }

    /// Phase 17 determinism: two identical full-pipeline ascent runs (same
    /// initial state, same step count) must produce bitwise-identical
    /// authoritative state. The architecture supports this: f64 math, fixed
    /// steps, single-vehicle iteration, chained systems — so exact equality
    /// is required rather than a tolerance.
    #[test]
    fn identical_ascent_runs_are_bitwise_deterministic() {
        let mut run_a = ascent_app();
        let mut run_b = ascent_app();

        // ~6.25 s of simulated flight: liftoff, throttle slew, gate region.
        for _ in 0..400 {
            run_a.update();
            run_b.update();
        }

        let snapshot = |app: &mut App| {
            let world = app.world_mut();
            let mut q = world.query::<(
                &RocketPhysicsState,
                &RocketPropulsion,
                &RocketMissionState,
                &RocketCommands,
                &GravityAcceleration,
            )>();
            let (rocket, propulsion, mission, commands, gravity) = q.single(world).unwrap();
            (
                rocket.dynamics.position_m,
                rocket.dynamics.velocity_mps,
                rocket.dynamics.orientation,
                rocket.dynamics.angular_velocity_radps,
                rocket.dynamics.mass_kg,
                propulsion.active_stage,
                propulsion.throttle,
                propulsion.separations_count,
                propulsion.propellant_remaining_kg.clone(),
                *mission,
                commands.target_attitude,
                gravity.value,
            )
        };

        let a = snapshot(&mut run_a);
        let b = snapshot(&mut run_b);

        let bits = |q: &DQuat| [q.x.to_bits(), q.y.to_bits(), q.z.to_bits(), q.w.to_bits()];
        let vec3 = |v: &DVec3| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];

        assert_eq!(vec3(&a.0), vec3(&b.0), "position diverged");
        assert_eq!(vec3(&a.1), vec3(&b.1), "velocity diverged");
        assert_eq!(bits(&a.2), bits(&b.2), "orientation diverged");
        assert_eq!(vec3(&a.3), vec3(&b.3), "angular velocity diverged");
        assert_eq!(a.4.to_bits(), b.4.to_bits(), "dynamics mass diverged");
        assert_eq!(a.5, b.5, "active stage diverged");
        assert_eq!(a.6.to_bits(), b.6.to_bits(), "throttle diverged");
        assert_eq!(a.7, b.7, "separations count diverged");
        assert_eq!(a.8, b.8, "propellant vector diverged");
        assert_eq!(a.9, b.9, "mission state diverged");
        assert_eq!(
            bits(&a.10),
            bits(&b.10),
            "guidance target attitude diverged"
        );
        assert_eq!(vec3(&a.11), vec3(&b.11), "gravity diverged");
    }
}

#[cfg(test)]
mod stage_coordinate_pipeline_tests {
    use super::*;
    use crate::domain::entities::rocket::{
        EngineState, Rocket, RocketEngine, RocketStage, ThrustReference,
    };
    use crate::domain::services::rocket_propulsion::{
        clamp_gimbal, engine_thrust_n, gimbal_torque_body,
    };
    use crate::domain::services::simulation_time::SimulationTime;
    use crate::infrastructure::bevy_adapters::rocket_propulsion::propulsion_gimbal;

    #[test]
    fn detached_stage_gimbal_uses_its_declared_stage_local_station() {
        let engine = RocketEngine {
            position_m: bevy::math::Vec3::new(0.0, -5.0, 0.0),
            thrust_axis: bevy::math::Vec3::Y,
            isp_sea_level: 250.0,
            isp_vacuum: 300.0,
            gimbal_range_deg: 10.0,
            rated_thrust_kn: 100.0,
            thrust_reference: ThrustReference::SeaLevel,
            throttle_min: 0.0,
            throttle_max: 1.0,
            max_ignitions: 2,
            ignition_count: 1,
            state: EngineState::Running,
        };
        let vehicle = Rocket {
            name: "Detached stage".into(),
            diameter_m: 2.0,
            height_m: 10.0,
            stages: vec![RocketStage {
                name: "Booster".into(),
                diameter_m: 2.0,
                height_m: 10.0,
                dry_mass_kg: 100.0,
                propellant_mass_kg: 900.0,
                recovery_propellant_reserve_kg: None,
                landing_gear: None,
                fairing_dry_mass_kg: None,
                engines: vec![engine.clone()],
            }],
            parallel_boosters: None,
        };
        let center_of_mass_m = DVec3::new(0.0, -2.5, 0.0);
        let mut app = App::new();
        app.insert_resource(SimulationTime::new(1.0 / 64.0));
        app.add_systems(FixedUpdate, propulsion_gimbal);
        app.world_mut().spawn((
            SpentStage {
                parent_rocket: Entity::PLACEHOLDER,
                kind: SpentStageKind::Booster,
            },
            RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    DVec3::ZERO,
                    DVec3::ZERO,
                    DQuat::IDENTITY,
                    1_000.0,
                    DMat3::IDENTITY,
                    center_of_mass_m,
                ),
            },
            RocketGeometry {
                radius_m: 1.0,
                height_m: 10.0,
                lower_extent_y_m: -5.0,
            },
            RocketFlightConditions::default(),
            RocketPropulsion {
                vehicle,
                active_stage: 0,
                propellant_remaining_kg: vec![900.0],
                booster_propellant_remaining_kg: Vec::new(),
                boosters_attached: false,
                throttle: 1.0,
                gimbal_pitch_rad: 0.1,
                gimbal_yaw_rad: 0.0,
                time_since_separation_s: 10.0,
                ullage_settle_time_s: 0.0,
                separations_count: 1,
                attached_payload_kg: 0.0,
            },
            TorqueAccumulator::default(),
        ));

        app.world_mut().run_schedule(FixedUpdate);

        let torque = app
            .world_mut()
            .query::<&TorqueAccumulator>()
            .single(app.world())
            .unwrap()
            .0;
        let expected = gimbal_torque_body(
            engine.position_m.as_dvec3(),
            center_of_mass_m,
            engine.thrust_axis.as_dvec3(),
            engine_thrust_n(&engine, 1.0, 0.0),
            clamp_gimbal(0.1, engine.gimbal_range_deg) as f64,
            0.0,
        );
        assert!(
            (torque - expected).length() < 1e-9,
            "{torque} vs {expected}"
        );
    }
}

#[cfg(test)]
mod parallel_booster_pipeline_tests {
    use super::*;
    use crate::domain::entities::rocket::{
        EngineState, ParallelBoosters, Rocket, RocketEngine, RocketStage, ThrustReference,
    };
    use crate::domain::events::StageSeparatedEvent;
    use crate::domain::services::rocket_dynamics::{rocket_inertia_tensor, RocketDynamicsState};
    use crate::domain::services::simulation_time::SimulationTime;
    use crate::infrastructure::bevy_adapters::rocket_dynamics::{
        accumulate_forces, integrate_6dof,
    };
    use crate::infrastructure::bevy_adapters::rocket_propulsion::{
        propulsion_consumption, propulsion_staging, propulsion_thrust,
    };

    fn engine(thrust_kn: f32) -> RocketEngine {
        RocketEngine {
            position_m: bevy::math::Vec3::new(0.0, -4.0, 0.0),
            thrust_axis: bevy::math::Vec3::Y,
            isp_sea_level: 1_000.0,
            isp_vacuum: 1_000.0,
            gimbal_range_deg: 0.0,
            rated_thrust_kn: thrust_kn,
            thrust_reference: ThrustReference::SeaLevel,
            throttle_min: 1.0,
            throttle_max: 1.0,
            max_ignitions: 1,
            ignition_count: 1,
            state: EngineState::Running,
        }
    }

    fn parallel_vehicle() -> Rocket {
        Rocket {
            name: "Parallel test".into(),
            diameter_m: 2.0,
            height_m: 12.0,
            stages: vec![RocketStage {
                name: "Core".into(),
                diameter_m: 2.0,
                height_m: 12.0,
                dry_mass_kg: 100.0,
                propellant_mass_kg: 100.0,
                recovery_propellant_reserve_kg: None,
                landing_gear: None,
                fairing_dry_mass_kg: None,
                engines: vec![engine(100.0)],
            }],
            parallel_boosters: Some(ParallelBoosters::new(
                RocketStage {
                    name: "Booster".into(),
                    diameter_m: 2.0,
                    height_m: 8.0,
                    dry_mass_kg: 20.0,
                    propellant_mass_kg: 20.0,
                    recovery_propellant_reserve_kg: None,
                    landing_gear: None,
                    fairing_dry_mass_kg: None,
                    engines: vec![engine(50.0)],
                },
                vec![
                    bevy::math::Vec3::new(-3.0, -1.0, 0.0),
                    bevy::math::Vec3::new(3.0, -1.0, 0.0),
                ],
            )),
        }
    }

    fn run_parallel_sequence() -> (f64, f64, Vec<(DVec3, DVec3)>) {
        let mut app = App::new();
        app.insert_resource(SimulationTime::new(0.1));
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.add_message::<StageSeparatedEvent>();
        app.add_systems(
            FixedUpdate,
            (
                propulsion_thrust,
                accumulate_forces,
                integrate_6dof,
                propulsion_consumption,
                propulsion_staging,
            )
                .chain(),
        );
        let vehicle = parallel_vehicle();
        let (inertia, com) = rocket_inertia_tensor(280.0, 0.0, 1.0, 12.0);
        let rocket_entity = app
            .world_mut()
            .spawn((
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        DVec3::ZERO,
                        DVec3::ZERO,
                        DQuat::IDENTITY,
                        280.0,
                        inertia,
                        com,
                    ),
                },
                RocketGeometry {
                    radius_m: 1.0,
                    height_m: 12.0,
                    lower_extent_y_m: -6.0,
                },
                RocketFlightConditions::default(),
                RocketPropulsion {
                    vehicle,
                    active_stage: 0,
                    propellant_remaining_kg: vec![100.0],
                    booster_propellant_remaining_kg: vec![20.0, 20.0],
                    boosters_attached: true,
                    throttle: 1.0,
                    gimbal_pitch_rad: 0.0,
                    gimbal_yaw_rad: 0.0,
                    time_since_separation_s: 0.0,
                    ullage_settle_time_s: 0.0,
                    separations_count: 0,
                    attached_payload_kg: 0.0,
                },
                RetroPropulsionEffect::default(),
                ForceAccumulator::default(),
                TorqueAccumulator::default(),
                GravityAcceleration::default(),
                RocketPlanetBinding {
                    planet_name: CelestialBodyId::earth(),
                },
            ))
            .id();

        app.world_mut().run_schedule(FixedUpdate);
        let joined_velocity_mps = app
            .world()
            .get::<RocketPhysicsState>(rocket_entity)
            .unwrap()
            .dynamics
            .velocity_mps
            .y;
        assert!(
            joined_velocity_mps > 0.07,
            "core + two boosters must exceed the core-only 100 kN impulse"
        );

        app.world_mut()
            .get_mut::<RocketPropulsion>(rocket_entity)
            .unwrap()
            .booster_propellant_remaining_kg = vec![0.0, 0.0];
        app.world_mut().run_schedule(FixedUpdate);
        let post_jettison = app.world().get::<RocketPropulsion>(rocket_entity).unwrap();
        assert!(!post_jettison.boosters_attached);
        assert!(post_jettison.booster_propellant_remaining_kg.is_empty());
        assert_eq!(
            post_jettison.active_stage, 0,
            "core serial stage must remain active"
        );
        let core_remaining_after_jettison = post_jettison.propellant_remaining_kg[0];
        let mut debris = app
            .world_mut()
            .query_filtered::<&RocketPhysicsState, With<SpentStage>>()
            .iter(app.world())
            .map(|state| (state.dynamics.position_m, state.dynamics.velocity_mps))
            .collect::<Vec<_>>();
        debris.sort_by(|left, right| left.0.x.total_cmp(&right.0.x));
        assert_eq!(debris.len(), 2);
        assert!(debris[0].0.distance(debris[1].0) > 2.0);
        assert_eq!(
            app.world_mut()
                .query_filtered::<&LandingLegs, With<SpentStage>>()
                .iter(app.world())
                .count(),
            0,
            "parallel boosters must not inherit core landing gear"
        );

        app.world_mut().run_schedule(FixedUpdate);
        assert!(
            app.world()
                .get::<RocketPropulsion>(rocket_entity)
                .unwrap()
                .propellant_remaining_kg[0]
                < core_remaining_after_jettison,
            "the core must continue burning after booster separation"
        );
        (
            joined_velocity_mps,
            core_remaining_after_jettison as f64,
            debris,
        )
    }

    #[test]
    fn boosters_join_the_fixed_pipeline_then_jettison_deterministically() {
        let first = run_parallel_sequence();
        let second = run_parallel_sequence();
        assert_eq!(first.0.to_bits(), second.0.to_bits());
        assert_eq!(first.1.to_bits(), second.1.to_bits());
        assert_eq!(
            first
                .2
                .iter()
                .map(|(p, v)| (
                    p.to_array().map(f64::to_bits),
                    v.to_array().map(f64::to_bits)
                ))
                .collect::<Vec<_>>(),
            second
                .2
                .iter()
                .map(|(p, v)| (
                    p.to_array().map(f64::to_bits),
                    v.to_array().map(f64::to_bits)
                ))
                .collect::<Vec<_>>(),
        );
    }
}

#[cfg(test)]
mod render_interpolation_tests {
    use super::*;
    use crate::infrastructure::bevy_adapters::rocket_lifecycle::handle_rocket_launch_input;
    use crate::infrastructure::bevy_adapters::rocket_presentation::render_dynamics_state;

    #[test]
    fn render_dynamics_interpolates_distinct_fixed_snapshots() {
        let current = RocketDynamicsState::new(
            DVec3::new(10.0, 20.0, 30.0),
            DVec3::new(1.0, 2.0, 3.0),
            DQuat::IDENTITY,
            100.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let previous = RocketDynamicsState {
            position_m: DVec3::new(-10.0, -20.0, -30.0),
            ..current
        };

        let rendered = render_dynamics_state(
            RocketRenderState {
                prev: previous,
                current,
            },
            0.5,
        );

        assert_eq!(rendered.position_m, DVec3::ZERO);
    }

    #[test]
    fn airborne_render_keeps_fixed_state_interpolation() {
        let current = RocketDynamicsState::new(
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::ZERO,
            DQuat::IDENTITY,
            100.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let previous = RocketDynamicsState {
            position_m: DVec3::ZERO,
            ..current
        };

        let rendered = render_dynamics_state(
            RocketRenderState {
                prev: previous,
                current,
            },
            0.25,
        );

        assert_eq!(rendered.position_m, DVec3::new(2.5, 0.0, 0.0));
    }

    #[test]
    fn terminal_surface_states_render_the_latest_fixed_snapshot() {
        let dynamics = RocketDynamicsState::new(
            DVec3::new(10.0, 20.0, 30.0),
            DVec3::new(1.0, 2.0, 3.0),
            DQuat::IDENTITY,
            100.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let stale_snapshot = RocketDynamicsState {
            position_m: DVec3::new(-10.0, -20.0, -30.0),
            ..dynamics
        };
        let mut app = App::new();
        app.add_systems(
            Update,
            crate::infrastructure::bevy_adapters::rocket_presentation::capture_render_state,
        );
        for (mission, ground_rest) in [
            (RocketMissionState::Landing, false),
            (RocketMissionState::Landed, false),
            (RocketMissionState::Crashed, false),
            (RocketMissionState::Ascent, true),
        ] {
            app.world_mut().spawn((
                RocketPhysicsState { dynamics },
                mission,
                GroundRest {
                    active: ground_rest,
                },
                RocketRenderState {
                    prev: stale_snapshot,
                    current: stale_snapshot,
                },
            ));
        }

        app.update();

        let world = app.world_mut();
        let mut render_states = world.query::<&RocketRenderState>();
        for render in render_states.iter(world) {
            assert_eq!(render.prev, dynamics);
            assert_eq!(render.current, dynamics);
            assert_eq!(render_dynamics_state(*render, 0.0), dynamics);
            assert_eq!(render_dynamics_state(*render, 0.999), dynamics);
        }
    }

    #[test]
    fn launch_resets_pad_snapshots_before_enabling_interpolation() {
        let dynamics = RocketDynamicsState::new(
            DVec3::new(10.0, 20.0, 30.0),
            DVec3::new(1.0, 2.0, 3.0),
            DQuat::IDENTITY,
            100.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let stale_snapshot = RocketDynamicsState {
            position_m: DVec3::new(-10.0, -20.0, -30.0),
            ..dynamics
        };
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        let rocket = app
            .world_mut()
            .spawn((
                RocketMissionState::PreLaunch,
                RocketPhysicsState { dynamics },
                RocketRenderState {
                    prev: stale_snapshot,
                    current: stale_snapshot,
                },
            ))
            .id();
        app.add_systems(Update, handle_rocket_launch_input);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);

        app.update();

        let world = app.world();
        assert_eq!(
            *world.get::<RocketMissionState>(rocket).unwrap(),
            RocketMissionState::Launch
        );
        let render = world.get::<RocketRenderState>(rocket).unwrap();
        assert_eq!(render.prev, dynamics);
        assert_eq!(render.current, dynamics);
    }
}

/// Baseline-recording regression suite (spec: determinism-regression). This
/// module reuses the full ascent-pipeline harness and converts the
/// authoritative state into [`RocketStateSample`] rows for the
/// [`domain::services::regression`] toolkit:
///
/// - records a canonical ascent baseline to `tests/baselines/ascent.ron`
///   (commit it as the CI gate fixture),
/// - re-simulates and compares per-tick/per-variable within the documented
///   tolerances, and
/// - proves the whole recording is bitwise reproducible across two fresh runs
///   (the deterministic-regression guarantee, AGENTS.md section 44/46).
///
/// Baseline recording is intentionally outside this test module so changing a
/// fixture always requires an explicit reviewed audit trail.
#[cfg(test)]
mod determinism_regression_tests {
    use super::ascent_pipeline_tests::ascent_app;
    use crate::components::rocket::{RocketMissionState, RocketPhysicsState};
    use crate::domain::entities::rocket::RocketMissionState as DomainMission;
    use crate::domain::services::regression::{
        load_baseline_ron, save_baseline_ron, RegressionConfig, RocketStateSample,
    };
    use bevy::prelude::*;

    /// Fixed-physics ticks captured in the baseline window (~4 s of ascent:
    /// liftoff, throttle slew, gravity-turn entry). Kept short so the fixture
    /// stays small while still exercising the full Guidance→Control→Forces→
    /// Integrate→GroundContact chain.
    const RECORD_TICKS: usize = 256;

    fn default_baseline_path() -> std::path::PathBuf {
        let dir =
            std::env::var("REGRESSION_BASELINE_DIR").unwrap_or_else(|_| "tests/baselines".into());
        std::path::PathBuf::from(dir).join("ascent.ron")
    }

    /// Map the mission phase to a stable, order-independent code byte. The
    /// codes are fixed enum indices so a reordered enum would change the
    /// recorded trajectory's guidance_mode field — exactly what the regression
    /// gate should catch.
    fn mission_code(mission: RocketMissionState) -> u8 {
        match mission.0 {
            DomainMission::PreLaunch => 0,
            DomainMission::Launch => 1,
            DomainMission::Ascent => 2,
            DomainMission::Orbit => 3,
            DomainMission::DeorbitBurn => 4,
            DomainMission::ReentryCorridor => 5,
            DomainMission::PoweredDescent => 6,
            DomainMission::UnpoweredDescent => 7,
            DomainMission::Landing => 8,
            DomainMission::Landed => 9,
            DomainMission::Crashed => 10,
        }
    }

    /// Capture one authoritative state row from the single rocket.
    fn capture_sample(app: &mut App) -> RocketStateSample {
        let world = app.world_mut();
        let mut q = world.query::<(&RocketPhysicsState, &RocketMissionState)>();
        let (rocket, mission) = q.single(world).unwrap();
        let d = &rocket.dynamics;
        RocketStateSample::new(
            [d.position_m.x, d.position_m.y, d.position_m.z],
            [d.velocity_mps.x, d.velocity_mps.y, d.velocity_mps.z],
            [
                d.orientation.x,
                d.orientation.y,
                d.orientation.z,
                d.orientation.w,
            ],
            [
                d.angular_velocity_radps.x,
                d.angular_velocity_radps.y,
                d.angular_velocity_radps.z,
            ],
            d.mass_kg,
            mission_code(*mission),
        )
    }

    /// Run the ascent harness and record `RECORD_TICKS` samples (one per
    /// fixed physics step), including the initial t=0 sample.
    fn record_ascent_samples() -> Vec<RocketStateSample> {
        let mut app = ascent_app();
        let mut samples = Vec::with_capacity(RECORD_TICKS + 1);
        samples.push(capture_sample(&mut app));
        for _ in 0..RECORD_TICKS {
            app.update();
            samples.push(capture_sample(&mut app));
        }
        samples
    }

    /// The CI gate: the freshly simulated ascent must match the committed
    /// baseline within the documented per-variable tolerances. Baselines are
    /// deliberately immutable in tests: recording requires an explicit,
    /// reviewed maintenance workflow outside CI.
    #[test]
    fn ascent_matches_committed_baseline_within_tolerances() {
        let path = default_baseline_path();
        assert!(
            path.exists(),
            "committed ascent baseline is missing; record and review it outside the test suite"
        );
        let ron = std::fs::read_to_string(&path).expect("baseline readable");
        let baseline = load_baseline_ron(&ron).expect("baseline RON valid");

        assert!(
            baseline.audit.is_signed_off(),
            "committed baseline audit trail is incomplete or unapproved"
        );

        assert!(
            baseline.hash_chain_consistent(),
            "committed baseline hash chain is internally inconsistent"
        );

        let current = record_ascent_samples();
        if std::env::var_os("REGRESSION_RECORD").is_some() {
            let mut recorded = baseline;
            recorded.git_commit = std::process::Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|commit| commit.trim().to_owned())
                .filter(|commit| !commit.is_empty())
                .unwrap_or_else(|| "uncommitted".to_string());
            recorded.samples = current;
            recorded.hash_chain = recorded.recompute_hash_chain();
            std::fs::write(
                &path,
                save_baseline_ron(&recorded).expect("baseline serializes"),
            )
            .expect("baseline writes");
            return;
        }
        let divergences = baseline.compare(&current, &RegressionConfig::default());

        assert!(
            divergences.is_empty(),
            "ascent diverged from committed baseline:\n{}",
            divergences
                .iter()
                .map(|d| d.describe())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// Two independent fresh-run recordings of the same flight must be
    /// bitwise identical (AGENTS.md section 44). This exercises the full
    /// harness through the public regression API rather than a hand-written
    /// snapshot, so a future change to either side is caught by the same
    /// path.
    #[test]
    fn two_fresh_ascent_runs_are_bitwise_identical() {
        let a = record_ascent_samples();
        let b = record_ascent_samples();
        assert_eq!(a.len(), b.len());
        let divergences = crate::domain::services::regression::compare_trajectory(
            &a,
            &b,
            &RegressionConfig::default(),
        );
        assert!(
            divergences.is_empty(),
            "identical runs diverged:\n{}",
            divergences
                .iter()
                .map(|d| d.describe())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// A deliberately injected perturbation well beyond the per-variable
    /// tolerance (a 0.5 m/s velocity error at a mid-flight tick) must be
    /// caught at the exact tick and variable — the invariant the CI gate
    /// relies on.
    #[test]
    fn injected_regression_is_reported_at_exact_tick_and_variable() {
        let mut baseline = record_ascent_samples();
        baseline[100].velocity_mps[0] += 0.5; // ≫ 1 µm/s tolerance
        let reference = record_ascent_samples();
        let divergences = crate::domain::services::regression::compare_trajectory(
            &reference,
            &baseline,
            &RegressionConfig::default(),
        );
        let hit = divergences
            .iter()
            .find(|d| {
                d.variable == crate::domain::services::regression::RegressionVariable::Velocity
            })
            .cloned();
        assert!(hit.is_some(), "velocity divergence must be reported");
        let hit = hit.unwrap();
        assert_eq!(hit.tick, 100);
        assert_eq!(
            hit.variable,
            crate::domain::services::regression::RegressionVariable::Velocity
        );
    }

    /// Sanity: the recorded baseline starts on the pad (ground contact) and
    /// reaches full throttle during the window — i.e. the fixture actually
    /// exercises the launch machinery and the samples are not all identical.
    #[test]
    fn recorded_baseline_is_a_real_flight() {
        let samples = match default_baseline_path().exists() {
            true => {
                let ron = std::fs::read_to_string(default_baseline_path()).unwrap();
                load_baseline_ron(&ron).unwrap().samples
            }
            false => record_ascent_samples(),
        };
        assert!(samples.len() > 1);
        // Position must move ~ a metre or more across the window (liftoff).
        let start = &samples[0];
        let end = &samples[samples.len() - 1];
        let dr = crate::domain::services::regression::vector_abs_diff(
            &start.position_m,
            &end.position_m,
        );
        assert!(
            dr > 1.0,
            "rocket barely moved ({dr:.3} m) — baseline is not a real flight"
        );
        // Velocity must grow (the vehicle is accelerating off the pad).
        let dv = crate::domain::services::regression::vector_abs_diff(
            &start.velocity_mps,
            &end.velocity_mps,
        );
        assert!(
            dv > 1.0,
            "rocket barely gained velocity ({dv:.3} m/s) — baseline is not an ascent"
        );
        // Mass must remain finite and positive throughout.
        assert!(
            samples
                .iter()
                .all(|s| s.mass_kg.is_finite() && s.mass_kg > 0.0),
            "mass became non-finite in the ascent baseline"
        );
        // Guidance must transition out of pre-launch.
        assert!(
            end.guidance_code >= 1,
            "guidance never advanced past pre-launch"
        );
    }
}
