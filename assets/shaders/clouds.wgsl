// Geographic cloud map: luminance is coverage; black regions are clear sky.
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}

struct CloudParams {
    coverage: f32,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> clouds: CloudParams;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr = pbr_input_from_standard_material(in, is_front);
    let mapped = pbr.material.base_color;
    let coverage = clamp(dot(mapped.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
    pbr.material.base_color = vec4<f32>(vec3<f32>(1.0), coverage * mapped.a * clouds.coverage);
    pbr.material.perceptual_roughness = 1.0;
    pbr.material.reflectance = vec3<f32>(0.0);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr);
    out.color = main_pass_post_lighting_processing(pbr, out.color);
    return out;
}
