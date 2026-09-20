//! Physically based rocket-mode sky.
//!
//! The sky is a camera-anchored dome shaded by a single-scattering atmosphere
//! that honours the true planet centre and the observer's local vertical from
//! the flight reference frame. It replaces the flat clear-colour sky and is
//! driven by the same ephemeris Sun and atmospheric optics as the rest of the
//! presentation. Nothing here is simulation authority.

use super::environment::{bound_planet_sun, solar_illuminance_lux};
use super::planet::RocketBoundPlanet;
use crate::domain::value_objects::atmospheric_optics::AtmosphericOptics;
use crate::infrastructure::bevy_adapters::entity_components::PlanetComponent;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::terrain::render::RenderOrigin;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

const SKY_SHADER: &str = "shaders/sky.wgsl";
/// The dome sits just inside the camera far plane so it is always drawn behind
/// scene geometry while still being depth-tested against it.
const SKY_DOME_FAR_FRACTION: f32 = 0.9;
/// Transparent sorting uses view-space depth, where larger means nearer. The
/// dome is centred on the camera, so it must be biased far behind every other
/// transparent surface to be drawn first as the background.
const SKY_SORT_DEPTH_BIAS: f32 = -1.0e6;
/// Calibration between the radiometric single-scattering integral and the
/// camera's exposure. The integral is linear in the solar irradiance; this
/// factor places a clear daytime sky at a natural fraction of a sunlit surface
/// so it does not clip to white and lose its Rayleigh colour.
const SKY_RADIANCE_SCALE: f32 = 0.03;

/// Uniform parameters for the sky shader. Field order and types must match the
/// `SkyParams` WGSL struct exactly.
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct SkyParams {
    pub planet_center: Vec3,
    pub bottom_radius: f32,
    pub top_radius: f32,
    pub rayleigh_scale_height: f32,
    pub mie_scale_height: f32,
    pub mie_asymmetry: f32,
    pub rayleigh_scattering: Vec3,
    pub mie_scattering: f32,
    pub mie_absorption: f32,
    pub ozone_layer_altitude: f32,
    pub ozone_layer_width: f32,
    pub ozone_absorption: Vec3,
    pub sun_direction: Vec3,
    pub sun_irradiance: Vec3,
    pub ground_albedo: Vec3,
    pub sky_strength: f32,
}

impl SkyParams {
    /// Build the shader parameters for a body, or a transparent vacuum sky when
    /// the body has no modelled atmosphere.
    fn for_optics(
        optics: Option<AtmosphericOptics>,
        planet_center: Vec3,
        sun_direction: Vec3,
        sun_irradiance: Vec3,
    ) -> Self {
        let Some(optics) = optics else {
            return Self {
                planet_center,
                sun_direction,
                sun_irradiance,
                ..default()
            };
        };
        Self {
            planet_center,
            bottom_radius: optics.bottom_radius_m,
            top_radius: optics.top_radius_m,
            rayleigh_scale_height: optics.rayleigh_scale_height_m,
            mie_scale_height: optics.mie_scale_height_m,
            mie_asymmetry: optics.mie_asymmetry,
            rayleigh_scattering: Vec3::from(optics.rayleigh_scattering_per_m),
            mie_scattering: optics.mie_scattering_per_m,
            mie_absorption: optics.mie_absorption_per_m,
            ozone_layer_altitude: optics.ozone_layer_altitude_m,
            ozone_layer_width: optics.ozone_layer_width_m,
            ozone_absorption: Vec3::from(optics.ozone_absorption_per_m),
            sun_direction,
            sun_irradiance,
            ground_albedo: Vec3::from(optics.ground_albedo),
            sky_strength: SKY_RADIANCE_SCALE,
        }
    }
}

/// Physically based single-scattering sky material.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct SkyMaterial {
    #[uniform(0)]
    pub params: SkyParams,
}

impl Material for SkyMaterial {
    fn fragment_shader() -> ShaderRef {
        SKY_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        // Premultiplied blending yields `sky_radiance + background * transmittance`,
        // so stars and terrain behind the sky are correctly extinguished.
        AlphaMode::Premultiplied
    }

