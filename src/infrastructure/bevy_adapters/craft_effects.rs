use super::craft_components::*;
use bevy::prelude::*;

#[expect(
    clippy::type_complexity,
    reason = "The effect query updates optional craft effect components in one pass."
)]
pub fn update_craft_visuals(
    time: Res<Time>,
    craft_query: Query<(Entity, &CraftComponent)>,
    children_query: Query<&Children>,
    mut effect_query: Query<(
        &mut Transform,
        &mut Visibility,
        &mut MeshMaterial3d<StandardMaterial>,
        Option<&CraftBubble>,
        Option<&CraftRing>,
        Option<&CraftCoreGlow>,
        Option<&CraftLens>,
        Option<&CraftWake>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    effects_enabled: Res<CraftEffectsEnabled>,
) {
    let Ok((craft_entity, craft)) = craft_query.single() else {
        return;
    };
    let Ok(children) = children_query.get(craft_entity) else {
        return;
    };
    let dt = time.delta_secs().min(0.05);
    let elapsed = time.elapsed_secs();

    let dc = craft.dc_field;
    let pulse = craft.pulse_resonance;
    let zpe = craft.physics.zpe_kilowatts;
    let speed = craft.linear_velocity.length();
    let parametric = craft.physics.parametric_gain;
    let speed_max = match craft.speed_mode {
        SpeedMode::Hover => 0.0,
        SpeedMode::Cruise => 40000.0 * dc,
        SpeedMode::Sprint => 120000.0 * dc,
    };
    let speed_ratio = if speed_max > 0.01 {
        (speed / speed_max).min(1.0)
    } else {
        0.0
    };
    let resonance_phase = elapsed * 22.4 * pulse;

    for i in 0..children.len() {
        let child = children[i];
        let Ok((mut transform, mut vis, mat_handle, bubble, ring, core, lens, wake)) =
            effect_query.get_mut(child)
        else {
            continue;
        };
        let is_effect = bubble.is_some()
            || ring.is_some()
            || core.is_some()
            || lens.is_some()
            || wake.is_some();
        if !effects_enabled.0 && is_effect {
            *vis = Visibility::Hidden;
            continue;
        }
        if is_effect {
            *vis = Visibility::Visible;
        }

        if bubble.is_some() {
            let breathe = 1.0 + (resonance_phase * 0.5).sin() * 0.04;
            let oblate = 1.0 - dc * 0.12;
            transform.scale = Vec3::new(breathe, breathe * oblate, breathe);
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                let a = 0.05 + dc * 0.25 + parametric as u32 as f32 * 0.08;
                mat.base_color = Color::srgba(0.05, 0.1, 0.25, a);
                let e = 0.02 + dc * 0.12 + pulse * 0.15;
                mat.emissive = LinearRgba::new(e * 0.3, e * 0.6, e, 1.0);
            }
        }

        if ring.is_some() {
            transform.rotation *=
                Quat::from_rotation_y((0.5 + pulse * 2.0 + speed_ratio * 1.5) * dt);
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                let b = 0.1 + pulse * 0.5 + zpe * 0.0008;
                let a = 0.2 + pulse * 0.4 + parametric as u32 as f32 * 0.25;
                mat.base_color = Color::srgba(0.2, 0.4, 0.8, a.min(1.0));
                mat.emissive = LinearRgba::new(b * 0.4, b * 0.7, b, 1.0);
                if parametric {
                    let flash = ((elapsed * 44.8).sin() * 0.5 + 0.5).powf(6.0);
                    mat.emissive = LinearRgba::new(
                        b * 0.8 + flash * 0.6,
                        b * 1.0 + flash * 0.4,
                        b * 1.2 + flash * 0.2,
                        1.0,
                    );
                }
            }
        }

        if core.is_some() {
            let p = if zpe > 5.0 {
                0.5 + (resonance_phase * 3.0).sin() * 0.3 + zpe * 0.0003
            } else {
                0.2
            }
            .min(1.0);
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.emissive = LinearRgba::new(0.2 + p * 0.8, 0.5 + p * 0.5, 0.8 + p * 0.2, 1.0);
                if parametric {
                    let flash = ((elapsed * 44.8).sin() * 0.5 + 0.5).powf(8.0);
                    mat.emissive = LinearRgba::new(1.0, 0.6 + flash * 0.4, 0.3 + flash * 0.7, 1.0);
                }
            }
            transform.scale = Vec3::splat(1.0 + (resonance_phase * 2.0).sin() * 0.1 * pulse);
        }

        if lens.is_some() {
            transform.scale = Vec3::new(1.0, 1.0 - speed_ratio * 0.2, 1.0 + speed_ratio * 2.0);
            transform.translation.z = speed_ratio * 3.0;
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.base_color = Color::srgba(
                    0.08,
                    0.12,
                    0.25,
                    (0.02 + dc * 0.06 + speed_ratio * 0.05).min(0.2),
                );
            }
        }

        if wake.is_some() {
            let len = 1.0 + speed_ratio * 7.0;
            transform.scale = Vec3::new(0.5, 0.3, len);
            transform.translation.z = -(len * 0.5 + 1.0);
            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.base_color = Color::srgba(0.08, 0.04, 0.15, speed_ratio * 0.12);
                mat.emissive = LinearRgba::new(0.0, 0.0, speed_ratio * 0.04, 1.0);
            }
        }
    }
}
