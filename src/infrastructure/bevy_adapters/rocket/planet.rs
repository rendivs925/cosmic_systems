//! Rocket-mode planet system.
//!
//! In rocket mode, the camera is at the rocket's position (flight units = meters).
//! This system spawns and positions the bound planet (Earth), its moons, and the
//! Sun in flight units using real textures. Their orbital presentation is
//! evaluated from the rocket simulation epoch rather than shared solar-map
//! transforms, whose display scale is not an authoritative flight frame.
//!
//! The existing display-scale conversion remains only for the current
//! parent-relative moon approximation.

use super::climate::climate_at_position;
use super::components::{PrimaryVehicle, RocketPhysicsState, RocketPlanetBinding};
use super::environment::{bound_planet_sun, solar_illuminance_lux};
use crate::application::solar_system_startup::solar_surface_luminance_nits;
use crate::domain::services::ephemeris::NaifBodyId;
use crate::domain::services::physics::calculate_planet_position_f64;
use crate::domain::services::physics_orbital::MOON_ORBIT_SCALE;
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::services::reference_frames::body_fixed_to_planet_inertial_rotation;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_source::ElevationBounds;
use crate::domain::value_objects::atmospheric_optics::AtmosphericOptics;
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::climate::MeasuredClimate;
use crate::infrastructure::bevy_adapters::entity_components::*;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::planet_appearance::color_for_body;
use crate::infrastructure::bevy_adapters::rendering::cloud_assets::CloudNoiseAssets;
use crate::infrastructure::bevy_adapters::rendering::materials::{
    create_planet_material, flight_cloud_altitude_m, flight_cloud_bottom_altitude_m, CloudField,
    CloudMaterial, CloudParams, CloudShadow, PlanetMaterialConfig,
};
use crate::infrastructure::bevy_adapters::rendering::meshes::create_flight_globe_mesh;
use crate::infrastructure::bevy_adapters::rendering::textures::{
    get_cloud_layer_config, get_planet_textures, load_texture,
};
use crate::infrastructure::bevy_adapters::terrain::render::{RenderOrigin, TerrainRenderConfig};
// The far-field planet and cloud shells enclose the flight camera. If they cast
// or receive directional shadows they produce a planet-scale dark arc, so they
// are excluded from the shadow system while remaining fully lit.
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::{DVec3, Mat3};
use bevy::prelude::*;

/// Marks the local visual representation of the Sun. Its transform is camera
/// relative so the rocket camera retains its local depth range while the disc
/// keeps the Sun's true angular diameter.
#[derive(Component, Debug)]
pub struct RocketSunDisc;

const ROCKET_SUN_DISC_DISTANCE_M: f64 = 20_000.0;
const SUN_RADIUS_M: f64 = 696_340_000.0;
/// The solar disc is hidden below this altitude. The small negative allowance
/// represents atmospheric refraction lifting the Sun slightly above the
/// geometric horizon; it is zero in vacuum, where the disc sets geometrically.
const SUN_DISC_HORIZON_ALTITUDE_RAD: f64 = -0.0087;
/// The cloud deck is drawn on a camera-anchored dome just inside the far plane
/// (matching the sky dome), so it stays within the flight camera's depth range.
const CLOUD_DOME_FAR_FRACTION: f32 = 0.9;

/// A low-detail geographic Earth shell beneath streamed terrain. It is a
/// presentation fallback only: terrain meshes remain the sole source for
/// elevation, collision, and surface queries.
#[derive(Component, Debug, Clone)]
pub struct RocketBoundPlanetSurface {
    body: CelestialBodyId,
}

/// Marks the Rocket-mode geographic cloud deck. Observer air density must not
/// fade this layer away when the vehicle rises above it.
#[derive(Component, Debug)]
pub struct RocketBoundPlanetCloud;

/// Which vertical layer a cloud dome represents. Both layers share one shader
/// and material type; only their slab, density, and transparent draw order
/// differ.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RocketCloudLayer {
    /// Water cloud between the boundary layer and the mid-troposphere.
    Low,
    /// Thin high ice-cloud veil above the water deck.
    Cirrus,
}

impl RocketCloudLayer {
    fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Cirrus => "Cirrus",
        }
    }
}

/// Cirrus slab altitude above the reference surface, in metres.
const CIRRUS_BOTTOM_ALTITUDE_M: f32 = 8_000.0;
const CIRRUS_TOP_ALTITUDE_M: f32 = 9_600.0;
/// Transparent-pass sort biases, added to the view distance. Smaller values
/// draw first, so the deck farther from the camera uses the "far" bias. Both
/// domes are camera-centred, so these are the only ordering signal.
const CLOUD_SORT_BIAS_NEAR: f32 = -1.0;
const CLOUD_SORT_BIAS_FAR: f32 = -2.0;

/// Component marking a moon entity managed by the rocket planet system.
#[derive(Component, Debug, Clone)]
pub struct RocketMoon {
    pub name: String,
    parent_planet: CelestialBodyId,
}

impl RocketMoon {
    fn new(name: String, parent_planet: CelestialBodyId) -> Self {
        Self {
            name,
            parent_planet,
        }
    }

    fn orbits(&self, planet_id: &CelestialBodyId) -> bool {
        &self.parent_planet == planet_id
    }
}

/// Resource storing the bound planet identifier for quick lookup.
#[derive(Resource, Debug, Default)]
pub struct RocketBoundPlanet(pub Option<CelestialBodyId>);

