use bevy::prelude::*;
use bevy::text::BreakLineOn;

use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::{
    NotificationQueue, NotificationType, PerformanceStats, PlanetComponent, Selectable,
    SelectedPlanet, UiPointerState,
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
pub(crate) struct OrbitToggleButton;

#[derive(Component)]
pub(crate) struct OrbitToggleLabel;

#[derive(Component)]
pub(crate) struct NavButton {
    name: String,
}

#[derive(Component)]
pub(crate) struct NavButtonLabel;

#[derive(Component)]
pub(crate) struct FpsText;

#[derive(Component)]
pub(crate) struct InfoCardRoot;

#[derive(Component)]
pub(crate) struct InfoCardTitle;

#[derive(Component)]
pub(crate) struct InfoCardSubtitle;

#[derive(Component)]
pub(crate) struct InfoCardBody;

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

    let navbar = commands
        .spawn(NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Px(56.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                background_color: BackgroundColor(Color::srgba(0.04, 0.05, 0.07, 0.9)),
                ..default()
            },
        )
        .id();

    commands.entity(navbar).with_children(|parent| {
        parent
            .spawn(NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    ..default()
                },
                ..default()
            })
            .with_children(|left| {
                left.spawn(TextBundle::from_section(
                    "COSMIC",
                    text_style(13.0, Color::srgb(0.7, 0.8, 0.95)),
                ));

                left.spawn((
                    ButtonBundle {
                        style: Style {
                            padding: UiRect::new(
                                Val::Px(10.0),
                                Val::Px(10.0),
                                Val::Px(4.0),
                                Val::Px(4.0),
                            ),
                            ..default()
                        },
                        background_color: BackgroundColor(Color::srgba(0.15, 0.2, 0.3, 0.9)),
                        ..default()
                    },
                    OrbitToggleButton,
                    UiCapture,
                ))
                .with_children(|button| {
                    button.spawn((
                        TextBundle::from_section(
                            "Hide Orbits",
                            text_style(11.0, Color::srgb(0.85, 0.9, 1.0)),
                        ),
                        OrbitToggleLabel,
                    ));
                });
            });

        parent
            .spawn(NodeBundle {
                style: Style {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    row_gap: Val::Px(4.0),
                    ..default()
                },
                ..default()
            })
            .with_children(|center| {
                for name in ordered_names() {
                    center
                        .spawn((
                            ButtonBundle {
                                style: Style {
                                    padding: UiRect::new(
                                        Val::Px(8.0),
                                        Val::Px(8.0),
                                        Val::Px(3.0),
                                        Val::Px(3.0),
                                    ),
                                    ..default()
                                },
                                background_color: BackgroundColor(Color::srgba(0.08, 0.1, 0.14, 0.85)),
                                ..default()
                            },
                            NavButton {
                                name: name.to_string(),
                            },
                            UiCapture,
                        ))
                        .with_children(|button| {
                            button.spawn((
                                TextBundle::from_section(
                                    name,
                                    text_style(10.5, Color::srgb(0.85, 0.9, 1.0)),
                                ),
                                NavButtonLabel,
                            ));
                        });
                }
            });

        parent
            .spawn(NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    ..default()
                },
                ..default()
            })
            .with_children(|right| {
                right.spawn((
                    TextBundle::from_section("fps 0", text_style(11.0, Color::srgb(0.75, 0.8, 0.9))),
                    FpsText,
                ));
            });
    });

    let info_card = commands
        .spawn((
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    right: Val::Px(20.0),
                    top: Val::Px(20.0),
                    width: Val::Px(360.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    display: Display::None,
                    ..default()
                },
                background_color: BackgroundColor(Color::srgba(0.03, 0.04, 0.06, 0.92)),
                ..default()
            },
            InfoCardRoot,
            UiCapture,
            Interaction::default(),
        ))
        .id();

    commands.entity(info_card).with_children(|parent| {
        parent.spawn((
            TextBundle::from_section("", text_style(18.0, Color::srgb(0.9, 0.95, 1.0))),
            InfoCardTitle,
        ));
        parent.spawn((
            TextBundle::from_section("", text_style(11.0, Color::srgb(0.65, 0.7, 0.8))),
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
    orbit_interactions: Query<&Interaction, (Changed<Interaction>, With<OrbitToggleButton>)>,
    mut selected_planet: ResMut<SelectedPlanet>,
    mut selectable_query: Query<(Entity, &mut Selectable)>,
    mut solar_params: ResMut<SolarSystemParameters>,
) {
    for interaction in orbit_interactions.iter() {
        if *interaction == Interaction::Pressed {
            solar_params.show_orbits = !solar_params.show_orbits;
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

        selected_planet.entity = target_entity;
        selected_planet.name = target_entity.map(|_| button.name.clone());

        for (entity, mut selectable) in selectable_query.iter_mut() {
            selectable.selected = Some(entity) == target_entity;
        }
    }
}

pub(crate) fn update_navbar(
    selected_planet: Res<SelectedPlanet>,
    solar_params: Res<SolarSystemParameters>,
    performance_stats: Res<PerformanceStats>,
    mut queries: ParamSet<(
        Query<(&NavButton, &mut BackgroundColor)>,
        Query<&mut BackgroundColor, With<OrbitToggleButton>>,
        Query<&mut Text, With<OrbitToggleLabel>>,
        Query<&mut Text, With<FpsText>>,
    )>,
) {
    let selected_name = selected_planet.name.as_deref();
    for (button, mut background) in queries.p0().iter_mut() {
        let is_selected = selected_name == Some(button.name.as_str());
        *background = BackgroundColor(nav_button_color(is_selected));
    }

    if let Ok(mut background) = queries.p1().get_single_mut() {
        *background = BackgroundColor(orbit_button_color(solar_params.show_orbits));
    }
    if let Ok(mut text) = queries.p2().get_single_mut() {
        text.sections[0].value = if solar_params.show_orbits {
            "Hide Orbits".to_string()
        } else {
            "Show Orbits".to_string()
        };
    }

    if let Ok(mut text) = queries.p3().get_single_mut() {
        text.sections[0].value = format!("fps {:.0}", performance_stats.fps);
        text.sections[0].style.color = fps_color(performance_stats.fps);
    }
}

pub(crate) fn update_info_card(
    selected_planet: Res<SelectedPlanet>,
    planet_query: Query<&PlanetComponent>,
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
) {
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
                            padding: UiRect::all(Val::Px(10.0)),
                            ..default()
                        },
                        background_color: BackgroundColor(
                            notification_color(&notification.notification_type),
                        ),
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

fn info_body_text(text: &str) -> Text {
    let mut body = Text::from_section(text, text_style(11.0, Color::srgb(0.85, 0.9, 0.95)));
    body.linebreak_behavior = BreakLineOn::WordBoundary;
    body
}

fn nav_button_color(selected: bool) -> Color {
    if selected {
        Color::srgba(0.16, 0.22, 0.34, 0.95)
    } else {
        Color::srgba(0.08, 0.1, 0.14, 0.85)
    }
}

fn orbit_button_color(enabled: bool) -> Color {
    if enabled {
        Color::srgba(0.15, 0.2, 0.3, 0.9)
    } else {
        Color::srgba(0.1, 0.12, 0.16, 0.8)
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
        NotificationType::Success => Color::srgba(0.08, 0.35, 0.12, 0.9),
        NotificationType::Error => Color::srgba(0.35, 0.1, 0.1, 0.9),
        NotificationType::Info => Color::srgba(0.08, 0.18, 0.35, 0.9),
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
    lines.push(format!("Type: {}", get_celestial_type(&planet.name)));
    lines.push(format!("Radius: {:.1} km", planet.radius_km));
    lines.push(format!("Mass: {:.3e} kg", planet.mass_kg));

    if planet.orbital_distance_au > 0.0 {
        lines.push(format!("Orbital Distance: {:.3} AU", planet.orbital_distance_au));
    } else {
        lines.push("Orbital Distance: N/A".to_string());
    }

    if planet.orbital_period_days > 0.0 {
        lines.push(format!(
            "Orbital Period: {:.2} days",
            planet.orbital_period_days
        ));
    } else {
        lines.push("Orbital Period: N/A".to_string());
    }

    if planet.rotation_period_hours != 0.0 {
        lines.push(format!(
            "Rotation Period: {:.2} hours",
            planet.rotation_period_hours
        ));
    } else {
        lines.push("Rotation Period: N/A".to_string());
    }

    if planet.axial_tilt_deg != 0.0 {
        lines.push(format!("Axial Tilt: {:.2} deg", planet.axial_tilt_deg));
    } else {
        lines.push("Axial Tilt: N/A".to_string());
    }

    if let Some(parent) = &planet.parent_entity {
        lines.push(format!("Parent: {}", parent));
    }

    lines.join("\n")
}

fn ordered_names() -> [&'static str; 33] {
    [
        "Sun",
        "Mercury",
        "Venus",
        "Earth",
        "Moon",
        "Mars",
        "Phobos",
        "Deimos",
        "Jupiter",
        "Io",
        "Europa",
        "Ganymede",
        "Callisto",
        "Saturn",
        "Mimas",
        "Enceladus",
        "Tethys",
        "Dione",
        "Rhea",
        "Titan",
        "Hyperion",
        "Iapetus",
        "Uranus",
        "Miranda",
        "Ariel",
        "Umbriel",
        "Titania",
        "Oberon",
        "Neptune",
        "Triton",
        "Proteus",
        "Nereid",
        "Larissa",
    ]
}
