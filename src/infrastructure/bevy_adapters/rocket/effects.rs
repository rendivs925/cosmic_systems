//! Rocket-only procedural engine plume presentation.
//!
//! Effects are children of the interpolated vehicle entity, so render-origin
//! rebasing affects them through the existing presentation hierarchy. They read
//! authoritative flight state but only mutate their own presentation entities.

use super::components::{
    RocketFlightConditions, RocketGeometry, RocketPresentationQuality, RocketPresentationSmoothing,
    RocketPropulsion, ThermalState,
};
use super::presentation_parameters::{map_presentation_parameters, RocketPresentationInputs};
use crate::domain::entities::rocket::EngineState;
use bevy::math::Vec3;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

const MAX_ENGINE_EFFECT_STATIONS: usize = 32;
const IGNITION_SMOOTHING_PER_SECOND: f32 = 7.0;

/// Rocket-mode-only snapshot consumed by shared cadence-limited performance reporting.
/// It tracks presentation entities and Update work only, never flight state.
#[derive(Resource, Debug, Default)]
pub(crate) struct RocketPresentationMetrics {
    pub(crate) effect_count: usize,
    pub(crate) visible_effect_count: usize,
    pub(crate) engine_effect_update_ms: f64,
    pub(crate) ground_effect_update_ms: f64,
}

impl RocketPresentationMetrics {
    pub(crate) fn total_update_ms(&self) -> f64 {
        self.engine_effect_update_ms + self.ground_effect_update_ms
    }

    pub(crate) fn record_engine_update(&mut self, elapsed: std::time::Duration) {
        self.engine_effect_update_ms = elapsed.as_secs_f64() * 1_000.0;
    }

    pub(crate) fn record_ground_update(&mut self, elapsed: std::time::Duration) {
        self.ground_effect_update_ms = elapsed.as_secs_f64() * 1_000.0;
    }
}

/// Stable catalog identity for an engine effect station owned by a vehicle.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EngineEffectStationKey {
    Core {
        stage_index: usize,
        engine_index: usize,
    },
    Booster {
        booster_index: usize,
        engine_index: usize,
    },
}

/// One visual layer of a bounded engine effect station.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EngineEffectLayer {
    Core,
    Inner,
    Outer,
}

/// Presentation ownership and local catalog station data for one plume layer.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct RocketEngineEffect {
    owner: Entity,
    key: EngineEffectStationKey,
    layer: EngineEffectLayer,
    station_m: Vec3,
    rated_thrust_kn: f32,
    running: bool,
}

#[derive(Debug, Clone, Copy)]
struct DesiredEngineEffectStation {
    key: EngineEffectStationKey,
    station_m: Vec3,
    rated_thrust_kn: f32,
    running: bool,
}

/// Reusable meshes and materials for every bounded engine-effect child.
#[derive(Resource, Default)]
pub(crate) struct RocketEngineEffectAssets {
    core_mesh: Option<Handle<Mesh>>,
    inner_mesh: Option<Handle<Mesh>>,
    outer_mesh: Option<Handle<Mesh>>,
    core_material: Option<Handle<StandardMaterial>>,
    inner_material: Option<Handle<StandardMaterial>>,
    outer_material: Option<Handle<StandardMaterial>>,
}

impl RocketEngineEffectAssets {
    fn initialize(&mut self, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
        if self.core_mesh.is_some() {
            return;
        }
        self.core_mesh = Some(meshes.add(Cylinder::new(1.0, 1.0)));
        self.inner_mesh = Some(meshes.add(Cone::new(1.0, 1.0)));
        self.outer_mesh = Some(meshes.add(Cone::new(1.0, 1.0)));
        self.core_material = Some(materials.add(emissive_plume_material(
            Color::srgb(0.55, 0.82, 1.0),
            10.0,
            0.95,
        )));
        self.inner_material = Some(materials.add(emissive_plume_material(
            Color::srgb(1.0, 0.42, 0.08),
            5.0,
            0.55,
        )));
        self.outer_material = Some(materials.add(emissive_plume_material(
            Color::srgb(0.72, 0.18, 0.03),
            1.5,
            0.18,
        )));
    }
}

fn emissive_plume_material(color: Color, emission: f32, alpha: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: color.with_alpha(alpha),
        emissive: color.to_linear() * emission,
        alpha_mode: AlphaMode::Add,
        unlit: true,
        cull_mode: None,
        ..default()
    }
}