/// Rocket Mode keeps shared celestial entities as the simulation authority, but
/// hides their solar-scale presentation. Flight-frame proxy meshes are the only
/// celestial visuals seen by the rocket camera.
#[expect(
    clippy::type_complexity,
    reason = "The query selects both planet and orbit presentation entities."
)]
pub fn isolate_rocket_presentation(
    mut solar_presentation: Query<
        &mut Visibility,
        Or<(With<PlanetComponent>, With<OrbitComponent>)>,
    >,
    mut solar_lights: Query<&mut PointLight>,
) {
    for mut visibility in solar_presentation.iter_mut() {
        *visibility = Visibility::Hidden;
    }
    for mut light in solar_lights.iter_mut() {
        light.intensity = 0.0;
    }
}

/// Startup system: spawn the bound planet's far-field shell, moons, and Sun in
/// flight units.
#[expect(
    clippy::too_many_arguments,
    reason = "Rocket startup composes independent render assets and shared authority queries."
)]
pub fn setup_rocket_planets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cloud_materials: ResMut<Assets<CloudMaterial>>,
    cloud_noise: Res<CloudNoiseAssets>,
    terrain_config: Res<TerrainRenderConfig>,
    asset_server: Res<AssetServer>,
    solar_params: Res<SolarSystemParameters>,
    rocket_query: Query<(&RocketPlanetBinding, &RocketPhysicsState), PrimaryVehicle>,
    planet_query: Query<(&PlanetComponent, Option<&PlanetTerrain>)>,
    mut bound_planet_res: ResMut<RocketBoundPlanet>,
) {
    let Some((binding, _rocket)) = rocket_query.iter().next() else {
        return;
    };
    bound_planet_res.0 = Some(binding.planet_name.clone());

    if let Some((planet, terrain)) = planet_query
        .iter()
        .find(|(planet, _)| planet.domain_planet.name == binding.planet_name.as_str())
    {
        let bounds = terrain.map_or(ElevationBounds::new(0.0, 0.0), |terrain| {
            terrain.source.elevation_bounds_m()
        });
        let fallback_radius_m = bound_planet_fallback_radius_m(
            planet.domain_planet.radius_km as f64 * 1_000.0,
            bounds,
            terrain_config.patch_resolution,
        );
        spawn_rocket_bound_planet_surface(
            &mut commands,
            &mut meshes,
            &mut materials,
            &asset_server,
            &planet.domain_planet,
            fallback_radius_m as f32,
        );
        spawn_rocket_cloud_deck(
            &mut commands,
            &mut meshes,
            &mut cloud_materials,
            &cloud_noise,
            &planet.domain_planet,
        );
    }

    for moon in PlanetFactory::get_moons_of_id(&binding.planet_name) {
        spawn_rocket_moon(
            &mut commands,
            &mut meshes,
            &mut materials,
            &asset_server,
            &moon,
        );
    }

    // Spawn the Sun
    spawn_rocket_sun(
        &mut commands,
        &mut meshes,
        &mut materials,
        &asset_server,
        &solar_params,
    );
}

fn spawn_rocket_bound_planet_surface(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &AssetServer,
    planet: &crate::domain::entities::planet::Planet,
    fallback_radius_m: f32,
) {
    let textures = get_planet_textures(&planet.name);
    let albedo = load_texture(asset_server, textures.albedo);
    let emissive_texture = load_texture(asset_server, textures.emissive);
    let material = create_planet_material(PlanetMaterialConfig {
        base_color_texture: albedo,
        normal_map_texture: None,
        emissive_texture,
        base_color: Color::WHITE,
        // Real night-emission sources only; no flat albedo-wide fill.
        emissive: if textures.emissive.is_some() {
            LinearRgba::WHITE
        } else {
            LinearRgba::BLACK
        },
        unlit: false,
        metallic: 0.0,
        reflectance: 0.4,
        perceptual_roughness: 0.55,
    });
    commands.spawn((
        Mesh3d(create_flight_globe_mesh(meshes, fallback_radius_m)),
        MeshMaterial3d(materials.add(material)),
        Transform::default(),
        RocketBoundPlanetSurface {
            body: CelestialBodyId::new(planet.name.clone())
                .expect("configured bound planet must have a valid identifier"),
        },
        NotShadowCaster,
        NotShadowReceiver,
        Name::new(format!("RocketFarField{}", planet.name)),
    ));
}

/// Spawn the volumetric cloud deck as a camera-anchored dome.
///
/// The deck's spherical slab is centred on the planet, but a planet-anchored
/// shell would sit at the planet radius (thousands of kilometres) and fall far
/// outside the flight camera's 100 km far plane, so it would never rasterize.
/// Instead the deck is drawn on a small dome that follows the camera; the shader
/// computes the ray's intersection with the cloud base/top spheres analytically
/// from the camera and the planet centre, so the cloud content stays pinned to
/// the planet while the proxy stays in the camera's depth range.
fn spawn_rocket_cloud_deck(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    cloud_materials: &mut Assets<CloudMaterial>,
    cloud_noise: &CloudNoiseAssets,
    planet: &crate::domain::entities::planet::Planet,
) {
    if get_cloud_layer_config(&planet.name).is_none() {
        return;
    }
    let surface_radius_m = planet.radius_km * 1_000.0;
    spawn_cloud_dome(
        commands,
        meshes,
        cloud_materials,
        cloud_noise,
        &planet.name,
        low_cloud_params(surface_radius_m, &planet.name),
        CLOUD_SORT_BIAS_NEAR,
        RocketCloudLayer::Low,
    );
    // Only Earth has a measured climatology and a reviewed upper-air slab. The
    // other bodies keep their single measured-texture cloud layer.
    if planet.name == "Earth" {
        spawn_cloud_dome(
            commands,
            meshes,
            cloud_materials,
            cloud_noise,
            &planet.name,
            cirrus_cloud_params(surface_radius_m),
            CLOUD_SORT_BIAS_FAR,
            RocketCloudLayer::Cirrus,
        );
    }
}

