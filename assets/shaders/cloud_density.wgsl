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
    // Share of the sky the deck covers, thresholded against the weather map's
    // rank channel by `weather_coverage`.
    coverage: f32,
    cloud_type: f32,
    // How far the weather map's type rank may move this deck's morph from
    // `cloud_type`; see `sample_density`.
    morph_variation: f32,
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
    // Beam transmittance of the atmosphere above the deck.
    sun_transmittance: vec3<f32>,
    // Inertial-to-body rotation as an xyzw quaternion.
    inertial_to_body: vec4<f32>,
}

// Steps along a ray through the deck, used both for the Sun path that decides a
// shadow and for the mirror path the sea reflects. The deck is a few kilometres
// thick and the noise field's features are sub-kilometre, so this is a coarse
// estimate of the beam's optical depth: enough to place a shadow under each
// cloud mass, not enough to resolve individual cell edges.
const CLOUD_DECK_MARCH_STEPS: u32 = 6u;
// Single-scattering albedo of cloud droplets at visible wavelengths. Water
// absorbs almost nothing, so droplets scatter conservatively (spec section 4:
// 0.99-1.0). Must match `CLOUD_SINGLE_SCATTER_ALBEDO` in rendering/materials.rs.
const CLOUD_ALBEDO: f32 = 0.99;
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

/// Local fraction of the sky the deck covers, in [0, 1].
///
/// The measured regional cloud fraction is a share of the sky, and the coverage
/// channel of the weather map is baked as a rank (uniform over the tile), so the
/// fraction is used as a threshold on that rank: everything below
/// `1 - fraction` is genuinely clear air, and the band above it ramps from a
/// thin edge to the dense core of a system. Scaling the field instead (`coverage
/// * weather * 1.4`) left almost no clear sky, because an fBm concentrates its
/// values near the mean and the product therefore saturates everywhere.
fn weather_coverage(field: CloudField, weather_rank: f32) -> f32 {
    let fraction = saturate(field.coverage);
    let clear_below = 1.0 - fraction;
    return saturate((weather_rank - clear_below) / max(fraction, 0.02));
}

// How far the density field is stretched along the wind. Convection in the
// boundary layer organizes into streets and rolls aligned with the wind, and an
// isotropic field reads as uniform popcorn no matter how correct its coverage
// is. This is a presentation approximation: the wind uniform is one measured
// site value, not a varying field, so the street axis does not turn with
// latitude.
const CLOUD_STREAK_STRETCH: f32 = 2.4;

/// Compress the horizontal part of a sample coordinate along `wind`, so the
/// features in a field sampled at the result are `stretch` times longer along
/// the wind than across it. The vertical coordinate is untouched.
///
/// This is a linear map of the sample coordinate, so the noise volume stays
/// seamless: its period simply grows by the same factor along the wind. The
/// caller's coordinate is body-fixed Cartesian, whose y axis is the spin axis,
/// so "horizontal" here is the equatorial plane; near the poles that plane is
/// not the local horizon, which is accepted as part of the same global-wind
/// approximation.
fn stretch_along_wind(p: vec3<f32>, wind: vec2<f32>, stretch: f32) -> vec3<f32> {
    let wind_length = length(wind);
    if wind_length < 1e-4 {
        return p;
    }
    let dir = wind / wind_length;
    let horizontal = p.xz;
    let along = dot(horizontal, dir);
    let stretched = horizontal - dir * along + dir * (along / max(stretch, 1.0));
    return vec3<f32>(stretched.x, p.y, stretched.y);
}

/// Mip level a march step of `step_m` can resolve.
///
/// The base volume holds six cells per tile, so one cell is
/// `1 / (noise_scale * 6)` metres across. A step shorter than a cell resolves
/// level zero; a step twice a cell resolves only the first mip, and so on. The
/// level is not clamped from above: past the last mip the sampler returns the
/// mean, which is exactly the right answer for a deck that no longer has
/// resolvable structure.
fn density_lod(field: CloudField, step_m: f32) -> f32 {
    let cell_m = 1.0 / max(field.noise_scale * 6.0, 1e-9);
    return max(log2(max(step_m / cell_m, 1.0)), 0.0);
}

