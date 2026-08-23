//! Landing-gear physics: deployment sequencing, leg spring-damper contact,
//! tip-over stability, and energy absorption limits.
//!
//! Pure f64 domain models (AGENTS.md section 3); Bevy systems in
//! `bevy_adapters::rocket_systems` adapt them into ECS execution. The single
//! authority for ground-contact resolution stays in
//! [`crate::domain::services::terrain_collision`] and the GroundContact set:
//! this module provides the gear-specific math those systems consume.
//!
//! ## Models
//!
//! - Deployment: a one-way latch that opens when the vehicle descends through
//!   the configured radar-altitude gate. Legs cannot retract in flight.
//! - Leg springs: each leg is a linear spring-damper with stiffness sized so
//!   that (a) static weight compresses it to half stroke and (b) a touchdown
//!   at the design speed stops within the full stroke. The damper uses a
//!   sub-critical damping ratio so the explicit per-tick force application is
//!   stable at the 64 Hz fixed step (`c·dt/m ≪ 2`).
//! - Tip-over: quasi-static criterion — the vehicle topples when the center-
//!   of-mass projection leaves the support polygon formed by the leg bases.

use bevy::math::DVec3;

use crate::domain::services::terrain_collision::TouchdownCriteria;

/// Touchdown vertical speed the gear is sized for (m/s). Matches
/// [`crate::domain::services::terrain_collision::TouchdownCriteria::default()
/// .max_vertical_speed_mps`] so criteria and gear capacity agree by design.
pub const DESIGN_TOUCHDOWN_SPEED_MPS: f64 = 5.0;

/// Damping ratio of the leg struts. Sub-critical: absorbs the impact while
/// keeping the explicit integrator comfortably inside its stability region.
pub const LEG_DAMPING_RATIO: f64 = 0.5;

/// Static-load ride height as a fraction of stroke: full-weight compression
/// may use up to this share of the stroke before the spring is considered
/// undersized.
pub const STATIC_RIDE_HEIGHT_FRACTION: f64 = 0.5;

/// Standard gravity used by the static sag requirement, m/s².
const STANDARD_GRAVITY_MPS2: f64 = 9.80665;

/// Static configuration of a vehicle's landing gear (from the RON catalog).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandingGearSpec {
    /// Number of legs forming the support polygon.
    pub count: u32,
    /// Distance from the body axis to each leg's foot pad, meters.
    pub base_radius_m: f64,
    /// Maximum leg compression travel, meters.
    pub stroke_m: f64,
    /// Maximum mass (kg) the gear can land; `None` means "the whole vehicle".
    pub max_landing_mass_kg: Option<f64>,
    /// Radar altitude (m) at which the legs deploy during descent.
    pub deploy_altitude_m: f64,
}

impl LandingGearSpec {
    /// Design landing mass: the configured limit, defaulting to the gross
    /// vehicle mass when the config omits `max_landing_mass_kg`.
    pub fn design_mass_kg(&self, gross_vehicle_mass_kg: f64) -> f64 {
        self.max_landing_mass_kg.unwrap_or(gross_vehicle_mass_kg)
    }

    /// Quasi-static tip-over criterion: the vehicle topples when its
    /// center-of-mass projection leaves the support polygon. For legs evenly
    /// spread on the base circle the polygon inscribes that circle closely
    /// enough for the conservative bound used here:
    /// tips over when `tan(tilt) > base_radius / com_height`.
    ///
    /// Boundary is exclusive: tilt exactly equal to the critical angle is
    /// still stable (the CoM projection sits exactly above the foot line).
    /// Degenerate geometry (no radius or no CoM height) cannot stand at all.
    pub fn tips_over(&self, tilt_deg: f64, com_height_m: f64) -> bool {
        if self.base_radius_m <= 0.0 || com_height_m <= 0.0 {
            return true;
        }
        let critical_rad = self.base_radius_m.atan2(com_height_m);
        tilt_deg.to_radians() > critical_rad
    }
}

/// Linear spring-damper parameters shared by every leg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegSpringConfig {
    /// Spring stiffness, N/m.
    pub stiffness_n_per_m: f64,
    /// Viscous damping coefficient, N·s/m.
    pub damping_n_s_per_m: f64,
}

