use super::craft_components::*;
use bevy::prelude::*;

#[derive(Component)]
pub struct DcFieldLabel;
#[derive(Component)]
pub struct LiftLabel;
#[derive(Component)]
pub struct EnergyLabel;
#[derive(Component)]
pub struct CamLabel;
#[derive(Component)]
pub struct FlightLabel;
#[derive(Component)]
pub struct EffectsLabel;

pub fn update_craft_ui(
    craft_query: Query<&CraftComponent>,
    control: Res<CraftControlState>,
    mut set: ParamSet<(
        Query<&mut Text, With<DcFieldLabel>>,
        Query<&mut Text, With<LiftLabel>>,
        Query<&mut Text, With<EnergyLabel>>,
        Query<&mut Text, With<CamLabel>>,
        Query<&mut Text, With<FlightLabel>>,
        Query<&mut Text, With<EffectsLabel>>,
    )>,
    effects_enabled: Res<CraftEffectsEnabled>,
) {
    let craft = match craft_query.single() {
        Ok(c) => c,
        _ => return,
    };

    for mut text in set.p0().iter_mut() {
        text.0 = format!("DC: {:.2}", control.dc_current);
    }
    for mut text in set.p1().iter_mut() {
        text.0 = format!("Lift: {:.1} kN", craft.physics.lift_force);
    }
    for mut text in set.p2().iter_mut() {
        text.0 = format!("Energy: {:.2} MJ", craft.physics.net_energy_mj);
    }
    for mut text in set.p3().iter_mut() {
        let name = match craft.camera_mode {
            CraftCameraMode::Chase => "Chase",
            CraftCameraMode::Orbit => "Orbit",
            CraftCameraMode::FirstPerson => "First-Person",
            CraftCameraMode::Free => "Free",
            CraftCameraMode::Cinematic => "Cinematic",
        };
        text.0 = format!("CAM: {}", name);
    }
    for mut text in set.p4().iter_mut() {
        let speed = craft.linear_velocity.length();
        let alt = craft.physics.vertical_position;
        let mode = match craft.speed_mode {
            SpeedMode::Hover => "HOV",
            SpeedMode::Cruise => "CRZ",
            SpeedMode::Sprint => "SPR",
        };
        text.0 = format!("{:.0}m/s  {:.0}m  {}", speed, alt, mode);
    }
    for mut text in set.p5().iter_mut() {
        text.0 = if effects_enabled.0 {
            "FX: ON".to_string()
        } else {
            "FX: OFF".to_string()
        };
    }
}
