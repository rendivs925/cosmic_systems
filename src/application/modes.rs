/// Application run modes selectable from the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Solar,
    Craft,
    Rocket,
    Gyro,
}

impl Mode {
    /// Parse the run mode from command-line arguments.
    ///
    /// Recognizes the exact tokens `rocket`, `craft`, and `gyro` anywhere in the
    /// arguments (matching the previous `args.contains(...)` behavior) without
    /// treating other arguments as mode selectors. Unknown bare arguments are
    /// not treated as a mode selector; the mode falls back to the default
    /// solar-system simulation.
    pub fn from_args<I>(args: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let mut mode = Mode::Solar;
        let mut saw_unknown = false;
        for arg in args {
            match arg.as_str() {
                "rocket" => mode = Mode::Rocket,
                "craft" => mode = Mode::Craft,
                "gyro" => mode = Mode::Gyro,
                _ => {
                    if !arg.starts_with('-') {
                        saw_unknown = true;
                    }
                }
            }
        }
        if saw_unknown {
            bevy::log::warn!(
                "Unknown mode argument; falling back to the default solar-system simulation"
            );
        }
        mode
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

#[cfg(test)]
mod tests {
    use super::Mode;

    fn parse(args: &[&str]) -> Mode {
        Mode::from_args(args.iter().map(|s| s.to_string()))
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
}
