//! Compact body-fixed terrain map for rocket-mode flight presentation.
//!
//! The map is deliberately a read-only consumer: its cached raster samples the
//! same per-body [`TerrainSource`] and [`surface_appearance`] law as terrain
//! rendering, while all overlays are derived from authoritative state.

use crate::components::rocket::{
    PlannedManeuver, RocketAutopilot, RocketMissionState, RocketPhysicsState, RocketPlanetBinding,
};
use crate::domain::services::reference_frames::planet_inertial_to_body_fixed;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_source::{slope_deg_at, surface_appearance, TerrainSource};
use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
use crate::infrastructure::bevy_adapters::components::{PlanetComponent, PlanetTerrain};
use crate::infrastructure::bevy_adapters::rocket_orbit::{
    predicted_orbit_with_maneuver, OrbitPrediction,
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

/// Build an RGBA terrain raster from the shared terrain visual law. This is a
/// pure function so cache creation does not introduce another terrain model.
pub fn terrain_map_raster(source: &dyn TerrainSource, width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let latitude_deg = 90.0 - (y as f64 + 0.5) * 180.0 / height as f64;
        for x in 0..width {
            let longitude_deg = -180.0 + (x as f64 + 0.5) * 360.0 / width as f64;
            let elevation_m = source.height_m(latitude_deg, longitude_deg);
            let appearance = surface_appearance(
                elevation_m,
                source.moisture(latitude_deg, longitude_deg),
                source.zone_lat(latitude_deg),
                slope_deg_at(source, latitude_deg, longitude_deg),
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
            .add_systems(Startup, spawn_terrain_map_panel)
            .add_systems(Update, update_terrain_map_panel);
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
                Text::new("BODY-FIXED MAP"),
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
fn update_terrain_map_panel(
    sim_time: Res<SimulationTime>,
    planet_query: Query<(&PlanetComponent, &PlanetTerrain)>,
    rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &LaunchSiteCoordinates,
        &RocketAutopilot,
        &RocketMissionState,
        &FlightRecorder,
        Option<&PlannedManeuver>,
    )>,
    mut cache: ResMut<TerrainMapRasterCache>,
    mut images: ResMut<Assets<Image>>,
    mut raster_query: Query<&mut ImageNode, With<TerrainMapRasterImage>>,
    mut overlay_query: Query<(&TerrainMapOverlay, &mut Node, &mut UiTransform)>,
) {
    let Some((binding, rocket, launch_site, autopilot, mission, recorder, maneuver)) =
        rocket_query.iter().next()
    else {
        return;
    };
    let Some((planet, terrain)) = planet_query
        .iter()
        .find(|(planet, _)| planet.matches_body(&binding.planet_name))
    else {
        return;
    };
    let body_name = planet.domain_planet.name.clone();
    let raster = cache.images.entry(body_name).or_insert_with(|| {
        let data = terrain_map_raster(&*terrain.source, MAP_RASTER_WIDTH, MAP_RASTER_HEIGHT);
        images.add(Image::new(
            Extent3d {
                width: MAP_RASTER_WIDTH,
                height: MAP_RASTER_HEIGHT,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::RENDER_WORLD,
        ))
    });
    for mut image in raster_query.iter_mut() {
        image.image = raster.clone();
    }

    let body_radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
    let to_map = |position_m: DVec3, at_time_s: f64| {
        let position_bf = planet_inertial_to_body_fixed(
            position_m,
            &planet.domain_planet,
            (at_time_s / 86_400.0) as f32,
        );
        let direction = position_bf.normalize_or_zero();
        if direction.length_squared() <= 1e-12 {
            return None;
        }
        let latitude_deg = direction.y.asin().to_degrees();
        let longitude_deg = direction.z.atan2(direction.x).to_degrees();
        equirectangular_point(latitude_deg, longitude_deg, MAP_WIDTH_PX, MAP_HEIGHT_PX)
            .map(|point| (point, latitude_deg))
    };
    let current = to_map(rocket.dynamics.position_m, sim_time.sim_time_s).map(|point| point.0);
    let launch = equirectangular_point(
        launch_site.latitude_deg as f64,
        launch_site.longitude_deg as f64,
        MAP_WIDTH_PX,
        MAP_HEIGHT_PX,
    );
    let target = (autopilot.target_landing_position_m.length_squared() > 1.0)
        .then(|| to_map(autopilot.target_landing_position_m, sim_time.sim_time_s))
        .flatten();
    let planned_impulse = maneuver.and_then(|maneuver| {
        let execute_after_s = maneuver.execute_at_sim_time_s - sim_time.sim_time_s;
        (execute_after_s > 0.0 && maneuver.delta_v_mps.is_finite()).then_some(
            crate::domain::services::trajectory::ManeuverImpulse {
                execute_after_s,
                delta_v_mps: maneuver.delta_v_mps,
            },
        )
    });
    let prediction = predicted_orbit_with_maneuver(
        rocket.dynamics.position_m,
        rocket.dynamics.velocity_mps,
        planet.domain_planet.mass_kg,
        body_radius_m,
        planned_impulse,
    );
    let impact = predicted_impact(&prediction, body_radius_m)
        .and_then(|position| to_map(position, sim_time.sim_time_s).map(|point| point.0));
    let history = history_track(recorder, &planet.domain_planet);
    let predicted = prediction_track(&prediction, &planet.domain_planet, sim_time.sim_time_s);
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
    let position_bf =
        planet_inertial_to_body_fixed(entry.position_m, planet, (entry.time_s / 86_400.0) as f32);
    let direction = position_bf.normalize_or_zero();
    (direction.length_squared() > 1e-12)
        .then(|| {
            equirectangular_point(
                direction.y.asin().to_degrees(),
                direction.z.atan2(direction.x).to_degrees(),
                MAP_WIDTH_PX,
                MAP_HEIGHT_PX,
            )
        })
        .flatten()
}

fn prediction_track(
    prediction: &OrbitPrediction,
    planet: &crate::domain::entities::planet::Planet,
    start_time_s: f64,
) -> Vec<TerrainMapPoint> {
    prediction
        .planet_frame_points
        .iter()
        .zip(&prediction.planet_frame_times_s)
        .filter_map(|(position_m, relative_time_s)| {
            let position_bf = planet_inertial_to_body_fixed(
                *position_m,
                planet,
                ((start_time_s + relative_time_s) / 86_400.0) as f32,
            );
            let direction = position_bf.normalize_or_zero();
            (direction.length_squared() > 1e-12).then(|| {
                equirectangular_point(
                    direction.y.asin().to_degrees(),
                    direction.z.atan2(direction.x).to_degrees(),
                    MAP_WIDTH_PX,
                    MAP_HEIGHT_PX,
                )
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
}
