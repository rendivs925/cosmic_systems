//! Deterministic-regression toolkit (AGENTS.md section 46).
//!
//! This module keeps the *machinery* for the baseline-flight regression suite
//! Bevy-free so it is unit-testable without launching an app:
//!
//! - [`RegressionConfig`]: per-variable numerical tolerances (position, 1 mm;
//!   velocity, 1 µm/s; attitude, 1 µrad; mass, 1 mg) with an exact-compare
//!   escape hatch for the discrete guidance mode.
//! - [`RocketStateSample`]: the atomic set of authoritative state hashed
//!   bit-for-bit (position, velocity, orientation, angular velocity, mass,
//!   guidance mode). Stored as flat `f64` arrays so it is serde/ron friendly
//!   and independent of Bevy's serde feature.
//! - [`Divergence`] + [`compare_sample`]/[`compare_trajectory`]: report the
//!   specific flight, timestep, and state variable that diverges, with the
//!   expected/actual values and the tolerance that was violated — the payload
//!   the CI gate renders on a failing baseline.
//! - [`FlightBaseline`] + [`BaselineJustification`]: the review trail that
//!   must accompany an intentional baseline update (see design.md: what
//!   changed, the expected numerical/enablement improvement, the trade-off,
//!   affected scenarios, reviewer approval).
//!
//! The ECS adapter that runs a headless flight and produces samples lives in
//! the infrastructure layer; it converts Bevy `DVec3`/`DQuat` into these
//! flat arrays here. State hashing uses FNV-1a over the raw little-endian bit
//! patterns of the f64 fields, so identical physics ⇒ identical hash and any
//! floating-point drift flips the chain. Guidance mode is hashed as the code
//! byte so a policy change is caught as deterministically as a numeric one.

use ron::de::from_str as from_ron;
use ron::ser::{to_string_pretty, PrettyConfig};
use serde::{Deserialize, Serialize};

/// FNV-1a offset basis and prime (64-bit). Chosen for speed and stability —
/// it is not cryptographic; the goal is a cheap, deterministic fingerprint.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The categorical state variables compared across a baseline flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegressionVariable {
    Position,
    Velocity,
    Attitude,
    AngularVelocity,
    Mass,
    GuidanceMode,
    /// Total number of recorded samples; a length mismatch is a divergence.
    SampleCount,
}

impl RegressionVariable {
    pub const ALL: [RegressionVariable; 6] = [
        RegressionVariable::Position,
        RegressionVariable::Velocity,
        RegressionVariable::Attitude,
        RegressionVariable::AngularVelocity,
        RegressionVariable::Mass,
        RegressionVariable::GuidanceMode,
    ];

    /// Human-readable label used in CI divergence reports.
    pub fn label(self) -> &'static str {
        match self {
            RegressionVariable::Position => "position",
            RegressionVariable::Velocity => "velocity",
            RegressionVariable::Attitude => "attitude",
            RegressionVariable::AngularVelocity => "angular_velocity",
            RegressionVariable::Mass => "mass",
            RegressionVariable::GuidanceMode => "guidance_mode",
            RegressionVariable::SampleCount => "sample_count",
        }
    }

    /// Default per-variable tolerance (design.md): position 1 mm, velocity
    /// 1 µm/s, attitude 1 µrad, angular velocity 1 µrad/s, mass 1 mg.
    /// Guidance mode is discrete and compared exactly (`0.0` tolerance).
    pub fn default_tolerance(self) -> f64 {
        match self {
            RegressionVariable::Position => 1.0e-3,
            RegressionVariable::Velocity => 1.0e-6,
            RegressionVariable::Attitude => 1.0e-6,
            RegressionVariable::AngularVelocity => 1.0e-6,
            RegressionVariable::Mass => 1.0e-6,
            RegressionVariable::GuidanceMode => 0.0,
            RegressionVariable::SampleCount => 0.0,
        }
    }
}

/// Per-variable comparison tolerances, the "regression config file" of the
/// spec. `0.0` meansexact (bitwise-equal) comparison — used for discrete
/// state such as the guidance mode code, and available for any value a team
/// wants to pin exactly.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RegressionConfig {
    pub position_m: f64,
    pub velocity_mps: f64,
    pub attitude_rad: f64,
    pub angular_velocity_radps: f64,
    pub mass_kg: f64,
    pub guidance_mode: f64,
}