/// Reconciles bounded catalog station children and updates their render-only state.
///
/// Staging, separation, and relaunch alter `RocketPropulsion`; reconciliation
/// observes that state on the next frame and removes obsolete children without
/// writing to any lifecycle or propulsion component.
#[expect(
    clippy::type_complexity,
    reason = "The presentation adapter reads the authoritative visual inputs and mutates only effect children."
)]
#[expect(
    clippy::too_many_arguments,
    reason = "One adapter needs the bounded assets, camera quality, authoritative inputs, and child presentation query."
)]
pub(crate) fn update_rocket_engine_effects(
    mut commands: Commands,
    time: Res<Time>,
    quality: Res<RocketPresentationQuality>,
    mut presentation_metrics: ResMut<RocketPresentationMetrics>,
    mut effect_assets: ResMut<RocketEngineEffectAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cameras: Query<&Transform, With<Camera3d>>,
    mut rockets: Query<
        (
            Entity,
            &RocketPropulsion,
            &RocketGeometry,
            &RocketFlightConditions,
            &ThermalState,
            &Transform,
            Option<&mut RocketPresentationSmoothing>,
        ),
        Without<RocketEngineEffect>,
    >,
    mut effects: Query<
        (
            Entity,
            &mut RocketEngineEffect,
            &mut Transform,
            &mut Visibility,
        ),
        Without<Camera3d>,
    >,
) {
    let update_started = Instant::now();
    effect_assets.initialize(&mut meshes, &mut materials);
    let existing = effects
        .iter()
        .map(|(entity, effect, _, _)| (entity, *effect))
        .collect::<Vec<_>>();
    let rocket_entities = rockets
        .iter()
        .map(|(entity, ..)| entity)
        .collect::<HashSet<_>>();

    // Self-heal effects whose owner was removed or no longer has propulsion.
    for (entity, effect) in &existing {
        if !rocket_entities.contains(&effect.owner) {
            commands.entity(*entity).despawn();
        }
    }

    for (rocket_entity, propulsion, geometry, conditions, thermal, rocket_transform, smoothing) in
        rockets.iter_mut()
    {
        if smoothing.is_none() {
            // This is presentation-local state. Inserting it here lets rockets
            // spawned by existing authoritative systems gain a smooth ignition
            // ramp without expanding their spawn bundles.
            commands
                .entity(rocket_entity)
                .insert(RocketPresentationSmoothing::default());
        }
        let desired = desired_engine_effect_stations(propulsion, *geometry);
        let desired_keys = desired
            .iter()
            .flat_map(|station| {
                [
                    (station.key, EngineEffectLayer::Core),
                    (station.key, EngineEffectLayer::Inner),
                    (station.key, EngineEffectLayer::Outer),
                ]
            })
            .collect::<HashSet<_>>();
        let mut owned = HashMap::new();
        for (entity, effect) in &existing {
            if effect.owner != rocket_entity {
                continue;
            }
            let key = (effect.key, effect.layer);
            if !desired_keys.contains(&key) || owned.insert(key, *entity).is_some() {
                commands.entity(*entity).despawn();
            }
        }

        for station in &desired {
            for layer in [
                EngineEffectLayer::Core,
                EngineEffectLayer::Inner,
                EngineEffectLayer::Outer,
            ] {
                if owned.contains_key(&(station.key, layer)) {
                    continue;
                }
                let (mesh, material) = effect_asset_handles(&effect_assets, layer);
                commands.entity(rocket_entity).with_children(|parent| {
                    parent.spawn((
                        RocketEngineEffect {
                            owner: rocket_entity,
                            key: station.key,
                            layer,
                            station_m: station.station_m,
                            rated_thrust_kn: station.rated_thrust_kn,
                            running: station.running,
                        },
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        Transform::default(),
                        Visibility::Hidden,
                    ));
                });
            }
        }

        let engine_running = desired.iter().any(|station| station.running);
        let target = map_presentation_parameters(RocketPresentationInputs {
            throttle_unit: f64::from(propulsion.throttle),
            thrust_fraction_unit: f64::from(engine_running as u8),
            // Smoothing below is the render-only ignition transition.
            ignition_elapsed_s: 1.0,
            ignition_ramp_duration_s: 1.0,
            ambient_pressure_pa: conditions.ambient_pressure_pa,
            density_kg_m3: conditions.density_kg_m3,
            terrain_distance_m: conditions.altitude_m,
            mach_number: conditions.mach_number,
            dynamic_pressure_pa: conditions.dynamic_pressure_pa,
            total_heat_flux_w_m2: thermal.total_heat_flux_w_m2,
            observer_distance_m: f64::from(nearest_camera_distance_m(rocket_transform, &cameras)),
        });
        let intensity = smoothing.map_or(target.ignition_intensity_unit as f32, |mut smoothing| {
            let blend = 1.0 - (-IGNITION_SMOOTHING_PER_SECOND * time.delta_secs()).exp();
            smoothing.plume_intensity_unit +=
                (target.plume_intensity_unit as f32 - smoothing.plume_intensity_unit) * blend;
            smoothing.ignition_intensity_unit +=
                (target.ignition_intensity_unit as f32 - smoothing.ignition_intensity_unit) * blend;
            smoothing.shock_intensity_unit +=
                (target.shock_intensity_unit as f32 - smoothing.shock_intensity_unit) * blend;
            smoothing.heating_intensity_unit +=
                (target.heating_intensity_unit as f32 - smoothing.heating_intensity_unit) * blend;
            smoothing.ignition_intensity_unit
        });
        let distance_m = nearest_camera_distance_m(rocket_transform, &cameras);

        for (entity, effect) in &existing {
            if effect.owner != rocket_entity || !desired_keys.contains(&(effect.key, effect.layer))
            {
                continue;
            }
            let Ok((_, mut effect, mut transform, mut visibility)) = effects.get_mut(*entity)
            else {
                continue;
            };
            if let Some(station) = desired.iter().find(|station| station.key == effect.key) {
                effect.station_m = station.station_m;
                effect.rated_thrust_kn = station.rated_thrust_kn;
                effect.running = station.running;
            }
            let visible = effect.running
                && intensity > 0.002
                && distance_m <= quality.max_effect_distance_m
                && layer_is_within_quality_budget(effect.layer, distance_m, *quality);
            *visibility = if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            if visible {
                *transform = plume_local_transform(*effect, intensity, target, effect.layer);
            }
        }
    }
    presentation_metrics.record_engine_update(update_started.elapsed());
}

