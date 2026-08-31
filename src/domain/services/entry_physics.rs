//! Pure entry, descent, and landing physics models.
//!
//! These functions are the single authority for plasma blackout, retro-
//! propulsion effectiveness, and parachute deployment sequencing. Bevy
//! systems in `bevy_adapters::rocket_entry` adapt them into ECS execution;
//! they never re-implement the math here (AGENTS.md sections 19 and 50).
//!
//! ## Models
//!
//! - Plasma electron density: empirical fit `n_e = C·ρ·v³`. Comms blackout
//!   when `n_e` exceeds the critical density of the comms carrier frequency
//!   (`n_e > n_crit` ⇔ plasma frequency above the carrier). The historical
//!   coefficient is preserved; recalibration is a config task, not a code
//!   change.
//! - Supersonic retro-propulsion: DLR-style base-pressure correlation. The
//!   effective-thrust multiplier decreases monotonically with Mach above the
//!   threshold and clamps at [`MIN_RETRO_EFFECTIVENESS`].
//! - Parachutes: drogue mortar-deploy gated on Mach/altitude AND a descent
//!   direction (deployment into an ascending airstream would destroy the
//!   canopy), reefed inflation over `reef_time_s`, main gated on altitude
//!   after the drogue is fully inflated.

/// Empirical coefficient of the electron-density fit `n_e = C·ρ·v³`
/// (historical value preserved from the original inline model).
pub const PLASMA_DENSITY_COEFFICIENT_M3: f64 = 1e-4;

/// Floor of the retro-propulsion thrust effectiveness (correlation clamp):
/// even at extreme Mach the plume never loses more than 90% effectiveness.
pub const MIN_RETRO_EFFECTIVENESS: f64 = 0.1;

/// Maximum Mach excess over the threshold used by the base-pressure fit.
pub const MAX_RETRO_MACH_EXCESS: f64 = 5.0;

/// Entry velocity (m/s) above which the Tauber-Sutton radiative term is
/// significant — the ~10 km/s lunar-return regime of the spec scenario
/// "lunar return radiative dominance".
pub const RADIATIVE_ENTRY_THRESHOLD_MPS: f64 = 10_000.0;

/// Stagnation-point convective heat flux: Sutton-Graves
/// `q_dot = k · sqrt(ρ / R_nose) · v³` [W/m²].
///
/// `k` is the body-calibrated convective coefficient. The flux scales as
/// `1/√R_nose`, so a blunter (larger) nose sees less heat — the spec scenario
/// "nose radius effect".
pub fn convective_heat_flux_w_m2(
    convective_coefficient: f64,
    density_kg_m3: f64,
    nose_radius_m: f64,
    velocity_mps: f64,
) -> f64 {
    if nose_radius_m <= 0.0 || density_kg_m3 <= 0.0 || velocity_mps <= 0.0 {
        return 0.0;
    }
    convective_coefficient * (density_kg_m3 / nose_radius_m).sqrt() * velocity_mps.powi(3)
}

/// Radiative heat flux (W/m²) via the Tauber-Sutton approximation. Zero below
/// the entry-interface velocity (the ~10 km/s lunar-return regime), then
/// `k_r · ρ · v⁸ / 1e24`. Above that velocity it becomes comparable to, then
/// exceeds, the convective term for large bodies.
pub fn radiative_heat_flux_w_m2(
    radiative_coefficient: f64,
    density_kg_m3: f64,
    velocity_mps: f64,
) -> f64 {
    if velocity_mps > RADIATIVE_ENTRY_THRESHOLD_MPS && density_kg_m3 > 0.0 {
        radiative_coefficient * density_kg_m3 * velocity_mps.powi(8) / 1e24
    } else {
        0.0
    }
}

/// Ablative TPS recession rate `dr/dt = q_dot / (ρ_tps · H_abl)` [m/s].
/// `q_dot` is the heat flux the surface absorbs, `ρ_tps` the TPS density, and
/// `H_abl` the heat of ablation.
pub fn tps_recession_rate_mps(
    total_heat_flux_w_m2: f64,
    tps_density_kg_m3: f64,
    heat_of_ablation_j_kg: f64,
) -> f64 {
    if tps_density_kg_m3 <= 0.0 || heat_of_ablation_j_kg <= 0.0 {
        return 0.0;
    }
    total_heat_flux_w_m2 / (tps_density_kg_m3 * heat_of_ablation_j_kg)
}

