#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_functions,
    mesh_view_bindings::view,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{
        alpha_discard, apply_pbr_lighting,
        main_pass_post_lighting_processing,
    },
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    view_transformations,
}
#import "shaders/landscape_lighting.wgsl"::direct_sun_self_shadow_correction

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
    // Fractional body-fixed tiling phase of the render origin, one vector per
    // tiling scale. Added after projecting the precise render-relative fragment
    // position so ground detail keeps sub-metre phase without evaluating
    // planet-scale f32 coordinates per fragment.
    detail_anchor: vec3<f32>,
    near_detail_anchor: vec3<f32>,
    layer_anchor: vec3<f32>,
    // Contrast of the height-aware ground-layer blend.
    layer_height_contrast: f32,
    planet_center: vec3<f32>,
    // Rotation from the current inertial render frame into the body-fixed frame
    // (xyzw quaternion). Ground texture fields are evaluated in body-fixed
    // coordinates so they neither slide with planetary rotation nor break phase
    // at patch and cube-face boundaries.
    inertial_to_body: vec4<f32>,
}

/// Rotate `v` by the unit quaternion `q` (xyz = axis*sin, w = cos).
fn rotate_by_quat(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
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
// Shared per-layer micro-height array (red channel), sampled at the same UV as
// the layer albedo/normal arrays for height-aware blending.
@group(#{MATERIAL_BIND_GROUP}) @binding(119) var terrain_layer_height: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(120) var terrain_layer_height_sampler: sampler;

const LAYER_COUNT: u32 = 5u;
// Overlay fade bands. Each is evaluated per pixel so shared patch edges agree.
const NEAR_DETAIL_FADE_START_M: f32 = 140.0;
const NEAR_DETAIL_FADE_END_M: f32 = 700.0;
// Triplanar sample of the shared micro-detail texture. Projecting on the three
// axes avoids both UV stretching on steep faces and cube-sphere seams.
// `local_position` is the render-relative fragment position rotated into the
// body frame (small, precise); `anchor` is the fractional body-fixed phase of
// the render origin at this scale, so the summed coordinate stays continuous
// across patches while retaining sub-metre precision.
fn sample_detail_triplanar(
    local_position: vec3<f32>,
    anchor: vec3<f32>,
    normal: vec3<f32>,
    scale: f32,
) -> vec4<f32> {
    var weights = abs(normal);
    weights = weights / max(weights.x + weights.y + weights.z, 1e-4);
    let x = textureSample(
        terrain_detail,
        terrain_detail_sampler,
        local_position.zy * scale + anchor.zy,
    );
    let y = textureSample(
        terrain_detail,
        terrain_detail_sampler,
        local_position.xz * scale + anchor.xz,
    );
    let z = textureSample(
        terrain_detail,
        terrain_detail_sampler,
        local_position.xy * scale + anchor.xy,
    );
    return x * weights.x + y * weights.y + z * weights.z;
}

// Body-fixed axis-plane (triplanar) projection. The three axis-plane
// coordinates are mixed by surface orientation into one continuous UV. Because
// the input is body-fixed, neighbouring patches and cube faces share one texture
// phase (no seams) and the fields stay pinned to the rotating ground instead of
// sliding with planetary rotation. Patch-local UV cannot provide both.
fn body_layer_uv(
    local_position: vec3<f32>,
    anchor: vec3<f32>,
    normal: vec3<f32>,
    scale: f32,
) -> vec2<f32> {
    let axis = abs(normal);
    let axis_sum = max(axis.x + axis.y + axis.z, 1e-4);
    let axis_weights = axis / axis_sum;
    let projected = local_position.zy * axis_weights.x
        + local_position.xz * axis_weights.y
        + local_position.xy * axis_weights.z;
    let anchored = anchor.zy * axis_weights.x
        + anchor.xz * axis_weights.y
        + anchor.xy * axis_weights.z;
    return projected * scale + anchored;
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

// Height-aware layer weights (Module 4, section 1.1). A layer keeps its weight
// only while its combined coverage + micro-height score stays within `contrast`
// of the locally tallest layer, so gravel/rock erupts out of soil instead of
// cross-fading through it. The result is renormalized and sums to one.
fn height_aware_weights(uv: vec2<f32>, alpha: array<f32, LAYER_COUNT>) -> array<f32, LAYER_COUNT> {
    var modified: array<f32, LAYER_COUNT>;
    var max_value = -1e-6;
    for (var i = 0u; i < LAYER_COUNT; i = i + 1u) {
        let height = textureSample(
            terrain_layer_height,
            terrain_layer_height_sampler,
            uv,
            i32(i),
        ).r;
        modified[i] = alpha[i] + height;
        max_value = max(max_value, modified[i]);
    }
    let threshold = max_value - max(terrain_surface.layer_height_contrast, 0.0);
    var output: array<f32, LAYER_COUNT>;
    var sum = 0.0;
    for (var i = 0u; i < LAYER_COUNT; i = i + 1u) {
        output[i] = max(modified[i] - threshold + 1e-5, 0.0);
        sum += output[i];
    }
    if sum > 1e-9 {
        for (var i = 0u; i < LAYER_COUNT; i = i + 1u) {
            output[i] = output[i] / sum;
        }
    } else {
        for (var i = 0u; i < LAYER_COUNT; i = i + 1u) {
            output[i] = alpha[i];
        }
    }
    return output;
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
    // Baked landscape occlusion, sampled with the patch-local UV: red is the
    // self-shadow visibility toward the shared ephemeris Sun, green is the
    // sky/ambient visibility. The terms are interpolated from the per-patch
    // bake rather than re-marched per fragment.
    let occlusion = textureSample(
        terrain_occlusion,
        terrain_occlusion_sampler,
        in.uv_b,
    );
    let baked_self_shadow = clamp(occlusion.r, 0.0, 1.0);
    let sky_occlusion = clamp(occlusion.g, 0.0, 1.0);
    // Per-pixel fade keeps detail continuous across patch and LOD boundaries.
    let view_distance = distance(in.world_position.xyz, view.world_position);
    // Ground texture fields are evaluated in the body-fixed frame so they are
    // continuous across patches and pinned to the rotating surface. This
    // planet-scale coordinate is only used for the low-frequency macro field,
    // where sub-metre error is invisible.
    let body_position = rotate_by_quat(
        terrain_surface.inertial_to_body,
        in.world_position.xyz - terrain_surface.planet_center,
    );
    // Render-relative position rotated into the body frame: a small, precise
    // vector. The per-scale anchors below restore the absolute tiling phase, so
    // high-frequency ground detail keeps sub-metre precision without evaluating
    // a planet-scale f32 coordinate per fragment.
    let body_local = rotate_by_quat(terrain_surface.inertial_to_body, in.world_position.xyz);
    // The triplanar axis weights must be selected in the same frame as the
    // position they project, so rotate the geometric normal into body-fixed too.
    // Lighting keeps the inertial `pbr_input.N`.
    let body_normal = rotate_by_quat(terrain_surface.inertial_to_body, pbr_input.N);
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
    let macro_variation = value_noise(body_position * MACRO_FREQUENCY) * 2.0 - 1.0;
    // Shared micro-detail carries the surface grain below the imagery/texture
    // resolution. It fades out sooner than the patch maps so it never aliases.
    let micro_fade = 1.0 - smoothstep(300.0, 1500.0, view_distance);
    let micro_weight = detail_fade * micro_fade;
    let detail = sample_detail_triplanar(
        body_local,
        terrain_surface.detail_anchor,
        body_normal,
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
        body_local,
        terrain_surface.near_detail_anchor,
        body_normal,
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
        let layer_uv = body_layer_uv(
            body_local,
            terrain_surface.layer_anchor,
            body_normal,
            terrain_surface.layer_tiling_scale,
        );
        let layered = sample_layers(layer_uv, height_aware_weights(layer_uv, weights));
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
    // The layered material is a unit-luminance tint over the continuous
    // geographic base, not a replacement. A patch that lacks layer maps (coarse
    // LOD, or before its weight map is ready) therefore cannot show a
    // rectangular base-colour step against a neighbour that has them; it only
    // loses the layer grain. `layer_fade` fades with view distance as before.
    let layered_luma = max(dot(layered_albedo, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.02);
    let layered_tint = layered_albedo / layered_luma;
    let layers_modulated = clamp(
        detail_albedo * layered_tint,
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    let layered_base = mix(detail_albedo, layers_modulated, layer_fade);
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
    // Patch-local UVs span the whole patch, so their screen-space derivatives and
    // determinant shrink steeply at grazing angles and distance. An absolute
    // threshold guards only genuinely degenerate (edge-on) quads; lowering it to
    // O(1e-14) admitted near-degenerate frames and produced tiled normal-map
    // artifacts across the terrain, so the conservative threshold stands.
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
    let self_shadow_term = mix(
        1.0,
        baked_self_shadow,
        terrain_surface.self_shadow_strength,
    );
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
