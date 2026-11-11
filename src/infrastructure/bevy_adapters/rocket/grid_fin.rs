//! Grid-fin aerodynamic force and torque adapter.
//!
//! Control writes the fin deflections into [`GridFins`]; this system turns
//! them into the body-frame force and torque the rigid-body integrator
//! consumes. The fin force is additional to the airframe drag/lift computed by
//! [`super::dynamics::aerodynamic_forces`]; both share the same atmosphere
//! sample and accumulate into the same fixed-tick force/torque budget.

use super::components::{
    ForceAccumulator, GridFins, RocketFlightConditions, RocketPhysicsState, TorqueAccumulator,
};
use bevy::prelude::Query;

/// Add grid-fin force and torque for every entity carrying an installation.
/// The cells are additive writers, so this system may run in any order within
/// the force/torque stages.
pub fn grid_fin_aerodynamics(
    mut fin_query: Query<(
        &RocketPhysicsState,
        &RocketFlightConditions,
        &GridFins,
        &mut ForceAccumulator,
        &mut TorqueAccumulator,
    )>,
) {
    for (rocket, conditions, fins, mut force_accum, mut torque_accum) in fin_query.iter_mut() {
        if conditions.airspeed_mps < 1.0 || conditions.density_kg_m3 <= 0.0 {
            continue;
        }
        let center_of_mass_y_m = rocket.dynamics.center_of_mass_m.y;
        let force_body = fins.spec.force_body_n(
            &fins.deflections_rad,
            conditions.dynamic_pressure_pa,
            conditions.mach_number,
        );
        force_accum.add_force_n(rocket.dynamics.orientation * force_body);
        torque_accum.add_torque_nm(fins.spec.torque_body_nm(
            &fins.deflections_rad,
            conditions.dynamic_pressure_pa,
            conditions.mach_number,
            center_of_mass_y_m,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::grid_fin::GridFinSpec;
    use crate::domain::services::rocket_dynamics::{rocket_inertia_tensor, RocketDynamicsState};
    use bevy::math::{DQuat, DVec3};
    use bevy::prelude::{App, Update};

    fn app_with_fins(deflections_rad: [f64; 4]) -> App {
        let spec = GridFinSpec {
            count: 4,
            fin_area_m2: 1.86,
            radial_arm_m: 2.4,
            axial_station_m: 18.5,
            max_deflection_rad: 30.0_f64.to_radians(),
            normal_force_coefficient_per_rad: 2.0,
        };
        let (inertia, center_of_mass_m) = rocket_inertia_tensor(25_600.0, 0.0, 1.85, 41.2);
        let mut app = App::new();
        app.add_systems(Update, grid_fin_aerodynamics);
        app.world_mut().spawn((
            RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    DVec3::X * 6_371_000.0,
                    DVec3::ZERO,
                    DQuat::IDENTITY,
                    25_600.0,
                    inertia,
                    center_of_mass_m,
                ),
            },
            RocketFlightConditions::from_sample(
                crate::domain::services::atmosphere::FlightConditions::from_atmosphere(
                    10_000.0,
                    crate::domain::services::atmosphere::AtmosphereProperties {
                        temperature_k: 223.0,
                        pressure_pa: 26_500.0,
                        density_kg_m3: 0.414,
                        speed_of_sound_mps: 300.0,
                    },
                    DVec3::Y * 200.0,
                ),
            ),
            GridFins {
                spec,
                deflections_rad,
            },
            ForceAccumulator::default(),
            TorqueAccumulator::default(),
        ));
        app
    }

    #[test]
    fn deflected_fins_add_force_and_torque() {
        let mut app = app_with_fins([0.2, 0.2, 0.2, 0.2]);
        app.update();
        let (force, torque) = {
            let world = app.world_mut();
            let mut query = world.query::<(&ForceAccumulator, &TorqueAccumulator)>();
            let (force, torque) = query.single(world).expect("one rocket");
            (force.force_n(), torque.torque_nm())
        };
        assert!(force.length() > 0.0, "deflected fins must produce force");
        assert!(
            torque.length() > 0.0,
            "deflected fins must produce torque about the center of mass"
        );
    }

    #[test]
    fn undeflected_fins_produce_nothing() {
        let mut app = app_with_fins([0.0; 4]);
        app.update();
        let (force, torque) = {
            let world = app.world_mut();
            let mut query = world.query::<(&ForceAccumulator, &TorqueAccumulator)>();
            let (force, torque) = query.single(world).expect("one rocket");
            (force.force_n(), torque.torque_nm())
        };
        assert_eq!(force, DVec3::ZERO);
        assert_eq!(torque, DVec3::ZERO);
    }
}
