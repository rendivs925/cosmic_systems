use bevy::math::Mat3;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
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

/// Maximum sun-march steps; must match `MAX_LIGHT_STEPS` in `clouds.wgsl`.
pub const MAX_CLOUD_LIGHT_STEPS: f32 = 8.0;

/// Single-scattering albedo of cloud droplets, unitless. Must match
/// `CLOUD_ALBEDO` in `clouds.wgsl`. Water absorbs almost nothing at visible
/// wavelengths, so droplet scattering is nearly conservative; this is a
/// physical constant, not a brightness knob. No presentation scale factor is
/// applied to the single-scatter integral.
pub const CLOUD_SINGLE_SCATTER_ALBEDO: f32 = 0.99;

/// Uniform parameters for the volumetric cloud shader. Field order and types
/// must match the `CloudParams` WGSL struct exactly.
///
/// The values are presentation-only. The renderer ray-marches the spherical
/// cloud shell between `bottom_radius` and `top_radius`, samples the 3D
/// Perlin-Worley field (co-rotating with the planet via `planet_rotation`),
/// carves it with the weather map and detail erosion, and integrates
/// sun-scattering with a triple-lobe Henyey-Greenstein phase and multi-scatter
/// octaves. The step integral is the physical single-scatter form,
/// `albedo * (1 - exp(-sigma_e ds))` in the incoming radiance scale, so every
/// uniform above has a physical meaning.
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct CloudParams {
    /// Planet centre in the camera-relative render frame.
    pub planet_center: Vec3,
    /// Cloud-base sphere radius, in metres from the planet centre.
    pub bottom_radius: f32,
    /// Cloud-top sphere radius, in metres from the planet centre.
    pub top_radius: f32,
    /// Body-fixed to planet-inertial rotation. Its transpose recovers a
    /// co-rotating body position for noise sampling.
    pub planet_rotation: Mat3,
    /// Unit direction from the observer toward the Sun.
    pub sun_direction: Vec3,
    /// Directional-light colour premultiplied by its illuminance.
    pub sun_irradiance: Vec3,
    /// Per-channel transmittance of the direct solar beam at the deck
    /// altitude, from the shared atmospheric optics. The diffuse term keeps the
    /// unattenuated irradiance because it is sky light rather than the beam.
    pub sun_transmittance: Vec3,
    /// Presentation clock in seconds; advects the wind only.
    pub time_s: f32,
    /// Global cloud-coverage multiplier.
    pub coverage: f32,
    /// Cloud-type morph: 0 stratus, 0.5 stratocumulus, 1 cumulus.
    pub cloud_type: f32,
    /// Density-scale multiplier.
    pub density_scale: f32,
    /// Extinction coefficient per unit density, in 1/m.
    pub absorption: f32,
    /// Forward Henyey-Greenstein lobe asymmetry.
    pub forward_g: f32,
    /// Base noise frequency, in volume tiles per metre.
    pub noise_scale: f32,
    /// Detail noise frequency relative to the base.
    pub detail_scale: f32,
    /// Edge-erosion strength.
    pub erosion_scale: f32,
    /// Wind advection speed along `wind_direction`, in metres per second.
    pub wind_speed: f32,
    /// Horizontal wind direction in the body-fixed x/z plane.
    pub wind_direction: Vec2,
    /// Primary ray-march step count.
    pub primary_steps: f32,
    /// Sun-march step count across the full cloud slab.
    pub light_steps: f32,
    /// Number of multi-scatter octaves.
    pub multi_scatter_octaves: f32,
    /// Ambient ground colour used to light cloud bases (bottom of the deck).
    pub ground_color: Vec3,
}

impl CloudParams {
    /// Bounded defaults for one Earth-like cloud deck (spec section 9 "Clear
    /// Day"), with radii filled in by the rocket planet system at spawn.
    pub fn deck() -> Self {
        Self {
            planet_center: Vec3::ZERO,
            bottom_radius: 0.0,
            top_radius: 0.0,
            planet_rotation: Mat3::IDENTITY,
            sun_direction: Vec3::Y,
            sun_irradiance: Vec3::ONE,
            sun_transmittance: Vec3::ONE,
            time_s: 0.0,
            coverage: 0.3,
            cloud_type: 0.68,
            density_scale: 0.6,
            absorption: 0.01,
            forward_g: 0.82,
            noise_scale: 1.0 / 4500.0,
            detail_scale: 2.5,
            erosion_scale: 0.5,
            wind_speed: 18.0,
            wind_direction: Vec2::new(1.0, 0.2),
            primary_steps: 96.0,
            light_steps: MAX_CLOUD_LIGHT_STEPS,
            multi_scatter_octaves: 3.0,
            ground_color: Vec3::new(0.35, 0.32, 0.28),
        }
    }
}

