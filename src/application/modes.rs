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

/// Invalid command-line input. Launch selection is explicit; unknown arguments
/// never silently select a different simulation mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchOptionError {
    MissingVehicleValue,
    UnknownArgument(String),
}

impl std::fmt::Display for LaunchOptionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVehicleValue => formatter.write_str("--vehicle requires a vehicle key"),
            Self::UnknownArgument(argument) => {
                write!(formatter, "unknown launch argument '{argument}'")
            }
        }
    }
}

impl std::error::Error for LaunchOptionError {}

impl Mode {
    /// Parse the run mode from command-line arguments.
    ///
    /// Recognizes the exact tokens `rocket`, `craft`, and `gyro` anywhere in
    /// the arguments without treating other arguments as mode selectors.
    /// `--vehicle <key>` consumes its value token so it is never mistaken for a
    /// mode argument. Invalid arguments return a typed error.
    pub fn from_args<I>(args: I) -> Result<Self, LaunchOptionError>
    where
        I: IntoIterator<Item = String>,
    {
        parse_launch_options(args).map(|options| options.mode)
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
/// Every supplied argument must be one of the documented mode tokens or the
/// `--vehicle <key>` option; invalid input fails instead of changing mode.
pub fn parse_launch_options<I>(args: I) -> Result<LaunchOptions, LaunchOptionError>
where
    I: IntoIterator<Item = String>,
{
    let mut options = LaunchOptions {
        mode: Mode::Solar,
        vehicle: None,
    };
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
            _ => return Err(LaunchOptionError::UnknownArgument(arg)),
        }
    }
    if expect_vehicle_value {
        return Err(LaunchOptionError::MissingVehicleValue);
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::{parse_launch_options, LaunchOptionError, LaunchOptions, Mode};

    fn parse(args: &[&str]) -> Mode {
        Mode::from_args(args.iter().map(|s| s.to_string())).expect("valid launch options")
    }

    fn options(args: &[&str]) -> LaunchOptions {
        parse_launch_options(args.iter().map(|s| s.to_string())).expect("valid launch options")
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
    fn unknown_positional_is_rejected() {
        assert_eq!(
            parse_launch_options(["unknown-mode".to_string()]),
            Err(LaunchOptionError::UnknownArgument("unknown-mode".into()))
        );
    }

    #[test]
    fn unrecognized_mode_like_tokens_are_rejected() {
        for argument in ["rocket-sim", "crafting", "mycraftfile"] {
            assert_eq!(
                parse_launch_options([argument.to_string()]),
                Err(LaunchOptionError::UnknownArgument(argument.into()))
            );
        }
    }

    #[test]
    fn unknown_flags_are_rejected() {
        for argument in ["--rocket", "--craft"] {
            assert_eq!(
                parse_launch_options([argument.to_string()]),
                Err(LaunchOptionError::UnknownArgument(argument.into()))
            );
        }
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
    fn dangling_vehicle_flag_is_rejected() {
        assert_eq!(
            parse_launch_options(["rocket".to_string(), "--vehicle".to_string()]),
            Err(LaunchOptionError::MissingVehicleValue)
        );
    }
}
