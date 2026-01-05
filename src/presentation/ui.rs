#![allow(dead_code)]

use bevy::prelude::*;
use bevy::text::BreakLineOn;

use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::{
    NotificationQueue, NotificationType, PerformanceStats, PlanetComponent, ScreenshotState,
    Selectable, SelectedPlanet, UiPointerState, ZenMode,
};

#[derive(Component)]
pub(crate) struct UiCapture;

#[derive(Resource)]
pub(crate) struct UiRoots {
    _navbar: Entity,
    _info_card: Entity,
    notifications: Entity,
}

#[derive(Component)]
pub(crate) struct NavButton {
    name: String,
    group: NavGroup,
}

#[derive(Clone, Copy)]
#[derive(PartialEq)]
enum NavGroup {
    CelestialBody, // Unified for all planets and moons
}

#[derive(Component)]
pub(crate) struct NavButtonLabel;

#[derive(Component)]
pub(crate) struct FpsText;

#[derive(Component)]
pub(crate) struct MenuButton {
    action: MenuAction,
    primary: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    Explore,
    Orbits,
}

#[derive(Component)]
pub(crate) struct SelectorPanelRoot;



#[derive(Resource)]
pub(crate) struct UiMenuState {
    selector_open: bool,
    info_card_open: bool,
}

impl Default for UiMenuState {
    fn default() -> Self {
        Self {
            selector_open: false,
            info_card_open: true,
        }
    }
}







#[derive(Component)]
pub(crate) struct InfoCardRoot;



#[derive(Component)]
pub(crate) struct InfoCardTitle;

#[derive(Component)]
pub(crate) struct InfoCardSubtitle;

#[derive(Component)]
pub(crate) struct InfoCardBody;

#[derive(Component)]
pub(crate) struct InfoCardToggleButton;

#[derive(Component)]
pub(crate) struct InfoCardExternalToggle;

#[derive(Component)]
pub(crate) struct NotificationLayer;

