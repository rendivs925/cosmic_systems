// Single-scattering planetary sky radiance, shared by the sky dome and the
// sea's mirror reflection.
//
// The dome integrates this per fragment at full resolution; the sea evaluates
// the same function along the reflected view direction with fewer samples.
// Sharing one implementation is what keeps the reflected sky the same colour
// as the sky above it -- and keeps the Rayleigh/Mie/ozone physics in one place.
//
// Positions are planet-relative, and the returned radiance is in the same
// radiometric scale as the PBR directional light (colour * illuminance), before
// camera exposure. Presentation only: nothing here feeds flight dynamics.
//
// Imported by asset path from both shaders. Do not add `#define_import_path`:
// that registers the module only under the custom name, and the asset-path
// `#import "shaders/sky_radiance.wgsl"` then fails silently (see the
// cloud_density.wgsl note and its regression test).

// Field order and types must match `SkyParams` in
// `infrastructure/bevy_adapters/rendering/sky.rs` exactly.
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

// Loop bounds are static; callers pass the actual counts so the sea can trade
// precision for cost without a second code path.
const SKY_MAX_VIEW_SAMPLES: u32 = 16u;
const SKY_MAX_SUN_SAMPLES: u32 = 8u;
const SKY_PI: f32 = 3.141592653589793;

/// In-scattered radiance toward the observer, and the transmittance of the air
/// between it and whatever the ray ends on (the ground or the top of the
/// atmosphere). Premultiplied semantics: `radiance + background * transmittance`.
struct SkyRadiance {
    radiance: vec3<f32>,
    transmittance: vec3<f32>,
}

// Positive near/far intersection distances, or a negative pair on a miss.
fn sky_ray_sphere(origin: vec3<f32>, dir: vec3<f32>, radius: f32) -> vec2<f32> {
    let b = dot(origin, dir);
    let c = dot(origin, origin) - radius * radius;
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return vec2(-1.0, -1.0);
    }
    let root = sqrt(discriminant);
    return vec2(-b - root, -b + root);
}

