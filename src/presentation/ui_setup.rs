use bevy::prelude::*;

use crate::presentation::ui_components::*;
use crate::presentation::ui_helpers::*;

pub fn setup_ui(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 10,
            clear_color: ClearColorConfig::None,
            ..default()
        },
    ));

    commands.insert_resource(UiMenuState::default());
    commands.insert_resource(
        crate::infrastructure::bevy_adapters::ui_components::VideoRecordingState::default(),
    );

    let navbar = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            bottom: Val::Px(10.0),
            width: Val::Percent(100.0),
            height: Val::Auto,
            ..default()
        })
        .id();

    commands.entity(navbar).with_children(|parent| {
        parent
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(32.0), // More compact height
                    padding: UiRect::new(
                        Val::Px(8.0), // Reduced padding
                        Val::Px(8.0),
                        Val::Px(2.0),
                        Val::Px(2.0),
                    ),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                UiCapture,
            ))
            .with_children(|bar| {
                bar.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0), // More compact spacing
                    ..default()
                })
                .with_children(|menu| {
                    spawn_menu_button(menu, "Explore", MenuAction::Explore, true);
                    spawn_menu_button(menu, "Orbits", MenuAction::Orbits, false);
                });
            });

        parent.spawn((
            Text::new("fps 0"),
            TextFont {
                font_size: 8.5,
                ..default()
            },
            TextColor(Color::srgb(0.6, 0.65, 0.7)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(20.0),
                bottom: Val::Px(12.0),
                ..default()
            },
            FpsText,
        ));
    });

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
                min_height: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
            ZIndex(10),
            SelectorPanelRoot,
            UiCapture,
        ))
        .with_children(|container| {
            container
                .spawn((
                    Node {
                        width: Val::Px(500.0), // More compact width
                        padding: UiRect::new(
                            Val::Px(12.0), // Reduced padding
                            Val::Px(12.0),
                            Val::Px(10.0),
                            Val::Px(10.0),
                        ),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(6.0), // Tighter spacing
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.02, 0.025, 0.035, 0.75)), // More minimal background
                    BorderColor::all(Color::srgba(0.15, 0.2, 0.3, 0.3)),     // Subtler border
                    BorderRadius::all(Val::Px(8.0)), // Smaller border radius
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Select Body"),
                        TextFont {
                            font_size: 10.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.75, 0.8, 0.85)),
                    ));

                    panel.spawn((
                        Text::new("All Bodies"),
                        TextFont {
                            font_size: 10.5,
                            ..default()
                        },
                        TextColor(Color::srgb(0.51, 0.59, 0.71)),
                    ));

                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            column_gap: Val::Px(4.0), // Tighter horizontal spacing
                            row_gap: Val::Px(4.0),    // Tighter vertical spacing
                            max_height: Val::Px(250.0), // More compact height
                            overflow: Overflow::clip_y(),
                            ..default()
                        })
                        .with_children(|bodies| {
                            // Planets first
                            for name in planet_names() {
                                spawn_nav_button(bodies, name, NavGroup::CelestialBody);
                            }
                            // Then moons
                            for parent_name in planet_names() {
                                for moon_name in moon_names_for_parent(parent_name) {
                                    spawn_nav_button(bodies, moon_name, NavGroup::CelestialBody);
                                }
                            }
                        });
                });
        });

    let info_card = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(20.0),
                top: Val::Px(56.0), // Below the Info toggle button
                width: Val::Px(340.0), // Slightly narrower
                border: UiRect::all(Val::Px(1.0)),
                padding: UiRect::new(Val::Px(14.0), Val::Px(14.0), Val::Px(12.0), Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.031, 0.039, 0.063, 0.88)),
            BorderColor::all(Color::srgba(0.196, 0.275, 0.431, 0.35)),
            BorderRadius::all(Val::Px(10.0)),
            InfoCardRoot,
            UiCapture,
            Interaction::default(),
        ))
        .id();

    commands.entity(info_card).with_children(|parent| {
        parent.spawn((
            Text::new(""),
            TextFont {
                font_size: 13.0,
                ..default()
            },
            TextColor(Color::srgb(0.82, 0.87, 0.92)),
            InfoCardTitle,
        ));
        parent.spawn((
            Text::new(""),
            TextFont {
                font_size: 9.0,
                ..default()
            },
            TextColor(Color::srgb(0.55, 0.62, 0.72)),
            InfoCardSubtitle,
        ));
        let (body_text, body_font, body_color) = info_body_text("");
        parent.spawn((
            body_text,
            body_font,
            body_color,
            Node {
                width: Val::Percent(100.0),
                ..default()
            },
            InfoCardBody,
        ));
    });

    commands
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(20.0),
                top: Val::Px(20.0),
                padding: UiRect::new(Val::Px(10.0), Val::Px(10.0), Val::Px(5.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.031, 0.039, 0.063, 0.78)),
            BorderColor::all(Color::srgba(0.196, 0.275, 0.431, 0.35)),
            BorderRadius::all(Val::Px(8.0)),
            InfoCardToggleButton,
            InfoCardExternalToggle,
            UiCapture,
        ))
        .with_children(|button| {
            let (font, color) = text_style(10.0, Color::srgb(0.78, 0.84, 0.94));
            button.spawn((Text::new("Info"), font, color));
        });

    let notifications = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(80.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(8.0),
                ..default()
            },
            ZIndex(5),
            NotificationLayer,
        ))
        .id();

    commands.insert_resource(UiRoots {
        _navbar: navbar,
        _info_card: info_card,
        notifications,
    });
}