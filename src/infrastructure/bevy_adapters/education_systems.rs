use bevy::prelude::*;

use crate::domain::value_objects::education::{
    EducationMode, EducationState, JournalDatabase, UnlockCondition,
};
use crate::infrastructure::bevy_adapters::craft_components::{CraftComponent, CraftControlState};
use crate::presentation::education_data::create_journal_database;
use crate::presentation::education_panel::{
    spawn_education_panel, update_education_panel,
};
use crate::presentation::knowledge_journal::{
    spawn_knowledge_journal, update_journal_display,
};
use crate::presentation::compare_mode::{spawn_rocket_craft, update_rocket_physics, update_rocket_hud};

/// Handle education mode input toggles.
pub fn handle_education_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<EducationState>,
    mut journal: ResMut<JournalDatabase>,
    craft_query: Query<Entity, With<CraftComponent>>,
) {
    if keyboard.just_pressed(KeyCode::KeyB) {
        state.panel_open = !state.panel_open;
        if state.panel_open {
            state.journal_open = false;
            // Show unlock notifications on open
            for idx in journal.drain_notifications() {
                // Notifications handled by UI system
            }
        }
    }

    if keyboard.just_pressed(KeyCode::KeyJ) && state.panel_open {
        state.journal_open = !state.journal_open;
    }

    if keyboard.just_pressed(KeyCode::KeyC) && state.panel_open {
        state.mode = match state.mode {
            EducationMode::Education => EducationMode::Compare,
            EducationMode::Compare => EducationMode::Education,
            EducationMode::Simulation => EducationMode::Education,
        };
    }
}

/// Periodically check and unlock journal entries based on craft state.
pub fn check_journal_unlocks(
    time: Res<Time>,
    craft_query: Query<&CraftComponent>,
    control: Res<CraftControlState>,
    mut state: ResMut<EducationState>,
    mut journal: ResMut<JournalDatabase>,
) {
    state.flight_time += time.delta_secs();

    let craft_exists = craft_query.single().is_ok();
    let dc = control.dc_current;
    let pulse = control.pulse_current;

    for (i, entry) in journal.entries.clone().iter().enumerate() {
        if journal.is_unlocked(i) {
            continue;
        }

        let should_unlock = match entry.unlock {
            UnlockCondition::Immediate => true,
            UnlockCondition::CraftSpawned => craft_exists,
            UnlockCondition::PulseAbove(t) => craft_exists && pulse > t,
            UnlockCondition::AltitudeAbove(a) => {
                craft_query.single().ok().map_or(false, |c| c.physics.vertical_position > a)
            }
            UnlockCondition::SpeedAbove(s) => {
                craft_query.single().ok().map_or(false, |c| c.linear_velocity.length() > s)
            }
            UnlockCondition::OrbitAchieved => {
                // Simplified: altitude > 500 + speed > 500
                craft_query.single().ok().map_or(false, |c| {
                    c.physics.vertical_position > 500.0 && c.linear_velocity.length() > 500.0
                })
            }
            UnlockCondition::Landed => {
                craft_query.single().ok().map_or(false, |c| c.physics.vertical_position < 1.0)
            }
            UnlockCondition::TimeElapsed(t) => state.flight_time > t,
        };

        if should_unlock {
            journal.unlock(i);
        }
    }
}

/// Register education systems manually (for non-plugin mode).
pub fn register_education_systems(app: &mut App) {
    app.insert_resource(EducationState::default());
    app.insert_resource(create_journal_database());
    app.add_systems(Startup, (
        spawn_education_panel,
        spawn_knowledge_journal,
    ));
    app.add_systems(Update, (
        handle_education_input,
        check_journal_unlocks,
        update_education_panel,
        update_journal_display,
        spawn_rocket_craft,
        update_rocket_physics,
        update_rocket_hud,
    ).chain());
}
