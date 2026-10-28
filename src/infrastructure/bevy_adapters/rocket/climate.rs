//! Shared presentation adapter for measured monthly climatology.
//!
//! Sky and cloud systems sample the same climatology package at the vehicle's
//! body-fixed subpoint and the current simulation month. Nothing here feeds
//! flight dynamics or the authoritative atmosphere source.

use crate::domain::services::body_orientation::BodyOrientation;
use crate::domain::services::climate::ClimateSample;
use crate::domain::services::reference_frames::{
    body_fixed_to_terrain_lat_lon, planet_inertial_to_body_fixed,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::climate::MeasuredClimate;
use bevy::math::DVec3;

/// Sample the loaded climatology at one vehicle position and simulation month.
///
/// Returns the terrain-radial subpoint and the measured sample. `None` means no
/// package, no rocket, or an unavailable scientific epoch; callers keep the
/// procedural presentation values.
pub fn climate_at_position(
    climate: &MeasuredClimate,
    position_m: DVec3,
    orientation: &BodyOrientation,
    time: &SimulationTime,
) -> Option<(f64, f64, ClimateSample)> {
    if position_m.length_squared() < 1.0 {
        return None;
    }
    let month = simulation_month_fraction(time)?;
    let body_fixed = planet_inertial_to_body_fixed(position_m, orientation);
    let (latitude_deg, longitude_deg) = body_fixed_to_terrain_lat_lon(body_fixed);
    let sample = climate.sample(latitude_deg, longitude_deg, month)?;
    Some((latitude_deg, longitude_deg, sample))
}

/// Month index in `[0, 12)` from the authoritative UTC Julian date. The
/// fractional part advances linearly through the length of the current month so
/// seasonal presentation does not jump at midnight on the first.
pub fn simulation_month_fraction(time: &SimulationTime) -> Option<f64> {
    let epoch = time.scientific_epoch().ok()?;
    month_fraction_from_julian_date(epoch.utc_julian_date())
}

fn month_fraction_from_julian_date(julian_date: f64) -> Option<f64> {
    if !julian_date.is_finite() {
        return None;
    }
    // Fliegel-Van Flandern civil-date conversion. Julian dates are measured
    // from noon, so shift by half a day before splitting date and time.
    let shifted = julian_date + 0.5;
    let mut z = shifted.floor() as i64;
    let day_fraction = shifted - z as f64;
    if z < 2_299_161 {
        if z < -1 {
            return None;
        }
    } else {
        let alpha = ((z as f64 - 1_867_216.25) / 36_524.25).floor() as i64;
        z += 1 + alpha - alpha / 4;
    }
    let b = z + 1524;
    let c = ((b as f64 - 122.1) / 365.25).floor() as i64;
    let d = (365.25 * c as f64).floor() as i64;
    let e = ((b - d) as f64 / 30.6001).floor() as i64;
    let day = b - d - (30.6001 * e as f64).floor() as i64;
    let month = if e < 14 { e - 1 } else { e - 13 };
    let year = if month > 2 { c - 4716 } else { c - 4715 };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_in_month(year, month) as f64;
    let day_index = (day - 1) as f64 + day_fraction;
    Some((month - 1) as f64 + (day_index / days).clamp(0.0, 1.0))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 30,
    }
}

/// Query helper for systems that only need the primary vehicle position.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn julian_dates_map_to_fractional_months() {
        // 2000-01-01 12:00 UTC.
        let january = month_fraction_from_julian_date(2_451_545.0).expect("valid date");
        assert!((january - 0.5 / 31.0).abs() < 1e-9);
        // 2000-02-01 12:00 UTC (leap year).
        let february = month_fraction_from_julian_date(2_451_576.0).expect("valid date");
        assert!((february - (1.0 + 0.5 / 29.0)).abs() < 1e-9);
        // 2001-01-01 00:00 UTC.
        let new_year = month_fraction_from_julian_date(2_451_910.5).expect("valid date");
        assert!(new_year.abs() < 1e-9);
    }

    #[test]
    fn invalid_dates_are_rejected() {
        assert_eq!(month_fraction_from_julian_date(f64::NAN), None);
        assert_eq!(month_fraction_from_julian_date(-1.0e12), None);
    }
}