/// Parameters for the boundary-layer/mid-troposphere water-cloud deck.
fn low_cloud_params(surface_radius_m: f32, body: &str) -> CloudParams {
    let mut params = CloudParams::deck();
    params.bottom_radius = surface_radius_m + flight_cloud_bottom_altitude_m(body);
    params.top_radius = surface_radius_m + flight_cloud_altitude_m(body);
    params
}

/// Parameters for the thin high cirrus veil. Ice cloud is much less dense and
/// optically thinner than the water deck, forms in a shallow layer, and is
/// advected by the upper-tropospheric wind rather than the surface wind.
fn cirrus_cloud_params(surface_radius_m: f32) -> CloudParams {
    let mut params = CloudParams::deck();
    params.bottom_radius = surface_radius_m + CIRRUS_BOTTOM_ALTITUDE_M;
    params.top_radius = surface_radius_m + CIRRUS_TOP_ALTITUDE_M;
    params.density_scale = 0.6;
    params.absorption = 0.008;
    params.coverage = 0.5;
    // Stratus-like vertical profile: a flat, shallow sheet.
    params.cloud_type = 0.15;
    params.erosion_scale = 0.7;
    params.detail_scale = 1.6;
    params.noise_scale = 1.0 / 12_000.0;
    params.wind_speed = 45.0;
    // A thin veil converges quickly; 40 view steps and 3 sun steps keep the
    // extra pass near 15% of the water deck's sample count.
    params.primary_steps = 40.0;
    params.light_steps = 3.0;
    params.multi_scatter_octaves = 2.0;
    params
}

/// Spawn one camera-anchored cloud dome and its material.
#[expect(
    clippy::too_many_arguments,
    reason = "Each argument is an independent render asset or layer descriptor for one spawn."
)]
fn spawn_cloud_dome(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    cloud_materials: &mut Assets<CloudMaterial>,
    cloud_noise: &CloudNoiseAssets,
    body_name: &str,
    params: CloudParams,
    sort_bias: f32,
    layer: RocketCloudLayer,
) {
    let cloud_material = cloud_materials.add(CloudMaterial {
        params,
        base_noise: cloud_noise.base_noise.clone(),
        detail_noise: cloud_noise.detail_noise.clone(),
        weather_map: cloud_noise.weather_map.clone(),
        depth_bias: sort_bias,
    });
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(1.0))),
        MeshMaterial3d(cloud_material),
        Transform::default(),
        RocketBoundPlanetCloud,
        layer,
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
        Name::new(format!("RocketFarField{body_name}{}Clouds", layer.label())),
    ));
}

/// The geographic fallback must lie below even the coarsest streamed triangles,
/// not merely below their spherical vertices. Otherwise it intersects ocean caps
/// along jagged grid-shaped lines. This conservative angular cell diameter also
/// covers stitched edges; no terrain sampling or high-resolution globe is needed.
fn bound_planet_fallback_radius_m(radius_m: f64, bounds: ElevationBounds, resolution: u32) -> f64 {
    let cell_angle_rad = 2.0 * std::f64::consts::SQRT_2 / f64::from(resolution.max(3) - 1);
    let minimum_surface_radius_m = radius_m + bounds.min_m.min(0.0);
    (minimum_surface_radius_m * cell_angle_rad.cos() - 2.0).max(radius_m * 0.1)
}

/// Spawn a moon in flight units.
fn spawn_rocket_moon(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &AssetServer,
    moon: &crate::domain::entities::planet::Planet,
) {
    let Some(parent_name) = moon.parent_entity.as_ref() else {
        return;
    };
    let Ok(parent_planet) = CelestialBodyId::new(parent_name.clone()) else {
        return;
    };

    let radius_m = moon.radius_km * 1000.0;
    let mesh_handle = meshes.add(Sphere::new(radius_m));

    let textures = get_planet_textures(&moon.name);
    let albedo_handle = load_texture(asset_server, textures.albedo);
    let emissive_handle = load_texture(asset_server, textures.emissive);

    let base_color = color_for_body(&moon.name)
        .expect("configured celestial bodies must have a presentation color");
    let (metallic, reflectance, perceptual_roughness) = match moon.name.as_str() {
        "Moon" => (0.05, 0.12, 0.9),
        "Phobos" | "Deimos" => (0.05, 0.1, 0.85),
        "Io" => (0.1, 0.3, 0.7),
        "Europa" => (0.05, 0.5, 0.3),
        "Ganymede" => (0.05, 0.4, 0.5),
        "Callisto" => (0.05, 0.3, 0.6),
        "Titan" => (0.05, 0.5, 0.4),
        _ => (0.05, 0.5, 0.7),
    };

    let material = create_planet_material(PlanetMaterialConfig {
        base_color_texture: albedo_handle.clone(),
        normal_map_texture: None,
        emissive_texture: emissive_handle.clone(),
        base_color,
        // Real night-emission sources only; the previous flat albedo fill made
        // every moon glow on its unlit side.
        emissive: if textures.emissive.is_some() {
            LinearRgba::WHITE
        } else {
            LinearRgba::BLACK
        },
        unlit: false,
        metallic,
        reflectance,
        perceptual_roughness,
    });

    let material_handle = materials.add(material);
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::default(),
        RocketMoon::new(moon.name.clone(), parent_planet),
    ));
}