/// Captures the current bounded Rocket-effect population after presentation updates.
pub(crate) fn capture_rocket_presentation_metrics(
    mut metrics: ResMut<RocketPresentationMetrics>,
    engine_effects: Query<&Visibility, With<RocketEngineEffect>>,
    ground_effects: Query<&Visibility, With<super::ground_presentation::RocketPadGroundEffect>>,
    pad_lights: Query<&Visibility, With<super::ground_presentation::RocketPadIllumination>>,
) {
    metrics.effect_count =
        engine_effects.iter().count() + ground_effects.iter().count() + pad_lights.iter().count();
    metrics.visible_effect_count = engine_effects
        .iter()
        .chain(ground_effects.iter())
        .chain(pad_lights.iter())
        .filter(|visibility| **visibility == Visibility::Visible)
        .count();
}

fn effect_asset_handles(
    assets: &RocketEngineEffectAssets,
    layer: EngineEffectLayer,
) -> (Handle<Mesh>, Handle<StandardMaterial>) {
    match layer {
        EngineEffectLayer::Core => (
            assets
                .core_mesh
                .clone()
                .expect("engine effect mesh initialized"),
            assets
                .core_material
                .clone()
                .expect("engine effect material initialized"),
        ),
        EngineEffectLayer::Inner => (
            assets
                .inner_mesh
                .clone()
                .expect("engine effect mesh initialized"),
            assets
                .inner_material
                .clone()
                .expect("engine effect material initialized"),
        ),
        EngineEffectLayer::Outer => (
            assets
                .outer_mesh
                .clone()
                .expect("engine effect mesh initialized"),
            assets
                .outer_material
                .clone()
                .expect("engine effect material initialized"),
        ),
    }
}

