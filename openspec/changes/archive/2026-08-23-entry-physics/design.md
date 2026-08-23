## Context

The rocket simulator has a complete physics pipeline: gravity, 6-DOF dynamics, propulsion, atmosphere/aero, guidance/control/actuation, terrain data/LOD/collision/streaming, terrain rendering, and descent guidance. The pipeline runs in `FixedUpdate` with the chain: guidance → control → actuation → terrain interaction → atmosphere → force accumulation → aero forces/torque → propulsion thrust/gimbal/consumption/staging → 6-DOF integration → render sync. Atmosphere state (density, temperature, speed of sound) is cached per rocket before aero/propulsion consume it. The descent guidance handles deorbit, reentry corridor, and powered/unpowered terminal guidance. What's missing is the physical entry physics: heating, ablation, plasma blackout, parachutes, and supersonic retro-propulsion.

## Goals / Non-Goals

**Goals:**
- Convective heating via Sutton-Graves correlation at stagnation point
- Radiative heating via Tauber-Sutton for high-velocity entries
- Ablation with char-layer recession, mass loss, and nose radius growth
- Plasma blackout detection from electron density
- Parachute deployment (mortar → reefed → full) with time-varying Cd
- Supersonic retro-propulsion with plume-freestream interaction
- Per-body `EntryPhysicsConfig` for coefficients, materials, parachute parameters

**Non-Goals:**
- Full CFD/Navier-Stokes simulation
- Multi-species ablation chemistry
- Real-time plasma sheath electromagnetics
- Parachute fluid-structure interaction (use empirical inflation curves)
- CFD-level plume modeling (use empirical base pressure correlation)

## Decisions

### Pipeline Integration

**Decision:** Add entry physics systems in `FixedUpdate` before 6-DOF integration, between atmosphere properties and force accumulation. New systems: `compute_heating`, `compute_ablation`, `compute_plasma_blackout`, `compute_parachute_forces`, `compute_retro_propulsion`.

**Rationale:** Keeps physics pipeline modular; each phenomenon is independent and testable. Atmosphere state is already cached—entry physics reads it and writes new components (`ThermalState`, `AblationState`, `ParachuteState`) consumed by force/torque accumulation.

**Alternatives considered:**
- Merge into existing `aerodynamic_forces`: couples heating to aero, violates single-responsibility
- Run in `Update`: breaks fixed-timestep determinism

### Heating Model

**Decision:** Sutton-Graves for convective (`q_dot = k * sqrt(rho/R_nose) * v^3`), Tauber-Sutton for radiative.

**Rationale:** Industry-standard engineering correlations; fast to compute; validated against flight data (Apollo, Shuttle, Orion). Constants `k` calibrated per body (Earth, Mars, Moon).

**Alternatives considered:**
- Fay-Riddell: more accurate but requires boundary-layer integration; overkill for this fidelity
- DPLR/CFD coupling: too slow for real-time simulation

### Ablation Model

**Decision:** Char-layer recession `dr/dt = q_dot / (rho_tps * H_abl)` with nose radius update and mass loss.

**Rationale:** Captures the dominant physics (recession + mass loss) without multi-layer thermal response. Mass loss feeds 6-DOF dynamics; radius growth feeds back into heating.

**Alternatives considered:**
- Full thermal response (TACOT, FIAT): too complex, requires material property databases
- No ablation: unrealistic for lunar/Mars return

### Plasma Blackout

**Decision:** Electron density `n_e = C * rho^a * v^b` (empirical fit); blackout when `n_e > n_crit(f_comms)`.

**Rationale:** Simple, fast, captures the essential physics. Signal loss is a discrete event useful for guidance/telemetry.

**Alternatives considered:**
- Full plasma sheath simulation (PIC/MHD): too slow
- No blackout: misses operational constraint

### Parachute Model

**Decision:** 3-stage inflation with time-varying `Cd(t)`: mortar (0.1s) → reefed (Cd_reduced, 5-10s) → full (Cd_max). Force applied at canopy attach point.

**Rationale:** Matches Falcon 9 / Dragon / Starship deployment sequences. Reefing reduces opening shock.

**Alternatives considered:**
- Instantaneous Cd step: causes numerical instability
- Full FSI: too slow

### Supersonic Retro-Propulsion

**Decision:** Empirical base pressure correlation (DLR model) for plume-freestream interaction.

**Rationale:** Captures thrust augmentation and base pressure changes at supersonic speeds without CFD.

**Alternatives considered:**
- CFD lookup tables: large memory, interpolation complexity
- No interaction: underestimates effective thrust at high Mach

### Configuration

**Decision:** `EntryPhysicsConfig` per body with coefficients, material properties, parachute parameters.

**Rationale:** Centralizes all entry physics constants; avoids magic numbers; enables per-body tuning.

## Risks / Trade-offs

| Risk | Mitigation |
|------|------------|
| Heating correlation accuracy vs. flight data | Calibrate `k` constants against Apollo/Orion/SpaceX data; document uncertainty bounds |
| Ablation mass loss affecting vehicle stability | Update inertia tensor in `compute_ablation`; validate with mass-properties tool |
| Plasma blackout false positives/negatives | Tune electron density fit against Shuttle/Orion comms data; add hysteresis |
| Parachute opening shock causing integration instability | Use implicit integration for parachute force; limit `Cd` rate of change |
| Retro-propulsion correlation validity range | Document Mach/altitude envelope; clamp outside validated range |

## Migration Plan

1. Add `ThermalState`, `AblationState`, `ParachuteState` components.
2. Add entry physics systems to `RocketModePlugin` FixedUpdate chain.
3. Add `EntryPhysicsConfig` resource and populate per-body defaults.
4. Wire heating → ablation → plasma → parachutes → retro-propulsion → force accumulation.
5. Add unit tests for each correlation against known flight data points.
6. Add integration test: lunar return entry → blackout → parachute deploy → splashdown.

## Open Questions

- Should retro-propulsion use a unified `Propulsion` interface or remain separate? (Defer: start separate, refactor if commonality emerges)
- Should ablation update the inertia tensor in the same system or a separate one? (Defer: same system for simplicity)