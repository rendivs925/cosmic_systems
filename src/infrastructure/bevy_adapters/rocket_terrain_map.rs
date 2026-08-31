//! Compact body-fixed terrain map for rocket-mode flight presentation.
//!
//! The map is deliberately a read-only, non-authoritative overview preview: its
//! cached raster samples the per-body [`TerrainSource`] overview and shared
//! [`surface_appearance`] law, while all overlays are derived from authoritative
//! state.

use crate::components::rocket::{
    GroundRest, RocketAutopilot, RocketMissionState, RocketPhysicsState, RocketPlanetBinding,
    TerrainCollisionState,
};
use crate::domain::services::reference_frames::{
    body_fixed_to_terrain_lat_lon, catalog_body_fixed_to_inertial_rotation,
    geodetic_to_terrain_lat_lon, planet_inertial_to_body_fixed,
};
use crate::domain::services::terrain_source::{surface_appearance, TerrainSource};
use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
use crate::infrastructure::bevy_adapters::components::{PlanetComponent, PlanetTerrain};
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::rocket_orbit::{
    update_orbit_prediction_cache, OrbitPrediction, OrbitPredictionCache,
};
use crate::infrastructure::bevy_adapters::rocket_telemetry::{FlightLogEntry, FlightRecorder};
use bevy::asset::RenderAssetUsages;
use bevy::math::{DVec3, Rot2, Vec2};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::collections::HashMap;

/// Global raster dimensions. The UI displays this at 1.5x for a compact but
/// legible panel while retaining a small, body-keyed cache.
pub const MAP_RASTER_WIDTH: u32 = 192;
pub const MAP_RASTER_HEIGHT: u32 = 96;
const MAP_WIDTH_PX: f32 = 288.0;
const MAP_HEIGHT_PX: f32 = 144.0;
const HISTORY_SEGMENTS: usize = 64;
const PREDICTION_SEGMENTS: usize = 96;
/// Presentation uncertainty only, not an input to landing guidance.
const ACTIVE_LANDING_UNCERTAINTY_M: f64 = 1_000.0;
const MAP_UPDATE_INTERVAL_S: f32 = 0.1;

/// A point in the map panel's top-left pixel coordinate system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainMapPoint {
    pub x_px: f32,
    pub y_px: f32,
}

/// A drawable line segment in map-panel pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainMapSegment {
    pub midpoint_px: TerrainMapPoint,
    pub length_px: f32,
    pub angle_rad: f32,
}

/// Equirectangular body-fixed projection: longitude spans the horizontal axis
/// and latitude spans the vertical axis, with north at the top.
pub fn equirectangular_point(
    latitude_deg: f64,
    longitude_deg: f64,
    width_px: f32,
    height_px: f32,
) -> Option<TerrainMapPoint> {
    if !latitude_deg.is_finite()
        || !longitude_deg.is_finite()
        || width_px <= 0.0
        || height_px <= 0.0
    {
        return None;
    }
    let latitude_deg = latitude_deg.clamp(-90.0, 90.0);
    // Keep the right seam at -180°, so every valid longitude lands in-panel.
    let longitude_deg = (longitude_deg + 180.0).rem_euclid(360.0) - 180.0;
    Some(TerrainMapPoint {
        x_px: ((longitude_deg + 180.0) / 360.0) as f32 * width_px,
        y_px: ((90.0 - latitude_deg) / 180.0) as f32 * height_px,
    })
}

/// Convert an in-panel pair to a line segment. Tracks crossing the map seam
/// are intentionally split rather than drawing a false line across the body.
pub fn map_segment(
    start: TerrainMapPoint,
    end: TerrainMapPoint,
    width_px: f32,
) -> Option<TerrainMapSegment> {
    let dx = end.x_px - start.x_px;
    let dy = end.y_px - start.y_px;
    if !dx.is_finite() || !dy.is_finite() || dx.abs() > width_px * 0.5 {
        return None;
    }
    let length_px = dx.hypot(dy);
    (length_px > 0.01).then_some(TerrainMapSegment {
        midpoint_px: TerrainMapPoint {
            x_px: (start.x_px + end.x_px) * 0.5,
            y_px: (start.y_px + end.y_px) * 0.5,
        },
        length_px,
        angle_rad: dy.atan2(dx),
    })
}

