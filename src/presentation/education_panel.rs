use bevy::prelude::*;

use crate::domain::services::vacuum_physics;
use crate::domain::value_objects::education::{EducationState, JournalCategory, JournalDatabase};
use crate::infrastructure::bevy_adapters::craft_components::{CraftComponent, CraftControlState};
use crate::presentation::markdown_renderer::{parse_markdown, spawn_markdown_blocks};

#[derive(Component)]
pub struct EducationPanelRoot;

#[derive(Component)]
pub struct EducationCard;

#[derive(Component)]
pub struct EducationTelemetryText;

#[derive(Component)]
pub struct EducationContextContainer;

#[derive(Component)]
pub struct EducationContextWrapper;

#[derive(Component)]
pub struct EducationJournalContainer;

#[derive(Component)]
pub struct EducationJournalListText;

#[derive(Component)]
pub struct EducationDetailWrapper;

pub fn spawn_education_panel(mut commands: Commands) {
    let accent = Color::srgb(0.3, 0.6, 0.9);
    let bright = Color::srgb(0.75, 0.8, 0.85);
    let dim = Color::srgb(0.4, 0.45, 0.5);
    let card_bg = Color::srgba(0.015, 0.02, 0.03, 0.94);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            EducationPanelRoot,
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(820.0),
                    max_height: Val::Percent(90.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(28.0)),
                    row_gap: Val::Px(8.0),
                    ..default()
                },
                BackgroundColor(card_bg),
                BorderColor::all(Color::srgba(0.3, 0.6, 0.9, 0.15)),
                BorderRadius::all(Val::Px(12.0)),
                EducationCard,
            ))
            .with_children(|card| {
                card.spawn((
                    Text::new("EDUCATION MODE"),
                    TextFont {
                        font_size: 18.0,
                        ..default()
                    },
                    TextColor(accent),
                ));
                card.spawn((
                    Text::new("Press B to close. Press J to toggle journal."),
                    TextFont {
                        font_size: 10.0,
                        ..default()
                    },
                    TextColor(dim),
                ));

                card.spawn((
                    Text::new("Lift: 0.0 kN | ZPE: 0.0 kW | DC: 0.00 | Pulse: 0.00"),
                    TextFont {
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(bright),
                    EducationTelemetryText,
                ));

                card.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(1.0),
                        margin: UiRect::vertical(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.3, 0.3, 0.4, 0.2)),
                ));

                card.spawn((
                    Text::new("Context"),
                    TextFont {
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(accent),
                ));

                let _context_wrapper = card
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            width: Val::Percent(100.0),
                            row_gap: Val::Px(4.0),
                            ..default()
                        },
                        EducationContextContainer,
                    ))
                    .with_children(|c| {
                        c.spawn((Node::default(), EducationContextWrapper));
                    })
                    .id();

                card.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(1.0),
                        margin: UiRect::vertical(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.3, 0.3, 0.4, 0.2)),
                ));

                card.spawn((
                    Text::new("Knowledge Journal  [J]"),
                    TextFont {
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(accent),
                ));

                card.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 11.0,
                        ..default()
                    },
                    TextColor(bright),
                    EducationJournalListText,
                ));

                card.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        width: Val::Percent(100.0),
                        row_gap: Val::Px(4.0),
                        ..default()
                    },
                    EducationJournalContainer,
                ))
                .with_children(|c| {
                    c.spawn((Node::default(), EducationDetailWrapper));
                });
            });
        });
}

fn build_context_markdown(craft: Option<&CraftComponent>, control: &CraftControlState) -> String {
    let Some(craft) = craft else {
        return "Spawn the craft (craft mode) to begin exploring vacuum physics.\n\nPress **B** to toggle this panel.".to_string();
    };

    let _dc = control.dc_current;
    let pulse = control.pulse_current;
    let speed = craft.linear_velocity.length();
    let alt = craft.physics.vertical_position;

    if alt < 1.0 && speed < 0.1 {
        "Craft is on the ground. Increase **DC field** (comma/period) to generate lift. The DC field polarizes the vacuum, creating a low-pressure zone above the craft.\n\n> Watch the Lift Force increase in telemetry.".to_string()
    } else if speed < 1.0 && alt > 1.0 {
        "Craft is hovering. The asymmetric vacuum polarization is pushing the craft upward against the local pressure gradient. Increase **Pulse** (brackets) to activate ZPE extraction.\n\n> The cavity is forming. Energy is beginning to flow from the vacuum.".to_string()
    } else if pulse > 0.42 {
        "**Parametric resonance threshold exceeded!**\n\nZPE power output is amplified. The vacuum cavity is oscillating at twice its natural frequency, and each energy pulse extracts more from the vacuum than it consumes.\n\nFormula: `P_zpe = 210 × pulse^1.8 × (1 + 2.6 × max(0, pulse - 0.42))`".to_string()
    } else if speed > 50.0 {
        "**High-speed flight.**\n\nThe segmented hull is steering the craft by reshaping the low-pressure zone. No G-forces are felt due to inertial decoupling — the craft moves within its own spacetime bubble.\n\n> *\"And it is He who created the night and the day and the sun and the moon; all [heavenly bodies] swim in an orbit.\"* — Quran 21:33".to_string()
    } else {
        "Craft is in motion. The **DC field** maintains lift while the segmented hull provides directional control.\n\nAdjust DC (comma/period) and Pulse (brackets) to optimize performance.".to_string()
    }
}

