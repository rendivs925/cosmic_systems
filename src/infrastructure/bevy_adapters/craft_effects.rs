use super::craft_components::*;
use bevy::prelude::*;
use rand::Rng;

#[derive(Component)]
pub struct CraftGlowMaterial(pub Handle<StandardMaterial>);

pub fn update_craft_visuals(
    control: Res<CraftControlState>,
    craft_query: Query<&CraftGlowMaterial>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dc = control.dc_current;
    let pulse = control.pulse_current;
    let emissive_base = 0.2 + dc * 0.8;

    for glow in craft_query.iter() {
        if let Some(mat) = materials.get_mut(&glow.0) {
            let r = emissive_base * (0.3 + pulse * 0.7);
            let g = emissive_base * (0.1 + dc * 0.3);
            let b = emissive_base * (0.05 + dc * 0.15);
            mat.emissive = LinearRgba::new(r, g, b, 1.0);
            mat.base_color = Color::srgba(0.0, 0.0, 0.0, 0.0);
        }
    }
}

pub fn spawn_zpe_effects(
    time: Res<Time>,
    control: Res<CraftControlState>,
    craft_query: Query<&Transform, With<CraftVisual>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if control.pulse_current < 0.1 {
        return;
    }
    let mut rng = rand::thread_rng();
    let spawn_chance = (control.pulse_current * 0.3 * time.delta_secs() * 60.0).min(1.0);
    if rng.gen::<f32>() > spawn_chance {
        return;
    }

    let Ok(transform) = craft_query.single() else {
        return;
    };
    let pos = transform.translation;

    let ring_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.8, 1.0, 0.6),
        emissive: LinearRgba::new(0.0, 0.5, 0.8, 1.0),
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Torus::new(0.3, 0.05))),
        MeshMaterial3d(ring_mat),
        Transform::from_translation(pos).with_rotation(Quat::from_rotation_x(
            rng.gen::<f32>() * std::f32::consts::TAU,
        )),
        ExpandingRing {
            lifetime: 1.5,
            age: 0.0,
            max_scale: 3.0 + control.pulse_current * 5.0,
        },
    ));

    if control.pulse_current > 0.15 && rng.gen_bool(0.3) {
        let spark_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.8, 0.3),
            emissive: LinearRgba::new(1.0, 0.6, 0.0, 1.0),
            ..default()
        });
        let dir = Vec3::new(
            rng.gen_range(-1.0..1.0),
            rng.gen_range(-1.0..1.0),
            rng.gen_range(-1.0..1.0),
        )
        .normalize_or_zero();
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(0.03))),
            MeshMaterial3d(spark_mat),
            Transform::from_translation(pos),
            SparkParticle {
                velocity: dir * (1.0 + control.pulse_current * 3.0),
                lifetime: 0.8,
                age: 0.0,
            },
        ));
    }
}

#[derive(Component)]
pub struct ExpandingRing {
    pub lifetime: f32,
    pub age: f32,
    pub max_scale: f32,
}

#[derive(Component)]
pub struct SparkParticle {
    pub velocity: Vec3,
    pub lifetime: f32,
    pub age: f32,
}

pub fn update_zpe_effects(
    time: Res<Time>,
    mut commands: Commands,
    mut ring_query: Query<(Entity, &mut Transform, &mut ExpandingRing), Without<SparkParticle>>,
    mut spark_query: Query<(Entity, &mut Transform, &mut SparkParticle), Without<ExpandingRing>>,
) {
    let dt = time.delta_secs();

    for (entity, mut transform, mut ring) in ring_query.iter_mut() {
        ring.age += dt;
        let progress = (ring.age / ring.lifetime).min(1.0);
        transform.scale = Vec3::splat(0.1 + progress * ring.max_scale);
        transform.translation.y += dt * 0.5;
        if ring.age >= ring.lifetime {
            commands.entity(entity).despawn();
        }
    }

    for (entity, mut transform, mut spark) in spark_query.iter_mut() {
        spark.age += dt;
        transform.translation += spark.velocity * dt;
        let life = (1.0 - spark.age / spark.lifetime).max(0.0);
        transform.scale = Vec3::splat(life);
        if spark.age >= spark.lifetime {
            commands.entity(entity).despawn();
        }
    }
}