impl Default for RegressionConfig {
    fn default() -> Self {
        Self {
            position_m: RegressionVariable::Position.default_tolerance(),
            velocity_mps: RegressionVariable::Velocity.default_tolerance(),
            attitude_rad: RegressionVariable::Attitude.default_tolerance(),
            angular_velocity_radps: RegressionVariable::AngularVelocity.default_tolerance(),
            mass_kg: RegressionVariable::Mass.default_tolerance(),
            guidance_mode: RegressionVariable::GuidanceMode.default_tolerance(),
        }
    }
}

impl RegressionConfig {
    /// Tolerance for a given variable.
    pub fn tolerance(&self, variable: RegressionVariable) -> f64 {
        match variable {
            RegressionVariable::Position => self.position_m,
            RegressionVariable::Velocity => self.velocity_mps,
            RegressionVariable::Attitude => self.attitude_rad,
            RegressionVariable::AngularVelocity => self.angular_velocity_radps,
            RegressionVariable::Mass => self.mass_kg,
            RegressionVariable::GuidanceMode => self.guidance_mode,
            RegressionVariable::SampleCount => 0.0,
        }
    }

    /// All variables with strict (exact) tolerance, for reporting.
    pub fn exact_variables(&self) -> Vec<RegressionVariable> {
        RegressionVariable::ALL
            .into_iter()
            .filter(|v| self.tolerance(*v) == 0.0)
            .collect()
    }
}

/// Atomic authorized-cosmic state captured at a single fixed physics tick.
/// Stored as flat `f64` arrays (not Bevy math types) so it can be hashed,
/// serialized to RON, and compared without depending on Bevy's serde feature
/// or on the order/layout of a single entity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RocketStateSample {
    /// Planet-centered inertial position, meters `[x, y, z]`.
    pub position_m: [f64; 3],
    /// Planet-centered inertial velocity, m/s `[x, y, z]`.
    pub velocity_mps: [f64; 3],
    /// Body→world orientation quaternion `[x, y, z, w]`.
    pub orientation: [f64; 4],
    /// Body-frame angular velocity, rad/s `[x, y, z]`.
    pub angular_velocity_radps: [f64; 3],
    /// Total mass, kg.
    pub mass_kg: f64,
    /// Discrete guidance/mission mode code (see the ECS adapter mapping).
    pub guidance_code: u8,
}

impl RocketStateSample {
    /// Build a sample from its raw components.
    pub fn new(
        position_m: [f64; 3],
        velocity_mps: [f64; 3],
        orientation: [f64; 4],
        angular_velocity_radps: [f64; 3],
        mass_kg: f64,
        guidance_code: u8,
    ) -> Self {
        Self {
            position_m,
            velocity_mps,
            orientation,
            angular_velocity_radps,
            mass_kg,
            guidance_code,
        }
    }

    /// A canonical all-zero sample (useful for baseline capture at t=0 before
    /// any state is set, and as the identity in tests).
    pub fn zero() -> Self {
        Self {
            position_m: [0.0; 3],
            velocity_mps: [0.0; 3],
            orientation: [0.0, 0.0, 0.0, 1.0],
            angular_velocity_radps: [0.0; 3],
            mass_kg: 0.0,
            guidance_code: 0,
        }
    }

    /// Bitwise FNV-1a fingerprint over the sample's raw state. Two samples
    /// from identical physics produce identical hashes; any floating-point
    /// drift flips the value.
    pub fn hash(&self) -> u64 {
        let mut hash = FNV_OFFSET;
        for &v in self
            .position_m
            .iter()
            .chain(self.velocity_mps.iter())
            .chain(self.angular_velocity_radps.iter())
        {
            hash = hash_bytes(hash, &v.to_le_bytes());
        }
        for &q in self.orientation.iter() {
            hash = hash_bytes(hash, &q.to_le_bytes());
        }
        hash = hash_bytes(hash, &self.mass_kg.to_le_bytes());
        hash = hash_bytes(hash, &[self.guidance_code]);
        hash
    }
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// A single reported divergence between the baseline and the current run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Divergence {
    /// Fixed-physics tick at which the difference was observed.
    pub tick: usize,
    /// The state variable that diverged.
    pub variable: RegressionVariable,
    /// Baseline value rendered as a comparable scalar for diagnostics.
    pub baseline_value: f64,
    /// Current value rendered as a comparable scalar.
    pub current_value: f64,
    /// Signed difference `current - baseline`.
    pub delta: f64,
    /// Tolerance that was violated (the threshold at which the value is
    /// still considered "in spec").
    pub tolerance: f64,
}

