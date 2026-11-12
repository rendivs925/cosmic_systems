//! Booster entry-burn sequencing.
//!
//! A downrange recovery starts with no boostback: the stage separates on a
//! ballistic arc, coasts through the upper atmosphere, and lights a small
//! engine subset to shed entry energy before the dense lower atmosphere. The
//! burn is a latched altitude gate, not a velocity threshold: an altitude gate
//! fires once on the descending leg and cannot chatter around an apogee where
//! the vertical speed crosses zero repeatedly.

/// Altitude band and engine allocation for the entry burn. Distances are
/// altitude above the body's reference radius, meters; speed is the
/// atmosphere-relative speed at the start gate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntryBurnPlan {
    /// Ignition altitude on the descending leg, m.
    pub start_altitude_m: f64,
    /// Cutoff altitude, m. Must be below `start_altitude_m`.
    pub end_altitude_m: f64,
    /// Engines lit for the entry burn.
    pub engines: u32,
    /// Engine subset kept for the terminal landing burn.
    pub landing_engines: u32,
    /// Engine subset for the high-speed part of the landing burn. A stage
    /// arriving at several hundred metres per second cannot remove that energy
    /// with one engine before the deck; the flown profile lights three and
    /// drops to the center engine for the final settle.
    pub landing_burn_engines: u32,
    /// Speed above which the landing burn keeps `landing_burn_engines`, m/s.
    pub landing_burn_switch_speed_mps: f64,
    /// Altitude above which the landing burn keeps `landing_burn_engines`, m.
    pub landing_burn_switch_altitude_m: f64,
    /// Minimum atmosphere-relative speed for ignition, m/s. Below this the
    /// stage is already slow enough that a retro burn would only waste the
    /// terminal reserve.
    pub min_speed_mps: f64,
}

impl EntryBurnPlan {
    /// Falcon 9-class downrange entry: three engines from 70 km to 38 km, then
    /// the center engine for terminal descent. The altitudes are the same
    /// order as the flown profile recorded in the scientific-correctness audit.
    pub const FALCON9_DOWNRANGE: Self = Self {
        start_altitude_m: 70_000.0,
        end_altitude_m: 55_000.0,
        engines: 3,
        landing_engines: 1,
        landing_burn_engines: 3,
        landing_burn_switch_speed_mps: 120.0,
        landing_burn_switch_altitude_m: 250.0,
        min_speed_mps: 1_000.0,
    };
}

/// Latched entry-burn state. `started` and `complete` are one-way flags, so
/// the gate cannot re-light after cutoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EntryBurnSequence {
    pub started: bool,
    pub complete: bool,
}

/// What the entry-burn law commands this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryBurnCommand {
    /// Still above the ignition band, or not yet descending fast enough.
    Coast,
    /// Burn the entry engine subset.
    Burn,
    /// Cutoff has passed; the terminal engine subset owns the landing.
    Complete,
}

impl EntryBurnSequence {
    /// Advance the latch and return the command for this tick.
    ///
    /// `radial_speed_mps` is positive outward, so a descending stage has a
    /// negative value. The burn starts only on the descending leg inside the
    /// band and finishes at the cutoff altitude; passing below the cutoff
    /// without ever starting (for example, a low, slow trajectory) completes
    /// the sequence so the terminal burn is not pre-empted.
    pub fn advance(
        &mut self,
        altitude_m: f64,
        radial_speed_mps: f64,
        airspeed_mps: f64,
        plan: &EntryBurnPlan,
    ) -> EntryBurnCommand {
        if self.complete {
            return EntryBurnCommand::Complete;
        }
        if self.started {
            if altitude_m <= plan.end_altitude_m {
                self.complete = true;
                return EntryBurnCommand::Complete;
            }
            return EntryBurnCommand::Burn;
        }
        if altitude_m <= plan.end_altitude_m {
            // The band was crossed without a usable ignition. Do not light
            // inside the dense atmosphere, where the burn would add structural
            // and control load without enough altitude to recover.
            self.complete = true;
            return EntryBurnCommand::Complete;
        }
        if altitude_m <= plan.start_altitude_m
            && radial_speed_mps < 0.0
            && airspeed_mps >= plan.min_speed_mps
        {
            self.started = true;
            return EntryBurnCommand::Burn;
        }
        EntryBurnCommand::Coast
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_burn_latches_and_cuts_off_at_the_band() {
        let plan = EntryBurnPlan::FALCON9_DOWNRANGE;
        let mut sequence = EntryBurnSequence::default();
        assert_eq!(
            sequence.advance(80_000.0, -500.0, 2_400.0, &plan),
            EntryBurnCommand::Coast
        );
        assert_eq!(
            sequence.advance(69_000.0, -600.0, 2_300.0, &plan),
            EntryBurnCommand::Burn
        );
        assert!(sequence.started);
        // Climbing back above the start gate must not cancel a latched burn.
        assert_eq!(
            sequence.advance(72_000.0, 100.0, 2_000.0, &plan),
            EntryBurnCommand::Burn
        );
        assert_eq!(
            sequence.advance(37_000.0, -800.0, 1_800.0, &plan),
            EntryBurnCommand::Complete
        );
        assert!(sequence.complete);
        assert_eq!(
            sequence.advance(69_000.0, -600.0, 2_300.0, &plan),
            EntryBurnCommand::Complete,
            "cutoff must be one-way"
        );
    }

    #[test]
    fn entry_burn_does_not_light_while_climbing_or_slow() {
        let plan = EntryBurnPlan::FALCON9_DOWNRANGE;
        for (radial_speed, airspeed) in [(200.0, 2_400.0), (-500.0, 400.0)] {
            let mut sequence = EntryBurnSequence::default();
            assert_eq!(
                sequence.advance(65_000.0, radial_speed, airspeed, &plan),
                EntryBurnCommand::Coast
            );
            assert!(!sequence.started);
        }
    }

    #[test]
    fn crossing_the_band_without_ignition_completes_instead_of_lighting_late() {
        let plan = EntryBurnPlan::FALCON9_DOWNRANGE;
        let mut sequence = EntryBurnSequence::default();
        assert_eq!(
            sequence.advance(30_000.0, -200.0, 900.0, &plan),
            EntryBurnCommand::Complete
        );
        assert!(sequence.complete);
    }
}