/// Advances an ablative layer by one fixed step without consuming material
/// beyond the configured physical thickness. Returns the actual recession
/// during the step in meters.
pub fn capped_tps_recession_m(
    recession_rate_mps: f64,
    timestep_s: f64,
    thickness_remaining_m: f64,
) -> f64 {
    if recession_rate_mps <= 0.0 || timestep_s <= 0.0 || thickness_remaining_m <= 0.0 {
        return 0.0;
    }
    (recession_rate_mps * timestep_s).min(thickness_remaining_m)
}

/// Mass removed from a circular TPS cap during a recession step. The cap area
/// is evaluated at the midpoint nose radius so a changing bluntness does not
/// make the fixed-step result depend on which endpoint is sampled.
pub fn tps_mass_loss_kg(
    tps_density_kg_m3: f64,
    nose_radius_before_m: f64,
    recession_m: f64,
) -> f64 {
    if tps_density_kg_m3 <= 0.0 || nose_radius_before_m <= 0.0 || recession_m <= 0.0 {
        return 0.0;
    }
    let mean_radius_m = nose_radius_before_m + recession_m * 0.5;
    std::f64::consts::PI * mean_radius_m.powi(2) * recession_m * tps_density_kg_m3
}

/// Electron density from the empirical fit `n_e = C·ρ·v³` [1/m³].
pub fn electron_density_m3(density_kg_m3: f64, velocity_mps: f64) -> f64 {
    PLASMA_DENSITY_COEFFICIENT_M3 * density_kg_m3 * velocity_mps.powi(3)
}

/// Comms blackout is active when the electron density exceeds the critical
/// density for the comms carrier frequency.
pub fn comms_blackout_active(electron_density_m3: f64, critical_density_m3: f64) -> bool {
    electron_density_m3 > critical_density_m3
}

/// Retro-propulsion thrust effectiveness multiplier from the DLR base-
/// pressure correlation shape: 1.0 at or below the Mach threshold, then
/// decreasing linearly in the Mach excess, clamped at
/// [`MIN_RETRO_EFFECTIVENESS`]. Monotonically non-increasing in Mach.
pub fn retro_propulsion_effectiveness(
    mach: f64,
    mach_threshold: f64,
    base_pressure_coefficient: f64,
) -> f64 {
    if mach <= mach_threshold {
        return 1.0;
    }
    let mach_excess = (mach - mach_threshold).min(MAX_RETRO_MACH_EXCESS);
    (1.0 - base_pressure_coefficient * mach_excess).max(MIN_RETRO_EFFECTIVENESS)
}

/// Deployment parameters for one canopy (mortar gate + reefing + drag area).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanopyConfig {
    pub deploy_mach: f64,
    pub deploy_altitude_m: f64,
    pub reef_time_s: f64,
    pub reef_cd: f64,
    pub full_cd: f64,
    pub reference_area_m2: f64,
}

impl CanopyConfig {
    /// Airless-body configuration whose deploy gates can never open.
    pub fn disabled() -> Self {
        Self {
            deploy_mach: 0.0,
            deploy_altitude_m: 0.0,
            reef_time_s: 0.0,
            reef_cd: 0.0,
            full_cd: 0.0,
            reference_area_m2: 0.0,
        }
    }
}

/// Full parachute deployment parameters mapped once from the per-body
/// `EntryPhysicsConfig` (single source; no duplicated constants here).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParachuteConfig {
    pub drogue: CanopyConfig,
    pub main: CanopyConfig,
}

impl ParachuteConfig {
    pub fn disabled() -> Self {
        Self {
            drogue: CanopyConfig::disabled(),
            main: CanopyConfig::disabled(),
        }
    }
}

/// Which canopy events occurred during one [`ParachuteDeploymentState::advance`]
/// step. Lets systems emit telemetry/log lines without re-deriving transitions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParachuteTransitions {
    pub drogue_deployed: bool,
    pub drogue_inflated: bool,
    pub main_deployed: bool,
    pub main_inflated: bool,
}

impl ParachuteTransitions {
    pub fn any(self) -> bool {
        self.drogue_deployed || self.drogue_inflated || self.main_deployed || self.main_inflated
    }
}