/// Density-relevant subset of [`CloudParams`], shared with the terrain
/// material's cloud-shadow uniform.
///
/// Mirrors `CloudField` in `assets/shaders/cloud_density.wgsl` field for field,
/// so the terrain shadows and the volumetric deck evaluate the same density
/// field and a shadow always lines up with the cloud that casts it. The terrain
/// pass only needs the density inputs, not the scattering or dome parameters.
/// Presentation only: nothing here feeds collision or flight dynamics.
#[derive(Resource, ShaderType, Reflect, Clone, Copy, Debug, Default, PartialEq)]
pub struct CloudField {
    /// Cloud-base sphere radius, in metres from the planet centre.
    pub bottom_radius: f32,
    /// Cloud-top sphere radius, in metres from the planet centre.
    pub top_radius: f32,
    /// Extinction coefficient per unit density, in 1/m. The shadow uses it
    /// directly, so its optical depth matches the deck's own opacity.
    pub absorption: f32,
    /// Presentation clock in seconds; advects the wind only.
    pub time_s: f32,
    /// Global cloud-coverage multiplier.
    pub coverage: f32,
    /// Cloud-type morph: 0 stratus, 0.5 stratocumulus, 1 cumulus.
    pub cloud_type: f32,
    /// Density-scale multiplier.
    pub density_scale: f32,
    /// Base noise frequency, in volume tiles per metre.
    pub noise_scale: f32,
    /// Detail noise frequency relative to the base.
    pub detail_scale: f32,
    /// Edge-erosion strength.
    pub erosion_scale: f32,
    /// Wind advection speed along `wind_direction`, in metres per second.
    pub wind_speed: f32,
    /// Horizontal wind direction in the body-fixed x/z plane.
    pub wind_direction: Vec2,
}

impl CloudField {
    /// The density inputs of one deck, taken after any climate mapping so the
    /// shadow follows the same coverage the deck is drawn with.
    pub fn from_cloud_params(params: &CloudParams) -> Self {
        Self {
            bottom_radius: params.bottom_radius,
            top_radius: params.top_radius,
            absorption: params.absorption,
            time_s: params.time_s,
            coverage: params.coverage,
            cloud_type: params.cloud_type,
            density_scale: params.density_scale,
            noise_scale: params.noise_scale,
            detail_scale: params.detail_scale,
            erosion_scale: params.erosion_scale,
            wind_speed: params.wind_speed,
            wind_direction: params.wind_direction,
        }
    }
}

/// Cloud-shadow uniform shared by the terrain and water materials.
///
/// The density field, the Sun direction, and the inertial-to-body rotation are
/// produced together by the cloud system, so every ground surface evaluates its
/// shadow against exactly the deck that was drawn. Mirrors `CloudShadow` in
/// `assets/shaders/cloud_density.wgsl`. Presentation only: nothing here feeds
/// collision or flight dynamics.
#[derive(Resource, ShaderType, Reflect, Clone, Copy, Debug, Default, PartialEq)]
pub struct CloudShadow {
    /// Density inputs of the low deck.
    pub field: CloudField,
    /// Unit direction from the planet toward the Sun in the inertial frame.
    pub sun_direction: Vec3,
    /// Beam transmittance of the atmosphere above the deck. The sea's mirror
    /// reflection needs it so a reflected deck reddens and dims with the same
    /// low-Sun extinction the drawn deck has.
    pub sun_transmittance: Vec3,
    /// Rotation from the inertial render frame into the body-fixed frame, as an
    /// `xyzw` quaternion. The density field is body-fixed, so both consumers
    /// rotate their fragment position and the Sun with it.
    pub inertial_to_body: Vec4,
}

impl CloudShadow {
    /// The shadow state of one drawn deck. The ground evaluates its shadow and
    /// its reflection against exactly the deck that was drawn, so every input is
    /// copied from the material the deck rendered with rather than recomputed.
    pub fn for_deck(params: &CloudParams, sun_direction: Vec3, inertial_to_body: Vec4) -> Self {
        Self {
            field: CloudField::from_cloud_params(params),
            sun_direction,
            sun_transmittance: params.sun_transmittance,
            inertial_to_body,
        }
    }
}

