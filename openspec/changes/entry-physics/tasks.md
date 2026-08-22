## 1. Components & Config

- [ ] 1.1 Add `ThermalState`, `AblationState`, `ParachuteState` components in `entity_components.rs`
- [ ] 1.2 Add `EntryPhysicsConfig` resource with per-body defaults (Earth, Moon, Mars)
- [ ] 1.3 Add `EntryPhysicsConfig::for_body(name)` constructor

## 2. Convective Heating System

- [ ] 2.1 Create `compute_heating` system in `FixedUpdate` (before force accumulation)
- [ ] 2.2 Implement Sutton-Graves convective heat flux: `q_dot = k * sqrt(rho/R_nose) * v^3`
- [ ] 2.3 Implement Tauber-Sutton radiative heat flux for v > 10 km/s
- [ ] 2.4 Write total heat flux to `ThermalState` component
- [ ] 2.5 Unit test: heating peak at max q matches analytical estimate
- [ ] 2.6 Unit test: nose radius effect (2x radius → ~0.7x peak flux)

## 3. Ablation System

- [ ] 3.1 Create `compute_ablation` system (after heating, before integration)
- [ ] 3.2 Implement char-layer recession: `dr/dt = q_dot / (rho_tps * H_abl)`
- [ ] 3.3 Update nose radius in `AblationState` from integrated recession
- [ ] 3.4 Update vehicle mass from TPS mass loss
- [ ] 3.5 Unit test: ablation mass loss matches analytical for constant heat flux
- [ ] 3.6 Unit test: shape change feedback reduces subsequent heating

## 4. Plasma Blackout System

- [ ] 4.1 Create `compute_plasma_blackout` system
- [ ] 4.2 Implement electron density model: `n_e = C * rho^a * v^b`
- [ ] 4.3 Detect blackout when `n_e > n_crit(f_comms)` (default S-band ~2.3 GHz)
- [ ] 4.4 Emit `CommsBlackoutEvent` (start/end) for guidance/telemetry
- [ ] 4.5 Unit test: blackout onset at expected altitude/velocity
- [ ] 4.6 Unit test: blackout clearance on velocity/density decay

## 5. Parachute System

- [ ] 5.1 Create `compute_parachute_forces` system
- [ ] 5.2 Implement 3-stage inflation: mortar (0.1s) → reefed (Cd_reduced, 5-10s) → full (Cd_max)
- [ ] 5.3 Apply drag at canopy attach point: `F = 0.5 * rho * v^2 * Cd(t) * A_ref`
- [ ] 5.4 Add drogue and main parachute configs to `EntryPhysicsConfig`
- [ ] 5.5 Unit test: drogue terminal velocity matches spec
- [ ] 5.6 Unit test: main parachute terminal velocity ~5-7 m/s

## 6. Supersonic Retro-Propulsion

- [ ] 6.1 Create `compute_retro_propulsion` system
- [ ] 6.2 Implement DLR base pressure correlation for plume-freestream interaction
- [ ] 6.2 Compute effective thrust and base pressure modification at Mach > 1
- [ ] 6.3 Feed modified thrust to propulsion force accumulator
- [ ] 6.4 Unit test: thrust augmentation at Mach 2 matches DLR data
- [ ] 6.5 Unit test: no modification below Mach 1

## 7. Pipeline Integration

- [ ] 7.1 Wire all 5 entry physics systems into `RocketModePlugin` FixedUpdate chain (before force accumulation)
- [ ] 7.2 Ensure systems read `AtmosphereState` and write respective components
- [ ] 7.3 Connect parachute forces to translational accumulator
- [ ] 7.4 Connect retro-propulsion thrust to propulsion accumulator
- [ ] 7.5 Add `EntryPhysicsConfig` resource to `RocketModePlugin`
- [ ] 7.6 Integration test: Earth reentry from lunar return → heating → ablation → blackout → parachute → splashdown
- [ ] 7.7 Integration test: Mars entry → heating → parachute → landing

## 8. Validation & Polish

- [ ] 8.1 Run full test suite: `cargo test --features dem`
- [ ] 8.2 Verify all three modes launch: `cargo run`, `cargo run -- craft`, `cargo run -- rocket`
- [ ] 8.3 Validate with `openspec validate --strict --change entry-physics`
- [ ] 8.4 Archive change: sync specs to `openspec/specs/`, archive to `openspec/changes/archive/`