/// Spawn the Sun in flight units.
fn spawn_rocket_sun(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &AssetServer,
    solar_params: &SolarSystemParameters,
) {
    let mesh_handle = meshes.add(Sphere::new(1.0));

    let textures = get_planet_textures("Sun");
    let albedo_handle = load_texture(asset_server, textures.albedo);

    let material = create_planet_material(PlanetMaterialConfig {
        base_color_texture: albedo_handle.clone(),
        normal_map_texture: None,
        emissive_texture: albedo_handle,
        base_color: Color::srgb(1.0, 1.0, 0.98),
        emissive: LinearRgba::new(
            solar_surface_luminance_nits(solar_params),
            solar_surface_luminance_nits(solar_params),
            solar_surface_luminance_nits(solar_params),
            1.0,
        ),
        // Match the solar-map material path so HDR emission participates in
        // the same tone mapping and bloom presentation.
        unlit: false,
        metallic: 0.0,
        reflectance: 0.0,
        perceptual_roughness: 0.0,
    });

    let material_handle = materials.add(material);
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::default(),
        RocketSunDisc,
        // This is a camera-relative image of the emitter, not an occluder.
        // Casting from the 20 km proxy creates a ~93 m circular ground shadow.
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// Update rocket celestial proxies from the authoritative rocket simulation epoch.
///
/// Rocket proxies must not consume shared solar-map transforms because the
/// flight presentation has a distinct meter-scale render origin. Both paths
/// derive their celestial state from the same `SimulationTime` epoch.
#[expect(
    clippy::too_many_arguments,
    reason = "This presentation system synchronizes independent shared and rocket-mode state."
)]
pub fn update_rocket_planets(
    solar_params: Res<SolarSystemParameters>,
    physical_scale: Res<PhysicalScale>,
    render_origin: Res<RenderOrigin>,
    sim_time: Res<SimulationTime>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    rocket_query: Query<(), With<RocketPhysicsState>>,
    planet_query: Query<&PlanetComponent, Without<RocketMoon>>,
    mut moon_query: Query<(&RocketMoon, &mut Transform), Without<RocketBoundPlanetSurface>>,
    mut bound_surface_query: Query<
        (&RocketBoundPlanetSurface, &mut Transform),
        Without<RocketMoon>,
    >,
    bound_planet_res: Res<RocketBoundPlanet>,
) {
    let Some(bound_planet_id) = &bound_planet_res.0 else {
        return;
    };
    if rocket_query.is_empty() {
        return;
    }
    let Some(bound_planet) = planet_query
        .iter()
        .find(|planet| planet.domain_planet.name == bound_planet_id.as_str())
    else {
        return;
    };

    let Some(bound_body) = NaifBodyId::for_catalog_name(bound_planet_id.as_str()) else {
        return;
    };
    // Conversion: solar display units -> meters
    let display_to_meters = physical_scale.solar_meters_per_display_unit;

    // Streamed terrain displaces the authoritative surface; this geographic
    // shell remains beneath it so the Earth is continuous while terrain jobs
    // refine and beyond the active patch coverage.
    let planet_center_flight = -render_origin.origin.as_vec3();
    for (surface, mut transform) in &mut bound_surface_query {
        let Some(orientation) =
            ephemeris_snapshot.orientation_for_catalog_body(surface.body.as_str())
        else {
            continue;
        };
        *transform = bound_planet_surface_transform(
            render_origin.origin,
            body_fixed_to_planet_inertial_rotation(orientation),
        );
    }

    // Moons: position relative to bound planet
    for (rocket_moon, mut transform) in &mut moon_query {
        if rocket_moon.orbits(bound_planet_id) {
            if let Some(moon_relative_to_bound) = NaifBodyId::for_catalog_name(&rocket_moon.name)
                .and_then(|body| ephemeris_snapshot.solar_inertial_relative_state(body, bound_body))
            {
                transform.translation =
                    (planet_center_flight.as_dvec3() + moon_relative_to_bound.position_m).as_vec3();
                continue;
            }
            // Only moons absent from the satellite kernel coverage fall through
            // to the labelled parent-relative approximation.
            if NaifBodyId::is_kernel_backed(&rocket_moon.name) {
                continue;
            }
            let moon_solar = planet_query
                .iter()
                .find(|planet| planet.domain_planet.name == rocket_moon.name)
                .map(|planet| {
                    calculate_planet_position_f64(
                        &planet.domain_planet,
                        sim_time.sim_time_s / 86_400.0,
                        &solar_params,
                        DVec3::ZERO,
                        Some(bound_planet.domain_planet.axial_tilt_deg),
                    )
                });

            if let Some(moon_pos) = moon_solar {
                // Shared solar presentation intentionally exaggerates moon
                // orbits. Flight proxies must undo that visual-only scale.
                let rel = moon_pos * (display_to_meters / MOON_ORBIT_SCALE as f64);
                transform.translation = (planet_center_flight.as_dvec3() + rel).as_vec3();
            }
        }
    }
}

fn bound_planet_surface_transform(
    render_origin: DVec3,
    body_to_inertial: bevy::math::DQuat,
) -> Transform {
    Transform::from_translation((-render_origin).as_vec3())
        .with_rotation(body_to_inertial.as_quat())
}

