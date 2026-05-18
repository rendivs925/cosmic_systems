use bevy::prelude::*;

use crate::domain::value_objects::education::{
    EducationState, JournalCategory, JournalDatabase,
};

#[derive(Component)]
pub struct JournalRoot;

#[derive(Component)]
pub struct JournalEntryBody;

#[derive(Component)]
pub struct JournalQuranRef;

pub fn spawn_knowledge_journal(mut commands: Commands) {
    let accent = Color::srgb(0.3, 0.6, 0.9);
    let bright = Color::srgb(0.75, 0.8, 0.85);
    let dim = Color::srgb(0.35, 0.4, 0.45);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(16.0)),
                row_gap: Val::Px(8.0),
                overflow: Overflow::clip_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.015, 0.025, 0.95)),
            JournalRoot,
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("KNOWLEDGE JOURNAL"),
                TextFont { font_size: 16.0, ..default() },
                TextColor(accent),
            ));

            p.spawn((
                Text::new("Press J to close. Entries unlock as you explore."),
                TextFont { font_size: 11.0, ..default() },
                TextColor(dim),
            ));

            p.spawn((
                Text::new("Select an entry using number keys (1-9) to view details."),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.5, 0.7, 0.5)),
            ));

            p.spawn((
                Text::new("---"),
                TextFont { font_size: 10.0, ..default() },
                TextColor(dim),
            ));

            // Entry list placeholder (populated by system)
            p.spawn((
                Text::new(""),
                TextFont { font_size: 11.0, ..default() },
                TextColor(bright),
                JournalEntryBody,
            ));
            p.spawn((
                Text::new(""),
                TextFont { font_size: 10.0, ..default() },
                TextColor(Color::srgb(0.5, 0.7, 0.5)),
                JournalQuranRef,
            ));
        });
}

pub fn update_journal_display(
    keyboard: Res<ButtonInput<KeyCode>>,
    state: Res<EducationState>,
    journal: Res<JournalDatabase>,
    mut root_query: Query<&mut Visibility, With<JournalRoot>>,
    mut body_query: Query<&mut Text, (With<JournalEntryBody>, Without<JournalQuranRef>)>,
    mut quran_query: Query<&mut Text, (With<JournalQuranRef>, Without<JournalEntryBody>)>,
) {
    let Ok(mut vis) = root_query.single_mut() else { return };
    *vis = if state.journal_open { Visibility::Visible } else { Visibility::Hidden };
    if !state.journal_open { return; }

    // Build entry list text grouped by category
    let mut lines: Vec<String> = Vec::new();
    let categories = [
        JournalCategory::VacuumSuperfluid,
        JournalCategory::AsymmetricPolarization,
        JournalCategory::ZpeExtraction,
        JournalCategory::MetricEngineering,
        JournalCategory::QuranicEvidence,
    ];

    let mut entry_count = 0;
    for cat in &categories {
        let cat_entries: Vec<String> = journal
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.category == *cat)
            .map(|(i, e)| {
                if journal.is_unlocked(i) {
                    format!("  {}: {}", i + 1, e.title)
                } else {
                    format!("  {}: [LOCKED] {}", i + 1, e.title)
                }
            })
            .collect();

        let n = cat_entries.len();
        if n > 0 {
            lines.push(format!("[{}]", cat.label()));
            lines.extend(cat_entries);
        }
    }

    for mut text in body_query.iter_mut() {
        if lines.is_empty() {
            text.0 = "No entries yet. Fly the craft to unlock knowledge.".to_string();
        } else {
            text.0 = lines.join("\n");
        }
    }

    // Show current entry detail if one is selected via keyboard
    let selected = (1..=9).find(|k| {
        let key = match k {
            1 => KeyCode::Digit1, 2 => KeyCode::Digit2, 3 => KeyCode::Digit3,
            4 => KeyCode::Digit4, 5 => KeyCode::Digit5, 6 => KeyCode::Digit6,
            7 => KeyCode::Digit7, 8 => KeyCode::Digit8, 9 => KeyCode::Digit9,
            _ => return false,
        };
        keyboard.just_pressed(key)
    });

    for mut text in quran_query.iter_mut() {
        if let Some(idx) = selected {
            let real_idx = idx - 1;
            if let Some(entry) = journal.entries.get(real_idx) {
                if journal.is_unlocked(real_idx) {
                    let mut detail = format!("--- {} ---\n", entry.title);
                    for p in entry.body.iter() {
                        detail.push_str(p);
                        detail.push('\n');
                    }
                    if let Some(f) = entry.formula {
                        detail.push_str(&format!("\nFormula: {}\n", f));
                    }
                    for qref in entry.quranic_refs.iter() {
                        detail.push_str(&format!(
                            "\nSurah {}:{} — {}\n{}\n{}\n",
                            qref.sura, qref.verse, qref.arabic, qref.translation, qref.explanation
                        ));
                    }
                    text.0 = detail;
                } else {
                    text.0 = "This entry is locked. Continue exploring to unlock it.".to_string();
                }
            }
        } else if let Some(idx) = state.current_entry_index {
            if let Some(entry) = journal.entries.get(idx) {
                if journal.is_unlocked(idx) {
                    let mut detail = format!("--- {} ---\n", entry.title);
                    for p in entry.body.iter() {
                        detail.push_str(p);
                        detail.push('\n');
                    }
                    text.0 = detail;
                }
            }
        } else {
            text.0 = "Press 1-9 to view an entry.".to_string();
        }
    }
}