/// Build an RGBA non-authoritative overview raster from the shared terrain
/// visual law. This is a pure function so cache creation does not introduce
/// another terrain model or initialize local terrain caches.
pub fn terrain_map_raster(source: &dyn TerrainSource, width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let latitude_deg = 90.0 - (y as f64 + 0.5) * 180.0 / height as f64;
        for x in 0..width {
            let longitude_deg = -180.0 + (x as f64 + 0.5) * 360.0 / width as f64;
            let elevation_m = source.overview_height_m(latitude_deg, longitude_deg);
            let appearance = surface_appearance(
                elevation_m,
                source.overview_moisture(latitude_deg, longitude_deg),
                source.zone_lat(latitude_deg),
                source.overview_slope_deg(latitude_deg, longitude_deg),
            );
            pixels.extend(appearance.albedo.map(|channel| (channel * 255.0) as u8));
            pixels.push(255);
        }
    }
    pixels
}

/// Pixel radii for a circular ground uncertainty region projected to the map.
/// Longitude expands toward the poles because this is an equirectangular map.
pub fn uncertainty_ellipse_radii_px(
    latitude_deg: f64,
    radius_m: f64,
    body_radius_m: f64,
    width_px: f32,
    height_px: f32,
) -> Option<Vec2> {
    if !latitude_deg.is_finite()
        || !radius_m.is_finite()
        || !body_radius_m.is_finite()
        || radius_m <= 0.0
        || body_radius_m <= 0.0
    {
        return None;
    }
    let latitude_radius_deg = (radius_m / body_radius_m).to_degrees();
    let longitude_radius_deg =
        latitude_radius_deg / latitude_deg.to_radians().cos().abs().max(0.01);
    Some(Vec2::new(
        longitude_radius_deg as f32 * width_px / 360.0,
        latitude_radius_deg as f32 * height_px / 180.0,
    ))
}

#[derive(Resource, Default)]
struct TerrainMapRasterCache {
    images: HashMap<String, Handle<Image>>,
}

#[derive(Resource, Default)]
struct TerrainMapUpdateState {
    initialized: bool,
    last_update_real_time_s: f32,
    last_key: Option<TerrainMapUpdateKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerrainMapUpdateKey {
    planet: Entity,
    rocket_pixel: Option<(i32, i32)>,
    landing_pixel: Option<(i32, i32)>,
    mission: RocketMissionState,
    ground_contact: crate::domain::services::terrain_collision::GroundContact,
    resting: bool,
    history_len: usize,
    prediction_start_sim_time_bits: u64,
}

#[derive(Component)]
struct TerrainMapRasterImage;

#[derive(Component, Debug, Clone, Copy)]
enum TerrainMapOverlay {
    Rocket,
    Launch,
    Landing,
    Impact,
    Uncertainty,
    History(usize),
    Prediction(usize),
}

/// Presentation-only plugin, registered solely by [`RocketModePlugin`].
pub struct RocketTerrainMapPlugin;

impl Plugin for RocketTerrainMapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainMapRasterCache>()
            .init_resource::<TerrainMapUpdateState>()
            .add_systems(Startup, spawn_terrain_map_panel)
            .add_systems(
                Update,
                update_terrain_map_panel.after(update_orbit_prediction_cache),
            );
    }
}

