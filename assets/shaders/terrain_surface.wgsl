#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_functions,
    mesh_view_bindings::view,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    view_transformations,
}

// Source-derived detail fades out with camera distance. Evaluating the fade per
// pixel (not per patch) keeps shared patch edges continuous, so neighbouring
// LODs never step in brightness or normal strength.
const DETAIL_FADE_START_M: f32 = 1500.0;
const DETAIL_FADE_END_M: f32 = 6000.0;
// Low-frequency albedo variation breaks up uniform source colour at scales the
// streamed imagery does not resolve.
const MACRO_FREQUENCY: f32 = 0.0009;
const MACRO_STRENGTH: f32 = 0.09;

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.3183099 + vec3<f32>(0.71, 0.113, 0.419));
    q += dot(q, q.zyx + 19.19);
    return fract((q.x + q.y) * q.z);
}

fn value_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let c000 = hash31(i + vec3<f32>(0.0, 0.0, 0.0));
    let c100 = hash31(i + vec3<f32>(1.0, 0.0, 0.0));
    let c010 = hash31(i + vec3<f32>(0.0, 1.0, 0.0));
    let c110 = hash31(i + vec3<f32>(1.0, 1.0, 0.0));
    let c001 = hash31(i + vec3<f32>(0.0, 0.0, 1.0));
    let c101 = hash31(i + vec3<f32>(1.0, 0.0, 1.0));
    let c011 = hash31(i + vec3<f32>(0.0, 1.0, 1.0));
    let c111 = hash31(i + vec3<f32>(1.0, 1.0, 1.0));
    let x00 = mix(c000, c100, u.x);
    let x10 = mix(c010, c110, u.x);
    let x01 = mix(c001, c101, u.x);
    let x11 = mix(c011, c111, u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

struct TerrainSurfaceExtension {
    local_detail_weight: f32,
    // 0 while only the global overview is available, 1 when a produced local
    // imagery tile has replaced it for this patch.
    imagery_weight: f32,
    // View-distance band over which a patch morphs toward its coarser parent.
    morph_start_m: f32,
    morph_end_m: f32,
    // Repetitions per metre of the shared micro-detail texture.
    detail_scale: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var terrain_local_albedo: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var terrain_local_albedo_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var terrain_local_normal: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var terrain_local_normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var<uniform> terrain_surface: TerrainSurfaceExtension;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var terrain_global_albedo: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var terrain_global_albedo_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var terrain_imagery_albedo: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var terrain_imagery_albedo_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var terrain_detail: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var terrain_detail_sampler: sampler;

// Triplanar sample of the shared micro-detail texture. Projecting on the three
// world axes avoids both UV stretching on steep faces and cube-sphere seams.
fn sample_detail_triplanar(world_position: vec3<f32>, normal: vec3<f32>, scale: f32) -> vec4<f32> {
    var weights = abs(normal);
    weights = weights / max(weights.x + weights.y + weights.z, 1e-4);
    let x = textureSample(terrain_detail, terrain_detail_sampler, world_position.zy * scale);
    let y = textureSample(terrain_detail, terrain_detail_sampler, world_position.xz * scale);
    let z = textureSample(terrain_detail, terrain_detail_sampler, world_position.xy * scale);
    return x * weights.x + y * weights.y + z * weights.z;
}

// Terrain vertex stage. Mirrors Bevy's default mesh vertex path for the
// attributes terrain actually carries, and adds continuous level-of-detail
// morphing: each vertex stores its offset to the coarser parent surface in the
// vertex-colour red channel (meters along the normal). The morph factor is
// derived from the vertex's own view distance, so shared edges between
// neighbouring patches agree and refinement never pops or cracks.
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var local_position = vertex.position;

#ifdef VERTEX_COLORS
#ifdef VERTEX_NORMALS
    let world_before = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(local_position, 1.0),
    );
    let view_distance = distance(world_before.xyz, view.world_position);
    let morph_factor = clamp(
        (view_distance - terrain_surface.morph_start_m)
            / max(terrain_surface.morph_end_m - terrain_surface.morph_start_m, 1.0),
        0.0,
        1.0,
    );
    local_position += vertex.normal * (vertex.color.r * morph_factor);
#endif
#endif

#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    );
#endif
#ifdef VERTEX_POSITIONS
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(local_position, 1.0),
    );
    out.position = view_transformations::position_world_to_clip(out.world_position.xyz);
