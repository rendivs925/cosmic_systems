#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct TerrainSurfaceExtension {
    local_detail_weight: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var terrain_local_albedo: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var terrain_local_albedo_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var terrain_local_normal: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var terrain_local_normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var<uniform> terrain_surface: TerrainSurfaceExtension;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    // Tile-local data must fade at its boundary so a refined leaf transitions
    // continuously into its coarser macro-imagery parent.
    let edge_distance = min(min(in.uv_b.x, 1.0 - in.uv_b.x), min(in.uv_b.y, 1.0 - in.uv_b.y));
    let edge_fade = smoothstep(0.04, 0.16, edge_distance);
    let detail_weight = terrain_surface.local_detail_weight * edge_fade;
    let local_albedo = textureSample(
        terrain_local_albedo,
        terrain_local_albedo_sampler,
        in.uv_b,
    );
    // Preserve global imagery as the macro albedo. The procedural material is
    // a restrained local modulation, never a full tile-shaped replacement.
    let local_modulation = mix(vec4<f32>(1.0), local_albedo, 0.35);
    pbr_input.material.base_color *= mix(vec4<f32>(1.0), local_modulation, detail_weight);

    let local_normal = textureSample(
        terrain_local_normal,
        terrain_local_normal_sampler,
        in.uv_b,
    ).xyz * 2.0 - vec3<f32>(1.0);
    let local_roughness = textureSample(
        terrain_local_normal,
        terrain_local_normal_sampler,
        in.uv_b,
    ).w;
    pbr_input.material.perceptual_roughness = mix(
        pbr_input.material.perceptual_roughness,
        local_roughness,
        detail_weight,
    );
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
        pbr_input.N = normalize(mix(pbr_input.N, mapped_normal, detail_weight));
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
