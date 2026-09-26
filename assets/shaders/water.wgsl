// Physically lit planetary water surface with a bounded Gerstner wave sum.
//
// Each terrain patch that contains ocean contributes a sea-level sphere cap.
// The red vertex-colour channel carries normalized depth (0 at the shoreline,
// 1 at the deepest sampled seafloor) and drives the shallow/deep colour blend,
// the opacity ramp, and the shoreline foam band. The vertex stage displaces the
// cap radially by a sum of Gerstner waves; the fragment stage reconstructs the
// analytic wave normal and applies Beer-Lambert depth absorption plus a crest
// subsurface term. Waves are presentation-only and never feed collision or any
// authoritative simulation state.

#define_import_path cosmic_systems::water

#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_functions,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    view_transformations,
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
    /// Peak vertical displacement of the primary swell, in meters.
    wave_height_m: f32,
    /// Beer-Lambert absorption exponent over the normalized visible depth.
    absorption: f32,
    /// Strength of the crest subsurface-scattering tint.
    sss_strength: f32,
    /// Colour scattered through wave crests.
    sss_color: vec4<f32>,
    /// Fraction of the baked terrain self-shadow applied to water shading.
    landscape_shadow_strength: f32,
    /// Number of active Gerstner wave components (1..=4).
    wave_components: f32,
    /// Global foam coverage multiplier.
    foam_coverage: f32,
    /// Phase speed of the flow-directed river ripple, in radians per second.
    /// Zero for the ocean; rivers animate along their per-vertex flow direction.
    flow_speed: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water: WaterParams;
// Baked terrain occlusion sampled with the patch-local UV1: red is the
// self-shadow visibility toward the sun. Neutral (1, 1) when unbound.
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var terrain_occlusion: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var terrain_occlusion_sampler: sampler;

const GRAVITY: f32 = 9.81;
const TAU: f32 = 6.283185307;

/// One wave component's contribution in the local tangent plane:
/// `(height_m, d(height)/d(tangent_x), d(height)/d(tangent_y))`.
fn wave_term(p: vec2<f32>, dir: vec2<f32>, wavelength_m: f32, amplitude_m: f32, time_s: f32) -> vec3<f32> {
    let k = TAU / max(wavelength_m, 1e-3);
    // Deep-water dispersion keeps long swells slow and short chop fast.
    let omega = sqrt(GRAVITY * k);
    let phase = k * dot(dir, p) - omega * time_s;
    let height = amplitude_m * sin(phase);
    let slope = dir * (amplitude_m * k * cos(phase));
    return vec3<f32>(height, slope.x, slope.y);
}

struct WaveSample {
    height: f32,
    grad: vec2<f32>,
    tangent: vec3<f32>,
    bitangent: vec3<f32>,
}

/// Sum of a bounded set of Gerstner-style directional waves evaluated in the
/// surface tangent plane. Amplitude derives from the configured swell height so
/// the same field drives both the vertex displacement and the fragment normal.
fn gerstner_sample(world_position: vec3<f32>, normal: vec3<f32>, time_s: f32) -> WaveSample {
    let reference = select(
        vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(0.0, 0.0, 1.0),
        abs(normal.y) > 0.9,
    );
    let tangent = normalize(cross(normal, reference));
    let bitangent = cross(normal, tangent);
    let p = vec2<f32>(dot(world_position, tangent), dot(world_position, bitangent));

    let swell_wavelength = 1.0 / max(water.wave_scale, 1e-4);
    let chop_wavelength = 1.0 / max(water.ripple_scale, 1e-4);
    let amplitude = water.wave_height_m;

    // Quality budget: each component is gated by the configured wave count so a
    // lower tier reduces work without changing the shader path.
    let active0 = select(0.0, 1.0, water.wave_components > 0.5);
    let active1 = select(0.0, 1.0, water.wave_components > 1.5);
    let active2 = select(0.0, 1.0, water.wave_components > 2.5);
    let active3 = select(0.0, 1.0, water.wave_components > 3.5);
    let term = wave_term(p, vec2<f32>(0.86, 0.51), swell_wavelength, amplitude, time_s) * active0
        + wave_term(p, vec2<f32>(-0.51, 0.86), swell_wavelength * 0.62, amplitude * 0.5, time_s) * active1
        + wave_term(p, vec2<f32>(0.21, -0.98), chop_wavelength, amplitude * 0.12, time_s) * active2
        + wave_term(p, vec2<f32>(-0.91, -0.42), chop_wavelength * 1.6, amplitude * 0.08, time_s) * active3;

    return WaveSample(term.x, vec2<f32>(term.y, term.z), tangent, bitangent);
}