fn spawn_terrain_map_panel(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(10.0),
                bottom: Val::Px(10.0),
                width: Val::Px(MAP_WIDTH_PX + 12.0),
                padding: UiRect::all(Val::Px(6.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.02, 0.05, 0.78)),
            BorderColor::all(Color::srgba(0.25, 0.5, 0.75, 0.55)),
            BorderRadius::all(Val::Px(4.0)),
            ZIndex(10),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("BODY-FIXED OVERVIEW"),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.72, 0.9)),
            ));
            panel
                .spawn((
                    Node {
                        position_type: PositionType::Relative,
                        width: Val::Px(MAP_WIDTH_PX),
                        height: Val::Px(MAP_HEIGHT_PX),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(Color::BLACK),
                ))
                .with_children(|map| {
                    map.spawn((
                        ImageNode::default(),
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Px(MAP_WIDTH_PX),
                            height: Val::Px(MAP_HEIGHT_PX),
                            ..default()
                        },
                        TerrainMapRasterImage,
                    ));
                    spawn_marker(
                        map,
                        TerrainMapOverlay::Rocket,
                        Color::srgb(1.0, 0.85, 0.15),
                        7.0,
                    );
                    spawn_marker(
                        map,
                        TerrainMapOverlay::Launch,
                        Color::srgb(0.85, 0.9, 1.0),
                        5.0,
                    );
                    spawn_marker(
                        map,
                        TerrainMapOverlay::Landing,
                        Color::srgb(0.25, 1.0, 0.45),
                        7.0,
                    );
                    spawn_marker(
                        map,
                        TerrainMapOverlay::Impact,
                        Color::srgb(1.0, 0.25, 0.2),
                        7.0,
                    );
                    map.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            display: Display::None,
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                        BorderColor::all(Color::srgba(0.25, 1.0, 0.45, 0.9)),
                        BorderRadius::all(Val::Percent(50.0)),
                        TerrainMapOverlay::Uncertainty,
                        ZIndex(2),
                    ));
                    for index in 0..HISTORY_SEGMENTS {
                        spawn_track_segment(
                            map,
                            TerrainMapOverlay::History(index),
                            Color::srgb(0.95, 0.65, 0.18),
                        );
                    }
                    for index in 0..PREDICTION_SEGMENTS {
                        spawn_track_segment(
                            map,
                            TerrainMapOverlay::Prediction(index),
                            Color::srgb(0.3, 0.75, 1.0),
                        );
                    }
                });
        });
}

fn spawn_marker(
    parent: &mut ChildSpawnerCommands,
    overlay: TerrainMapOverlay,
    color: Color,
    size: f32,
) {
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Px(size),
            height: Val::Px(size),
            display: Display::None,
            ..default()
        },
        BackgroundColor(color),
        BorderRadius::all(Val::Percent(50.0)),
        overlay,
        ZIndex(3),
    ));
}

fn spawn_track_segment(
    parent: &mut ChildSpawnerCommands,
    overlay: TerrainMapOverlay,
    color: Color,
) {
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            height: Val::Px(1.5),
            display: Display::None,
            ..default()
        },
        BackgroundColor(color),
        overlay,
        ZIndex(1),
    ));
}

