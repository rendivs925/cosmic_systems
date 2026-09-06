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
