use bevy::prelude::*;
use super::components::*;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::{
    NotificationQueue, PerformanceStats, ScreenshotState, SelectedPlanet, Selectable, ZenMode,
};

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
    video_state: Res<crate::infrastructure::bevy_adapters::ui_components::VideoRecordingState>,
    notifications: Res<NotificationQueue>,
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
    let active_parent = selected_name.map(|name| {
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

fn is_primary_body(name: &str) -> bool {
    planet_names().contains(&name)
}

fn planet_names() -> [&'static str; 9] {
    [
        "Sun", "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
    ]
}