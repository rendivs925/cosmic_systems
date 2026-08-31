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
    let edge_fade = smoothstep(0.02, 0.08, edge_distance);
    let detail_weight = terrain_surface.local_detail_weight * edge_fade;
    let local_albedo = textureSample(
        terrain_local_albedo,
        terrain_local_albedo_sampler,
        in.uv_b,
    );
    // The source-derived local albedo refines the same procedural appearance
    // represented by coarse mesh vertex colors. It is not multiplied against
    // unrelated global imagery, which caused near-field color conflicts.
    pbr_input.material.base_color = mix(
        pbr_input.material.base_color,
        local_albedo,
        detail_weight,
    );

    let local_surface = textureSample(
        terrain_local_normal,
        terrain_local_normal_sampler,
        in.uv_b,
    );
    let local_roughness = local_surface.w;
    pbr_input.material.perceptual_roughness = mix(
        pbr_input.material.perceptual_roughness,
        local_roughness,
        detail_weight,
    );
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
