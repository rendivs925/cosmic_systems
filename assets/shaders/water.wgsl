// Physically lit planetary water surface.
//
// Each terrain patch that contains ocean contributes a sea-level sphere cap.
// The red vertex-colour channel carries normalized depth (0 at the shoreline,
// 1 at the deepest sampled seafloor) and drives the shallow/deep colour blend
// and the opacity ramp. Waves are a presentation-only normal perturbation;
// nothing here feeds collision or any authoritative simulation state.

#define_import_path cosmic_systems::water

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct WaterParams {
    /// Presentation clock in seconds; advances the wave phase only.
    time_s: f32,
    /// Spatial frequency of the two crossed wave trains, in 1/m.
    wave_scale: f32,
    /// Normal-perturbation strength.
    wave_strength: f32,
    shallow_color: vec4<f32>,
    deep_color: vec4<f32>,
    opacity_shallow: f32,
    opacity_deep: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water: WaterParams;

// Two crossed cosine wave trains give a cheap, stable, non-tiling ripple
// gradient. The height gradient is converted into a surface normal.
fn wave_normal(world_position: vec3<f32>, time_s: f32) -> vec3<f32> {
    let k = water.wave_scale;
    let dir_a = vec2<f32>(k, k * 0.73);
    let dir_b = vec2<f32>(-k * 0.61, k * 0.92);
    let phase_a = dot(world_position.xz, dir_a) + time_s * 1.15;
    let phase_b = dot(world_position.xz, dir_b) + time_s * 1.71;
    let height_a = cos(phase_a);
    let height_b = cos(phase_b);
    let gradient = vec2<f32>(
        dir_a.x * height_a + dir_b.x * height_b,
        dir_a.y * height_a + dir_b.y * height_b,
    ) * water.wave_strength;
    return normalize(vec3<f32>(-gradient.x, 1.0, -gradient.y));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);

    let depth = clamp(in.color.r, 0.0, 1.0);
    let water_color = mix(water.shallow_color, water.deep_color, depth);
    let opacity = mix(water.opacity_shallow, water.opacity_deep, depth);
    pbr.material.base_color = vec4<f32>(water_color.rgb, water_color.a * opacity);
    // Smooth water reads as a glossy dielectric: low roughness gives a tight
    // sun glint, zero metallic keeps the body colour alive in shadow.
    pbr.material.perceptual_roughness = 0.05;
    pbr.material.metallic = 0.0;

    let ripple = wave_normal(in.world_position.xyz, water.time_s);
    pbr.N = normalize(mix(pbr.N, ripple, 0.65));

    pbr.material.base_color =
        alpha_discard(pbr.material, pbr.material.base_color);

    var out: FragmentOutput;
    if (pbr.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr);
    } else {
        out.color = pbr.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
