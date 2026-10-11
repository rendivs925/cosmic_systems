// Wind-swayed, alpha-masked vegetation.
//
// Trees, shrubs, and grass are merged into one mesh per terrain patch. The
// second UV channel carries a per-vertex sway weight (0 at the trunk base and
// buttress roots, rising to 1 at the canopy and grass tips) so the vertex stage
// bends the plant with a quadratic profile that keeps the base planted. Wind is
// presentation only: it never feeds collision, altitude, guidance, or any
// authoritative simulation state.

#define_import_path cosmic_systems::vegetation

#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_functions,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    view_transformations,
}

struct VegetationParams {
    /// Presentation clock in seconds; advances the sway phase only.
    time_s: f32,
    /// Peak horizontal sway displacement at sway weight 1, in meters.
    wind_strength: f32,
    /// Temporal frequency of the sway, in radians per second.
    sway_frequency: f32,
    /// Spatial frequency of the sway phase, in radians per metre, so adjacent
    /// plants do not sway in lockstep.
    wind_spatial_frequency: f32,
    /// Horizontal wind direction in the patch-local body-fixed frame.
    wind_direction: vec3<f32>,
    /// Planet centre in the rebased inertial metre frame (the negated render
    /// origin). Subtracting it recovers an origin-invariant body position so the
    /// sway phase stays pinned to the planet when the render origin recentres.
    planet_center: vec3<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> vegetation: VegetationParams;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

#ifdef VERTEX_UVS_B
    let sway_weight = vertex.uv_b.x;
#else
    let sway_weight = 0.0;
#endif

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let base_world = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );
    let body_position = base_world.xyz - vegetation.planet_center;
    let phase = vegetation.time_s * vegetation.sway_frequency
        + dot(body_position, vegetation.wind_direction) * vegetation.wind_spatial_frequency;
    // Quadratic bend keeps the base planted and lets the canopy travel.
    let bend = sway_weight * sway_weight;
    let sway = sin(phase) * vegetation.wind_strength * bend;
    let displaced = vertex.position + vegetation.wind_direction * sway;

    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    );
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(displaced, 1.0),
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
    pbr.material.base_color = alpha_discard(pbr.material, pbr.material.base_color);

    var out: FragmentOutput;
    if (pbr.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr);
    } else {
        out.color = pbr.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