/// A vehicle's complete landing-gear assembly: the configured spec plus the
/// strut springs sized from it. Construct once (at spawn), then query — all
/// gear math hangs off this type so there is one authority per quantity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandingGear {
    pub spec: LandingGearSpec,
    pub spring: LegSpringConfig,
}

impl LandingGear {
    /// Build an assembly with springs sized from the design landing
    /// condition. Stiffness is the larger of:
    /// - static requirement: full design mass at rest compresses the strut
    ///   to [`STATIC_RIDE_HEIGHT_FRACTION`] of stroke, `k = m·g/(f·x_max)`,
    /// - dynamic requirement: stopping `v_design` within the full stroke
    ///   using spring work alone, `k = m·v²/x_max²`.
    ///
    /// Damping is critically-scaled: `c = 2ζ√(k·m)` with ζ =
    /// [`LEG_DAMPING_RATIO`].
    pub fn new(spec: LandingGearSpec, gross_vehicle_mass_kg: f64) -> Self {
        let m = spec.design_mass_kg(gross_vehicle_mass_kg).max(1e-6);
        let x = spec.stroke_m.max(1e-6);
        let k_static = m * STANDARD_GRAVITY_MPS2 / (STATIC_RIDE_HEIGHT_FRACTION * x);
        let k_dynamic = m * DESIGN_TOUCHDOWN_SPEED_MPS * DESIGN_TOUCHDOWN_SPEED_MPS / (x * x);
        let k = k_static.max(k_dynamic);
        let c = 2.0 * LEG_DAMPING_RATIO * (k * m).sqrt();
        Self {
            spec,
            spring: LegSpringConfig {
                stiffness_n_per_m: k,
                damping_n_s_per_m: c,
            },
        }
    }

    /// Height of the center of mass above the foot plane while settled on
    /// deployed struts: hull half-height (CoM ≈ geometric center) plus the
    /// full stroke as ride height (documented approximation).
    pub fn com_height_on_gear_m(&self, hull_height_m: f64) -> f64 {
        hull_height_m / 2.0 + self.spec.stroke_m
    }

    /// Touchdown criteria adjusted for deployed gear: a stance wider than
    /// the CoM is tall absorbs proportionally more lateral drift before the
    /// vehicle topples, so the lateral limit scales with
    /// `(1 + base_radius/com_height)`; every other limit passes through
    /// unchanged.
    pub fn touchdown_criteria(
        &self,
        base: TouchdownCriteria,
        hull_height_m: f64,
    ) -> TouchdownCriteria {
        let stance_aspect =
            self.spec.base_radius_m / self.com_height_on_gear_m(hull_height_m).max(1e-6);
        TouchdownCriteria {
            max_lateral_speed_mps: base.max_lateral_speed_mps * (1.0 + stance_aspect),
            ..base
        }
    }

    /// Axial force along a compressed leg strut (N). A leg can only push:
    /// tension returns zero. `compression_rate_mps > 0` means the strut is
    /// being compressed (damper adds resistance).
    pub fn axial_force_n(&self, compression_m: f64, compression_rate_mps: f64) -> f64 {
        let x = compression_m.clamp(0.0, f64::MAX);
        (self.spring.stiffness_n_per_m * x + self.spring.damping_n_s_per_m * compression_rate_mps)
            .max(0.0)
    }

    /// True when the struts' spring work alone can absorb the kinetic energy
    /// of `mass_kg` at `speed_mps` within the full stroke. Damping is
    /// deliberately excluded: it is margin, not capacity.
    pub fn absorbs_touchdown_energy(&self, mass_kg: f64, speed_mps: f64) -> bool {
        let kinetic_j = 0.5 * mass_kg * speed_mps * speed_mps;
        let capacity_j = 0.5 * self.spring.stiffness_n_per_m * self.spec.stroke_m.powi(2);
        kinetic_j <= capacity_j
    }