#[allow(clippy::type_complexity)]
#[expect(
    clippy::too_many_arguments,
    reason = "The terrain-map UI system combines independent ECS data sources."
)]
fn update_terrain_map_panel(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    real_time: Res<Time>,
    planet_query: Query<(Entity, &PlanetComponent, &PlanetTerrain)>,
    rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &LaunchSiteCoordinates,
        &RocketAutopilot,
        &RocketMissionState,
        &TerrainCollisionState,
        &GroundRest,
        &FlightRecorder,
    )>,
    prediction_cache: Res<OrbitPredictionCache>,
    mut cache: ResMut<TerrainMapRasterCache>,
    mut update_state: ResMut<TerrainMapUpdateState>,
    mut images: ResMut<Assets<Image>>,
    mut raster_query: Query<&mut ImageNode, With<TerrainMapRasterImage>>,
    mut overlay_query: Query<(&TerrainMapOverlay, &mut Node, &mut UiTransform)>,
) {
    let Some((binding, rocket, launch_site, autopilot, mission, collision, ground_rest, recorder)) =
        rocket_query.iter().next()
    else {
        return;
    };
    let Some((planet_entity, planet, terrain)) = planet_query
        .iter()
        .find(|(_, planet, _)| planet.matches_body(&binding.planet_name))
    else {
        return;
    };
    let body_radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
    let to_map = |position_m: DVec3| {
        let orientation =
            ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)?;
        let position_bf = planet_inertial_to_body_fixed(position_m, orientation);
        let direction = position_bf.normalize_or_zero();
        if direction.length_squared() <= 1e-12 {
            return None;
        }
        let (latitude_deg, longitude_deg) = body_fixed_to_terrain_lat_lon(direction);
        equirectangular_point(latitude_deg, longitude_deg, MAP_WIDTH_PX, MAP_HEIGHT_PX)
            .map(|point| (point, latitude_deg))
    };
    let current = to_map(rocket.dynamics.position_m).map(|point| point.0);
    let (launch_latitude_deg, launch_longitude_deg) =
        geodetic_to_terrain_lat_lon(launch_site, &planet.domain_planet);
    let launch = equirectangular_point(
        launch_latitude_deg,
        launch_longitude_deg,
        MAP_WIDTH_PX,
        MAP_HEIGHT_PX,
    );
    let target = (autopilot.target_landing_position_m.length_squared() > 1.0)
        .then(|| to_map(autopilot.target_landing_position_m))
        .flatten();
    if !terrain_map_update_due(&update_state, real_time.elapsed_secs()) {
        return;
    }
    let key = TerrainMapUpdateKey {
        planet: planet_entity,
        rocket_pixel: current.map(terrain_map_pixel_key),
        landing_pixel: target.map(|(point, _)| terrain_map_pixel_key(point)),
        mission: *mission,
        ground_contact: collision.ground_contact,
        resting: ground_rest.active,
        history_len: recorder.entries().len(),
        prediction_start_sim_time_bits: prediction_cache.prediction_start_sim_time_s().to_bits(),
    };
    update_state.initialized = true;
    update_state.last_update_real_time_s = real_time.elapsed_secs();
    if update_state.last_key == Some(key) {
        return;
    }
    update_state.last_key = Some(key);

    let raster = if let Some(raster) = cache.images.get(&planet.domain_planet.name) {
        raster.clone()
    } else {
        let data = terrain_map_raster(&*terrain.source, MAP_RASTER_WIDTH, MAP_RASTER_HEIGHT);
        let raster = images.add(Image::new(
            Extent3d {
                width: MAP_RASTER_WIDTH,
                height: MAP_RASTER_HEIGHT,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::RENDER_WORLD,
        ));
        cache
            .images
            .insert(planet.domain_planet.name.clone(), raster.clone());
        raster
    };
    for mut image in raster_query.iter_mut() {
        if image.image != raster {
            image.image = raster.clone();
        }
    }

    let prediction = prediction_cache.prediction();
    let impact = predicted_impact(prediction, body_radius_m)
        .and_then(|position| to_map(position).map(|point| point.0));
    let history = history_track(recorder, &planet.domain_planet);
    let predicted = prediction_track(
        prediction,
        &planet.domain_planet,
        prediction_cache.prediction_start_sim_time_s(),
    );
    let uncertainty = target
        .and_then(|(point, latitude_deg)| {
            landing_uncertainty_active(*mission).then(|| {
                uncertainty_ellipse_radii_px(
                    latitude_deg,
                    ACTIVE_LANDING_UNCERTAINTY_M,
                    body_radius_m,
                    MAP_WIDTH_PX,
                    MAP_HEIGHT_PX,
                )
                .map(|radii| (point, radii))
            })
        })
        .flatten();

    for (overlay, mut node, mut transform) in overlay_query.iter_mut() {
        match *overlay {
            TerrainMapOverlay::Rocket => set_marker(&mut node, current),
            TerrainMapOverlay::Launch => set_marker(&mut node, launch),
            TerrainMapOverlay::Landing => set_marker(&mut node, target.map(|point| point.0)),
            TerrainMapOverlay::Impact => set_marker(&mut node, impact),
            TerrainMapOverlay::Uncertainty => set_uncertainty(&mut node, uncertainty),
            TerrainMapOverlay::History(index) => {
                set_track_segment(
                    &mut node,
                    &mut transform,
                    history_segment(&history, index, HISTORY_SEGMENTS),
                );
            }
            TerrainMapOverlay::Prediction(index) => {
                set_track_segment(
                    &mut node,
                    &mut transform,
                    history_segment(&predicted, index, PREDICTION_SEGMENTS),
                );
            }
        }
    }
}

