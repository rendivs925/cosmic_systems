#import bevy_pbr::{
    lighting,
    lighting::LAYER_BASE,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    mesh_view_bindings::{lights, view},
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    pbr_functions::{calculate_diffuse_color, calculate_F0},
    pbr_types,
    shadows,
}

// Scale only the directional contribution already shaded by Bevy. Baked
// landscape shadows must not extinguish ambient sky fill or emissive light.
fn direct_sun_self_shadow_correction(pbr_input: pbr_types::PbrInput, self_shadow: f32) -> vec3<f32> {
    if self_shadow >= 1.0 || lights.n_directional_lights == 0u {
        return vec3<f32>(0.0);
    }
    let view_z = dot(vec4<f32>(
        view.view_from_world[0].z, view.view_from_world[1].z,
        view.view_from_world[2].z, view.view_from_world[3].z,
    ), pbr_input.world_position);
    let ndotv = max(dot(pbr_input.N, pbr_input.V), 0.0001);
    var input: lighting::LightingInput;
    input.layers[LAYER_BASE].NdotV = ndotv;
    input.layers[LAYER_BASE].N = pbr_input.N;
    input.layers[LAYER_BASE].R = reflect(-pbr_input.V, pbr_input.N);
    input.layers[LAYER_BASE].perceptual_roughness = pbr_input.material.perceptual_roughness;
    input.layers[LAYER_BASE].roughness = lighting::perceptualRoughnessToRoughness(pbr_input.material.perceptual_roughness);
    input.P = pbr_input.world_position.xyz;
    input.V = pbr_input.V;
    input.diffuse_color = calculate_diffuse_color(pbr_input.material.base_color.rgb,
        pbr_input.material.metallic, pbr_input.material.specular_transmission,
        pbr_input.material.diffuse_transmission);
    input.F0_ = calculate_F0(pbr_input.material.base_color.rgb,
        pbr_input.material.metallic, pbr_input.material.reflectance);
    input.F_ab = lighting::F_AB(pbr_input.material.perceptual_roughness, ndotv);
    var direct = vec3<f32>(0.0);
    for (var i = 0u; i < lights.n_directional_lights; i = i + 1u) {
        var shadow = 1.0;
        if (pbr_input.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u
            && (lights.directional_lights[i].flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            shadow = shadows::fetch_directional_shadow(i, pbr_input.world_position,
                pbr_input.world_normal, view_z);
        }
        direct += lighting::directional_light(i, &input, true) * shadow;
    }
    return view.exposure * (self_shadow - 1.0) * direct;
}