    fn depth_bias(&self) -> f32 {
        SKY_SORT_DEPTH_BIAS
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The camera is inside the dome, so both faces must be rasterizable.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Marks the single sky dome entity.
#[derive(Component, Debug)]
pub struct RocketSkyDome;

/// Spawn the camera-anchored sky dome with a unit sphere scaled per frame to the
/// active camera far plane.
pub fn spawn_rocket_sky_dome(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(1.0))),
        MeshMaterial3d(materials.add(SkyMaterial::default())),
        Transform::default(),
        RocketSkyDome,
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
        Name::new("RocketSkyDome"),
    ));
}

/// Update the sky from the authoritative ephemeris Sun, the bound planet's
/// catalog radius, and the shared render origin. The atmosphere optics come
/// from the single domain source for the bound body.
#[expect(
    clippy::type_complexity,
    reason = "The query selects the active camera projection and the disjoint sky dome."
)]
pub fn update_rocket_sky(
    render_origin: Res<RenderOrigin>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
    planet_query: Query<&PlanetComponent>,
    camera_query: Query<
        (&Camera, &Transform, &Projection),
        (With<Camera3d>, Without<RocketSkyDome>),
    >,
    mut dome_query: Query<(&mut Transform, &MeshMaterial3d<SkyMaterial>), With<RocketSkyDome>>,
    mut sky_materials: ResMut<Assets<SkyMaterial>>,
) {
    let Some((_, camera_transform, projection)) =
        camera_query.iter().find(|(camera, _, _)| camera.is_active)
    else {
        return;
    };
    let Some(bound_planet_id) = bound_planet.0.as_ref() else {
        return;
    };
    let Some(planet) = planet_query
        .iter()
        .find(|planet| planet.matches_body(bound_planet_id))
    else {
        return;
    };
    let Some(sun) = bound_planet_sun(&ephemeris_snapshot, bound_planet_id) else {
        return;
    };

    let surface_radius_m = planet.domain_planet.radius_km * 1_000.0;
    let optics = AtmosphericOptics::for_body(bound_planet_id.as_str(), surface_radius_m);
    let planet_center = (-render_origin.origin).as_vec3();
    let sun_direction = sun.direction.as_vec3();
    let sun_color = LinearRgba::from(Color::srgb(1.0, 1.0, 0.98));
    let sun_irradiance = Vec3::new(sun_color.red, sun_color.green, sun_color.blue)
        * solar_illuminance_lux(sun.distance_m);
    let params = SkyParams::for_optics(optics, planet_center, sun_direction, sun_irradiance);

    let dome_radius = match projection {
        Projection::Perspective(perspective) => (perspective.far * SKY_DOME_FAR_FRACTION).max(1.0),
        _ => 1.0,
    };

    for (mut transform, material_handle) in &mut dome_query {
        transform.translation = camera_transform.translation;
        transform.scale = Vec3::splat(dome_radius);
        if let Some(material) = sky_materials.get_mut(&material_handle.0) {
            material.params = params;
        }
    }
}

/// Register the sky material pipeline. Called from rocket-mode composition.
pub fn sky_material_plugin() -> MaterialPlugin<SkyMaterial> {
    MaterialPlugin::<SkyMaterial>::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vacuum_bodies_produce_a_transparent_sky() {
        let params = SkyParams::for_optics(None, Vec3::ZERO, Vec3::X, Vec3::splat(127_000.0));

        assert_eq!(params.top_radius, 0.0);
        assert_eq!(params.sky_strength, 0.0);
        assert_eq!(params.rayleigh_scattering, Vec3::ZERO);
    }

    #[test]
    fn earth_optics_flow_into_the_uniform() {
        let optics = AtmosphericOptics::earth(6_371_000.0);
        let params = SkyParams::for_optics(
            Some(optics),
            Vec3::new(0.0, -6_371_000.0, 0.0),
            Vec3::X,
            Vec3::splat(127_000.0),
        );

        assert_eq!(params.bottom_radius, optics.bottom_radius_m);
        assert_eq!(params.top_radius, optics.top_radius_m);
        assert!(params.rayleigh_scattering.z > params.rayleigh_scattering.x);
        assert_eq!(params.sky_strength, SKY_RADIANCE_SCALE);
    }
}