    /// Resolve one fixed step of soft gear contact against flat ground whose
    /// surface normal is `surface_normal` (pointing away from the ground).
    ///
    /// Penalty-method formulation: `hull_penetration_m` is the MEASURED depth
    /// of the hull reference below the point-contact surface this tick
    /// (0 = struts fully extended, feet just touching). The strut responds
    /// with its spring-damper axial force along the normal, applied as a
    /// direct velocity impulse (`Δv = F·dt/m`) so GroundContact stays the
    /// single authority without coupling into the next tick's force
    /// accumulator. Penetration past the stroke flags rigid fallback instead
    /// of clamping here (no duplicated constraint code). Because compression
    /// is measured from actual geometry every tick, the contact is absolutely
    /// anchored — no state integration drift.
    pub fn resolve_contact_step(
        &self,
        velocity_mps: DVec3,
        surface_normal: DVec3,
        hull_penetration_m: f64,
        mass_kg: f64,
        dt_s: f64,
    ) -> GearContactOutcome {
        let n = surface_normal.normalize_or_zero();
        let normal_speed = velocity_mps.dot(n); // negative = into the ground
                                                // Signed strut rate: positive while compressing, negative while
                                                // extending. The axial-force model clamps at zero, so the damper
                                                // dissipates on both phases without ever pulling.
        let compression_rate = -normal_speed;

        let bottomed_out = hull_penetration_m > self.spec.stroke_m;
        let compression = hull_penetration_m.clamp(0.0, self.spec.stroke_m);

        let normal_force_n = if bottomed_out {
            0.0 // rigid fallback owns this step
        } else {
            self.axial_force_n(compression, compression_rate)
        };

        let mut velocity = velocity_mps;
        if normal_force_n > 0.0 && mass_kg > 0.0 {
            velocity += n * (normal_force_n * dt_s / mass_kg);
        }

        GearContactOutcome {
            velocity_mps: velocity,
            compression_m: compression,
            bottomed_out,
            normal_force_n,
        }
    }
}

/// One-way deployment latch (legs cannot retract once down).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LegDeploymentState {
    pub deployed: bool,
}

impl LegDeploymentState {
    /// Advance the latch: deploys when descending (`vertical_speed_mps < 0`)
    /// through the radar-altitude gate. Returns true on the transition tick.
    pub fn update(
        &mut self,
        deploy_gate_altitude_m: f64,
        radar_altitude_m: f64,
        vertical_speed_mps: f64,
    ) -> bool {
        if self.deployed {
            return false;
        }
        if radar_altitude_m <= deploy_gate_altitude_m && vertical_speed_mps < 0.0 {
            self.deployed = true;
            return true;
        }
        false
    }
}

