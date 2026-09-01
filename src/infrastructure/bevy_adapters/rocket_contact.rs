//! Terrain-contact preparation and constraint adapters.

use crate::components::rocket::{
    DroneShipLandingTarget, GroundRest, LandingLegs, LandingScorecard, RecoveringStage,
    RocketAutopilot, RocketFlightConditions, RocketGeometry, RocketMissionState,
    RocketPhysicsState, RocketPlanetBinding, RocketPropulsion, TipOverState,
};
use crate::domain::events::SplashdownDetectedEvent;
use crate::domain::services::landing_gear::{topple_critical_angle_rad, ToppleFall};
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
    sample_surface, GroundContact, SurfaceSample, TouchdownCriteria,
};
use crate::domain::services::terrain_source::TerrainSource;
use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
use crate::infrastructure::bevy_adapters::components::{
    PlanetComponent, PlanetTerrain, TerrainCollisionState,
};
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use bevy::ecs::query::QueryData;
use bevy::log::info;
use bevy::math::{DMat3, DQuat, DVec3};
use bevy::prelude::{Entity, MessageWriter, Query, Res, Resource};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

const TERRAIN_SURFACE_SAMPLE_CACHE_CAPACITY: usize = 512;

/// Thread-safe exact-sample cache for repeated fixed-step contact probes.
///
/// The key uses the input f64 bit patterns rather than spatial quantization, so
/// a cached result is bit-identical to direct `TerrainSource` evaluation. It is
/// presentation-independent and does not alter the source or collision model.
#[derive(Clone, Resource)]
pub struct TerrainSurfaceSampleCache {
    entries: Arc<Mutex<TerrainSurfaceSampleLru>>,
}

impl Default for TerrainSurfaceSampleCache {
    fn default() -> Self {
        Self {
            entries: Arc::new(Mutex::new(TerrainSurfaceSampleLru::new(
                TERRAIN_SURFACE_SAMPLE_CACHE_CAPACITY,
            ))),
        }
    }
}

impl TerrainSurfaceSampleCache {
    pub(crate) fn sample(
        &self,
        planet: Entity,
        source: &dyn TerrainSource,
        latitude_deg: f64,
        longitude_deg: f64,
        radius_m: f64,
    ) -> SurfaceSample {
        let key = TerrainSurfaceSampleKey::new(planet, latitude_deg, longitude_deg, radius_m);
        if let Some(sample) = self.lock().get(&key) {
            return sample;
        }

        // Do not hold the lock through the multi-octave terrain evaluation.
        let sample = sample_surface(source, latitude_deg, longitude_deg, radius_m);
        let mut entries = self.lock();
        if let Some(existing) = entries.get(&key) {
            return existing;
        }
        entries.insert(key, sample);
        sample
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, TerrainSurfaceSampleLru> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TerrainSurfaceSampleKey {
    planet: Entity,
    latitude_bits: u64,
    longitude_bits: u64,
    radius_bits: u64,
}

impl TerrainSurfaceSampleKey {
    fn new(planet: Entity, latitude_deg: f64, longitude_deg: f64, radius_m: f64) -> Self {
        Self {
            planet,
            latitude_bits: latitude_deg.to_bits(),
            longitude_bits: longitude_deg.to_bits(),
            radius_bits: radius_m.to_bits(),
        }
    }
}

struct TerrainSurfaceSampleLru {
    samples: HashMap<TerrainSurfaceSampleKey, SurfaceSample>,
    recency: VecDeque<TerrainSurfaceSampleKey>,
    capacity: usize,
}

impl TerrainSurfaceSampleLru {
    fn new(capacity: usize) -> Self {
        Self {
            samples: HashMap::with_capacity(capacity),
            recency: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn get(&mut self, key: &TerrainSurfaceSampleKey) -> Option<SurfaceSample> {
        let sample = self.samples.get(key).copied()?;
        self.touch(*key);
        Some(sample)
    }

    fn insert(&mut self, key: TerrainSurfaceSampleKey, sample: SurfaceSample) {
        match self.samples.entry(key) {
            Entry::Occupied(mut entry) => {
                entry.insert(sample);
                self.touch(key);
                return;
            }
            Entry::Vacant(_) => {}
        }
        if self.samples.len() == self.capacity {
            if let Some(oldest) = self.recency.pop_front() {
                self.samples.remove(&oldest);
            }
        }
        self.samples.insert(key, sample);
        self.recency.push_back(key);
    }

    fn touch(&mut self, key: TerrainSurfaceSampleKey) {
        if let Some(index) = self.recency.iter().position(|candidate| *candidate == key) {
            self.recency.remove(index);
        }
        self.recency.push_back(key);
    }
}

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
pub fn resolve_ground_contact(
    sim_time: Res<SimulationTime>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    surface_cache: Option<Res<TerrainSurfaceSampleCache>>,
    mut splashdown_writer: MessageWriter<SplashdownDetectedEvent>,
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
                .vehicle
                .stages
                .get(propulsion.active_stage)
                .map(|stage| {
                    let thrust_body =
                        stage_thrust_body(&stage.engines, propulsion.throttle, ambient_pressure_pa)
                            .0;
                    (rocket.dynamics.orientation * thrust_body)
                        .dot(normal)
                        .max(0.0)
                })
                .unwrap_or(0.0);
            if liftoff_from_rest(upward_thrust_n, weight_n) {
                rest.active = false;
                collision.ground_contact = GroundContact::None;
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
                    }
                }
            }
            GroundContact::Crash => {
                if *mission_state != RocketMissionState::PreLaunch {
                    *mission_state = RocketMissionState::Crashed;
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
    use super::*;
    use crate::components::rocket::RocketRenderState;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::terrain_collision::decompose_velocity;
    use crate::domain::services::terrain_source::ProceduralTerrainSource;
    use crate::infrastructure::bevy_adapters::rocket_presentation::render_dynamics_state;
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
        let _visual = render_dynamics_state(render, 0.5);
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
