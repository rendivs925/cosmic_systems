use crate::domain::entities::planet::BodyClass;
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;

#[derive(Debug, Clone)]
pub struct PlanetMaterialConfig {
    pub base_color_texture: Option<Handle<Image>>,
    pub normal_map_texture: Option<Handle<Image>>,
    pub emissive_texture: Option<Handle<Image>>,
    pub base_color: Color,
    pub emissive: LinearRgba,
    pub unlit: bool,
    pub metallic: f32,
    pub reflectance: f32,
    pub perceptual_roughness: f32,
}

impl Default for PlanetMaterialConfig {
    fn default() -> Self {
        Self {
            base_color_texture: None,
            normal_map_texture: None,
            emissive_texture: None,
            base_color: Color::WHITE,
            emissive: LinearRgba::BLACK,
            unlit: false,
            metallic: 0.0,
            reflectance: 0.5,
            perceptual_roughness: 0.5,
        }
    }
}

pub fn create_planet_material(config: PlanetMaterialConfig) -> StandardMaterial {
    StandardMaterial {
        base_color_texture: config.base_color_texture,
        normal_map_texture: config.normal_map_texture,
        emissive_texture: config.emissive_texture,
        base_color: config.base_color,
        emissive: config.emissive,
        unlit: config.unlit,
        metallic: config.metallic,
        reflectance: config.reflectance,
        perceptual_roughness: config.perceptual_roughness,
        ..default()
    }
}

// Keep the old function signature for backward compatibility
pub fn create_planet_material_legacy(
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
    create_planet_material(PlanetMaterialConfig {
        base_color_texture,
        normal_map_texture,
        emissive_texture,
        base_color,
        emissive,
        unlit,
        metallic,
        reflectance,
        perceptual_roughness,
    })
}

pub fn orbit_color_for(body_class: BodyClass, selected: bool) -> Color {
    let base = match body_class {
        BodyClass::Star => Color::srgb(1.0, 0.95, 0.85),
        BodyClass::Terrestrial => Color::srgb(0.95, 0.90, 0.80),
        BodyClass::GasGiant => Color::srgb(0.90, 0.85, 0.65),
        BodyClass::IceGiant => Color::srgb(0.65, 0.80, 0.90),
        BodyClass::Dwarf => Color::srgb(0.75, 0.75, 0.80),
        BodyClass::Moon => Color::srgb(0.72, 0.72, 0.76),
    };
    if selected {
        let linear: LinearRgba = base.into();
        Color::srgb(
            (linear.red * 1.3).min(1.0),
            (linear.green * 1.3).min(1.0),
            (linear.blue * 1.3).min(1.0),
        )
    } else {
        base
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
