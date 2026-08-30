//! Pure civil and dynamical epoch conversion using a pinned NAIF leap-second table.

use std::fmt;

use crate::domain::services::ephemeris::{EphemerisError, TdbEpoch, J2000_JULIAN_DATE_TDB};

const SECONDS_PER_DAY: f64 = 86_400.0;
const TT_MINUS_TAI_SECONDS: f64 = 32.184;

/// A proleptic-Gregorian UTC calendar instant. Leap-second labels are deferred
/// until the EOP/civil-time reference suite is added.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UtcDateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: f64,
}

impl UtcDateTime {
    pub fn julian_date(self) -> Result<f64, SimulationEpochError> {
        if !(1..=12).contains(&self.month)
            || self.day == 0
            || self.day > days_in_month(self.year, self.month)
            || self.hour > 23
            || self.minute > 59
            || !self.second.is_finite()
            || !(0.0..60.0).contains(&self.second)
        {
            return Err(SimulationEpochError::InvalidUtcDateTime(self));
        }

        let mut year = self.year;
        let mut month = i32::from(self.month);
        if month <= 2 {
            year -= 1;
            month += 12;
        }
        let century = year.div_euclid(100);
        let correction = 2 - century + century.div_euclid(4);
        let calendar_day = (365.25 * f64::from(year + 4_716)).floor()
            + (30.6001 * f64::from(month + 1)).floor()
            + f64::from(self.day)
            + f64::from(correction)
            - 1_524.5;
        let day_fraction =
            (f64::from(self.hour) + f64::from(self.minute) / 60.0 + self.second / 3_600.0) / 24.0;
        Ok(calendar_day + day_fraction)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LeapSecondEntry {
    effective_utc_jd: f64,
    tai_minus_utc_seconds: f64,
}

/// Periodic TDB minus TT model supplied by a NAIF LSK's DELTET constants.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TdbModel {
    k_seconds: f64,
    eb: f64,
    mean_anomaly_at_j2000_rad: f64,
    mean_anomaly_rate_rad_per_second: f64,
}

/// Versioned UTC-to-dynamical-time data parsed from a local NAIF LSK.
#[derive(Clone, Debug, PartialEq)]
pub struct LeapSecondTable {
    entries: Vec<LeapSecondEntry>,
    tdb_model: TdbModel,
}

impl LeapSecondTable {
    pub fn parse_lsk(contents: &str) -> Result<Self, SimulationEpochError> {
        let tdb_model = TdbModel {
            k_seconds: parse_lsk_scalar(contents, "DELTET/K")?,
            eb: parse_lsk_scalar(contents, "DELTET/EB")?,
            mean_anomaly_at_j2000_rad: parse_lsk_pair(contents, "DELTET/M")?.0,
            mean_anomaly_rate_rad_per_second: parse_lsk_pair(contents, "DELTET/M")?.1,
        };
        let (_, delta_at_contents) = contents
            .split_once("DELTET/DELTA_AT")
            .ok_or(SimulationEpochError::MissingLskField("DELTET/DELTA_AT"))?;
        let mut entries = Vec::new();
        for line in delta_at_contents.lines().skip(1) {
            let trimmed = line.trim();
            if trimmed.starts_with(')') {
                break;
            }
            let Some((offset, date)) = trimmed.split_once('@') else {
                continue;
            };
            let offset = offset
                .trim_matches(|character: char| {
                    character == '(' || character == ',' || character.is_whitespace()
                })
                .split_whitespace()
                .last()
                .ok_or(SimulationEpochError::InvalidLskEntry(trimmed.to_string()))?
                .parse::<f64>()
                .map_err(|_| SimulationEpochError::InvalidLskEntry(trimmed.to_string()))?;
            let date = date
                .trim()
                .trim_end_matches(|character: char| character == ',' || character == ')');
            entries.push(LeapSecondEntry {
                effective_utc_jd: parse_lsk_date(date)?.julian_date()?,
                tai_minus_utc_seconds: offset,
            });
        }
        if entries.is_empty()
            || !entries
                .windows(2)
                .all(|pair| pair[0].effective_utc_jd < pair[1].effective_utc_jd)
        {
            return Err(SimulationEpochError::InvalidLskLeapSecondTable);
        }
        Ok(Self { entries, tdb_model })
    }

