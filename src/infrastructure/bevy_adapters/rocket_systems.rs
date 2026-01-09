use crate::infrastructure::bevy_adapters::components::*;
use bevy::prelude::*;

// System to update rocket physics
pub fn update_rocket_physics(
    time: Res<Time>,
    mut rocket_query: Query<(&mut RocketComponent, &mut Transform)>,
) {
    let dt = time.delta_secs();

    for (mut rocket, mut transform) in rocket_query.iter_mut() {
        // Basic physics integration
        // F = ma, so a = F/m
        let acceleration = rocket.thrust / rocket.mass;
        let current_velocity = rocket.velocity;

        // Update velocity and position
        rocket.velocity += acceleration * dt;
        rocket.position += current_velocity * dt;

        // Update angular velocity
        rocket.orientation = rocket.orientation * Quat::from_vec4(rocket.angular_velocity.extend(0.0)) * dt;

        // Update transform
        transform.translation = rocket.position;
        transform.rotation = rocket.orientation;

        // Fuel consumption (simplified)
        if rocket.fuel_mass > 0.0 && rocket.thrust.length() > 0.0 {
            let mass_flow_rate = 100.0; // kg/s - simplified
            rocket.fuel_mass = (rocket.fuel_mass - mass_flow_rate * dt).max(0.0);
            rocket.mass = rocket.dry_mass_kg + rocket.fuel_mass;
        }
    }
}

// System to handle rocket controls (placeholder)
pub fn update_rocket_controls(
    time: Res<Time>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut rocket_query: Query<&mut RocketComponent>,
) {
    let dt = time.delta_secs();

    for mut rocket in rocket_query.iter_mut() {
        // Simple thrust control
        let mut thrust = Vec3::ZERO;

        if keyboard_input.pressed(KeyCode::Space) {
            thrust.y = 100000.0; // Upward thrust
        }

        // Basic attitude control
        let mut torque = Vec3::ZERO;

        if keyboard_input.pressed(KeyCode::KeyW) {
            torque.x = 10.0; // Pitch up
        }
        if keyboard_input.pressed(KeyCode::KeyS) {
            torque.x = -10.0; // Pitch down
        }
        if keyboard_input.pressed(KeyCode::KeyA) {
            torque.z = 10.0; // Roll left
        }
        if keyboard_input.pressed(KeyCode::KeyD) {
            torque.z = -10.0; // Roll right
        }

        rocket.thrust = thrust;
        rocket.angular_velocity += torque * dt;
    }
}