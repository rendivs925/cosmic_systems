// Geographic cloud map with separated coverage, shape, detail, and coherent
// wind motion. Coverage is the sampled geographic luminance; shape and detail
// are bounded procedural value noise over the planet-centred position, and the
// detail octave drifts along one coherent wind vector. Presentation only: no
// texture is regenerated per frame and nothing here feeds simulation state.
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}

// Field order and types must match `CloudExtension` in materials.rs.
struct CloudParams {
    coverage: f32,
    shape_scale: f32,
    detail_scale: f32,
    wind_speed: f32,
    time_s: f32,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> clouds: CloudParams;

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 345.45));
    q += dot(q, q + 34.345);
    return fract(q.x * q.y);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    let mapped = pbr.material.base_color;
    let coverage = clamp(dot(mapped.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.0, 1.0);

    // Planet-centred position. The cloud shell is a child of the bound planet,
    // so its world position is small render-frame coordinates.
    let position = in.world_position.xyz;
    let planar = position.xz + position.xy * 0.5;

    // Shape: low-frequency masses that break the deck into coherent lobes.
    let shape = value_noise(planar * clouds.shape_scale);
    // Detail: higher-frequency edges, scrolled along one wind vector so the
    // whole field drifts coherently instead of each cell moving independently.
    let wind = vec2<f32>(
        clouds.time_s * clouds.wind_speed,
        clouds.time_s * clouds.wind_speed * 0.35,
    );
    let detail = value_noise(planar * clouds.detail_scale + wind);

    // Both factors are centred on their sampled noise mean (0.5) so the deck's
    // average opacity is preserved; only the structure changes.
    let shaped = coverage * (0.75 + 0.5 * shape);
    let alpha = clamp(shaped * (0.9 + 0.2 * detail) * clouds.coverage, 0.0, 1.0);

    pbr.material.base_color = vec4<f32>(vec3<f32>(1.0), alpha);
    pbr.material.perceptual_roughness = 1.0;
    pbr.material.reflectance = vec3<f32>(0.0);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
