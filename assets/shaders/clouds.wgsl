// Volumetric ray-marched planetary clouds (cloud engine spec sections 3-6).
//
// The cloud deck is a spherical slab between `bottom_radius` and `top_radius`.
// Each fragment marches the view ray through the slab, sampling a 3D
// Perlin-Worley density field (co-rotating with the planet), carving it with the
// weather map and high-frequency Worley erosion, then integrating
// sun-scattering with a triple-lobe Henyey-Greenstein phase, a powder
// (dark-edge) term, and multi-scatter octaves. Output is premultiplied: rgb is
// the scattered radiance in the same radiometric scale as the PBR directional
// light (colour * illuminance), and alpha is 1 - transmittance, so the blend
// over terrain and sky is `cloud_radiance + background * transmittance`.
//
// Presentation only: nothing here feeds collision or any simulation state.

#define_import_path cosmic_systems::clouds

#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::forward_io::VertexOutput

// Field order and types must match `CloudParams` in rendering/materials.rs.
struct CloudParams {
    planet_center: vec3<f32>,
    bottom_radius: f32,
    top_radius: f32,
    planet_rotation: mat3x3<f32>,
    sun_direction: vec3<f32>,
    sun_irradiance: vec3<f32>,
    time_s: f32,
    coverage: f32,
    cloud_type: f32,
    density_scale: f32,
    absorption: f32,
    forward_g: f32,
    powder_strength: f32,
    noise_scale: f32,
    detail_scale: f32,
    erosion_scale: f32,
    wind_speed: f32,
    wind_direction: vec2<f32>,
    aerial_haze: f32,
    primary_steps: f32,
    light_steps: f32,
    multi_scatter_octaves: f32,
    strength: f32,
    sky_color: vec3<f32>,
    ground_color: vec3<f32>,
    ambient_fraction: f32,
    direct_boost: f32,
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

const PI: f32 = 3.141592653589793;
// Hard upper bounds keep the loop trip counts statically bounded; the actual
// counts come from the quality uniform and may be lower.
const MAX_PRIMARY_STEPS: u32 = 128u;
const MAX_LIGHT_STEPS: u32 = 8u;
const MAX_MULTI_OCTAVES: u32 = 4u;

fn saturate(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

fn remap(v: f32, l1: f32, h1: f32, l2: f32, h2: f32) -> f32 {
    return l2 + (v - l1) / max(h1 - l1, 1e-6) * (h2 - l2);
}

// Interleaved gradient noise (Jimenez): a cheap, well-distributed per-pixel
// dither. Far less grainy than a white-noise hash at these step counts, so it
// breaks ray-march banding without turning the deck into static.
fn interleaved_gradient_noise(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(0.06711056 * p.x + 0.00583715 * p.y));
}

// Positive near/far intersection distances, or a negative pair on a miss.
fn ray_sphere(origin: vec3<f32>, dir: vec3<f32>, radius: f32) -> vec2<f32> {
    let b = dot(origin, dir);
    let c = dot(origin, origin) - radius * radius;
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return vec2(-1.0, -1.0);
    }
    let root = sqrt(discriminant);
    return vec2(-b - root, -b + root);
}