pub(crate) fn setup_ui(mut commands: Commands) {
    commands.spawn(Camera2dBundle {
        camera: Camera {
            order: 10,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        ..default()
    });

    commands.insert_resource(UiMenuState::default());

    let navbar = commands
        .spawn(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                bottom: Val::Px(10.0),
                width: Val::Percent(100.0),
                height: Val::Auto,
                ..default()
            },
            ..default()
        })
        .id();

    commands.entity(navbar).with_children(|parent| {
        parent
            .spawn((
                NodeBundle {
                    style: Style {
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
                    ..default()
                },
                UiCapture,
            ))
            .with_children(|bar| {
                bar.spawn(NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(8.0), // More compact spacing
                        ..default()
                    },
                    ..default()
                })
                .with_children(|menu| {
                    spawn_menu_button(menu, "Explore", MenuAction::Explore, true);
                    spawn_menu_button(menu, "Orbits", MenuAction::Orbits, false);
                });
            });

        parent.spawn((
            TextBundle {
                text: Text::from_section(
                    "fps 0",
                    text_style(8.5, Color::srgb(0.6, 0.65, 0.7)), // Smaller, more subtle FPS display
                ),
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Px(20.0),
                    bottom: Val::Px(12.0),
                    ..default()
                },
                ..default()
            },
            FpsText,
        ));

    });

    commands
        .spawn((
            NodeBundle {
                style: Style {
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
                background_color: BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
                z_index: ZIndex::Global(10),
                ..default()
            },
            SelectorPanelRoot,
            UiCapture,
        ))
        .with_children(|container| {
            container
                .spawn(NodeBundle {
                    style: Style {
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
                    background_color: BackgroundColor(Color::srgba(0.02, 0.025, 0.035, 0.75)), // More minimal background
                    border_color: BorderColor(Color::srgba(0.15, 0.2, 0.3, 0.3)), // Subtler border
                    border_radius: BorderRadius::all(Val::Px(8.0)), // Smaller border radius
                    ..default()
                })
                .with_children(|panel| {
                    panel.spawn(TextBundle::from_section(
                        "Select Body",
                        text_style(10.0, Color::srgb(0.75, 0.8, 0.85)),
                    ));



                    panel.spawn(TextBundle::from_section(
                        "All Bodies",
                        text_style(10.5, Color::srgb(0.51, 0.59, 0.71)),
                    ));

                    panel
                        .spawn(NodeBundle {
                            style: Style {
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                column_gap: Val::Px(4.0), // Tighter horizontal spacing
                                row_gap: Val::Px(4.0), // Tighter vertical spacing
                                max_height: Val::Px(250.0), // More compact height
                                overflow: Overflow::clip_y(),
                                ..default()
                            },
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
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    right: Val::Px(20.0),
                    top: Val::Px(20.0),
                    width: Val::Px(380.0),
                    border: UiRect::all(Val::Px(1.0)),
                    padding: UiRect::new(
                        Val::Px(16.0),
                        Val::Px(16.0),
                        Val::Px(14.0),
                        Val::Px(14.0),
                    ),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    display: Display::None,
                    ..default()
                },
                background_color: BackgroundColor(Color::srgba(0.031, 0.039, 0.063, 0.86)),
                border_color: BorderColor(Color::srgba(0.196, 0.275, 0.431, 0.47)),
                border_radius: BorderRadius::all(Val::Px(12.0)),
                ..default()
            },
            InfoCardRoot,
            UiCapture,
            Interaction::default(),
        ))
        .id();

    commands.entity(info_card).with_children(|parent| {
        parent
            .spawn(NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                ..default()
            })
            .with_children(|header| {
                header.spawn((
                    TextBundle::from_section(
                        "",
                        text_style(14.0, Color::srgb(0.8, 0.85, 0.9)), // Smaller, more subtle title
                    ),
                    InfoCardTitle,
                ));
                header
                    .spawn((
                        ButtonBundle {
                            style: Style {
                                padding: UiRect::new(
                                    Val::Px(6.0), // Reduced padding
                                    Val::Px(6.0),
                                    Val::Px(4.0),
                                    Val::Px(4.0),
                                ),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            background_color: BackgroundColor(Color::srgba(0.031, 0.039, 0.063, 0.78)),
                            border_color: BorderColor(Color::srgba(0.196, 0.275, 0.431, 0.28)),
                            border_radius: BorderRadius::all(Val::Px(10.0)),
                            ..default()
                        },
                        InfoCardToggleButton,
                        UiCapture,
                    ))
                    .with_children(|button| {
        button.spawn(TextBundle::from_section(
            "X",
            text_style(12.0, Color::srgb(0.74, 0.8, 0.9)),
        ));
                    });
            });
        parent.spawn((
            TextBundle::from_section("", text_style(9.5, Color::srgb(0.6, 0.65, 0.7))), // Smaller, more subtle
            InfoCardSubtitle,
        ));
        parent.spawn((
            TextBundle {
                text: info_body_text(""),
                style: Style {
                    width: Val::Percent(100.0),
                    ..default()
                },
                ..default()
            },
            InfoCardBody,
        ));
    });

    commands.spawn((
        ButtonBundle {
            style: Style {
                position_type: PositionType::Absolute,
                right: Val::Px(20.0),
                top: Val::Px(20.0),
                padding: UiRect::new(Val::Px(12.0), Val::Px(12.0), Val::Px(6.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                display: Display::None,
                ..default()
            },
            background_color: BackgroundColor(Color::srgba(0.031, 0.039, 0.063, 0.86)),
            border_color: BorderColor(Color::srgba(0.196, 0.275, 0.431, 0.47)),
            border_radius: BorderRadius::all(Val::Px(12.0)),
            ..default()
        },
        InfoCardToggleButton,
        InfoCardExternalToggle,
        UiCapture,
    ))
    .with_children(|button| {
        button.spawn(TextBundle::from_section(
            "Info",
            text_style(10.5, Color::srgb(0.82, 0.88, 0.98)),
        ));
    });

    let notifications = commands
        .spawn((
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(80.0),
                    left: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(8.0),
                    ..default()
                },
                z_index: ZIndex::Local(5),
                ..default()
            },
            NotificationLayer,
        ))
        .id();

    commands.insert_resource(UiRoots {
        _navbar: navbar,
        _info_card: info_card,
        notifications,
    });
}

pub(crate) fn handle_nav_interactions(
    interactions: Query<(&Interaction, &NavButton), Changed<Interaction>>,
    menu_interactions: Query<(&Interaction, &MenuButton), Changed<Interaction>>,
    info_card_interactions: Query<&Interaction, (Changed<Interaction>, With<InfoCardToggleButton>)>,
    mut selected_planet: ResMut<SelectedPlanet>,
    mut selectable_query: Query<(Entity, &mut Selectable)>,
    mut solar_params: ResMut<SolarSystemParameters>,
    mut menu_state: ResMut<UiMenuState>,
) {
    for (interaction, button) in menu_interactions.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button.action {
            MenuAction::Explore => {
                menu_state.selector_open = !menu_state.selector_open;
            }
            MenuAction::Orbits => {
                solar_params.show_orbits = !solar_params.show_orbits;
            }
        }
    }

    for interaction in info_card_interactions.iter() {
        if *interaction == Interaction::Pressed {
            menu_state.info_card_open = !menu_state.info_card_open;
        }
    }

    for (interaction, button) in interactions.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let mut target_entity = None;
        for (entity, selectable) in selectable_query.iter_mut() {
            if selectable.name == button.name {
                target_entity = Some(entity);
                break;
            }
        }

        // Only update selection if it's different (idempotent)
        if selected_planet.entity != target_entity {
            selected_planet.entity = target_entity;
            selected_planet.name = target_entity.map(|_| button.name.clone());
            menu_state.selector_open = false;
            if target_entity.is_some() {
                menu_state.info_card_open = true;
            }

            for (entity, mut selectable) in selectable_query.iter_mut() {
                selectable.selected = Some(entity) == target_entity;
            }
        }
    }
}


pub(crate) fn update_navbar(
    selected_planet: Res<SelectedPlanet>,
    solar_params: Res<SolarSystemParameters>,
    performance_stats: Res<PerformanceStats>,
    screenshot_state: Res<ScreenshotState>,
    notifications: Res<NotificationQueue>,
    zen_mode: Res<ZenMode>,
    menu_state: Res<UiMenuState>,
    mut queries: ParamSet<(
        Query<(&NavButton, &mut Style, &mut BackgroundColor, &mut BorderColor)>,
        Query<(&MenuButton, &mut Style, &mut BackgroundColor, &mut BorderColor)>,
        Query<&mut Text, With<FpsText>>,
        Query<&mut Style, With<SelectorPanelRoot>>,
    )>,
) {
    let hide_ui = zen_mode.enabled || screenshot_state.pending || notifications.hide_for_screenshot;

    let selected_name = selected_planet.name.as_deref();
    let active_parent = selected_name.map(|name| {
        if is_primary_body(name) {
            name
        } else {
            get_parent_body(name)
        }
    });

    let show_selector = menu_state.selector_open;

    // No longer need moon visibility logic - all bodies shown in unified list

    if let Ok(mut style) = queries.p3().get_single_mut() {
        style.display = if show_selector && !hide_ui {
            Display::Flex
        } else {
            Display::None
        };
    }



    for (button, mut style, mut background, mut border) in queries.p0().iter_mut() {
        let is_selected = selected_name == Some(button.name.as_str());
        if hide_ui {
            style.display = Display::None;
            continue;
        }
        // Unified celestial body list - show all when selector is open
        let show_button = show_selector;

        style.display = if show_button {
            Display::Flex
        } else {
            Display::None
        };

        *background = BackgroundColor(nav_button_color(is_selected));
        border.0 = nav_button_border_color(is_selected);
    }

    for (button, mut style, mut background, mut border) in queries.p1().iter_mut() {
        let active = match button.action {
            MenuAction::Orbits => solar_params.show_orbits,
            MenuAction::Explore => menu_state.selector_open,
        };
        if hide_ui {
            style.display = Display::None;
            *background = BackgroundColor(Color::NONE);
            border.0 = Color::NONE;
            continue;
        }
        style.display = Display::Flex;
        let (bg, stroke) = menu_button_colors(button.primary, active);
        *background = BackgroundColor(bg);
        border.0 = stroke;
    }



    if let Ok(mut text) = queries.p2().get_single_mut() {
        let display_fps = performance_stats.average_fps;
        if hide_ui {
            text.sections[0].value.clear();
        } else {
            text.sections[0].value = format!("fps {:.0}", display_fps);
            text.sections[0].style.color = fps_color(display_fps);
        }
    }
}


pub(crate) fn update_info_card(
    selected_planet: Res<SelectedPlanet>,
    planet_query: Query<&PlanetComponent>,
    menu_state: Res<UiMenuState>,
    zen_mode: Res<ZenMode>,
    mut root_query: Query<&mut Style, With<InfoCardRoot>>,
    mut text_queries: ParamSet<(
        Query<&mut Text, With<InfoCardTitle>>,
        Query<&mut Text, With<InfoCardSubtitle>>,
        Query<&mut Text, With<InfoCardBody>>,
    )>,
) {
    let Ok(mut root_style) = root_query.get_single_mut() else {
        return;
    };

    if zen_mode.enabled || !menu_state.info_card_open {
        root_style.display = Display::None;
        return;
    };

    let Some(entity) = selected_planet.entity else {
        root_style.display = Display::None;
        return;
    };

    let Ok(planet) = planet_query.get(entity) else {
        root_style.display = Display::None;
        return;
    };

    root_style.display = Display::Flex;

    if let Ok(mut title) = text_queries.p0().get_single_mut() {
        title.sections[0].value = planet.domain_planet.name.clone();
    }

    if let Ok(mut subtitle) = text_queries.p1().get_single_mut() {
        subtitle.sections[0].value = get_celestial_type(&planet.domain_planet.name).to_string();
    }

    if let Ok(mut body) = text_queries.p2().get_single_mut() {
        body.sections[0].value = build_info_body(&planet.domain_planet);
    }
}

pub(crate) fn update_notifications_ui(
    mut commands: Commands,
    time: Res<Time>,
    mut notifications: ResMut<NotificationQueue>,
    roots: Res<UiRoots>,
    zen_mode: Res<ZenMode>,
) {
    if zen_mode.enabled {
        commands.entity(roots.notifications).despawn_descendants();
        return;
    }
    if notifications.hide_for_screenshot {
        notifications.hide_for_screenshot = false;
        commands.entity(roots.notifications).despawn_descendants();
        return;
    }

    let current_time = time.elapsed_seconds();
    notifications
        .notifications
        .retain(|n| current_time - n.created_at < n.duration);

    commands.entity(roots.notifications).despawn_descendants();

    commands.entity(roots.notifications).with_children(|parent| {
        for notification in notifications.notifications.iter() {
            parent
                .spawn((
                    NodeBundle {
                        style: Style {
                            border: UiRect::all(Val::Px(1.0)),
                            padding: UiRect::all(Val::Px(10.0)),
                            ..default()
                        },
                        background_color: BackgroundColor(
                            notification_color(&notification.notification_type),
                        ),
                        border_color: BorderColor(Color::srgba(1.0, 1.0, 1.0, 0.35)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    UiCapture,
                    Interaction::default(),
                ))
                .with_children(|row| {
                    row.spawn(TextBundle::from_section(
                        notification.message.clone(),
                        text_style(12.0, Color::srgb(0.95, 0.95, 0.98)),
                    ));
                });
        }
    });
}

pub(crate) fn update_ui_hover_state(
    mut ui_state: ResMut<UiPointerState>,
    query: Query<&Interaction, With<UiCapture>>,
) {
    ui_state.is_over_ui = query
        .iter()
        .any(|interaction| matches!(interaction, Interaction::Hovered | Interaction::Pressed));
}

fn text_style(font_size: f32, color: Color) -> TextStyle {
    TextStyle {
        font_size,
        color,
        ..default()
    }
}

fn spawn_menu_button(
    parent: &mut ChildBuilder,
    label: &str,
    action: MenuAction,
    primary: bool,
) {
    let padding = if primary {
        UiRect::new(Val::Px(10.0), Val::Px(10.0), Val::Px(5.0), Val::Px(5.0)) // Reduced padding
    } else {
        UiRect::new(Val::Px(8.0), Val::Px(8.0), Val::Px(4.0), Val::Px(4.0)) // Reduced padding
    };
    let radius = if primary { 16.0 } else { 12.0 }; // Smaller border radius

    parent
        .spawn((
            ButtonBundle {
                style: Style {
                    border: UiRect::all(Val::Px(1.0)),
                    padding,
                    ..default()
                },
                background_color: BackgroundColor(Color::srgba(0.031, 0.039, 0.063, 0.78)),
                border_color: BorderColor(Color::srgba(0.196, 0.275, 0.431, 0.28)),
                border_radius: BorderRadius::all(Val::Px(radius)),
                ..default()
            },
            MenuButton { action, primary },
            UiCapture,
        ))
        .with_children(|button| {
            button.spawn(TextBundle::from_section(
                label,
                text_style(9.5, Color::srgb(0.75, 0.8, 0.85)), // Smaller, more subtle font
            ));
        });
}

fn spawn_nav_button(parent: &mut ChildBuilder, name: &str, group: NavGroup) {
    parent
        .spawn((
            ButtonBundle {
                style: Style {
                    border: UiRect::all(Val::Px(0.5)), // Thinner borders
                    padding: UiRect::new(
                        Val::Px(6.0), // Reduced padding
                        Val::Px(6.0),
                        Val::Px(2.0),
                        Val::Px(2.0),
                    ),
                    ..default()
                },
                background_color: BackgroundColor(nav_button_color(false)),
                border_color: BorderColor(nav_button_border_color(false)),
                border_radius: BorderRadius::all(Val::Px(3.0)), // Smaller border radius
                ..default()
            },
            NavButton {
                name: name.to_string(),
                group,
            },
            UiCapture,
        ))
        .with_children(|button| {
            button.spawn((TextBundle::from_section(
                name,
                text_style(9.0, Color::srgb(0.75, 0.8, 0.85)), // Smaller, more subtle font
            ), NavButtonLabel));
        });
}

fn info_body_text(text: &str) -> Text {
    let mut body = Text::from_section(text, text_style(9.5, Color::srgb(0.82, 0.85, 0.88))); // Smaller, more subtle
    body.linebreak_behavior = BreakLineOn::WordBoundary;
    body
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
        (
            Color::srgb(0.16, 0.24, 0.39),
            Color::srgb(0.31, 0.47, 0.78),
        )
    } else if primary {
        (
            Color::srgba(0.031, 0.039, 0.063, 0.86),
            Color::srgba(0.196, 0.275, 0.431, 0.28),
        )
    } else {
        (
            Color::srgba(0.031, 0.039, 0.063, 0.78),
            Color::srgba(0.196, 0.275, 0.431, 0.28),
        )
    }
}

fn fps_color(fps: f32) -> Color {
    if fps >= 55.0 {
        Color::srgb(0.7, 0.85, 0.7)
    } else if fps >= 45.0 {
        Color::srgb(0.9, 0.9, 0.7)
    } else if fps >= 30.0 {
        Color::srgb(0.9, 0.8, 0.7)
    } else {
        Color::srgb(0.9, 0.7, 0.7)
    }
}

fn notification_color(notification_type: &NotificationType) -> Color {
    match notification_type {
        NotificationType::Success => Color::srgba(0.06, 0.25, 0.06, 0.86),
        NotificationType::Error => Color::srgba(0.25, 0.06, 0.06, 0.86),
        NotificationType::Info => Color::srgba(0.06, 0.13, 0.25, 0.86),
    }
}

fn get_celestial_type(name: &str) -> &'static str {
    match name {
        "Sun" => "G-type Main Sequence Star",
        "Mercury" | "Venus" | "Earth" | "Mars" => "Terrestrial Planet",
        "Jupiter" | "Saturn" => "Gas Giant",
        "Uranus" | "Neptune" => "Ice Giant",
        _ => "Natural Satellite",
    }
}

fn build_info_body(planet: &crate::domain::entities::planet::Planet) -> String {
    let mut lines = Vec::new();
    let name = planet.name.as_str();

    match name {
        "Sun" => {
            section_header(&mut lines, "Overview");
            lines.push("The solar system's central star, a hot ball of plasma powering planetary climates. Its gravity holds every planet, moon, and comet in orbit.".to_string());

            section_header(&mut lines, "Key Data");
            info_line(&mut lines, "Type", "G-type main-sequence star (G2V)");
            info_line(&mut lines, "Mass", "1.9885 x 10^30 kg (333,000 Earth masses)");
            info_line(&mut lines, "Radius", "696,340 km (109 Earth radii)");
            info_line(&mut lines, "Surface Temperature", "5,778 K (5,505 deg C)");
            info_line(&mut lines, "Luminosity", "3.83 x 10^26 W");
            info_line(&mut lines, "Age", "4.6 billion years");
            info_line(&mut lines, "Distance from Earth", "149.6 million km (1 AU)");
        }
        "Mercury" => {
            section_header(&mut lines, "Overview");
            lines.push("Mercury is the smallest planet and closest to the Sun. It experiences extreme temperature swings due to its lack of atmosphere.".to_string());

            section_header(&mut lines, "Key Data");
            info_line(&mut lines, "Type", "Terrestrial planet");
            info_line(&mut lines, "Mass", "3.301 x 10^23 kg (0.055 Earth masses)");
            info_line(&mut lines, "Radius", "2,439.7 km (0.383 Earth radii)");
            info_line(&mut lines, "Distance from Sun", "0.387 AU (57.9 million km)");
            info_line(&mut lines, "Orbital Period", "87.969 Earth days");
            info_line(&mut lines, "Rotation Period", "58.646 Earth days");
            info_line(&mut lines, "Surface Temperature", "-173 to 427 deg C");
            info_line(&mut lines, "Moons", "0");
        }
        "Venus" => {
            section_header(&mut lines, "Overview");
            lines.push("Venus is Earth's twin in size but has a toxic atmosphere that traps heat, making it the hottest planet in the solar system.".to_string());

            section_header(&mut lines, "Key Data");
            info_line(&mut lines, "Type", "Terrestrial planet");
            info_line(&mut lines, "Mass", "4.867 x 10^24 kg (0.815 Earth masses)");
            info_line(&mut lines, "Radius", "6,051.8 km (0.949 Earth radii)");
            info_line(&mut lines, "Distance from Sun", "0.723 AU (108.2 million km)");
            info_line(&mut lines, "Orbital Period", "224.701 Earth days");
            info_line(&mut lines, "Rotation Period", "243.025 Earth days (retrograde)");
            info_line(&mut lines, "Surface Temperature", "462 deg C (hottest planet)");
            info_line(&mut lines, "Atmosphere", "96.5% CO2, sulfuric acid clouds");
        }
        "Earth" => {
            section_header(&mut lines, "Overview");
            lines.push("Earth is the only planet known to support life, with liquid water on its surface and a protective atmosphere.".to_string());

            section_header(&mut lines, "Key Data");
            info_line(&mut lines, "Type", "Terrestrial planet");
            info_line(&mut lines, "Mass", "5.972 x 10^24 kg");
            info_line(&mut lines, "Radius", "6,371 km");
            info_line(&mut lines, "Distance from Sun", "1.000 AU (149.6 million km)");
            info_line(&mut lines, "Orbital Period", "365.256 days");
            info_line(&mut lines, "Rotation Period", "23.934 hours");
            info_line(&mut lines, "Surface Temperature", "-89 to 58 deg C");
            info_line(&mut lines, "Moons", "1 (The Moon)");
            info_line(&mut lines, "Atmosphere", "78% N2, 21% O2, trace gases");
        }
        "Mars" => {
            section_header(&mut lines, "Overview");
            lines.push("Mars is the Red Planet with polar ice caps and the largest volcano in the solar system. It is a prime target for exploration.".to_string());

            section_header(&mut lines, "Key Data");
            info_line(&mut lines, "Type", "Terrestrial planet");
            info_line(&mut lines, "Mass", "6.417 x 10^23 kg (0.107 Earth masses)");
            info_line(&mut lines, "Radius", "3,389.5 km (0.532 Earth radii)");
            info_line(&mut lines, "Distance from Sun", "1.524 AU (227.9 million km)");
            info_line(&mut lines, "Orbital Period", "686.980 Earth days");
            info_line(&mut lines, "Rotation Period", "24.623 hours");
            info_line(&mut lines, "Surface Temperature", "-87 to -5 deg C");
            info_line(&mut lines, "Moons", "2 (Phobos, Deimos)");
            info_line(&mut lines, "Atmosphere", "95% CO2, thin and dusty");
        }
        "Jupiter" => {
            section_header(&mut lines, "Overview");
            lines.push("Jupiter is the largest planet in the solar system, a gas giant with a powerful magnetic field and the famous Great Red Spot storm.".to_string());

            section_header(&mut lines, "Key Data");
            info_line(&mut lines, "Type", "Gas giant");
            info_line(&mut lines, "Mass", "1.898 x 10^27 kg (317.8 Earth masses)");
            info_line(&mut lines, "Radius", "69,911 km (10.97 Earth radii)");
            info_line(&mut lines, "Distance from Sun", "5.204 AU (778.6 million km)");
            info_line(&mut lines, "Orbital Period", "11.862 years");
            info_line(&mut lines, "Rotation Period", "9.925 hours");
            info_line(&mut lines, "Atmosphere", "Hydrogen, helium, ammonia clouds");
        }
        "Saturn" => {
            section_header(&mut lines, "Overview");
            lines.push("Saturn is known for its spectacular ring system, composed of ice and rock particles. It is a gas giant like Jupiter.".to_string());

            section_header(&mut lines, "Key Data");
            info_line(&mut lines, "Type", "Gas giant");
            info_line(&mut lines, "Mass", "5.683 x 10^26 kg (95.2 Earth masses)");
            info_line(&mut lines, "Radius", "58,232 km (9.14 Earth radii)");
            info_line(&mut lines, "Distance from Sun", "9.582 AU (1.433 billion km)");
            info_line(&mut lines, "Orbital Period", "29.457 years");
            info_line(&mut lines, "Rotation Period", "10.7 hours");
            info_line(&mut lines, "Moons", "146+ (major: Titan, Enceladus, Mimas)");
            info_line(&mut lines, "Rings", "Complex ring system of ice and rock");
            info_line(&mut lines, "Atmosphere", "Hydrogen, helium, trace methane");
        }
        "Uranus" => {
            section_header(&mut lines, "Overview");
            lines.push("Uranus is an ice giant with a unique sideways rotation. Its atmosphere gives it a pale blue-green color.".to_string());

            section_header(&mut lines, "Key Data");
            info_line(&mut lines, "Type", "Ice giant");
            info_line(&mut lines, "Mass", "8.681 x 10^25 kg (14.5 Earth masses)");
            info_line(&mut lines, "Radius", "25,362 km (4.01 Earth radii)");
            info_line(&mut lines, "Distance from Sun", "19.201 AU (2.871 billion km)");
            info_line(&mut lines, "Orbital Period", "84.0205 years");
            info_line(&mut lines, "Rotation Period", "17.24 hours (retrograde)");
            info_line(&mut lines, "Axial Tilt", "97.77 deg (rotates on its side)");
            info_line(&mut lines, "Atmosphere", "Hydrogen, helium, methane (blue color)");
        }
        "Neptune" => {
            section_header(&mut lines, "Overview");
            lines.push("Neptune is an ice giant known for its vivid blue color and supersonic winds. It is the farthest planet from the Sun.".to_string());

            section_header(&mut lines, "Key Data");
            info_line(&mut lines, "Type", "Ice giant");
            info_line(&mut lines, "Mass", "1.024 x 10^26 kg (17.1 Earth masses)");
            info_line(&mut lines, "Radius", "24,622 km (3.88 Earth radii)");
            info_line(&mut lines, "Distance from Sun", "30.047 AU (4.495 billion km)");
            info_line(&mut lines, "Orbital Period", "164.8 years");
            info_line(&mut lines, "Rotation Period", "16.11 hours");
            info_line(&mut lines, "Moons", "14 known (major: Triton)");
            info_line(&mut lines, "Atmosphere", "Hydrogen, helium, methane (deep blue)");
        }
        _ => {
            let parent = get_parent_body(name);
            section_header(&mut lines, "Overview");
            lines.push(format!("{} is a natural satellite orbiting {} in the solar system.", name, parent));

            section_header(&mut lines, "Key Data");
            info_line(&mut lines, "Type", "Natural satellite");
            info_line(&mut lines, "Parent Planet", parent);
            info_line(&mut lines, "Discovery", get_discovery_info(name));
        }
    }

    section_header(&mut lines, "Interesting Facts");
    for fact in get_fun_facts(name) {
        lines.push(format!("- {}", fact));
    }

    section_header(&mut lines, "Exploration");
    lines.push(get_exploration_status(name).to_string());

    lines.join("
")
}

fn section_header(lines: &mut Vec<String>, title: &str) {
    if !lines.is_empty() {
        lines.push(String::new());
    }
    lines.push(title.to_string());
}

fn info_line(lines: &mut Vec<String>, label: &str, value: &str) {
    lines.push(format!("{}: {}", label, value));
}

fn get_fun_facts(name: &str) -> Vec<String> {
    match name {
        "Sun" => vec![
            "The Sun accounts for 99.86% of the solar system's mass".to_string(),
            "It will eventually expand into a red giant".to_string(),
            "The Sun's core temperature reaches 15 million deg C".to_string(),
        ],
        "Mercury" => vec![
            "Mercury has the most eccentric orbit of all planets".to_string(),
            "A day on Mercury is longer than its year".to_string(),
            "It has no atmosphere, only a thin exosphere".to_string(),
        ],
        "Venus" => vec![
            "Venus rotates backwards compared to most planets".to_string(),
            "Its surface pressure is 92 times Earth's".to_string(),
            "It rains sulfuric acid, but it evaporates before reaching the ground".to_string(),
        ],
        "Earth" => vec![
            "Earth is the only known planet with liquid water on the surface".to_string(),
            "It has a powerful magnetic field protecting life".to_string(),
            "Earth's atmosphere is 21% oxygen".to_string(),
        ],
        "Mars" => vec![
            "Mars has the tallest volcano in the solar system (Olympus Mons)".to_string(),
            "It has the largest canyon (Valles Marineris)".to_string(),
            "Mars has seasons similar to Earth".to_string(),
        ],
        "Jupiter" => vec![
            "Jupiter's Great Red Spot is a storm larger than Earth".to_string(),
            "It has a strong magnetic field and faint rings".to_string(),
            "Jupiter emits more heat than it receives from the Sun".to_string(),
        ],
        "Saturn" => vec![
            "Saturn's rings are made of ice and rock particles".to_string(),
            "It is the least dense planet and could float in water".to_string(),
            "Saturn has more moons than any other planet".to_string(),
        ],
        "Uranus" => vec![
            "Uranus rotates on its side, likely due to a massive collision".to_string(),
            "It has the coldest atmosphere of any planet".to_string(),
            "Its rings are faint and dark".to_string(),
        ],
        "Neptune" => vec![
            "Neptune has the fastest winds in the solar system".to_string(),
            "It was mathematically predicted before it was observed".to_string(),
            "Neptune's moon Triton orbits backward".to_string(),
        ],
        "Moon" => vec![
            "The Moon is tidally locked, showing the same face to Earth".to_string(),
            "It is drifting away from Earth by about 3.8 cm per year".to_string(),
            "The Moon has water ice in its polar craters".to_string(),
        ],
        "Phobos" => vec![
            "Phobos orbits Mars faster than Mars rotates".to_string(),
            "It is slowly spiraling inward and may one day break apart".to_string(),
            "Phobos has a huge impact crater called Stickney".to_string(),
        ],
        "Deimos" => vec![
            "Deimos is smaller and farther from Mars than Phobos".to_string(),
            "It has a smooth surface due to a layer of regolith".to_string(),
            "Deimos takes 30.3 hours to orbit Mars".to_string(),
        ],
        "Io" => vec![
            "Io is the most volcanically active body in the solar system".to_string(),
            "It has hundreds of active volcanoes".to_string(),
            "Its surface is constantly reshaped by lava".to_string(),
        ],
        "Europa" => vec![
            "Europa likely has a subsurface ocean beneath its icy crust".to_string(),
            "It is a prime target for the search for extraterrestrial life".to_string(),
            "Its surface is crisscrossed by dark lines".to_string(),
        ],
        "Ganymede" => vec![
            "Ganymede is the largest moon in the solar system".to_string(),
            "It has its own magnetic field".to_string(),
            "It may have a subsurface ocean".to_string(),
        ],
        "Callisto" => vec![
            "Callisto has the most heavily cratered surface in the solar system".to_string(),
            "It may have a subsurface ocean".to_string(),
            "Callisto is the second-largest moon of Jupiter".to_string(),
        ],
        "Mimas" => vec![
            "Mimas has a giant crater that makes it look like the Death Star".to_string(),
            "It is one of Saturn's smaller moons".to_string(),
            "Mimas orbits inside Saturn's rings".to_string(),
        ],
        "Enceladus" => vec![
            "Enceladus has geysers that spew water and ice".to_string(),
            "It has a subsurface ocean and is a key target for astrobiology".to_string(),
            "Its surface is very bright and reflective".to_string(),
        ],
        "Tethys" => vec![
            "Tethys has a massive canyon called Ithaca Chasma".to_string(),
            "It has a huge crater called Odysseus".to_string(),
            "Tethys is mostly composed of water ice".to_string(),
        ],
        "Dione" => vec![
            "Dione has bright ice cliffs and heavily cratered terrain".to_string(),
            "It has a tenuous oxygen atmosphere".to_string(),
            "Dione may have a subsurface ocean".to_string(),
        ],
        "Rhea" => vec![
            "Rhea is the second-largest moon of Saturn".to_string(),
            "It has a very thin atmosphere of oxygen and carbon dioxide".to_string(),
            "Rhea's surface is heavily cratered".to_string(),
        ],
        "Titan" => vec![
            "Titan has a thick atmosphere and lakes of liquid methane".to_string(),
            "It is larger than Mercury".to_string(),
            "It has weather cycles similar to Earth's water cycle".to_string(),
        ],
        "Hyperion" => vec![
            "Hyperion has a sponge-like appearance due to its porous surface".to_string(),
            "It rotates chaotically".to_string(),
            "It is one of Saturn's irregular moons".to_string(),
        ],
        "Iapetus" => vec![
            "Iapetus has a dramatic two-tone coloration".to_string(),
            "It has an equatorial ridge giving it a walnut shape".to_string(),
            "Iapetus orbits far from Saturn".to_string(),
        ],
        "Miranda" => vec![
            "Miranda has some of the most varied terrain in the solar system".to_string(),
            "It has giant cliffs up to 20 km high".to_string(),
            "Miranda was likely shattered and reassembled".to_string(),
        ],
        "Ariel" => vec![
            "Ariel has the brightest surface of Uranus's moons".to_string(),
            "It shows evidence of past geological activity".to_string(),
            "Ariel's surface is covered with canyons".to_string(),
        ],
        "Umbriel" => vec![
            "Umbriel is the darkest of Uranus's major moons".to_string(),
            "It has an ancient, heavily cratered surface".to_string(),
            "Umbriel may contain a subsurface ocean".to_string(),
        ],
        "Titania" => vec![
            "Titania is the largest moon of Uranus".to_string(),
            "It has a complex system of faults and canyons".to_string(),
            "Titania may have a subsurface ocean".to_string(),
        ],
        "Oberon" => vec![
            "Oberon is the second-largest moon of Uranus".to_string(),
            "It has a heavily cratered and icy surface".to_string(),
            "Oberon may have formed from a collision".to_string(),
        ],
        "Triton" => vec![
            "Triton is the only large moon with a retrograde orbit".to_string(),
            "It has nitrogen geysers on its surface".to_string(),
            "Triton is slowly spiraling toward Neptune".to_string(),
        ],
        "Proteus" => vec![
            "Proteus is irregularly shaped".to_string(),
            "It is one of Neptune's largest moons".to_string(),
            "Proteus was discovered by Voyager 2".to_string(),
        ],
        "Nereid" => vec![
            "Nereid has a highly eccentric orbit".to_string(),
            "It was discovered in 1949".to_string(),
            "Nereid is one of Neptune's outer moons".to_string(),
        ],
        "Larissa" => vec![
            "Larissa was discovered in 1981".to_string(),
            "It is one of Neptune's inner moons".to_string(),
            "Larissa orbits inside Neptune's faint rings".to_string(),
        ],
        _ => vec![format!("{} has unique and fascinating characteristics", name)],
    }
}

fn get_exploration_status(name: &str) -> &'static str {
    match name {
        "Sun" => "Studied by SOHO, SDO, and Parker Solar Probe",
        "Mercury" => "Mariner 10 (1974-1975), MESSENGER (2011-2015), BepiColombo (en route)",
        "Venus" => "Venera program (1960s-1980s), Magellan (1990-1994), Akatsuki (2015-present)",
        "Earth" => "Humanity's home - extensively explored and mapped",
        "Mars" => "Perseverance, Curiosity, Insight (active), Mars Sample Return (planned)",
        "Jupiter" => "Pioneer 10 (1973), Voyager (1979), Galileo (1995-2003), Juno (2016-present)",
        "Saturn" => "Pioneer 11 (1979), Voyager (1980-1981), Cassini-Huygens (2004-2017)",
        "Uranus" => "Voyager 2 flyby (1986) - only spacecraft to visit",
        "Neptune" => "Voyager 2 flyby (1989) - only spacecraft to visit",
        "Moon" => "Apollo missions (1969-1972), Luna program, Chang'e missions, Artemis (planned)",
        _ => "Observed remotely by telescopes and space probes",
    }
}

fn get_parent_body(name: &str) -> &'static str {
    match name {
        "Phobos" | "Deimos" => "Mars",
        "Io" | "Europa" | "Ganymede" | "Callisto" => "Jupiter",
        "Mimas" | "Enceladus" | "Tethys" | "Dione" | "Rhea" | "Titan" | "Hyperion" | "Iapetus" => {
            "Saturn"
        }
        "Miranda" | "Ariel" | "Umbriel" | "Titania" | "Oberon" => "Uranus",
        "Triton" | "Proteus" | "Nereid" | "Larissa" => "Neptune",
        "Moon" => "Earth",
        _ => "Unknown",
    }
}

