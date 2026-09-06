//! Force accumulation and authoritative rigid-body integration adapters.

use super::components::{
    AblationState, AerodynamicForces, ForceAccumulator, GravityAcceleration, MaxQTracker,
    RocketFlightConditions, RocketGeometry, RocketPhysicsState, SpecificForceAcceleration,
    TorqueAccumulator,
};
use crate::domain::services::aerodynamics::{
    aerodynamic_coefficients_with_nose_bluntness, aerodynamic_torque_body, angle_of_attack,
    angle_of_sideslip, center_of_pressure_m, drag_force_body, lift_force_body, side_force_body,
    update_max_q,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::entity_components::EntryPhysicsConfig;
use bevy::math::DVec3;
use bevy::prelude::{Query, Res};

/// Add gravity after all other force writers without overwriting their output.
pub fn accumulate_forces(
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &GravityAcceleration,
        &mut ForceAccumulator,
    )>,
) {
    for (rocket, gravity, mut force_accum) in rocket_query.iter_mut() {
        force_accum.add_force_n(gravity.value * rocket.dynamics.mass_kg);
    }
}

/// Integrate the authoritative f64 state and clear each fixed-tick accumulator.
#[expect(
    clippy::type_complexity,
    reason = "Integration consumes the complete fixed-tick force and rigid-body state."
)]
pub fn integrate_6dof(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &mut RocketPhysicsState,
        Option<&GravityAcceleration>,
        Option<&mut SpecificForceAcceleration>,
        &mut ForceAccumulator,
        &mut TorqueAccumulator,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (mut rocket, gravity, specific_force, mut force_accum, mut torque_accum) in
        rocket_query.iter_mut()
    {
        let net_force_n = force_accum.take_force_n();
        let net_torque_nm = torque_accum.take_torque_nm();
        if let Some(mut specific_force) = specific_force {
            let gravity_mps2 = gravity.map_or(DVec3::ZERO, |gravity| gravity.value);
            specific_force.value =
                net_force_n / rocket.dynamics.mass_kg.max(f64::MIN_POSITIVE) - gravity_mps2;
        }
        rocket.dynamics.integrate_translation(net_force_n, dt);
        rocket.dynamics.integrate_rotation(net_torque_nm, dt);
    }
}

/// Compute aerodynamic forces from the fixed-tick flight-condition authority.
pub fn aerodynamic_forces(
    config: Res<EntryPhysicsConfig>,
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketGeometry,
        &RocketFlightConditions,
        &AblationState,
        &mut AerodynamicForces,
        &mut MaxQTracker,
        &mut ForceAccumulator,
    )>,
) {
    for (rocket, geometry, conditions, ablation, mut aero, mut max_q, mut force_accum) in
        rocket_query.iter_mut()
    {
        aero.center_of_pressure_body = center_of_pressure_m(geometry.height_m as f64);
        if conditions.airspeed_mps < 1.0 || conditions.density_kg_m3 <= 0.0 {
            aero.force_body = DVec3::ZERO;
            continue;
        }

        max_q.max_q_pa = update_max_q(conditions.dynamic_pressure_pa, max_q.max_q_pa);
        let reference_area_m2 = std::f64::consts::PI * (geometry.radius_m as f64).powi(2);
        let body_velocity =
            rocket.dynamics.orientation.inverse() * conditions.atmosphere_relative_velocity_mps;
        let nose_radius_ratio =
            if ablation.nose_radius_m > 0.0 && config.nose_radius_initial_m > 0.0 {
                ablation.nose_radius_m / config.nose_radius_initial_m
            } else {
                1.0
            };
        let (cd, cl, cy) = aerodynamic_coefficients_with_nose_bluntness(
            angle_of_attack(body_velocity),
            angle_of_sideslip(body_velocity),
            nose_radius_ratio,
        );
        let q = conditions.dynamic_pressure_pa;
        let force_body = drag_force_body(q, cd, reference_area_m2, body_velocity)
            + lift_force_body(q, cl, reference_area_m2, body_velocity)
            + side_force_body(q, cy, reference_area_m2, body_velocity);
        aero.force_body = force_body;
        force_accum.add_force_n(rocket.dynamics.orientation * force_body);
    }
}

/// Apply aerodynamic force at the center of pressure as body-frame torque.
pub fn aerodynamic_torque(
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &AerodynamicForces,
        &mut TorqueAccumulator,
    )>,
) {
    for (rocket, aero, mut torque_accum) in rocket_query.iter_mut() {
        if aero.force_body.length_squared() > 0.0 {
            torque_accum.add_torque_nm(aerodynamic_torque_body(
                aero.force_body,
                aero.center_of_pressure_body,
                rocket.dynamics.center_of_mass_m,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use bevy::math::{DMat3, DQuat};
    use bevy::prelude::{App, Update};

    #[test]
    fn integration_records_non_gravitational_specific_force() {
        let mut app = App::new();
        app.insert_resource(SimulationTime::new(1.0 / 60.0));
        app.add_systems(Update, integrate_6dof);
        let entity = app
            .world_mut()
            .spawn((
                RocketPhysicsState {
                    dynamics: RocketDynamicsState {
                        position_m: DVec3::ZERO,
                        velocity_mps: DVec3::ZERO,
                        orientation: DQuat::IDENTITY,
                        angular_velocity_radps: DVec3::ZERO,
                        angular_acceleration_radps2: DVec3::ZERO,
                        mass_kg: 100.0,
                        inertia_body: DMat3::IDENTITY,
                        center_of_mass_m: DVec3::ZERO,
                    },
                },
                GravityAcceleration {
                    value: DVec3::new(0.0, -9.80665, 0.0),
                },
                SpecificForceAcceleration::default(),
                // 1,000 N of thrust plus the 980.665 N gravitational force.
                ForceAccumulator::from_force_n(DVec3::new(1_000.0, -980.665, 0.0)),
                TorqueAccumulator::from_torque_nm(DVec3::new(0.0, 10.0, 0.0)),
            ))
            .id();

        app.update();

        let specific_force = app
            .world()
            .get::<SpecificForceAcceleration>(entity)
            .unwrap();
        assert_eq!(specific_force.value, DVec3::new(10.0, 0.0, 0.0));
        assert_eq!(
            app.world()
                .get::<ForceAccumulator>(entity)
                .unwrap()
                .force_n(),
            DVec3::ZERO
        );
        assert_eq!(
            app.world()
                .get::<TorqueAccumulator>(entity)
                .unwrap()
                .torque_nm(),
            DVec3::ZERO
        );
    }
}