// Lifted-condensation-level vertical profile: stratus is a thin flat layer,
// cumulus has a flat base and billowing middle (spec section 3.3).
fn height_profile(height_fraction: f32, cloud_type: f32) -> f32 {
    let stratus = saturate(remap(height_fraction, 0.0, 0.1, 0.0, 1.0))
        * saturate(remap(height_fraction, 0.15, 0.35, 1.0, 0.0));
    let cumulus = saturate(remap(height_fraction, 0.0, 0.15, 0.0, 1.0))
        * saturate(remap(height_fraction, 0.45, 0.9, 1.0, 0.0));
    return mix(stratus, cumulus, cloud_type);
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

fn weather_uv(body_dir: vec3<f32>) -> vec2<f32> {
    let u = atan2(body_dir.z, body_dir.x) / (2.0 * PI) + 0.5;
    let v = asin(clamp(body_dir.y, -1.0, 1.0)) / PI + 0.5;
    return vec2(u, v);
}

// Cloud density at a planet-relative position, in [0, 1].
fn sample_density(body_pos: vec3<f32>) -> f32 {
    let altitude = length(body_pos) - clouds.bottom_radius;
    let layer = max(clouds.top_radius - clouds.bottom_radius, 1.0);
    let height_fraction = saturate(altitude / layer);
    if altitude < 0.0 || altitude > layer {
        return 0.0;
    }

    // Co-rotating body-fixed position: undo the planet rotation so the noise
    // field stays pinned to the ground instead of sliding in inertial space.
    let body_fixed = transpose(clouds.planet_rotation) * body_pos;
    let wind_offset = vec3<f32>(clouds.wind_direction.x, 0.0, clouds.wind_direction.y)
        * (clouds.time_s * clouds.wind_speed * clouds.noise_scale);

    // Weather map: coverage carves clear air, cloud type morphs the profile.
    let body_dir = normalize(body_pos);
    let weather = textureSample(
        weather_map,
        weather_map_sampler,
        weather_uv(body_dir) + clouds.wind_direction * clouds.time_s * 1.0e-6,
    );
    let coverage = saturate(clouds.coverage * weather.r * 1.4);
    if coverage < 0.01 {
        return 0.0;
    }
    let cloud_type = saturate(mix(clouds.cloud_type, weather.b, 0.5));
    let gradient = height_profile(height_fraction, cloud_type);
    if gradient <= 0.001 {
        return 0.0;
    }

    let sample_pos = body_fixed * clouds.noise_scale + wind_offset;
    let base = textureSample(base_noise, base_noise_sampler, sample_pos);
    let low_fbm = base.g * 0.625 + base.b * 0.25 + base.a * 0.125;
    var base_density = remap(base.r, low_fbm - 1.0, 1.0, 0.0, 1.0);
    base_density = base_density * gradient;
    let density_with_coverage = saturate(remap(base_density, 1.0 - coverage, 1.0, 0.0, 1.0));
    if density_with_coverage <= 0.001 {
        return 0.0;
    }

    let detail = textureSample(
        detail_noise,
        detail_noise_sampler,
        sample_pos * clouds.detail_scale + wind_offset * 1.5,
    );
    let high_fbm = detail.r * 0.625 + detail.g * 0.25 + detail.b * 0.125;
    let erosion = mix(high_fbm, 1.0 - high_fbm, saturate(height_fraction * 2.0));
    let final_density = saturate(remap(
        density_with_coverage,
        erosion * clouds.erosion_scale * 0.55,
        1.0,
        0.0,
        1.0,
    ));
    return saturate(final_density * clouds.density_scale);
}

// Sun visibility: march toward the Sun across the whole cloud slab, then fold
// in multi-scatter octaves and the powder (dark-edge) term (spec sections
// 4.3-4.4). A photon leaving a sample must cross the remaining cloud to the top,
// so a fixed small fraction of the layer would under-estimate optical depth.
fn march_to_sun(body_pos: vec3<f32>, cos_theta: f32) -> f32 {
    let layer = clouds.top_radius - clouds.bottom_radius;
    let light_step = clouds.sun_direction * (layer / max(clouds.light_steps, 1.0));
    var total_density = 0.0;
    var p = body_pos;
    for (var i: u32 = 0u; i < MAX_LIGHT_STEPS; i = i + 1u) {
        if f32(i) >= clouds.light_steps {
            break;
        }
        p += light_step;
        total_density += sample_density(p);
    }

    var multi = 0.0;
    for (var octave: u32 = 0u; octave < MAX_MULTI_OCTAVES; octave = octave + 1u) {
        if f32(octave) >= clouds.multi_scatter_octaves {
            break;
        }
        let optical_depth = total_density * clouds.absorption * pow(0.5, f32(octave));
        multi += exp(-optical_depth) * pow(0.5, f32(octave));
    }

    let powder = 1.0 - exp(-total_density * clouds.absorption * 2.5 * clouds.powder_strength);
    let powder_blend = mix(1.0, powder, saturate(cos_theta * 0.5 + 0.5));
    return multi * powder_blend;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> CloudFragmentOutput {
    var out: CloudFragmentOutput;
    out.depth = 1.0;

    let camera_world = view.world_position;
    let ray_dir = normalize(in.world_position.xyz - camera_world);
    let cam_pos = camera_world - clouds.planet_center;
    let cam_radius = length(cam_pos);

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
    out.depth = clip_depth(camera_world + ray_dir * segment_start);

    let cos_theta = dot(ray_dir, clouds.sun_direction);
    let phase = triple_lobe_phase(cos_theta, clouds.forward_g);

    let steps = max(clouds.primary_steps, 1.0);
    let step_size = (segment_end - segment_start) / steps;
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
        let density = sample_density(sample_pos);
        if density > 0.001 {
            let sigma_e = density * clouds.absorption;
            let step_optical_depth = sigma_e * step_size;
            let step_transmittance = exp(-step_optical_depth);
            let sun_visibility = march_to_sun(sample_pos, cos_theta);
            // Direct single-scatter (silver lining emphasised by direct_boost)
            // plus a height-graded ambient term standing in for the sky and
            // ground light that multiple scattering would deliver (spec 4.2-4.4).
            let height_fraction = saturate(
                (length(sample_pos) - clouds.bottom_radius) / layer,
            );
            let direct = clouds.sun_irradiance * sun_visibility * phase
                * clouds.direct_boost;
            let ambient_color = mix(
                clouds.ground_color * 0.35,
                clouds.sky_color * 0.85,
                height_fraction,
            );
            let ambient = clouds.sun_irradiance * clouds.ambient_fraction
                * ambient_color;
            let scattering = (direct + ambient) * density;
            // The analytic step integral is exact for constant density across
            // the step: integral of scattering * exp(-sigma_e * s) ds.
            let inscatter = scattering * (1.0 - step_transmittance)
                / max(sigma_e, 1e-6);
            in_scatter += transmittance * inscatter;
            transmittance *= step_transmittance;
            t += step_size;
        } else {
            // Adaptive empty-space skipping.
            t += step_size * 1.8;
        }
    }

    let radiance = in_scatter * clouds.strength * view.exposure;
    let opacity = saturate(1.0 - transmittance);
    out.color = vec4<f32>(radiance, opacity);
    return out;
}
