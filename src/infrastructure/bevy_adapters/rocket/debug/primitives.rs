//! Shared debug-draw primitives (arrow, render origin, unit conversion).

use super::super::components::RocketPlanetBinding;
use crate::infrastructure::bevy_adapters::entity_components::PlanetComponent;
use crate::infrastructure::bevy_adapters::physical_scale::PhysicalScale;
use bevy::math::DVec3;
use bevy::prelude::*;

use super::RocketDebugGizmos;

/// Shared vector-with-arrowhead primitive. All force/velocity vectors route
/// through here so there is exactly one arrow implementation.
pub(super) fn draw_vector(
    gizmos: &mut Gizmos<RocketDebugGizmos>,
    origin: Vec3,
    vector: Vec3,
    color: Color,
    visual_scale: f32,
    max_visual_length: f32,
) {
    let magnitude = vector.length();
    if magnitude < 1e-9 || !magnitude.is_finite() {
        return;
    }
    let dir = vector / magnitude;
    let visual_len = (visual_scale * magnitude).min(max_visual_length).max(0.5);

    // Shaft
    gizmos.ray(origin, dir * visual_len, color);

    // Arrowhead: four barbs around the tip.
    let tip = origin + dir * visual_len;
    let reference = if dir.y.abs() > 0.95 { Vec3::X } else { Vec3::Y };
    let side_a = dir.cross(reference).normalize_or_zero();
    let side_b = dir.cross(side_a).normalize_or_zero();
    let base = tip - dir * (visual_len * 0.12);
    let barb = visual_len * 0.06;

    gizmos.line(tip, base + side_a * barb, color);
    gizmos.line(tip, base - side_a * barb, color);
    gizmos.line(tip, base + side_b * barb, color);
    gizmos.line(tip, base - side_b * barb, color);
}

/// World-space render origin of the rocket, matching `sync_render_transform`:
/// planet translation + PhysicalScale-converted planet-centered meters.
pub(super) fn rocket_render_origin(
    planet_translation: DVec3,
    scale: &PhysicalScale,
    position_m: DVec3,
) -> Vec3 {
    (planet_translation
        + DVec3::new(
            scale.solar_meters_to_units(position_m.x),
            scale.solar_meters_to_units(position_m.y),
            scale.solar_meters_to_units(position_m.z),
        ))
    .as_vec3()
}

/// Convert a planet-centered meter offset to render-unit offset.
pub(super) fn offset_to_units(scale: &PhysicalScale, offset_m: DVec3) -> Vec3 {
    DVec3::new(
        scale.solar_meters_to_units(offset_m.x),
        scale.solar_meters_to_units(offset_m.y),
        scale.solar_meters_to_units(offset_m.z),
    )
    .as_vec3()
}

pub(super) fn find_bound_planet<'a>(
    planet_query: &'a Query<(&PlanetComponent, &Transform)>,
    binding: &RocketPlanetBinding,
) -> Option<(&'a PlanetComponent, &'a Transform)> {
    planet_query
        .iter()
        .find(|(planet, _)| planet.matches_body(&binding.planet_name))
}