fn terrain_map_update_due(state: &TerrainMapUpdateState, now_s: f32) -> bool {
    !state.initialized || now_s >= state.last_update_real_time_s + MAP_UPDATE_INTERVAL_S
}

fn terrain_map_pixel_key(point: TerrainMapPoint) -> (i32, i32) {
    (point.x_px.round() as i32, point.y_px.round() as i32)
}

fn predicted_impact(prediction: &OrbitPrediction, body_radius_m: f64) -> Option<DVec3> {
    prediction
        .planet_frame_points
        .last()
        .copied()
        .filter(|point| point.length() <= body_radius_m + 1e-3)
}

fn history_track(
    recorder: &FlightRecorder,
    planet: &crate::domain::entities::planet::Planet,
) -> Vec<TerrainMapPoint> {
    recorder
        .entries()
        .iter()
        .filter_map(|entry| map_recorded_entry(entry, planet))
        .collect()
}

fn map_recorded_entry(
    entry: &FlightLogEntry,
    planet: &crate::domain::entities::planet::Planet,
) -> Option<TerrainMapPoint> {
    // Historical points have no matching snapshot orientation. This map is
    // presentation-only, so it retains the labelled catalog approximation
    // rather than performing per-sample kernel evaluation.
    let position_bf = catalog_body_fixed_to_inertial_rotation(planet, entry.time_s / 86_400.0)
        .inverse()
        * entry.position_m;
    let direction = position_bf.normalize_or_zero();
    (direction.length_squared() > 1e-12)
        .then(|| {
            let (latitude_deg, longitude_deg) = body_fixed_to_terrain_lat_lon(direction);
            equirectangular_point(latitude_deg, longitude_deg, MAP_WIDTH_PX, MAP_HEIGHT_PX)
        })
        .flatten()
}

fn prediction_track(
    prediction: &OrbitPrediction,
    planet: &crate::domain::entities::planet::Planet,
    start_time_s: f64,
) -> Vec<TerrainMapPoint> {
    // Future predicted points likewise have no shared snapshot orientation;
    // this non-authoritative overlay must not query kernels independently.
    prediction
        .planet_frame_points
        .iter()
        .zip(&prediction.planet_frame_times_s)
        .filter_map(|(position_m, relative_time_s)| {
            let position_bf = catalog_body_fixed_to_inertial_rotation(
                planet,
                (start_time_s + relative_time_s) / 86_400.0,
            )
            .inverse()
                * *position_m;
            let direction = position_bf.normalize_or_zero();
            (direction.length_squared() > 1e-12).then(|| {
                let (latitude_deg, longitude_deg) = body_fixed_to_terrain_lat_lon(direction);
                equirectangular_point(latitude_deg, longitude_deg, MAP_WIDTH_PX, MAP_HEIGHT_PX)
            })
        })
        .flatten()
        .collect()
}

fn history_segment(
    points: &[TerrainMapPoint],
    index: usize,
    max_segments: usize,
) -> Option<TerrainMapSegment> {
    if points.len() < 2 || index >= max_segments {
        return None;
    }
    let available = points.len() - 1;
    let start = index * available / max_segments;
    let end = ((index + 1) * available / max_segments).min(available);
    (end > start)
        .then(|| map_segment(points[start], points[end], MAP_WIDTH_PX))
        .flatten()
}

fn landing_uncertainty_active(mission: RocketMissionState) -> bool {
    matches!(
        mission,
        RocketMissionState::ReentryCorridor
            | RocketMissionState::PoweredDescent
            | RocketMissionState::UnpoweredDescent
            | RocketMissionState::Landing
    )
}

fn set_marker(node: &mut Node, point: Option<TerrainMapPoint>) {
    let Some(point) = point else {
        node.display = Display::None;
        return;
    };
    node.display = Display::Flex;
    node.left = Val::Px(point.x_px - 3.5);
    node.top = Val::Px(point.y_px - 3.5);
}

