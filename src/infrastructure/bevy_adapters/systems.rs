use bevy::prelude::*;
use crate::domain::value_objects::simulation_params::SimulationParameters;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::application::simulation_service::SimulationService;
use crate::domain::services::physics;
use super::components::*;

// System to update gyroscopes
pub fn update_gyroscopes(
    time: Res<Time>,
    params: Res<SimulationParameters>,
    mut query: Query<(&mut Transform, &mut GyroscopeComponent)>,
) {
    for (mut transform, mut gyro_comp) in query.iter_mut() {
        let spin_axis = Vec3::Y; // Assuming Y axis for spin
        SimulationService::update_gyroscope(&mut gyro_comp.domain_gyro, &params, spin_axis);
        let precession_angle = SimulationService::get_precession_angle(&gyro_comp.domain_gyro, time.delta_seconds());
        transform.rotate_y(precession_angle);
    }
}

// System to update thrust visualization
pub fn update_thrust(
    time: Res<Time>,
    params: Res<SimulationParameters>,
    gyro_query: Query<&GyroscopeComponent>,
    mut arrow_query: Query<&mut Transform, With<ThrustArrow>>,
) {
    let gyros: Vec<_> = gyro_query.iter().map(|g| &g.domain_gyro).collect();
    if gyros.is_empty() {
        return;
    }
    let total_thrust = SimulationService::calculate_thrust(&gyros, &params);

    for mut transform in arrow_query.iter_mut() {
        let scale = crate::domain::services::physics::calculate_arrow_scale(total_thrust);
        transform.scale = Vec3::new(0.1, 0.1, scale);

        if total_thrust.length() > 0.001 {
            let translation = transform.translation;
            let target = translation + total_thrust.normalize();
            transform.look_at(target, Vec3::Y);
        }

        transform.scale *= 1.0 + 0.1 * (time.elapsed_seconds() * 5.0).sin();
    }
}

// System to handle user input for controlling simulation parameters
pub fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut params: ResMut<SimulationParameters>,
    time: Res<Time>,
) {
    let rpm_delta = 5000.0 * time.delta_seconds(); // Adjust RPM by 5000 per second

    if keyboard.pressed(KeyCode::ArrowUp) {
        params.rpm += rpm_delta;
        println!("RPM increased to: {:.0}", params.rpm);
    }
    if keyboard.pressed(KeyCode::ArrowDown) {
        params.rpm -= rpm_delta;
        if params.rpm < 0.0 {
            params.rpm = 0.0;
        }
        println!("RPM decreased to: {:.0}", params.rpm);
    }

    // Optional: Add controls for other parameters
    let param_delta = 10.0 * time.delta_seconds();
    if keyboard.pressed(KeyCode::KeyW) {
        params.precession_hz += param_delta;
        println!("Precession Hz increased to: {:.1}", params.precession_hz);
    }
    if keyboard.pressed(KeyCode::KeyS) {
        params.precession_hz -= param_delta;
        if params.precession_hz < 0.0 {
            params.precession_hz = 0.0;
        }
        println!("Precession Hz decreased to: {:.1}", params.precession_hz);
    }

    if keyboard.pressed(KeyCode::KeyA) {
        params.asymmetry -= param_delta * 0.1;
        params.asymmetry = params.asymmetry.max(0.0).min(1.0);
        println!("Asymmetry decreased to: {:.2}", params.asymmetry);
    }
    if keyboard.pressed(KeyCode::KeyD) {
        params.asymmetry += param_delta * 0.1;
        params.asymmetry = params.asymmetry.max(0.0).min(1.0);
        println!("Asymmetry increased to: {:.2}", params.asymmetry);
    }
}

// System to update planet positions in their orbits
pub fn update_planet_positions(
    time: Res<Time>,
    solar_params: Res<SolarSystemParameters>,
    mut query: Query<(&mut Transform, &PlanetComponent)>,
) {
    let elapsed_seconds = time.elapsed_seconds();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    for (mut transform, planet_comp) in query.iter_mut() {
        let new_position = physics::calculate_planet_position(
            &planet_comp.domain_planet,
            time_days,
            &solar_params,
        );
        transform.translation = new_position;
    }
}

// System to update planet rotations
pub fn update_planet_rotations(
    time: Res<Time>,
    solar_params: Res<SolarSystemParameters>,
    mut query: Query<(&mut Transform, &PlanetComponent)>,
) {
    let elapsed_seconds = time.elapsed_seconds();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    for (mut transform, planet_comp) in query.iter_mut() {
        let rotation_angle = physics::calculate_planet_rotation(
            &planet_comp.domain_planet,
            time_days,
        );

        // Rotate around the planet's local Y axis (for simplicity)
        // In reality, planets have different rotation axes, but this works for visualization
        transform.rotation = Quat::from_rotation_y(rotation_angle);
    }
}

// System to handle solar system controls (time scale, etc.)
pub fn handle_solar_system_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut solar_params: ResMut<SolarSystemParameters>,
) {
    // Time scale controls
    if keyboard.pressed(KeyCode::KeyT) {
        solar_params.time_scale *= 1.1;
        println!("Time scale: {:.1}x", solar_params.time_scale);
    }
    if keyboard.pressed(KeyCode::KeyR) && solar_params.time_scale > 0.1 {
        solar_params.time_scale /= 1.1;
        println!("Time scale: {:.1}x", solar_params.time_scale);
    }

    // Reset time scale
    if keyboard.pressed(KeyCode::KeyY) {
        solar_params.time_scale = 1.0;
        println!("Time scale reset to: {:.1}x", solar_params.time_scale);
    }

    // Toggle orbit visualization (placeholder for future feature)
    if keyboard.just_pressed(KeyCode::KeyO) {
        solar_params.show_orbits = !solar_params.show_orbits;
        println!("Orbit visualization: {}", if solar_params.show_orbits { "ON" } else { "OFF" });
    }
}