/// Keep the visual Sun inside the local rocket camera depth range while its
/// direction, angular size, and horizon visibility come from the evaluated
/// ephemeris snapshot. This is presentation only; the directional light remains
/// the illumination authority.
pub fn update_rocket_sun_disc(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
    rocket_query: Query<&RocketPhysicsState, PrimaryVehicle>,
    camera_query: Query<&Transform, (With<Camera3d>, Without<RocketSunDisc>)>,
    mut sun_query: Query<(&mut Transform, &mut Visibility), With<RocketSunDisc>>,
) {
    let Some(bound_body) = bound_planet
        .0
        .as_ref()
        .and_then(|id| NaifBodyId::for_catalog_name(id.as_str()))
    else {
        return;
    };
    let Some(sun_state) =
        ephemeris_snapshot.solar_inertial_relative_state(NaifBodyId::SUN, bound_body)
    else {
        return;
    };
    let planet_sun_distance_m = sun_state.position_m.length();
    let Some(sun_direction) = (planet_sun_distance_m.is_finite() && planet_sun_distance_m > 0.0)
        .then(|| sun_state.position_m / planet_sun_distance_m)
    else {
        return;
    };
    let Some(camera) = camera_query.iter().next() else {
        return;
    };

    let visible = rocket_query
        .iter()
        .next()
        .map(|rocket| {
            let local_up = rocket.dynamics.position_m.normalize_or_zero();
            local_up.dot(sun_direction).clamp(-1.0, 1.0).asin() > SUN_DISC_HORIZON_ALTITUDE_RAD
        })
        .unwrap_or(false);

    let radius_m = rocket_sun_disc_radius_m(planet_sun_distance_m);
    let translation =
        camera.translation + sun_direction.as_vec3() * ROCKET_SUN_DISC_DISTANCE_M as f32;
    for (mut transform, mut visibility) in sun_query.iter_mut() {
        transform.translation = translation;
        transform.scale = Vec3::splat(radius_m as f32);
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Apparent solar radius for a unit-distance Sun scaled to the disc's render
/// distance. Because the disc and the physical angular size both scale with the
/// planet-Sun distance, the rendered angular radius matches the real one at any
/// heliocentric distance.
fn rocket_sun_disc_radius_m(planet_sun_distance_m: f64) -> f64 {
    if !planet_sun_distance_m.is_finite() || planet_sun_distance_m <= 0.0 {
        return 0.0;
    }
    ROCKET_SUN_DISC_DISTANCE_M * (SUN_RADIUS_M / planet_sun_distance_m)
}

/// Update the rocket-mode volumetric cloud deck: position the camera-anchored
/// dome on the active camera's far plane and refresh the cloud uniforms from the
/// authoritative ephemeris Sun, the bound planet's orientation, and the shared
/// render origin. Only a small uniform and the dome transform are written per
/// frame; the noise textures are generated once at startup and never
/// regenerated, and nothing here touches simulation state.
#[expect(
    clippy::type_complexity,
    clippy::too_many_arguments,
    reason = "The query selects the active camera projection and the disjoint cloud dome; the remaining parameters are independent ECS resources."
)]
pub fn update_rocket_clouds(
    render_origin: Res<RenderOrigin>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    bound_planet: Res<RocketBoundPlanet>,
    time: Res<Time>,
    rocket_query: Query<&RocketPhysicsState, PrimaryVehicle>,
    measured_climate: Option<Res<MeasuredClimate>>,
    simulation_time: Res<SimulationTime>,
    planet_query: Query<&PlanetComponent>,
    camera_query: Query<
        (&Camera, &Transform, &Projection),
        (With<Camera3d>, Without<RocketBoundPlanetCloud>),
    >,
    mut clouds: Query<
        (
            &mut Transform,
            &MeshMaterial3d<CloudMaterial>,
            &RocketCloudLayer,
        ),
        With<RocketBoundPlanetCloud>,
    >,
    mut materials: ResMut<Assets<CloudMaterial>>,
    mut cloud_shadow: ResMut<CloudShadow>,
) {
    let Some((_, camera_transform, projection)) =
        camera_query.iter().find(|(camera, _, _)| camera.is_active)
    else {
        return;
    };
    let Some(bound_id) = bound_planet.0.as_ref() else {
        return;
    };
    let Some(sun) = bound_planet_sun(&ephemeris_snapshot, bound_id) else {
        return;
    };
    let Some(orientation) = ephemeris_snapshot.orientation_for_catalog_body(bound_id.as_str())
    else {
        return;
    };
    let body_to_inertial = body_fixed_to_planet_inertial_rotation(orientation);
    let planet_rotation = Mat3::from_quat(body_to_inertial.as_quat());
    // Both ground materials rotate their fragment and the Sun into the
    // body-fixed frame the density field lives in.
    let inertial_to_body = Vec4::from_array(body_to_inertial.inverse().as_quat().to_array());
    let planet_center = (-render_origin.origin).as_vec3();
    // Draw the deck farther from the camera first. From the ground the cirrus
    // veil is behind the water deck, from orbit it is in front, and a fixed
    // bias would composite them backwards in one of those views.
    let surface_radius_m = planet_query
        .iter()
        .find(|planet| planet.matches_body(bound_id))
        .map(|planet| planet.domain_planet.radius_km * 1_000.0)
        .unwrap_or(0.0);
    let camera_radius = (camera_transform.translation - planet_center).length();
    let cirrus_draws_first = camera_radius < surface_radius_m + CIRRUS_BOTTOM_ALTITUDE_M;
    let sun_direction = sun.direction.as_vec3();
    let sun_color = LinearRgba::from(Color::srgb(1.0, 1.0, 0.98));
    let sun_irradiance = Vec3::new(sun_color.red, sun_color.green, sun_color.blue)
        * solar_illuminance_lux(sun.distance_m);
    let elapsed_s = time.elapsed_secs();
    if !elapsed_s.is_finite() {
        return;
    }
    // Direct sunlight reaching each deck is attenuated by the atmosphere above
    // it. Both decks sit above the ground, so their beams are less reddened
    // than the observer's, and the high cirrus is brighter than the water deck
    // at a low Sun.
    let optics = AtmosphericOptics::for_body(bound_id.as_str(), surface_radius_m);
    let cloud_sun_cos = rocket_query
        .iter()
        .next()
        .map(|rocket| {
            rocket
                .dynamics
                .position_m
                .normalize_or_zero()
                .dot(sun.direction)
                .clamp(0.0, 1.0) as f32
        })
        .unwrap_or(1.0);
    let measured = measured_climate.as_ref().and_then(|climate| {
        let rocket = rocket_query.iter().next()?;
        climate_at_position(
            climate,
            rocket.dynamics.position_m,
            orientation,
            &simulation_time,
        )
    });

    let dome_radius = match projection {
        Projection::Perspective(perspective) => {
            (perspective.far * CLOUD_DOME_FAR_FRACTION).max(1.0)
        }
        _ => 1.0,
    };

    for (mut transform, handle, layer) in &mut clouds {
        transform.translation = camera_transform.translation;
        transform.scale = Vec3::splat(dome_radius);
        if let Some(material) = materials.get_mut(&handle.0) {
            material.params.planet_center = planet_center;
            material.params.planet_rotation = planet_rotation;
            material.params.sun_direction = sun_direction;
            material.params.sun_irradiance = sun_irradiance;
            let deck_altitude_m = (0.5
                * (material.params.bottom_radius + material.params.top_radius)
                - surface_radius_m)
                .max(0.0);
            material.params.sun_transmittance = optics
                .map(|optics| Vec3::from(optics.sun_transmittance(deck_altitude_m, cloud_sun_cos)))
                .unwrap_or(Vec3::ONE);
            material.params.time_s = elapsed_s;
            let draws_first = match layer {
                RocketCloudLayer::Cirrus => cirrus_draws_first,
                RocketCloudLayer::Low => !cirrus_draws_first,
            };
            material.depth_bias = if draws_first {
                CLOUD_SORT_BIAS_FAR
            } else {
                CLOUD_SORT_BIAS_NEAR
            };
            if let Some((latitude_deg, longitude_deg, sample)) = measured {
                match layer {
                    RocketCloudLayer::Low => apply_measured_cloud_climate(
                        &mut material.params,
                        sample,
                        latitude_deg,
                        longitude_deg,
                    ),
                    RocketCloudLayer::Cirrus => apply_measured_cirrus_climate(
                        &mut material.params,
                        sample,
                        latitude_deg,
                        longitude_deg,
                    ),
                }
            }
            // Publish the water deck's field for the terrain shadow pass. Only
            // the low deck casts a ground shadow worth marching: the cirrus
            // veil is optically thin and 8-10 km up.
            if *layer == RocketCloudLayer::Low {
                cloud_shadow.field = CloudField::from_cloud_params(&material.params);
                cloud_shadow.sun_direction = sun_direction;
                cloud_shadow.inertial_to_body = inertial_to_body;
            }
        }
    }
}

