//! Presentation adapters for the authoritative rocket dynamics state.

use super::components::{
    GroundRest, RocketMissionState, RocketPhysicsState, RocketRenderState, TipOverState,
};
use crate::domain::math::DVec3;
use crate::domain::services::rocket_dynamics::RocketDynamicsState;
use crate::infrastructure::bevy_adapters::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::terrain::render::RenderOrigin;
use bevy::prelude::{Query, Res, Time, Transform};
use bevy::time::Fixed;

/// Snapshot fixed-step simulation state for subsequent render interpolation.
#[expect(
    clippy::type_complexity,
    reason = "The snapshot query reads the cohesive state that controls one presentation transition."
)]
pub fn capture_render_state(
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketMissionState,
        Option<&GroundRest>,
        Option<&TipOverState>,
        &mut RocketRenderState,
    )>,
) {
    for (rocket, mission, ground_rest, tip_over, mut render) in rocket_query.iter_mut() {
        // Terrain and planet presentation use the latest fixed ephemeris pose.
        // During surface-constrained terminal states, interpolating the rocket
        // from the previous tick makes the chase camera sawtooth relative to
        // that surface once per fixed update.
        let is_toppling = tip_over.is_some_and(TipOverState::is_toppling);
        if !is_toppling
            && matches!(
                *mission,
                RocketMissionState::Landing
                    | RocketMissionState::Landed
                    | RocketMissionState::Crashed
            )
            || (!is_toppling && ground_rest.is_some_and(|rest| rest.active))
        {
            render.prev = rocket.dynamics;
            render.current = rocket.dynamics;
            continue;
        }
        render.prev = render.current;
        render.current = rocket.dynamics;
    }
}

/// Interpolate fixed snapshots and update presentation-only components.
pub fn interpolate_render_transform(
    render_origin: Res<RenderOrigin>,
    physical_scale: Res<PhysicalScale>,
    time: Res<Time<Fixed>>,
    mut rocket_query: Query<(&RocketPhysicsState, &RocketRenderState, &mut Transform)>,
) {
    let alpha = time.overstep_fraction() as f64;
    for (_rocket, render, mut transform) in rocket_query.iter_mut() {
        let interpolated = render_dynamics_state(*render, alpha);
        *transform = render_transform(interpolated, render_origin.origin, &physical_scale);
    }
}

/// Convert an authoritative f64 rocket state to a camera-relative Bevy transform.
/// Rebase before downcasting so local meter-scale motion survives solar distances.
pub fn render_transform(
    dynamics: RocketDynamicsState,
    local_origin: DVec3,
    scale: &PhysicalScale,
) -> Transform {
    let local_m = dynamics.position_m - local_origin;
    let display = DVec3::new(
        scale.flight_meters_to_units(local_m.x),
        scale.flight_meters_to_units(local_m.y),
        scale.flight_meters_to_units(local_m.z),
    )
    .as_vec3();
    Transform::from_translation(display).with_rotation(dynamics.orientation.as_quat())
}

