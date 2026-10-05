//! Bounded atmospheric-flight shock and heating presentation.
//!
//! These children read the presentation-smoothed shock/heating intensities that
//! `effects.rs` already derives from authoritative flight conditions and thermal
//! state. They never resample atmosphere or heating physics and never write
//! simulation components.

use super::camera::nearest_active_camera_distance_m;
use super::components::{RocketGeometry, RocketPresentationQuality, RocketPresentationSmoothing};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy_mesh::{Indices, Mesh, PrimitiveTopology};
use std::collections::HashSet;

const ATMOSPHERIC_EFFECT_VISIBLE_THRESHOLD: f32 = 0.02;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RocketAtmosphericEffectLayer {
    Shock,
    Heating,
}

/// Presentation ownership of one atmospheric shell child.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct RocketAtmosphericEffect {
    owner: Entity,
    layer: RocketAtmosphericEffectLayer,
}

/// Reusable shell mesh and materials for every bounded atmospheric child.
#[derive(Resource, Default)]
pub(crate) struct RocketAtmosphericEffectAssets {
    shell_mesh: Option<Handle<Mesh>>,
    shock_material: Option<Handle<StandardMaterial>>,
    heating_material: Option<Handle<StandardMaterial>>,
}

impl RocketAtmosphericEffectAssets {
    fn initialize(&mut self, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
        if self.shell_mesh.is_some() {
            return;
        }
        self.shell_mesh = Some(meshes.add(atmospheric_shell_mesh()));
        self.shock_material = Some(materials.add(StandardMaterial {
            base_color: Color::srgba(0.86, 0.92, 1.0, 0.16),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        }));
        self.heating_material = Some(materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.34, 0.06, 0.22),
            emissive: Color::srgb(1.0, 0.28, 0.04).to_linear() * 4.0,
            alpha_mode: AlphaMode::Add,
            unlit: true,
            cull_mode: None,
            ..default()
        }));
    }
}

/// Low-poly unit shell. Vertex colours fade from the equator toward the poles
/// so the additive shock/heating shells read as a soft envelope rather than a
/// hard sphere, and normals are irrelevant because both materials are unlit.
fn atmospheric_shell_mesh() -> Mesh {
    const LATITUDE_SEGMENTS: usize = 8;
    const LONGITUDE_SEGMENTS: usize = 12;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();

    for lat in 0..=LATITUDE_SEGMENTS {
        let v = lat as f32 / LATITUDE_SEGMENTS as f32;
        let theta = v * std::f32::consts::PI;
        let ring_radius = theta.sin();
        let y = theta.cos();
        let fade = ring_radius;
        for lon in 0..=LONGITUDE_SEGMENTS {
            let u = lon as f32 / LONGITUDE_SEGMENTS as f32;
            let phi = u * std::f32::consts::TAU;
            positions.push([ring_radius * phi.cos(), y, ring_radius * phi.sin()]);
            normals.push([0.0, 1.0, 0.0]);
            colors.push([fade, fade, fade, fade]);
        }
    }

    let stride = (LONGITUDE_SEGMENTS + 1) as u32;
    for lat in 0..LATITUDE_SEGMENTS as u32 {
        for lon in 0..LONGITUDE_SEGMENTS as u32 {
            let a = lat * stride + lon;
            let b = a + 1;
            let c = a + stride;
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

/// Reconciles atmospheric shell children and updates their render-only state
/// from the already-smoothed shock/heating intensities. Quality `Disabled`
/// hides existing shells and prevents new ones from being spawned.
#[expect(
    clippy::too_many_arguments,
    reason = "One adapter needs bounded assets, camera quality, smoothed inputs, and child presentation state."
)]
#[expect(
    clippy::type_complexity,
    reason = "The disjoint queries keep the rocket and effect Transform access explicit."
)]
pub(crate) fn update_rocket_atmospheric_effects(
    mut commands: Commands,
    quality: Res<RocketPresentationQuality>,
    mut assets: ResMut<RocketAtmosphericEffectAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cameras: Query<(&Camera, &Transform), (With<Camera3d>, Without<RocketAtmosphericEffect>)>,
    rockets: Query<
        (
            Entity,
            &RocketPresentationSmoothing,
            &RocketGeometry,
            &Transform,
        ),
        Without<RocketAtmosphericEffect>,
    >,
    mut effects: Query<
        (
            Entity,
            &mut RocketAtmosphericEffect,
            &mut Transform,
            &mut Visibility,
        ),
        Without<Camera3d>,
    >,
) {
    assets.initialize(&mut meshes, &mut materials);
    let enabled = quality.level.effects_enabled();
    let existing = effects
        .iter()
        .map(|(entity, effect, _, _)| (entity, *effect))
        .collect::<Vec<_>>();
    let rocket_entities = rockets
        .iter()
        .map(|(entity, ..)| entity)
        .collect::<HashSet<_>>();

    // Self-heal shells whose owner was removed.
    for (entity, effect) in &existing {
        if !rocket_entities.contains(&effect.owner) {
            commands.entity(*entity).despawn();
        }
    }

    for (rocket_entity, smoothing, geometry, rocket_transform) in rockets.iter() {
        if enabled {
            for layer in [
                RocketAtmosphericEffectLayer::Shock,
                RocketAtmosphericEffectLayer::Heating,
            ] {
                if existing
                    .iter()
                    .any(|(_, effect)| effect.owner == rocket_entity && effect.layer == layer)
                {
                    continue;
                }
                let (mesh, material) = asset_handles(&assets, layer);
                commands.entity(rocket_entity).with_children(|parent| {
                    parent.spawn((
                        RocketAtmosphericEffect {
                            owner: rocket_entity,
                            layer,
                        },
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        Transform::default(),
                        Visibility::Hidden,
                    ));
                });
            }
        }

        let camera_distance_m =
            nearest_active_camera_distance_m(rocket_transform.translation, &cameras);
        for (_entity, effect, mut transform, mut visibility) in &mut effects {
            if effect.owner != rocket_entity {
                continue;
            }
            let intensity = match effect.layer {
                RocketAtmosphericEffectLayer::Shock => smoothing.shock_intensity_unit,
                RocketAtmosphericEffectLayer::Heating => smoothing.heating_intensity_unit,
            };
            let visible = enabled
                && atmospheric_effect_visible(
                    intensity,
                    camera_distance_m,
                    quality.max_effect_distance_m,
                );
            *visibility = if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            if visible {
                *transform = shell_transform(*effect, geometry, intensity);
            }
        }
    }
}