fn get_discovery_info(name: &str) -> &'static str {
    match name {
        "Moon" => "Ancient times",
        "Phobos" | "Deimos" => "1877 (Asaph Hall)",
        "Io" | "Europa" | "Ganymede" | "Callisto" => "1610 (Galileo)",
        "Mimas" | "Enceladus" | "Tethys" | "Dione" | "Rhea" | "Titan" | "Hyperion" | "Iapetus" => {
            "Various 17th-19th century"
        }
        "Miranda" | "Ariel" | "Umbriel" | "Titania" | "Oberon" => "1787-1851 (William Herschel)",
        "Triton" => "1846 (William Lassell)",
        _ => "Various space missions",
    }
}

fn planet_names() -> [&'static str; 9] {
    [
        "Sun",
        "Mercury",
        "Venus",
        "Earth",
        "Mars",
        "Jupiter",
        "Saturn",
        "Uranus",
        "Neptune",
    ]
}

fn moon_list() -> [&'static str; 24] {
    [
        "Moon",
        "Phobos",
        "Deimos",
        "Io",
        "Europa",
        "Ganymede",
        "Callisto",
        "Mimas",
        "Enceladus",
        "Tethys",
        "Dione",
        "Rhea",
        "Titan",
        "Hyperion",
        "Iapetus",
        "Miranda",
        "Ariel",
        "Umbriel",
        "Titania",
        "Oberon",
        "Triton",
        "Proteus",
        "Nereid",
        "Larissa",
    ]
}

fn moon_names_for_parent(parent: &str) -> &'static [&'static str] {
    match parent {
        "Earth" => &["Moon"],
        "Mars" => &["Phobos", "Deimos"],
        "Jupiter" => &["Io", "Europa", "Ganymede", "Callisto"],
        "Saturn" => &["Mimas", "Enceladus", "Tethys", "Dione", "Rhea", "Titan", "Hyperion", "Iapetus"],
        "Uranus" => &["Miranda", "Ariel", "Umbriel", "Titania", "Oberon"],
        "Neptune" => &["Triton", "Proteus", "Nereid", "Larissa"],
        _ => &[],
    }
}

fn moon_pairs() -> impl Iterator<Item = (&'static str, &'static str)> {
    moon_list()
        .into_iter()
        .map(|name| (get_parent_body(name), name))
}

fn is_primary_body(name: &str) -> bool {
    planet_names().contains(&name)
}