impl Divergence {
    /// Renders a single-line CI report entry.
    pub fn describe(&self) -> String {
        format!(
            "tick {tick}: {name} diverged: baseline={base:e} current={cur:e} delta={delta:e} \
             (tolerance {tol:e})",
            tick = self.tick,
            name = self.variable.label(),
            base = self.baseline_value,
            cur = self.current_value,
            delta = self.delta,
            tol = self.tolerance,
        )
    }
}

/// Maximum component-wise absolute difference between two `[f64; 3]` vectors.
pub fn vector_abs_diff(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0_f64, f64::max)
}

/// Angle (radians) between two unit quaternions, in `[0, π]`. Uses
/// `2·acos(|dot|)` so `q` and `-q` (same rotation) compare as zero.
pub fn quaternion_angle_diff(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let dot = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]).abs();
    2.0 * dot.clamp(0.0, 1.0).acos()
}

/// Compare one baseline/current sample pair against the tolerance config,
/// returning every variable that exceeds it. Returns an empty `Vec<T>` when
/// the pair matches within tolerances.
pub fn compare_sample(
    baseline: &RocketStateSample,
    current: &RocketStateSample,
    config: &RegressionConfig,
    tick: usize,
) -> Vec<Divergence> {
    let mut divergences = Vec::new();

    let position_delta = vector_abs_diff(&baseline.position_m, &current.position_m);
    if divergence_exceeded(
        position_delta,
        config.tolerance(RegressionVariable::Position),
    ) {
        divergences.push(Divergence {
            tick,
            variable: RegressionVariable::Position,
            baseline_value: vector_magnitude(&baseline.position_m),
            current_value: vector_magnitude(&current.position_m),
            delta: position_delta,
            tolerance: config.tolerance(RegressionVariable::Position),
        });
    }

    let velocity_delta = vector_abs_diff(&baseline.velocity_mps, &current.velocity_mps);
    if divergence_exceeded(
        velocity_delta,
        config.tolerance(RegressionVariable::Velocity),
    ) {
        divergences.push(Divergence {
            tick,
            variable: RegressionVariable::Velocity,
            baseline_value: vector_magnitude(&baseline.velocity_mps),
            current_value: vector_magnitude(&current.velocity_mps),
            delta: velocity_delta,
            tolerance: config.tolerance(RegressionVariable::Velocity),
        });
    }

    let attitude_delta = quaternion_angle_diff(&baseline.orientation, &current.orientation);
    if divergence_exceeded(
        attitude_delta,
        config.tolerance(RegressionVariable::Attitude),
    ) {
        divergences.push(Divergence {
            tick,
            variable: RegressionVariable::Attitude,
            baseline_value: 0.0,
            current_value: attitude_delta,
            delta: attitude_delta,
            tolerance: config.tolerance(RegressionVariable::Attitude),
        });
    }

    let angular_velocity_delta = vector_abs_diff(
        &baseline.angular_velocity_radps,
        &current.angular_velocity_radps,
    );
    if divergence_exceeded(
        angular_velocity_delta,
        config.tolerance(RegressionVariable::AngularVelocity),
    ) {
        divergences.push(Divergence {
            tick,
            variable: RegressionVariable::AngularVelocity,
            baseline_value: vector_magnitude(&baseline.angular_velocity_radps),
            current_value: vector_magnitude(&current.angular_velocity_radps),
            delta: angular_velocity_delta,
            tolerance: config.tolerance(RegressionVariable::AngularVelocity),
        });
    }

    let mass_delta = (baseline.mass_kg - current.mass_kg).abs();
    if divergence_exceeded(mass_delta, config.tolerance(RegressionVariable::Mass)) {
        divergences.push(Divergence {
            tick,
            variable: RegressionVariable::Mass,
            baseline_value: baseline.mass_kg,
            current_value: current.mass_kg,
            delta: mass_delta,
            tolerance: config.tolerance(RegressionVariable::Mass),
        });
    }

    if baseline.guidance_code != current.guidance_code {
        divergences.push(Divergence {
            tick,
            variable: RegressionVariable::GuidanceMode,
            baseline_value: baseline.guidance_code as f64,
            current_value: current.guidance_code as f64,
            delta: 1.0,
            tolerance: config.tolerance(RegressionVariable::GuidanceMode),
        });
    }

    divergences
}

