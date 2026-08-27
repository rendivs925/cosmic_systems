//! Presentation adapters for the authoritative rocket dynamics state.

use crate::components::rocket::{
    RocketFacade, RocketMissionState, RocketPhysicsState, RocketRenderState,
};
use crate::domain::services::rocket_dynamics::RocketDynamicsState;
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::terrain_render::RenderOrigin;
use bevy::prelude::{Query, Res, Time, Transform};
use bevy::time::Fixed;

/// Snapshot fixed-step simulation state for subsequent render interpolation.
pub fn capture_render_state(
    mut rocket_query: Query<(&RocketPhysicsState, &mut RocketRenderState)>,
) {
    for (rocket, mut render) in rocket_query.iter_mut() {
        render.prev = render.current;
        render.current = rocket.dynamics;
    }
}

/// Interpolate fixed snapshots and update presentation-only components.
pub fn interpolate_render_transform(
    render_origin: Res<RenderOrigin>,
    physical_scale: Res<PhysicalScale>,
    time: Res<Time<Fixed>>,
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketRenderState,
        &RocketMissionState,
        &mut RocketFacade,
        &mut Transform,
    )>,
) {
    let alpha = time.overstep_fraction() as f64;
    for (rocket, render, mission, mut facade, mut transform) in rocket_query.iter_mut() {
        let interpolated = render_dynamics_state(*mission, rocket.dynamics, *render, alpha);
        *transform = interpolated.render_transform(render_origin.origin, &physical_scale);
        facade.position = transform.translation;
        facade.velocity = interpolated.velocity_mps.as_vec3();
        facade.orientation = interpolated.orientation.as_quat();
        facade.angular_velocity = interpolated.angular_velocity_radps.as_vec3();
        facade.mass = rocket.dynamics.mass_kg as f32;
    }
}

/// Prevent pre-launch interpolation between distinct rotating-body pad poses.
pub(crate) fn render_dynamics_state(
    mission: RocketMissionState,
    dynamics: RocketDynamicsState,
    render: RocketRenderState,
    alpha: f64,
) -> RocketDynamicsState {
    if mission == RocketMissionState::PreLaunch {
        return dynamics;
    }

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