/// Volumetric ray-marched cloud material. Custom rather than an extension of
/// `StandardMaterial` because the fragment stage computes its own light
/// transport instead of using Bevy's PBR lighting.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct CloudMaterial {
    #[uniform(0)]
    pub params: CloudParams,
    /// Perlin-Worley base density volume.
    #[texture(1, dimension = "3d")]
    #[sampler(2)]
    pub base_noise: Handle<Image>,
    /// Worley detail volume for edge erosion.
    #[texture(3, dimension = "3d")]
    #[sampler(4)]
    pub detail_noise: Handle<Image>,
    /// 2D weather map (coverage, precipitation, type, wind mask).
    #[texture(5)]
    #[sampler(6)]
    pub weather_map: Handle<Image>,
    /// Transparent-pass sort bias. Smaller values draw first, so a high cloud
    /// layer must be more negative than the deck below it.
    pub depth_bias: f32,
}

impl Default for CloudMaterial {
    fn default() -> Self {
        Self {
            params: CloudParams::deck(),
            base_noise: Handle::default(),
            detail_noise: Handle::default(),
            weather_map: Handle::default(),
            depth_bias: 0.0,
        }
    }
}

impl Material for CloudMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/clouds.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        // Premultiplied blending yields `cloud_radiance + background * transmittance`,
        // so terrain and sky behind the cloud layer are correctly extinguished.
        AlphaMode::Premultiplied
    }

    fn depth_bias(&self) -> f32 {
        // Overlapping decks are all camera-centred, so only this bias decides
        // their transparent draw order.
        self.depth_bias
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The flight camera sits inside the cloud shell near the ground and
        // outside it at altitude, so both faces must rasterize; the fragment
        // shader keeps exactly one of them to avoid double-blending.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Cloud altitude for the flight shell, in metres above the reference surface.
/// Solar-map exaggeration (`CloudLayerConfig::scale`) is not a flight altitude.
pub fn flight_cloud_altitude_m(body: &str) -> f32 {
    match body {
        "Venus" => 60_000.0,
        "Titan" => 20_000.0,
        _ => 6_000.0,
    }
}

/// Cloud-base altitude for the flight shell, in metres above the reference
/// surface. The cloud layer is a spherical slab between this and
/// [`flight_cloud_altitude_m`].
pub fn flight_cloud_bottom_altitude_m(body: &str) -> f32 {
    match body {
        "Venus" => 45_000.0,
        "Titan" => 15_000.0,
        _ => 1_500.0,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A reflected deck must dim and redden with the same atmospheric beam
    /// extinction the drawn deck has, so the shadow state has to carry the
    /// material's Sun transmittance, not a default.
    #[test]
    fn cloud_shadow_copies_the_deck_sun_transmittance() {
        let mut params = CloudParams::deck();
        params.sun_transmittance = Vec3::new(0.94, 0.71, 0.52);
        let sun = Vec3::new(0.0, 0.0, 1.0);
        let rotation = Vec4::new(0.0, 0.0, 0.0, 1.0);

        let shadow = CloudShadow::for_deck(&params, sun, rotation);
        assert_eq!(shadow.sun_transmittance, params.sun_transmittance);
        assert_eq!(shadow.sun_direction, sun);
        assert_eq!(shadow.inertial_to_body, rotation);
        assert_eq!(shadow.field.bottom_radius, params.bottom_radius);
        assert_eq!(shadow.field.absorption, params.absorption);
    }

    #[test]
    fn cloud_field_copies_every_density_input() {
        let mut params = CloudParams::deck();
        params.bottom_radius = 6_372_500.0;
        params.top_radius = 6_377_000.0;
        params.coverage = 0.42;
        params.cloud_type = 0.3;
        params.density_scale = 0.7;
        params.noise_scale = 1.0 / 4_500.0;
        params.detail_scale = 2.25;
        params.erosion_scale = 0.45;
        params.wind_speed = 12.5;
        params.wind_direction = Vec2::new(0.6, -0.8);
        params.absorption = 0.012;
        params.time_s = 123.5;

        let field = CloudField::from_cloud_params(&params);
        assert_eq!(field.bottom_radius, params.bottom_radius);
        assert_eq!(field.top_radius, params.top_radius);
        assert_eq!(field.absorption, params.absorption);
        assert_eq!(field.time_s, params.time_s);
        assert_eq!(field.coverage, params.coverage);
        assert_eq!(field.cloud_type, params.cloud_type);
        assert_eq!(field.density_scale, params.density_scale);
        assert_eq!(field.noise_scale, params.noise_scale);
        assert_eq!(field.detail_scale, params.detail_scale);
        assert_eq!(field.erosion_scale, params.erosion_scale);
        assert_eq!(field.wind_speed, params.wind_speed);
        assert_eq!(field.wind_direction, params.wind_direction);
    }

    #[test]
    fn shadow_state_defaults_to_no_deck() {
        let state = CloudShadow::default();
        assert_eq!(state.field.bottom_radius, 0.0);
        assert_eq!(state.sun_direction, Vec3::ZERO);
        assert_eq!(state.inertial_to_body, Vec4::ZERO);
    }
}