    pub fn epoch_from_utc(
        &self,
        utc: UtcDateTime,
    ) -> Result<ScientificEpoch, SimulationEpochError> {
        self.epoch_from_utc_julian_date(utc.julian_date()?)
    }

    pub fn epoch_from_utc_julian_date(
        &self,
        utc_julian_date: f64,
    ) -> Result<ScientificEpoch, SimulationEpochError> {
        if !utc_julian_date.is_finite() {
            return Err(SimulationEpochError::InvalidJulianDate(utc_julian_date));
        }
        let tai_julian_date =
            utc_julian_date + self.tai_minus_utc(utc_julian_date)? / SECONDS_PER_DAY;
        let tt_julian_date = tai_julian_date + TT_MINUS_TAI_SECONDS / SECONDS_PER_DAY;
        let tdb_julian_date =
            tt_julian_date + self.tdb_minus_tt_seconds(tt_julian_date) / SECONDS_PER_DAY;
        Ok(ScientificEpoch {
            utc_julian_date,
            tai_julian_date,
            tt_julian_date,
            tdb_epoch: TdbEpoch::from_julian_date(tdb_julian_date)?,
            ut1_julian_date: None,
        })
    }

    pub fn epoch_from_tdb(
        &self,
        tdb_epoch: TdbEpoch,
    ) -> Result<ScientificEpoch, SimulationEpochError> {
        let mut tt_julian_date = tdb_epoch.julian_date();
        for _ in 0..3 {
            tt_julian_date = tdb_epoch.julian_date()
                - self.tdb_minus_tt_seconds(tt_julian_date) / SECONDS_PER_DAY;
        }
        let tai_julian_date = tt_julian_date - TT_MINUS_TAI_SECONDS / SECONDS_PER_DAY;
        let mut utc_julian_date = tai_julian_date
            - self
                .entries
                .last()
                .expect("validated non-empty LSK table")
                .tai_minus_utc_seconds
                / SECONDS_PER_DAY;
        utc_julian_date = tai_julian_date - self.tai_minus_utc(utc_julian_date)? / SECONDS_PER_DAY;
        Ok(ScientificEpoch {
            utc_julian_date,
            tai_julian_date,
            tt_julian_date,
            tdb_epoch,
            ut1_julian_date: None,
        })
    }

    fn tai_minus_utc(&self, utc_julian_date: f64) -> Result<f64, SimulationEpochError> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.effective_utc_jd <= utc_julian_date)
            .map(|entry| entry.tai_minus_utc_seconds)
            .ok_or(SimulationEpochError::UtcOutsideLeapSecondCoverage(
                utc_julian_date,
            ))
    }

    fn tdb_minus_tt_seconds(&self, tt_julian_date: f64) -> f64 {
        let tdb_seconds_since_j2000 = (tt_julian_date - J2000_JULIAN_DATE_TDB) * SECONDS_PER_DAY;
        let mean_anomaly = self.tdb_model.mean_anomaly_at_j2000_rad
            + self.tdb_model.mean_anomaly_rate_rad_per_second * tdb_seconds_since_j2000;
        self.tdb_model.k_seconds * (mean_anomaly + self.tdb_model.eb * mean_anomaly.sin()).sin()
    }
}

/// One physical instant with explicit civil and dynamical representations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScientificEpoch {
    utc_julian_date: f64,
    tai_julian_date: f64,
    tt_julian_date: f64,
    tdb_epoch: TdbEpoch,
    ut1_julian_date: Option<f64>,
}

impl ScientificEpoch {
    pub const fn utc_julian_date(self) -> f64 {
        self.utc_julian_date
    }
    pub const fn tai_julian_date(self) -> f64 {
        self.tai_julian_date
    }
    pub const fn tt_julian_date(self) -> f64 {
        self.tt_julian_date
    }
    pub const fn tdb_epoch(self) -> TdbEpoch {
        self.tdb_epoch
    }
    pub const fn ut1_julian_date(self) -> Option<f64> {
        self.ut1_julian_date
    }

