use bevy::prelude::*;

#[derive(Clone, Debug)]
pub struct Planet {
    pub name: String,
    pub radius_km: f32,
    pub mass_kg: f64,
    pub color: Color,
    pub orbital_distance_au: f32, // Average distance from Sun (or parent planet) in AU
    pub orbital_period_days: f32,
    pub rotation_period_hours: f32,
    pub axial_tilt_deg: f32,
    pub parent_entity: Option<String>, // Name of parent body (None for Sun, planet name for moons)
}

impl Planet {
    pub fn new(
        name: String,
        radius_km: f32,
        mass_kg: f64,
        color: Color,
        orbital_distance_au: f32,
        orbital_period_days: f32,
        rotation_period_hours: f32,
        axial_tilt_deg: f32,
        parent_entity: Option<String>,
    ) -> Self {
        Self {
            name,
            radius_km,
            mass_kg,
            color,
            orbital_distance_au,
            orbital_period_days,
            rotation_period_hours,
            axial_tilt_deg,
            parent_entity,
        }
    }

    // Convenience method to create planets with data (using accurate astronomical radii)
    pub fn create_mercury() -> Self {
        Self::new(
            "Mercury".to_string(),
            2439.7, // Accurate radius in km
            3.3011e23,
            Color::srgb(0.45, 0.45, 0.42), // Realistic gray-brown Mercury surface
            0.387,  // Semi-major axis in AU
            87.969, // Orbital period in days
            1407.504, // 58.646 days
            0.034,
            None, // orbits Sun
        )
    }

    pub fn create_venus() -> Self {
        Self::new(
            "Venus".to_string(),
            6051.8, // Accurate radius in km
            4.8675e24,
            Color::srgb(0.95, 0.85, 0.65), // Realistic Venus yellowish-white clouds
            0.723,   // Semi-major axis in AU
            224.701, // Orbital period in days
            -5832.6, // Retrograde rotation (243.025 days)
            177.36,  // Retrograde axial tilt
            None, // orbits Sun
        )
    }

    pub fn create_earth() -> Self {
        Self::new(
            "Earth".to_string(),
            6371.0, // Accurate radius in km
            5.97237e24,
            Color::srgb(0.25, 0.45, 0.85), // Realistic Earth: deep blue oceans with some continental green
            1.000,   // Semi-major axis in AU
            365.256, // Orbital period in days
            23.934,
            23.439,
            None, // orbits Sun
        )
    }

    pub fn create_mars() -> Self {
        Self::new(
            "Mars".to_string(),
            3389.5, // Accurate radius in km
            6.4171e23,
            Color::srgb(0.85, 0.35, 0.15), // Realistic Mars: butterscotch-orange with iron oxide dust
            1.524,    // Semi-major axis in AU
            686.980,  // Orbital period in days
            24.623,
            25.19,
            None, // orbits Sun
        )
    }

    pub fn create_jupiter() -> Self {
        Self::new(
            "Jupiter".to_string(),
            69911.0, // Accurate radius in km
            1.8982e27,
            Color::srgb(0.85, 0.65, 0.45), // Realistic Jupiter: beige with brown/white zonal bands
            5.204,    // Semi-major axis in AU
            4332.667, // 11.862 years
            9.925,
            3.13,
            None, // orbits Sun
        )
    }

    pub fn create_saturn() -> Self {
        Self::new(
            "Saturn".to_string(),
            58232.0, // Accurate radius in km
            5.6834e26,
            Color::srgb(0.9, 0.8, 0.5), // golden rings
            9.582,    // Semi-major axis in AU
            10759.346, // 29.457 years
            10.7,
            26.73,
            None, // orbits Sun
        )
    }

    pub fn create_uranus() -> Self {
        Self::new(
            "Uranus".to_string(),
            25362.0, // Accurate radius in km
            8.6810e25,
            Color::srgb(0.6, 0.8, 0.9), // pale cyan
            19.201,   // Semi-major axis in AU
            30688.992, // 84.0205 years
            -17.24,
            97.77,
            None, // orbits Sun
        )
    }

