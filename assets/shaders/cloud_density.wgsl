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

// Cloud-shadow state shared by the ground materials (terrain and water).
// Field order and types must match `CloudShadow` in
// `infrastructure/bevy_adapters/rendering/materials.rs`.
struct CloudShadow {
    field: CloudField,
    // Unit direction from the planet toward the Sun, inertial frame.
    sun_direction: vec3<f32>,
    // Inertial-to-body rotation as an xyzw quaternion.
    inertial_to_body: vec4<f32>,
}

// Steps along the Sun ray through the deck. The deck is a few kilometres thick
// and the noise field's features are sub-kilometre, so this is a coarse
// estimate of the beam's optical depth: enough to place a shadow under each
// cloud mass, not enough to resolve individual cell edges.
const CLOUD_SHADOW_STEPS: u32 = 6u;
// A real overcast deck still delivers about a fifth of the clear-sky irradiance
// as diffuse skylight. Neither ground material injects the deck's hemisphere
// radiance into the scene ambient, so keeping that fraction of the blocked beam
// stands in for it; without this an overcast day is far darker than reality.
// Presentation approximation only.
const CLOUD_SHADOW_DIFFUSE_FLOOR: f32 = 0.2;

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

/// Rotate `v` by the unit quaternion `q` (xyz = axis*sin, w = cos).
fn rotate_by_quat(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

/// Fraction of direct sunlight that survives the cloud deck between a ground
/// point and the Sun, in [0, 1].
///
/// `planet_relative_pos` is the ground position in the inertial render frame,
/// relative to the planet centre; the function rotates it and the Sun into the
/// body-fixed frame the density field lives in. The march uses the same field
/// and the same extinction coefficient the volumetric deck is drawn with, so a
/// shadow is exactly as opaque as the cloud above it. The result includes the
/// documented diffuse floor. Presentation only: it scales the direct-sun term
/// and never touches terrain height, collision, altitude, or simulation state.
fn cloud_shadow_visibility(
    shadow: CloudShadow,
    planet_relative_pos: vec3<f32>,
    base_noise: texture_3d<f32>,
    base_noise_sampler: sampler,
    detail_noise: texture_3d<f32>,
    detail_noise_sampler: sampler,
    weather_map: texture_2d<f32>,
    weather_map_sampler: sampler,
) -> f32 {
    // No deck reported yet, or a degenerate Sun direction.
    if shadow.field.bottom_radius <= 0.0
        || dot(shadow.sun_direction, shadow.sun_direction) < 1e-6 {
        return 1.0;
    }
    let body_pos = rotate_by_quat(shadow.inertial_to_body, planet_relative_pos);
    let sun_body = normalize(rotate_by_quat(shadow.inertial_to_body, shadow.sun_direction));
    let field = shadow.field;
    // A deck below the fragment cannot shadow it.
    if length(body_pos) >= field.bottom_radius {
        return 1.0;
    }
    let bottom = ray_sphere(body_pos, sun_body, field.bottom_radius);
    let top = ray_sphere(body_pos, sun_body, field.top_radius);
    // The fragment sits inside both spheres, so the ray leaves the clear air
    // below the base at the far root of the base sphere and leaves the deck at
    // the far root of the top sphere. Both roots are the near-side solutions
    // for a point inside the sphere, a few kilometres apart at deck altitude.
    let enter = bottom.y;
    let exit = top.y;
    if enter <= 0.0 || exit <= enter {
        return 1.0;
    }
    let steps = f32(CLOUD_SHADOW_STEPS);
    let step_length = (exit - enter) / steps;
    var density_length = 0.0;
    for (var i: u32 = 0u; i < CLOUD_SHADOW_STEPS; i = i + 1u) {
        let sample_pos = body_pos + sun_body * (enter + (f32(i) + 0.5) * step_length);
        density_length += sample_density(
            field,
            base_noise,
            base_noise_sampler,
            detail_noise,
            detail_noise_sampler,
            weather_map,
            weather_map_sampler,
            sample_pos,
        ) * step_length;
    }
    let transmittance = exp(-density_length * field.absorption);
    return mix(CLOUD_SHADOW_DIFFUSE_FLOOR, 1.0, transmittance);
}
