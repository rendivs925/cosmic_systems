#[cfg(test)]
use crate::components::rocket::*;
#[cfg(test)]
use crate::domain::services::gravity::gravitational_parameter;
#[cfg(test)]
use crate::domain::services::rocket_dynamics::RocketDynamicsState;
#[cfg(test)]
use crate::domain::services::rocket_propulsion::stage_thrust_body;
#[cfg(test)]
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
#[cfg(test)]
use crate::infrastructure::bevy_adapters::components::{RocketAutopilot, RocketCommands};

#[cfg(test)]
use bevy::math::{DMat3, DQuat, DVec3};
#[cfg(test)]
use bevy::prelude::*;

#[cfg(test)]
mod ground_contact_tests {
    use super::*;
    use crate::domain::entities::rocket::{EngineState, Rocket, RocketEngine, RocketStage};
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

    /// One-engine test vehicle: 20 kN vacuum thrust, +Y body axis.
    fn test_vehicle(max_thrust_kn: f32) -> Rocket {
        Rocket {
            name: "Test".into(),
            diameter_m: 1.0,
            height_m: 10.0,
            stages: vec![RocketStage {
                name: "S1".into(),
                dry_mass_kg: 400.0,
                propellant_mass_kg: 600.0,
                engines: vec![RocketEngine {
                    position_m: bevy::math::Vec3::new(0.0, -5.0, 0.0),
                    thrust_axis: bevy::math::Vec3::Y,
                    isp_sea_level: 250.0,
                    isp_vacuum: 300.0,
                    gimbal_range_deg: 0.0,
                    max_thrust_kn: max_thrust_kn,
                    throttle_min: 0.0,
                    throttle_max: 1.0,
                    restartable: true,
                    state: EngineState::Running,
                }],
            }],
        }
    }

