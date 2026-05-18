use bevy::prelude::*;

use crate::domain::value_objects::education::{EducationMode, EducationState};
use crate::infrastructure::bevy_adapters::craft_components::CraftComponent;

#[derive(Component)]
pub struct CompareModeRoot;

#[derive(Component)]
pub struct RocketCraftTag;

#[derive(Component)]
pub struct RocketPhysics {
    pub position: Vec3,
    pub velocity: Vec3,
    pub throttle: f32,
    pub fuel: f32,
    pub mass: f32,
}

#[derive(Component)]
pub struct RocketHudText;

pub fn spawn_rocket_craft(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    craft_query: Query<&Transform, With<CraftComponent>>,
    state: Res<EducationState>,
) {
    if state.mode != EducationMode::Compare {
        return;
    }

    let Ok(vacuum_pos) = craft_query.single() else { return };
    let rocket_offset = Vec3::new(5.0, 0.0, 0.0);
    let rocket_pos = vacuum_pos.translation + rocket_offset;

    let body_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.6, 0.6, 0.65),
        metallic: 0.8,
        perceptual_roughness: 0.3,
        ..default()
    });
    let nose_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.2, 0.1),
        ..default()
    });

    commands
        .spawn((
            RocketCraftTag,
            RocketPhysics {
                position: rocket_pos,
                velocity: Vec3::ZERO,
                throttle: 0.0,
                fuel: 100.0,
                mass: 1000.0,
            },
            Transform::from_translation(rocket_pos),
            Visibility::default(),
        ))
        .with_children(|p| {
            p.spawn((
                Mesh3d(meshes.add(Cylinder::new(1.0, 0.3))),
                MeshMaterial3d(body_mat),
                Transform::from_xyz(0.0, 0.0, 0.0),
            ));
            p.spawn((
                Mesh3d(meshes.add(Cylinder::new(0.3, 0.3))),
                MeshMaterial3d(nose_mat),
                Transform::from_xyz(0.0, 0.65, 0.0),
            ));
        });
}

pub fn update_rocket_physics(
    time: Res<Time>,
    state: Res<EducationState>,
    mut rocket_query: Query<(&mut RocketPhysics, &mut Transform)>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if state.mode != EducationMode::Compare {
        return;
    }
    let dt = time.delta_secs().min(0.05);

    for (mut rocket, mut transform) in rocket_query.iter_mut() {
        if keys.pressed(KeyCode::ArrowUp) {
            rocket.throttle = (rocket.throttle + 2.0 * dt).min(1.0);
        }
        if keys.pressed(KeyCode::ArrowDown) {
            rocket.throttle = (rocket.throttle - 2.0 * dt).max(0.0);
        }

        if rocket.fuel > 0.0 && rocket.throttle > 0.01 {
            let max_thrust = 15000.0;
            let thrust_force = rocket.throttle * max_thrust;
            let acceleration = thrust_force / rocket.mass;
            rocket.velocity.y += acceleration * dt;
            let fuel_burn = rocket.throttle * 5.0 * dt;
            rocket.fuel = (rocket.fuel - fuel_burn).max(0.0);
        }

        let vel = rocket.velocity;
        rocket.position += vel * dt;
        transform.translation = rocket.position;
    }
}

pub fn update_rocket_hud(
    state: Res<EducationState>,
    _rocket_query: Query<&RocketPhysics>,
) {
    if state.mode != EducationMode::Compare {
        return;
    }
}