/// Deployment state machine for the drogue/main chute sequence. Pure data +
/// transition logic; the Bevy component wraps this struct.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ParachuteDeploymentState {
    pub drogue_deployed: bool,
    pub drogue_reefed: bool,
    pub drogue_fully_inflated: bool,
    pub drogue_timer_s: f64,
    pub main_deployed: bool,
    pub main_reefed: bool,
    pub main_fully_inflated: bool,
    pub main_timer_s: f64,
    /// Current combined drag coefficient of all deployed canopies.
    pub current_cd: f64,
    /// Current combined reference area of all deployed canopies [m²].
    pub current_reference_area_m2: f64,
}

impl ParachuteDeploymentState {
    /// Advance the deployment sequence by `dt`. Deployment requires descent:
    /// `vertical_speed_mps < 0` along the local up direction (a mortar charge
    /// fired into an ascending airstream would collapse the canopy).
    /// Returns the transitions that occurred this step.
    pub fn advance(
        &mut self,
        config: &ParachuteConfig,
        altitude_m: f64,
        mach: f64,
        vertical_speed_mps: f64,
        dt: f64,
    ) -> ParachuteTransitions {
        let mut transitions = ParachuteTransitions::default();
        if !self.drogue_deployed {
            let gate_open = vertical_speed_mps < 0.0
                && mach <= config.drogue.deploy_mach
                && altitude_m <= config.drogue.deploy_altitude_m;
            if gate_open {
                self.drogue_deployed = true;
                self.drogue_reefed = true;
                self.drogue_timer_s = 0.0;
                self.current_cd = config.drogue.reef_cd;
                self.current_reference_area_m2 = config.drogue.reference_area_m2;
                transitions.drogue_deployed = true;
            }
            return transitions;
        }

        // Reefed drogue inflation timer.
        if !self.drogue_fully_inflated {
            self.drogue_timer_s += dt;
            if self.drogue_timer_s >= config.drogue.reef_time_s {
                self.drogue_fully_inflated = true;
                self.drogue_reefed = false;
                self.current_cd = config.drogue.full_cd;
                self.current_reference_area_m2 = config.drogue.reference_area_m2;
                transitions.drogue_inflated = true;
            }
            return transitions;
        }

        // Main deploys only after the drogue has fully inflated.
        if !self.main_deployed && altitude_m <= config.main.deploy_altitude_m {
            self.main_deployed = true;
            self.main_reefed = true;
            self.main_timer_s = 0.0;
            self.current_cd = config.main.reef_cd;
            self.current_reference_area_m2 = config.main.reference_area_m2;
            transitions.main_deployed = true;
            return transitions;
        }

        // Reefed main inflation timer.
        if self.main_deployed && !self.main_fully_inflated {
            self.main_timer_s += dt;
            if self.main_timer_s >= config.main.reef_time_s {
                self.main_fully_inflated = true;
                self.main_reefed = false;
                self.current_cd = config.main.full_cd;
                self.current_reference_area_m2 = config.main.reference_area_m2;
                transitions.main_inflated = true;
            }
        }
        transitions
    }