#endif
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
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    // Per-pixel fade keeps detail continuous across patch and LOD boundaries.
    let view_distance = distance(in.world_position.xyz, view.world_position);
    let detail_fade = 1.0 - smoothstep(
        DETAIL_FADE_START_M,
        DETAIL_FADE_END_M,
        view_distance,
    );
    let detail_weight = terrain_surface.local_detail_weight * detail_fade;
    let local_albedo = textureSample(
        terrain_local_albedo,
        terrain_local_albedo_sampler,
        in.uv_b,
    );
    // UV0 is the continuous body-fixed equirectangular coordinate. Preserve the
    // catalog Earth image as the geographic base; UV1 only adds local character.
    let global_albedo = textureSample(
        terrain_global_albedo,
        terrain_global_albedo_sampler,
        in.uv,
    );
    // Detailed local imagery is a produced cube-sphere tile sampled with the
    // patch-local UV1. It replaces the global overview only once it is ready;
    // until then `imagery_weight` is zero and the overview remains visible.
    let imagery_albedo = textureSample(
        terrain_imagery_albedo,
        terrain_imagery_albedo_sampler,
        in.uv_b,
    );
    let base_albedo = mix(
        global_albedo.rgb,
        imagery_albedo.rgb,
        terrain_surface.imagery_weight,
    );
    let local_surface = textureSample(
        terrain_local_normal,
        terrain_local_normal_sampler,
        in.uv_b,
    );
    let local_normal = local_surface.xyz * 2.0 - vec3<f32>(1.0);
    let local_roughness = local_surface.w;
    // Macro albedo variation at a scale the streamed imagery does not resolve.
    let macro_variation = value_noise(in.world_position.xyz * MACRO_FREQUENCY) * 2.0 - 1.0;
    // Shared micro-detail carries the surface grain below the imagery/texture
    // resolution. It fades out sooner than the patch maps so it never aliases.
    let micro_fade = 1.0 - smoothstep(300.0, 1500.0, view_distance);
    let micro_weight = detail_fade * micro_fade;
    let detail = sample_detail_triplanar(
        in.world_position.xyz,
        pbr_input.N,
        terrain_surface.detail_scale,
    );
    let micro_albedo = 1.0 + (detail.b - 0.5) * 0.4 * micro_weight;
    // Crevices (low tangent-space normal.z) darken slightly, adding depth a flat
    // albedo cannot carry. Fades with the same distance-based detail weight.
    let crevice = mix(
        1.0,
        0.55 + 0.45 * smoothstep(0.55, 0.95, local_surface.z),
        detail_weight,
    );
    let detail_albedo = mix(base_albedo, local_albedo.rgb, 0.4 * detail_weight);
    pbr_input.material.base_color = vec4(
        detail_albedo * (1.0 + macro_variation * MACRO_STRENGTH) * crevice * micro_albedo,
        pbr_input.material.base_color.a,
    );
    pbr_input.material.perceptual_roughness = clamp(
        mix(pbr_input.material.perceptual_roughness, local_roughness, detail_weight)
            + (detail.a - 0.5) * 0.3 * micro_weight,
        0.04,
        1.0,
    );
    // Reconstruct the local tangent frame from the patch UVs. The normal map
    // contains source detail absent from the streamed mesh, so it must affect
    // lighting rather than only roughness.
    let position_dx = dpdx(in.world_position.xyz);
    let position_dy = dpdy(in.world_position.xyz);
    let uv_dx = dpdx(in.uv_b);
    let uv_dy = dpdy(in.uv_b);
    let determinant = uv_dx.x * uv_dy.y - uv_dx.y * uv_dy.x;
    if abs(determinant) > 1e-6 {
        let raw_tangent = (position_dx * uv_dy.y - position_dy * uv_dx.y) / determinant;
        let tangent = normalize(raw_tangent - pbr_input.N * dot(pbr_input.N, raw_tangent));
        let raw_bitangent = (-position_dx * uv_dy.x + position_dy * uv_dx.x) / determinant;
        let handedness = select(-1.0, 1.0, dot(cross(pbr_input.N, tangent), raw_bitangent) >= 0.0);
        let bitangent = cross(pbr_input.N, tangent) * handedness;
        let mapped_normal = normalize(
            tangent * local_normal.x + bitangent * local_normal.y + pbr_input.N * local_normal.z,
        );
        // Layer the triplanar micro-detail normal on top of the patch normal.
        let micro_normal = detail.xy * 2.0 - vec2<f32>(1.0, 1.0);
        let with_micro = normalize(
            mapped_normal
                + tangent * (micro_normal.x * 0.6 * micro_weight)
                + bitangent * (micro_normal.y * 0.6 * micro_weight),
        );
        pbr_input.N = normalize(mix(pbr_input.N, with_micro, detail_weight));
    }
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
