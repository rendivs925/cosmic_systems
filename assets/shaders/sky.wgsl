// Physically based single-scattering planetary sky.
//
// Unlike Bevy's built-in atmosphere, this shader honours the true planet centre
// and the observer's local vertical from the flight reference frame instead of
// assuming world +Y is up. It integrates Rayleigh, Mie, and ozone extinction
// and in-scattering along the view ray with a bounded sample count, and applies
// the same extinction along the secondary path toward the Sun.
//
// Output is premultiplied: rgb is the in-scattered radiance in the same
// radiometric scale as the PBR directional light (colour * illuminance), and
// alpha is 1 - transmittance, so the blend over already-drawn stars and terrain
// is sky_radiance + background * transmittance.

#define_import_path cosmic_systems::sky

#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::forward_io::{VertexOutput, FragmentOutput}

struct SkyParams {
    // Planet centre in the camera-relative render frame.
    planet_center: vec3<f32>,
    bottom_radius: f32,
    top_radius: f32,
    rayleigh_scale_height: f32,
    mie_scale_height: f32,
    mie_asymmetry: f32,
    rayleigh_scattering: vec3<f32>,
    mie_scattering: f32,
    mie_absorption: f32,
    ozone_layer_altitude: f32,
    ozone_layer_width: f32,
    ozone_absorption: vec3<f32>,
    // Unit direction from the observer toward the Sun, and the directional
    // light colour premultiplied by its illuminance.
    sun_direction: vec3<f32>,
    sun_irradiance: vec3<f32>,
    ground_albedo: vec3<f32>,
    sky_strength: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sky: SkyParams;

const PI: f32 = 3.141592653589793;
const VIEW_SAMPLES: u32 = 16u;
const SUN_SAMPLES: u32 = 8u;

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

fn exponential_density(altitude: f32, scale_height: f32) -> f32 {
    return exp(-max(altitude, 0.0) / max(scale_height, 1.0));
}

// Ozone is concentrated in a band, approximated as a triangle profile centred
// on its layer altitude.
fn ozone_density(altitude: f32) -> f32 {
    let half_width = max(sky.ozone_layer_width * 0.5, 1.0);
    return saturate(1.0 - abs(altitude - sky.ozone_layer_altitude) / half_width);
}

fn rayleigh_phase(cos_theta: f32) -> f32 {
    return 3.0 / (16.0 * PI) * (1.0 + cos_theta * cos_theta);
}

fn mie_phase(cos_theta: f32) -> f32 {
    let g = sky.mie_asymmetry;
    let g2 = g * g;
    let denom = max(1.0 + g2 - 2.0 * g * cos_theta, 1e-4);
    return (1.0 - g2) / (4.0 * PI * pow(denom, 1.5));
}

// Position is relative to the planet centre.
fn extinction_at(position: vec3<f32>) -> vec3<f32> {
    let altitude = length(position) - sky.bottom_radius;
    let rayleigh = exponential_density(altitude, sky.rayleigh_scale_height);
    let mie = exponential_density(altitude, sky.mie_scale_height);
    let ozone = ozone_density(altitude);
    return sky.rayleigh_scattering * rayleigh
        + (sky.mie_scattering + sky.mie_absorption) * mie
        + sky.ozone_absorption * ozone;
}

// Transmittance from a sample to the top of the atmosphere along the Sun
// direction. Zero when the Sun is occluded by the planet.
fn sun_transmittance(position: vec3<f32>) -> vec3<f32> {
    let local_up = normalize(position);
    let ground = ray_sphere(position, sky.sun_direction, sky.bottom_radius);
    if ground.x > 0.0 && dot(local_up, sky.sun_direction) < 0.0 {
        return vec3(0.0);
    }
    let top = ray_sphere(position, sky.sun_direction, sky.top_radius);
    if top.y <= 0.0 {
        return vec3(1.0);
    }
    var optical_depth = vec3(0.0);
    let step = top.y / f32(SUN_SAMPLES);
    for (var i: u32 = 0u; i < SUN_SAMPLES; i = i + 1u) {
        let t = (f32(i) + 0.5) * step;
        optical_depth += extinction_at(position + sky.sun_direction * t) * step;
    }
    return exp(-optical_depth);
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    let camera_world = view.world_position;
    let ray_dir = normalize(in.world_position.xyz - camera_world);
    let position = camera_world - sky.planet_center;
    let radius = length(position);
    let local_up = normalize(position);
    let cos_theta = dot(ray_dir, local_up);

    let atmosphere_hit = ray_sphere(position, ray_dir, sky.top_radius);
    let ground_hit = ray_sphere(position, ray_dir, sky.bottom_radius);
    let hits_ground = ground_hit.x > 0.0;

    var segment_start = 0.0;
    var segment_end = atmosphere_hit.y;
    if radius < sky.top_radius {
        segment_end = select(atmosphere_hit.y, ground_hit.x, hits_ground);
    } else {
        if atmosphere_hit.x < 0.0 {
            out.color = vec4(0.0, 0.0, 0.0, 0.0);
            return out;
        }
        segment_start = atmosphere_hit.x;
        segment_end = select(atmosphere_hit.y, ground_hit.x, hits_ground);
    }
    let segment_length = max(segment_end - segment_start, 0.0);
    if segment_length <= 0.0 {
        out.color = vec4(0.0, 0.0, 0.0, 0.0);
        return out;
    }

    let step = segment_length / f32(VIEW_SAMPLES);
    var in_scatter = vec3(0.0);
    var transmittance = vec3(1.0);
    for (var i: u32 = 0u; i < VIEW_SAMPLES; i = i + 1u) {
        let t = segment_start + (f32(i) + 0.5) * step;
        let sample_position = position + ray_dir * t;
        let altitude = length(sample_position) - sky.bottom_radius;
        let rayleigh = exponential_density(altitude, sky.rayleigh_scale_height);
        let mie = exponential_density(altitude, sky.mie_scale_height);
        let ozone = ozone_density(altitude);
        let extinction = sky.rayleigh_scattering * rayleigh
            + (sky.mie_scattering + sky.mie_absorption) * mie
            + sky.ozone_absorption * ozone;
        let scattering = sky.rayleigh_scattering * rayleigh * rayleigh_phase(cos_theta)
            + sky.mie_scattering * mie * mie_phase(cos_theta);
        let step_optical_depth = extinction * step;
        let step_transmittance = exp(-step_optical_depth);
        let transmittance_to_sun = sun_transmittance(sample_position);
        // Analytic integral of the in-scattering across the step.
        let inscattered = scattering * transmittance_to_sun
            * (vec3(1.0) - step_transmittance) / max(extinction, vec3(1e-6));
        in_scatter += transmittance * inscattered;
        transmittance *= step_transmittance;
        if all(transmittance < vec3(0.002)) {
            break;
        }
    }

    var radiance = in_scatter * sky.sun_irradiance * sky.sky_strength;

    // Ground bounce for rays that reach the surface without geometry coverage
    // (for example beyond streamed terrain). Physically it is the surface
    // radiance attenuated back through the atmosphere.
    if hits_ground {
        let ground_position = position + ray_dir * segment_end;
        let ground_normal = normalize(ground_position);
        let cos_sun = max(dot(ground_normal, sky.sun_direction), 0.0);
        let ground_radiance = sky.ground_albedo / PI
            * sky.sun_irradiance
            * sun_transmittance(ground_position)
            * cos_sun;
        radiance += transmittance * ground_radiance;
    }

    let opacity = saturate(1.0 - dot(transmittance, vec3(1.0 / 3.0)));
    out.color = vec4(radiance, opacity);
    return out;
}
