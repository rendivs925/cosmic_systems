use bevy::prelude::*;

// Shared utility functions for UI components

pub fn text_style(font_size: f32, color: Color) -> (TextFont, TextColor) {
    (
        TextFont {
            font_size,
            ..default()
        },
        TextColor(color),
    )
}

pub fn nav_button_color(selected: bool) -> Color {
    if selected {
        Color::srgb(0.16, 0.24, 0.39)
    } else {
        Color::srgba(0.031, 0.039, 0.063, 0.78)
    }
}

pub fn nav_button_border_color(selected: bool) -> Color {
    if selected {
        Color::srgb(0.31, 0.47, 0.78)
    } else {
        Color::srgba(0.196, 0.275, 0.431, 0.28)
    }
}

pub fn nav_button_color_hover(selected: bool, hovered: bool) -> Color {
    if selected {
        Color::srgb(0.16, 0.24, 0.39) // Selected color unchanged
    } else if hovered {
        Color::srgba(0.051, 0.059, 0.083, 0.85) // Slightly brighter on hover
    } else {
        Color::srgba(0.031, 0.039, 0.063, 0.78) // Normal color
    }
}

pub fn nav_button_border_color_hover(selected: bool, hovered: bool) -> Color {
    if selected {
        Color::srgb(0.31, 0.47, 0.78) // Selected border unchanged
    } else if hovered {
        Color::srgba(0.216, 0.295, 0.451, 0.35) // Slightly brighter border on hover
    } else {
        Color::srgba(0.196, 0.275, 0.431, 0.28) // Normal border
    }
}

pub fn menu_button_colors(primary: bool, active: bool) -> (Color, Color) {
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

pub fn fps_color(fps: f32) -> Color {
    if fps >= 58.0 {
        Color::srgb(0.2, 0.8, 0.2) // Green for good performance
    } else if fps >= 30.0 {
        Color::srgb(0.8, 0.8, 0.2) // Yellow for acceptable performance
    } else {
        Color::srgb(0.8, 0.2, 0.2) // Red for poor performance
    }
}

pub fn notification_color(notification_type: &crate::infrastructure::bevy_adapters::components::NotificationType) -> Color {
    match notification_type {
        crate::infrastructure::bevy_adapters::components::NotificationType::Info => Color::srgb(0.4, 0.6, 0.8),
        crate::infrastructure::bevy_adapters::components::NotificationType::Success => Color::srgb(0.4, 0.8, 0.4),
        crate::infrastructure::bevy_adapters::components::NotificationType::Warning => Color::srgb(0.8, 0.8, 0.4),
        crate::infrastructure::bevy_adapters::components::NotificationType::Error => Color::srgb(0.8, 0.4, 0.4),
    }
}

pub fn planet_names() -> [&'static str; 9] {
    [
        "Sun", "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
    ]
}

pub fn moon_names_for_parent(parent: &str) -> &'static [&'static str] {
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