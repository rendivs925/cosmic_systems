//! Presentation-only unit conversion for the rocket HUD.
//!
//! The simulation domain stays strictly SI. These conversions exist only so
//! the operator can read the HUD in either metric or US customary units; every
//! conversion constant has exactly one definition here (AGENTS.md sections 15
//! and 40).

/// Unit system selected for HUD display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HudUnits {
    #[default]
    Metric,
    Imperial,
}

pub const FEET_PER_METER: f64 = 3.280_839_895_013_123;
pub const POUNDS_PER_KILOGRAM: f64 = 2.204_622_621_848_775;
pub const METERS_PER_MILE: f64 = 1609.344;
pub const METERS_PER_KILOMETER: f64 = 1000.0;
pub const MPS_TO_MPH: f64 = 2.236_936_292_054_4;
pub const KILONEWTONS_TO_POUND_FORCE: f64 = 224.808_943_161_363_32;

impl HudUnits {
    /// Altitude/vertical distance, formatted value plus the unit suffix.
    pub fn altitude(self, meters: f64) -> (f64, &'static str) {
        match self {
            Self::Metric => (meters, "m"),
            Self::Imperial => (meters * FEET_PER_METER, "ft"),
        }
    }

    /// Speed, formatted value plus the unit suffix.
    pub fn speed(self, meters_per_second: f64) -> (f64, &'static str) {
        match self {
            Self::Metric => (meters_per_second, "m/s"),
            Self::Imperial => (meters_per_second * MPS_TO_MPH, "mph"),
        }
    }

    /// Mass, formatted value plus the unit suffix.
    pub fn mass(self, kilograms: f64) -> (f64, &'static str) {
        match self {
            Self::Metric => (kilograms, "kg"),
            Self::Imperial => (kilograms * POUNDS_PER_KILOGRAM, "lb"),
        }
    }

    /// Long distance (orbit radii/altitudes), returned in km or miles.
    pub fn distance_km(self, meters: f64) -> (f64, &'static str) {
        match self {
            Self::Metric => (meters / METERS_PER_KILOMETER, "km"),
            Self::Imperial => (meters / METERS_PER_MILE, "mi"),
        }
    }

    /// Force from kilonewtons, formatted value plus the unit suffix.
    pub fn thrust_kn(self, kilonewtons: f64) -> (f64, &'static str) {
        match self {
            Self::Metric => (kilonewtons, "kN"),
            Self::Imperial => (kilonewtons * KILONEWTONS_TO_POUND_FORCE, "lbf"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn metric_units_are_identity() {
        close(HudUnits::Metric.altitude(1_000.0).0, 1_000.0);
        close(HudUnits::Metric.speed(100.0).0, 100.0);
        close(HudUnits::Metric.mass(1_000.0).0, 1_000.0);
        close(HudUnits::Metric.distance_km(1_000.0).0, 1.0);
        close(HudUnits::Metric.thrust_kn(1.0).0, 1.0);
    }

    #[test]
    fn imperial_units_convert_with_standard_constants() {
        close(HudUnits::Imperial.altitude(1.0).0, FEET_PER_METER);
        close(HudUnits::Imperial.speed(1.0).0, MPS_TO_MPH);
        close(HudUnits::Imperial.mass(1.0).0, POUNDS_PER_KILOGRAM);
        close(HudUnits::Imperial.distance_km(METERS_PER_MILE).0, 1.0);
        close(
            HudUnits::Imperial.thrust_kn(1.0).0,
            KILONEWTONS_TO_POUND_FORCE,
        );
    }
}