/// Map a measured climatology sample onto the cloud presentation uniforms.
///
/// Cloud amount is used directly as the deck coverage multiplier; the shader's
/// procedural weather map supplies the spatial texture. Wind is the measured
/// 2 m vector projected into the body-fixed x/z plane, scaled toward the
/// mid-tropospheric advection speed rather than the surface value. No value
/// here enters flight dynamics.
fn apply_measured_cloud_climate(
    params: &mut CloudParams,
    sample: crate::domain::services::climate::ClimateSample,
    latitude_deg: f64,
    longitude_deg: f64,
) {
    params.coverage = (sample.cloud_fraction as f32).clamp(0.02, 0.95);
    params.wind_speed = (sample.wind_speed_mps() as f32 * 2.5).clamp(2.0, 60.0);
    let (x, z) = sample.wind_direction_xz(latitude_deg, longitude_deg);
    params.wind_direction = Vec2::new(x, z);
}

/// Map a measured climatology sample onto the high cirrus veil. Upper-air cloud
/// amount is not measured, so the surface cloud fraction only scales a thin
/// veil, and the wind uses an upper-tropospheric speed rather than the surface
/// value. Presentation only.
fn apply_measured_cirrus_climate(
    params: &mut CloudParams,
    sample: crate::domain::services::climate::ClimateSample,
    latitude_deg: f64,
    longitude_deg: f64,
) {
    params.coverage = (sample.cloud_fraction as f32).clamp(0.3, 0.8);
    params.wind_speed = (sample.wind_speed_mps() as f32 * 6.0).clamp(15.0, 90.0);
    let (x, z) = sample.wind_direction_xz(latitude_deg, longitude_deg);
    params.wind_direction = Vec2::new(x, z);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::body_orientation::BodyOrientation;
    use crate::domain::services::cube_sphere::{build_patch_geometry, TerrainPatch};
    use crate::domain::services::ephemeris::{BodyState, TdbEpoch};
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::terrain_source::ProceduralTerrainSource;
    use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
    use crate::infrastructure::bevy_adapters::entity_components::PlanetComponent;
    use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
    use crate::infrastructure::bevy_adapters::physical_scale::PhysicalScale;
    use crate::infrastructure::bevy_adapters::rendering::materials::{
        CLOUD_SINGLE_SCATTER_ALBEDO, MAX_CLOUD_LIGHT_STEPS,
    };
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::math::{DMat3, DQuat, DVec3};

    #[test]
    fn cloud_params_defaults_are_bounded_and_finite() {
        let deck = CloudParams::deck();
        assert!((0.0..=1.0).contains(&deck.coverage));
        assert!((0.0..=1.0).contains(&deck.cloud_type));
        assert!(deck.density_scale.is_finite() && deck.density_scale > 0.0);
        assert!(deck.absorption.is_finite() && deck.absorption > 0.0);
        assert!((-1.0..1.0).contains(&deck.forward_g));
        assert!(deck.noise_scale.is_finite() && deck.noise_scale > 0.0);
        assert!(deck.detail_scale >= 1.0);
        assert!(deck.wind_speed.is_finite() && deck.wind_speed > 0.0);
        assert!(deck.primary_steps >= 1.0 && deck.light_steps >= 1.0);
        assert!(deck.light_steps <= MAX_CLOUD_LIGHT_STEPS);
        assert!(deck.multi_scatter_octaves >= 1.0);
        assert!((0.99..=1.0).contains(&CLOUD_SINGLE_SCATTER_ALBEDO));
        assert_eq!(deck.sun_transmittance, Vec3::ONE);
        assert_eq!(deck.time_s, 0.0);
    }

    #[test]
    fn measured_climate_sets_bounded_cloud_coverage_and_wind() {
        let mut params = CloudParams::deck();
        let sample = crate::domain::services::climate::ClimateSample {
            temperature_k: 288.15,
            pressure_pa: 101_325.0,
            relative_humidity: 0.8,
            wind_east_mps: 8.0,
            wind_north_mps: 0.0,
            cloud_fraction: 0.72,
        };
        apply_measured_cloud_climate(&mut params, sample, 0.0, 0.0);
        assert!((params.coverage - 0.72).abs() < 1e-6);
        assert!((params.wind_speed - 20.0).abs() < 1e-6);
        assert!((params.wind_direction.x - 0.0).abs() < 1e-6);
        assert!((params.wind_direction.y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn visual_sun_disc_never_casts_or_receives_local_shadows() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_asset::<Image>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.insert_resource(SolarSystemParameters::for_visualization());
        app.add_systems(
            Startup,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut materials: ResMut<Assets<StandardMaterial>>,
             asset_server: Res<AssetServer>,
             solar: Res<SolarSystemParameters>| {
                spawn_rocket_sun(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &asset_server,
                    &solar,
                )
            },
        );
        app.update();
        let mut sun = app
            .world_mut()
            .query_filtered::<(Has<NotShadowCaster>, Has<NotShadowReceiver>), With<RocketSunDisc>>(
            );
        assert_eq!(sun.single(app.world()).unwrap(), (true, true));
    }

    #[test]
    fn far_field_planet_and_cloud_shells_never_cast_or_receive_shadows() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_asset::<Image>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<CloudMaterial>>();
        let planet = PlanetFactory::create_by_id(&CelestialBodyId::earth())
            .expect("Earth is a configured planet");
        app.add_systems(
            Startup,
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut cloud_materials: ResMut<Assets<CloudMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  asset_server: Res<AssetServer>| {
                let cloud_noise = CloudNoiseAssets::generate(&mut images, 1);
                spawn_rocket_bound_planet_surface(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &asset_server,
                    &planet,
                    6_371_000.0,
                );
                spawn_rocket_cloud_deck(
                    &mut commands,
                    &mut meshes,
                    &mut cloud_materials,
                    &cloud_noise,
                    &planet,
                );
            },
        );
        app.update();

        let world = app.world_mut();
        let mut surface = world.query_filtered::<
            (Has<NotShadowCaster>, Has<NotShadowReceiver>),
            With<RocketBoundPlanetSurface>,
        >();
        assert_eq!(
            surface.single(world).unwrap(),
            (true, true),
            "the enclosing far-field globe must not cast or receive shadows"
        );

        let mut cloud = world.query_filtered::<(
            Has<NotShadowCaster>,
            Has<NotShadowReceiver>,
            &RocketCloudLayer,
        ), With<RocketBoundPlanetCloud>>();
        let decks: Vec<_> = cloud.iter(world).collect();
        assert_eq!(
            decks.len(),
            2,
            "Earth has one water deck and one cirrus veil"
        );
        for flags in decks {
            assert_eq!(
                (flags.0, flags.1),
                (true, true),
                "the enclosing cloud shells must not cast or receive shadows"
            );
        }
    }

    #[test]
    fn cirrus_veil_sits_above_the_water_deck_and_is_thinner() {
        let surface_radius_m = 6_371_000.0;
        let low = low_cloud_params(surface_radius_m, "Earth");
        let cirrus = cirrus_cloud_params(surface_radius_m);

        assert!(cirrus.bottom_radius > low.top_radius);
        assert!(cirrus.top_radius > cirrus.bottom_radius);
        // Extinction per unit density is what makes the veil optically thin.
        assert!(
            cirrus.density_scale * cirrus.absorption < low.density_scale * low.absorption,
            "cirrus must be optically thinner than the water deck"
        );
        assert!(cirrus.absorption < low.absorption);
        assert!(cirrus.cloud_type < low.cloud_type);
        assert!(cirrus.primary_steps < low.primary_steps);
        assert!(cirrus.light_steps <= MAX_CLOUD_LIGHT_STEPS);
        // The veil contains about a third of the layer thickness of the deck.
        let low_thickness = low.top_radius - low.bottom_radius;
        let cirrus_thickness = cirrus.top_radius - cirrus.bottom_radius;
        assert!(cirrus_thickness < low_thickness * 0.5);
    }

    #[test]
    fn measured_cirrus_climate_stays_a_thin_veil() {
        let mut params = cirrus_cloud_params(6_371_000.0);
        let overcast = crate::domain::services::climate::ClimateSample {
            temperature_k: 288.15,
            pressure_pa: 101_325.0,
            relative_humidity: 0.9,
            wind_east_mps: 8.0,
            wind_north_mps: 0.0,
            cloud_fraction: 1.0,
        };
        apply_measured_cirrus_climate(&mut params, overcast, 0.0, 0.0);
        assert!(params.coverage <= 0.8);
        assert!(params.coverage >= 0.3);
        assert!(params.wind_speed <= 90.0);

        let clear = crate::domain::services::climate::ClimateSample {
            cloud_fraction: 0.0,
            ..overcast
        };
        apply_measured_cirrus_climate(&mut params, clear, 0.0, 0.0);
        assert!((params.coverage - 0.3).abs() < 1e-6);
    }

    #[test]
    fn fallback_globe_is_below_coarse_terrain_triangle_planes() {
        let radius_m = 6_371_000.0;
        let source = ProceduralTerrainSource::new(0, 0.0, 0.0, 0);
        let fallback_m =
            bound_planet_fallback_radius_m(radius_m, ElevationBounds::new(0.0, 0.0), 33);
        for root in TerrainPatch::roots() {
            let geometry = build_patch_geometry(&root, &source, radius_m, 33, 0.0);
            for triangle in geometry.indices.as_chunks::<3>().0 {
                if triangle.iter().any(|index| *index >= 33 * 33) {
                    continue;
                }
                let [a, b, c] =
                    triangle.map(|index| DVec3::from_array(geometry.positions[index as usize]));
                let plane_normal = (b - a).cross(c - a).normalize();
                assert!(plane_normal.dot(a).abs() > fallback_m);
            }
        }
    }

    fn earth_orientation(epoch: TdbEpoch) -> BodyOrientation {
        BodyOrientation::from_kernel(
            NaifBodyId::EARTH,
            epoch,
            "test-orientation".to_string(),
            DQuat::IDENTITY,
            DVec3::Z * (std::f64::consts::TAU / (23.934 * 3_600.0)),
        )
    }

    #[test]
    fn test_physical_scale_conversion() {
        let solar = SolarSystemParameters::for_visualization();
        let scale = PhysicalScale::from_solar_parameters(&solar);
        // 1 AU in meters should map to scale_factor display units
        let au_display = scale.solar_meters_to_units(149_597_870_700.0);
        assert!((au_display - solar.scale_factor as f64).abs() < 1.0);
    }

    #[test]
    fn rocket_moon_proxy_follows_simulation_time_not_shared_transforms() {
        let solar = SolarSystemParameters::for_visualization();
        let scale = PhysicalScale::from_solar_parameters(&solar);
        let sim_time_s = 86_400.0;
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let moon = PlanetFactory::create_by_name("Moon").unwrap();

        let mut app = App::new();
        app.insert_resource(solar.clone());
        app.insert_resource(scale);
        app.insert_resource(RenderOrigin::default());
        let mut simulation_time = SimulationTime::default();
        simulation_time.sim_time_s = sim_time_s;
        app.insert_resource(simulation_time);
        app.insert_resource(RocketBoundPlanet(Some(CelestialBodyId::earth())));
        let epoch = TdbEpoch::j2000();
        app.insert_resource(EphemerisSnapshot::from_states_and_orientations(
            vec![
                BodyState {
                    target: NaifBodyId::EARTH,
                    center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                    epoch,
                    position_m: DVec3::ZERO,
                    velocity_mps: DVec3::ZERO,
                },
                BodyState {
                    target: NaifBodyId::SUN,
                    center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                    epoch,
                    position_m: -DVec3::X * 149_597_870_700.0,
                    velocity_mps: DVec3::ZERO,
                },
                BodyState {
                    target: NaifBodyId::MOON,
                    center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                    epoch,
                    position_m: DVec3::X * 384_400_000.0,
                    velocity_mps: DVec3::ZERO,
                },
            ],
            vec![earth_orientation(epoch)],
        ));
        app.world_mut().spawn(RocketPhysicsState {
            dynamics: RocketDynamicsState::new(
                DVec3::ZERO,
                DVec3::ZERO,
                DQuat::IDENTITY,
                1.0,
                DMat3::IDENTITY,
                DVec3::ZERO,
            ),
        });
        app.world_mut().spawn(PlanetComponent {
            domain_planet: earth.clone(),
            material: default(),
            has_texture: false,
            base_reflectance: 0.0,
            base_roughness: 0.0,
        });
        app.world_mut().spawn(PlanetComponent {
            domain_planet: moon.clone(),
            material: default(),
            has_texture: false,
            base_reflectance: 0.0,
            base_roughness: 0.0,
        });
        let rocket_moon = app
            .world_mut()
            .spawn((
                RocketMoon::new("Moon".to_string(), CelestialBodyId::earth()),
                Transform::default(),
            ))
            .id();
        app.add_systems(Update, update_rocket_planets);

        app.update();

        let expected_moon = (DVec3::X * 384_400_000.0).as_vec3();

        assert_eq!(
            app.world()
                .entity(rocket_moon)
                .get::<Transform>()
                .unwrap()
                .translation,
            expected_moon
        );
    }

    #[test]
    fn rocket_sun_disc_preserves_the_true_solar_angular_radius() {
        let one_au = crate::domain::units::AU_IN_METERS;
        let radius_m = rocket_sun_disc_radius_m(one_au);
        let angular_radius_rad = (radius_m / ROCKET_SUN_DISC_DISTANCE_M).asin();
        let expected_angular_radius_rad = (SUN_RADIUS_M / one_au).asin();

        assert!((angular_radius_rad - expected_angular_radius_rad).abs() < 1e-12);
        assert!(rocket_sun_disc_radius_m(1.52 * one_au) < radius_m);
    }

    #[test]
    fn bound_planet_surface_uses_the_terrain_render_frame() {
        let render_origin = DVec3::new(1_000.0, -2_000.0, 3_000.0);
        let rotation = DQuat::from_rotation_y(0.25);
        let transform = bound_planet_surface_transform(render_origin, rotation);

        assert_eq!(transform.translation, (-render_origin).as_vec3());
        assert_eq!(transform.rotation, rotation.as_quat());
    }
}
