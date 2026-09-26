#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    lighting,
    lighting::LAYER_BASE,
    mesh_functions,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    mesh_view_bindings::{lights, view},
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{
        alpha_discard, apply_pbr_lighting, calculate_diffuse_color, calculate_F0,
        main_pass_post_lighting_processing,
    },
    pbr_types,
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    shadows,
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
    // Fraction of the baked self-shadow applied to the direct-sun term.
    self_shadow_strength: f32,
    // Fraction of the baked sky occlusion applied to the indirect term.
    sky_occlusion_strength: f32,
    // One while the layered ground material is active for this patch, zero for
    // the single-layer fallback. Uniform per patch, so the layered branch below
    // is uniform control flow.
    layer_blend_weight: f32,
    // Repetitions per metre of a ground-layer texture.
    layer_tiling_scale: f32,
    // Repetitions of a ground-layer texture across the patch-local UV, matched to
    // the world-space triplanar scale so both projections agree physically.
    layer_patch_uv_scale: f32,
    // Gain applied to the blended layer tangent-space normal.
    layer_normal_strength: f32,
    // Repetitions per metre of the near-camera detail overlay.
    near_detail_scale: f32,
    // Gain of the near-camera detail overlay, faded by view distance.
    near_detail_strength: f32,
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
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var terrain_occlusion: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(112) var terrain_occlusion_sampler: sampler;
// Shared per-layer PBR sets. Layer order matches the terrain layer catalog:
// grass, soil, rock, sand, snow. Albedo/roughness packs rgb + roughness, and the
// normal array carries a tangent-space normal in rgb.
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var terrain_layer_albedo: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(114) var terrain_layer_albedo_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(115) var terrain_layer_normal: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(116) var terrain_layer_normal_sampler: sampler;
// Per-patch layer weights: rgb = grass, soil, rock; a = sand. Snow is the
// remaining unit, so one RGBA8 map encodes all five layers.
@group(#{MATERIAL_BIND_GROUP}) @binding(117) var terrain_layer_weights: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(118) var terrain_layer_weights_sampler: sampler;

const LAYER_COUNT: u32 = 5u;
// Overlay fade bands. Each is evaluated per pixel so shared patch edges agree.
const NEAR_DETAIL_FADE_START_M: f32 = 140.0;
const NEAR_DETAIL_FADE_END_M: f32 = 700.0;
// Orientation band over which the patch-local projection blends into the
// world-axis (triplanar) projection on steep faces.
const PROJECTION_BLEND_START: f32 = 0.25;
const PROJECTION_BLEND_END: f32 = 0.6;

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

// Continuous world-axis projection blended with the patch-local UV. The three
// axis-plane coordinates are mixed by surface orientation into one UV, so steep
// faces stop stretching without introducing a projection seam. Blending into the
// patch-local projection keeps the flatter terrain's existing alignment.
fn blended_layer_uv(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    patch_uv: vec2<f32>,
    world_scale: f32,
    patch_scale: f32,
) -> vec2<f32> {
    let axis = abs(normal);
    let axis_sum = max(axis.x + axis.y + axis.z, 1e-4);
    let axis_weights = axis / axis_sum;
    let triplanar = world_position.zy * axis_weights.x
        + world_position.xz * axis_weights.y
        + world_position.xy * axis_weights.z;
    let radial = normalize(world_position + vec3<f32>(1e-6, 1e-6, 1e-6));
    let steep = 1.0 - clamp(abs(dot(normal, radial)), 0.0, 1.0);
    let blend = smoothstep(PROJECTION_BLEND_START, PROJECTION_BLEND_END, steep);
    return mix(patch_uv * patch_scale, triplanar * world_scale, blend);
}

// Blend every ground layer with the same weights for albedo, roughness, and the
// tangent-space normal. `weights` sums to one, so the result is the weighted
// material mix without a hard biome band.
struct LayeredSample {
    albedo: vec3<f32>,
    roughness: f32,
    normal_ts: vec3<f32>,
}

fn sample_layers(uv: vec2<f32>, weights: array<f32, LAYER_COUNT>) -> LayeredSample {
    var result: LayeredSample;
    result.albedo = vec3<f32>(0.0);
    result.roughness = 0.0;
    var normal_accum = vec3<f32>(0.0);
    for (var i = 0u; i < LAYER_COUNT; i = i + 1u) {
        let weight = weights[i];
        let layer = textureSample(
            terrain_layer_albedo,
            terrain_layer_albedo_sampler,
            uv,
            i32(i),
        );
        result.albedo += layer.rgb * weight;
        result.roughness += layer.a * weight;
        let normal = textureSample(
            terrain_layer_normal,
            terrain_layer_normal_sampler,
            uv,
            i32(i),
        );
        normal_accum += (normal.xyz * 2.0 - vec3<f32>(1.0)) * weight;
    }
    result.normal_ts = normal_accum;
    return result;
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

// Return the change needed to scale only the directional-sun contribution by
// `self_shadow`. Bevy's `apply_pbr_lighting` has no per-material hook for a
// single light, so the terrain reconstructs the same directional contribution
// from the same public lighting functions and subtracts the part the baked
// self-shadow hides. Indirect sky/ambient light is left untouched, so shadowed
// slopes keep readable fill instead of going black.
fn direct_sun_self_shadow_correction(
    pbr_input: pbr_types::PbrInput,
    self_shadow: f32,
) -> vec3<f32> {
    if self_shadow >= 1.0 || lights.n_directional_lights == 0u {
        return vec3<f32>(0.0);
    }
    let view_z = dot(
        vec4<f32>(
            view.view_from_world[0].z,
            view.view_from_world[1].z,
            view.view_from_world[2].z,
            view.view_from_world[3].z,
        ),
        pbr_input.world_position,
    );
    let ndotv = max(dot(pbr_input.N, pbr_input.V), 0.0001);
    var lighting_input: lighting::LightingInput;
    lighting_input.layers[LAYER_BASE].NdotV = ndotv;
    lighting_input.layers[LAYER_BASE].N = pbr_input.N;
    lighting_input.layers[LAYER_BASE].R = reflect(-pbr_input.V, pbr_input.N);
    lighting_input.layers[LAYER_BASE].perceptual_roughness =
        pbr_input.material.perceptual_roughness;
    lighting_input.layers[LAYER_BASE].roughness =
        lighting::perceptualRoughnessToRoughness(pbr_input.material.perceptual_roughness);
    lighting_input.P = pbr_input.world_position.xyz;
    lighting_input.V = pbr_input.V;
    lighting_input.diffuse_color = calculate_diffuse_color(
        pbr_input.material.base_color.rgb,
        pbr_input.material.metallic,
        pbr_input.material.specular_transmission,
        pbr_input.material.diffuse_transmission,
    );
    lighting_input.F0_ = calculate_F0(
        pbr_input.material.base_color.rgb,
        pbr_input.material.metallic,
        pbr_input.material.reflectance,
    );
    lighting_input.F_ab = lighting::F_AB(pbr_input.material.perceptual_roughness, ndotv);

    var direct = vec3<f32>(0.0);
    for (var i = 0u; i < lights.n_directional_lights; i = i + 1u) {
        var shadow = 1.0;
        if (pbr_input.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u
            && (lights.directional_lights[i].flags
                & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            shadow = shadows::fetch_directional_shadow(
                i,
                pbr_input.world_position,
                pbr_input.world_normal,
                view_z,
            );
        }
        direct += lighting::directional_light(i, &lighting_input, true) * shadow;
    }
    return view.exposure * (self_shadow - 1.0) * direct;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    // Baked landscape occlusion, sampled with the patch-local UV: red is the
    // self-shadow visibility toward the shared ephemeris Sun, green is the
    // sky/ambient visibility. The terms are interpolated from the per-patch
    // bake rather than re-marched per fragment.
    let occlusion = textureSample(
        terrain_occlusion,
        terrain_occlusion_sampler,
        in.uv_b,
    );
    let self_shadow = clamp(occlusion.r, 0.0, 1.0);
    let sky_occlusion = clamp(occlusion.g, 0.0, 1.0);
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
    // Near-camera detail overlay: higher frequency than the shared micro detail,
    // faded per pixel so shared patch edges stay continuous and it never aliases
    // at distance.
    let near_fade = 1.0 - smoothstep(
        NEAR_DETAIL_FADE_START_M,
        NEAR_DETAIL_FADE_END_M,
        view_distance,
    );
    let near_detail = sample_detail_triplanar(
        in.world_position.xyz,
        pbr_input.N,
        terrain_surface.near_detail_scale,
    );
    let near_normal = near_detail.xy * 2.0 - vec2<f32>(1.0, 1.0);
    let near_albedo =
        1.0 + (near_detail.b - 0.5) * terrain_surface.near_detail_strength * near_fade;
    // Layered ground material. The weight map is neutral and `layer_blend_weight`
    // is zero on coarse patches and the single-layer fallback, so the layered
    // work is skipped there. The branch is uniform per patch.
    var layered_albedo = vec3<f32>(0.0);
    var layered_roughness = 0.0;
    var layered_normal_ts = vec3<f32>(0.0, 0.0, 1.0);
    let layer_fade = terrain_surface.layer_blend_weight * detail_fade;
    if terrain_surface.layer_blend_weight > 0.0 {
        let splat = textureSample(
            terrain_layer_weights,
            terrain_layer_weights_sampler,
            in.uv_b,
        );
        let weights = array<f32, LAYER_COUNT>(
            splat.r,
            splat.g,
            splat.b,
            splat.a,
            max(0.0, 1.0 - (splat.r + splat.g + splat.b + splat.a)),
        );
        let layer_uv = blended_layer_uv(
            in.world_position.xyz,
            pbr_input.N,
            in.uv_b,
            terrain_surface.layer_tiling_scale,
            terrain_surface.layer_patch_uv_scale,
        );
        let layered = sample_layers(layer_uv, weights);
        layered_albedo = layered.albedo;
        layered_roughness = layered.roughness;
        layered_normal_ts = layered.normal_ts;
    }
    // Crevices (low tangent-space normal.z) darken slightly, adding depth a flat
    // albedo cannot carry. Fades with the same distance-based detail weight.
    let texture_crevice = mix(
        1.0,
        0.55 + 0.45 * smoothstep(0.55, 0.95, local_surface.z),
        detail_weight,
    );
    // The baked sky-occlusion term now physically darkens indirect light in
    // concavities, so the older texture-only crevice darkening is faded out
    // where that term is present. Both are continuous, so neighbouring LODs do
    // not step in brightness and coarse patches keep their existing look.
    let crevice = mix(texture_crevice, 1.0, 1.0 - sky_occlusion);
    let detail_albedo = mix(base_albedo, local_albedo.rgb, 0.4 * detail_weight);
    let layered_base = mix(detail_albedo, layered_albedo, layer_fade);
    pbr_input.material.base_color = vec4(
        layered_base
            * (1.0 + macro_variation * MACRO_STRENGTH)
            * crevice
            * micro_albedo
            * near_albedo,
        pbr_input.material.base_color.a,
    );
    var roughness_value =
        mix(pbr_input.material.perceptual_roughness, local_roughness, detail_weight);
    roughness_value = mix(roughness_value, layered_roughness, layer_fade);
    pbr_input.material.perceptual_roughness = clamp(
        roughness_value + (detail.a - 0.5) * 0.3 * micro_weight,
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
        // Blend the layered material normal into the same tangent frame.
        let layered_normal = normalize(
            tangent * (layered_normal_ts.x * terrain_surface.layer_normal_strength)
                + bitangent * (layered_normal_ts.y * terrain_surface.layer_normal_strength)
                + pbr_input.N * max(layered_normal_ts.z, 0.05),
        );
        let with_layers = normalize(mix(mapped_normal, layered_normal, layer_fade));
        // Layer the triplanar micro and near-camera detail normals on top.
        let micro_normal = detail.xy * 2.0 - vec2<f32>(1.0, 1.0);
        let near_strength = terrain_surface.near_detail_strength * near_fade;
        let with_micro = normalize(
            with_layers
                + tangent * (micro_normal.x * 0.6 * micro_weight)
                + bitangent * (micro_normal.y * 0.6 * micro_weight)
                + tangent * (near_normal.x * near_strength)
                + bitangent * (near_normal.y * near_strength),
        );
        pbr_input.N = normalize(mix(pbr_input.N, with_micro, detail_weight));
    }
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );
    // The sky-occlusion term scales only the indirect contribution; the
    // self-shadow term scales only the direct-sun contribution below.
    let self_shadow_term = mix(1.0, self_shadow, terrain_surface.self_shadow_strength);
    let sky_occlusion_term =
        mix(1.0, sky_occlusion, terrain_surface.sky_occlusion_strength);
    pbr_input.diffuse_occlusion = pbr_input.diffuse_occlusion * sky_occlusion_term;
    pbr_input.specular_occlusion = pbr_input.specular_occlusion * sky_occlusion_term;

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        var lit_color = apply_pbr_lighting(pbr_input);
        lit_color = vec4<f32>(
            lit_color.rgb + direct_sun_self_shadow_correction(pbr_input, self_shadow_term),
            lit_color.a,
        );
        out.color = lit_color;
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