fn divergence_exceeded(delta: f64, tolerance: f64) -> bool {
    if tolerance == 0.0 {
        delta > 0.0
    } else {
        delta > tolerance
    }
}

fn vector_magnitude(v: &[f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Compare two entire trajectories (baseline vs current), returning every
/// divergence at every matching tick plus a length-mismatch divergence if the
/// sample counts differ. Ticks align by index — both recordings must be at
/// the same fixed physics timestep.
pub fn compare_trajectory(
    baseline: &[RocketStateSample],
    current: &[RocketStateSample],
    config: &RegressionConfig,
) -> Vec<Divergence> {
    let mut divergences = Vec::new();
    let common_len = baseline.len().min(current.len());

    for tick in 0..common_len {
        divergences.extend(compare_sample(
            &baseline[tick],
            &current[tick],
            config,
            tick,
        ));
    }

    if baseline.len() != current.len() {
        divergences.push(Divergence {
            tick: common_len,
            variable: RegressionVariable::SampleCount,
            baseline_value: baseline.len() as f64,
            current_value: current.len() as f64,
            delta: baseline.len().abs_diff(current.len()) as f64,
            tolerance: 0.0,
        });
    }

    divergences
}

/// The justification that must accompany an intentional baseline update
/// (design.md: "physics change audit trail"). Recording a new baseline
/// without describing the expected improvement, the numerical trade-offs and
/// the affected scenarios is rejected by [`FlightBaseline::validate_audit`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BaselineJustification {
    /// Free-text description of the physics/software change.
    pub change_description: String,
    /// Expected improvement this change buys (accuracy, stability, reach).
    pub expected_improvement: String,
    /// Numerical trade-offs accepted (integration order, coarser step at
    /// time-warp, tolerance changes, etc.).
    pub numerical_tradeoffs: String,
    /// The flight scenarios whose baselines are re-recorded.
    pub affected_scenarios: Vec<String>,
    /// Reviewer sign-off. Baselines are immutable once approved; a new one
    /// requires a fresh approval.
    pub reviewer_approved: bool,
    /// Author identifier (team/git author) that ran the re-record.
    pub recorded_by: String,
}

impl BaselineJustification {
    /// True when the audit trail is complete enough to publish a baseline.
    pub fn is_signed_off(&self) -> bool {
        self.reviewer_approved
            && !self.change_description.trim().is_empty()
            && !self.expected_improvement.trim().is_empty()
            && !self.numerical_tradeoffs.trim().is_empty()
            && !self.affected_scenarios.is_empty()
    }
}

/// A canonical recorded flight: the full sample sequence at the fixed physics
/// timestep, an FNV-1a hash chain (one hash per tick, compact and cheap to
/// store/commit), and the provenance needed for the audit trail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightBaseline {
    /// Identifier, e.g. `ascent`, `leo-insertion`, `reentry`, `rtls-recovery`.
    pub name: String,
    /// Git commit the baseline was recorded against.
    pub git_commit: String,
    /// The signed-off justification for why this revision of the baseline
    /// exists (the audit trail entry).
    pub audit: BaselineJustification,
    /// Per-tick authoritative samples at the fixed physics timestep.
    pub samples: Vec<RocketStateSample>,
    /// Per-tick bitwise hash chain derived from `samples`.
    pub hash_chain: Vec<u64>,
}

impl FlightBaseline {
    /// Build a baseline from a recorded sample sequence, computing the hash
    /// chain. The `audit` must already be signed off.
    pub fn record(
        name: impl Into<String>,
        git_commit: impl Into<String>,
        audit: BaselineJustification,
        samples: Vec<RocketStateSample>,
    ) -> Result<Self, String> {
        if !audit.is_signed_off() {
            return Err("baseline audit trail is not signed off".to_string());
        }
        let hash_chain = samples.iter().map(|s| s.hash()).collect();
        Ok(Self {
            name: name.into(),
            git_commit: git_commit.into(),
            audit,
            samples,
            hash_chain,
        })
    }

    /// Rebuild the hash chain from the stored samples (defense against a
    /// tampered fixture).
    pub fn recompute_hash_chain(&self) -> Vec<u64> {
        self.samples.iter().map(|s| s.hash()).collect()
    }

    /// Verify the stored hash chain is internally consistent — the fixture is
    /// a recording of deterministic physics.
    pub fn hash_chain_consistent(&self) -> bool {
        self.hash_chain == self.recompute_hash_chain()
    }

