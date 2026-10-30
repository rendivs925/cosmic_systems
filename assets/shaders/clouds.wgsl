// Volumetric ray-marched planetary clouds (cloud engine spec sections 3-6).
//
// The cloud deck is a spherical slab between `bottom_radius` and `top_radius`.
// Each fragment marches the view ray through the slab, sampling a 3D
// Perlin-Worley density field (co-rotating with the planet), carving it with the
// weather map and high-frequency Worley erosion, then integrating
// sun-scattering with a triple-lobe Henyey-Greenstein phase and multi-scatter
// octaves. The scattering integral is the physical single-scatter form: the
// source radiance is multiplied by the analytic optical-depth integral
// `(1 - exp(-sigma_e * ds))` and the droplet single-scattering albedo, with no
// presentation fudge factors. Output is premultiplied: rgb is the scattered
// radiance in the same radiometric scale as the PBR directional light
// (colour * illuminance), and alpha is 1 - transmittance, so the blend over
// terrain and sky is `cloud_radiance + background * transmittance`.
//
// Presentation only: nothing here feeds collision or any simulation state.

#define_import_path cosmic_systems::clouds

#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::forward_io::VertexOutput
// Shared density field: the terrain cloud-shadow pass evaluates the same
// function, so the shadow always lines up with the cloud above it.
#import "shaders/cloud_density.wgsl"::CLOUD_ALBEDO
#import "shaders/cloud_density.wgsl"::CloudField
#import "shaders/cloud_density.wgsl"::PI
#import "shaders/cloud_density.wgsl"::cloud_sun_gate
#import "shaders/cloud_density.wgsl"::ray_sphere
#import "shaders/cloud_density.wgsl"::saturate
#import "shaders/cloud_density.wgsl"::sample_density
#import "shaders/cloud_density.wgsl"::density_lod

// Field order and types must match `CloudParams` in rendering/materials.rs.
struct CloudParams {
    planet_center: vec3<f32>,
    bottom_radius: f32,
    top_radius: f32,
    planet_rotation: mat3x3<f32>,
    sun_direction: vec3<f32>,
    sun_irradiance: vec3<f32>,
    sun_transmittance: vec3<f32>,
    time_s: f32,
    coverage: f32,
    cloud_type: f32,
    morph_variation: f32,
    density_scale: f32,
    absorption: f32,
    forward_g: f32,
    noise_scale: f32,
    detail_scale: f32,
    erosion_scale: f32,
    wind_speed: f32,
    wind_direction: vec2<f32>,
    primary_steps: f32,
    light_steps: f32,
    multi_scatter_octaves: f32,
    ground_color: vec3<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> clouds: CloudParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var base_noise: texture_3d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var base_noise_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var detail_noise: texture_3d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var detail_noise_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var weather_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var weather_map_sampler: sampler;

// The cloud dome is a camera-anchored proxy at a fixed radius, but the clouds
// themselves sit on a planet-centred shell whose distance varies from a few km
// (on the ground) to tens of km (at altitude). Writing the true shell-entry
// distance as the fragment depth lets the depth test place the cloud ahead of
// the terrain instead of the proxy's fixed radius doing so.
struct CloudFragmentOutput {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

fn clip_depth(world_point: vec3<f32>) -> f32 {
    let clip = view.clip_from_world * vec4<f32>(world_point, 1.0);
    return clip.z / clip.w;
}

// Hard upper bounds keep the loop trip counts statically bounded; the actual
// counts come from the quality uniform and may be lower.
const MAX_PRIMARY_STEPS: u32 = 128u;
const MAX_LIGHT_STEPS: u32 = 8u;
const MAX_MULTI_OCTAVES: u32 = 4u;

// Interleaved gradient noise (Jimenez): a cheap, well-distributed per-pixel
// dither. Far less grainy than a white-noise hash at these step counts, so it
// breaks ray-march banding without turning the deck into static.
fn interleaved_gradient_noise(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(0.06711056 * p.x + 0.00583715 * p.y));
}

// Density-relevant subset of the cloud material uniform, in the shared field's
// layout. Rebuilt per call; the compiler folds the copies into the uniform
// register reads either way.
fn cloud_field() -> CloudField {
    var field: CloudField;
    field.bottom_radius = clouds.bottom_radius;
    field.top_radius = clouds.top_radius;
    field.absorption = clouds.absorption;
    field.time_s = clouds.time_s;
    field.coverage = clouds.coverage;
    field.cloud_type = clouds.cloud_type;
    field.morph_variation = clouds.morph_variation;
    field.density_scale = clouds.density_scale;
    field.noise_scale = clouds.noise_scale;
    field.detail_scale = clouds.detail_scale;
    field.erosion_scale = clouds.erosion_scale;
    field.wind_speed = clouds.wind_speed;
    field.wind_direction = clouds.wind_direction;
    return field;
}

fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = max(1.0 + g2 - 2.0 * g * cos_theta, 1e-4);
    return (1.0 - g2) / (4.0 * PI * pow(denom, 1.5));
}