fn build_entry_markdown(journal: &JournalDatabase, index: usize) -> String {
    let Some(entry) = journal.entries.get(index) else {
        return String::new();
    };
    if !journal.is_unlocked(index) {
        return "*This entry is locked. Continue exploring to unlock it.*".to_string();
    }

    let mut md = format!("## {}\n\n", entry.title);
    for p in entry.body.iter() {
        md.push_str(p);
        md.push_str("\n\n");
    }

    if let Some(f) = entry.formula {
        md.push_str("---\n\n");
        md.push_str(&format!("**Formula:** `{}`\n\n", f));
    }

    for qref in entry.quranic_refs.iter() {
        md.push_str("---\n\n");
        md.push_str(&format!(
            "> **Surah {}:{}**\n> \n> {}\n> \n> *{}*\n> \n> {}\n\n",
            qref.sura, qref.verse, qref.arabic, qref.translation, qref.explanation
        ));
    }

    md
}

fn replace_wrapper(
    container: Entity,
    marker: impl Component,
    blocks: &[crate::presentation::markdown_renderer::MdBlock],
    commands: &mut Commands,
    children_query: &Query<&Children>,
) {
    if let Ok(children) = children_query.get(container) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    let wrapper = commands.spawn((Node::default(), marker)).id();
    commands.entity(container).add_child(wrapper);
    spawn_markdown_blocks(wrapper, blocks, commands);
}

#[expect(
    clippy::too_many_arguments,
    reason = "This Bevy UI system receives independent resources and presentation queries."
)]
#[expect(
    clippy::type_complexity,
    reason = "The ParamSet keeps mutually exclusive education text queries borrow-safe."
)]
pub fn update_education_panel(
    craft_query: Query<&CraftComponent>,
    control: Res<CraftControlState>,
    state: Res<EducationState>,
    journal: Res<JournalDatabase>,
    mut panel_query: Query<&mut Visibility, With<EducationPanelRoot>>,
    context_container: Query<Entity, With<EducationContextContainer>>,
    detail_container: Query<
        Entity,
        (
            With<EducationJournalContainer>,
            Without<EducationContextContainer>,
        ),
    >,
    children_query: Query<&Children>,
    mut text_queries: ParamSet<(
        Query<&mut Text, With<EducationTelemetryText>>,
        Query<&mut Text, With<EducationJournalListText>>,
    )>,
    mut commands: Commands,
    mut last_context: Local<String>,
    mut last_detail: Local<Option<usize>>,
) {
    let Ok(mut vis) = panel_query.single_mut() else {
        return;
    };
    *vis = if state.panel_open {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if !state.panel_open {
        return;
    }

    if let Ok(mut text) = text_queries.p0().single_mut() {
        if let Ok(_craft) = craft_query.single() {
            let lift = vacuum_physics::lift_force(control.dc_current);
            let zpe = vacuum_physics::zpe_power(control.pulse_current, control.dc_current);
            let gain = vacuum_physics::parametric_gain_active(control.pulse_current);
            let gs = if gain { " PARAMETRIC" } else { "" };
            text.0 = format!(
                "Lift: {:.1} kN | ZPE: {:.1} kW{} | DC: {:.3} | Pulse: {:.3}",
                lift, zpe, gs, control.dc_current, control.pulse_current
            );
        } else {
            text.0 = "Lift: 0.0 kN | ZPE: 0.0 kW | DC: 0.00 | Pulse: 0.00".to_string();
        }
    }

    let context_md = build_context_markdown(craft_query.single().ok(), &control);
    if *last_context != context_md {
        *last_context = context_md.clone();
        if let Ok(entity) = context_container.single() {
            let blocks = parse_markdown(&context_md);
            replace_wrapper(
                entity,
                EducationContextWrapper,
                &blocks,
                &mut commands,
                &children_query,
            );
        }
    }

    let show_journal = state.journal_section_open;
    if let Ok(mut text) = text_queries.p1().single_mut() {
        if show_journal {
            let categories = [
                JournalCategory::VacuumSuperfluid,
                JournalCategory::AsymmetricPolarization,
                JournalCategory::ZpeExtraction,
                JournalCategory::MetricEngineering,
                JournalCategory::QuranicEvidence,
            ];
            let mut lines: Vec<String> = Vec::new();
            let mut entry_num = 0;
            for cat in &categories {
                let cat_lines: Vec<String> = journal
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.category == *cat)
                    .map(|(i, e)| {
                        entry_num += 1;
                        if journal.is_unlocked(i) {
                            format!("  {}: {}", entry_num, e.title)
                        } else {
                            format!("  {}: [LOCKED] {}", entry_num, e.title)
                        }
                    })
                    .collect();
                if !cat_lines.is_empty() {
                    lines.push(cat.label().to_string());
                    lines.extend(cat_lines);
                    lines.push(String::new());
                }
            }
            text.0 = lines.join("\n");
        } else {
            text.0 = "Press J to expand journal. Select entries 1-9 to view.".to_string();
        }
    }

    let detail_index = if show_journal {
        state.current_entry_index
    } else {
        None
    };
    if *last_detail != detail_index {
        *last_detail = detail_index;
        if let Ok(entity) = detail_container.single() {
            if let Some(idx) = detail_index {
                if journal.is_unlocked(idx) {
                    let md = build_entry_markdown(&journal, idx);
                    let blocks = parse_markdown(&md);
                    replace_wrapper(
                        entity,
                        EducationDetailWrapper,
                        &blocks,
                        &mut commands,
                        &children_query,
                    );
                }
            }
        }
    }
}
