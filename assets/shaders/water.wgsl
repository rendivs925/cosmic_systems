// Physically lit planetary water surface.
//
// Each terrain patch that contains ocean contributes a sea-level sphere cap.
// The red vertex-colour channel carries normalized depth (0 at the shoreline,
// 1 at the deepest sampled seafloor) and drives the shallow/deep colour blend,
// the opacity ramp, and the shoreline foam band. Waves are a presentation-only
// normal perturbation; nothing here feeds collision or any authoritative
// simulation state.

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
    /// Spatial frequency of the two crossed swell trains, in 1/m.
    wave_scale: f32,
    /// Swell normal-perturbation strength.
    wave_strength: f32,
    shallow_color: vec4<f32>,
    deep_color: vec4<f32>,
    opacity_shallow: f32,
    opacity_deep: f32,
    /// Breaking-wave foam colour blended onto the shoreline band.
    foam_color: vec4<f32>,
    /// Depth of the foam band, as the normalized vertex-depth channel value.
    foam_depth_normalized: f32,
    /// Peak foam coverage at the waterline.
    foam_strength: f32,
    /// Spatial frequency of the fine ripple train, in 1/m.
    ripple_scale: f32,
    /// Fine-ripple normal-perturbation strength.
    ripple_strength: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water: WaterParams;

// Crossed wave trains give a cheap, stable, non-tiling ripple gradient. The
// height gradient is converted into a surface normal. `shoal` attenuates the
// perturbation in very shallow water so the waterline does not erupt into
// spiky normals where the seafloor slope is steep.
fn wave_normal(world_position: vec3<f32>, time_s: f32, shoal: f32) -> vec3<f32> {
    let k = water.wave_scale;
    let dir_a = vec2<f32>(k, k * 0.73);
    let dir_b = vec2<f32>(-k * 0.61, k * 0.92);
    let phase_a = dot(world_position.xz, dir_a) + time_s * 1.15;
    let phase_b = dot(world_position.xz, dir_b) + time_s * 1.71;
    let height_a = cos(phase_a);
    let height_b = cos(phase_b);

    // A finer, faster ripple train breaks up the swell so the surface never
    // reads as two clean geometric waves.
    let kr = water.ripple_scale;
    let dir_c = vec2<f32>(kr * 0.42, -kr * 0.9);
    let phase_c = dot(world_position.xz, dir_c) + time_s * 2.6;
    let height_c = cos(phase_c);

    let swell = vec2<f32>(
        dir_a.x * height_a + dir_b.x * height_b,
        dir_a.y * height_a + dir_b.y * height_b,
    ) * water.wave_strength;
    let ripple = vec2<f32>(dir_c.x * height_c, dir_c.y * height_c) * water.ripple_strength;

    let gradient = (swell + ripple) * shoal;
    return normalize(vec3<f32>(-gradient.x, 1.0, -gradient.y));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);

    let depth = clamp(in.color.r, 0.0, 1.0);
    var water_color = mix(water.shallow_color, water.deep_color, depth);
    var opacity = mix(water.opacity_shallow, water.opacity_deep, depth);

    // Shoaling: full ripple amplitude in open water, damped at the shoreline.
    let shoal = smoothstep(0.0, 0.06, depth);
    let ripple = wave_normal(in.world_position.xyz, water.time_s, mix(0.35, 1.0, shoal));
    pbr.N = normalize(mix(pbr.N, ripple, 0.65));

    // Shoreline foam: strongest exactly at the waterline and modulated by a
    // slow travelling pattern so the edge reads as moving surf, not a hard line.
    let foam_band = 1.0 - smoothstep(0.0, max(water.foam_depth_normalized, 1e-5), depth);
    let foam_pattern =
        0.5 + 0.5 * sin(in.world_position.x * 0.35 + water.time_s * 1.3)
            * sin(in.world_position.z * 0.31 - water.time_s * 1.05);
    let foam = clamp(foam_band * water.foam_strength * (0.4 + 0.6 * foam_pattern), 0.0, 1.0);
    water_color = mix(water_color, water.foam_color, foam);

    // Grazing angles reflect more and transmit less, so the surface becomes
    // near-opaque at the horizon and stays clear looking straight down.
    let fresnel = pow(1.0 - clamp(dot(pbr.V, pbr.N), 0.0, 1.0), 5.0);
    opacity = mix(opacity, 1.0, fresnel * 0.75);
    opacity = mix(opacity, 1.0, foam);

    pbr.material.base_color = vec4<f32>(water_color.rgb, water_color.a * opacity);
    // Smooth water reads as a glossy dielectric: low roughness gives a tight
    // sun glint, zero metallic keeps the body colour alive in shadow.
    pbr.material.perceptual_roughness = 0.05;
    pbr.material.metallic = 0.0;

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