// Forward silver-lining lobe plus backward and side fill (spec section 4.2).
fn triple_lobe_phase(cos_theta: f32, g_forward: f32) -> f32 {
    let forward_lobe = henyey_greenstein(cos_theta, g_forward);
    let backward_lobe = henyey_greenstein(cos_theta, -0.35);
    let side_lobe = henyey_greenstein(cos_theta, 0.1);
    return mix(mix(forward_lobe, backward_lobe, 0.25), side_lobe, 0.15);
}

// Sun visibility: march toward the Sun across the whole cloud slab and sum the
// density-length a photon must cross to reach the top (spec section 4.3).
// `step_length` is a real path length in metres, so the summed density times
// the extinction coefficient is the optical depth. Multi-scatter octaves then
// add the energy that higher scattering orders return along the same direction
// (spec section 4.4).
//
// There is deliberately no separate powder term: the previous one multiplied
// this visibility by `1 - exp(-density_length * absorption * 2.5)`, which is
// largest where the Sun path is clear. That made sunlit cloud tops the darkest
// part of the deck and left the whole layer at ambient level. Optically thin
// cloud is already dimmed by the single-scatter step integral below.
fn march_to_sun(body_pos: vec3<f32>) -> f32 {
    let layer = clouds.top_radius - clouds.bottom_radius;
    let step_length = layer / max(clouds.light_steps, 1.0);
    let light_step = clouds.sun_direction * step_length;
    let field = cloud_field();
    let lod = density_lod(field, step_length);
    let inertial_to_body = transpose(clouds.planet_rotation);
    var density_length = 0.0;
    var p = body_pos;
    for (var i: u32 = 0u; i < MAX_LIGHT_STEPS; i = i + 1u) {
        if f32(i) >= clouds.light_steps {
            break;
        }
        p += light_step;
        density_length += sample_density(
            field,
            base_noise,
            base_noise_sampler,
            detail_noise,
            detail_noise_sampler,
            weather_map,
            weather_map_sampler,
            inertial_to_body * p,
            lod,
        ) * step_length;
    }

    var multi = 0.0;
    for (var octave: u32 = 0u; octave < MAX_MULTI_OCTAVES; octave = octave + 1u) {
        if f32(octave) >= clouds.multi_scatter_octaves {
            break;
        }
        let optical_depth = density_length * clouds.absorption * pow(0.5, f32(octave));
        multi += exp(-optical_depth) * pow(0.5, f32(octave));
    }
    return multi;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> CloudFragmentOutput {
    var out: CloudFragmentOutput;
    out.depth = 1.0;

    let camera_world = view.world_position;

    let ray_dir = normalize(in.world_position.xyz - camera_world);
    let cam_pos = camera_world - clouds.planet_center;
    let cam_radius = length(cam_pos);
    let field = cloud_field();
    let inertial_to_body = transpose(clouds.planet_rotation);

    // The dome is camera-centred, so only its far hemisphere (back faces) is
    // ever visible; keeping exactly one face avoids double-blending.
    if is_front {
        out.color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        return out;
    }

    // Robust spherical-shell slab intersection, valid whether the camera sits
    // inside or outside the [bottom_radius, top_radius] shell.
    let top_hit = ray_sphere(cam_pos, ray_dir, clouds.top_radius);
    if top_hit.y <= 0.0 {
        out.color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        return out;
    }
    var segment_start = max(top_hit.x, 0.0);
    var segment_end = top_hit.y;
    let bottom_hit = ray_sphere(cam_pos, ray_dir, clouds.bottom_radius);
    if bottom_hit.x > 0.0 {
        segment_end = min(segment_end, bottom_hit.x);
    }
    if cam_radius < clouds.bottom_radius {
        segment_start = max(segment_start, bottom_hit.y);
    }
    if segment_end <= segment_start {
        out.color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        return out;
    }
    // Place the cloud at its true shell-entry depth for the depth test.
    // The shell entry can sit at or beyond the far plane, where the depth value
    // saturates to 1.0 and ties with the far-field globe's own saturated depth.
    // A tie is resolved per pixel by draw order, which cuts the limb into
    // rectangular blocks; keeping the deck just inside the far plane means a
    // cloud that reaches the horizon is still drawn in front of the planet.
    out.depth = min(clip_depth(camera_world + ray_dir * segment_start), 0.9999);

    let cos_theta = dot(ray_dir, clouds.sun_direction);
    let phase = triple_lobe_phase(cos_theta, clouds.forward_g);

    let steps = max(clouds.primary_steps, 1.0);
    let step_size = (segment_end - segment_start) / steps;
    // A step through a distant deck spans kilometres, so the volume is sampled
    // at the level that step resolves. Without this the limb aliases into
    // blocky white silhouettes, because consecutive steps jump whole cells.
    let lod = density_lod(field, step_size);
    // Mild per-pixel dither (amplitude < a full step) breaks banding without
    // the static that full white-noise jitter produces at these step counts.
    let jitter = interleaved_gradient_noise(in.position.xy);
    var t = segment_start + step_size * jitter;
    var transmittance = 1.0;
    var in_scatter = vec3<f32>(0.0);

    let layer = max(clouds.top_radius - clouds.bottom_radius, 1.0);
    for (var i: u32 = 0u; i < MAX_PRIMARY_STEPS; i = i + 1u) {
        if f32(i) >= steps || t > segment_end || transmittance < 0.005 {
            break;
        }
        let sample_pos = cam_pos + ray_dir * t;
        let density = sample_density(
            field,
            base_noise,
            base_noise_sampler,
            detail_noise,
            detail_noise_sampler,
            weather_map,
            weather_map_sampler,
            inertial_to_body * sample_pos,
            lod,
        );
        if density > 0.001 {
            let sigma_e = density * clouds.absorption;
            let step_optical_depth = sigma_e * step_size;
            let step_transmittance = exp(-step_optical_depth);
            let sun_visibility = march_to_sun(sample_pos);
            // Direct single scattering from the triple-lobe phase function,
            // plus diffuse multiple scattering. Cloud droplets are nearly
            // conservative scatterers, so an optically thick deck reflects most
            // of the sunlight it intercepts as a near-Lambertian layer: the
            // diffuse radiance is `albedo * E * cos(sun zenith) / PI`, tinted
            // by the sky above and the ground below (spec 4.2-4.4). Because the
            // step integral weights this source by the local scattering
            // fraction, an optically thin wisp reflects proportionally less.
            let height_fraction = saturate(
                (length(sample_pos) - clouds.bottom_radius) / layer,
            );
            // Direct sunlight only reaches a sample while the Sun is above its
            // local horizon; the shared gate owns that curve.
            let sun_elevation = dot(normalize(sample_pos), clouds.sun_direction);
            let terminator = cloud_sun_gate(sun_elevation);
            // The atmosphere between the Sun and the deck removes part of the
            // beam, mostly blue, so a low Sun reddens the cloud before the
            // terminator gate closes it.
            let direct = clouds.sun_irradiance * clouds.sun_transmittance
                * sun_visibility * phase * terminator;
            let sun_cos = saturate(sun_elevation);
            // Droplet scattering is spectrally neutral, so the diffuse light
            // is white at the sunlit top and takes the ground bounce colour at
            // the base.
            let ambient_color = mix(clouds.ground_color, vec3(1.0), height_fraction);
            let ambient = clouds.sun_irradiance * (CLOUD_ALBEDO / PI) * sun_cos
                * ambient_color;
            // Radiance scattered toward the camera from this step. `sigma_e *
            // ds` is the step's optical depth, so `1 - step_transmittance` is
            // its analytic integral; the source radiance is per unit optical
            // depth, not per unit extinction, so no division by `sigma_e` is
            // needed and the result is independent of how the optical depth is
            // split between density and the absorption coefficient.
            let source = (direct + ambient) * CLOUD_ALBEDO;
            let inscatter = source * (1.0 - step_transmittance);
            in_scatter += transmittance * inscatter;
            transmittance *= step_transmittance;
            t += step_size;
        } else {
            // Adaptive empty-space skipping.
            t += step_size * 1.8;
        }
    }

    let radiance = in_scatter * view.exposure;
    let opacity = saturate(1.0 - transmittance);
    out.color = vec4<f32>(radiance, opacity);
    return out;
}