fn desired_engine_effect_stations(
    propulsion: &RocketPropulsion,
    geometry: RocketGeometry,
) -> Vec<DesiredEngineEffectStation> {
    let mut stations = Vec::new();
    let Some(stage) = propulsion.active_stage_configuration() else {
        return stations;
    };
    let origin_m = propulsion
        .active_stage_origin_in_stack_m(geometry.height_m)
        .unwrap_or_default()
        .as_vec3();
    let core_running = propulsion.running_core_stage().is_some();
    for (engine_index, engine) in stage.engines.iter().enumerate() {
        stations.push(DesiredEngineEffectStation {
            key: EngineEffectStationKey::Core {
                stage_index: propulsion.active_stage,
                engine_index,
            },
            station_m: origin_m + engine.position_m,
            rated_thrust_kn: engine.rated_thrust_kn,
            running: core_running && engine.state == EngineState::Running,
        });
    }
    if let Some((boosters, _)) = propulsion.attached_boosters() {
        for booster_index in 0..boosters.count() {
            let Some(attachment_m) = boosters.attachment_position_m(booster_index) else {
                continue;
            };
            for (engine_index, engine) in boosters.stage.engines.iter().enumerate() {
                stations.push(DesiredEngineEffectStation {
                    key: EngineEffectStationKey::Booster {
                        booster_index,
                        engine_index,
                    },
                    station_m: attachment_m + engine.position_m,
                    rated_thrust_kn: engine.rated_thrust_kn,
                    running: propulsion.booster_is_ignitable(booster_index)
                        && engine.state == EngineState::Running,
                });
            }
        }
    }
    stations.truncate(MAX_ENGINE_EFFECT_STATIONS);
    stations
}

fn plume_local_transform(
    effect: RocketEngineEffect,
    intensity_unit: f32,
    parameters: super::presentation_parameters::RocketPresentationParameters,
    layer: EngineEffectLayer,
) -> Transform {
    let thrust_scale = (effect.rated_thrust_kn.max(1.0) / 1_000.0)
        .sqrt()
        .clamp(0.35, 1.6);
    let (radius_m, length_m) = match layer {
        EngineEffectLayer::Core => (0.16, 1.5),
        EngineEffectLayer::Inner => (0.35, 4.0),
        EngineEffectLayer::Outer => (0.6, 6.0),
    };
    let expansion = match layer {
        EngineEffectLayer::Core => 1.0,
        EngineEffectLayer::Inner => parameters.plume_expansion_ratio as f32,
        EngineEffectLayer::Outer => {
            parameters.plume_expansion_ratio as f32
                * (1.0 + parameters.shock_intensity_unit as f32 * 0.2)
        }
    };
    let thermal_length = 1.0 + parameters.heating_intensity_unit as f32 * 0.15;
    let length_m = length_m * intensity_unit.max(0.05) * thermal_length;
    Transform {
        translation: effect.station_m - Vec3::Y * (length_m * 0.5),
        scale: Vec3::new(
            radius_m * thrust_scale * expansion,
            length_m,
            radius_m * thrust_scale * expansion,
        ),
        ..default()
    }
}

fn layer_is_within_quality_budget(
    layer: EngineEffectLayer,
    distance_m: f32,
    quality: RocketPresentationQuality,
) -> bool {
    let max_distance_m = match layer {
        EngineEffectLayer::Core => quality.max_effect_distance_m,
        EngineEffectLayer::Inner => quality.max_effect_distance_m * 0.5,
        EngineEffectLayer::Outer => quality.max_effect_distance_m * 0.25,
    };
    distance_m <= max_distance_m
}