/// Terrain vertex stage. Mirrors Bevy's default mesh vertex path for the
/// attributes water carries, and adds the Gerstner radial displacement so the
/// sea surface moves instead of reading as a static glossy cap. Shared patch
/// and tile boundaries stay continuous because the wave field is a function of
/// the body-fixed surface position and the presentation clock only.
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    var local_position = vertex.position;

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let world_normal = normalize(
        mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index).xyz,
    );
    let world_before = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(local_position, 1.0),
    )
    .xyz;
    let wave = gerstner_sample(world_before, world_normal, water.time_s);
    local_position += vertex.normal * wave.height;

    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    );
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(local_position, 1.0),
    );
    out.position = view_transformations::position_world_to_clip(out.world_position.xyz);

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif

    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);

    let depth = clamp(in.color.r, 0.0, 1.0);
    var water_color = mix(water.shallow_color, water.deep_color, depth);
    var opacity = mix(water.opacity_shallow, water.opacity_deep, depth);

    // Shoaling: full wave amplitude in open water, damped at the shoreline.
    let shoal = smoothstep(0.0, 0.06, depth);
    let wave = gerstner_sample(in.world_position.xyz, pbr.N, water.time_s);
    // Analytic gradient of the wave field, projected into the tangent plane.
    let gradient = wave.grad * shoal * water.wave_strength;
    // Flow-directed ripple: rivers carry a normalized flow direction in the
    // vertex-colour green/blue channels (the ocean leaves them neutral). Rebuild
    // the surface east/north frame and advect a travelling ripple along the flow
    // so the channel reads as moving water rather than a still sheet.
    let flow_coded = vec2<f32>(in.color.g, in.color.b) * 2.0 - 1.0;
    let radial_east = normalize(
        cross(vec3<f32>(0.0, 1.0, 0.0), pbr.N) + vec3<f32>(1e-5, 0.0, 1e-5),
    );
    let radial_north = normalize(cross(pbr.N, radial_east));
    let flow_world = radial_east * flow_coded.x + radial_north * flow_coded.y;
    let flow_t = vec2<f32>(
        dot(flow_world, wave.tangent),
        dot(flow_world, wave.bitangent),
    );
    let flow_position = vec2<f32>(
        dot(in.world_position.xyz, wave.tangent),
        dot(in.world_position.xyz, wave.bitangent),
    );
    let flow_phase =
        dot(flow_t, flow_position) * water.ripple_scale * 4.0 - water.time_s * water.flow_speed;
    let flow_slope = cos(flow_phase) * water.flow_speed * 0.08;
    let wave_normal = normalize(
        pbr.N
            - wave.tangent * (gradient.x + flow_t.x * flow_slope)
            - wave.bitangent * (gradient.y + flow_t.y * flow_slope),
    );
    pbr.N = normalize(mix(pbr.N, wave_normal, 0.85));

    // Beer-Lambert absorption: deep water extinguishes transmitted light and
    // becomes opaque smoothly instead of relying on a linear opacity ramp.
    let transmittance = exp(-max(water.absorption, 0.0) * depth);
    opacity = max(opacity, (1.0 - transmittance) * water.opacity_deep);

    // Crest subsurface scattering: the highest wave samples glow with the
    // scattering colour, reading as light transmitted through the swell.
    let crest = clamp(wave.height / max(water.wave_height_m, 1e-3), 0.0, 1.0);
    water_color = mix(water_color, water.sss_color, water.sss_strength * crest);

    // Landscape self-shadow: terrain beyond the directional-shadow cascades
    // still darkens the sea through the patch's baked terrain occlusion map.
    var landscape_visibility = 1.0;
#ifdef VERTEX_UVS_B
    let landscape = textureSample(
        terrain_occlusion,
        terrain_occlusion_sampler,
        in.uv_b,
    )
    .r;
    landscape_visibility = mix(1.0, landscape, water.landscape_shadow_strength);
#endif
    water_color = vec4<f32>(
        water_color.rgb * mix(0.45, 1.0, landscape_visibility),
        water_color.a,
    );

    // Shoreline foam: strongest exactly at the waterline and modulated by a
    // slow travelling pattern so the edge reads as moving surf, not a hard line.
    let foam_band = 1.0 - smoothstep(0.0, max(water.foam_depth_normalized, 1e-5), depth);
    let foam_pattern =
        0.5 + 0.5 * sin(in.world_position.x * 0.35 + water.time_s * 1.3)
            * sin(in.world_position.z * 0.31 - water.time_s * 1.05);
    // Expose wave crests as whitecaps in open water as the swell steepens.
    let whitecap = smoothstep(0.75, 1.0, wave.height / max(water.wave_height_m, 1e-3))
        * water.foam_strength;
    let foam = clamp(
        foam_band * water.foam_strength * (0.4 + 0.6 * foam_pattern) + whitecap,
        0.0,
        1.0,
    ) * clamp(water.foam_coverage, 0.0, 1.0);
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