fn atmospheric_effect_visible(
    intensity_unit: f32,
    camera_distance_m: f32,
    max_effect_distance_m: f32,
) -> bool {
    intensity_unit.is_finite()
        && intensity_unit > ATMOSPHERIC_EFFECT_VISIBLE_THRESHOLD
        && camera_distance_m.is_finite()
        && camera_distance_m <= max_effect_distance_m
}

fn shell_transform(
    effect: RocketAtmosphericEffect,
    geometry: &RocketGeometry,
    intensity_unit: f32,
) -> Transform {
    // Shock wraps the upper body near max-Q; heating wraps the base during
    // entry. Both stay children of the rocket, so render-origin rebasing is
    // inherited rather than recomputed.
    let (radius_scale, y_center_frac, length_frac) = match effect.layer {
        RocketAtmosphericEffectLayer::Shock => (1.25, 0.3, 0.6),
        RocketAtmosphericEffectLayer::Heating => (1.08, -0.05, 0.4),
    };
    let radius_m = geometry.radius_m * radius_scale;
    let length_m = (geometry.height_m * length_frac).max(0.5);
    let grow = 0.85 + intensity_unit.clamp(0.0, 1.0) * 0.25;
    Transform::from_translation(Vec3::new(0.0, geometry.height_m * y_center_frac, 0.0))
        .with_scale(Vec3::new(radius_m, length_m, radius_m) * grow)
}

fn asset_handles(
    assets: &RocketAtmosphericEffectAssets,
    layer: RocketAtmosphericEffectLayer,
) -> (Handle<Mesh>, Handle<StandardMaterial>) {
    let mesh = assets
        .shell_mesh
        .clone()
        .expect("atmospheric shell mesh initialized");
    let material = match layer {
        RocketAtmosphericEffectLayer::Shock => assets
            .shock_material
            .clone()
            .expect("shock material initialized"),
        RocketAtmosphericEffectLayer::Heating => assets
            .heating_material
            .clone()
            .expect("heating material initialized"),
    };
    (mesh, material)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atmospheric_shell_fades_from_equator_to_poles() {
        let mesh = atmospheric_shell_mesh();
        let Some(bevy_mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("shell mesh must carry vertex colours");
        };
        let pole_alpha = colors.first().expect("pole vertex")[3];
        let equator_alpha = colors
            .get(4 * 13)
            .map(|color| color[3])
            .expect("equator vertex");
        assert!(pole_alpha < 1e-6, "pole must fade out: {pole_alpha}");
        assert!(
            equator_alpha > 0.99,
            "equator must be opaque: {equator_alpha}"
        );
    }

    #[test]
    fn visibility_requires_intensity_distance_and_finite_inputs() {
        assert!(atmospheric_effect_visible(1.0, 1_000.0, 20_000.0));
        assert!(!atmospheric_effect_visible(0.0, 1_000.0, 20_000.0));
        assert!(!atmospheric_effect_visible(1.0, 20_000.1, 20_000.0));
        assert!(!atmospheric_effect_visible(f32::NAN, 1_000.0, 20_000.0));
    }

    #[test]
    fn shell_transform_is_rocket_local_and_origin_independent() {
        let geometry = RocketGeometry {
            radius_m: 2.0,
            height_m: 30.0,
            lower_extent_y_m: -15.0,
        };
        let shock = shell_transform(
            RocketAtmosphericEffect {
                owner: Entity::PLACEHOLDER,
                layer: RocketAtmosphericEffectLayer::Shock,
            },
            &geometry,
            1.0,
        );
        assert_eq!(shock.translation.x, 0.0);
        assert_eq!(shock.translation.z, 0.0);
        assert!(shock.translation.y > 0.0 && shock.translation.y < geometry.height_m);
    }
}