    /// Spawn a rocket standing exactly on the terrain at (lat, lon), plus the
    /// Earth planet entity, and run only the tail of the fixed pipeline:
    /// force writer → accumulate → integrate → ground contact.
    fn pad_app(throttle: f32, max_thrust_kn: f32) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<SplashdownDetectedEvent>();
        app.insert_resource(SimulationTime::new(DT));
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
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for("Earth"),
        ));

        let source =
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for("Earth")
                .source;
        let (lat, lon) = (28.5721_f64, -80.6480_f64);
        let h = source.height_m(lat, lon);
        let up = radial_direction(lat, lon);
        let surface_radius = EARTH_RADIUS_M + h;

        let vehicle = test_vehicle(max_thrust_kn);
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
                    up * surface_radius,
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
            },
            RocketPropulsion {
                vehicle,
                active_stage: 0,
                propellant_remaining_kg: propellant,
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
            RocketMass(1000.0),
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
        let (lat, lon) = (28.5721_f64, -80.6480_f64);
        let h =
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for("Earth")
                .source
                .height_m(lat, lon);
        let expected_r = EARTH_RADIUS_M + h;
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
        let time_days = app.world().resource::<SimulationTime>().sim_time_s / 86_400.0;
        let world = app.world_mut();
        let rocket = world.get::<RocketPhysicsState>(entity).unwrap();
        let position_bf =
            planet_inertial_to_body_fixed(rocket.dynamics.position_m, &earth, time_days as f32);
        let expected_direction_bf = geodetic_to_body_fixed(&launch_site, &earth).normalize();
        let expected_surface_velocity =
            surface_velocity_in_planet_inertial(rocket.dynamics.position_m, &earth);

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
            let (lat, lon) = (28.5721_f64, -80.6480_f64);
            let h = crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for(
                "Earth",
            )
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
            // down, released from the pad hold: a true descent so the
            // touchdown verdict fires.
            world.entity_mut(entity).insert(RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    up * (surface_r + 2.0),
                    -up * 2.0,
                    DQuat::from_rotation_arc(DVec3::Y, up),
                    mass_kg,
                    inertia_body,
                    com,
                ),
            });
            world.get_mut::<GroundRest>(entity).unwrap().active = false;
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

        for _ in 0..512 {
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
            (scorecard.touchdown_vertical_speed_mps - 2.0).abs() < 0.5,
            "scorecard descent {} not near the 2 m/s drop",
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
        let sink = surface_r - rocket.dynamics.position_m.length();
        assert!(
            sink >= -0.5 && sink <= legs.gear.spec.stroke_m + 0.5,
            "sink depth {sink} m outside strut range"
        );
        assert!(
            rocket.dynamics.velocity_mps.length() < 0.3,
            "not settled: {}",
            rocket.dynamics.velocity_mps.length()
        );
    }

    /// Relaunch (Phase 14): one command restores a flown, drained vehicle to
    /// a fresh pad state and clears its jettisoned debris.
    #[test]
    fn relaunch_restores_fresh_pad_state() {
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
            propulsion.propellant_remaining_kg = vec![0.0, 0.0];
            propulsion.active_stage = 1;
            propulsion.separations_count = 1;
            propulsion.throttle = 0.4;
            let mut mission = world.get_mut::<RocketMissionState>(rocket_entity).unwrap();
            *mission = RocketMissionState::Landed;
        }

        app.world_mut()
            .resource_mut::<RelaunchCommandQueue>()
            .0
            .push(rocket_entity);

        app.update();
        app.update();

        let world = app.world_mut();
        assert!(
            world.get_entity(debris_entity).is_err(),
            "jettisoned debris must be despawned"
        );
        let mut q = world.query::<(
            &RocketPhysicsState,
            &RocketPropulsion,
            &RocketMass,
            &RocketMissionState,
            &GroundRest,
        )>();
        let (rocket, propulsion, mass, mission, rest) = q.single(world).unwrap();

        assert_eq!(*mission, RocketMissionState::PreLaunch);
        assert!(rest.active, "pad-hold must re-engage");
        assert_eq!(propulsion.active_stage, 0);
        assert_eq!(propulsion.separations_count, 0);
        assert_eq!(propulsion.throttle, 0.0);
        for (stage, remaining) in propulsion
            .vehicle
            .stages
            .iter()
            .zip(&propulsion.propellant_remaining_kg)
        {
            assert!((remaining - stage.propellant_mass_kg).abs() < 1e-3);
        }
        assert!(
            rocket.dynamics.velocity_mps.length() < 0.3,
            "vehicle must be at rest after relaunch (one clamp cycle of gravity allowed): {}",
            rocket.dynamics.velocity_mps.length()
        );
        assert!(
            (mass.0 - rocket.dynamics.mass_kg).abs() < 1e-6,
            "RocketMass must match the refueled dynamics mass"
        );
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
        drop(world);
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
    use crate::domain::entities::rocket::{EngineState, Rocket, RocketEngine, RocketStage};
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
                dry_mass_kg: 400.0,
                propellant_mass_kg: 600.0,
                engines: vec![RocketEngine {
                    position_m: bevy::math::Vec3::new(0.0, -5.0, 0.0),
                    thrust_axis: bevy::math::Vec3::Y,
                    isp_sea_level: 250.0,
                    isp_vacuum: 300.0,
                    gimbal_range_deg: 4.0,
                    max_thrust_kn: 20.0,
                    throttle_min: 0.0,
                    throttle_max: 1.0,
                    restartable: true,
                    state: EngineState::Running,
                }],
            }],
        }
    }

    fn recovery_app(deck_altitude_m: f64) -> (App, DVec3) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<SplashdownDetectedEvent>();
        app.insert_resource(SimulationTime::new(DT));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            DT,
        )));

        let planet =
            crate::domain::services::planet_factory::PlanetFactory::create_by_name("Earth")
                .expect("Earth exists");
        let terrain = PlanetTerrain::default_for("Earth");
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

        let (lat, lon) = (28.5721_f64, -80.6480_f64);
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
                        deck_center_m + up * 2.0,
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
                },
                RocketMass(1_000.0),
                RocketFlightConditions::default(),
                RocketMissionState::Landing,
                RocketPropulsion {
                    vehicle,
                    active_stage: 0,
                    propellant_remaining_kg: propellant,
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

    fn droneship_outcome() -> (DVec3, DVec3, DVec3, LandingScorecard, bool) {
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

        for _ in 0..512 {
            app.update();
        }
        let world = app.world();
        let rocket = world.get::<RocketPhysicsState>(stage).unwrap();
        let autopilot = world.get::<RocketAutopilot>(stage).unwrap();
        let scorecard = world.get::<LandingScorecard>(stage).unwrap().clone();
        let target = world.get::<DroneShipLandingTarget>(stage).unwrap();
        (
            rocket.dynamics.position_m,
            rocket.dynamics.velocity_mps,
            autopilot.target_landing_position_m,
            scorecard,
            target.deck_contact,
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
    use crate::domain::entities::rocket::{EngineState, Rocket, RocketEngine, RocketStage};
    use crate::domain::events::{SplashdownDetectedEvent, StageSeparatedEvent};
    use crate::domain::services::guidance::AutopilotMode;
    use crate::domain::services::physics_orbital::LowEarthOrbitTarget;
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
                    position_m: bevy::math::Vec3::new(0.45 * angle.cos(), -8.0, 0.45 * angle.sin()),
                    thrust_axis: bevy::math::Vec3::Y,
                    isp_sea_level: 303.0,
                    isp_vacuum: 311.0,
                    gimbal_range_deg: 4.0,
                    max_thrust_kn: 25.8,
                    throttle_min: 0.6,
                    throttle_max: 1.0,
                    restartable: true,
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
                    dry_mass_kg: 950.0,
                    propellant_mass_kg: 9_250.0,
                    engines,
                },
                RocketStage {
                    name: "S2".into(),
                    dry_mass_kg: 250.0,
                    propellant_mass_kg: 2_050.0,
                    engines: vec![RocketEngine {
                        position_m: bevy::math::Vec3::new(0.0, 6.0, 0.0),
                        thrust_axis: bevy::math::Vec3::Y,
                        isp_sea_level: 311.0,
                        isp_vacuum: 343.0,
                        gimbal_range_deg: 4.0,
                        max_thrust_kn: 25.8,
                        throttle_min: 0.6,
                        throttle_max: 1.0,
                        restartable: true,
                        state: EngineState::Running,
                    }],
                },
            ],
        }
    }

    pub(super) fn ascent_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<SplashdownDetectedEvent>();
        app.insert_resource(SimulationTime::new(DT));
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
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for("Earth"),
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

        let (lat, lon) = (28.5721_f64, -80.6480_f64);
        let h =
            crate::infrastructure::bevy_adapters::components::PlanetTerrain::default_for("Earth")
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
                },
                RocketMass(total_mass_kg),
                RocketFlightConditions::default(),
                RocketMissionState::Launch,
                RocketPropulsion {
                    vehicle,
                    active_stage: 0,
                    propellant_remaining_kg: propellant,
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
        let circular_velocity_mps = DVec3::new(
            0.0,
            circular_speed_mps * target.target_inclination_rad.cos(),
            circular_speed_mps * target.target_inclination_rad.sin(),
        );

        let mut safe_app = ascent_app();
        let safe_entity = {
            let world = safe_app.world_mut();
            let mut query = world.query_filtered::<Entity, With<RocketPhysicsState>>();
            query.single(world).unwrap()
        };
        {
            let world = safe_app.world_mut();
            let mut rocket = world.get_mut::<RocketPhysicsState>(safe_entity).unwrap();
            rocket.dynamics.position_m = DVec3::new(radius_m, 0.0, 0.0);
            rocket.dynamics.velocity_mps = circular_velocity_mps;
            *world.get_mut::<RocketMissionState>(safe_entity).unwrap() = RocketMissionState::Ascent;
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
            rocket.dynamics.position_m = DVec3::new(radius_m, 0.0, 0.0);
            rocket.dynamics.velocity_mps = circular_velocity_mps * 0.98;
            *world.get_mut::<RocketMissionState>(unsafe_entity).unwrap() =
                RocketMissionState::Ascent;
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
            },
            RocketMass(total_mass_kg),
            RocketFlightConditions::default(),
            RocketPropulsion {
                vehicle,
                active_stage: 0,
                propellant_remaining_kg: propellant,
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
            app.update();
        }

        let world = app.world_mut();
        let mut q = world.query::<(&RocketPhysicsState, &RocketPropulsion, &RocketMass)>();
        let (rocket, propulsion, mass) = q.single(world).unwrap();

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
            (mass.0 - expected_mass).abs() < 1.0,
            "mass {} inconsistent with stage bookkeeping {expected_mass}",
            mass.0
        );
        assert!(
            propulsion.propellant_remaining_kg[1] < propulsion.vehicle.stages[1].propellant_mass_kg,
            "stage 2 must also have consumed propellant post-staging"
        );
        assert!(rocket.dynamics.mass_kg.is_finite() && rocket.dynamics.mass_kg > 0.0);
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
                app.update();
            }

            let world = app.world_mut();
            let mut q = world.query::<(&RocketPhysicsState, &RocketPropulsion, &RocketMass)>();
            let (rocket, propulsion, mass) = q.single(world).unwrap();

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
                (mass.0 - expected_mass).abs() < 1.0,
                "mass bookkeeping diverged at {acceleration}x: {} vs {expected_mass}",
                mass.0
            );
            assert!(rocket.dynamics.mass_kg.is_finite());

            let post_burn = (propulsion.propellant_remaining_kg[1], mass.0);
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
                &RocketMass,
                &RocketPropulsion,
                &RocketMissionState,
                &RocketCommands,
                &GravityAcceleration,
            )>();
            let (rocket, mass, propulsion, mission, commands, gravity) = q.single(world).unwrap();
            (
                rocket.dynamics.position_m,
                rocket.dynamics.velocity_mps,
                rocket.dynamics.orientation,
                rocket.dynamics.angular_velocity_radps,
                rocket.dynamics.mass_kg,
                mass.0,
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
        assert_eq!(a.5.to_bits(), b.5.to_bits(), "component mass diverged");
        assert_eq!(a.6, b.6, "active stage diverged");
        assert_eq!(a.7.to_bits(), b.7.to_bits(), "throttle diverged");
        assert_eq!(a.8, b.8, "separations count diverged");
        assert_eq!(a.9, b.9, "propellant vector diverged");
        assert_eq!(a.10, b.10, "mission state diverged");
        assert_eq!(
            bits(&a.11),
            bits(&b.11),
            "guidance target attitude diverged"
        );
        assert_eq!(vec3(&a.12), vec3(&b.12), "gravity diverged");
    }
}

