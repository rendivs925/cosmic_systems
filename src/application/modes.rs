use std::error::Error;
use std::fmt::{Display, Formatter, Result as FormatterResult};

use crate::domain::services::gravity::ForceModelTier;
use crate::domain::value_objects::launch_site_coordinates::PredefinedLaunchSite;
use crate::domain::value_objects::recovery_deck::RecoveryDeckCoordinates;

/// Application run modes selectable from the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Solar,
    Craft,
    Rocket,
}

/// Everything parsed from the command line: the run mode plus mode-specific
/// options.
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchOptions {
    pub mode: Mode,
    /// `--vehicle <key>` selection; only meaningful in rocket mode.
    pub vehicle: Option<String>,
    /// `--site <id>` selection; only meaningful in rocket mode.
    pub site: Option<PredefinedLaunchSite>,
    /// `--recovery-deck <lat>,<lon>` sea-surface recovery deck, terrain-radial
    /// degrees; only meaningful in rocket mode. Supplying one selects the
    /// downrange recovery profile.
    pub recovery_deck: Option<RecoveryDeckCoordinates>,
    /// `--force-model <tier>` selection; only meaningful in rocket mode.
    pub force_model: Option<ForceModelTier>,
}

/// Invalid command-line input. Launch selection is explicit; unknown arguments
/// never silently select a different simulation mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchOptionError {
    MissingVehicleValue,
    MissingSiteValue,
    MissingRecoveryDeckValue,
    MissingForceModelValue,
    UnknownSite(String),
    InvalidRecoveryDeck(String),
    UnknownForceModel(String),
    UnknownArgument(String),
}

impl Display for LaunchOptionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FormatterResult {
        match self {
            Self::MissingVehicleValue => formatter.write_str("--vehicle requires a vehicle key"),
            Self::MissingSiteValue => formatter.write_str("--site requires a launch-site id"),
            Self::MissingRecoveryDeckValue => {
                formatter.write_str("--recovery-deck requires <latitude>,<longitude>")
            }
            Self::MissingForceModelValue => {
                formatter.write_str("--force-model requires a force-model key")
            }
            Self::UnknownSite(site) => write!(formatter, "unknown launch site '{site}'"),
            Self::InvalidRecoveryDeck(value) => write!(
                formatter,
                "invalid recovery deck '{value}': expected <latitude>,<longitude> within range"
            ),
            Self::UnknownForceModel(tier) => write!(formatter, "unknown force model '{tier}'"),
            Self::UnknownArgument(argument) => {
                write!(formatter, "unknown launch argument '{argument}'")
            }
        }
    }
}

impl Error for LaunchOptionError {}

impl Mode {
    /// Parse the run mode from command-line arguments.
    ///
    /// Recognizes the exact tokens `rocket` and `craft` anywhere in
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
        site: None,
        recovery_deck: None,
        force_model: None,
    };
    let mut expect_vehicle_value = false;
    let mut expect_site_value = false;
    let mut expect_recovery_deck_value = false;
    let mut expect_force_model_value = false;
    for arg in args {
        if expect_vehicle_value {
            options.vehicle = Some(arg);
            expect_vehicle_value = false;
            continue;
        }
        if expect_site_value {
            let Some(site) = PredefinedLaunchSite::parse(&arg) else {
                return Err(LaunchOptionError::UnknownSite(arg));
            };
            options.site = Some(site);
            expect_site_value = false;
            continue;
        }
        if expect_force_model_value {
            let Some(tier) = ForceModelTier::from_key(&arg) else {
                return Err(LaunchOptionError::UnknownForceModel(arg));
            };
            options.force_model = Some(tier);
            expect_force_model_value = false;
            continue;
        }
        if expect_recovery_deck_value {
            let Some(deck) = parse_recovery_deck(&arg) else {
                return Err(LaunchOptionError::InvalidRecoveryDeck(arg));
            };
            options.recovery_deck = Some(deck);
            expect_recovery_deck_value = false;
            continue;
        }
        match arg.as_str() {
            "rocket" => options.mode = Mode::Rocket,
            "craft" => options.mode = Mode::Craft,
            "--vehicle" => expect_vehicle_value = true,
            "--site" => expect_site_value = true,
            "--recovery-deck" => expect_recovery_deck_value = true,
            "--force-model" => expect_force_model_value = true,
            _ => return Err(LaunchOptionError::UnknownArgument(arg)),
        }
    }
    if expect_vehicle_value {
        return Err(LaunchOptionError::MissingVehicleValue);
    }
    if expect_site_value {
        return Err(LaunchOptionError::MissingSiteValue);
    }
    if expect_recovery_deck_value {
        return Err(LaunchOptionError::MissingRecoveryDeckValue);
    }
    if expect_force_model_value {
        return Err(LaunchOptionError::MissingForceModelValue);
    }
    Ok(options)
}

/// Parse a sea-surface recovery deck from `<latitude>,<longitude>` degrees.
/// The deck belongs to Earth and is specified in the terrain-radial convention
/// used by the terrain source, capture poses and impact logs, so the measured
/// ground track can be supplied directly. Altitude is the sea surface.
pub fn parse_recovery_deck(value: &str) -> Option<RecoveryDeckCoordinates> {
    let (latitude, longitude) = value.split_once(',')?;
    let latitude: f64 = latitude.trim().parse().ok()?;
    let longitude: f64 = longitude.trim().parse().ok()?;
    RecoveryDeckCoordinates::try_new(latitude, longitude).ok()
}

/// Window-size override for measurement runs, in physical pixels.
///
/// The default window is 1280x720. P0-04 needs the same build measured at
/// 720p and 1440p, so the size is a launch-time presentation knob rather than
/// a rebuild. It affects only the window; simulation and rendering paths are
/// identical.
pub const WINDOW_SIZE_ENV: &str = "COSMIC_SYSTEMS_WINDOW_SIZE";