fn set_uncertainty(node: &mut Node, ellipse: Option<(TerrainMapPoint, Vec2)>) {
    let Some((center, radii)) = ellipse else {
        node.display = Display::None;
        return;
    };
    node.display = Display::Flex;
    node.width = Val::Px(radii.x * 2.0);
    node.height = Val::Px(radii.y * 2.0);
    node.left = Val::Px(center.x_px - radii.x);
    node.top = Val::Px(center.y_px - radii.y);
}

fn set_track_segment(
    node: &mut Node,
    transform: &mut UiTransform,
    segment: Option<TerrainMapSegment>,
) {
    let Some(segment) = segment else {
        node.display = Display::None;
        return;
    };
    node.display = Display::Flex;
    node.width = Val::Px(segment.length_px);
    node.left = Val::Px(segment.midpoint_px.x_px - segment.length_px * 0.5);
    node.top = Val::Px(segment.midpoint_px.y_px - 0.75);
    transform.rotation = Rot2::radians(segment.angle_rad);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FlatTerrain;

    impl TerrainSource for FlatTerrain {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            -10.0
        }
    }

    #[derive(Debug)]
    struct OverviewOnlyTerrain;

    impl TerrainSource for OverviewOnlyTerrain {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            panic!("map preview must not query authoritative height")
        }

        fn moisture(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            panic!("map preview must not query authoritative moisture")
        }

        fn overview_height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            100.0
        }

        fn overview_moisture(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            0.75
        }
    }

    #[test]
    fn equirectangular_projection_places_equator_and_poles() {
        assert_eq!(
            equirectangular_point(0.0, 0.0, 360.0, 180.0),
            Some(TerrainMapPoint {
                x_px: 180.0,
                y_px: 90.0,
            })
        );
        assert_eq!(
            equirectangular_point(90.0, -180.0, 360.0, 180.0),
            Some(TerrainMapPoint {
                x_px: 0.0,
                y_px: 0.0,
            })
        );
    }

    #[test]
    fn raster_uses_shared_surface_appearance_deterministically() {
        let a = terrain_map_raster(&FlatTerrain, 3, 2);
        let b = terrain_map_raster(&FlatTerrain, 3, 2);
        assert_eq!(a, b);
        assert_eq!(a.len(), 3 * 2 * 4);
        assert_eq!(&a[..4], &[30, 71, 106, 255]);
    }

    #[test]
    fn raster_uses_only_non_authoritative_overview_samples() {
        let raster = terrain_map_raster(&OverviewOnlyTerrain, 3, 2);
        assert_eq!(raster.len(), 3 * 2 * 4);
    }

    #[test]
    fn track_segments_do_not_cross_the_antimeridian() {
        let west = equirectangular_point(0.0, 179.0, 360.0, 180.0).unwrap();
        let east = equirectangular_point(0.0, -179.0, 360.0, 180.0).unwrap();
        assert!(map_segment(west, east, 360.0).is_none());
    }

    #[test]
    fn uncertainty_expands_horizontally_toward_the_poles() {
        let equator =
            uncertainty_ellipse_radii_px(0.0, 1_000.0, 6_371_000.0, 288.0, 144.0).unwrap();
        let high_latitude =
            uncertainty_ellipse_radii_px(75.0, 1_000.0, 6_371_000.0, 288.0, 144.0).unwrap();
        assert!(high_latitude.x > equator.x);
        assert_eq!(high_latitude.y, equator.y);
    }

    #[test]
    fn map_updates_are_cadence_limited() {
        let state = TerrainMapUpdateState {
            initialized: true,
            last_update_real_time_s: 5.0,
            ..default()
        };
        assert!(!terrain_map_update_due(&state, 5.05));
        assert!(terrain_map_update_due(&state, 5.0 + MAP_UPDATE_INTERVAL_S));
    }

    #[test]
    fn map_pixel_key_ignores_subpixel_motion() {
        assert_eq!(
            terrain_map_pixel_key(TerrainMapPoint {
                x_px: 10.1,
                y_px: 20.4,
            }),
            terrain_map_pixel_key(TerrainMapPoint {
                x_px: 10.49,
                y_px: 20.49,
            })
        );
    }
}
