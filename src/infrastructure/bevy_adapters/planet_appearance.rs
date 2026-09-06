//! Rendering appearance catalog for configured celestial bodies.

use bevy::prelude::Color;

/// Preserve the established body colors without making physical catalog data
/// depend on rendering types.
pub fn color_for_body(name: &str) -> Option<Color> {
    let color = match name {
        "Sun" => Color::srgb(1.0, 1.0, 0.9),
        "Mercury" => Color::srgb(0.45, 0.45, 0.42),
        "Venus" => Color::srgb(0.95, 0.85, 0.65),
        "Earth" => Color::srgb(0.25, 0.45, 0.85),
        "Mars" => Color::srgb(0.85, 0.35, 0.15),
        "Jupiter" => Color::srgb(0.85, 0.65, 0.45),
        "Saturn" => Color::srgb(0.9, 0.8, 0.5),
        "Uranus" => Color::srgb(0.6, 0.8, 0.9),
        "Neptune" => Color::srgb(0.3, 0.5, 0.9),
        "Moon" => Color::srgb(0.75, 0.75, 0.78),
        "Phobos" => Color::srgb(0.4, 0.3, 0.2),
        "Deimos" => Color::srgb(0.5, 0.4, 0.3),
        "Io" => Color::srgb(0.9, 0.8, 0.4),
        "Europa" => Color::srgb(0.8, 0.8, 0.9),
        "Ganymede" => Color::srgb(0.6, 0.6, 0.7),
        "Callisto" => Color::srgb(0.5, 0.5, 0.6),
        "Mimas" | "Iapetus" => Color::srgb(0.8, 0.8, 0.8),
        "Enceladus" => Color::srgb(0.9, 0.9, 0.9),
        "Tethys" | "Dione" | "Rhea" | "Ariel" => Color::srgb(0.7, 0.7, 0.8),
        "Titan" => Color::srgb(0.7, 0.6, 0.4),
        "Hyperion" => Color::srgb(0.6, 0.5, 0.4),
        "Miranda" | "Titania" | "Nereid" => Color::srgb(0.6, 0.6, 0.7),
        "Umbriel" => Color::srgb(0.4, 0.4, 0.5),
        "Oberon" | "Proteus" => Color::srgb(0.5, 0.5, 0.6),
        "Triton" => Color::srgb(0.6, 0.7, 0.8),
        "Larissa" => Color::srgb(0.5, 0.5, 0.5),
        _ => return None,
    };
    Some(color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::value_objects::planet_configs::PLANET_CONFIGS;

    #[test]
    fn every_configured_body_has_a_presentation_color() {
        assert!(PLANET_CONFIGS
            .iter()
            .all(|config| color_for_body(config.name).is_some()));
    }
}