/// Largest accepted window dimension, to catch a transposed or malformed
/// measurement value before creating a surface.
const MAX_WINDOW_DIMENSION: u32 = 16_384;

/// Parse a `WIDTHxHEIGHT` window size. Zero, non-numeric and out-of-range
/// values are rejected; the caller decides whether that is fatal.
pub fn parse_window_size(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.split_once('x')?;
    let width: u32 = width.trim().parse().ok()?;
    let height: u32 = height.trim().parse().ok()?;
    if width == 0 || height == 0 || width > MAX_WINDOW_DIMENSION || height > MAX_WINDOW_DIMENSION {
        return None;
    }
    Some((width, height))
}

/// Read the window-size override for a measurement run. Unset means the
/// documented 1280x720 default. A malformed value is a configuration error
/// and fails before the renderer starts, rather than silently measuring a
/// different resolution than the one claimed.
pub fn window_size_from_env() -> Option<(u32, u32)> {
    let value = std::env::var(WINDOW_SIZE_ENV).ok()?;
    Some(parse_window_size(&value).unwrap_or_else(|| {
        panic!("Invalid {WINDOW_SIZE_ENV}='{value}': expected WIDTHxHEIGHT, e.g. 2560x1440")
    }))
}

#[cfg(test)]
mod tests {
    use super::{parse_launch_options, parse_window_size, LaunchOptionError, LaunchOptions, Mode};
    use crate::domain::services::gravity::ForceModelTier;
    use crate::domain::value_objects::launch_site_coordinates::PredefinedLaunchSite;

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
    }

    #[test]
    fn unknown_positional_is_rejected() {
        for argument in ["unknown-mode", "gyro"] {
            assert_eq!(
                parse_launch_options([argument.to_string()]),
                Err(LaunchOptionError::UnknownArgument(argument.into()))
            );
        }
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
        assert!(parsed.site.is_none());
    }

    #[test]
    fn site_is_parsed_independently_of_vehicle() {
        let parsed = options(&["rocket", "--site", "ksc", "--vehicle", "starship"]);
        assert_eq!(parsed.mode, Mode::Rocket);
        assert_eq!(parsed.site, Some(PredefinedLaunchSite::KennedyLc39a));
        assert_eq!(parsed.vehicle.as_deref(), Some("starship"));
    }

    #[test]
    fn invalid_site_and_dangling_site_are_rejected() {
        assert_eq!(
            parse_launch_options(["rocket".to_string(), "--site".to_string()]),
            Err(LaunchOptionError::MissingSiteValue)
        );
        assert_eq!(
            parse_launch_options([
                "rocket".to_string(),
                "--site".to_string(),
                "moon".to_string()
            ]),
            Err(LaunchOptionError::UnknownSite("moon".to_string()))
        );
    }

    #[test]
    fn dangling_vehicle_flag_is_rejected() {
        assert_eq!(
            parse_launch_options(["rocket".to_string(), "--vehicle".to_string()]),
            Err(LaunchOptionError::MissingVehicleValue)
        );
    }

    #[test]
    fn parses_explicit_force_model_selection() {
        assert_eq!(
            options(&["rocket", "--force-model", "earth-harmonics"]).force_model,
            Some(ForceModelTier::EarthHarmonics)
        );
        assert_eq!(options(&["rocket"]).force_model, None);
        assert_eq!(
            ForceModelTier::from_key("planet-sun"),
            Some(ForceModelTier::PlanetSun)
        );
    }

    #[test]
    fn rejects_unknown_or_dangling_force_model_flags() {
        assert_eq!(
            parse_launch_options(["rocket".to_string(), "--force-model".to_string()]),
            Err(LaunchOptionError::MissingForceModelValue)
        );
        assert_eq!(
            parse_launch_options([
                "rocket".to_string(),
                "--force-model".to_string(),
                "warp".to_string()
            ]),
            Err(LaunchOptionError::UnknownForceModel("warp".to_string()))
        );
    }

    #[test]
    fn parses_a_terrain_radial_recovery_deck() {
        let parsed = options(&["rocket", "--recovery-deck", "25.84,-90.12"]);
        let deck = parsed.recovery_deck.expect("deck parsed");
        assert!((deck.latitude_deg - 25.84).abs() < 1e-9);
        assert!((deck.longitude_deg + 90.12).abs() < 1e-9);

        assert_eq!(
            parse_launch_options(["rocket".to_string(), "--recovery-deck".to_string()]),
            Err(LaunchOptionError::MissingRecoveryDeckValue)
        );
        for value in ["25.84", "91,0", "25.84,b", "a,b"] {
            assert!(
                matches!(
                    parse_launch_options([
                        "rocket".to_string(),
                        "--recovery-deck".to_string(),
                        value.to_string()
                    ]),
                    Err(LaunchOptionError::InvalidRecoveryDeck(_))
                ),
                "accepted {value:?}"
            );
        }
    }

    #[test]
    fn parses_the_measurement_window_sizes() {
        assert_eq!(parse_window_size("1280x720"), Some((1280, 720)));
        assert_eq!(parse_window_size("2560x1440"), Some((2560, 1440)));
        assert_eq!(parse_window_size(" 1920 x 1080 "), Some((1920, 1080)));
    }

    #[test]
    fn rejects_malformed_window_sizes() {
        for value in ["", "1280", "1280*720", "0x720", "1280x0", "-1x720", "ax720"] {
            assert_eq!(parse_window_size(value), None, "accepted {value:?}");
        }
        assert_eq!(parse_window_size("16385x720"), None);
    }
}
