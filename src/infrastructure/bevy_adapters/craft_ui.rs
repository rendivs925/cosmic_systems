use super::craft_components::*;
use bevy::prelude::*;

#[derive(Component)]
pub struct DcFieldLabel;

#[derive(Component)]
pub struct PulseLabel;

#[derive(Component)]
pub struct LiftLabel;

#[derive(Component)]
pub struct ZpeLabel;

#[derive(Component)]
pub struct EnergyLabel;

#[derive(Component)]
pub struct CamLabel;

#[derive(Component)]
pub struct GainLabel;

pub fn update_craft_ui(
    craft_query: Query<&CraftComponent>,
    control: Res<CraftControlState>,
    mut dc_query: Query<&mut Text, With<DcFieldLabel>>,
    mut pulse_query: Query<&mut Text, (With<PulseLabel>, Without<DcFieldLabel>)>,
    mut lift_query: Query<&mut Text, (With<LiftLabel>, Without<DcFieldLabel>)>,
    mut zpe_query: Query<&mut Text, (With<ZpeLabel>, Without<DcFieldLabel>)>,
    mut energy_query: Query<&mut Text, (With<EnergyLabel>, Without<DcFieldLabel>)>,
    mut cam_query: Query<&mut Text, (With<CamLabel>, Without<DcFieldLabel>)>,
    mut gain_query: Query<&mut Text, (With<GainLabel>, Without<DcFieldLabel>)>,
) {
    let craft = match craft_query.single() {
        Ok(c) => c,
        _ => return,
    };

    for mut text in dc_query.iter_mut() {
        text.0 = format!("DC Field: {:.2}", control.dc_current);
    }
    for mut text in pulse_query.iter_mut() {
        text.0 = format!("Pulse: {:.2}", control.pulse_current);
    }
    for mut text in lift_query.iter_mut() {
        text.0 = format!("Lift: {:.1} kN", craft.physics.lift_force);
    }
    for mut text in zpe_query.iter_mut() {
        text.0 = format!("ZPE: {:.1} kW", craft.physics.zpe_kilowatts);
    }
    for mut text in energy_query.iter_mut() {
        text.0 = format!("Energy: {:.2} MJ", craft.physics.net_energy_mj);
    }
    for mut text in cam_query.iter_mut() {
        let mode = match craft.camera_mode {
            CraftCameraMode::External => "External",
            CraftCameraMode::FirstPerson => "First-Person",
        };
        text.0 = format!("CAM: {}", mode);
    }
    for mut text in gain_query.iter_mut() {
        if craft.physics.parametric_gain {
            text.0 = "PARAMETRIC GAIN ACTIVE".to_string();
        } else {
            if text.0 != "" {
                text.0 = "".to_string();
            }
        }
    }
}
