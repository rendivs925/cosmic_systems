use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy_mesh::{Indices, PrimitiveTopology};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::Starfield;

const STAR_SEED: u64 = 0xC05F_1C5A;
const STAR_COUNT: usize = 3_000;
const MILKY_WAY_COUNT: usize = 900;

pub fn spawn_starfield(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    solar_params: &SolarSystemParameters,
) {
    let radius = solar_params.au_to_units(90.0);
    let mesh = meshes.add(create_starfield_mesh(radius));
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: LinearRgba::rgb(0.25, 0.28, 0.34),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        ..default()
    });

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::default(),
        Starfield,
        Name::new("Procedural Starfield"),
    ));
}

fn create_starfield_mesh(radius: f32) -> Mesh {
    let total = STAR_COUNT + MILKY_WAY_COUNT;
    let mut positions = Vec::with_capacity(total * 5);
    let mut normals = Vec::with_capacity(total * 5);
    let mut uvs = Vec::with_capacity(total * 5);
    let mut colors = Vec::with_capacity(total * 5);
    let mut indices = Vec::with_capacity(total * 12);
    let mut rng = StdRng::seed_from_u64(STAR_SEED);

    for _ in 0..STAR_COUNT {
        let direction = random_unit_vector(&mut rng);
        let size = rng.gen_range(260.0..900.0) * brightness_bias(&mut rng);
        let alpha = rng.gen_range(0.35..0.85);
        let color = star_color(&mut rng, alpha);
        push_star_quad(
            direction,
            radius,
            size,
            color,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut colors,
            &mut indices,
        );
    }

    for _ in 0..MILKY_WAY_COUNT {
        let direction = milky_way_direction(&mut rng);
        let size = rng.gen_range(320.0..1_300.0) * brightness_bias(&mut rng);
        let alpha = rng.gen_range(0.10..0.32);
        let color = star_color(&mut rng, alpha);
        push_star_quad(
            direction,
            radius,
            size,
            color,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut colors,
            &mut indices,
        );
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn random_unit_vector(rng: &mut StdRng) -> Vec3 {
    let z = rng.gen_range(-1.0..1.0);
    let theta = rng.gen_range(0.0..std::f32::consts::TAU);
    let radius = (1.0_f32 - z * z).sqrt();
    Vec3::new(radius * theta.cos(), z, radius * theta.sin())
}

fn milky_way_direction(rng: &mut StdRng) -> Vec3 {
    let longitude: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
    let latitude: f32 = rng.gen_range(-0.12..0.12) + rng.gen_range(-0.05..0.05);
    let flat = Vec3::new(longitude.cos(), latitude.sin(), longitude.sin()).normalize();
    Quat::from_euler(EulerRot::XYZ, 0.42, 0.15, -0.24) * flat
}

fn brightness_bias(rng: &mut StdRng) -> f32 {
    if rng.gen_bool(0.06) {
        rng.gen_range(1.6..3.0)
    } else {
        rng.gen_range(0.55..1.15)
    }
}

fn star_color(rng: &mut StdRng, alpha: f32) -> [f32; 4] {
    let base = match rng.gen_range(0..10) {
        0 => Color::srgb(0.65, 0.76, 1.0),
        1 => Color::srgb(1.0, 0.78, 0.58),
        2 => Color::srgb(1.0, 0.90, 0.72),
        _ => Color::srgb(0.86, 0.90, 0.96),
    };
    let linear: LinearRgba = base.into();
    [linear.red, linear.green, linear.blue, alpha]
}

#[allow(clippy::too_many_arguments)]
fn push_star_quad(
    direction: Vec3,
    radius: f32,
    size: f32,
    color: [f32; 4],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    let center = direction * radius;
    let right = direction.any_orthonormal_vector() * size;
    let up = direction.cross(right).normalize_or_zero() * size;
    let base_index = positions.len() as u32;
    let normal = (-direction).to_array();
    let edge_color = [color[0], color[1], color[2], 0.0];

    for (offset, uv, vertex_color) in [
        (Vec3::ZERO, [0.5, 0.5], color),
        (-up, [0.5, 0.0], edge_color),
        (right, [1.0, 0.5], edge_color),
        (up, [0.5, 1.0], edge_color),
        (-right, [0.0, 0.5], edge_color),
    ] {
        positions.push((center + offset).to_array());
        normals.push(normal);
        uvs.push(uv);
        colors.push(vertex_color);
    }

    indices.extend_from_slice(&[
        base_index,
        base_index + 1,
        base_index + 2,
        base_index,
        base_index + 2,
        base_index + 3,
        base_index,
        base_index + 3,
        base_index + 4,
        base_index,
        base_index + 4,
        base_index + 1,
    ]);
}
