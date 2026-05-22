use bevy::prelude::*;

use crate::domain::value_objects::education::EducationState;

/// Handle keyboard selection of journal entries (1-9).
pub fn handle_journal_selection(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<EducationState>,
) {
    if !state.panel_open || !state.journal_section_open {
        return;
    }

    for digit in 1..=9 {
        let key = match digit {
            1 => KeyCode::Digit1,
            2 => KeyCode::Digit2,
            3 => KeyCode::Digit3,
            4 => KeyCode::Digit4,
            5 => KeyCode::Digit5,
            6 => KeyCode::Digit6,
            7 => KeyCode::Digit7,
            8 => KeyCode::Digit8,
            9 => KeyCode::Digit9,
            _ => continue,
        };
        if keyboard.just_pressed(key) {
            state.current_entry_index = match state.current_entry_index {
                Some(i) if i == digit - 1 => None,
                _ => Some(digit - 1),
            };
        }
    }
}