/// Interpolate every rocket state at the same presentation timestamp as terrain.
///
/// A pre-launch rocket is fixed to a rotating planetary surface, so rendering
/// its newest fixed state against interpolated terrain makes it visibly snap
/// across the pad once per physics step.
pub(crate) fn render_dynamics_state(render: RocketRenderState, alpha: f64) -> RocketDynamicsState {
    let previous = render.prev;
    let current = render.current;
    RocketDynamicsState {
        position_m: previous.position_m.lerp(current.position_m, alpha),
        velocity_mps: previous.velocity_mps.lerp(current.velocity_mps, alpha),
        orientation: previous.orientation.slerp(current.orientation, alpha),
        angular_velocity_radps: previous
            .angular_velocity_radps
            .lerp(current.angular_velocity_radps, alpha),
        angular_acceleration_radps2: current.angular_acceleration_radps2,
        mass_kg: current.mass_kg,
        inertia_body: current.inertia_body,
        center_of_mass_m: current.center_of_mass_m,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::math::{DMat3, DQuat};
    use bevy::math::Vec3;

    #[test]
    fn render_transform_rebases_before_f32_conversion() {
        let scale = PhysicalScale::default();
        let local_origin = DVec3::new(1.0e12, -2.0e12, 3.0e12);
        let dynamics = RocketDynamicsState::new(
            local_origin + DVec3::new(1.0, -2.0, 3.0),
            DVec3::ZERO,
            DQuat::IDENTITY,
            1.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let transform = render_transform(dynamics, local_origin, &scale);
        assert_eq!(transform.translation, Vec3::new(1.0, -2.0, 3.0));
    }

    /// A render-origin change must rebase translation without altering the
    /// rendered attitude. This pins the presentation boundary that previously
    /// passed orientation through unchanged.
    #[test]
    fn render_transform_preserves_rotation_across_origin_rebase() {
        let scale = PhysicalScale::default();
        let orientation = DQuat::from_axis_angle(DVec3::new(0.2, 0.9, -0.3).normalize(), 1.2);
        let base_position_m = DVec3::new(6_600_000.0, -1_200_000.0, 300_000.0);
        let dynamics = RocketDynamicsState::new(
            base_position_m,
            DVec3::ZERO,
            orientation,
            1.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let before = render_transform(dynamics, base_position_m, &scale);
        let after = render_transform(
            dynamics,
            base_position_m + DVec3::new(50.0, -10.0, 5.0),
            &scale,
        );
        assert_eq!(before.rotation, after.rotation);
        assert_eq!(before.rotation, orientation.as_quat());
    }

    /// The interpolated render quaternion must land between the two fixed
    /// snapshots and exactly on each endpoint, even when one snapshot is stored
    /// in the opposite quaternion hemisphere (`q` and `-q` are the same
    /// rotation). This is the intermediate-frame interpolation the flight
    /// matrix compares against.
    #[test]
    fn render_dynamics_state_slerps_intermediate_orientation() {
        let a = DQuat::from_axis_angle(DVec3::Y, 0.10);
        let b = DQuat::from_axis_angle(DVec3::Y, 0.70);
        let dynamics_at = |orientation: DQuat| {
            RocketDynamicsState::new(
                DVec3::new(1.0, 2.0, 3.0),
                DVec3::new(4.0, 5.0, 6.0),
                orientation,
                10.0,
                DMat3::IDENTITY,
                DVec3::ZERO,
            )
        };
        let render = RocketRenderState {
            prev: dynamics_at(a),
            current: dynamics_at(b),
        };

        let start = render_dynamics_state(render, 0.0);
        let end = render_dynamics_state(render, 1.0);
        let mid = render_dynamics_state(render, 0.5);
        assert!(start.orientation.angle_between(a) < 1e-12);
        assert!(end.orientation.angle_between(b) < 1e-12);
        assert!(
            (mid.orientation.angle_between(a) - (b.angle_between(a) * 0.5)).abs() < 1e-9,
            "midpoint interpolation must be the half-angle slerp"
        );
        assert!(mid.position_m.abs_diff_eq(DVec3::new(1.0, 2.0, 3.0), 1e-12));
    }

    /// The presentation adapter is a pure projection of the authoritative
    /// state: producing a render transform must not mutate physical dynamics.
    #[test]
    fn render_transform_leaves_physical_state_unchanged() {
        let scale = PhysicalScale::default();
        let original = RocketDynamicsState::new(
            DVec3::new(6_600_000.0, 0.0, 0.0),
            DVec3::new(0.0, 7_000.0, 0.0),
            DQuat::from_axis_angle(DVec3::Z, 0.4),
            1_000.0,
            DMat3::IDENTITY,
            DVec3::ZERO,
        );
        let before = original;
        let _ = render_transform(original, DVec3::ZERO, &scale);
        assert_eq!(original, before);
    }
}
