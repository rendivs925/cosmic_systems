use crate::domain::entities::craft::Craft;

#[derive(Debug, Clone)]
pub struct CraftPhysicsState {
    pub lift_force: f32,
    pub zpe_kilowatts: f32,
    pub net_energy_mj: f32,
    pub parametric_gain: bool,
    pub net_accel: f32,
    pub vertical_velocity: f32,
    pub vertical_position: f32,
}

impl Default for CraftPhysicsState {
    fn default() -> Self {
        Self {
            lift_force: 0.0,
            zpe_kilowatts: 0.0,
            net_energy_mj: 0.0,
            parametric_gain: false,
            net_accel: 0.0,
            vertical_velocity: 0.0,
            vertical_position: 5.0,
        }
    }
}

const GRAVITY: f32 = 0.29;

pub fn calculate_lift(dc: f32) -> f32 {
    (47.0 * dc.powf(1.35)).min(65.0)
}

pub fn calculate_zpe(pulse: f32, dc: f32) -> f32 {
    let base = 210.0 * pulse.powf(1.8);
    let parametric_boost = if pulse > 0.42 {
        1.0 + (pulse - 0.42) * 2.6
    } else {
        1.0
    };
    let synergy = 1.0 + 0.4 * dc;
    (base * parametric_boost * synergy).min(1250.0)
}

pub fn parametric_gain_active(pulse: f32) -> bool {
    pulse > 0.42
}

pub fn compute_physics(craft: &Craft, state: &mut CraftPhysicsState, dc: f32, pulse: f32, dt: f32) {
    state.lift_force = calculate_lift(dc);
    state.zpe_kilowatts = calculate_zpe(pulse, dc);
    state.parametric_gain = parametric_gain_active(pulse);

    let net_vertical = state.lift_force - craft.weight_kilonewtons;
    state.net_accel = net_vertical / craft.mass_tonnes - GRAVITY;

    state.vertical_velocity += state.net_accel * dt;
    state.vertical_position += state.vertical_velocity * dt;
    state.net_energy_mj += state.zpe_kilowatts * dt / 1000.0;
}
