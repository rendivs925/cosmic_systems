use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;

pub fn create_planet_material(
    base_color_texture: Option<Handle<Image>>,
    normal_map_texture: Option<Handle<Image>>,
    emissive_texture: Option<Handle<Image>>,
    base_color: Color,
    emissive: LinearRgba,
    unlit: bool,
    metallic: f32,
    reflectance: f32,
    perceptual_roughness: f32,
) -> StandardMaterial {
    StandardMaterial {
        base_color_texture,
        normal_map_texture,
        emissive_texture,
        base_color,
        emissive,
        unlit,
        metallic,
        reflectance,
        perceptual_roughness,
        ..default()
    }
}

pub fn create_orbit_material(
    base_color: Color,
    emissive: LinearRgba,
    alpha: f32,
) -> StandardMaterial {
    StandardMaterial {
        base_color: base_color.with_alpha(alpha),
        emissive,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        ..default()
    }
}

pub fn create_ring_material(
    base_color_texture: Option<Handle<Image>>,
    base_color: Color,
    emissive: LinearRgba,
) -> StandardMaterial {
    StandardMaterial {
        base_color_texture,
        base_color,
        metallic: 0.0,
        reflectance: 0.8,
        perceptual_roughness: 0.2,
        emissive,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        unlit: false,
        ..default()
    }
}

pub fn create_cloud_material(
    base_color_texture: Option<Handle<Image>>,
    alpha: f32,
) -> StandardMaterial {
    StandardMaterial {
        base_color_texture,
        base_color: Color::srgba(1.0, 1.0, 1.0, alpha),
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        perceptual_roughness: 0.9,
        unlit: true,
        ..default()
    }
}

pub fn orbit_emissive(color: Color, intensity: f32) -> LinearRgba {
    let linear: LinearRgba = color.into();
    LinearRgba::new(
        linear.red * intensity,
        linear.green * intensity,
        linear.blue * intensity,
        1.0,
    )
}