fn weather_uv(body_dir: vec3<f32>) -> vec2<f32> {
    let u = atan2(body_dir.z, body_dir.x) / (2.0 * PI) + 0.5;
    let v = asin(clamp(body_dir.y, -1.0, 1.0)) / PI + 0.5;
    return vec2(u, v);
}

// Cloud density at a body-fixed, planet-relative position, in [0, 1].
//
// `lod` picks the noise volume's mip level. A ray march through a distant deck
// takes steps kilometres long, and a step that long cannot resolve a 4 km cell:
// sampling the finest mip there aliases the field into blocky silhouettes, most
// visibly along the limb where one step spans the whole cell. Callers pass the
// level their step length can actually resolve, so the far deck is drawn from
// the same field at the scale it is being seen.
fn sample_density(
    field: CloudField,
    base_noise: texture_3d<f32>,
    base_noise_sampler: sampler,
    detail_noise: texture_3d<f32>,
    detail_noise_sampler: sampler,
    weather_map: texture_2d<f32>,
    weather_map_sampler: sampler,
    body_pos: vec3<f32>,
    lod: f32,
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
    // The weather map's features are synoptic (tens to hundreds of kilometres),
    // so even a kilometre-long march step resolves it and it keeps level zero.
    let weather = textureSampleLevel(
        weather_map,
        weather_map_sampler,
        (weather_uv(body_dir) + weather_drift) * WEATHER_TILE,
        0.0,
    );
    let coverage = weather_coverage(field, weather.r);
    if coverage < 0.01 {
        return 0.0;
    }
    // Morphology is regional, not global. Blue is the cloud-type rank: because
    // it is a rank, a threshold on it selects a share of the deck rather than a
    // slice of one narrow band, so one deck holds flat sheets, broken cells and
    // towering cumulus in different places instead of one shape everywhere.
    // Green is the convective-vigour rank: the top tenth of the map converts
    // actively, and the third above 0.55 is broken cumulus. Where convection is
    // deep the vertical profile is forced to its fullest, so towers rise out of
    // the deck at the weather that is actually raining.
    let region_type = mix(field.cloud_type, weather.b, field.morph_variation);
    let convection =
        saturate(remap(weather.g, 0.55, 0.9, 0.0, 1.0)) * field.morph_variation;
    let cloud_type = saturate(region_type + 0.4 * convection);
    let gradient = height_profile(height_fraction, cloud_type);
    if gradient <= 0.001 {
        return 0.0;
    }

    let sample_pos = body_pos * field.noise_scale + wind_offset;
    let base = textureSampleLevel(
        base_noise,
        base_noise_sampler,
        stretch_along_wind(sample_pos, field.wind_direction, CLOUD_STREAK_STRETCH),
        lod,
    );
    let low_fbm = base.g * 0.625 + base.b * 0.25 + base.a * 0.125;
    var base_density = remap(base.r, low_fbm - 1.0, 1.0, 0.0, 1.0);
    base_density = base_density * gradient;
    let density_with_coverage = saturate(remap(base_density, 1.0 - coverage, 1.0, 0.0, 1.0));
    if density_with_coverage <= 0.001 {
        return 0.0;
    }

    // The erosion follows the same street axis, so cell edges are fibrous
    // along the wind instead of randomly ragged.
    let detail = textureSampleLevel(
        detail_noise,
        detail_noise_sampler,
        stretch_along_wind(
            sample_pos * field.detail_scale + wind_offset * 1.5,
            field.wind_direction,
            CLOUD_STREAK_STRETCH,
        ),
        lod,
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

/// Gate on how far the Sun is above a sample's local horizon: direct sunlight
/// only reaches a sample while the Sun is up. At cloud altitude the geometric
/// horizon sits slightly below the tangent plane (`acos(sqrt(1 - (R/(R+h))^2))`
/// is about 1.8 degrees at 3 km), so the gate opens a little below zero and
/// fades through civil twilight. Without it the sun march reports a clear path
/// for a below-horizon Sun and lights the night-side deck with the full solar
/// irradiance.
fn cloud_sun_gate(cos_elevation: f32) -> f32 {
    return smoothstep(-0.05, 0.02, cos_elevation);
}

/// Density-length (density times path length, in the field's units) along
/// `dir_body` from a point below the deck out through its top. Zero when the
/// point is not under the deck or the ray misses it.
///
/// This is the one march both the ground shadow (along the Sun) and the sea's
/// mirror path (along the reflected view) use, so a reflected deck has exactly
/// the opacity of the deck drawn above it.
fn cloud_deck_density_length(
    field: CloudField,
    body_pos: vec3<f32>,
    dir_body: vec3<f32>,
    base_noise: texture_3d<f32>,
    base_noise_sampler: sampler,
    detail_noise: texture_3d<f32>,
    detail_noise_sampler: sampler,
    weather_map: texture_2d<f32>,
    weather_map_sampler: sampler,
) -> f32 {
    if field.bottom_radius <= 0.0 || dot(dir_body, dir_body) < 1e-6 {
        return 0.0;
    }
    // A deck below the fragment cannot shadow it, and a ray that starts inside
    // the shell is not the geometry either caller asks about.
    if length(body_pos) >= field.bottom_radius {
        return 0.0;
    }
    let dir = normalize(dir_body);
    let bottom = ray_sphere(body_pos, dir, field.bottom_radius);
    let top = ray_sphere(body_pos, dir, field.top_radius);
    // The fragment sits inside both spheres, so the ray leaves the clear air
    // below the base at the far root of the base sphere and leaves the deck at
    // the far root of the top sphere. Both roots are the near-side solutions
    // for a point inside the sphere, a few kilometres apart at deck altitude.
    let enter = bottom.y;
    let exit = top.y;
    if enter <= 0.0 || exit <= enter {
        return 0.0;
    }
    let steps = f32(CLOUD_DECK_MARCH_STEPS);
    let step_length = (exit - enter) / steps;
    let lod = density_lod(field, step_length);
    var density_length = 0.0;
    for (var i: u32 = 0u; i < CLOUD_DECK_MARCH_STEPS; i = i + 1u) {
        let sample_pos = body_pos + dir * (enter + (f32(i) + 0.5) * step_length);
        density_length += sample_density(
            field,
            base_noise,
            base_noise_sampler,
            detail_noise,
            detail_noise_sampler,
            weather_map,
            weather_map_sampler,
            sample_pos,
            lod,
        ) * step_length;
    }
    return density_length;
}

/// Radiance an optically thick deck returns downward toward a surface below it.
///
/// Cloud droplets are nearly conservative scatterers, so a thick deck reflects
/// most of the sunlight it intercepts as a near-Lambertian layer:
/// `albedo * E * cos(sun zenith) / PI`, tinted white at the sunlit top. The sea
/// uses this as the radiance in its mirror direction whenever the deck is in
/// the way, which is what stops an overcast sea from reflecting a blue sky that
/// is not there. The forward-scattering lobe is deliberately omitted: from
/// below, a water cloud's diffuse transmission dominates its directed lobe.
fn cloud_deck_diffuse_radiance(
    field: CloudField,
    sun_direction_body: vec3<f32>,
    sun_irradiance: vec3<f32>,
    sun_transmittance: vec3<f32>,
    surface_position_body: vec3<f32>,
) -> vec3<f32> {
    if field.bottom_radius <= 0.0 {
        return vec3(0.0);
    }
    let sun_elevation = dot(normalize(surface_position_body), sun_direction_body);
    let sun_cos = saturate(sun_elevation);
    return sun_irradiance * sun_transmittance * (CLOUD_ALBEDO / PI) * sun_cos
        * cloud_sun_gate(sun_elevation);
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
    let density_length = cloud_deck_density_length(
        field,
        body_pos,
        sun_body,
        base_noise,
        base_noise_sampler,
        detail_noise,
        detail_noise_sampler,
        weather_map,
        weather_map_sampler,
    );
    let transmittance = exp(-density_length * field.absorption);
    return mix(CLOUD_SHADOW_DIFFUSE_FLOOR, 1.0, transmittance);
}
