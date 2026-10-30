// Physically based single-scattering planetary sky.
//
// Unlike Bevy's built-in atmosphere, this shader honours the true planet centre
// and the observer's local vertical from the flight reference frame instead of
// assuming world +Y is up. The scattering integral itself lives in
// `sky_radiance.wgsl`, which the sea also evaluates for its mirror reflection:
// one model, one colour.
//
// Output is premultiplied: rgb is the in-scattered radiance in the same
// radiometric scale as the PBR directional light (colour * illuminance), and
// alpha is 1 - transmittance, so the blend over already-drawn stars and terrain
// is sky_radiance + background * transmittance.

#define_import_path cosmic_systems::sky

#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::forward_io::{VertexOutput, FragmentOutput}
#import "shaders/sky_radiance.wgsl"::SkyParams
#import "shaders/sky_radiance.wgsl"::sky_radiance_direction

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sky: SkyParams;

const VIEW_SAMPLES: u32 = 16u;
const SUN_SAMPLES: u32 = 8u;

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    let camera_world = view.world_position;
    let ray_dir = normalize(in.world_position.xyz - camera_world);
    let position = camera_world - sky.planet_center;
    let sample = sky_radiance_direction(
        sky,
        position,
        ray_dir,
        VIEW_SAMPLES,
        SUN_SAMPLES,
    );

    // PBR lighting has already applied camera exposure before writing HDR.
    // Match that scale here too: unexposed solar radiance clips the sky to
    // white and blooms over the terrain when the camera turns toward it.
    let radiance = sample.radiance * sky.sky_strength * view.exposure;
    let opacity = clamp(1.0 - dot(sample.transmittance, vec3(1.0 / 3.0)), 0.0, 1.0);
    out.color = vec4(radiance, opacity);
    return out;
}
