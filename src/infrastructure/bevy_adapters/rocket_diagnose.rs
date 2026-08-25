//! TEMPORARY diagnostic system for rocket-mode rendering investigation.
//! Remove before finishing.

use crate::components::rocket::*;
use crate::infrastructure::bevy_adapters::terrain_render::{RenderOrigin, TerrainPatchRenderState};
use bevy::prelude::*;
use bevy::camera::visibility::VisibleEntities;

pub fn diagnose_rocket_scene(
    time: Res<Time>,
    mut last: Local<f32>,
    mut test_spawned: Local<bool>,
    mut test_cube_entity: Local<Option<Entity>>,
    mut commands: Commands,
    mut test_meshes: ResMut<Assets<Mesh>>,
    mut test_materials: ResMut<Assets<StandardMaterial>>,
    render_origin: Res<RenderOrigin>,
    clear_color: Res<ClearColor>,
    camera_query: Query<(&Transform, &Projection), With<Camera3d>>,
    rocket_query: Query<(Entity, &Transform, &GlobalTransform, &ViewVisibility, Option<&Mesh3d>), With<RocketPhysicsState>>,
    terrain_query: Query<&TerrainPatchRenderState>,
    sphere_entities: Query<Entity, With<RocketEarthSphere>>,
    cube_visibility: Query<(Entity, &ViewVisibility, &GlobalTransform)>,
) {
    if time.elapsed_secs() - *last < 1.0 {
        return;
    }
    *last = time.elapsed_secs();

    if !*test_spawned {
        *test_spawned = true;
        // Spawn a fresh entity with the ROCKET's OWN mesh handle + a bright
        // material. If this renders, the rocket entity setup is the problem;
        // if not, the mesh/asset itself fails to draw.
        let rocket_mesh = rocket_query.iter().next().and_then(|(_, _, _, _, m)| m.cloned());
        if let Some(handle) = rocket_mesh {
            let mat = test_materials.add(StandardMaterial {
                base_color: Color::srgb(0.0, 1.0, 0.0),
                unlit: true,
                ..default()
            });
            let cube = commands
                .spawn((
                    Mesh3d(handle.0),
                    MeshMaterial3d(mat),
                    Transform::from_xyz(0.0, 10.0, 0.0),
                    Name::new("ROCKET_MESH_COPY"),
                ))
                .id();
            *test_cube_entity = Some(cube);
            bevy::log::info!("DIAG spawned rocket-mesh copy at +10Y entity={cube}");
        } else {
            bevy::log::warn!("DIAG: rocket has no Mesh3d");
        }
    }

    let cam = camera_query
        .iter()
        .next()
        .map(|(t, p)| {
            format!(
                "cam pos={:?} rot={:?} proj={:?}",
                t.translation,
                t.rotation,
                match p {
                    Projection::Perspective(pp) => format!(
                        "Perspective near={:.3} far={:.0} fov={:.1}",
                        pp.near, pp.far, pp.fov
                    ),
                    _ => format!("{p:?}"),
                }
            )
        })
        .unwrap_or_else(|| "NO CAMERA3d".into());

    let rocket = rocket_query
        .iter()
        .next()
        .map(|(e, t, gt, vv, _m)| {
            format!(
                "rocket entity={e} pos={:?} global={:?} visible={}",
                t.translation,
                gt.translation(),
                vv.get()
            )
        })
        .unwrap_or_else(|| "NO ROCKET".into());

    let cubes: Vec<String> = test_cube_entity
        .iter()
        .filter_map(|e| cube_visibility.get(*e).ok())
        .map(|(e, vv, gt)| format!("cube {e} visible={} pos={:?}", vv.get(), gt.translation()))
        .collect();

    bevy::log::info!(
        "DIAG[t={:.1}] origin={:?} clear={:?} sphere_count={}\n  {}\n  {}\n  terrain_patches={} cubes={:?}",
        time.elapsed_secs(),
        render_origin.origin,
        clear_color.0,
        sphere_entities.iter().count(),
        cam,
        rocket,
        terrain_query.iter().count(),
        cubes,
    );
}