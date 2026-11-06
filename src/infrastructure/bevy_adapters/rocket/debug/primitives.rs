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
/// the bound planet is rendered at `-render_origin`, so a planet-centered
/// position becomes `position - origin` in the camera-relative flight frame.
///
/// The flight frame is one display unit per metre. Using the solar map's
/// metres-to-units factor here shrank every debug vector by ~five orders of
/// magnitude, which is why the overlay drew at the planet centre instead of
/// on the vehicle.
pub(super) fn rocket_render_origin(
    planet_translation: DVec3,
    scale: &PhysicalScale,
    position_m: DVec3,
) -> Vec3 {
    (planet_translation
        + DVec3::new(
            scale.flight_meters_to_units(position_m.x),
            scale.flight_meters_to_units(position_m.y),
            scale.flight_meters_to_units(position_m.z),
        ))
    .as_vec3()
}

/// Convert a planet-centered meter offset to render-unit offset in the flight
/// frame (one display unit per metre).
pub(super) fn offset_to_units(scale: &PhysicalScale, offset_m: DVec3) -> Vec3 {
    DVec3::new(
        scale.flight_meters_to_units(offset_m.x),
        scale.flight_meters_to_units(offset_m.y),
        scale.flight_meters_to_units(offset_m.z),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Debug geometry must land on the rendered vehicle. The flight frame is
    /// one unit per metre; the solar-map conversion shrank a 20 m axis to
    /// ~1e-5 units, so nothing was visible at the vehicle.
    #[test]
    fn debug_offsets_use_the_flight_frame() {
        let scale = PhysicalScale::default();
        assert_eq!(
            offset_to_units(&scale, DVec3::new(1.0, -2.0, 3.0)),
            Vec3::new(1.0, -2.0, 3.0)
        );

        // The bound planet is rendered at `-render_origin`.
        let render_origin = DVec3::new(6_600_000.0, -1_200_000.0, 300_000.0);
        let rocket_position_m = render_origin + DVec3::new(1.0, 2.0, 3.0);
        assert_eq!(
            rocket_render_origin(-render_origin, &scale, rocket_position_m),
            Vec3::new(1.0, 2.0, 3.0)
        );
    }
}