    /// Compare a freshly simulated trajectory against this baseline.
    pub fn compare(
        &self,
        current: &[RocketStateSample],
        config: &RegressionConfig,
    ) -> Vec<Divergence> {
        compare_trajectory(&self.samples, current, config)
    }
}

/// Parse a [`FlightBaseline`] from a RON document.
pub fn load_baseline_ron(ron_text: &str) -> Result<FlightBaseline, String> {
    from_ron(ron_text).map_err(|e| format!("failed to parse baseline RON: {e}"))
}

/// Serialize a [`FlightBaseline`] to a RON document.
pub fn save_baseline_ron(baseline: &FlightBaseline) -> Result<String, String> {
    to_string_pretty(baseline, PrettyConfig::default())
        .map_err(|e| format!("failed to serialize baseline: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(pos: [f64; 3], mass: f64, code: u8) -> RocketStateSample {
        RocketStateSample::new(
            pos,
            [10.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [0.1, 0.0, 0.0],
            mass,
            code,
        )
    }

    #[test]
    fn identical_samples_hash_bitwise_equal() {
        let a = sample([1.0, 2.0, 3.0], 12_000.0, 2);
        let b = sample([1.0, 2.0, 3.0], 12_000.0, 2);
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn single_bit_flip_changes_hash() {
        let a = sample([1.0, 2.0, 3.0], 12_000.0, 2);
        let mut b = a;
        // Flip one mantissa bit of the mass (keeps the exponent, so the value
        // is finite and near 12 000 kg while being a genuinely different f64).
        let flipped = f64::from_bits(b.mass_kg.to_bits() ^ 1);
        b.mass_kg = flipped;
        assert_ne!(a.mass_kg, b.mass_kg);
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn quaternion_sign_equivalence_has_same_rotation() {
        let a = RocketStateSample::new([0.0; 3], [0.0; 3], [0.0, 0.0, 0.0, 1.0], [0.0; 3], 1.0, 0);
        let neg =
            RocketStateSample::new([0.0; 3], [0.0; 3], [0.0, 0.0, 0.0, -1.0], [0.0; 3], 1.0, 0);
        assert_eq!(quaternion_angle_diff(&a.orientation, &neg.orientation), 0.0);
    }

    #[test]
    fn default_tolerances_match_spec() {
        let cfg = RegressionConfig::default();
        assert_eq!(cfg.position_m, 1.0e-3);
        assert_eq!(cfg.velocity_mps, 1.0e-6);
        assert_eq!(cfg.attitude_rad, 1.0e-6);
        assert_eq!(cfg.mass_kg, 1.0e-6);
        assert_eq!(cfg.tolerance(RegressionVariable::GuidanceMode), 0.0);
    }

    #[test]
    fn within_tolerance_produces_no_divergence() {
        let cfg = RegressionConfig::default();
        let base = sample([6_371_000.0, 0.0, 0.0], 100_000.0, 3);
        let mut cur = base;
        cur.position_m[0] += cfg.position_m * 0.99;
        cur.mass_kg += cfg.mass_kg * 0.99;
        assert!(compare_sample(&base, &cur, &cfg, 42).is_empty());
    }

    #[test]
    fn exceeding_tolerance_reports_variable_and_tick() {
        let cfg = RegressionConfig::default();
        let base = sample([6_371_000.0, 0.0, 0.0], 100_000.0, 3);
        let mut cur = base;
        cur.position_m[0] += cfg.position_m * 2.0; // 2 mm > 1 mm
        let divergences = compare_sample(&base, &cur, &cfg, 7);
        assert_eq!(divergences.len(), 1);
        let d = &divergences[0];
        assert_eq!(d.variable, RegressionVariable::Position);
        assert_eq!(d.tick, 7);
        assert!(d.delta > d.tolerance);
        assert!(d.describe().contains("position"));
    }

    #[test]
    fn mass_budget_divergence_reported_exactly() {
        let cfg = RegressionConfig::default();
        let base = sample([0.0; 3], 100_000.0, 1);
        let mut cur = base;
        cur.mass_kg = 99_900.0;
        let d = compare_sample(&base, &cur, &cfg, 3);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].variable, RegressionVariable::Mass);
        assert_eq!(d[0].delta, 100.0);
    }

    #[test]
    fn guidance_mode_change_is_exact_divergence() {
        let cfg = RegressionConfig::default();
        let base = sample([0.0; 3], 1.0, 1);
        let cur = sample([0.0; 3], 1.0, 2);
        let d = compare_sample(&base, &cur, &cfg, 0);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].variable, RegressionVariable::GuidanceMode);
    }

    #[test]
    fn length_mismatch_is_reported() {
        let cfg = RegressionConfig::default();
        let a = vec![sample([0.0; 3], 1.0, 0); 10];
        let b = vec![sample([0.0; 3], 1.0, 0); 12];
        let d = compare_trajectory(&a, &b, &cfg);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].variable, RegressionVariable::SampleCount);
        assert_eq!(d[0].delta, 2.0);
    }

    #[test]
    fn trajectory_compare_finds_divergent_tick() {
        let cfg = RegressionConfig::default();
        let mut baseline = Vec::new();
        let mut current = Vec::new();
        for i in 0..5 {
            baseline.push(sample([i as f64, 0.0, 0.0], 1.0, 0));
            current.push(sample([i as f64, 0.0, 0.0], 1.0, 0));
        }
        current[3].velocity_mps = [999.0, 0.0, 0.0];
        let d = compare_trajectory(&baseline, &current, &cfg);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].tick, 3);
        assert_eq!(d[0].variable, RegressionVariable::Velocity);
    }

    #[test]
    fn baseline_ron_round_trips() {
        let audit = BaselineJustification {
            change_description: "switch to symplectic integration".into(),
            expected_improvement: "energy conservation".into(),
            numerical_tradeoffs: "2nd order error, no closed-form".into(),
            affected_scenarios: vec!["ascent".into(), "reentry".into()],
            reviewer_approved: true,
            recorded_by: "opencode".into(),
        };
        let base = FlightBaseline::record(
            "ascent",
            "abc123",
            audit.clone(),
            vec![sample([1.0, 0.0, 0.0], 14_000.0, 1)],
        )
        .expect("signed-off baseline records");
        assert!(base.hash_chain_consistent());
        let ron_text = save_baseline_ron(&base).unwrap();
        let loaded = load_baseline_ron(&ron_text).unwrap();
        assert_eq!(loaded.name, base.name);
        assert_eq!(loaded.git_commit, base.git_commit);
        assert_eq!(loaded.samples, base.samples);
        assert_eq!(loaded.hash_chain, base.hash_chain);
        assert_eq!(loaded.audit, audit);
    }

    #[test]
    fn unsigned_baseline_is_rejected() {
        let audit = BaselineJustification::default(); // not signed off
        let result = FlightBaseline::record("ascent", "abc", audit, Vec::new());
        assert!(result.is_err());
    }

    #[test]
    fn signed_off_requires_complete_audit() {
        let audit = BaselineJustification {
            reviewer_approved: true,
            ..Default::default()
        };
        assert!(!audit.is_signed_off());
        let full = BaselineJustification {
            change_description: "x".into(),
            expected_improvement: "y".into(),
            numerical_tradeoffs: "z".into(),
            affected_scenarios: vec!["a".into()],
            reviewer_approved: true,
            recorded_by: "opencode".into(),
        };
        assert!(full.is_signed_off());
    }

    #[test]
    fn mock_baseline_detects_injected_regression() {
        // The CI gate's core loop: a signed-off baseline, then a re-simulation
        // with a single injected unit change must be caught at the exact tick
        // and variable.
        let audit = BaselineJustification {
            change_description: "baseline for ascent".into(),
            expected_improvement: "capture canonical reference".into(),
            numerical_tradeoffs: "none".into(),
            affected_scenarios: vec!["ascent".into()],
            reviewer_approved: true,
            recorded_by: "opencode".into(),
        };
        let mut samples = Vec::new();
        for i in 0..100 {
            samples.push(sample(
                [i as f64 * 1.0, 0.0, 0.0],
                14_000.0 - i as f64 * 20.0,
                1,
            ));
        }
        let baseline = FlightBaseline::record("ascent", "abc", audit, samples.clone()).unwrap();

        // Injected physics regression: one tick drifts 1 mm in position.
        let mut regressed = samples.clone();
        regressed[50].position_m[0] += 1.0e-3 + 1.0e-9;
        let d = baseline.compare(&regressed, &RegressionConfig::default());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].tick, 50);
        assert_eq!(d[0].variable, RegressionVariable::Position);
    }
}