    /// UT1 is unavailable until a validated Earth-orientation dataset supplies
    /// it. UTC is never substituted because that would mislabel Earth-fixed
    /// outputs as reference-grade.
    pub fn require_ut1_julian_date(self) -> Result<f64, SimulationEpochError> {
        self.ut1_julian_date
            .ok_or(SimulationEpochError::EarthOrientationUnavailable)
    }
}

#[derive(Debug)]
pub enum SimulationEpochError {
    UnconfiguredAuthority,
    EarthOrientationUnavailable,
    InvalidUtcDateTime(UtcDateTime),
    InvalidJulianDate(f64),
    MissingLskField(&'static str),
    InvalidLskEntry(String),
    InvalidLskLeapSecondTable,
    UtcOutsideLeapSecondCoverage(f64),
    Ephemeris(EphemerisError),
}

impl From<EphemerisError> for SimulationEpochError {
    fn from(error: EphemerisError) -> Self {
        Self::Ephemeris(error)
    }
}

impl fmt::Display for SimulationEpochError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnconfiguredAuthority => {
                formatter.write_str("scientific epoch authority has not been configured")
            }
            Self::EarthOrientationUnavailable => formatter
                .write_str("UT1 is unavailable because no Earth-orientation dataset is configured"),
            Self::InvalidUtcDateTime(value) => {
                write!(formatter, "invalid UTC date-time: {value:?}")
            }
            Self::InvalidJulianDate(value) => write!(formatter, "invalid Julian date: {value}"),
            Self::MissingLskField(field) => write!(formatter, "LSK is missing {field}"),
            Self::InvalidLskEntry(entry) => write!(formatter, "invalid LSK entry: {entry}"),
            Self::InvalidLskLeapSecondTable => formatter.write_str("invalid LSK leap-second table"),
            Self::UtcOutsideLeapSecondCoverage(value) => {
                write!(formatter, "UTC JD {value} is outside leap-second coverage")
            }
            Self::Ephemeris(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SimulationEpochError {}

fn parse_lsk_scalar(contents: &str, name: &'static str) -> Result<f64, SimulationEpochError> {
    let line = contents
        .lines()
        .find(|line| line.trim_start().starts_with(name))
        .ok_or(SimulationEpochError::MissingLskField(name))?;
    line.split_once('=')
        .and_then(|(_, value)| value.split_whitespace().next())
        .map(|value| value.replace('D', "E").parse::<f64>())
        .ok_or(SimulationEpochError::InvalidLskEntry(line.to_string()))?
        .map_err(|_| SimulationEpochError::InvalidLskEntry(line.to_string()))
}

fn parse_lsk_pair(contents: &str, name: &'static str) -> Result<(f64, f64), SimulationEpochError> {
    let line = contents
        .lines()
        .find(|line| line.trim_start().starts_with(name))
        .ok_or(SimulationEpochError::MissingLskField(name))?;
    let mut values = line
        .split_once('=')
        .map(|(_, value)| value)
        .unwrap_or_default()
        .trim_matches(|character: char| {
            character == '(' || character == ')' || character.is_whitespace()
        })
        .split_whitespace()
        .map(|value| value.replace('D', "E").parse::<f64>());
    let first = values
        .next()
        .ok_or_else(|| SimulationEpochError::InvalidLskEntry(line.to_string()))?
        .map_err(|_| SimulationEpochError::InvalidLskEntry(line.to_string()))?;
    let second = values
        .next()
        .ok_or_else(|| SimulationEpochError::InvalidLskEntry(line.to_string()))?
        .map_err(|_| SimulationEpochError::InvalidLskEntry(line.to_string()))?;
    if values.next().is_some() {
        return Err(SimulationEpochError::InvalidLskEntry(line.to_string()));
    }
    Ok((first, second))
}

fn parse_lsk_date(value: &str) -> Result<UtcDateTime, SimulationEpochError> {
    let mut fields = value.trim().split('-');
    let year = fields.next().and_then(|field| field.parse().ok());
    let month = fields.next().and_then(month_number);
    let day = fields.next().and_then(|field| field.parse().ok());
    match (year, month, day, fields.next()) {
        (Some(year), Some(month), Some(day), None) => Ok(UtcDateTime {
            year,
            month,
            day,
            hour: 0,
            minute: 0,
            second: 0.0,
        }),
        _ => Err(SimulationEpochError::InvalidLskEntry(value.to_string())),
    }
}

fn month_number(value: &str) -> Option<u8> {
    match value {
        "JAN" => Some(1),
        "FEB" => Some(2),
        "MAR" => Some(3),
        "APR" => Some(4),
        "MAY" => Some(5),
        "JUN" => Some(6),
        "JUL" => Some(7),
        "AUG" => Some(8),
        "SEP" => Some(9),
        "OCT" => Some(10),
        "NOV" => Some(11),
        "DEC" => Some(12),
        _ => None,
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if year.rem_euclid(4) == 0
            && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0) =>
        {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAIF0012: &str = include_str!("../../../assets/large_files/kernels/de440/naif0012.tls");

    #[test]
    fn lsk_offsets_are_used_for_utc_to_tai() {
        let table = LeapSecondTable::parse_lsk(NAIF0012).unwrap();
        let epoch = table
            .epoch_from_utc(UtcDateTime {
                year: 2000,
                month: 1,
                day: 1,
                hour: 12,
                minute: 0,
                second: 0.0,
            })
            .unwrap();
        // A Julian date near J2000 has roughly 50 microseconds of f64 spacing.
        assert!(
            ((epoch.tai_julian_date() - epoch.utc_julian_date()) * SECONDS_PER_DAY - 32.0).abs()
                < 1.0e-4
        );
        assert!(
            ((epoch.tt_julian_date() - epoch.tai_julian_date()) * SECONDS_PER_DAY
                - TT_MINUS_TAI_SECONDS)
                .abs()
                < 1.0e-4
        );
    }

    #[test]
    fn tdb_round_trip_preserves_the_physical_instant() {
        let table = LeapSecondTable::parse_lsk(NAIF0012).unwrap();
        let source = table
            .epoch_from_utc(UtcDateTime {
                year: 2017,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0.0,
            })
            .unwrap();
        let recovered = table.epoch_from_tdb(source.tdb_epoch()).unwrap();
        assert!(
            (recovered.utc_julian_date() - source.utc_julian_date()).abs() * SECONDS_PER_DAY
                < 1.0e-6
        );
        assert_eq!(source.ut1_julian_date(), None);
    }

    #[test]
    fn tai_records_the_2016_leap_second_discontinuity() {
        let table = LeapSecondTable::parse_lsk(NAIF0012).unwrap();
        let before = table
            .epoch_from_utc(UtcDateTime {
                year: 2016,
                month: 12,
                day: 31,
                hour: 23,
                minute: 59,
                second: 59.0,
            })
            .unwrap();
        let after = table
            .epoch_from_utc(UtcDateTime {
                year: 2017,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0.0,
            })
            .unwrap();

        // UTC labels are one nominal second apart; the inserted 23:59:60 UTC
        // makes the corresponding TAI instants two SI seconds apart.
        assert!(
            ((after.tai_julian_date() - before.tai_julian_date()) * SECONDS_PER_DAY - 2.0).abs()
                < 1.0e-4
        );
    }

    #[test]
    fn ut1_request_fails_explicitly_without_earth_orientation_data() {
        let table = LeapSecondTable::parse_lsk(NAIF0012).unwrap();
        let epoch = table.epoch_from_tdb(TdbEpoch::j2000()).unwrap();

        assert!(matches!(
            epoch.require_ut1_julian_date(),
            Err(SimulationEpochError::EarthOrientationUnavailable)
        ));
    }
}
