/// Lift force from asymmetric vacuum polarization (kN)
/// F_lift = 47.0 * dc^1.35, capped at 65 kN
pub fn lift_force(dc: f32) -> f32 {
    (47.0 * dc.clamp(0.0, 1.0).powf(1.35)).min(65.0)
}

/// Parametric gain factor from pulsed magnetic resonance.
/// Below 42% pulse: gain = 1.0
/// Above 42%: gain = 1.0 + (pulse - 0.42) * 2.6
pub fn parametric_gain(pulse: f32) -> f32 {
    if pulse > 0.42 {
        1.0 + (pulse - 0.42) * 2.6
    } else {
        1.0
    }
}

/// Duty cycle synergy factor: 1.0 + 0.4 * dc
pub fn duty_synergy(dc: f32) -> f32 {
    1.0 + 0.4 * dc.clamp(0.0, 1.0)
}

/// ZPE power extracted (kW)
/// P = 210 * pulse^1.8 * parametric_gain * duty_synergy, capped at 1250 kW
pub fn zpe_power(pulse: f32, dc: f32) -> f32 {
    let base = 210.0 * pulse.clamp(0.0, 1.0).powf(1.8);
    let boost = parametric_gain(pulse);
    let synergy = duty_synergy(dc);
    (base * boost * synergy).min(1250.0)
}

/// Whether parametric gain is active (pulse > 42%)
pub fn parametric_gain_active(pulse: f32) -> bool {
    pulse > 0.42
}

/// Vacuum polarization gradient at a distance from the hull (T/m).
/// G(d) = 12.0 * dc * exp(-d * 3.0)
/// where d is distance from hull surface in meters.
pub fn polarization_gradient(dc: f32, distance_from_hull: f32) -> f32 {
    let d = distance_from_hull.max(0.0);
    12.0 * dc.clamp(0.0, 1.0) * (-d * 3.0).exp()
}

/// Vacuum density modifier near a massive body.
/// rho = 1.0 + G * M / (c^2 * r)
/// where r is distance from body center (m), M is body mass (kg).
/// Returns relative density (1.0 = far space, >1.0 = near mass).
pub fn vacuum_density(mass_kg: f64, distance_m: f64) -> f64 {
    const G: f64 = 6.67430e-11;
    const C2: f64 = 8.987551787e16;
    let correction = G * mass_kg / (C2 * distance_m.max(1.0));
    (1.0 + correction).min(2.0)
}

/// Net energy harvested per pulse cycle (MJ).
pub fn pulse_energy_gain(pulse: f32, dc: f32, period_seconds: f32) -> f32 {
    zpe_power(pulse, dc) * period_seconds / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lift_at_reference_dc() {
        let lift = lift_force(0.38);
        // F_lift = 47.0 * 0.38^1.35
        // 0.38^1.35 = exp(1.35 * ln(0.38)) = exp(1.35 * -0.9676) = exp(-1.306) = 0.271
        // 47.0 * 0.271 = 12.74
        let expected = 12.26;
        assert!(
            (lift - expected).abs() < 1.0,
            "lift at dc=0.38: got {lift}, expected ~{expected}"
        );
    }

    #[test]
    fn test_lift_clamp_max() {
        assert!((lift_force(0.0) - 0.0).abs() < 0.01);
        // Clamped internally: dc above 1 raises lift beyond 65, then clamped
        assert!(lift_force(2.0) <= 65.0);
    }

    #[test]
    fn test_zpe_at_reference() {
        let power = zpe_power(0.5, 0.38);
        // base = 210 * 0.5^1.8 = 210 * 0.287 = 60.3
        // parametric_boost = 1.0 + 0.08 * 2.6 = 1.208
        // synergy = 1.0 + 0.4 * 0.38 = 1.152
        // P = 60.3 * 1.208 * 1.152 = 83.9
        assert!(
            power > 50.0 && power < 200.0,
            "zpe at pulse=0.5, dc=0.38: got {power}, expected ~84"
        );
    }

    #[test]
    fn test_parametric_gain_threshold() {
        assert!((parametric_gain(0.42) - 1.0).abs() < 0.01);
        assert!(parametric_gain(0.5) > 1.2);
        assert!((parametric_gain(0.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_polarization_gradient_decay() {
        let at_hull = polarization_gradient(1.0, 0.0);
        assert!(at_hull > 10.0);
        let far = polarization_gradient(1.0, 2.0);
        assert!(far < at_hull * 0.01);
    }

    #[test]
    fn test_vacuum_density_sanity() {
        // Near Earth surface (r ~ 6.37e6 m, M ~ 5.97e24 kg)
        // Correction = G*M/(c^2*r) ~ 7e-10, so density ≈ 1.0000000007
        let near_earth = vacuum_density(5.972e24, 6.371e6);
        assert!(
            near_earth > 1.0,
            "near Earth density should be > 1.0, got {near_earth}"
        );
        assert!(
            (near_earth - 1.0) < 1e-6,
            "correction should be tiny, got {}",
            near_earth - 1.0
        );
        // Far space (1e15 m ~ 0.1 ly)
        let far = vacuum_density(5.972e24, 1.0e15);
        assert!((far - 1.0).abs() < 1e-20);
        // Near a neutron star proxy: M=2e30 kg, r=1e4 m
        let near_ns = vacuum_density(2.0e30, 1.0e4);
        assert!(
            near_ns > 1.001,
            "near neutron star density should show correction, got {near_ns}"
        );
    }

    #[test]
    fn test_zpe_never_negative() {
        for pulse in [0.0, 0.1, 0.5, 1.0] {
            for dc in [0.0, 0.2, 0.5, 0.8] {
                assert!(zpe_power(pulse, dc) >= 0.0);
            }
        }
    }

    #[test]
    fn test_lift_monotonic() {
        let mut prev = 0.0;
        for i in 0..=10 {
            let dc = i as f32 / 10.0;
            let lift = lift_force(dc);
            assert!(
                lift >= prev,
                "lift not monotonic at dc={dc}: {lift} < {prev}"
            );
            prev = lift;
        }
    }
}
