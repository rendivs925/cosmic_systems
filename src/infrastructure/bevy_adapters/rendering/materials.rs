use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

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

pub const ORBIT_LINE_COLOR: Color = Color::srgb(0.72, 0.72, 0.76);

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
        // Clouds are lit by the shared Sun so they darken at night and across
        // the terminator instead of staying full-bright everywhere.
        unlit: false,
        ..default()
    }
}

/// Flight clouds use the existing geographic texture as coverage, not a black
/// opaque layer. The shared standard-material factory still owns lighting.
///
/// The extension separates the four visual inputs: the sampled base colour is
/// coverage, `shape_scale` drives low-frequency cloud masses, `detail_scale`
/// drives higher-frequency edges, and a bounded wind vector scrolls the detail
/// coherently. All four are presentation-only uniforms; no whole-field texture
/// is regenerated per frame.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct CloudExtension {
    #[uniform(100)]
    pub coverage: f32,
    /// Low-frequency structure scale, cycles per render unit.
    #[uniform(100)]
    pub shape_scale: f32,
    /// High-frequency detail scale, cycles per render unit.
    #[uniform(100)]
    pub detail_scale: f32,
    /// Detail scroll speed along the coherent wind direction.
    #[uniform(100)]
    pub wind_speed: f32,
    /// Presentation clock in seconds, advanced once per frame.
    #[uniform(100)]
    pub time_s: f32,
}

impl CloudExtension {
    /// Bounded defaults for one cloud deck. Phases match the WGSL struct order.
    pub fn deck() -> Self {
        Self {
            coverage: 1.0,
            shape_scale: 0.9,
            detail_scale: 4.5,
            wind_speed: 0.015,
            time_s: 0.0,
        }
    }
}

impl MaterialExtension for CloudExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/clouds.wgsl".into()
    }
}

pub type CloudMaterial = ExtendedMaterial<StandardMaterial, CloudExtension>;

/// Cloud altitude for the flight shell, in metres above the reference surface.
/// Solar-map exaggeration (`CloudLayerConfig::scale`) is not a flight altitude.
pub fn flight_cloud_altitude_m(body: &str) -> f32 {
    match body {
        "Venus" => 60_000.0,
        "Titan" => 20_000.0,
        _ => 6_000.0,
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