/// Outcome of one step of gear-contact resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GearContactOutcome {
    /// Updated surface-relative velocity after the strut impulse.
    pub velocity_mps: DVec3,
    /// Strut compression for this step, meters (clamped to [0, stroke]).
    pub compression_m: f64,
    /// True when the hull sat deeper than the stroke allows this step: the
    /// caller must fall back to rigid point-contact resolution.
    pub bottomed_out: bool,
    /// Total axial strut force applied along the surface normal (N).
    pub normal_force_n: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 64.0;
    const STROKE: f64 = 3.0;
    const DESIGN_MASS: f64 = 30_000.0;

    fn gear() -> LandingGear {
        LandingGear::new(
            LandingGearSpec {
                count: 4,
                base_radius_m: 4.5,
                stroke_m: STROKE,
                max_landing_mass_kg: Some(DESIGN_MASS),
                deploy_altitude_m: 100.0,
            },
            DESIGN_MASS,
        )
    }

    #[test]
    fn spring_sizing_meets_static_and_dynamic_requirements() {
        let g = gear();
        // Static: full weight compresses to ≤ half stroke.
        let static_compression = DESIGN_MASS * 9.80665 / g.spring.stiffness_n_per_m;
        assert!(
            static_compression <= STATIC_RIDE_HEIGHT_FRACTION * STROKE + 1e-9,
            "static sag {static_compression} exceeds half stroke"
        );
        // Dynamic: design-speed kinetic energy fits within the full stroke.
        assert!(g.absorbs_touchdown_energy(DESIGN_MASS, DESIGN_TOUCHDOWN_SPEED_MPS));
    }

    #[test]
    fn axial_force_is_spring_plus_damper_and_never_pulls() {
        let g = gear();
        let expected = g.spring.stiffness_n_per_m * 1.0 + g.spring.damping_n_s_per_m * 2.0;
        assert!((g.axial_force_n(1.0, 2.0) - expected).abs() < 1e-9);
        // Extension (negative rate) reduces the force but never below zero.
        assert_eq!(g.axial_force_n(0.01, -1.0e9), 0.0);
        // Zero compression and zero rate: no force.
        assert_eq!(g.axial_force_n(0.0, 0.0), 0.0);
    }

    /// Shared 1-D vertical harness: weight each tick, integrate position,
    /// measure hull penetration below the surface, respond with the strut.
    struct Drop {
        altitude_m: f64, // + above the surface
        velocity: f64,   // + upward
        compression_m: f64,
        bottomed: bool,
    }

    impl Drop {
        fn new(start_altitude_m: f64, speed_mps: f64) -> Self {
            Self {
                altitude_m: start_altitude_m,
                velocity: -speed_mps,
                compression_m: 0.0,
                bottomed: false,
            }
        }

        fn step(&mut self, g: &LandingGear, mass_kg: f64) {
            // Weight, then position integration for this tick.
            self.velocity -= 9.80665 * DT;
            self.altitude_m += self.velocity * DT;
            let penetration = (-self.altitude_m).max(0.0);
            let out = g.resolve_contact_step(
                DVec3::new(0.0, self.velocity, 0.0),
                DVec3::Y,
                penetration,
                mass_kg,
                DT,
            );
            self.velocity = out.velocity_mps.y;
            self.compression_m = out.compression_m;
            self.bottomed = out.bottomed_out;
        }
    }

    #[test]
    fn gear_contact_stops_design_touchdown_within_stroke() {
        let g = gear();
        let mut d = Drop::new(2.0, DESIGN_TOUCHDOWN_SPEED_MPS);
        for _ in 0..512 {
            d.step(&g, DESIGN_MASS);
            assert!(!d.bottomed, "design touchdown must not bottom out");
            assert!(
                d.compression_m <= STROKE + 1e-12,
                "compression escaped stroke"
            );
        }
        // Settled near the static ride height with the motion arrested.
        assert!(
            d.velocity.abs() < 0.2,
            "vertical speed not arrested: {}",
            d.velocity
        );
        let static_sag = DESIGN_MASS * 9.80665 / g.spring.stiffness_n_per_m;
        assert!(
            (d.compression_m - static_sag).abs() < 0.25,
            "settled at {}, expected ~{static_sag}",
            d.compression_m
        );
    }

    /// Damping stability at the production timestep: the explicit impulse
    /// must not overshoot into oscillation growth on the lightest plausible
    /// vehicle (worst case for c·dt/m).
    #[test]
    fn gear_contact_is_damped_not_oscillating_at_64hz() {
        let g = gear();
        // Half-design mass: stiffest normalized damping case.
        let mass = DESIGN_MASS * 0.5;
        let mut d = Drop::new(2.0, DESIGN_TOUCHDOWN_SPEED_MPS);
        let mut max_rebound_up = 0.0_f64;
        for _ in 0..1024 {
            d.step(&g, mass);
            max_rebound_up = max_rebound_up.max(d.velocity);
        }
        // Rebound never exceeds the impact speed (no energy gain).
        assert!(
            max_rebound_up < DESIGN_TOUCHDOWN_SPEED_MPS,
            "strut gained energy: rebound {max_rebound_up}"
        );
        assert!(d.velocity.abs() < 0.3, "not settled: {}", d.velocity);
        assert!(
            d.compression_m > 0.0 && d.compression_m <= STROKE,
            "settled compression {} outside physical range",
            d.compression_m
        );
    }

    #[test]
    fn bottom_out_flags_rigid_fallback_instead_of_tunneling() {
        let g = gear();
        // Hull slammed far past the stroke in one tick.
        let out = g.resolve_contact_step(
            DVec3::new(0.0, -500.0, 0.0),
            DVec3::Y,
            STROKE * 1.5,
            DESIGN_MASS,
            DT,
        );
        assert!(out.bottomed_out, "must flag bottom-out beyond the stroke");
        assert_eq!(out.compression_m, STROKE);
        assert_eq!(out.normal_force_n, 0.0);
        assert_eq!(out.velocity_mps, DVec3::new(0.0, -500.0, 0.0));
    }

    #[test]
    fn extended_strut_above_ground_produces_no_force() {
        let g = gear();
        // Fully extended and clear of the ground.
        let out = g.resolve_contact_step(DVec3::new(0.0, 2.0, 0.0), DVec3::Y, 0.0, DESIGN_MASS, DT);
        assert_eq!(out.compression_m, 0.0);
        assert_eq!(out.normal_force_n, 0.0);
        assert_eq!(out.velocity_mps, DVec3::new(0.0, 2.0, 0.0));
        assert!(!out.bottomed_out);
    }

    #[test]
    fn tip_over_boundary_is_exclusive() {
        let spec = gear().spec;
        let com_h = 35.0_f64;
        let critical = spec.base_radius_m.atan2(com_h).to_degrees();
        assert!(critical > 0.0 && critical < 90.0);
        assert!(!spec.tips_over(critical, com_h));
        assert!(!spec.tips_over(critical - 0.1, com_h));
        assert!(spec.tips_over(critical + 0.1, com_h));
        // Vertical vehicle never tips.
        assert!(!spec.tips_over(0.0, com_h));
        // Degenerate geometry cannot stand at any tilt.
        let no_radius = LandingGearSpec {
            base_radius_m: 0.0,
            ..spec
        };
        assert!(no_radius.tips_over(1.0, com_h));
        assert!(spec.tips_over(1.0, 0.0));
    }

    #[test]
    fn energy_check_respects_capacity_margin() {
        let g = gear();
        assert!(g.absorbs_touchdown_energy(DESIGN_MASS, DESIGN_TOUCHDOWN_SPEED_MPS));
        // Static-load sizing dominates the stiffness here, so the pure
        // spring work covers the design speed with headroom; well beyond it
        // the capacity is exceeded (damping excluded from capacity by
        // definition).
        let capacity_speed = (g.spring.stiffness_n_per_m * STROKE * STROKE / DESIGN_MASS).sqrt();
        assert!(
            !g.absorbs_touchdown_energy(DESIGN_MASS, capacity_speed * 1.1),
            "energy beyond spring capacity must not fit the stroke"
        );
    }

    /// Gear-aware criteria widen only the lateral limit, proportionally to
    /// the stance aspect ratio; every other limit passes through unchanged.
    #[test]
    fn touchdown_criteria_widen_lateral_limit_by_stance_aspect() {
        let g = gear();
        let base = TouchdownCriteria::default();
        let hull_height = 70.0;
        let adjusted = g.touchdown_criteria(base, hull_height);

        let com_height = g.com_height_on_gear_m(hull_height);
        let aspect = g.spec.base_radius_m / com_height;
        assert!(
            (adjusted.max_lateral_speed_mps - base.max_lateral_speed_mps * (1.0 + aspect)).abs()
                < 1e-12
        );
        assert_eq!(adjusted.max_vertical_speed_mps, base.max_vertical_speed_mps);
        assert_eq!(adjusted.max_slope_deg, base.max_slope_deg);
        assert_eq!(adjusted.max_tilt_deg, base.max_tilt_deg);
    }

    #[test]
    fn deployment_latch_opens_once_through_the_gate() {
        let mut state = LegDeploymentState::default();
        // Ascending through the gate does not deploy.
        assert!(!state.update(100.0, 50.0, 30.0));
        assert!(!state.deployed);
        // Descending through the gate deploys exactly once.
        assert!(state.update(100.0, 50.0, -20.0));
        assert!(state.deployed);
        // Latched: further updates are no-ops.
        assert!(!state.update(100.0, 50.0, -20.0));
        assert!(!state.update(100.0, 10_000.0, -200.0));
    }
}
