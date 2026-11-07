//! Rocket-only procedural engine plume presentation.
//!
//! Effects are children of the interpolated vehicle entity, so render-origin
//! rebasing affects them through the existing presentation hierarchy. They read
//! authoritative flight state but only mutate their own presentation entities.

use super::components::{
    AppliedPropulsionState, RocketFlightConditions, RocketGeometry, RocketPresentationQuality,
    RocketPresentationSmoothing, RocketPropulsion, ThermalState,
};
use super::presentation_parameters::{map_presentation_parameters, RocketPresentationInputs};
use crate::domain::entities::rocket::{EngineState, RocketEngine};
use crate::domain::services::rocket_propulsion::{
    clamp_gimbal, gimbaled_thrust_direction_body, stage_thrust_body,
};
use bevy::asset::RenderAssetUsages;
use bevy::math::Vec3;
use bevy::prelude::*;
use bevy_mesh::{Indices, Mesh, PrimitiveTopology};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

const MAX_ENGINE_EFFECT_STATIONS: usize = 32;
const IGNITION_SMOOTHING_PER_SECOND: f32 = 7.0;

/// Capture-only switch that logs each vehicle's plume driving state every
/// 30 frames. The offscreen capture has no debug console, so this is how the
/// numbers behind a frame are recorded. Unset, nothing is logged.
const CAPTURE_EFFECTS_ENV: &str = "COSMIC_SYSTEMS_CAPTURE_EFFECTS";

fn capture_effects_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var(CAPTURE_EFFECTS_ENV).is_ok_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "on" | "true" | "yes"
            )
        })
    })
}

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
    /// Gimbaled thrust direction in the vehicle body frame. The plume exhaust
    /// points the opposite way; a gimballed engine must tilt its plume.
    thrust_axis_body: Vec3,
    rated_thrust_kn: f32,
    running: bool,
}