    pub fn create_neptune() -> Self {
        Self::new(
            "Neptune".to_string(),
            24622.0, // Accurate radius in km
            1.02413e26,
            Color::srgb(0.3, 0.5, 0.9), // deep azure
            30.047,   // Semi-major axis in AU
            60194.189, // 164.8 years
            16.11,
            28.32,
            None, // orbits Sun
        )
    }

    pub fn create_sun() -> Self {
        Self::new(
            "Sun".to_string(),
            696340.0, // Mean radius in km
            1.9885e30,
            Color::srgb(1.0, 1.0, 0.9), // bright yellowish-white
            0.0,                        // Sun doesn't orbit
            0.0,
            601.2, // 25.05 days sidereal rotation
            7.25,
            None,   // central body
        )
    }

    // Moon creation methods
    pub fn create_moon() -> Self {
        Self::new(
            "Moon".to_string(),
            1737.4, // Earth's moon
            7.342e22,
            Color::srgb(0.75, 0.75, 0.78), // Realistic Moon: bright gray with slight blue tint from space weathering
            0.002569,  // 384,400 km
            27.3217,   // sidereal month
            27.3217 * 24.0, // synchronous rotation
            0.0,
            Some("Earth".to_string()),
        )
    }

    pub fn create_phobos() -> Self {
        Self::new(
            "Phobos".to_string(),
            11.1, // Mars' moon
            1.06e16,
            Color::srgb(0.4, 0.3, 0.2), // dark gray
            0.000032,                   // Accurate: ~9,400 km from Mars center
            0.32,                       // very fast orbit (7.6 hours)
            0.32 * 24.0,                // synchronous rotation
            0.0,
            Some("Mars".to_string()),
        )
    }

    pub fn create_deimos() -> Self {
        Self::new(
            "Deimos".to_string(),
            6.2, // Mars' smaller moon
            1.48e15,
            Color::srgb(0.5, 0.4, 0.3), // gray
            0.000156,                   // Accurate: ~23,500 km from Mars center
            1.26,                       // slower orbit (30.3 hours)
            1.26 * 24.0,                // synchronous rotation
            0.0,
            Some("Mars".to_string()),
        )
    }

    pub fn create_io() -> Self {
        Self::new(
            "Io".to_string(),
            1821.6, // Jupiter's moon
            8.93e22,
            Color::srgb(0.9, 0.8, 0.4), // yellowish sulfur
            0.0028,                     // Accurate: 421,700 km from Jupiter
            1.77,                       // 42.5 hours
            1.77 * 24.0,                // synchronous rotation
            0.0,
            Some("Jupiter".to_string()),
        )
    }

    pub fn create_europa() -> Self {
        Self::new(
            "Europa".to_string(),
            1560.8, // Jupiter's moon
            4.8e22,
            Color::srgb(0.8, 0.8, 0.9), // icy blue-white
            0.0045,                     // Accurate: 670,900 km from Jupiter
            3.55,                       // 85.2 hours
            3.55 * 24.0,                // synchronous rotation
            0.0,
            Some("Jupiter".to_string()),
        )
    }

    pub fn create_ganymede() -> Self {
        Self::new(
            "Ganymede".to_string(),
            2634.1, // Jupiter's largest moon
            1.48e23,
            Color::srgb(0.6, 0.6, 0.7), // gray icy
            0.0072,                     // Accurate: 1,070,400 km from Jupiter
            7.15,                       // 171.7 hours
            7.15 * 24.0,                // synchronous rotation
            0.0,
            Some("Jupiter".to_string()),
        )
    }

    pub fn create_callisto() -> Self {
        Self::new(
            "Callisto".to_string(),
            2410.3, // Jupiter's moon
            1.08e23,
            Color::srgb(0.5, 0.5, 0.6), // dark icy
            0.0126,                     // Accurate: 1,882,700 km from Jupiter
            16.69,                      // 401.4 hours
            16.69 * 24.0,               // synchronous rotation
            0.0,
            Some("Jupiter".to_string()),
        )
    }

