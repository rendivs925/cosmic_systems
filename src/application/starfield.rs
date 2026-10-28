use std::f32::consts::{PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_mesh::{Indices, PrimitiveTopology};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::domain::services::star_catalog::{galactic_frame_solar_inertial, StarCatalog};
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::entity_components::{FlightStarfield, Starfield};

const STAR_SEED: u64 = 0xC05F_1C5A;
/// Procedural fallback layers, used only when the catalog cannot be decoded.
const NEAR_STAR_COUNT: usize = 2_000;
const FAR_STAR_COUNT: usize = 8_000;
const MILKY_WAY_COUNT: usize = 3_000;
const MILKY_WAY_GLOW_COUNT: usize = 80;
const STARFIELD_NEAR_RADIUS_AU: f32 = 65.0;
const STARFIELD_FAR_RADIUS_AU: f32 = 110.0;
const BRIGHT_STAR_GLOW_THRESHOLD: f32 = 0.93;
/// Apparent magnitude below which a cataloged star receives a soft halo.
const CATALOG_GLOW_MAG: f32 = 1.5;
/// Brightest and faintest magnitudes spanned by the brightness ramp. The
/// faintest cataloged star (V = 7.96) is invisible; V = 7.2 is the sky limit.
const FAINTEST_VISIBLE_MAG: f32 = 7.2;
const BRIGHTEST_MAG_RANGE: f32 = 8.7;
/// Rendering star sprites as masked geometry keeps them in the depth-writing
/// opaque pass, so planets reliably occlude stars behind their discs.
const STAR_ALPHA_CUTOFF: f32 = 0.01;

/// Mark the shared solar-map star mesh for camera-relative flight presentation.
pub fn configure_flight_starfield(
    mut commands: Commands,
    starfield_query: Query<Entity, With<Starfield>>,
) {
    for entity in &starfield_query {
        commands.entity(entity).insert(FlightStarfield);
    }
}

/// Keep the solar-map star distribution inside the current flight camera's
/// depth range. Scaling the mesh preserves each star's angular size while the
/// current camera transform prevents rebase and camera-transition jitter.
#[expect(
    clippy::type_complexity,
    reason = "The query proves that the active flight camera and star mesh are disjoint."
)]
pub fn update_flight_starfield(
    solar_params: Res<SolarSystemParameters>,
    camera_query: Query<(&Camera, &Transform, &Projection), (With<Camera3d>, Without<Starfield>)>,
    mut starfield_query: Query<&mut Transform, (With<Starfield>, With<FlightStarfield>)>,
) {
    let Some((_, camera_transform, projection)) =
        camera_query.iter().find(|(camera, _, _)| camera.is_active)
    else {
        return;
    };
    let Projection::Perspective(projection) = projection else {
        return;
    };
    let source_radius = solar_params.au_to_units(STARFIELD_FAR_RADIUS_AU).max(1.0);
    let scale = (projection.far * 0.92 / source_radius).max(f32::MIN_POSITIVE);
    for mut starfield_transform in &mut starfield_query {
        starfield_transform.translation = camera_transform.translation;
        starfield_transform.scale = Vec3::splat(scale);
    }
}

pub fn spawn_starfield(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    solar_params: &SolarSystemParameters,
) {
    let near_radius = solar_params.au_to_units(STARFIELD_NEAR_RADIUS_AU);
    let far_radius = solar_params.au_to_units(STARFIELD_FAR_RADIUS_AU);
    let catalog = match StarCatalog::embedded_yale_bsc5() {
        Ok(catalog) => {
            info!("starfield: {} measured stars from Yale BSC5", catalog.len());
            Some(catalog)
        }
        Err(error) => {
            warn!("star catalog unavailable ({error:?}); using procedural stars");
            None
        }
    };
    let mesh = meshes.add(create_starfield_mesh(
        near_radius,
        far_radius,
        catalog.as_ref(),
    ));
    let star_texture = images.add(create_gaussian_star_texture());
    let material = materials.add(starfield_material(star_texture));

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::default(),
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
        Starfield,
        Name::new(if catalog.is_some() {
            "Yale BSC5 Starfield"
        } else {
            "Procedural Starfield"
        }),
    ));
}

