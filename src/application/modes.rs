/// Application run modes selectable from the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Solar,
    Craft,
    Rocket,
    Gyro,
}

/// Everything parsed from the command line: the run mode plus mode-specific
/// options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOptions {
    pub mode: Mode,
    /// `--vehicle <key>` selection; only meaningful in rocket mode.
    pub vehicle: Option<String>,
}

impl Mode {
    /// Parse the run mode from command-line arguments.
    ///
    /// Recognizes the exact tokens `rocket`, `craft`, and `gyro` anywhere in
    /// the arguments without treating other arguments as mode selectors.
    /// Unknown bare arguments are not treated as a mode selector; the mode
    /// falls back to the default solar-system simulation. `--vehicle <key>`
    /// consumes its value token so it is never mistaken for a mode argument.
    pub fn from_args<I>(args: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        parse_launch_options(args).mode
    }

    pub fn title(self) -> &'static str {
        match self {
            Mode::Solar => "Cosmic Systems Simulator",
            Mode::Craft => "Cosmic Systems - ZPE Craft",
            Mode::Rocket => "Cosmic Systems - Rocket Flight",
            Mode::Gyro => "Cosmic Systems - Gyro Propulsion",
        }
    }
}

/// Parse the full launch options: run mode plus `--vehicle <key>` selection.
/// A dangling `--vehicle` with no following value is warned about and
/// ignored; unknown bare arguments keep their existing warn-and-fallback
/// behavior so other subsystems' arguments stay harmless.
pub fn parse_launch_options<I>(args: I) -> LaunchOptions
where
    I: IntoIterator<Item = String>,
{
    let mut options = LaunchOptions {
        mode: Mode::Solar,
        vehicle: None,
    };
    let mut saw_unknown = false;
    let mut expect_vehicle_value = false;
    for arg in args {
        if expect_vehicle_value {
            options.vehicle = Some(arg);
            expect_vehicle_value = false;
            continue;
        }
        match arg.as_str() {
            "rocket" => options.mode = Mode::Rocket,
            "craft" => options.mode = Mode::Craft,
            "gyro" => options.mode = Mode::Gyro,
            "--vehicle" => expect_vehicle_value = true,
            _ => {
                if !arg.starts_with('-') {
                    saw_unknown = true;
                }
            }
        }
    }
    if expect_vehicle_value {
        bevy::log::warn!("--vehicle given without a value; using the default vehicle");
    }
    if saw_unknown {
        bevy::log::warn!(
            "Unknown mode argument; falling back to the default solar-system simulation"
        );
    }
    options
}

#[cfg(test)]
mod tests {
    use super::{parse_launch_options, LaunchOptions, Mode};

    fn parse(args: &[&str]) -> Mode {
        Mode::from_args(args.iter().map(|s| s.to_string()))
    }

    fn options(args: &[&str]) -> LaunchOptions {
        parse_launch_options(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn defaults_to_solar() {
        assert_eq!(parse(&[]), Mode::Solar);
    }

    #[test]
    fn recognizes_each_mode() {
        assert_eq!(parse(&["rocket"]), Mode::Rocket);
        assert_eq!(parse(&["craft"]), Mode::Craft);
        assert_eq!(parse(&["gyro"]), Mode::Gyro);
    }

    #[test]
    fn mode_token_position_is_irrelevant() {
        assert_eq!(parse(&["rocket", "--flag"]), Mode::Rocket);
        assert_eq!(parse(&["--flag", "craft"]), Mode::Craft);
        assert_eq!(parse(&["--flag", "gyro"]), Mode::Gyro);
    }

    #[test]
    fn unknown_positional_falls_back_to_solar() {
        assert_eq!(parse(&["unknown-mode"]), Mode::Solar);
    }

    #[test]
    fn no_substring_match() {
        assert_eq!(parse(&["rocket-sim"]), Mode::Solar);
        assert_eq!(parse(&["crafting"]), Mode::Solar);
        assert_eq!(parse(&["mycraftfile"]), Mode::Solar);
    }

    #[test]
    fn flag_arguments_are_not_modes() {
        assert_eq!(parse(&["--rocket"]), Mode::Solar);
        assert_eq!(parse(&["--craft"]), Mode::Solar);
    }

    #[test]
    fn vehicle_value_is_captured_and_not_a_mode() {
        let parsed = options(&["rocket", "--vehicle", "starship"]);
        assert_eq!(parsed.mode, Mode::Rocket);
        assert_eq!(parsed.vehicle.as_deref(), Some("starship"));

        // The value token must never be mistaken for an unknown mode.
        let parsed = options(&["--vehicle", "electron"]);
        assert_eq!(parsed.mode, Mode::Solar);
        assert_eq!(parsed.vehicle.as_deref(), Some("electron"));
    }

    #[test]
    fn vehicle_defaults_to_none() {
        let parsed = options(&["rocket"]);
        assert_eq!(parsed.mode, Mode::Rocket);
        assert!(parsed.vehicle.is_none());
    }

    #[test]
    fn dangling_vehicle_flag_is_ignored() {
        let parsed = options(&["rocket", "--vehicle"]);
        assert_eq!(parsed.mode, Mode::Rocket);
        assert!(parsed.vehicle.is_none());
    }
}
