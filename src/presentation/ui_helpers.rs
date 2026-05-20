use bevy::prelude::*;

use crate::infrastructure::bevy_adapters::components::NotificationType;
use crate::presentation::ui_components::*;

pub fn text_style(font_size: f32, color: Color) -> (TextFont, TextColor) {
    (
        TextFont {
            font_size,
            ..default()
        },
        TextColor(color),
    )
}

pub fn spawn_menu_button(parent: &mut ChildSpawnerCommands, label: &str, action: MenuAction, primary: bool) {
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

pub fn spawn_nav_button(parent: &mut ChildSpawnerCommands, name: &str, group: NavGroup) {
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
            NavButton {
                name: name.to_string(),
                group,
            },
            UiCapture,
        ))
        .with_children(|button| {
            let (font, color) = text_style(9.0, Color::srgb(0.75, 0.8, 0.85));
            button.spawn((Text::new(name), font, color, NavButtonLabel));
        });
}

pub fn info_body_text(text: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(text),
        TextFont {
            font_size: 9.5,
            ..default()
        },
        TextColor(Color::srgb(0.82, 0.85, 0.88)),
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
        (Color::srgb(0.16, 0.24, 0.39), Color::srgb(0.31, 0.47, 0.78))
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

pub fn fps_color(fps: f32) -> Color {
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

pub fn notification_color(notification_type: &NotificationType) -> Color {
    match notification_type {
        NotificationType::Success => Color::srgba(0.06, 0.25, 0.06, 0.86),
        NotificationType::Error => Color::srgba(0.25, 0.06, 0.06, 0.86),
        NotificationType::Info => Color::srgba(0.06, 0.13, 0.25, 0.86),
        NotificationType::Warning => Color::srgba(0.25, 0.19, 0.06, 0.86),
    }
}

pub fn get_celestial_type(name: &str) -> &'static str {
    match name {
        "Sun" => "G-type Main Sequence Star",
        "Mercury" | "Venus" | "Earth" | "Mars" => "Terrestrial Planet",
        "Jupiter" | "Saturn" => "Gas Giant",
        "Uranus" | "Neptune" => "Ice Giant",
        _ => "Natural Satellite",
    }
}

pub fn build_info_body(planet: &crate::domain::entities::planet::Planet) -> String {
    let mut lines = Vec::new();
    let name = planet.name.as_str();

    match name {
        "Sun" => {
            info_line(&mut lines, "Type", "G2V main-sequence star");
            info_line(&mut lines, "Mass", "1.989 x 10^30 kg (333,000 Earth)");
            info_line(&mut lines, "Radius", "696,340 km (109 Earth)");
            info_line(&mut lines, "Surface", "5,778 K");
            info_line(&mut lines, "Luminosity", "3.83 x 10^26 W");
            info_line(&mut lines, "Age", "4.6 billion years");
            lines.push(String::new());
            lines.push("• 99.86% of solar system mass".to_string());
            lines.push("• Core reaches 15 million deg C".to_string());
            lines.push(format!("• {}", get_exploration_status(name)));
        }
        "Mercury" => {
            info_line(&mut lines, "Type", "Terrestrial planet");
            info_line(&mut lines, "Mass", "3.301 x 10^23 kg (0.055 Earth)");
            info_line(&mut lines, "Radius", "2,440 km (0.383 Earth)");
            info_line(&mut lines, "Distance", "0.387 AU");
            info_line(&mut lines, "Orbit", "87.97 days");
            info_line(&mut lines, "Rotation", "58.65 days");
            info_line(&mut lines, "Temp", "-173 to 427 deg C");
            lines.push(String::new());
            lines.push("• Most eccentric orbit of all planets".to_string());
            lines.push("• A day is longer than its year".to_string());
            lines.push(format!("• {}", get_exploration_status(name)));
        }
        "Venus" => {
            info_line(&mut lines, "Type", "Terrestrial planet");
            info_line(&mut lines, "Mass", "4.867 x 10^24 kg (0.815 Earth)");
            info_line(&mut lines, "Radius", "6,052 km (0.949 Earth)");
            info_line(&mut lines, "Distance", "0.723 AU");
            info_line(&mut lines, "Orbit", "224.7 days");
            info_line(&mut lines, "Rotation", "243 days (retrograde)");
            info_line(&mut lines, "Temp", "462 deg C (hottest)");
            lines.push(String::new());
            lines.push("• Rotates backwards (retrograde)".to_string());
            lines.push("• Surface pressure 92x Earth's".to_string());
            lines.push(format!("• {}", get_exploration_status(name)));
        }
        "Earth" => {
            info_line(&mut lines, "Type", "Terrestrial planet");
            info_line(&mut lines, "Mass", "5.972 x 10^24 kg");
            info_line(&mut lines, "Radius", "6,371 km");
            info_line(&mut lines, "Distance", "1.000 AU");
            info_line(&mut lines, "Orbit", "365.26 days");
            info_line(&mut lines, "Rotation", "23.93 hours");
            info_line(&mut lines, "Temp", "-89 to 58 deg C");
            info_line(&mut lines, "Atmosphere", "78% N2, 21% O2");
            lines.push(String::new());
            lines.push("• Only known planet with liquid surface water".to_string());
            lines.push("• Powerful magnetic field protects life".to_string());
            lines.push(format!("• {}", get_exploration_status(name)));
        }
        "Mars" => {
            info_line(&mut lines, "Type", "Terrestrial planet");
            info_line(&mut lines, "Mass", "6.417 x 10^23 kg (0.107 Earth)");
            info_line(&mut lines, "Radius", "3,390 km (0.532 Earth)");
            info_line(&mut lines, "Distance", "1.524 AU");
            info_line(&mut lines, "Orbit", "686.98 days");
            info_line(&mut lines, "Rotation", "24.62 hours");
            info_line(&mut lines, "Temp", "-87 to -5 deg C");
            info_line(&mut lines, "Moons", "2 (Phobos, Deimos)");
            lines.push(String::new());
            lines.push("• Olympus Mons — tallest volcano in solar system".to_string());
            lines.push("• Valles Marineris — largest canyon".to_string());
            lines.push(format!("• {}", get_exploration_status(name)));
        }
        "Jupiter" => {
            info_line(&mut lines, "Type", "Gas giant");
            info_line(&mut lines, "Mass", "1.898 x 10^27 kg (318 Earth)");
            info_line(&mut lines, "Radius", "69,911 km (10.97 Earth)");
            info_line(&mut lines, "Distance", "5.204 AU");
            info_line(&mut lines, "Orbit", "11.86 years");
            info_line(&mut lines, "Rotation", "9.93 hours");
            info_line(&mut lines, "Moons", "95+ known");
            lines.push(String::new());
            lines.push("• Great Red Spot is a storm larger than Earth".to_string());
            lines.push("• Emits more heat than it receives from Sun".to_string());
            lines.push(format!("• {}", get_exploration_status(name)));
        }
        "Saturn" => {
            info_line(&mut lines, "Type", "Gas giant");
            info_line(&mut lines, "Mass", "5.683 x 10^26 kg (95.2 Earth)");
            info_line(&mut lines, "Radius", "58,232 km (9.14 Earth)");
            info_line(&mut lines, "Distance", "9.582 AU");
            info_line(&mut lines, "Orbit", "29.46 years");
            info_line(&mut lines, "Rotation", "10.7 hours");
            info_line(&mut lines, "Moons", "146+ known");
            lines.push(String::new());
            lines.push("• Rings span 282,000 km but are only ~10 m thick".to_string());
            lines.push("• Least dense planet — would float in water".to_string());
            lines.push(format!("• {}", get_exploration_status(name)));
        }
        "Uranus" => {
            info_line(&mut lines, "Type", "Ice giant");
            info_line(&mut lines, "Mass", "8.681 x 10^25 kg (14.5 Earth)");
            info_line(&mut lines, "Radius", "25,362 km (4.01 Earth)");
            info_line(&mut lines, "Distance", "19.20 AU");
            info_line(&mut lines, "Orbit", "84.02 years");
            info_line(&mut lines, "Rotation", "17.24 h (retrograde)");
            info_line(&mut lines, "Tilt", "97.77 deg (on its side)");
            lines.push(String::new());
            lines.push("• Rotates on its side — likely from massive collision".to_string());
            lines.push("• Coldest planetary atmosphere".to_string());
            lines.push(format!("• {}", get_exploration_status(name)));
        }
        "Neptune" => {
            info_line(&mut lines, "Type", "Ice giant");
            info_line(&mut lines, "Mass", "1.024 x 10^26 kg (17.1 Earth)");
            info_line(&mut lines, "Radius", "24,622 km (3.88 Earth)");
            info_line(&mut lines, "Distance", "30.05 AU");
            info_line(&mut lines, "Orbit", "164.8 years");
            info_line(&mut lines, "Rotation", "16.11 hours");
            info_line(&mut lines, "Moons", "14 known (major: Triton)");
            lines.push(String::new());
            lines.push("• Fastest winds in the solar system".to_string());
            lines.push("• Predicted mathematically before discovery".to_string());
            lines.push(format!("• {}", get_exploration_status(name)));
        }
        _ => {
            let parent = get_parent_body(name);
            info_line(&mut lines, "Type", "Natural satellite");
            info_line(&mut lines, "Parent", parent);
            info_line(&mut lines, "Discovery", get_discovery_info(name));
            lines.push(String::new());
            let facts = get_fun_facts(name);
            for fact in facts.iter().take(2) {
                lines.push(format!("• {}", fact));
            }
            lines.push(format!("• {}", get_exploration_status(name)));
        }
    }

    lines.join(
        "
",
    )
}

pub fn section_header(lines: &mut Vec<String>, title: &str) {
    if !lines.is_empty() {
        lines.push(String::new());
    }
    lines.push(title.to_string());
}

pub fn info_line(lines: &mut Vec<String>, label: &str, value: &str) {
    lines.push(format!("{}: {}", label, value));
}

pub fn get_fun_facts(name: &str) -> Vec<String> {
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
        _ => vec![format!(
            "{} has unique and fascinating characteristics",
            name
        )],
    }
}

pub fn get_exploration_status(name: &str) -> &'static str {
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

pub fn get_parent_body(name: &str) -> &'static str {
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

pub fn get_discovery_info(name: &str) -> &'static str {
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

pub fn planet_names() -> [&'static str; 9] {
    [
        "Sun", "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
    ]
}

pub fn moon_list() -> [&'static str; 24] {
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

pub fn moon_pairs() -> impl Iterator<Item = (&'static str, &'static str)> {
    moon_list()
        .into_iter()
        .map(|name| (get_parent_body(name), name))
}

pub fn is_primary_body(name: &str) -> bool {
    planet_names().contains(&name)
}