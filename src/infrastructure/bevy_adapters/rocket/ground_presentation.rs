//! Bounded launch-pad dust, haze, and illumination presentation.
//!
//! These pad children use the terrain collision authority only as a read-only
//! distance input. They neither sample visual terrain nor affect flight state.

use super::components::{
    LaunchPadPresentation, RocketFlightConditions, RocketPresentationQuality, RocketPropulsion,
    TerrainCollisionState,
};
use super::effects::RocketPresentationMetrics;
use super::presentation_parameters::{map_presentation_parameters, RocketPresentationInputs};
use bevy::prelude::*;
use std::time::Instant;

const DUST_HAZE_MAX_ALTITUDE_M: f64 = 50.0;
const PAD_LIGHT_MAX_DISTANCE_M: f32 = 5_000.0;

#[derive(Component)]
pub(crate) struct RocketPadGroundEffect {
    layer: RocketPadGroundEffectLayer,
}

#[derive(Clone, Copy)]
pub(crate) enum RocketPadGroundEffectLayer {
    Dust,
    Haze,
}

#[derive(Component)]
pub(crate) struct RocketPadIllumination;

#[derive(Resource, Default)]
pub(crate) struct RocketGroundPresentationAssets {
    dust_mesh: Option<Handle<Mesh>>,
    haze_mesh: Option<Handle<Mesh>>,
    dust_material: Option<Handle<StandardMaterial>>,
    haze_material: Option<Handle<StandardMaterial>>,
}

impl RocketGroundPresentationAssets {
    fn initialize(&mut self, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
        if self.dust_mesh.is_some() {
            return;
        }
        self.dust_mesh = Some(meshes.add(Cylinder::new(1.0, 0.05)));
        self.haze_mesh = Some(meshes.add(Cylinder::new(1.0, 0.08)));
        self.dust_material =
            Some(materials.add(ground_effect_material(Color::srgb(0.42, 0.32, 0.2), 0.28)));
        self.haze_material =
            Some(materials.add(ground_effect_material(Color::srgb(0.72, 0.66, 0.52), 0.12)));
    }
}

fn ground_effect_material(color: Color, alpha: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: color.with_alpha(alpha),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 1.0,
        cull_mode: None,
        ..default()
    }
}

/// Updates the fixed set of pad-local ground-effect children from authoritative
/// propulsion and terrain-distance samples.
#[expect(
    clippy::too_many_arguments,
    reason = "The adapter reads shared presentation state and writes only bounded pad children."
)]
pub(crate) fn update_rocket_ground_presentation(
    mut commands: Commands,
    quality: Res<RocketPresentationQuality>,
    mut presentation_metrics: ResMut<RocketPresentationMetrics>,
    mut assets: ResMut<RocketGroundPresentationAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cameras: Query<&Transform, (With<Camera3d>, Without<RocketPadGroundEffect>)>,
    rockets: Query<(
        &RocketPropulsion,
        &RocketFlightConditions,
        &TerrainCollisionState,
    )>,
    pads: Query<(Entity, &Transform), With<LaunchPadPresentation>>,
    mut effects: Query<
        (&RocketPadGroundEffect, &mut Transform, &mut Visibility),
        (
            Without<LaunchPadPresentation>,
            Without<RocketPadIllumination>,
        ),
    >,
    mut lights: Query<(&mut PointLight, &mut Visibility), With<RocketPadIllumination>>,
) {
    let update_started = Instant::now();
    assets.initialize(&mut meshes, &mut materials);
    let Some((propulsion, conditions, terrain)) = rockets.iter().next() else {
        presentation_metrics.record_ground_update(update_started.elapsed());
        return;
    };
    let Some((pad, pad_transform)) = pads.iter().next() else {
        presentation_metrics.record_ground_update(update_started.elapsed());
        return;
    };
    if effects.is_empty() && lights.is_empty() {
        spawn_pad_children(&mut commands, pad, &assets);
        presentation_metrics.record_ground_update(update_started.elapsed());
        return;
    }

    let running = propulsion.running_core_stage().is_some()
        || propulsion.attached_boosters().is_some_and(|(boosters, _)| {
            (0..boosters.count()).any(|index| propulsion.booster_is_ignitable(index))
        });
    let parameters = map_presentation_parameters(RocketPresentationInputs {
        throttle_unit: f64::from(propulsion.throttle),
        thrust_fraction_unit: f64::from(running as u8),
        ignition_elapsed_s: 1.0,
        ignition_ramp_duration_s: 1.0,
        ambient_pressure_pa: conditions.ambient_pressure_pa,
        density_kg_m3: conditions.density_kg_m3,
        terrain_distance_m: terrain.radar_altitude_m,
        mach_number: conditions.mach_number,
        dynamic_pressure_pa: conditions.dynamic_pressure_pa,
        total_heat_flux_w_m2: 0.0,
        observer_distance_m: 0.0,
    });
    let camera_distance_m = cameras
        .iter()
        .map(|camera| camera.translation.distance(pad_transform.translation))
        .reduce(f32::min)
        .unwrap_or(0.0);
    let visible = ground_effect_visible(
        parameters.ground_effect_intensity_unit,
        terrain.radar_altitude_m,
        camera_distance_m,
        quality.max_effect_distance_m,
    );
    for (effect, mut transform, mut visibility) in &mut effects {
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if !visible {
            continue;
        }
        let intensity = parameters.ground_effect_intensity_unit as f32;
        let (radius_m, height_m) = match effect.layer {
            RocketPadGroundEffectLayer::Dust => (12.0 + intensity * 35.0, 0.04),
            RocketPadGroundEffectLayer::Haze => (24.0 + intensity * 56.0, 0.35 + intensity * 1.5),
        };
        transform.scale = Vec3::new(radius_m, height_m, radius_m);
    }
    for (mut light, mut visibility) in &mut lights {
        let light_visible = visible && camera_distance_m <= PAD_LIGHT_MAX_DISTANCE_M;
        *visibility = if light_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        light.intensity = 8_000.0 + parameters.ground_effect_intensity_unit as f32 * 80_000.0;
    }
    presentation_metrics.record_ground_update(update_started.elapsed());
}

