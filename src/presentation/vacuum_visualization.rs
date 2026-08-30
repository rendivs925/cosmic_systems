use bevy::prelude::*;
use rand::Rng;

use crate::domain::value_objects::education::EducationState;
use crate::infrastructure::bevy_adapters::craft_components::{
    CraftComponent, CraftControlState, CraftVisual,
};

#[derive(Component)]
pub struct FieldGradientRing {
    pub dc_field: f32,
}

#[derive(Component)]
pub struct VirtualParticle {
    pub velocity: Vec3,
    pub lifetime: f32,
    pub age: f32,
}

#[derive(Component)]
pub struct ZpeRipple {
    pub lifetime: f32,
    pub age: f32,
    pub max_scale: f32,
}

pub fn spawn_field_gradient(
    craft_query: Query<Entity, (With<CraftComponent>, With<CraftVisual>)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(craft_entity) = craft_query.single() else {
        return;
    };

    let upper_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.3, 0.0, 0.15),
        emissive: LinearRgba::new(0.3, 0.1, 0.0, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands.entity(craft_entity).with_child((
        Mesh3d(meshes.add(Torus::new(1.0, 0.05))),
        MeshMaterial3d(upper_mat),
        Transform::from_xyz(0.0, 0.6, 0.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        FieldGradientRing { dc_field: 0.0 },
    ));

    let lower_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.3, 0.8, 0.15),
        emissive: LinearRgba::new(0.0, 0.1, 0.3, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands.entity(craft_entity).with_child((
        Mesh3d(meshes.add(Torus::new(1.1, 0.06))),
        MeshMaterial3d(lower_mat),
        Transform::from_xyz(0.0, -0.4, 0.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        FieldGradientRing { dc_field: 0.0 },
    ));
}

pub fn update_field_gradient(
    control: Res<CraftControlState>,
    mut ring_query: Query<(
        &mut MeshMaterial3d<StandardMaterial>,
        &mut FieldGradientRing,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dc = control.dc_current;
    let intensity = (dc * 2.0).min(1.0);

    for (mat_handle, mut ring) in ring_query.iter_mut() {
        ring.dc_field = dc;
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            let alpha = 0.05 + intensity * 0.25;
            mat.base_color.set_alpha(alpha);
        }
    }
}

pub fn spawn_virtual_particles(
    time: Res<Time>,
    control: Res<CraftControlState>,
    state: Res<EducationState>,
    craft_query: Query<&Transform, With<CraftComponent>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !state.show_particles || !state.panel_open {
        return;
    }
    let Ok(transform) = craft_query.single() else {
        return;
    };

    let mut rng = rand::thread_rng();
    let spawn_rate = control.dc_current * 30.0 * time.delta_secs();
    if rng.gen::<f32>() > spawn_rate.min(1.0) {
        return;
    }

    let particle_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.6, 0.8, 1.0, 0.6),
        emissive: LinearRgba::new(0.1, 0.2, 0.4, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    let offset = Vec3::new(
        rng.gen_range(-1.2..1.2),
        rng.gen_range(-0.8..0.8),
        rng.gen_range(-1.2..1.2),
    );
    let pos = transform.translation + offset;

    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(0.02 + rng.gen::<f32>() * 0.03))),
        MeshMaterial3d(particle_mat),
        Transform::from_translation(pos),
        VirtualParticle {
            velocity: Vec3::new(
                rng.gen_range(-0.2..0.2),
                rng.gen_range(0.1..0.3),
                rng.gen_range(-0.2..0.2),
            ),
            lifetime: 0.5 + rng.gen::<f32>() * 1.0,
            age: 0.0,
        },
    ));
}

pub fn update_virtual_particles(
    time: Res<Time>,
    mut commands: Commands,
    mut particle_query: Query<(Entity, &mut Transform, &mut VirtualParticle)>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut particle) in particle_query.iter_mut() {
        particle.age += dt;
        transform.translation += particle.velocity * dt;
        let life = (1.0 - particle.age / particle.lifetime).max(0.0);
        let scale = life * 0.05;
        transform.scale = Vec3::splat(scale.max(0.001));
        if particle.age >= particle.lifetime {
            commands.entity(entity).despawn();
        }
    }
}

pub fn spawn_zpe_ripple(
    time: Res<Time>,
    control: Res<CraftControlState>,
    state: Res<EducationState>,
    craft_query: Query<&Transform, With<CraftComponent>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !state.show_ripples || !state.panel_open {
        return;
    }
    if control.pulse_current < 0.05 {
        return;
    }

    let mut rng = rand::thread_rng();
    let spawn_chance = control.pulse_current * 0.4 * time.delta_secs() * 60.0;
    if rng.gen::<f32>() > spawn_chance.min(1.0) {
        return;
    }

    let Ok(transform) = craft_query.single() else {
        return;
    };
    let pos = transform.translation;

    let ripple_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.8, 1.0, 0.4),
        emissive: LinearRgba::new(0.0, 0.5, 0.8, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    commands.spawn((
        Mesh3d(meshes.add(Torus::new(0.3, 0.03))),
        MeshMaterial3d(ripple_mat),
        Transform::from_translation(pos).with_rotation(Quat::from_rotation_x(
            rng.gen::<f32>() * std::f32::consts::TAU,
        )),
        ZpeRipple {
            lifetime: 1.2,
            age: 0.0,
            max_scale: 2.0 + control.pulse_current * 4.0,
        },
    ));
}

pub fn update_zpe_ripples(
    time: Res<Time>,
    mut commands: Commands,
    mut ripple_query: Query<(Entity, &mut Transform, &mut ZpeRipple)>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut ripple) in ripple_query.iter_mut() {
        ripple.age += dt;
        let progress = (ripple.age / ripple.lifetime).min(1.0);
        transform.scale = Vec3::splat(0.1 + progress * ripple.max_scale);
        transform.translation.y += dt * 0.3;
        if ripple.age >= ripple.lifetime {
            commands.entity(entity).despawn();
        }
    }
}
