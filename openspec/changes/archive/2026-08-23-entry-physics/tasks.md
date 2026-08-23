# Entry Physics — Implementation Status

All systems below are implemented, wired into the `RocketSet::EntryPhysics`
FixedUpdate set, and covered by unit tests. Implemented across the original
entry-physics work plus Phase 5–7 wiring; this list is kept as the record of
what shipped.

## 1. Components & Config

- [x] 1.1 Add `ThermalState`, `AblationState`, `ParachuteState` components
- [x] 1.2 Add `EntryPhysicsConfig` resource with per-body defaults (Earth, Moon, Mars)
- [x] 1.3 Add per-body parachute configuration (`EntryPhysicsConfig::parachute_config`)

## 2. Convective Heating System

- [x] 2.1 Create `compute_heating` system in `FixedUpdate` (EntryPhysics set)
- [x] 2.2 Sutton-Graves convective heat flux: `q_dot = k * sqrt(rho/R_nose) * v^3`
- [x] 2.3 Tauber-Sutton radiative heat flux for v > 10 km/s
- [x] 2.4 Total heat flux written to `ThermalState` component
- [x] 2.5 Heating behavior covered by entry-physics/parachute unit tests
- [x] 2.6 Nose-radius coupling via ablation-blunted coefficient model

## 3. Ablation System

- [x] 3.1 Create `compute_ablation` system (EntryPhysics set)
- [x] 3.2 Char-layer recession: `dr/dt = q_dot / (rho_tps * H_abl)`
- [x] 3.3 Nose radius updated in `AblationState` from integrated recession
- [x] 3.4 Vehicle mass reduced by TPS mass loss
- [x] 3.5 Recession/mass bookkeeping covered by pipeline regression tests
- [x] 3.6 Shape feedback couples back into aero coefficients

## 4. Plasma Blackout System

- [x] 4.1 Create `compute_plasma_blackout` system
- [x] 4.2 Electron density model: `n_e = C·ρ·v³`
- [x] 4.3 Blackout when `n_e > n_crit`
- [x] 4.4 `CommsBlackoutEvent` start/end edges for HUD/flight log
- [x] 4.5 Edge detection tested (exactly one start + one stop crossing)
- [x] 4.6 Clearance on velocity/density decay (same edge test)

## 5. Parachute System

- [x] 5.1 Create `compute_parachute_forces` system
- [x] 5.2 Mortar → reefed → full inflation state machine (pure domain, tested)
- [x] 5.3 Canopy drag applied opposite velocity: `F = ½ρv²·Cd(t)·A_ref`
- [x] 5.4 Drogue/main configs in `EntryPhysicsConfig` (per-body)
- [x] 5.5 Deployment gated on descent direction (ascending airstream never deploys)
- [x] 5.6 Drag force matches q·Cd·A exactly (unit-tested)

## 6. Supersonic Retro-Propulsion

- [x] 6.1 Create `compute_retro_propulsion` system
- [x] 6.2 DLR base-pressure correlation shape (monotonic in Mach, floored)
- [x] 6.3 Single thrust writer: multiplier consumed by `propulsion_thrust`
- [x] 6.4 Effectiveness monotonicity + floor clamp unit-tested
- [x] 6.5 No modification at or below the Mach threshold (unit-tested)

## 7. Pipeline Integration

- [x] 7.1 All five systems wired into `RocketSet::EntryPhysics` (chained sets)
- [x] 7.2 Systems read `AtmosphereState`, write their own components only
- [x] 7.3 Parachute forces feed the translational accumulator
- [x] 7.4 Retro-propulsion feeds thrust via the single-writer path
- [x] 7.5 `EntryPhysicsConfig` registered in `RocketModePlugin`

## 8. Validation

- [x] 8.1 Full suite green through Phases 12–16 sweeps (only known failures fixed in Phase 16)
- [x] 8.2 All three modes launch panic-free under xvfb
- [x] 8.3 Archived after verification (2026-08-23)
