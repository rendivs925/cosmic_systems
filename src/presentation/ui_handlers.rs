use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};

use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::{
    NotificationQueue, PerformanceStats, PlanetComponent, ScreenshotState,
    Selectable, SelectedPlanet, UiPointerState, ZenMode,
};
use crate::infrastructure::bevy_adapters::craft_components::CraftTravelTarget;
use crate::presentation::ui_components::*;
use crate::presentation::ui_helpers::*;

pub fn handle_nav_interactions(
    interactions: Query<(&Interaction, &NavButton), Changed<Interaction>>,
    menu_interactions: Query<(&Interaction, &MenuButton), Changed<Interaction>>,
    info_card_interactions: Query<&Interaction, (Changed<Interaction>, With<InfoCardToggleButton>)>,
    mut selected_planet: ResMut<SelectedPlanet>,
    mut selectable_query: Query<(Entity, &mut Selectable)>,
    mut solar_params: ResMut<SolarSystemParameters>,
    mut menu_state: ResMut<UiMenuState>,
    mut craft_target: Option<ResMut<CraftTravelTarget>>,
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

        if let Some(target) = craft_target.as_mut() {
            target.entity = target_entity;
            target.name = target_entity.map(|_| button.name.clone());
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

pub fn update_navbar(
    selected_planet: Res<SelectedPlanet>,
    solar_params: Res<SolarSystemParameters>,
    performance_stats: Res<PerformanceStats>,
    _screenshot_state: Res<ScreenshotState>,
    video_state: Res<crate::infrastructure::bevy_adapters::ui_components::VideoRecordingState>,
    _notifications: Res<NotificationQueue>,
    zen_mode: Res<ZenMode>,
    menu_state: Res<UiMenuState>,
    time: Res<Time>,
    mut last_update: Local<f32>,
    mut queries: ParamSet<(
        Query<(
            &NavButton,
            &mut Node,
            &mut BackgroundColor,
            &mut BorderColor,
        )>,
        Query<(
            &MenuButton,
            &mut Node,
            &mut BackgroundColor,
            &mut BorderColor,
        )>,
        Query<&mut Text, With<FpsText>>,
        Query<&mut Node, With<SelectorPanelRoot>>,
    )>,
) {
    let hide_ui = zen_mode.enabled;

    let current_time = time.elapsed_secs();

    // Reduce update frequency during video recording to prevent UI flickering
    let update_interval = if video_state.is_recording { 0.1 } else { 0.016 }; // 10 FPS during recording, 60 FPS normally

    if current_time - *last_update < update_interval {
        return;
    }
    *last_update = current_time;

    let selected_name = selected_planet.name.as_deref();
    let _active_parent = selected_name.map(|name| {
        if is_primary_body(name) {
            name
        } else {
            get_parent_body(name)
        }
    });

    let show_selector = menu_state.selector_open;

    // No longer need moon visibility logic - all bodies shown in unified list

    if let Ok(mut style) = queries.p3().single_mut() {
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
        border.top = nav_button_border_color(is_selected);
        border.right = nav_button_border_color(is_selected);
        border.bottom = nav_button_border_color(is_selected);
        border.left = nav_button_border_color(is_selected);
    }

    for (button, mut style, mut background, mut border) in queries.p1().iter_mut() {
        let active = match button.action {
            MenuAction::Orbits => solar_params.show_orbits,
            MenuAction::Explore => menu_state.selector_open,
        };
        if hide_ui {
            style.display = Display::None;
            *background = BackgroundColor(Color::NONE);
            border.top = Color::NONE;
            border.right = Color::NONE;
            border.bottom = Color::NONE;
            border.left = Color::NONE;
            continue;
        }
        style.display = Display::Flex;
        let (bg, stroke) = menu_button_colors(button.primary, active);
        *background = BackgroundColor(bg);
        border.top = stroke;
        border.right = stroke;
        border.bottom = stroke;
        border.left = stroke;
    }

    if let Ok(mut text) = queries.p2().single_mut() {
        let display_fps = performance_stats.average_fps;
        if hide_ui {
            *text = Text::new("");
        } else {
            *text = Text::new(format!("fps {:.0}", display_fps));
        }
    }
}

pub fn update_info_card(
    selected_planet: Res<SelectedPlanet>,
    planet_query: Query<&PlanetComponent>,
    menu_state: Res<UiMenuState>,
    zen_mode: Res<ZenMode>,
    mut root_query: Query<&mut Node, With<InfoCardRoot>>,
    mut text_queries: ParamSet<(
        Query<&mut Text, With<InfoCardTitle>>,
        Query<&mut Text, With<InfoCardSubtitle>>,
        Query<&mut Text, With<InfoCardBody>>,
    )>,
) {
    let Ok(mut root_style) = root_query.single_mut() else {
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

    if let Ok(mut title) = text_queries.p0().single_mut() {
        *title = Text::new(planet.domain_planet.name.clone());
    }

    if let Ok(mut subtitle) = text_queries.p1().single_mut() {
        *subtitle = Text::new(get_celestial_type(&planet.domain_planet.name).to_string());
    }

    if let Ok(mut body) = text_queries.p2().single_mut() {
        *body = Text::new(build_info_body(&planet.domain_planet));
    }
}

pub fn update_notifications_ui(
    mut notifications: ResMut<NotificationQueue>,
    mut commands: Commands,
    roots: Res<UiRoots>,
    children_query: Query<&Children>,
    time: Res<Time>,
    video_state: Res<crate::infrastructure::bevy_adapters::ui_components::VideoRecordingState>,
    mut last_update: Local<f32>,
) {
    let current_time = time.elapsed_secs();

    // Reduce notification update frequency during video recording to prevent UI flickering
    let update_interval = if video_state.is_recording { 0.2 } else { 0.016 }; // 5 FPS during recording, 60 FPS normally

    if current_time - *last_update < update_interval {
        return;
    }
    *last_update = current_time;

    notifications
        .notifications
        .retain(|n| current_time - n.created_at < n.duration);

    // Keep only the most recent few notifications to avoid stacking long lists
    const MAX_NOTIFICATIONS: usize = 3;
    if notifications.notifications.len() > MAX_NOTIFICATIONS {
        let excess = notifications.notifications.len() - MAX_NOTIFICATIONS;
        notifications.notifications.drain(0..excess);
    }

    // Clear any existing notification UI elements before spawning new ones
    if let Ok(children) = children_query.get(roots.notifications) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    commands
        .entity(roots.notifications)
        .with_children(|parent| {
            for notification in notifications.notifications.iter() {
                parent
                    .spawn((
                        Node {
                            border: UiRect::all(Val::Px(1.0)),
                            padding: UiRect::all(Val::Px(10.0)),
                            ..default()
                        },
                        BackgroundColor(notification_color(&notification.notification_type)),
                        BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.35)),
                        BorderRadius::all(Val::Px(8.0)),
                        NotificationUi,
                        UiCapture,
                        Interaction::default(),
                    ))
                    .with_children(|row| {
                        let (font, color) = text_style(12.0, Color::srgb(0.95, 0.95, 0.98));
                        row.spawn((Text::new(notification.message.clone()), font, color));
                    });
            }
        });
}

pub fn update_ui_hover_state(
    mut ui_state: ResMut<UiPointerState>,
    query: Query<&Interaction, With<UiCapture>>,
) {
    ui_state.is_over_ui = query
        .iter()
        .any(|interaction| matches!(interaction, Interaction::Hovered | Interaction::Pressed));
}

pub fn update_cursor_icon(
    mut commands: Commands,
    mut last_cursor: Local<Option<SystemCursorIcon>>,
    windows: Query<Entity, With<Window>>,
    nav_buttons: Query<&Interaction, With<NavButton>>,
    menu_buttons: Query<&Interaction, With<MenuButton>>,
    toggle_buttons: Query<&Interaction, With<InfoCardToggleButton>>,
    ext_buttons: Query<&Interaction, With<InfoCardExternalToggle>>,
) {
    let Ok(window_entity) = windows.single() else {
        return;
    };
    let hovering = nav_buttons
        .iter()
        .chain(menu_buttons.iter())
        .chain(toggle_buttons.iter())
        .chain(ext_buttons.iter())
        .any(|interaction| *interaction == Interaction::Hovered);

    let new_cursor = if hovering {
        SystemCursorIcon::Pointer
    } else {
        SystemCursorIcon::Default
    };
    if *last_cursor == Some(new_cursor) {
        return;
    }
    *last_cursor = Some(new_cursor);
    commands.entity(window_entity).insert(CursorIcon::System(new_cursor));
}
