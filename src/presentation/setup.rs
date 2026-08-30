use bevy::prelude::*;
use super::components::*;

pub(crate) fn setup_ui(mut commands: Commands) {
    commands.spawn((
        Camera2d::default(),
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
                top: Val::Px(20.0),
                width: Val::Px(380.0),
                border: UiRect::all(Val::Px(1.0)),
                padding: UiRect::new(Val::Px(16.0), Val::Px(16.0), Val::Px(14.0), Val::Px(14.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.031, 0.039, 0.063, 0.86)),
            BorderColor::all(Color::srgba(0.196, 0.275, 0.431, 0.47)),
            BorderRadius::all(Val::Px(12.0)),
            InfoCardRoot,
            UiCapture,
            Interaction::default(),
        ))
        .id();

    commands.entity(info_card).with_children(|parent| {
        parent
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|header| {
                header.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.8, 0.85, 0.9)),
                    InfoCardTitle,
                ));
                header
                    .spawn((
                        Button,
                        Node {
                            padding: UiRect::new(
                                Val::Px(6.0), // Reduced padding
                                Val::Px(6.0),
                                Val::Px(4.0),
                                Val::Px(4.0),
                            ),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.031, 0.039, 0.063, 0.78)),
                        BorderColor::all(Color::srgba(0.196, 0.275, 0.431, 0.28)),
                        BorderRadius::all(Val::Px(10.0)),
                        InfoCardToggleButton,
                        UiCapture,
                    ))
                    .with_children(|button| {
                        button.spawn((
                            Text::new("X"),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.74, 0.8, 0.9)),
                        ));
                    });
            });
        parent.spawn((
            Text::new(""),
            TextFont {
                font_size: 9.5,
                ..default()
            },
            TextColor(Color::srgb(0.6, 0.65, 0.7)),
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
                padding: UiRect::new(Val::Px(12.0), Val::Px(12.0), Val::Px(6.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.031, 0.039, 0.063, 0.86)),
            BorderColor::all(Color::srgba(0.196, 0.275, 0.431, 0.47)),
            BorderRadius::all(Val::Px(12.0)),
            InfoCardToggleButton,
            InfoCardExternalToggle,
            UiCapture,
        ))
        .with_children(|button| {
            let (font, color) = text_style(10.5, Color::srgb(0.82, 0.88, 0.98));
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

fn text_style(font_size: f32, color: Color) -> (TextFont, TextColor) {
    (
        TextFont {
            font_size,
            ..default()
        },
        TextColor(color),
    )
}

fn spawn_menu_button(parent: &mut ChildSpawnerCommands, label: &str, action: MenuAction, primary: bool) {
    let padding = if primary {
        UiRect::new(Val::Px(10.0), Val::Px(10.0), Val::Px(5.0), Val::Px(5.0))
    } else {
        UiRect::new(Val::Px(8.0), Val::Px(8.0), Val::Px(4.0), Val::Px(4.0))
    };
    let radius = if primary { 16.0 } else { 12.0 };

    parent
        .spawn((
            Button,
            Node {
                border: UiRect::all(Val::Px(1.0)),
                padding,
                ..default()
            },
            BackgroundColor(Color::srgba(0.031, 0.039, 0.063, 0.78)),
            BorderColor::all(Color::srgba(0.196, 0.275, 0.431, 0.28)),
            BorderRadius::all(Val::Px(radius)),
            MenuButton { action, primary },
            UiCapture,
        ))
        .with_children(|button| {
            let (font, color) = text_style(9.5, Color::srgb(0.75, 0.8, 0.85));
            button.spawn((Text::new(label), font, color));
        });
}

fn spawn_nav_button(parent: &mut ChildSpawnerCommands, name: &str, _group: NavGroup) {
    parent
        .spawn((
            Button,
            Node {
                border: UiRect::all(Val::Px(0.5)),
                padding: UiRect::new(Val::Px(6.0), Val::Px(6.0), Val::Px(2.0), Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(nav_button_color(false)),
            BorderColor::all(nav_button_border_color(false)),
            BorderRadius::all(Val::Px(3.0)),
        NavButton { name: name.to_string() },
            UiCapture,
        ))
        .with_children(|button| {
            let (font, color) = text_style(9.0, Color::srgb(0.75, 0.8, 0.85));
            button.spawn((Text::new(name), font, color, NavButtonLabel));
        });
}

fn info_body_text(text: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(text),
        TextFont {
            font_size: 9.5,
            ..default()
        },
        TextColor(Color::srgb(0.82, 0.85, 0.88)),
    )
}

fn nav_button_color(selected: bool) -> Color {
    if selected {
        Color::srgb(0.16, 0.24, 0.39)
    } else {
        Color::srgba(0.031, 0.039, 0.063, 0.78)
    }
}

fn nav_button_border_color(selected: bool) -> Color {
    if selected {
        Color::srgb(0.31, 0.47, 0.78)
    } else {
        Color::srgba(0.196, 0.275, 0.431, 0.28)
    }
}

fn nav_button_color_hover(selected: bool, hovered: bool) -> Color {
    if selected {
        Color::srgb(0.16, 0.24, 0.39) // Selected color unchanged
    } else if hovered {
        Color::srgba(0.051, 0.059, 0.083, 0.85) // Slightly brighter on hover
    } else {
        Color::srgba(0.031, 0.039, 0.063, 0.78) // Normal color
    }
}

fn nav_button_border_color_hover(selected: bool, hovered: bool) -> Color {
    if selected {
        Color::srgb(0.31, 0.47, 0.78) // Selected border unchanged
    } else if hovered {
        Color::srgba(0.216, 0.295, 0.451, 0.35) // Slightly brighter border on hover
    } else {
        Color::srgba(0.196, 0.275, 0.431, 0.28) // Normal border
    }
}

fn menu_button_colors(primary: bool, active: bool) -> (Color, Color) {
    if active {
        if primary {
            (
                Color::srgb(0.24, 0.35, 0.55),
                Color::srgb(0.35, 0.5, 0.78),
            )
        } else {
            (
                Color::srgb(0.18, 0.26, 0.41),
                Color::srgb(0.27, 0.39, 0.59),
            )
        }
    } else if primary {
        (
            Color::srgba(0.031, 0.039, 0.063, 0.78),
            Color::srgba(0.196, 0.275, 0.431, 0.28),
        )
    } else {
        (
            Color::srgba(0.024, 0.031, 0.05, 0.78),
            Color::srgba(0.157, 0.22, 0.345, 0.28),
        )
    }
}

fn fps_color(fps: f32) -> Color {
    if fps >= 58.0 {
        Color::srgb(0.2, 0.8, 0.2) // Green for good performance
    } else if fps >= 30.0 {
        Color::srgb(0.8, 0.8, 0.2) // Yellow for acceptable performance
    } else {
        Color::srgb(0.8, 0.2, 0.2) // Red for poor performance
    }
}

fn notification_color(notification_type: &crate::infrastructure::bevy_adapters::components::NotificationType) -> Color {
    match notification_type {
        crate::infrastructure::bevy_adapters::components::NotificationType::Info => Color::srgb(0.4, 0.6, 0.8),
        crate::infrastructure::bevy_adapters::components::NotificationType::Success => Color::srgb(0.4, 0.8, 0.4),
        crate::infrastructure::bevy_adapters::components::NotificationType::Warning => Color::srgb(0.8, 0.8, 0.4),
        crate::infrastructure::bevy_adapters::components::NotificationType::Error => Color::srgb(0.8, 0.4, 0.4),
    }
}

fn planet_names() -> [&'static str; 9] {
    [
        "Sun", "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
    ]
}

fn moon_names_for_parent(parent: &str) -> &'static [&'static str] {
    match parent {
        "Earth" => &["Moon"],
        "Mars" => &["Phobos", "Deimos"],
        "Jupiter" => &["Io", "Europa", "Ganymede", "Callisto"],
        "Saturn" => &[
            "Mimas",
            "Enceladus",
            "Tethys",
            "Dione",
            "Rhea",
            "Titan",
            "Hyperion",
            "Iapetus",
        ],
        "Uranus" => &["Miranda", "Ariel", "Umbriel", "Titania", "Oberon"],
        "Neptune" => &["Triton", "Proteus", "Nereid", "Larissa"],
        _ => &[],
    }
}
