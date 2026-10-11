// Wind-matched depth/shadow vertex pass for vegetation.
//
// The shadow map and any depth prepass must see the same swaying geometry as the
// colour pass, otherwise a tree's shadow lags its canopy and reads as detached.
// This entry point applies the identical wind displacement as the colour vertex
// shader and writes the prepass vertex output. The alpha mask is applied by the
// base `StandardMaterial` prepass fragment shader (the extension leaves
// `prepass_fragment_shader` at its default), so leaf and grass cards discard
// their transparent texels and cast dappled shadows instead of opaque
// rectangles. Wind is presentation only; nothing here feeds simulation state.

#define_import_path cosmic_systems::vegetation_prepass

#import bevy_pbr::{
    mesh_functions,
    prepass_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
}

struct VegetationParams {
    time_s: f32,
    wind_strength: f32,
    sway_frequency: f32,
    wind_spatial_frequency: f32,
    wind_direction: vec3<f32>,
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
    // Quadratic bend keeps the base planted and lets the canopy travel. This must
    // stay identical to the colour vertex shader so the shadow matches the mesh.
    let bend = sway_weight * sway_weight;
    let sway = sin(phase) * vegetation.wind_strength * bend;
    let displaced = vertex.position + vegetation.wind_direction * sway;

    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(displaced, 1.0),
    );
    out.position = position_world_to_clip(out.world_position.xyz);

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index,
    );
#endif
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif

    return out;
}