    pub fn create_mimas() -> Self {
        Self::new(
            "Mimas".to_string(),
            198.2, // Saturn's moon
            3.75e19,
            Color::srgb(0.8, 0.8, 0.8), // icy
            0.0012,                     // Accurate: 185,539 km from Saturn
            0.94,                       // 22.6 hours
            0.94 * 24.0,                // synchronous rotation
            0.0,
            Some("Saturn".to_string()),
        )
    }

    pub fn create_enceladus() -> Self {
        Self::new(
            "Enceladus".to_string(),
            252.1, // Saturn's moon
            1.08e20,
            Color::srgb(0.9, 0.9, 0.9), // very bright icy
            0.0016,                     // Accurate: 237,948 km from Saturn
            1.37,                       // 32.9 hours
            1.37 * 24.0,                // synchronous rotation
            0.0,
            Some("Saturn".to_string()),
        )
    }

    pub fn create_tethys() -> Self {
        Self::new(
            "Tethys".to_string(),
            531.1, // Saturn's moon
            6.18e20,
            Color::srgb(0.7, 0.7, 0.8), // icy
            0.0020,                     // Accurate: 294,672 km from Saturn
            1.89,                       // 45.3 hours
            1.89 * 24.0,                // synchronous rotation
            0.0,
            Some("Saturn".to_string()),
        )
    }

    pub fn create_dione() -> Self {
        Self::new(
            "Dione".to_string(),
            561.4, // Saturn's moon
            1.1e21,
            Color::srgb(0.7, 0.7, 0.8), // icy
            0.0025,                     // Accurate: 377,415 km from Saturn
            2.74,                       // 65.7 hours
            2.74 * 24.0,                // synchronous rotation
            0.0,
            Some("Saturn".to_string()),
        )
    }

    pub fn create_rhea() -> Self {
        Self::new(
            "Rhea".to_string(),
            763.8, // Saturn's moon
            2.31e21,
            Color::srgb(0.7, 0.7, 0.8), // icy
            0.0035,                     // Accurate: 527,108 km from Saturn
            4.52,                       // 108.4 hours
            4.52 * 24.0,                // synchronous rotation
            0.0,
            Some("Saturn".to_string()),
        )
    }

    pub fn create_titan() -> Self {
        Self::new(
            "Titan".to_string(),
            2574.7, // Saturn's largest moon
            1.35e23,
            Color::srgb(0.7, 0.6, 0.4), // orange atmosphere
            0.0082,                     // Accurate: 1,221,870 km from Saturn
            15.95,                      // 383.9 hours
            15.95 * 24.0,               // synchronous rotation
            0.0,
            Some("Saturn".to_string()),
        )
    }

    pub fn create_hyperion() -> Self {
        Self::new(
            "Hyperion".to_string(),
            135.0, // Saturn's moon
            5.6e18,
            Color::srgb(0.6, 0.5, 0.4), // dark porous
            0.0099,                     // Accurate: 1,481,009 km from Saturn
            21.28,                      // 511.0 hours
            21.28 * 24.0,               // synchronous rotation
            0.0,
            Some("Saturn".to_string()),
        )
    }

    pub fn create_iapetus() -> Self {
        Self::new(
            "Iapetus".to_string(),
            734.5, // Saturn's moon
            1.81e21,
            Color::srgb(0.8, 0.8, 0.8), // icy with dark hemisphere
            0.0238,                     // Accurate: 3,561,300 km from Saturn
            79.32,                      // 1903.7 hours
            79.32 * 24.0,               // synchronous rotation
            0.0,
            Some("Saturn".to_string()),
        )
    }

    pub fn create_miranda() -> Self {
        Self::new(
            "Miranda".to_string(),
            235.8, // Uranus' moon
            6.41e19,
            Color::srgb(0.6, 0.6, 0.7), // icy
            0.0008,                     // Accurate: 129,783 km from Uranus
            1.41,                       // 33.9 hours
            1.41 * 24.0,                // synchronous rotation
            0.0,
            Some("Uranus".to_string()),
        )
    }

