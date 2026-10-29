// Shared cloud density field (cloud engine spec sections 3.3 and 7).
//
// The volumetric deck and the terrain's cloud shadows must agree: a shadow that
// does not line up with the cloud that casts it is worse than no shadow. Both
// shaders therefore evaluate this single density field, parameterized over the
// noise textures so it can read either material's bindings.
//
// Inputs are body-fixed positions relative to the planet centre. Rotating the
// sample point is the caller's job because the cloud shader marches in inertial
// space and converts per sample, while the terrain shader converts once and
// marches entirely in the body frame.
//
// Presentation only: nothing here feeds collision or any simulation state.

const PI: f32 = 3.141592653589793;
// Regional tiling of the periodic weather map; see `sample_density`.
const WEATHER_TILE: f32 = 48.0;

// Density-field parameters shared by the cloud material and the terrain
// material's shadow uniform. Field order and types must match `CloudField` in
// `infrastructure/bevy_adapters/rendering/materials.rs`.
struct CloudField {
    bottom_radius: f32,
    top_radius: f32,
    absorption: f32,
    time_s: f32,
    coverage: f32,
    cloud_type: f32,
    density_scale: f32,
    noise_scale: f32,
    detail_scale: f32,
    erosion_scale: f32,
    wind_speed: f32,
    wind_direction: vec2<f32>,
}

fn saturate(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

fn remap(v: f32, l1: f32, h1: f32, l2: f32, h2: f32) -> f32 {
    return l2 + (v - l1) / max(h1 - l1, 1e-6) * (h2 - l2);
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

fn weather_uv(body_dir: vec3<f32>) -> vec2<f32> {
    let u = atan2(body_dir.z, body_dir.x) / (2.0 * PI) + 0.5;
    let v = asin(clamp(body_dir.y, -1.0, 1.0)) / PI + 0.5;
    return vec2(u, v);
}

// Cloud density at a body-fixed, planet-relative position, in [0, 1].
fn sample_density(
    field: CloudField,
    base_noise: texture_3d<f32>,
    base_noise_sampler: sampler,
    detail_noise: texture_3d<f32>,
    detail_noise_sampler: sampler,
    weather_map: texture_2d<f32>,
    weather_map_sampler: sampler,
    body_pos: vec3<f32>,
) -> f32 {
    let altitude = length(body_pos) - field.bottom_radius;
    let layer = max(field.top_radius - field.bottom_radius, 1.0);
    let height_fraction = saturate(altitude / layer);
    if altitude < 0.0 || altitude > layer {
        return 0.0;
    }

    let wind_offset = vec3<f32>(field.wind_direction.x, 0.0, field.wind_direction.y)
        * (field.time_s * field.wind_speed * field.noise_scale);

    // Weather map: coverage carves clear air, cloud type morphs the profile.
    // The map's fBm spans the whole sphere, so one tile puts every feature at
    // synoptic scale (thousands of km) and a flight view sees a constant value.
    // Sampling the periodic map at an integer tiling factor adds regional
    // structure; integer multiples keep the longitude and pole seams closed,
    // and scaling the whole UV (drift included) preserves the wind's physical
    // speed. WEATHER_TILE * 3 fBm cells keeps the largest features near 300 km.
    // Coverage and the density field must advect at the same measured wind
    // speed. The weather map is a body-fixed equirectangular field, so one UV
    // unit spans the equatorial circumference; dividing by it converts metres
    // per second into UV per second (latitude foreshortening is ignored, which
    // is a presentation approximation).
    let body_dir = normalize(body_pos);
    let weather_drift = field.wind_direction * field.wind_speed * field.time_s
        / (2.0 * PI * max(field.bottom_radius, 1.0));
    let weather = textureSample(
        weather_map,
        weather_map_sampler,
        (weather_uv(body_dir) + weather_drift) * WEATHER_TILE,
    );
    let coverage = saturate(field.coverage * weather.r * 1.4);
    if coverage < 0.01 {
        return 0.0;
    }
    let cloud_type = saturate(mix(field.cloud_type, weather.b, 0.5));
    let gradient = height_profile(height_fraction, cloud_type);
    if gradient <= 0.001 {
        return 0.0;
    }

    let sample_pos = body_pos * field.noise_scale + wind_offset;
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
        sample_pos * field.detail_scale + wind_offset * 1.5,
    );
    let high_fbm = detail.r * 0.625 + detail.g * 0.25 + detail.b * 0.125;
    let erosion = mix(high_fbm, 1.0 - high_fbm, saturate(height_fraction * 2.0));
    let final_density = saturate(remap(
        density_with_coverage,
        erosion * field.erosion_scale * 0.55,
        1.0,
        0.0,
        1.0,
    ));
    return saturate(final_density * field.density_scale);
}
