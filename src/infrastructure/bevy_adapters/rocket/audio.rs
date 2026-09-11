//! Rocket-only audio control derivation.
//!
//! This module deliberately creates no audio players or source assets. It
//! provides bounded controls for future licensed Rocket audio at a fixed render
//! cadence, without reusing the craft/UFO audio loop.

use super::components::{
    RocketAudioControls, RocketCameraMode, RocketFlightConditions, RocketPropulsion, ThermalState,
};
use super::presentation_parameters::{map_presentation_parameters, RocketPresentationInputs};
use bevy::prelude::*;

const AUDIO_CONTROL_UPDATE_INTERVAL_S: f32 = 0.05;
const STAGING_AUDIO_DURATION_S: f32 = 2.0;

/// Bounded cadence state for Rocket audio-control derivation.
#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct RocketAudioControlCadence {
    last_update_at_s: f32,
}

impl Default for RocketAudioControlCadence {
    fn default() -> Self {
        Self {
            last_update_at_s: f32::NEG_INFINITY,
        }
    }
}

/// Samples Rocket presentation and authoritative state for future audio playback.
///
/// This runs at most 20 Hz because gain and pitch controls do not need per-frame
/// updates. It writes only presentation components and does not create playback.
#[expect(
    clippy::type_complexity,
    reason = "One audio-control pass reads the cohesive authoritative and presentation state."
)]
pub(crate) fn update_rocket_audio_controls(
    mut commands: Commands,
    time: Res<Time>,
    camera_mode: Res<RocketCameraMode>,
    mut cadence: ResMut<RocketAudioControlCadence>,
    mut rockets: Query<(
        Entity,
        &RocketPropulsion,
        &RocketFlightConditions,
        &ThermalState,
        &Transform,
        Option<&mut RocketAudioControls>,
    )>,
    cameras: Query<&Transform, With<Camera3d>>,
) {
    let now_s = time.elapsed_secs();
    if now_s - cadence.last_update_at_s < AUDIO_CONTROL_UPDATE_INTERVAL_S {
        return;
    }
    cadence.last_update_at_s = now_s;

    for (entity, propulsion, conditions, thermal, rocket_transform, existing_controls) in
        &mut rockets
    {
        let engine_running = propulsion.has_running_engines();
        let presentation = map_presentation_parameters(RocketPresentationInputs {
            throttle_unit: f64::from(propulsion.throttle),
            thrust_fraction_unit: f64::from(engine_running as u8),
            ignition_elapsed_s: 1.0,
            ignition_ramp_duration_s: 1.0,
            ambient_pressure_pa: conditions.ambient_pressure_pa,
            density_kg_m3: conditions.density_kg_m3,
            terrain_distance_m: conditions.altitude_m,
            mach_number: conditions.mach_number,
            dynamic_pressure_pa: conditions.dynamic_pressure_pa,
            total_heat_flux_w_m2: thermal.total_heat_flux_w_m2,
            observer_distance_m: f64::from(nearest_camera_distance_m(rocket_transform, &cameras)),
        });
        let next_controls = RocketAudioControls {
            engine_gain_unit: presentation.ignition_intensity_unit as f32,
            engine_pitch_ratio: 0.8 + presentation.plume_intensity_unit as f32 * 0.4,
            ground_rumble_gain_unit: presentation.ground_effect_intensity_unit as f32,
            staging_gain_unit: staging_gain_unit(propulsion),
            interior_attenuation_unit: f32::from(matches!(*camera_mode, RocketCameraMode::Cockpit)),
            external_attenuation_unit: presentation.external_audio_attenuation_unit as f32,
        };

        if let Some(mut existing) = existing_controls {
            *existing = next_controls;
        } else {
            commands.entity(entity).insert(next_controls);
        }
    }
}

fn nearest_camera_distance_m(
    rocket_transform: &Transform,
    cameras: &Query<&Transform, With<Camera3d>>,
) -> f32 {
    cameras
        .iter()
        .map(|camera| camera.translation.distance(rocket_transform.translation))
        .reduce(f32::min)
        .unwrap_or(0.0)
}

fn staging_gain_unit(propulsion: &RocketPropulsion) -> f32 {
    if propulsion.separations_count == 0 {
        return 0.0;
    }
    (1.0 - propulsion.time_since_separation_s / STAGING_AUDIO_DURATION_S).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::rocket::Rocket;

    #[test]
    fn staging_control_is_bounded_and_decays_from_authoritative_separation_time() {
        let mut propulsion =
            RocketPropulsion::for_fresh_flight(Rocket::falcon9_test_fixture(), 0.0, 0.0);
        propulsion.separations_count = 1;
        propulsion.time_since_separation_s = 0.5;

        assert_eq!(staging_gain_unit(&propulsion), 0.75);

        propulsion.time_since_separation_s = 5.0;
        assert_eq!(staging_gain_unit(&propulsion), 0.0);
    }
}