fn starfield_material(star_texture: Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(star_texture),
        emissive: LinearRgba::rgb(0.25, 0.28, 0.34),
        unlit: true,
        alpha_mode: AlphaMode::Mask(STAR_ALPHA_CUTOFF),
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

fn create_starfield_mesh(near_radius: f32, far_radius: f32, catalog: Option<&StarCatalog>) -> Mesh {
    let total = NEAR_STAR_COUNT + FAR_STAR_COUNT + MILKY_WAY_COUNT + MILKY_WAY_GLOW_COUNT;
    let mut positions = Vec::with_capacity(total * 4);
    let mut normals = Vec::with_capacity(total * 4);
    let mut uvs = Vec::with_capacity(total * 4);
    let mut colors = Vec::with_capacity(total * 4);
    let mut indices = Vec::with_capacity(total * 6);
    let mut rng = StdRng::seed_from_u64(STAR_SEED);

    if let Some(catalog) = catalog {
        push_cataloged_stars(
            catalog,
            far_radius,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut colors,
            &mut indices,
        );
    } else {
        for _ in 0..NEAR_STAR_COUNT {
            let profile = star_profile(&mut rng, 1.0);
            push_star_with_optional_glow(
                random_unit_vector(&mut rng),
                near_radius,
                5_000.0 + profile.magnitude * 11_000.0,
                profile,
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut colors,
                &mut indices,
            );
        }

        for _ in 0..FAR_STAR_COUNT {
            let profile = star_profile(&mut rng, 0.62);
            push_star_with_optional_glow(
                random_unit_vector(&mut rng),
                far_radius,
                3_000.0 + profile.magnitude * 8_500.0,
                profile,
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut colors,
                &mut indices,
            );
        }
    }

    for _ in 0..MILKY_WAY_COUNT {
        let Some((direction, core_bias)) = milky_way_direction(&mut rng) else {
            continue;
        };
        let profile = milky_way_star_profile(&mut rng, core_bias);
        let radius = rng.gen_range(near_radius..far_radius);
        push_star_with_optional_glow(
            direction,
            radius,
            3_500.0 + profile.magnitude * 13_500.0,
            profile,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut colors,
            &mut indices,
        );
    }

    for _ in 0..MILKY_WAY_GLOW_COUNT {
        let (direction, core_bias) = milky_way_glow_direction(&mut rng);
        let alpha = 0.012 + core_bias * 0.032;
        let color = milky_way_glow_color(core_bias, alpha);
        push_star_quad(
            direction,
            far_radius * 0.98,
            rng.gen_range(110_000.0..330_000.0),
            color,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut colors,
            &mut indices,
        );
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn create_gaussian_star_texture() -> Image {
    const TEXTURE_SIZE: u32 = 128;
    const CHANNELS: usize = 4;

    let mut pixels = Vec::with_capacity((TEXTURE_SIZE * TEXTURE_SIZE) as usize * CHANNELS);
    let max = (TEXTURE_SIZE - 1) as f32;

    for y in 0..TEXTURE_SIZE {
        for x in 0..TEXTURE_SIZE {
            let u = x as f32 / max * 2.0 - 1.0;
            let v = y as f32 / max * 2.0 - 1.0;
            let radius = (u * u + v * v).sqrt();
            let core = (-radius * radius * 5.8).exp();
            let edge_fade = 1.0 - smoothstep(0.72, 1.0, radius);
            let alpha = (core * edge_fade).clamp(0.0, 1.0);

            pixels.extend_from_slice(&[255, 255, 255, (alpha * 255.0).round() as u8]);
        }
    }

    Image::new(
        Extent3d {
            width: TEXTURE_SIZE,
            height: TEXTURE_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Copy)]
struct StarProfile {
    color: [f32; 4],
    magnitude: f32,
}

#[derive(Clone, Copy)]
enum SpectralClass {
    O,
    B,
    A,
    F,
    G,
    K,
    M,
}

fn random_unit_vector(rng: &mut StdRng) -> Vec3 {
    let z = rng.gen_range(-1.0..1.0);
    let theta = rng.gen_range(0.0..TAU);
    let radius = (1.0_f32 - z * z).sqrt();
    Vec3::new(radius * theta.cos(), z, radius * theta.sin())
}

fn milky_way_direction(rng: &mut StdRng) -> Option<(Vec3, f32)> {
    // The band is generated in IAU galactic coordinates so it passes through
    // the real Cygnus/Sagittarius star clouds and agrees with the measured
    // star positions. Longitude zero is the galactic centre.
    let frame = galactic_frame_solar_inertial();
    let longitude: f32 = rng.gen_range(-PI..PI);
    let core_bias = longitude.cos().max(0.0).powf(2.0);
    let band_width = 0.105 + core_bias * 0.045;
    let latitude: f32 = rng.gen_range(-band_width..band_width) + rng.gen_range(-0.025..0.025);

    if is_dust_lane(longitude, latitude, rng) {
        return None;
    }

    Some((
        frame
            .direction(f64::from(longitude), f64::from(latitude))
            .as_vec3(),
        core_bias,
    ))
}

fn is_dust_lane(longitude: f32, latitude: f32, rng: &mut StdRng) -> bool {
    let primary_lane = (latitude + 0.035 * (longitude * 2.2).sin()).abs() < 0.022;
    let secondary_lane = (latitude - 0.05 * (longitude * 1.35 + 0.6).sin()).abs() < 0.012;
    let lane_probability = if primary_lane {
        0.62
    } else if secondary_lane {
        0.38
    } else {
        0.0
    };
    rng.gen_bool(lane_probability)
}

fn milky_way_glow_direction(rng: &mut StdRng) -> (Vec3, f32) {
    let frame = galactic_frame_solar_inertial();
    let longitude: f32 = rng.gen_range(-PI..PI);
    let core_bias = longitude.cos().max(0.0).powf(2.0);
    let latitude: f32 = rng.gen_range(-0.16..0.16) + rng.gen_range(-0.04..0.04);
    (
        frame
            .direction(f64::from(longitude), f64::from(latitude))
            .as_vec3(),
        core_bias,
    )
}

fn milky_way_glow_color(core_bias: f32, alpha: f32) -> [f32; 4] {
    let base = Color::srgb(
        0.18 + core_bias * 0.30,
        0.20 + core_bias * 0.18,
        0.34 - core_bias * 0.08,
    );
    let linear: LinearRgba = base.into();
    [linear.red, linear.green, linear.blue, alpha]
}

fn star_profile(rng: &mut StdRng, layer_brightness: f32) -> StarProfile {
    let class = random_spectral_class(rng);
    let magnitude = apparent_magnitude(rng) * spectral_visibility(class);
    let alpha = (0.08 + magnitude * 0.78) * layer_brightness;
    StarProfile {
        color: spectral_color(class, alpha.clamp(0.045, 0.92)),
        magnitude,
    }
}

fn milky_way_star_profile(rng: &mut StdRng, core_bias: f32) -> StarProfile {
    let class = random_spectral_class(rng);
    let magnitude = (apparent_magnitude(rng) * spectral_visibility(class) * 0.72
        + core_bias * 0.28)
        .clamp(0.0, 1.0);
    let alpha = 0.05 + magnitude * 0.34;
    let mut color = spectral_color(class, alpha);
    color[0] = (color[0] + core_bias * 0.20).min(1.0);
    color[1] = (color[1] + core_bias * 0.11).min(1.0);
    color[2] = (color[2] * (1.0 - core_bias * 0.16)).max(0.0);
    StarProfile { color, magnitude }
}

fn apparent_magnitude(rng: &mut StdRng) -> f32 {
    let dim_cluster = rng.gen_range(0.0..1.0_f32).powf(3.0);
    let bright_tail = if rng.gen_bool(0.035) {
        rng.gen_range(0.75..1.0)
    } else {
        0.0
    };
    (dim_cluster * 0.78 + bright_tail * 0.22).clamp(0.0, 1.0)
}

fn random_spectral_class(rng: &mut StdRng) -> SpectralClass {
    let roll = rng.gen_range(0.0..1.0_f32);
    match roll {
        roll if roll < 0.000_003 => SpectralClass::O,
        roll if roll < 0.001_3 => SpectralClass::B,
        roll if roll < 0.007_3 => SpectralClass::A,
        roll if roll < 0.037_3 => SpectralClass::F,
        roll if roll < 0.112_3 => SpectralClass::G,
        roll if roll < 0.232_3 => SpectralClass::K,
        _ => SpectralClass::M,
    }
}

fn spectral_color(class: SpectralClass, alpha: f32) -> [f32; 4] {
    let base = match class {
        SpectralClass::O => Color::srgb(0.62, 0.72, 1.0),
        SpectralClass::B => Color::srgb(0.72, 0.82, 1.0),
        SpectralClass::A => Color::srgb(0.88, 0.92, 1.0),
        SpectralClass::F => Color::srgb(1.0, 0.96, 0.86),
        SpectralClass::G => Color::srgb(1.0, 0.88, 0.66),
        SpectralClass::K => Color::srgb(1.0, 0.70, 0.42),
        SpectralClass::M => Color::srgb(1.0, 0.48, 0.34),
    };
    let linear: LinearRgba = base.into();
    [linear.red, linear.green, linear.blue, alpha]
}

fn spectral_visibility(class: SpectralClass) -> f32 {
    match class {
        SpectralClass::O | SpectralClass::B | SpectralClass::A => 1.0,
        SpectralClass::F => 0.92,
        SpectralClass::G => 0.78,
        SpectralClass::K => 0.58,
        SpectralClass::M => 0.36,
    }
}

/// Convert apparent V magnitude into a normalized visual weight. The ramp is
/// anchored at the naked-eye limit so a real bright star always outshines any
/// procedural density filler.
fn apparent_brightness(v_mag: f32) -> f32 {
    ((FAINTEST_VISIBLE_MAG - v_mag) / BRIGHTEST_MAG_RANGE).clamp(0.0, 1.0)
}

/// Approximate an sRGB star colour from the B-V colour index.
///
/// Effective temperature follows the Ballesteros (2012) B-V relation; the
/// temperature-to-sRGB fit is the commonly used Tanner Helland approximation.
/// Both are presentation approximations, not calibrated photometry. The source
/// catalog publishes spectral types rather than measured B-V, so the ingest
/// step already documents that approximation.
fn bv_to_color(bv: f32, alpha: f32) -> [f32; 4] {
    let bv = f64::from(bv.clamp(-0.4, 2.2));
    let temperature_k = 4600.0 * (1.0 / (0.92 * bv + 1.70) + 1.0 / (0.92 * bv + 0.62));
    let t = (temperature_k / 100.0).clamp(10.0, 400.0);
    let red = if t <= 66.0 {
        255.0
    } else {
        329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2)
    };
    let green = if t <= 66.0 {
        99.470_802_586_1 * t.ln() - 161.119_568_166_1
    } else {
        288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2)
    };
    let blue = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7
    };
    let linear: LinearRgba = Color::srgb(
        (red / 255.0).clamp(0.0, 1.0) as f32,
        (green / 255.0).clamp(0.0, 1.0) as f32,
        (blue / 255.0).clamp(0.0, 1.0) as f32,
    )
    .into();
    [linear.red, linear.green, linear.blue, alpha]
}

#[allow(clippy::too_many_arguments)]
fn push_cataloged_stars(
    catalog: &StarCatalog,
    radius: f32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    for star in catalog.stars() {
        let brightness = apparent_brightness(star.v_mag);
        if brightness <= 0.0 {
            continue;
        }
        let direction = star.solar_inertial_direction().as_vec3();
        if direction.length_squared() < 0.5 {
            continue;
        }
        let alpha = (0.055 + 0.88 * brightness.powf(1.9)).clamp(0.05, 0.95);
        let color = bv_to_color(star.bv_color, alpha);
        let size = 2_400.0 + brightness.powf(1.5) * 13_000.0;
        push_star_quad(
            direction, radius, size, color, positions, normals, uvs, colors, indices,
        );
        if star.v_mag <= CATALOG_GLOW_MAG {
            let glow_alpha = (((CATALOG_GLOW_MAG - star.v_mag) / 3.0) * 0.10).clamp(0.0, 0.10);
            push_star_quad(
                direction,
                radius,
                size * 3.4,
                [color[0], color[1], color[2], glow_alpha],
                positions,
                normals,
                uvs,
                colors,
                indices,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_star_with_optional_glow(
    direction: Vec3,
    radius: f32,
    size: f32,
    profile: StarProfile,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    push_star_quad(
        direction,
        radius,
        size,
        profile.color,
        positions,
        normals,
        uvs,
        colors,
        indices,
    );

    if profile.magnitude < BRIGHT_STAR_GLOW_THRESHOLD {
        return;
    }

    let glow_alpha = (profile.magnitude - BRIGHT_STAR_GLOW_THRESHOLD) * 1.55;
    let glow_color = [
        profile.color[0],
        profile.color[1],
        profile.color[2],
        glow_alpha.min(0.11),
    ];
    push_star_quad(
        direction,
        radius,
        size * 3.8,
        glow_color,
        positions,
        normals,
        uvs,
        colors,
        indices,
    );
}

#[allow(clippy::too_many_arguments)]
fn push_star_quad(
    direction: Vec3,
    radius: f32,
    size: f32,
    color: [f32; 4],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    let center = direction * radius;
    let right = direction.any_orthonormal_vector() * size;
    let up = direction.cross(right).normalize_or_zero() * size;
    let base_index = positions.len() as u32;
    let normal = (-direction).to_array();

    for (offset, uv) in [
        (-right - up, [0.0, 0.0]),
        (right - up, [1.0, 0.0]),
        (right + up, [1.0, 1.0]),
        (-right + up, [0.0, 1.0]),
    ] {
        positions.push((center + offset).to_array());
        normals.push(normal);
        uvs.push(uv);
        colors.push(color);
    }

    indices.extend_from_slice(&[
        base_index,
        base_index + 1,
        base_index + 2,
        base_index,
        base_index + 2,
        base_index + 3,
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn starfield_material_writes_depth_for_planetary_occlusion() {
        assert!(matches!(
            starfield_material(Handle::default()).alpha_mode,
            AlphaMode::Mask(cutoff) if cutoff == STAR_ALPHA_CUTOFF
        ));
    }

    #[test]
    fn apparent_brightness_is_monotonic_and_saturates_like_the_eye() {
        assert!(apparent_brightness(-1.46) > apparent_brightness(0.0));
        assert!(apparent_brightness(0.0) > apparent_brightness(4.0));
        assert!(apparent_brightness(4.0) > apparent_brightness(6.5));
        assert_eq!(apparent_brightness(7.96), 0.0);
        assert_eq!(apparent_brightness(-2.0), 1.0);
    }

    #[test]
    fn bluer_colour_indices_produce_bluer_stars() {
        let blue = bv_to_color(0.0, 1.0);
        let red = bv_to_color(1.8, 1.0);
        assert!(blue[2] / blue[0] > red[2] / red[0]);
        for channel in bv_to_color(0.65, 1.0) {
            assert!(channel.is_finite());
        }
    }

    #[test]
    fn cataloged_mesh_contains_a_quad_per_visible_star() {
        let catalog = StarCatalog::embedded_yale_bsc5().unwrap();
        let mesh = create_starfield_mesh(6_500.0, 11_000.0, Some(&catalog));
        assert!(mesh.count_vertices() >= catalog.len() * 4);
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
        let VertexAttributeValues::Float32x3(values) = positions else {
            panic!("star positions must be f32x3");
        };
        assert!(values.iter().flatten().all(|value| value.is_finite()));
    }

    #[test]
    fn procedural_fallback_survives_a_missing_catalog() {
        let mesh = create_starfield_mesh(6_500.0, 11_000.0, None);
        assert!(mesh.count_vertices() > 0);
    }
}
