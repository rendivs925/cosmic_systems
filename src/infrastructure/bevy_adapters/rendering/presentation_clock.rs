//! Deterministic presentation clock for captured frames.
//!
//! Every animated presentation shader (cloud drift, sea waves, vegetation sway)
//! is fed from `Time::elapsed_secs()`, which counts from the moment the
//! application started updating. Two runs of the same build therefore never
//! render the same frame: a capture waits for a wall-clock interval while
//! startup varies by tens of seconds, so a before/after comparison of a shader
//! change is confounded by a different cloud and wave phase.
//!
//! `COSMIC_SYSTEMS_CAPTURE_TIME=<seconds>` pins the clock to a fixed phase for
//! measurement. Unset, `Time::elapsed_secs()` is used unchanged, so nothing
//! differs in normal operation.
//!
//! Presentation only: this clock can never reach the simulation, the terrain
//! source, or collision state.

use std::sync::OnceLock;

/// Environment variable that pins the presentation clock, in seconds.
pub const FIXED_TIME_ENV: &str = "COSMIC_SYSTEMS_CAPTURE_TIME";

/// The clock every presentation shader should advance with.
pub fn presentation_time_s(elapsed_s: f32) -> f32 {
    apply_override(elapsed_s, fixed_time_s())
}

fn apply_override(elapsed_s: f32, fixed: Option<f32>) -> f32 {
    fixed.unwrap_or(elapsed_s)
}

fn fixed_time_s() -> Option<f32> {
    static FIXED: OnceLock<Option<f32>> = OnceLock::new();
    *FIXED.get_or_init(|| parse_fixed_time(std::env::var(FIXED_TIME_ENV).ok().as_deref()))
}

/// Parse the fixed-clock override. Anything that is not a finite, non-negative
/// number of seconds is rejected rather than silently mangling the clock.
fn parse_fixed_time(value: Option<&str>) -> Option<f32> {
    let raw = value?.trim();
    let seconds: f32 = raw.parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    Some(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_time_accepts_only_a_finite_non_negative_duration() {
        assert_eq!(parse_fixed_time(Some("42")), Some(42.0));
        assert_eq!(parse_fixed_time(Some(" 42.5 ")), Some(42.5));
        assert_eq!(parse_fixed_time(Some("0")), Some(0.0));
        assert_eq!(parse_fixed_time(Some("-1")), None);
        assert_eq!(parse_fixed_time(Some("nan")), None);
        assert_eq!(parse_fixed_time(Some("inf")), None);
        assert_eq!(parse_fixed_time(Some("soon")), None);
        assert_eq!(parse_fixed_time(Some("")), None);
        assert_eq!(parse_fixed_time(None), None);
    }

    /// Without an override the shader clock must keep running, or every
    /// animated surface in the simulator would freeze.
    #[test]
    fn the_clock_is_unchanged_when_no_capture_is_requested() {
        assert_eq!(apply_override(12.25, None), 12.25);
        assert_eq!(apply_override(12.25, Some(42.0)), 42.0);
    }
}
