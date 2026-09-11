//! Rocket-only derived controls and playback for locally reviewed engine audio.
//!
//! The fixed 20 Hz control pass samples authoritative state. Playback owns one
//! stable external engine loop per Rocket and only creates a short one-shot on
//! ignition or staging, so audio never participates in flight authority.

use super::components::{
    RocketAudioControls, RocketCameraMode, RocketFlightConditions, RocketPropulsion, ThermalState,
};
use super::presentation_parameters::{map_presentation_parameters, RocketPresentationInputs};
use bevy::audio::{AudioSinkPlayback, PlaybackMode, Volume};
use bevy::prelude::*;

const AUDIO_CONTROL_UPDATE_INTERVAL_S: f32 = 0.05;
const STAGING_AUDIO_DURATION_S: f32 = 2.0;
const ENGINE_LOOP_VOLUME: f32 = 0.72;
const INTERIOR_EXTERIOR_MIX: f32 = 0.35;
const ENGINE_AUDIO_SMOOTHING_UNIT: f32 = 0.45;
const IGNITION_TRIGGER_THRESHOLD: f32 = 0.01;

/// Marks the stable engine-loop source owned by a Rocket entity.
#[derive(Component)]
pub(crate) struct RocketEngineAudioLoop {
    owner: Entity,
}

/// Tracks event edges and the smoothed output gain for one Rocket's audio.
#[derive(Component, Default)]
pub(crate) struct RocketAudioPlaybackState {
    was_engine_running: bool,
    was_staging: bool,
    engine_gain_unit: f32,
}

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

/// Samples Rocket presentation and authoritative state for playback controls.
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

/// Adds one muted looping source for each Rocket. This runs once per vehicle;
/// gain and pitch changes are applied through its sink rather than respawning it.
pub(crate) fn ensure_rocket_audio_playback(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    rockets: Query<Entity, (With<RocketAudioControls>, Without<RocketAudioPlaybackState>)>,
) {
    for rocket in &rockets {
        commands
            .entity(rocket)
            .insert(RocketAudioPlaybackState::default());
        commands.entity(rocket).with_children(|parent| {
            parent.spawn((
                RocketEngineAudioLoop { owner: rocket },
                AudioPlayer::new(asset_server.load("sounds/rocket_engine_loop.ogg")),
                PlaybackSettings {
                    mode: PlaybackMode::Loop,
                    volume: Volume::Linear(0.0),
                    ..default()
                },
                Name::new("RocketExternalEngineAudio"),
            ));
        });
    }
}

/// Applies the current bounded controls to existing audio sinks. The external
/// loop is intentionally silent in vacuum; cockpit view retains only a quiet
/// approximation of exterior sound until a modeled interior source exists.
pub(crate) fn apply_rocket_audio_playback(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut rockets: Query<
        (Entity, &RocketAudioControls, &mut RocketAudioPlaybackState),
        Changed<RocketAudioControls>,
    >,
    mut engine_loops: Query<(&RocketEngineAudioLoop, &mut AudioSink)>,
) {
    for (rocket, controls, mut playback) in &mut rockets {
        let target_gain = external_engine_gain_unit(*controls);
        playback.engine_gain_unit +=
            (target_gain - playback.engine_gain_unit) * ENGINE_AUDIO_SMOOTHING_UNIT;
        let engine_running = target_gain > IGNITION_TRIGGER_THRESHOLD;
        let staging = controls.staging_gain_unit > IGNITION_TRIGGER_THRESHOLD;

        if (engine_running && !playback.was_engine_running) || (staging && !playback.was_staging) {
            // Transitions are rare lifecycle events, unlike the steady loop.
            commands.spawn((
                AudioPlayer::new(asset_server.load("sounds/rocket_ignition.ogg")),
                PlaybackSettings {
                    mode: PlaybackMode::Despawn,
                    volume: Volume::Linear((0.18 + 0.52 * target_gain).clamp(0.0, 0.7)),
                    speed: (0.88 + controls.engine_pitch_ratio * 0.12).clamp(0.8, 1.2),
                    ..default()
                },
                Name::new("RocketEngineTransitionAudio"),
            ));
        }
        playback.was_engine_running = engine_running;
        playback.was_staging = staging;

        for (source, mut sink) in &mut engine_loops {
            if source.owner != rocket {
                continue;
            }
            sink.set_volume(Volume::Linear(
                (ENGINE_LOOP_VOLUME * playback.engine_gain_unit).clamp(0.0, ENGINE_LOOP_VOLUME),
            ));
            sink.set_speed(controls.engine_pitch_ratio.clamp(0.8, 1.2));
        }
    }
}

fn external_engine_gain_unit(controls: RocketAudioControls) -> f32 {
    let cockpit_mix = 1.0 - controls.interior_attenuation_unit * (1.0 - INTERIOR_EXTERIOR_MIX);
    (controls.engine_gain_unit * controls.external_attenuation_unit * cockpit_mix).clamp(0.0, 1.0)
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

    #[test]
    fn external_engine_mix_respects_atmospheric_and_cockpit_attenuation() {
        let exterior = RocketAudioControls {
            engine_gain_unit: 1.0,
            external_attenuation_unit: 1.0,
            ..default()
        };
        let cockpit = RocketAudioControls {
            interior_attenuation_unit: 1.0,
            ..exterior
        };
        let vacuum = RocketAudioControls {
            external_attenuation_unit: 0.0,
            ..exterior
        };

        assert_eq!(external_engine_gain_unit(exterior), 1.0);
        assert!(external_engine_gain_unit(cockpit) < external_engine_gain_unit(exterior));
        assert_eq!(external_engine_gain_unit(vacuum), 0.0);
    }
}