fn sky_saturate(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

fn sky_exponential_density(altitude: f32, scale_height: f32) -> f32 {
    return exp(-max(altitude, 0.0) / max(scale_height, 1.0));
}

// Ozone is concentrated in a band, approximated as a triangle profile centred
// on its layer altitude.
fn sky_ozone_density(params: SkyParams, altitude: f32) -> f32 {
    let half_width = max(params.ozone_layer_width * 0.5, 1.0);
    return sky_saturate(1.0 - abs(altitude - params.ozone_layer_altitude) / half_width);
}

fn sky_rayleigh_phase(cos_theta: f32) -> f32 {
    return 3.0 / (16.0 * SKY_PI) * (1.0 + cos_theta * cos_theta);
}

fn sky_mie_phase(params: SkyParams, cos_theta: f32) -> f32 {
    let g = params.mie_asymmetry;
    let g2 = g * g;
    let denom = max(1.0 + g2 - 2.0 * g * cos_theta, 1e-4);
    return (1.0 - g2) / (4.0 * SKY_PI * pow(denom, 1.5));
}

// Position is relative to the planet centre.
fn sky_extinction_at(params: SkyParams, position: vec3<f32>) -> vec3<f32> {
    let altitude = length(position) - params.bottom_radius;
    let rayleigh = sky_exponential_density(altitude, params.rayleigh_scale_height);
    let mie = sky_exponential_density(altitude, params.mie_scale_height);
    let ozone = sky_ozone_density(params, altitude);
    return params.rayleigh_scattering * rayleigh
        + (params.mie_scattering + params.mie_absorption) * mie
        + params.ozone_absorption * ozone;
}

// Transmittance from a sample to the top of the atmosphere along the Sun
// direction. Zero when the Sun is occluded by the planet.
fn sky_sun_transmittance(
    params: SkyParams,
    position: vec3<f32>,
    sun_samples: u32,
) -> vec3<f32> {
    let local_up = normalize(position);
    let ground = sky_ray_sphere(position, params.sun_direction, params.bottom_radius);
    if ground.x > 0.0 && dot(local_up, params.sun_direction) < 0.0 {
        return vec3(0.0);
    }
    let top = sky_ray_sphere(position, params.sun_direction, params.top_radius);
    if top.y <= 0.0 {
        return vec3(1.0);
    }
    var optical_depth = vec3(0.0);
    let step = top.y / f32(max(sun_samples, 1u));
    for (var i: u32 = 0u; i < SKY_MAX_SUN_SAMPLES; i = i + 1u) {
        if i >= sun_samples {
            break;
        }
        let t = (f32(i) + 0.5) * step;
        optical_depth += sky_extinction_at(params, position + params.sun_direction * t) * step;
    }
    return exp(-optical_depth);
}

/// Single-scattering sky radiance along `dir` from `position`.
///
/// `position` is relative to the planet centre, in the same frame as
/// `params.sun_direction`. Rays that reach the ground also carry the surface
/// radiance bounce, so a sea reflection across the horizon still returns the
/// airlight a real horizon has.
fn sky_radiance_direction(
    params: SkyParams,
    position: vec3<f32>,
    dir: vec3<f32>,
    view_samples: u32,
    sun_samples: u32,
) -> SkyRadiance {
    var result: SkyRadiance;
    result.radiance = vec3(0.0);
    result.transmittance = vec3(1.0);
    if params.top_radius <= params.bottom_radius {
        return result;
    }

    let radius = length(position);
    // The scattering angle is between the incident sunlight and the light
    // scattered toward the camera, so the phase functions use the cosine
    // between the view ray and the Sun direction (not the zenith angle).
    let cos_theta = dot(dir, params.sun_direction);

    let atmosphere_hit = sky_ray_sphere(position, dir, params.top_radius);
    let ground_hit = sky_ray_sphere(position, dir, params.bottom_radius);
    let hits_ground = ground_hit.x > 0.0;

    var segment_start = 0.0;
    var segment_end = atmosphere_hit.y;
    if radius < params.top_radius {
        segment_end = select(atmosphere_hit.y, ground_hit.x, hits_ground);
    } else {
        if atmosphere_hit.x < 0.0 {
            return result;
        }
        segment_start = atmosphere_hit.x;
        segment_end = select(atmosphere_hit.y, ground_hit.x, hits_ground);
    }
    let segment_length = max(segment_end - segment_start, 0.0);
    if segment_length <= 0.0 {
        return result;
    }

    let step = segment_length / f32(max(view_samples, 1u));
    var in_scatter = vec3(0.0);
    var transmittance = vec3(1.0);
    for (var i: u32 = 0u; i < SKY_MAX_VIEW_SAMPLES; i = i + 1u) {
        if i >= view_samples {
            break;
        }
        let t = segment_start + (f32(i) + 0.5) * step;
        let sample_position = position + dir * t;
        let altitude = length(sample_position) - params.bottom_radius;
        let rayleigh = sky_exponential_density(altitude, params.rayleigh_scale_height);
        let mie = sky_exponential_density(altitude, params.mie_scale_height);
        let ozone = sky_ozone_density(params, altitude);
        let extinction = params.rayleigh_scattering * rayleigh
            + (params.mie_scattering + params.mie_absorption) * mie
            + params.ozone_absorption * ozone;
        let scattering = params.rayleigh_scattering * rayleigh * sky_rayleigh_phase(cos_theta)
            + params.mie_scattering * mie * sky_mie_phase(params, cos_theta);
        let step_optical_depth = extinction * step;
        let step_transmittance = exp(-step_optical_depth);
        let transmittance_to_sun = sky_sun_transmittance(params, sample_position, sun_samples);
        // Analytic integral of the in-scattering across the step.
        let inscattered = scattering * transmittance_to_sun
            * (vec3(1.0) - step_transmittance) / max(extinction, vec3(1e-6));
        in_scatter += transmittance * inscattered;
        transmittance *= step_transmittance;
        if all(transmittance < vec3(0.002)) {
            break;
        }
    }

    var radiance = in_scatter * params.sun_irradiance;
    if hits_ground {
        let ground_position = position + dir * segment_end;
        let ground_normal = normalize(ground_position);
        let cos_sun = max(dot(ground_normal, params.sun_direction), 0.0);
        let ground_radiance = params.ground_albedo / SKY_PI
            * params.sun_irradiance
            * sky_sun_transmittance(params, ground_position, sun_samples)
            * cos_sun;
        radiance += transmittance * ground_radiance;
    }
    result.radiance = radiance;
    result.transmittance = transmittance;
    return result;
}