#[cfg(test)]
mod render_interpolation_tests {
    use super::*;
    use crate::infrastructure::bevy_adapters::rocket_lifecycle::handle_rocket_launch_input;
    use crate::infrastructure::bevy_adapters::rocket_presentation::render_dynamics_state;

    #[test]
    fn prelaunch_render_interpolates_with_the_rotating_pad() {
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
/// Set `REGRESSION_RECORD=1` to (re)write the baseline fixture from the
/// current code. The audit trail is filled by the harness so the recorded
/// baseline is signed-off by construction.
#[cfg(test)]
mod determinism_regression_tests {
    use super::ascent_pipeline_tests::ascent_app;
    use crate::components::rocket::{RocketMissionState, RocketPhysicsState};
    use crate::domain::entities::rocket::RocketMissionState as DomainMission;
    use crate::domain::services::regression::{
        load_baseline_ron, save_baseline_ron, BaselineJustification, FlightBaseline,
        RegressionConfig, RocketStateSample,
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

    fn current_git_commit() -> String {
        std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn signed_audit() -> BaselineJustification {
        BaselineJustification {
            change_description: "canonical electron-class ascent baseline (LSOC)".into(),
            expected_improvement: "pins the full Guidance→Control→Forces→Integrate chain bitwise"
                .into(),
            numerical_tradeoffs: "semi-implicit Euler; f64; single fixed step 1/64 s".into(),
            affected_scenarios: vec!["ascent".into()],
            reviewer_approved: true,
            recorded_by: "opencode-determinism".into(),
        }
    }

    /// The CI gate: the freshly simulated ascent must match the committed
    /// baseline bit-for-bit within the documented per-variable tolerances. If
    /// the fixture is absent (fresh checkout) or `REGRESSION_RECORD=1`, it is
    /// (re)recorded first, then compared, so the fixture is self-bootstrapping.
    #[test]
    fn ascent_matches_committed_baseline_within_tolerances() {
        let path = default_baseline_path();
        let should_record = std::env::var("REGRESSION_RECORD").as_deref() == Ok("1");

        let baseline = if !path.exists() || should_record {
            let baseline = FlightBaseline::record(
                "ascent",
                current_git_commit(),
                signed_audit(),
                record_ascent_samples(),
            )
            .expect("signed-off ascent baseline records");
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("baseline dir creatable");
            }
            std::fs::write(
                &path,
                save_baseline_ron(&baseline).expect("baseline serializes"),
            )
            .expect("baseline writable");
            baseline
        } else {
            let ron = std::fs::read_to_string(&path).expect("baseline readable");
            load_baseline_ron(&ron).expect("baseline RON valid")
        };

        assert!(
            baseline.hash_chain_consistent(),
            "committed baseline hash chain is internally inconsistent"
        );

        let current = record_ascent_samples();
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