    pub fn create_ariel() -> Self {
        Self::new(
            "Ariel".to_string(),
            578.9, // Uranus' moon
            1.25e21,
            Color::srgb(0.7, 0.7, 0.8), // bright icy
            0.0012,                     // Accurate: 191,020 km from Uranus
            2.52,                       // 60.5 hours
            2.52 * 24.0,                // synchronous rotation
            0.0,
            Some("Uranus".to_string()),
        )
    }

    pub fn create_umbriel() -> Self {
        Self::new(
            "Umbriel".to_string(),
            584.7, // Uranus' moon
            1.28e21,
            Color::srgb(0.4, 0.4, 0.5), // very dark icy
            0.0018,                     // Accurate: 266,000 km from Uranus
            4.14,                       // 99.4 hours
            4.14 * 24.0,                // synchronous rotation
            0.0,
            Some("Uranus".to_string()),
        )
    }

    pub fn create_titania() -> Self {
        Self::new(
            "Titania".to_string(),
            788.4, // Uranus' moon
            3.53e21,
            Color::srgb(0.6, 0.6, 0.7), // icy
            0.0029,                     // Accurate: 436,300 km from Uranus
            8.71,                       // 208.9 hours
            8.71 * 24.0,                // synchronous rotation
            0.0,
            Some("Uranus".to_string()),
        )
    }

    pub fn create_oberon() -> Self {
        Self::new(
            "Oberon".to_string(),
            761.4, // Uranus' moon
            3.01e21,
            Color::srgb(0.5, 0.5, 0.6), // dark icy
            0.0039,                     // Accurate: 583,519 km from Uranus
            13.46,                      // 323.1 hours
            13.46 * 24.0,               // synchronous rotation
            0.0,
            Some("Uranus".to_string()),
        )
    }

    pub fn create_triton() -> Self {
        Self::new(
            "Triton".to_string(),
            1353.4, // Neptune's moon
            2.14e22,
            Color::srgb(0.6, 0.7, 0.8), // icy with nitrogen
            0.0024,                     // Accurate: 354,759 km from Neptune
            5.88,                       // 141.0 hours
            5.88 * 24.0,                // synchronous rotation
            0.0,
            Some("Neptune".to_string()),
        )
    }

    pub fn create_proteus() -> Self {
        Self::new(
            "Proteus".to_string(),
            210.0, // Neptune's moon
            4.4e19,
            Color::srgb(0.5, 0.5, 0.6), // dark icy
            0.0008,                     // Close to Neptune
            1.12,                       // 26.9 hours
            1.12 * 24.0,                // synchronous rotation
            0.0,
            Some("Neptune".to_string()),
        )
    }

    pub fn create_nereid() -> Self {
        Self::new(
            "Nereid".to_string(),
            170.0, // Neptune's moon
            3.1e19,
            Color::srgb(0.6, 0.6, 0.7), // icy
            0.0369,                     // Distant orbit: ~5.5 million km from Neptune
            360.13,                     // Very long orbital period
            360.13 * 24.0,              // synchronous rotation
            0.0,
            Some("Neptune".to_string()),
        )
    }

    pub fn create_larissa() -> Self {
        Self::new(
            "Larissa".to_string(),
            97.0, // Neptune's moon
            4.2e17,
            Color::srgb(0.5, 0.5, 0.5), // gray
            0.0005,                     // Close to Neptune
            0.55,                       // 13.2 hours
            0.55 * 24.0,                // synchronous rotation
            0.0,
            Some("Neptune".to_string()),
        )
    }

    pub fn create_charon() -> Self {
        Self::new(
            "Charon".to_string(),
            603.6, // Pluto's moon
            1.59e21,
            Color::srgb(0.5, 0.5, 0.5), // gray icy
            1.0,                        // simplified orbit around Pluto
            6.39,                       // 153.3 hours
            6.39 * 24.0,                // synchronous rotation
            0.0,
            Some("Pluto".to_string()),  // Note: Pluto not in main solar system yet
        )
    }
}