#[derive(Debug, Clone, Copy)]
struct DesiredEngineEffectStation {
    key: EngineEffectStationKey,
    station_m: Vec3,
    thrust_axis_body: Vec3,
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
        self.core_mesh = Some(meshes.add(plume_cone_mesh(12, 1)));
        self.inner_mesh = Some(meshes.add(plume_cone_mesh(16, 4)));
        self.outer_mesh = Some(meshes.add(plume_cone_mesh(16, 5)));
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

/// Unit plume cone: apex at `+Y` (the nozzle), base at `-Y` (the far end),
/// radius growing from zero at the apex to one at the base. Per-vertex colour
/// fades from opaque white at the nozzle to fully transparent at the far end,
/// so the additive plume reads as a soft exhaust rather than a hard cone.
/// Normals are irrelevant because the plume material is unlit.
fn plume_cone_mesh(radial_segments: usize, axial_segments: usize) -> Mesh {
    let radial_segments = radial_segments.max(3);
    let axial_segments = axial_segments.max(1);
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();

    for ring in 0..=axial_segments {
        let t = ring as f32 / axial_segments as f32;
        let y = 0.5 - t;
        let radius = t;
        let fade = 1.0 - t;
        for segment in 0..=radial_segments {
            let angle = segment as f32 / radial_segments as f32 * std::f32::consts::TAU;
            positions.push([radius * angle.cos(), y, radius * angle.sin()]);
            normals.push([0.0, 1.0, 0.0]);
            colors.push([fade, fade, fade, fade]);
        }
    }

    let ring_stride = (radial_segments + 1) as u32;
    for ring in 0..axial_segments as u32 {
        for segment in 0..radial_segments as u32 {
            let a = ring * ring_stride + segment;
            let b = a + 1;
            let c = a + ring_stride;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
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
    cameras: Query<(&Camera, &Transform), With<Camera3d>>,
    mut rockets: Query<
        (
            Entity,
            &RocketPropulsion,
            &RocketGeometry,
            &RocketFlightConditions,
            &ThermalState,
            &Transform,
            Option<&AppliedPropulsionState>,
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
    mut capture_frame: Local<u32>,
) {
    let update_started = Instant::now();
    *capture_frame = capture_frame.wrapping_add(1);
    let capture_tick = *capture_frame;
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

    for (
        rocket_entity,
        propulsion,
        geometry,
        conditions,
        thermal,
        rocket_transform,
        applied,
        smoothing,
    ) in rockets.iter_mut()
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

        if quality.level.effects_enabled() {
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
                                thrust_axis_body: station.thrust_axis_body,
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
        }

        let engine_running = desired.iter().any(|station| station.running);
        let mass_flow_fraction_unit = applied_mass_flow_fraction(
            propulsion,
            conditions.ambient_pressure_pa,
            applied,
            engine_running,
        );
        let target = map_presentation_parameters(RocketPresentationInputs {
            mass_flow_fraction_unit,
            // Smoothing below is the render-only ignition transition.
            ignition_elapsed_s: 1.0,
            ignition_ramp_duration_s: 1.0,
            ambient_pressure_pa: conditions.ambient_pressure_pa,
            density_kg_m3: conditions.density_kg_m3,
            terrain_distance_m: conditions.altitude_m,
            mach_number: conditions.mach_number,
            dynamic_pressure_pa: conditions.dynamic_pressure_pa,
            total_heat_flux_w_m2: thermal.total_heat_flux_w_m2,
            observer_distance_m: f64::from(super::camera::nearest_active_camera_distance_m(
                rocket_transform.translation,
                &cameras,
            )),
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
        let distance_m =
            super::camera::nearest_active_camera_distance_m(rocket_transform.translation, &cameras);

        if capture_effects_enabled() && intensity > 0.01 && capture_tick.is_multiple_of(30) {
            bevy::log::info!(
                "Plume capture frame {} ({}): stage {}, throttle {:.3}, mass flow {:.1}/{:.1} kg/s, \
                 altitude {:.0} m, pressure {:.0} Pa, fraction {:.3}, expansion {:.2}, shock {:.2}, \
                 camera {:.0} m / {:.0} m, intensity {:.3}, outer length {:.1} m, heating {:.3}",
                capture_tick,
                propulsion.vehicle.name,
                propulsion.active_stage,
                propulsion.throttle,
                applied.map_or(f64::NAN, |value| value.mass_flow_kg_s),
                rated_mass_flow_at_ambient_kg_s(propulsion, conditions.ambient_pressure_pa),
                conditions.altitude_m,
                conditions.ambient_pressure_pa,
                mass_flow_fraction_unit,
                target.plume_expansion_ratio,
                target.shock_intensity_unit,
                distance_m,
                quality.max_effect_distance_m,
                intensity,
                existing
                    .iter()
                    .filter(|(_, effect)| {
                        effect.owner == rocket_entity && effect.layer == EngineEffectLayer::Outer
                    })
                    .map(|(_, effect)| {
                        plume_local_transform(*effect, intensity, target, EngineEffectLayer::Outer)
                            .scale
                            .y
                    })
                    .next()
                    .unwrap_or(0.0),
                target.heating_intensity_unit,
            );
        }

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
                effect.thrust_axis_body = station.thrust_axis_body;
                effect.rated_thrust_kn = station.rated_thrust_kn;
                effect.running = station.running;
            }
            let visible = quality.level.effects_enabled()
                && effect.running
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
            thrust_axis_body: gimbaled_plume_axis_body(
                engine,
                propulsion.gimbal_pitch_rad,
                propulsion.gimbal_yaw_rad,
            ),
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
                    thrust_axis_body: gimbaled_plume_axis_body(
                        engine,
                        propulsion.gimbal_pitch_rad,
                        propulsion.gimbal_yaw_rad,
                    ),
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

/// Gimbaled thrust direction in the body frame, using the same clamp and
/// direction function the force path uses. Falls back to the nominal `+Y` axis
/// if a catalog entry declares a degenerate thrust axis.
fn gimbaled_plume_axis_body(
    engine: &RocketEngine,
    gimbal_pitch_rad: f32,
    gimbal_yaw_rad: f32,
) -> Vec3 {
    let direction = gimbaled_thrust_direction_body(
        engine.thrust_axis.as_dvec3(),
        f64::from(clamp_gimbal(gimbal_pitch_rad, engine.gimbal_range_deg)),
        f64::from(clamp_gimbal(gimbal_yaw_rad, engine.gimbal_range_deg)),
    )
    .as_vec3();
    if direction.length_squared() > 1e-12 {
        direction.normalize()
    } else {
        Vec3::Y
    }
}

/// Fraction of full-throttle mass flow the current engine set burns.
///
/// The numerator is the propellant flow the fixed pipeline actually consumed,
/// so the plume follows a propellant-limited final tick instead of a binary
/// "engine running" flag. It deliberately ignores the entry-flow force
/// multiplier: combustion still feeds the plume when external flow reduces the
/// net axial force. Without the diagnostic component (minimal test fixtures)
/// the commanded throttle is the best available projection.
fn applied_mass_flow_fraction(
    propulsion: &RocketPropulsion,
    ambient_pressure_pa: f64,
    applied: Option<&AppliedPropulsionState>,
    engine_running: bool,
) -> f64 {
    if !engine_running {
        return 0.0;
    }
    let Some(applied) = applied else {
        return f64::from(propulsion.throttle.clamp(0.0, 1.0));
    };
    let rated_mass_flow_kg_s = rated_mass_flow_at_ambient_kg_s(propulsion, ambient_pressure_pa);
    if !rated_mass_flow_kg_s.is_finite() || rated_mass_flow_kg_s <= 0.0 {
        return 0.0;
    }
    (applied.mass_flow_kg_s / rated_mass_flow_kg_s).clamp(0.0, 1.0)
}

/// Full-throttle mass flow of every engine currently running or ignitable. This
/// is a normalizer only: it reuses the authoritative engine operating point and
/// never produces a force.
fn rated_mass_flow_at_ambient_kg_s(propulsion: &RocketPropulsion, ambient_pressure_pa: f64) -> f64 {
    let mut rated_mass_flow_kg_s = 0.0;
    if let Some(stage) = propulsion.active_stage_configuration() {
        rated_mass_flow_kg_s += stage_thrust_body(&stage.engines, 1.0, ambient_pressure_pa).1;
    }
    if let Some((boosters, _)) = propulsion.attached_boosters() {
        for index in 0..boosters.count() {
            if propulsion.booster_is_ignitable(index) {
                rated_mass_flow_kg_s +=
                    stage_thrust_body(&boosters.stage.engines, 1.0, ambient_pressure_pa).1;
            }
        }
    }
    rated_mass_flow_kg_s
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
    // An under-expanded plume both widens and lengthens as ambient pressure
    // falls. The core stays close to the nozzle; the outer envelope carries the
    // visible altitude response. Real plume shape is a gas-dynamic solution;
    // this is a bounded real-time approximation.
    let axial_expansion = match layer {
        EngineEffectLayer::Core => 1.0,
        EngineEffectLayer::Inner => 1.0 + (expansion - 1.0) * 0.6,
        EngineEffectLayer::Outer => expansion,
    };
    let thermal_length = 1.0 + parameters.heating_intensity_unit as f32 * 0.15;
    let length_m = length_m * intensity_unit.max(0.05) * thermal_length * axial_expansion;

    // The cone is authored with its apex at local `+Y`; exhaust leaves along
    // the gimbaled thrust axis, so the plume body points the opposite way.
    let thrust_axis = effect.thrust_axis_body.normalize_or_zero();
    let exhaust = if thrust_axis == Vec3::ZERO {
        Vec3::NEG_Y
    } else {
        -thrust_axis
    };
    let rotation = Quat::from_rotation_arc(Vec3::NEG_Y, exhaust);

    Transform {
        translation: effect.station_m + exhaust * (length_m * 0.5),
        rotation,
        scale: Vec3::new(
            radius_m * thrust_scale * expansion,
            length_m,
            radius_m * thrust_scale * expansion,
        ),
    }
}

fn layer_is_within_quality_budget(
    layer: EngineEffectLayer,
    distance_m: f32,
    quality: RocketPresentationQuality,
) -> bool {
    // Reduced quality keeps only the core plume layer; the inner and outer
    // layers are secondary detail that never affects flight readability.
    if !quality.level.full_detail() && !matches!(layer, EngineEffectLayer::Core) {
        return false;
    }
    let max_distance_m = match layer {
        EngineEffectLayer::Core => quality.max_effect_distance_m,
        EngineEffectLayer::Inner => quality.max_effect_distance_m * 0.5,
        EngineEffectLayer::Outer => quality.max_effect_distance_m * 0.25,
    };
    distance_m <= max_distance_m
}

#[cfg(test)]
mod tests {
    use super::super::presentation_parameters::RocketPresentationParameters;
    use super::*;
    use crate::domain::entities::rocket::{ParallelBoosters, Rocket};
    use crate::domain::services::simulation_time::SimulationTime;
    use crate::infrastructure::bevy_adapters::rocket::atmospheric_effects::{
        update_rocket_atmospheric_effects, RocketAtmosphericEffectAssets,
    };
    use bevy_mesh::VertexAttributeValues;

    #[test]
    fn plume_cone_fades_from_nozzle_to_tail() {
        let mesh = plume_cone_mesh(8, 3);
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("plume mesh must carry vertex colours");
        };
        let apex_alpha = colors.first().expect("apex vertex")[3];
        let tail_alpha = colors.last().expect("tail vertex")[3];
        assert!(
            (apex_alpha - 1.0).abs() < 1e-6,
            "nozzle must be opaque: {apex_alpha}"
        );
        assert!(tail_alpha < 1e-6, "tail must fade out: {tail_alpha}");
    }

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
            thrust_axis_body: Vec3::Y,
            rated_thrust_kn: 1_000.0,
            running: true,
        };
        let parameters = map_presentation_parameters(RocketPresentationInputs {
            mass_flow_fraction_unit: 1.0,
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

    fn plume_effect(axis_body: Vec3, layer: EngineEffectLayer) -> RocketEngineEffect {
        RocketEngineEffect {
            owner: Entity::PLACEHOLDER,
            key: EngineEffectStationKey::Core {
                stage_index: 0,
                engine_index: 0,
            },
            layer,
            station_m: Vec3::new(0.0, -20.0, 0.0),
            thrust_axis_body: axis_body,
            rated_thrust_kn: 1_000.0,
            running: true,
        }
    }

    fn plume_parameters(ambient_pressure_pa: f64) -> RocketPresentationParameters {
        map_presentation_parameters(RocketPresentationInputs {
            mass_flow_fraction_unit: 1.0,
            ignition_elapsed_s: 1.0,
            ignition_ramp_duration_s: 1.0,
            ambient_pressure_pa,
            density_kg_m3: 0.0,
            terrain_distance_m: 1_000.0,
            mach_number: 0.0,
            dynamic_pressure_pa: 0.0,
            total_heat_flux_w_m2: 0.0,
            observer_distance_m: 0.0,
        })
    }

    /// The gimbal is part of the authoritative thrust direction. A gimballed
    /// engine must tilt its plume, otherwise the effect contradicts the force
    /// the simulation is applying.
    #[test]
    fn plume_follows_the_gimbaled_thrust_axis() {
        let angle = 0.12_f32;
        let axis = Vec3::new(angle.sin(), angle.cos(), 0.0).normalize();
        let effect = plume_effect(axis, EngineEffectLayer::Inner);
        let parameters = plume_parameters(101_325.0);

        let transform = plume_local_transform(effect, 1.0, parameters, EngineEffectLayer::Inner);

        let exhaust = transform.rotation * Vec3::NEG_Y;
        assert!(
            exhaust.dot(-axis) > 0.9999,
            "plume must leave along the gimbaled thrust axis: {exhaust:?} vs {axis:?}"
        );
        let expected_center = effect.station_m + (-axis) * (transform.scale.y * 0.5);
        assert!(
            (transform.translation - expected_center).length() < 1e-4,
            "plume must stay anchored at the engine station"
        );
    }

    /// An under-expanded plume lengthens as ambient pressure falls, not only
    /// widens. The core stays pinned to the nozzle so the effect still reads as
    /// an engine rather than a long beam.
    #[test]
    fn outer_plume_lengthens_as_ambient_pressure_falls() {
        let sea_level = plume_parameters(101_325.0);
        let vacuum = plume_parameters(0.0);
        let effect = plume_effect(Vec3::Y, EngineEffectLayer::Outer);

        let sea = plume_local_transform(effect, 1.0, sea_level, EngineEffectLayer::Outer);
        let vac = plume_local_transform(effect, 1.0, vacuum, EngineEffectLayer::Outer);

        assert!(sea.scale.y > 0.0);
        assert!(
            vac.scale.y > sea.scale.y * 2.0,
            "vacuum plume must lengthen: {} vs {}",
            vac.scale.y,
            sea.scale.y
        );
        assert!(vac.scale.x > sea.scale.x, "vacuum plume must also widen");

        let sea_core = plume_local_transform(effect, 1.0, sea_level, EngineEffectLayer::Core);
        let vac_core = plume_local_transform(effect, 1.0, vacuum, EngineEffectLayer::Core);
        assert_eq!(
            sea_core.scale.y, vac_core.scale.y,
            "the core plume stays anchored near the nozzle"
        );
    }

    /// Plume intensity follows the propellant the fixed pipeline actually
    /// burned, not a binary running flag: a propellant-limited final tick fades
    /// out continuously. The entry-flow force multiplier is excluded because
    /// combustion still feeds the plume.
    #[test]
    fn plume_intensity_follows_applied_mass_flow() {
        let mut propulsion =
            RocketPropulsion::for_fresh_flight(Rocket::falcon9_test_fixture(), 0.0, 0.0);
        for engine in &mut propulsion.vehicle.stages[0].engines {
            engine.state = EngineState::Running;
        }
        let ambient_pressure_pa = 101_325.0;
        let rated_kg_s = rated_mass_flow_at_ambient_kg_s(&propulsion, ambient_pressure_pa);
        assert!(
            rated_kg_s > 0.0,
            "running engines must have a rated mass flow"
        );

        let partial = AppliedPropulsionState {
            thrust_n: 0.0,
            mass_flow_kg_s: rated_kg_s * 0.4,
            active_engine_count: 9,
        };
        let fraction =
            applied_mass_flow_fraction(&propulsion, ambient_pressure_pa, Some(&partial), true);
        assert!((fraction - 0.4).abs() < 1e-9);

        // A reduced entry-flow force multiplier must not dim a burning plume.
        let retro_reduced = AppliedPropulsionState {
            thrust_n: 1.0,
            mass_flow_kg_s: rated_kg_s,
            active_engine_count: 9,
        };
        assert_eq!(
            applied_mass_flow_fraction(
                &propulsion,
                ambient_pressure_pa,
                Some(&retro_reduced),
                true
            ),
            1.0
        );

        assert_eq!(
            applied_mass_flow_fraction(&propulsion, ambient_pressure_pa, Some(&partial), false),
            0.0,
            "a stopped engine set must extinguish the plume even with a stale sample"
        );

        let over_rated = AppliedPropulsionState {
            thrust_n: 0.0,
            mass_flow_kg_s: rated_kg_s * 2.0,
            active_engine_count: 9,
        };
        assert_eq!(
            applied_mass_flow_fraction(&propulsion, ambient_pressure_pa, Some(&over_rated), true),
            1.0
        );
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

    #[test]
    fn presentation_updates_leave_authoritative_state_untouched() {
        let mut app = App::new();
        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .init_resource::<Time>()
            .init_resource::<RocketPresentationQuality>()
            .init_resource::<RocketPresentationMetrics>()
            .init_resource::<RocketEngineEffectAssets>()
            .init_resource::<RocketAtmosphericEffectAssets>()
            .insert_resource(SimulationTime::new(0.02))
            .add_systems(
                Update,
                (
                    update_rocket_engine_effects,
                    update_rocket_atmospheric_effects,
                    capture_rocket_presentation_metrics,
                )
                    .chain(),
            );
        app.world_mut()
            .spawn((Camera3d::default(), Transform::default()));
        let rocket = app
            .world_mut()
            .spawn((
                RocketPropulsion::for_fresh_flight(Rocket::falcon9_test_fixture(), 0.0, 0.0),
                RocketGeometry {
                    radius_m: 1.8,
                    height_m: 40.0,
                    lower_extent_y_m: -20.0,
                },
                RocketFlightConditions::default(),
                ThermalState::default(),
                Transform::default(),
            ))
            .id();

        let (throttle_before, stage_before, propellant_before) = {
            let propulsion = app
                .world()
                .entity(rocket)
                .get::<RocketPropulsion>()
                .expect("rocket has propulsion");
            (
                propulsion.throttle,
                propulsion.active_stage,
                propulsion.propellant_remaining_kg.clone(),
            )
        };
        let (sim_before, real_before) = {
            let time = app.world().resource::<SimulationTime>();
            (time.sim_time_s, time.real_time_s)
        };

        for _ in 0..3 {
            app.update();
        }

        let propulsion = app
            .world()
            .entity(rocket)
            .get::<RocketPropulsion>()
            .expect("rocket still has propulsion");
        assert_eq!(propulsion.throttle, throttle_before);
        assert_eq!(propulsion.active_stage, stage_before);
        assert_eq!(propulsion.propellant_remaining_kg, propellant_before);
        let time = app.world().resource::<SimulationTime>();
        assert_eq!(time.sim_time_s, sim_before);
        assert_eq!(time.real_time_s, real_before);
    }
}
