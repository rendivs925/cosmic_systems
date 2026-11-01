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
// The ground evaluates the volumetric deck's own density field for cloud
// shadows, so a shadow always lines up with the cloud casting it.
#import "shaders/cloud_density.wgsl"::CloudShadow
#import "shaders/cloud_density.wgsl"::cloud_shadow_visibility
#import "shaders/cloud_density.wgsl"::rotate_by_quat

// Source-derived detail fades out with camera distance. Evaluating the fade per
// pixel (not per patch) keeps shared patch edges continuous, so neighbouring
// LODs never step in brightness or normal strength.
const DETAIL_FADE_START_M: f32 = 1500.0;
const DETAIL_FADE_END_M: f32 = 6000.0;
// Low-frequency albedo variation breaks up uniform source colour at scales the
// streamed imagery does not resolve.
const MACRO_FREQUENCY: f32 = 0.0009;
const MACRO_STRENGTH: f32 = 0.09;
// Mid-scale patchiness at 6-17 m: the band the streamed imagery cannot resolve
// and the sub-metre ground detail averages away beyond a few metres, so close
// ground otherwise reads as a flat wash of colour.
const MESO_FREQUENCY: f32 = 0.06;
const MESO_STRENGTH: f32 = 0.22;
const MESO_FINE_FREQUENCY: f32 = 0.17;
const MESO_FINE_STRENGTH: f32 = 0.09;
// The 6-17 m band is a few pixels wide beyond a few kilometres, so fade it out
// before it can shimmer at range. The streamed imagery still carries variation
// there.
const MESO_FADE_START_M: f32 = 3000.0;
const MESO_FADE_END_M: f32 = 10000.0;

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
    // Fractional body-fixed phase of the render origin at the mid-scale albedo
    // frequency, so that field stays pinned and continuous across patches.
    meso_anchor: vec3<f32>,
    layer_anchor: vec3<f32>,
    // Contrast of the height-aware ground-layer blend.
    layer_height_contrast: f32,
    planet_center: vec3<f32>,
    // Rotation from the current inertial render frame into the body-fixed frame
    // (xyzw quaternion). Ground texture fields are evaluated in body-fixed
    // coordinates so they neither slide with planetary rotation nor break phase
    // at patch and cube-face boundaries.
    inertial_to_body: vec4<f32>,
    // Low cloud deck's density field, Sun, and body rotation, for the
    // cloud-shadow march. A zero bottom radius means no deck.
    cloud: CloudShadow,
    // Developer diagnostic mode. Zero is production rendering; any other value
    // makes the fragment return a false colour of exactly one input. Uniform
    // per patch, so the diagnostic branch is uniform control flow and the
    // production path costs one comparison.
    diagnostic_mode: u32,
    // This patch's LOD level, for the level view and for reading a capture.
    diagnostic_patch_level: u32,
    // Stable hash of the patch's face, level and tile coordinates.
    diagnostic_patch_identity: u32,
    // Bitfield of the per-patch material inputs that exist. Bits are defined by
    // `PatchInputFlags` in terrain/render.rs and repeated in the shader below.
    diagnostic_patch_flags: u32,
    // Radius of the sea-level datum at the patch centre, in metres from the
    // planet centre. Used by the height and land/sea views only.
    diagnostic_datum_radius_m: f32,
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
// grass, soil, rock, sand, snow, gravel, mud. Albedo/roughness packs rgb +
// roughness, and the normal array carries a tangent-space normal in rgb.
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var terrain_layer_albedo: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(114) var terrain_layer_albedo_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(115) var terrain_layer_normal: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(116) var terrain_layer_normal_sampler: sampler;
// Per-patch layer weights as a two-layer RGBA8 array. Array layer 0 carries
// [grass, soil, rock, sand]; array layer 1 carries [snow, gravel, mud, unused].
@group(#{MATERIAL_BIND_GROUP}) @binding(117) var terrain_layer_weights: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(118) var terrain_layer_weights_sampler: sampler;
// Shared per-layer micro-height array (red channel), sampled at the same UV as
// the layer albedo/normal arrays for height-aware blending.
@group(#{MATERIAL_BIND_GROUP}) @binding(119) var terrain_layer_height: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(120) var terrain_layer_height_sampler: sampler;
// The volumetric cloud deck's own noise volumes, so the shadow optical depth is
// evaluated from the field the deck is drawn from rather than a second one.
@group(#{MATERIAL_BIND_GROUP}) @binding(121) var terrain_cloud_base_noise: texture_3d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(122) var terrain_cloud_base_noise_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(123) var terrain_cloud_detail_noise: texture_3d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(124) var terrain_cloud_detail_noise_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(125) var terrain_cloud_weather_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(126) var terrain_cloud_weather_map_sampler: sampler;

const LAYER_COUNT: u32 = 7u;
// Overlay fade bands. Each is evaluated per pixel so shared patch edges agree.
const NEAR_DETAIL_FADE_START_M: f32 = 140.0;
const NEAR_DETAIL_FADE_END_M: f32 = 700.0;
// Triplanar sample of the shared micro-detail texture. Projecting on the three
// axes avoids both UV stretching on steep faces and cube-sphere seams.
// `local_position` is the render-relative fragment position rotated into the
// body frame (small, precise); `anchor` is the fractional body-fixed phase of
// the render origin at this scale, so the summed coordinate stays continuous
// across patches while retaining sub-metre precision. `offset` is an optional
// body-frame parallax displacement in metres (zero for the micro layer).
fn sample_detail_triplanar(
    local_position: vec3<f32>,
    anchor: vec3<f32>,
    normal: vec3<f32>,
    scale: f32,
    offset: vec3<f32>,
) -> vec4<f32> {
    var weights = abs(normal);
    weights = weights / max(weights.x + weights.y + weights.z, 1e-4);
    let x = textureSample(
        terrain_detail,
        terrain_detail_sampler,
        local_position.zy * scale + anchor.zy + offset.zy * scale,
    );
    let y = textureSample(
        terrain_detail,
        terrain_detail_sampler,
        local_position.xz * scale + anchor.xz + offset.xz * scale,
    );
    let z = textureSample(
        terrain_detail,
        terrain_detail_sampler,
        local_position.xy * scale + anchor.xy + offset.xy * scale,
    );
    return x * weights.x + y * weights.y + z * weights.z;
}

// Parallax occlusion mapping for the near-camera detail (Module 4, section 1.2).
// The shared detail texture stores its micro-height in the blue channel
// (`0.5 + 0.5 * height` by construction), so no extra texture is needed. The
// march uses an explicit LOD sample because it runs in per-fragment (non-uniform)
// control flow, where implicit-derivative `textureSample` is not permitted.
const POM_STEPS: i32 = 8;
/// Displacement of the tallest detail texel as a fraction of one detail tile.
const POM_HEIGHT_TILES: f32 = 0.06;
/// View distance over which parallax fades out; beyond it the ground is too far
/// for the displacement to be visible and the extra samples are not worth it.
const POM_FADE_START_M: f32 = 160.0;
const POM_FADE_END_M: f32 = 320.0;

fn detail_height_at(uv: vec2<f32>) -> f32 {
    return textureSampleLevel(terrain_detail, terrain_detail_sampler, uv, 0.0).b;
}

/// March one triplanar plane toward the eye and return the 2D UV displacement.
fn pom_plane_offset(uv: vec2<f32>, view_tan: vec3<f32>) -> vec2<f32> {
    // Clamp the grazing denominator so near-horizontal rays cannot demand a
    // huge step count; the displacement just saturates instead.
    let dir = view_tan.xy / max(abs(view_tan.z), 0.4);
    let step_uv = -dir * (POM_HEIGHT_TILES / f32(POM_STEPS));
    let layer_depth = 1.0 / f32(POM_STEPS);
    var current = uv;
    var depth = 0.0;
    var height = detail_height_at(current);
    for (var i = 0; i < POM_STEPS; i = i + 1) {
        if depth >= height {
            break;
        }
        current += step_uv;
        height = detail_height_at(current);
        depth += layer_depth;
    }
    return current - uv;
}

/// Dominant-axis parallax displacement in the body frame for the near detail.
fn near_detail_parallax_offset(
    local_position: vec3<f32>,
    anchor: vec3<f32>,
    normal: vec3<f32>,
    scale: f32,
    view_dir_body: vec3<f32>,
) -> vec3<f32> {
    let axis = abs(normal);
    if axis.z >= axis.x && axis.z >= axis.y {
        let n = vec3<f32>(0.0, 0.0, select(-1.0, 1.0, normal.z >= 0.0));
        let view_tan = vec3<f32>(view_dir_body.x, view_dir_body.y, dot(view_dir_body, n));
        let o = pom_plane_offset(local_position.xy * scale + anchor.xy, view_tan);
        return vec3<f32>(o.x, o.y, 0.0);
    } else if axis.x >= axis.y {
        let n = vec3<f32>(select(-1.0, 1.0, normal.x >= 0.0), 0.0, 0.0);
        let view_tan = vec3<f32>(view_dir_body.z, view_dir_body.y, dot(view_dir_body, n));
        let o = pom_plane_offset(local_position.zy * scale + anchor.zy, view_tan);
        return vec3<f32>(0.0, o.y, o.x);
    }
    let n = vec3<f32>(0.0, select(-1.0, 1.0, normal.y >= 0.0), 0.0);
    let view_tan = vec3<f32>(view_dir_body.x, view_dir_body.z, dot(view_dir_body, n));
    let o = pom_plane_offset(local_position.xz * scale + anchor.xz, view_tan);
    return vec3<f32>(o.x, 0.0, o.y);
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

// Triplanar body-fixed projection of the layered ground material.
//
// Each axis plane is sampled and layer-blended separately, then the three
// material results are mixed by surface orientation. This mirrors
// `sample_detail_triplanar` and is required for coordinate invariance: the
// `anchor` supplied by the CPU is only the fractional phase of a planet-scale
// body coordinate, so it wraps by whole tiles. Blending the three axis
// *coordinates* into one UV first (the previous approach) turns a whole-tile
// wrap of one axis, multiplied by a fractional orientation weight, into a
// partial-tile shift of the sampled texture. That shifted the ground albedo,
// roughness, normal, and height-blended layer choice whenever the render origin
// or body rotation advanced, producing the green/brown terrain flicker. Sampling
// each plane at its own phase and blending the sampled values preserves the
// invariance, because a repeating texture sampled a whole tile away is
// unchanged.
fn sample_layered_triplanar(
    local_position: vec3<f32>,
    anchor: vec3<f32>,
    normal: vec3<f32>,
    scale: f32,
    alpha: array<f32, LAYER_COUNT>,
) -> LayeredSample {
    let axis = abs(normal);
    let axis_sum = max(axis.x + axis.y + axis.z, 1e-4);
    let axis_weights = axis / axis_sum;

    let uv_x = local_position.zy * scale + anchor.zy;
    let uv_y = local_position.xz * scale + anchor.xz;
    let uv_z = local_position.xy * scale + anchor.xy;

    let sample_x = sample_layers(uv_x, height_aware_weights(uv_x, alpha));
    let sample_y = sample_layers(uv_y, height_aware_weights(uv_y, alpha));
    let sample_z = sample_layers(uv_z, height_aware_weights(uv_z, alpha));

    var result: LayeredSample;
    result.albedo = sample_x.albedo * axis_weights.x
        + sample_y.albedo * axis_weights.y
        + sample_z.albedo * axis_weights.z;
    result.roughness = sample_x.roughness * axis_weights.x
        + sample_y.roughness * axis_weights.y
        + sample_z.roughness * axis_weights.z;
    result.normal_ts = sample_x.normal_ts * axis_weights.x
        + sample_y.normal_ts * axis_weights.y
        + sample_z.normal_ts * axis_weights.z;
    return result;
}

// ---------------------------------------------------------------------------
// Developer diagnostic render modes.
//
// Each mode returns a deterministic false colour of exactly one authoritative
// input. Codes must match `TerrainDiagnostic` in src/presentation/diagnostics.rs;
// a test in terrain/render.rs pins the two together. The modes are presentation
// only: nothing here can change height, collision, material inputs, or any
// simulation state.
//
// Values are written as linear radiance. Rocket mode renders an HDR view with a
// filmic display transform and bloom, so while a diagnostic mode is active the
// camera is switched to `Tonemapping::None` with bloom muted, and the previous
// values are restored on return. With that bypass, the PNG holds the linear
// value encoded as sRGB: `linear = srgb_to_linear(byte / 255)`.
// ---------------------------------------------------------------------------
const DIAG_NORMAL: u32 = 0u;
const DIAG_ALBEDO: u32 = 1u;
const DIAG_WORLD_NORMALS: u32 = 2u;
const DIAG_UV0: u32 = 3u;
const DIAG_UV1: u32 = 4u;
const DIAG_HEIGHT: u32 = 5u;
const DIAG_LAND_SEA: u32 = 6u;
const DIAG_SURFACE_CLASS: u32 = 7u;
const DIAG_LOD: u32 = 8u;
const DIAG_PATCH_BOUNDARY: u32 = 9u;
const DIAG_PATCH_IDENTITY: u32 = 10u;
const DIAG_INPUTS: u32 = 11u;
const DIAG_OCCLUSION: u32 = 12u;
const DIAG_ROUGHNESS: u32 = 13u;
const DIAG_METALLIC: u32 = 14u;
const DIAG_LAYER_BLEND: u32 = 15u;
const DIAG_MORPH_OFFSET: u32 = 16u;
const DIAG_DETAIL_WEIGHT: u32 = 17u;

/// Height view: a symmetric square-root ramp centred on the sea-level datum.
///
/// A linear ramp over a planetary range makes a metre-scale seam step
/// unmeasurable — one pixel of 8-bit sRGB is about 5 m at a 5,000 m span. The
/// square root keeps the whole range visible while giving the coastal band the
/// resolution defect work actually needs: 10 m is about 13/255 away from 0,
/// against 0.5/255 on a linear ramp.
/// `t = 0.5 + 0.5 * sign(h) * sqrt(min(|h|, HEIGHT_LIMIT) / HEIGHT_LIMIT)`.
const DIAG_HEIGHT_LIMIT_M: f32 = 2000.0;
/// Contour interval of the height view, and how much a contour darkens it.
const DIAG_HEIGHT_CONTOUR_M: f32 = 100.0;
const DIAG_CONTOUR_DARKEN: f32 = 0.75;
/// Morph-offset view range, metres. The displacement is the difference between
/// this patch's surface and its parent's, so both signs occur.
const DIAG_MORPH_MIN_M: f32 = -100.0;
const DIAG_MORPH_MAX_M: f32 = 100.0;
/// Half-width of the patch-U offset drawn as a boundary line.
const DIAG_BOUNDARY_HALF_WIDTH: f32 = 0.0035;
/// Input-availability bits, repeated from `PatchInputFlags` in terrain/render.rs.
const DIAG_INPUT_LOCAL_SURFACES: u32 = 1u;
const DIAG_INPUT_LAYER_WEIGHTS: u32 = 2u;
const DIAG_INPUT_IMAGERY: u32 = 4u;
const DIAG_INPUT_OCCLUSION: u32 = 8u;

fn diag_patch_edge(uv: vec2<f32>) -> f32 {
    return min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
}

/// One while the fragment is on a patch seam. Derived from the patch-local UV,
/// so it follows the rendered patch boundary rather than a nominal grid.
fn diag_boundary(uv: vec2<f32>) -> f32 {
    return select(0.0, 1.0, diag_patch_edge(uv) < DIAG_BOUNDARY_HALF_WIDTH);
}

/// Grid overlay for the UV views: dark lines every 1/divisions.
fn diag_uv_grid(uv: vec2<f32>, divisions: f32) -> f32 {
    let cell = abs(fract(uv * divisions) - 0.5);
    return select(1.0, 0.35, min(cell.x, cell.y) > 0.46);
}

/// Hue ramp for the LOD view: a full turn every 12 levels, held saturation and
/// value so only the hue carries the level.
fn diag_hue(t: f32) -> vec3<f32> {
    let h = fract(t) * 6.0;
    let x = 1.0 - abs(fract(h) * 2.0 - 1.0);
    let sector = u32(floor(h)) % 6u;
    if sector == 0u { return vec3<f32>(1.0, x, 0.0); }
    if sector == 1u { return vec3<f32>(x, 1.0, 0.0); }
    if sector == 2u { return vec3<f32>(0.0, 1.0, x); }
    if sector == 3u { return vec3<f32>(0.0, x, 1.0); }
    if sector == 4u { return vec3<f32>(x, 0.0, 1.0); }
    return vec3<f32>(1.0, 0.0, x);
}

/// Ground-layer palette for the surface-class view. Index order matches the
/// terrain layer catalog used by `sample_layers`.
fn diag_class_colour(index: u32) -> vec3<f32> {
    if index == 0u { return vec3<f32>(0.10, 0.55, 0.10); }  // grass
    if index == 1u { return vec3<f32>(0.45, 0.28, 0.12); }  // soil
    if index == 2u { return vec3<f32>(0.55, 0.55, 0.58); }  // rock
    if index == 3u { return vec3<f32>(0.90, 0.80, 0.35); }  // sand
    if index == 4u { return vec3<f32>(0.95, 0.97, 1.00); }  // snow
    if index == 5u { return vec3<f32>(0.30, 0.30, 0.32); }  // gravel
    return vec3<f32>(0.22, 0.14, 0.10);                     // mud
}

/// Index of the largest class weight, used by the surface-class view. The two
/// splats are the same texels the layered material blends.
fn diag_dominant_layer(weights_0: vec4<f32>, weights_1: vec4<f32>) -> i32 {
    let weights = array<f32, LAYER_COUNT>(
        weights_0.r,
        weights_0.g,
        weights_0.b,
        weights_0.a,
        weights_1.r,
        weights_1.g,
        weights_1.b,
    );
    var best = 0u;
    var best_weight = weights[0];
    for (var i = 1u; i < LAYER_COUNT; i = i + 1u) {
        if weights[i] > best_weight {
            best_weight = weights[i];
            best = i;
        }
    }
    return i32(best);
}

/// Boundaries darken a view slightly instead of being invisible; the factor is
/// shared so every overlaid view reads the seam the same way.
fn boundary_gain(boundary: f32) -> f32 {
    return 1.0 - 0.6 * boundary;
}

/// The false colour of one diagnostic mode.
///
/// `dominant_layer` is the index of the largest weight-map channel, or -1 when
/// the patch has no classification data at all: those pixels are drawn magenta,
/// because "no data" must never look like a classification result.
fn terrain_diagnostic_colour(
    mode: u32,
    uv0: vec2<f32>,
    uv1: vec2<f32>,
    geometric_normal: vec3<f32>,
    height_m: f32,
    morph_offset_m: f32,
    geographic_albedo: vec3<f32>,
    occlusion: vec3<f32>,
    roughness: f32,
    metallic: f32,
    layer_blend: f32,
    dominant_layer: i32,
    detail_weight: f32,
) -> vec3<f32> {
    let boundary = diag_boundary(uv1);
    let no_data = vec3<f32>(1.0, 0.0, 1.0);

    if mode == DIAG_ALBEDO {
        return geographic_albedo;
    }
    if mode == DIAG_WORLD_NORMALS {
        return normalize(geometric_normal) * 0.5 + vec3<f32>(0.5);
    }
    if mode == DIAG_UV0 {
        // Geographic coordinates span the whole sphere, so the ramp is faint by
        // design and the grid carries the readable signal.
        return vec3<f32>(uv0, 0.25) * diag_uv_grid(uv0, 16.0) * boundary_gain(boundary);
    }
    if mode == DIAG_UV1 {
        return vec3<f32>(uv1, 0.25) * diag_uv_grid(uv1, 8.0) * boundary_gain(boundary);
    }
    if mode == DIAG_HEIGHT {
        // Symmetric square-root ramp about the datum; see DIAG_HEIGHT_LIMIT_M.
        let magnitude = min(abs(height_m), DIAG_HEIGHT_LIMIT_M) / DIAG_HEIGHT_LIMIT_M;
        let t = 0.5 + 0.5 * sign(height_m) * sqrt(magnitude);
        // Contours mark 100 m intervals so slope and terracing are legible in a
        // still frame without changing the plotted value away from a contour.
        let contour_phase = fract(height_m / DIAG_HEIGHT_CONTOUR_M);
        let on_contour = contour_phase < 0.08;
        let shade = select(1.0, DIAG_CONTOUR_DARKEN, on_contour);
        return vec3<f32>(t * shade);
    }
    if mode == DIAG_LAND_SEA {
        // The datum band is drawn white rather than being folded into one of the
        // two classes, so the coastline is where the classification changes.
        if abs(height_m) < 2.0 {
            return vec3<f32>(1.0);
        }
        if height_m < 0.0 {
            return vec3<f32>(0.05, 0.20, 0.85);
        }
        return vec3<f32>(0.15, 0.70, 0.20);
    }
    if mode == DIAG_SURFACE_CLASS {
        if dominant_layer < 0 {
            return no_data;
        }
        return diag_class_colour(u32(dominant_layer)) * boundary_gain(boundary);
    }
    if mode == DIAG_LOD {
        let level = terrain_surface.diagnostic_patch_level;
        let hue = diag_hue(f32(level) * 0.083);
        // Darken by level within the turn so neighbouring levels stay separable
        // even when the hue ramp wraps.
        let shade = 0.55 + 0.45 * fract(f32(level) * 0.083);
        return hue * shade * boundary_gain(boundary);
    }
    if mode == DIAG_PATCH_BOUNDARY {
        return vec3<f32>(0.25) + vec3<f32>(0.75) * boundary;
    }
    if mode == DIAG_PATCH_IDENTITY {
        let id = terrain_surface.diagnostic_patch_identity;
        let colour = vec3<f32>(
            f32((id >> 16u) & 255u),
            f32((id >> 8u) & 255u),
            f32(id & 255u),
        ) / 255.0;
        // A fixed floor keeps every patch visible; the seam stays black so the
        // patch extent is readable without a second overlay.
        return (colour * 0.85 + vec3<f32>(0.08)) * (1.0 - boundary);
    }
    if mode == DIAG_INPUTS {
        let flags = terrain_surface.diagnostic_patch_flags;
        var colour = vec3<f32>(0.06);
        if (flags & DIAG_INPUT_LOCAL_SURFACES) != 0u { colour.r += 0.9; }
        if (flags & DIAG_INPUT_LAYER_WEIGHTS) != 0u { colour.g += 0.9; }
        if (flags & DIAG_INPUT_IMAGERY) != 0u { colour.b += 0.9; }
        if (flags & DIAG_INPUT_OCCLUSION) != 0u { colour += vec3<f32>(0.35); }
        return colour * boundary_gain(boundary);
    }
    if mode == DIAG_OCCLUSION {
        // Red is the baked self-shadow visibility toward the Sun, green the baked
        // sky visibility. Blue is unused by the bake, so it marks the patch edge.
        return vec3<f32>(occlusion.r, occlusion.g, boundary);
    }
    if mode == DIAG_ROUGHNESS {
        return vec3<f32>(roughness) * boundary_gain(boundary);
    }
    if mode == DIAG_METALLIC {
        return vec3<f32>(metallic) * boundary_gain(boundary);
    }
    if mode == DIAG_LAYER_BLEND {
        // Magenta marks the single-layer fallback, so one look separates the
        // patches that take the layered path from those that do not.
        return mix(no_data, vec3<f32>(0.15, 0.85, 0.95), step(0.5, layer_blend));
    }
    if mode == DIAG_MORPH_OFFSET {
        let t = clamp(
            (morph_offset_m - DIAG_MORPH_MIN_M) / (DIAG_MORPH_MAX_M - DIAG_MORPH_MIN_M),
            0.0,
            1.0,
        );
        // Zero displacement is mid grey, so an unsupported morph (root patches)
        // reads as its own value instead of an extreme.
        return vec3<f32>(t);
    }
    if mode == DIAG_DETAIL_WEIGHT {
        return vec3<f32>(clamp(detail_weight, 0.0, 1.0)) * boundary_gain(boundary);
    }
    return no_data;
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
    // The normal diagnostic shows the geometric surface. `pbr_input.N` is
    // perturbed below by the detail, layer and micro normal maps, so it must be
    // captured here or the view would show the shading normal instead.
    let geometric_normal = pbr_input.N;
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
    // Mid-scale ground patchiness from the precise render-relative position plus
    // the fractional origin phase. The doubled coordinate keeps the finer octave
    // continuous without a second anchor.
    let meso_position = body_local * MESO_FREQUENCY + terrain_surface.meso_anchor;
    let meso_fade = 1.0 - smoothstep(MESO_FADE_START_M, MESO_FADE_END_M, view_distance);
    let meso_variation = 1.0
        + (value_noise(meso_position) * 2.0 - 1.0) * MESO_STRENGTH * meso_fade
        + (value_noise(
            meso_position * (MESO_FINE_FREQUENCY / MESO_FREQUENCY)
                + vec3<f32>(17.0, 5.0, 9.0),
        ) * 2.0 - 1.0) * MESO_FINE_STRENGTH * meso_fade;
    // Shared micro-detail carries the surface grain below the imagery/texture
    // resolution. It fades out sooner than the patch maps so it never aliases.
    let micro_fade = 1.0 - smoothstep(300.0, 1500.0, view_distance);
    let micro_weight = detail_fade * micro_fade;
    let detail = sample_detail_triplanar(
        body_local,
        terrain_surface.detail_anchor,
        body_normal,
        terrain_surface.detail_scale,
        vec3<f32>(0.0),
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
    // Parallax occlusion of the near detail. Only the very closest ground pays
    // for the march, and the displacement is faded to zero so the transition
    // cannot pop. The eye direction is measured in the body frame, matching the
    // projection frame of the sampled detail.
    let pom_fade = 1.0 - smoothstep(POM_FADE_START_M, POM_FADE_END_M, view_distance);
    var near_offset = vec3<f32>(0.0);
    if pom_fade > 0.0 {
        let view_dir_body = normalize(
            rotate_by_quat(
                terrain_surface.inertial_to_body,
                view.world_position.xyz - in.world_position.xyz,
            ),
        );
        near_offset = near_detail_parallax_offset(
            body_local,
            terrain_surface.near_detail_anchor,
            body_normal,
            terrain_surface.near_detail_scale,
            view_dir_body,
        ) * pom_fade;
    }
    let near_detail = sample_detail_triplanar(
        body_local,
        terrain_surface.near_detail_anchor,
        body_normal,
        terrain_surface.near_detail_scale,
        near_offset,
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
    // Raw class weights, hoisted so the surface-class and layer-blend
    // diagnostics read exactly the texels the material reads. They keep their
    // neutral fallback (pure grass) where no weight map exists, which the
    // diagnostics turn into "no data" rather than a confident classification.
    var class_weights_0 = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    var class_weights_1 = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    if terrain_surface.layer_blend_weight > 0.0 {
        let splat0 = textureSample(
            terrain_layer_weights,
            terrain_layer_weights_sampler,
            in.uv_b,
            0,
        );
        let splat1 = textureSample(
            terrain_layer_weights,
            terrain_layer_weights_sampler,
            in.uv_b,
            1,
        );
        class_weights_0 = splat0;
        class_weights_1 = splat1;
        let weights = array<f32, LAYER_COUNT>(
            splat0.r,
            splat0.g,
            splat0.b,
            splat0.a,
            splat1.r,
            splat1.g,
            splat1.b,
        );
        let layered = sample_layered_triplanar(
            body_local,
            terrain_surface.layer_anchor,
            body_normal,
            terrain_surface.layer_tiling_scale,
            weights,
        );
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
            * meso_variation
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
    // Developer diagnostic rendering. Uniform per patch, so this is one
    // comparison in production and a uniform branch otherwise; it runs after
    // every material input is resolved so each view shows the value the
    // production path actually used. The early return deliberately bypasses
    // lighting, occlusion and post-processing: a diagnostic frame must not be
    // shaded by the terms it is meant to reveal.
    if terrain_surface.diagnostic_mode != DIAG_NORMAL {
        var morph_offset_m = 0.0;
#ifdef VERTEX_COLORS
        morph_offset_m = in.color.r;
#endif
        // Sea-level datum radius is supplied per patch from the terrain
        // authority's own sea level at the patch centre, so the height and
        // land/sea views use the same datum the water cap is built from.
        let diagnostic_height_m = length(body_position)
            - terrain_surface.diagnostic_datum_radius_m;
        let dominant_layer = select(
            diag_dominant_layer(class_weights_0, class_weights_1),
            -1,
            terrain_surface.layer_blend_weight <= 0.0,
        );
        let diagnostic = terrain_diagnostic_colour(
            terrain_surface.diagnostic_mode,
            in.uv,
            in.uv_b,
            geometric_normal,
            diagnostic_height_m,
            morph_offset_m,
            base_albedo,
            occlusion.rgb,
            pbr_input.material.perceptual_roughness,
            pbr_input.material.metallic,
            layer_fade,
            dominant_layer,
            detail_weight,
        );
        var diagnostic_out: FragmentOutput;
        diagnostic_out.color = vec4<f32>(
            clamp(diagnostic, vec3<f32>(0.0), vec3<f32>(1.0)),
            1.0,
        );
        return diagnostic_out;
    }
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material,
        pbr_input.material.base_color,
    );
    // The sky-occlusion term scales only the indirect contribution; the
    // self-shadow term scales only the direct-sun contribution below. Cloud
    // shadows ride on the same direct term, so a shaded patch keeps its ambient
    // sky fill.
    let cloud_shadow = cloud_shadow_visibility(
        terrain_surface.cloud,
        in.world_position.xyz - terrain_surface.planet_center,
        terrain_cloud_base_noise,
        terrain_cloud_base_noise_sampler,
        terrain_cloud_detail_noise,
        terrain_cloud_detail_noise_sampler,
        terrain_cloud_weather_map,
        terrain_cloud_weather_map_sampler,
    );
    let self_shadow_term = mix(
        1.0,
        baked_self_shadow,
        terrain_surface.self_shadow_strength,
    ) * cloud_shadow;
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