fn ground_effect_visible(
    intensity_unit: f64,
    terrain_distance_m: f64,
    camera_distance_m: f32,
    max_effect_distance_m: f32,
) -> bool {
    intensity_unit.is_finite()
        && intensity_unit > 0.002
        && terrain_distance_m.is_finite()
        && (0.0..=DUST_HAZE_MAX_ALTITUDE_M).contains(&terrain_distance_m)
        && camera_distance_m.is_finite()
        && camera_distance_m <= max_effect_distance_m
}

fn spawn_pad_children(
    commands: &mut Commands,
    pad: Entity,
    assets: &RocketGroundPresentationAssets,
) {
    let dust_mesh = assets.dust_mesh.clone().expect("ground assets initialized");
    let haze_mesh = assets.haze_mesh.clone().expect("ground assets initialized");
    let dust_material = assets
        .dust_material
        .clone()
        .expect("ground assets initialized");
    let haze_material = assets
        .haze_material
        .clone()
        .expect("ground assets initialized");
    commands.entity(pad).with_children(|parent| {
        parent.spawn((
            RocketPadGroundEffect {
                layer: RocketPadGroundEffectLayer::Dust,
            },
            Mesh3d(dust_mesh),
            MeshMaterial3d(dust_material),
            Transform::from_xyz(0.0, 0.03, 0.0),
            Visibility::Hidden,
            Name::new("RocketPadLiftoffDust"),
        ));
        parent.spawn((
            RocketPadGroundEffect {
                layer: RocketPadGroundEffectLayer::Haze,
            },
            Mesh3d(haze_mesh),
            MeshMaterial3d(haze_material),
            Transform::from_xyz(0.0, 0.25, 0.0),
            Visibility::Hidden,
            Name::new("RocketPadLiftoffHaze"),
        ));
        parent.spawn((
            RocketPadIllumination,
            PointLight {
                color: Color::srgb(1.0, 0.38, 0.12),
                intensity: 8_000.0,
                range: 90.0,
                radius: 5.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(0.0, 3.0, 0.0),
            Visibility::Hidden,
            Name::new("RocketPadEngineGlow"),
        ));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_ground_effects_are_strictly_bounded_to_near_terrain_and_camera() {
        assert!(ground_effect_visible(1.0, 0.0, 100.0, 1_000.0));
        assert!(!ground_effect_visible(
            1.0,
            DUST_HAZE_MAX_ALTITUDE_M + 0.01,
            100.0,
            1_000.0
        ));
        assert!(!ground_effect_visible(1.0, 0.0, 1_000.1, 1_000.0));
        assert!(!ground_effect_visible(f64::NAN, 0.0, 100.0, 1_000.0));
    }
}