    /// Combined parachute drag magnitude [N] at the current flight condition.
    pub fn drag_force_n(&self, density_kg_m3: f64, speed_mps: f64) -> f64 {
        if !(self.drogue_deployed || self.main_deployed) {
            return 0.0;
        }
        0.5 * density_kg_m3
            * speed_mps
            * speed_mps
            * self.current_cd
            * self.current_reference_area_m2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRITICAL_DENSITY_M3: f64 = 6.6e16;

    #[test]
    fn blackout_crossing_produces_exactly_start_and_stop() {
        // Sequence crossing up through the threshold then back down must
        // produce exactly two edge detections: start then stop.
        let mut active = false;
        let mut edges = Vec::new();
        let samples = [
            1e10,
            6.5e16,
            CRITICAL_DENSITY_M3 * 2.0,
            CRITICAL_DENSITY_M3 * 3.0,
            5e16,
            1e10,
        ];
        for n in samples {
            let now = comms_blackout_active(n, CRITICAL_DENSITY_M3);
            if now != active {
                edges.push(now);
                active = now;
            }
        }
        assert_eq!(edges, vec![true, false], "exactly one start and one stop");
    }

    #[test]
    fn electron_density_follows_empirical_fit() {
        assert_eq!(
            electron_density_m3(0.02, 7000.0),
            1e-4 * 0.02 * 7000f64.powi(3)
        );
        assert_eq!(electron_density_m3(0.0, 7000.0), 0.0);
        assert_eq!(electron_density_m3(1.0, 0.0), 0.0);
    }

    #[test]
    fn retro_effectiveness_monotonic_decreasing_with_mach() {
        let threshold = 1.2;
        let coeff = 0.1;
        let mut previous = retro_propulsion_effectiveness(0.5, threshold, coeff);
        for mach in [0.8, 1.2, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0] {
            let current = retro_propulsion_effectiveness(mach, threshold, coeff);
            assert!(
                current <= previous,
                "effectiveness must not increase: {mach} → {current} > {previous}"
            );
            previous = current;
        }
    }

    #[test]
    fn retro_effectiveness_clamped_at_floor() {
        // A strong enough interaction reaches the floor within the excess cap.
        let eff = retro_propulsion_effectiveness(100.0, 1.2, 0.25);
        assert!((eff - MIN_RETRO_EFFECTIVENESS).abs() < 1e-12);
        assert!(retro_propulsion_effectiveness(50.0, 1.0, 0.5) >= MIN_RETRO_EFFECTIVENESS);
    }

    #[test]
    fn retro_effectiveness_is_one_below_threshold() {
        assert_eq!(retro_propulsion_effectiveness(1.2, 1.2, 0.1), 1.0);
        assert_eq!(retro_propulsion_effectiveness(0.9, 1.2, 0.1), 1.0);
    }

    fn earth_config() -> ParachuteConfig {
        ParachuteConfig {
            drogue: CanopyConfig {
                deploy_mach: 2.5,
                deploy_altitude_m: 15_000.0,
                reef_time_s: 5.0,
                reef_cd: 0.5,
                full_cd: 1.2,
                reference_area_m2: 20.0,
            },
            main: CanopyConfig {
                deploy_mach: 0.0,
                deploy_altitude_m: 3_000.0,
                reef_time_s: 3.0,
                reef_cd: 0.8,
                full_cd: 2.2,
                reference_area_m2: 150.0,
            },
        }
    }

    #[test]
    fn drogue_requires_descent_direction() {
        let config = earth_config();
        let mut state = ParachuteDeploymentState::default();

        // Ascending through the gate: no deployment even inside Mach/altitude.
        let t = state.advance(&config, 14_000.0, 2.0, 200.0, 0.016);
        assert!(!state.drogue_deployed);
        assert!(!t.any());

        // Descending through the gate: deploys immediately.
        let t = state.advance(&config, 14_000.0, 2.0, -200.0, 0.016);
        assert!(state.drogue_deployed);
        assert!(t.drogue_deployed);
        assert!(state.drogue_reefed);
    }

    #[test]
    fn drogue_gate_respects_mach_and_altitude() {
        let config = earth_config();
        let mut state = ParachuteDeploymentState::default();
        // Too fast.
        state.advance(&config, 10_000.0, 3.0, -200.0, 0.016);
        assert!(!state.drogue_deployed);
        // Too high.
        state.advance(&config, 20_000.0, 2.0, -200.0, 0.016);
        assert!(!state.drogue_deployed);
        // Inside both gates.
        state.advance(&config, 10_000.0, 2.0, -200.0, 0.016);
        assert!(state.drogue_deployed);
    }

    #[test]
    fn reefing_timers_sequence_drogue_then_main() {
        let config = earth_config();
        let dt = 0.1;
        let mut state = ParachuteDeploymentState::default();

        state.advance(&config, 14_000.0, 2.0, -200.0, dt);
        assert!(state.current_cd > 0.0 && state.current_cd < config.drogue.full_cd);

        // Reef phase: no transitions while the timer runs (well short of the
        // 5 s reef time, immune to float drift).
        for _ in 0..40 {
            let t = state.advance(&config, 12_000.0, 1.5, -80.0, dt);
            assert!(!t.any(), "no transitions mid-reef");
        }
        // Inflation happens once reef_time elapses (drift-tolerant window).
        let mut inflated = false;
        for _ in 0..15 {
            if state
                .advance(&config, 12_000.0, 1.5, -80.0, dt)
                .drogue_inflated
            {
                inflated = true;
                break;
            }
        }
        assert!(inflated, "drogue must inflate after reef_time");
        assert_eq!(state.current_cd, config.drogue.full_cd);

        // Above the main gate: nothing deploys.
        let t = state.advance(&config, 4_000.0, 0.8, -70.0, dt);
        assert!(!t.any());

        // Below the main gate: deploys reefed.
        let t = state.advance(&config, 2_500.0, 0.5, -60.0, dt);
        assert!(t.main_deployed);
        assert_eq!(state.current_cd, config.main.reef_cd);

        // Main reef phase: quiet until its timer completes (3 s).
        for _ in 0..20 {
            let t = state.advance(&config, 2_000.0, 0.4, -55.0, dt);
            assert!(!t.any(), "no transitions during main reef");
        }
        let mut main_inflated = false;
        for _ in 0..15 {
            if state
                .advance(&config, 2_000.0, 0.4, -55.0, dt)
                .main_inflated
            {
                main_inflated = true;
                break;
            }
        }
        assert!(main_inflated, "main must inflate after reef_time");
        assert!(state.main_fully_inflated);
        assert_eq!(state.current_cd, config.main.full_cd);
        assert_eq!(
            state.current_reference_area_m2,
            config.main.reference_area_m2
        );
    }

    #[test]
    fn disabled_config_never_deploys() {
        let config = ParachuteConfig::disabled();
        let mut state = ParachuteDeploymentState::default();
        let t = state.advance(&config, 100.0, 0.1, -10.0, 0.016);
        assert!(!state.drogue_deployed);
        assert!(!t.any());
        assert_eq!(state.drag_force_n(1.225, 50.0), 0.0);
    }

    #[test]
    fn drag_force_matches_q_cd_a() {
        let config = earth_config();
        let mut state = ParachuteDeploymentState::default();
        state.advance(&config, 14_000.0, 2.0, -200.0, 0.016);
        let expected =
            0.5 * 0.3 * 300.0_f64.powi(2) * config.drogue.reef_cd * config.drogue.reference_area_m2;
        assert!((state.drag_force_n(0.3, 300.0) - expected).abs() < 1e-6);
    }

    // -----------------------------------------------------------------------
    // Flight validation (spec: entry-physics "needs flight validation").
    // A synthetic descent profile is pushed through the heating/ablation
    // models to validate the physical ordering the spec calls out: heating
    // peaks at the high-drag region then decays as velocity drops; ablation
    // recedes a blunt nose which then reduces subsequent heating.
    // -----------------------------------------------------------------------

    const K_CONV: f64 = 2.0e-5;
    const NOSE_M: f64 = 0.5;

    /// Exponential tropospheric-style density from altitude (scale height ~
    /// 7.5 km), just realistic enough to shape the entry profile.
    fn density_at_alt(altitude_m: f64) -> f64 {
        1.2 * (-altitude_m / 7_500.0).exp()
    }

    #[test]
    fn reentry_convective_flux_peaks_at_high_drag_then_decays() {
        // Descent: altitude 90 km → 0, velocity 7 700 → 300 m/s (a capsule
        // bleeding energy), density rising exponentially. The Sutton-Graves
        // term ∝ sqrt(rho)·v³ must rise from near-zero at the top, peak in the
        // high-drag region, then decay as the vehicle slows.
        let mut flux = Vec::new();
        let mut alt = 90_000.0;
        let mut v = 7_700.0;

        for _ in 0..150 {
            flux.push(convective_heat_flux_w_m2(
                K_CONV,
                density_at_alt(alt),
                NOSE_M,
                v,
            ));
            // Descend and bleed energy (drag + gravity turn).
            alt = (alt - 800.0).max(0.0);
            v = (v - 55.0).max(300.0);
        }

        // The flux must be unimodal: a single interior peak (the high-drag
        // region) with a monotonic decay after it.
        let peak = flux
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .expect("flux vector non-empty");
        assert!(
            peak > 5,
            "peak {peak} too close to the thin-atmosphere start"
        );
        assert!(
            peak < flux.len() - 5,
            "peak {peak} too close to the slowed low-altitude end"
        );
        assert!(
            flux[peak] > flux[0] * 3.0,
            "peak {} not >> thin-atmosphere start {}",
            flux[peak],
            flux[0]
        );
        assert!(
            flux[peak] > flux[flux.len() - 1] * 3.0,
            "peak {} not >> slowed end {}",
            flux[peak],
            flux[flux.len() - 1]
        );
        // Once past the peak, velocity loss dominates: flux decays (small
        // tolerance for the discrete sample).
        for w in flux[peak..].windows(2) {
            assert!(w[1] <= w[0] * 1.01, "flux must decay after the peak");
        }
    }

    #[test]
    fn doubling_nose_radius_reduces_flux_by_sqrt_two() {
        // Spec "nose radius effect": peak flux scales as 1/√R_nose, so a nose
        // twice the radius sees the flux divided by √2.
        let rho = 5.0e-4;
        let v = 7_000.0;
        let q_r = convective_heat_flux_w_m2(K_CONV, rho, NOSE_M, v);
        let q_2r = convective_heat_flux_w_m2(K_CONV, rho, 2.0 * NOSE_M, v);
        let expected_ratio = std::f64::consts::SQRT_2;
        let ratio = q_r / q_2r;
        assert!(
            (ratio - expected_ratio).abs() < 1e-9,
            "1/√R nose scaling broken: ratio {ratio} vs √2 {expected_ratio}"
        );
        assert!(q_2r < q_r, "blunter nose must see less convective heat");
    }

    #[test]
    fn radiative_dominance_appears_at_lunar_return_velocity() {
        // Spec "lunar return radiative dominance": below the ~10 km/s interface
        // the radiative term is zero; above it, with a lunar-return-calibrated
        // coefficient, it becomes comparable to (here, exceeds) convective.
        let rho = 2.0e-4;
        let v = 12_000.0; // > threshold
        assert_eq!(radiative_heat_flux_w_m2(K_CONV, rho, 9_000.0), 0.0);
        let q_rad = radiative_heat_flux_w_m2(10.0, rho, v);
        let q_conv = convective_heat_flux_w_m2(K_CONV, rho, NOSE_M, v);
        assert!(q_rad > 0.0);
        assert!(
            q_rad > q_conv,
            "lunar-return radiative flux {q_rad} must exceed convective {q_conv}"
        );
        // Strong v^8 growth: a 10% speed increase multiplies radiative flux by
        // roughly 1.1^8 ≈ 2.14.
        let q_rad_up = radiative_heat_flux_w_m2(10.0, rho, v * 1.1);
        let expected_growth = 1.1_f64.powi(8);
        let growth = q_rad_up / q_rad;
        assert!((growth - expected_growth).abs() < 1e-9);
    }

    #[test]
    fn ablation_recedes_nose_and_lowers_subsequent_heating() {
        // Spec "shape change effect": a blunted (recessed) nose reduces the
        // heat flux on the next pass (1/√R). Drive a fixed heat load through
        // the TPS recession model for the spec scenario "TPS recession".
        let tps_density = 1_600.0;
        let heat_of_ablation = 3.0e6; // J/kg
        let q_total = 2.0e6; // W/m² sustained for 1 s
        let rate = tps_recession_rate_mps(q_total, tps_density, heat_of_ablation);
        assert!(rate > 0.0);
        let recession = rate * 1.0;
        // Nose radius grows by the recession (blunting).
        let nose_after = NOSE_M + recession;
        assert!(nose_after > NOSE_M);
        // Subsequent heating at the same flight condition is reduced by 1/√R.
        let rho = 1.0e-4;
        let v = 6_500.0;
        let q_before = convective_heat_flux_w_m2(K_CONV, rho, NOSE_M, v);
        let q_after = convective_heat_flux_w_m2(K_CONV, rho, nose_after, v);
        assert!(
            q_after < q_before,
            "blunted nose must reduce subsequent heating"
        );
        // Cumulative heat load would also have grown (the ECS integrates it);
        // here we validate the shape-change coupling.
    }

    #[test]
    fn tps_recession_stops_exactly_at_the_remaining_thickness() {
        let recession = capped_tps_recession_m(0.03, 1.0, 0.01);
        assert_eq!(recession, 0.01);
        assert_eq!(capped_tps_recession_m(0.03, 1.0, 0.0), 0.0);

        let mass_loss = tps_mass_loss_kg(1_500.0, 2.5, recession);
        assert!(mass_loss > 0.0);
        assert!(mass_loss.is_finite());
    }
}
