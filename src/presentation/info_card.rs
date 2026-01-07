use bevy::prelude::*;
use super::components::*;
use crate::infrastructure::bevy_adapters::components::{PlanetComponent, SelectedPlanet, ZenMode};

pub(crate) fn update_info_card(
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
            info_line(
                &mut lines,
                "Mass",
                "1.9885 x 10^30 kg (333,000 Earth masses)",
            );
            info_line(&mut lines, "Radius", "696,340 km (109 Earth radii)");
            info_line(&mut lines, "Surface Temperature", "5,778 K (5,505 deg C)");
            info_line(&mut lines, "Luminosity", "3.83 x 10^26 W");
            info_line(&mut lines, "Age", "4.6 billion years");
            info_line(&mut lines, "Distance from Earth", "149.6 million km (1 AU)");
        }
        _ => {
            // For planets and moons, show orbital and physical data
            section_header(&mut lines, "Orbital Characteristics");
            info_line(&mut lines, "Semi-major Axis", &format!("{:.2} AU", planet.semi_major_axis_au));
            info_line(&mut lines, "Eccentricity", &format!("{:.4}", planet.eccentricity));
            info_line(&mut lines, "Inclination", &format!("{:.2}°", planet.inclination_deg));
            info_line(&mut lines, "Orbital Period", &format!("{:.1} Earth years", planet.orbital_period_years));

            if let Some(parent) = &planet.parent_entity {
                info_line(&mut lines, "Parent Body", parent);
            }

            section_header(&mut lines, "Physical Characteristics");
            info_line(&mut lines, "Radius", &format!("{:.0} km", planet.radius_km));
            info_line(&mut lines, "Mass", &format!("{:.2e} kg", planet.mass_kg));
            info_line(&mut lines, "Density", &format!("{:.1} g/cm³", planet.density_g_cm3));

            if planet.surface_gravity_m_s2 > 0.0 {
                info_line(&mut lines, "Surface Gravity", &format!("{:.1} m/s²", planet.surface_gravity_m_s2));
            }

            section_header(&mut lines, "Additional Information");
            info_line(&mut lines, "Discovery", get_discovery_info(name));
            info_line(&mut lines, "Exploration Status", get_exploration_status(name));

            let fun_facts = get_fun_facts(name);
            if !fun_facts.is_empty() {
                section_header(&mut lines, "Fun Facts");
                for fact in fun_facts {
                    lines.push(format!("• {}", fact));
                }
            }
        }
    }

    lines.join("\n")
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

fn get_discovery_info(name: &str) -> &'static str {
    match name {
        "Moon" => "Ancient times",
        "Mercury" | "Venus" | "Mars" | "Jupiter" | "Saturn" => "Ancient times",
        "Uranus" => "1781 (William Herschel)",
        "Neptune" => "1846 (Johann Galle)",
        "Phobos" | "Deimos" => "1877 (Asaph Hall)",
        "Io" | "Europa" | "Ganymede" | "Callisto" => "1610 (Galileo Galilei)",
        "Mimas" | "Enceladus" | "Tethys" | "Dione" | "Rhea" | "Titan" | "Iapetus" => "1671-1684 (Giovanni Cassini)",
        "Hyperion" => "1848 (William Cranch Bond)",
        "Miranda" | "Ariel" | "Umbriel" | "Titania" | "Oberon" => "1787 (William Herschel)",
        "Triton" => "1846 (William Lassell)",
        "Proteus" => "1989 (Voyager 2)",
        "Nereid" => "1949 (Gerard Kuiper)",
        "Larissa" => "1981 (Voyager 2)",
        _ => "Unknown",
    }
}

fn get_exploration_status(name: &str) -> &'static str {
    match name {
        "Moon" => "Landed (Apollo 11-17, Luna, Chang'e)",
        "Mercury" => "Orbited (MESSENGER)",
        "Venus" => "Landed (Venera, Vega)",
        "Mars" => "Orbited, Landed, Roved (Mars orbiters, rovers)",
        "Jupiter" => "Orbited (Galileo, Juno)",
        "Saturn" => "Orbited (Cassini)",
        "Uranus" => "Flew by (Voyager 2)",
        "Neptune" => "Flew by (Voyager 2)",
        "Sun" => "Orbited (Parker Solar Probe)",
        "Io" | "Europa" | "Ganymede" | "Callisto" => "Flew by (Galileo, Juno)",
        "Enceladus" | "Titan" => "Flew by (Cassini)",
        "Triton" => "Flew by (Voyager 2)",
        _ => "Not explored",
    }
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
            "Mercury has no atmosphere and extreme temperature swings".to_string(),
        ],
        "Venus" => vec![
            "Venus rotates backwards compared to most planets".to_string(),
            "It's the hottest planet despite not being closest to the Sun".to_string(),
            "A day on Venus is longer than its year".to_string(),
        ],
        "Earth" => vec![
            "Earth is the only known planet with life".to_string(),
            "71% of Earth's surface is covered by water".to_string(),
            "Earth has a powerful magnetic field protecting it from solar wind".to_string(),
        ],
        "Mars" => vec![
            "Mars has the largest volcano in the solar system (Olympus Mons)".to_string(),
            "A day on Mars is very similar to Earth (24h 37m)".to_string(),
            "Mars has polar ice caps made of water and carbon dioxide".to_string(),
        ],
        "Jupiter" => vec![
            "Jupiter is larger than all other planets combined".to_string(),
            "It has a Great Red Spot - a storm larger than Earth".to_string(),
            "Jupiter acts as a cosmic vacuum cleaner, protecting inner planets".to_string(),
        ],
        "Saturn" => vec![
            "Saturn has the most spectacular ring system".to_string(),
            "It's less dense than water - it would float!".to_string(),
            "Saturn radiates more heat than it receives from the Sun".to_string(),
        ],
        "Uranus" => vec![
            "Uranus rotates on its side (98° axial tilt)".to_string(),
            "It appears blue-green due to methane in its atmosphere".to_string(),
            "Uranus has faint rings discovered in 1977".to_string(),
        ],
        "Neptune" => vec![
            "Neptune has the strongest winds in the solar system".to_string(),
            "It's the farthest planet from the Sun".to_string(),
            "Neptune was predicted mathematically before being observed".to_string(),
        ],
        "Moon" => vec![
            "The Moon is tidally locked to Earth".to_string(),
            "It has no atmosphere and extreme temperature swings".to_string(),
            "The Moon's gravity causes Earth's tides".to_string(),
        ],
        "Io" => vec![
            "Io is the most volcanically active body in the solar system".to_string(),
            "Its surface is constantly being resurfaced by volcanic activity".to_string(),
            "Io has over 400 active volcanoes".to_string(),
        ],
        "Europa" => vec![
            "Europa likely has a subsurface ocean beneath its icy crust".to_string(),
            "It has the smoothest surface of any solid body in the solar system".to_string(),
            "Europa is a candidate for extraterrestrial life".to_string(),
        ],
        "Titan" => vec![
            "Titan is larger than Mercury and has a thick atmosphere".to_string(),
            "It has lakes and rivers of liquid methane and ethane".to_string(),
            "Titan is the only moon known to have a substantial atmosphere".to_string(),
        ],
        "Enceladus" => vec![
            "Enceladus has geysers spraying water vapor into space".to_string(),
            "It has a subsurface ocean that may harbor life".to_string(),
            "Its south pole has 'tiger stripe' fractures".to_string(),
        ],
        _ => vec![],
    }
}