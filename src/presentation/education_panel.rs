use bevy::prelude::*;

use crate::domain::services::vacuum_physics;
use crate::domain::value_objects::education::{EducationState, JournalDatabase};
use crate::infrastructure::bevy_adapters::craft_components::{CraftComponent, CraftControlState};

#[derive(Component)]
pub struct EducationPanelRoot;

#[derive(Component)]
pub struct EdTelemetryText;

#[derive(Component)]
pub struct EdExplanationText;

#[derive(Component)]
pub struct EdCategoryLabel;

pub fn spawn_education_panel(mut commands: Commands) {
    let bg = Color::srgba(0.02, 0.025, 0.035, 0.82);
    let border = Color::srgba(0.15, 0.2, 0.3, 0.3);
    let bright = Color::srgb(0.75, 0.8, 0.85);
    let dim = Color::srgb(0.4, 0.45, 0.5);
    let accent = Color::srgb(0.3, 0.6, 0.9);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Px(380.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(6.0),
                overflow: Overflow::clip_y(),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(border),
            BorderRadius::all(Val::Px(0.0)),
            EducationPanelRoot,
            Visibility::Hidden,
        ))
        .with_children(|p| {
            // Header
            p.spawn((
                Text::new("EDUCATION MODE"),
                TextFont { font_size: 13.0, ..default() },
                TextColor(accent),
            ));

            p.spawn((
                Text::new("Telemetry"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(dim),
            ));

            // Telemetry values (updated each frame)
            p.spawn((
                Text::new("Lift: 0.0 kN  ZPE: 0.0 kW"),
                TextFont { font_size: 12.0, ..default() },
                TextColor(bright),
                EdTelemetryText,
            ));
            p.spawn((
                Text::new("DC: 0.00  Pulse: 0.00"),
                TextFont { font_size: 12.0, ..default() },
                TextColor(bright),
                EdTelemetryText,
            ));

            // Separator
            p.spawn((
                Text::new("---"),
                TextFont { font_size: 10.0, ..default() },
                TextColor(dim),
            ));

            // Context explanation
            p.spawn((
                Text::new("Spawn the craft to begin."),
                TextFont { font_size: 11.0, ..default() },
                TextColor(bright),
                EdExplanationText,
            ));

            // Separator
            p.spawn((
                Text::new("---"),
                TextFont { font_size: 10.0, ..default() },
                TextColor(dim),
            ));

            // Journal button
            p.spawn((
                Text::new("[ J ] Knowledge Journal"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.5, 0.7, 0.5)),
            ));

            // Compare mode button
            p.spawn((
                Text::new("[ C ] Compare Mode"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.7, 0.7, 0.4)),
            ));

            // Category indicator
            p.spawn((
                Text::new("Category: --"),
                TextFont { font_size: 10.0, ..default() },
                TextColor(dim),
                EdCategoryLabel,
            ));
        });
}

pub fn update_education_panel(
    craft_query: Query<&CraftComponent>,
    control: Res<CraftControlState>,
    state: Res<EducationState>,
    journal: Res<JournalDatabase>,
    mut panel_query: Query<&mut Visibility, With<EducationPanelRoot>>,
    mut telemetry_query: Query<&mut Text, (With<EdTelemetryText>, Without<EdExplanationText>, Without<EdCategoryLabel>)>,
    mut explain_query: Query<&mut Text, (With<EdExplanationText>, Without<EdTelemetryText>, Without<EdCategoryLabel>)>,
    mut cat_query: Query<&mut Text, (With<EdCategoryLabel>, Without<EdTelemetryText>, Without<EdExplanationText>)>,
) {
    let Ok(mut vis) = panel_query.single_mut() else { return };
    *vis = if state.panel_open { Visibility::Visible } else { Visibility::Hidden };
    if !state.panel_open { return; }

    // Update telemetry
    for mut text in telemetry_query.iter_mut() {
        if let Ok(craft) = craft_query.single() {
            let dc = control.dc_current;
            let pulse = control.pulse_current;
            let lift = vacuum_physics::lift_force(dc);
            let zpe = vacuum_physics::zpe_power(pulse, dc);
            let gain_active = vacuum_physics::parametric_gain_active(pulse);
            let gain_str = if gain_active { " PARAMETRIC" } else { "" };
            text.0 = format!("Lift: {:.1} kN  ZPE: {:.1} kW{}", lift, zpe, gain_str);
        } else {
            text.0 = "Lift: 0.0 kN  ZPE: 0.0 kW".to_string();
        }
    }

    // Get the second telemetry text (DC/Pulse line)
    let mut telemetry_iter = telemetry_query.iter_mut();
    if let Some(_first) = telemetry_iter.next() {
        // skip first
    }
    for mut text in telemetry_iter {
        if let Ok(craft) = craft_query.single() {
            text.0 = format!("DC: {:.3}  Pulse: {:.3}", control.dc_current, control.pulse_current);
        } else {
            text.0 = "DC: 0.00  Pulse: 0.00".to_string();
        }
    }

    // Update explanation based on craft state
    for mut text in explain_query.iter_mut() {
        if let Ok(craft) = craft_query.single() {
            let dc = control.dc_current;
            let pulse = control.pulse_current;
            let speed = craft.linear_velocity.length();
            let alt = craft.physics.vertical_position;

            if alt < 1.0 && speed < 0.1 {
                text.0 = "Craft is on the ground. Increase DC field (comma/period) to generate lift. The DC field polarizes the vacuum, creating a low-pressure zone above the craft. Watch the Lift Force increase in telemetry.".to_string();
            } else if speed < 1.0 && alt > 1.0 {
                text.0 = "Craft is hovering. The asymmetric vacuum polarization is pushing the craft upward against the local pressure gradient. Increase Pulse (brackets) to activate ZPE extraction and generate power.".to_string();
            } else if pulse > 0.42 {
                text.0 = "Parametric resonance threshold exceeded! ZPE power output is amplified. The vacuum cavity is oscillating at twice its natural frequency, and each energy pulse extracts more from the vacuum than it consumes.".to_string();
            } else if speed > 50.0 {
                text.0 = "High-speed flight. The segmented hull is steering the craft by reshaping the low-pressure zone. No G-forces are felt due to inertial decoupling — the craft moves within its own spacetime bubble.".to_string();
            } else {
                text.0 = "Craft is in motion. The DC field maintains lift while the segmented hull provides directional control. Adjust DC and Pulse to optimize performance.".to_string();
            }
        } else {
            text.0 = "Spawn the craft (craft mode) to begin exploring vacuum physics. Press B to toggle this panel.".to_string();
        }
    }

    // Update category label
    for mut text in cat_query.iter_mut() {
        text.0 = format!("Category: {}", state.current_category.label());
    }
}