fn nearest_camera_distance_m(
    rocket_transform: &Transform,
    cameras: &Query<&Transform, With<Camera3d>>,
) -> f32 {
    cameras
        .iter()
        .map(|camera| camera.translation.distance(rocket_transform.translation))
        .reduce(f32::min)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::rocket::{ParallelBoosters, Rocket};

    #[test]
    fn active_stage_stations_use_the_current_presentation_root() {
        let mut propulsion =
            RocketPropulsion::for_fresh_flight(Rocket::falcon9_test_fixture(), 0.0, 0.0);
        propulsion.active_stage = 1;
        let geometry = RocketGeometry {
            radius_m: 1.8,
            height_m: propulsion.vehicle.stages[1].height_m,
            lower_extent_y_m: -propulsion.vehicle.stages[1].height_m * 0.5,
        };

        let stations = desired_engine_effect_stations(&propulsion, geometry);

        assert_eq!(stations.len(), 1);
        assert_eq!(
            stations[0].station_m,
            propulsion.vehicle.stages[1].engines[0].position_m
        );
    }

    #[test]
    fn attached_booster_stations_are_owned_and_retired_with_the_attachment() {
        let mut vehicle = Rocket::falcon9_test_fixture();
        let booster_stage = vehicle.stages[0].clone();
        vehicle.parallel_boosters = Some(ParallelBoosters::new(
            booster_stage,
            vec![Vec3::new(4.0, 0.0, 0.0)],
        ));
        let mut propulsion = RocketPropulsion::for_fresh_flight(vehicle, 0.0, 0.0);
        let geometry = RocketGeometry {
            radius_m: 2.0,
            height_m: propulsion.vehicle.height_m,
            lower_extent_y_m: -35.0,
        };

        let attached = desired_engine_effect_stations(&propulsion, geometry);
        assert!(attached
            .iter()
            .any(|station| matches!(station.key, EngineEffectStationKey::Booster { .. })));
        propulsion.detach_boosters();
        let detached = desired_engine_effect_stations(&propulsion, geometry);
        assert!(detached
            .iter()
            .all(|station| !matches!(station.key, EngineEffectStationKey::Booster { .. })));
    }

    #[test]
    fn local_plume_transform_is_independent_of_render_origin() {
        let effect = RocketEngineEffect {
            owner: Entity::PLACEHOLDER,
            key: EngineEffectStationKey::Core {
                stage_index: 0,
                engine_index: 0,
            },
            layer: EngineEffectLayer::Core,
            station_m: Vec3::new(2.0, -10.0, 3.0),
            rated_thrust_kn: 1_000.0,
            running: true,
        };
        let parameters = map_presentation_parameters(RocketPresentationInputs {
            throttle_unit: 1.0,
            thrust_fraction_unit: 1.0,
            ignition_elapsed_s: 1.0,
            ignition_ramp_duration_s: 1.0,
            ambient_pressure_pa: 101_325.0,
            density_kg_m3: 1.225,
            terrain_distance_m: 1_000.0,
            mach_number: 0.0,
            dynamic_pressure_pa: 0.0,
            total_heat_flux_w_m2: 0.0,
            observer_distance_m: 0.0,
        });

        let transform = plume_local_transform(effect, 1.0, parameters, EngineEffectLayer::Core);

        assert_eq!(transform.translation.x, 2.0);
        assert_eq!(transform.translation.z, 3.0);
        assert!(transform.translation.y < effect.station_m.y);
    }

    #[test]
    fn reconciliation_removes_booster_children_after_attachment_changes() {
        let mut vehicle = Rocket::falcon9_test_fixture();
        let booster_stage = vehicle.stages[0].clone();
        vehicle.parallel_boosters = Some(ParallelBoosters::new(
            booster_stage,
            vec![Vec3::new(4.0, 0.0, 0.0)],
        ));
        let geometry = RocketGeometry {
            radius_m: 2.0,
            height_m: vehicle.height_m,
            lower_extent_y_m: -35.0,
        };
        let mut app = App::new();
        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .init_resource::<Time>()
            .init_resource::<RocketPresentationQuality>()
            .init_resource::<RocketPresentationMetrics>()
            .init_resource::<RocketEngineEffectAssets>()
            .add_systems(
                Update,
                (
                    update_rocket_engine_effects,
                    capture_rocket_presentation_metrics,
                )
                    .chain(),
            );
        app.world_mut()
            .spawn((Camera3d::default(), Transform::default()));
        let rocket = app
            .world_mut()
            .spawn((
                RocketPropulsion::for_fresh_flight(vehicle, 0.0, 0.0),
                geometry,
                RocketFlightConditions::default(),
                ThermalState::default(),
                Transform::default(),
            ))
            .id();

        // The first update queues the child spawns; the second applies them.
        app.update();
        app.update();
        assert!(app
            .world()
            .resource::<RocketPresentationMetrics>()
            .engine_effect_update_ms
            .is_finite());
        assert_eq!(
            app.world()
                .resource::<RocketPresentationMetrics>()
                .effect_count,
            54
        );
        let attached_count = app
            .world_mut()
            .query::<&RocketEngineEffect>()
            .iter(app.world())
            .count();
        assert_eq!(attached_count, 54);

        app.world_mut()
            .entity_mut(rocket)
            .get_mut::<RocketPropulsion>()
            .expect("test rocket has propulsion")
            .detach_boosters();
        app.update();
        let detached_count = app
            .world_mut()
            .query::<&RocketEngineEffect>()
            .iter(app.world())
            .count();
        assert_eq!(detached_count, 27);
    }
}
