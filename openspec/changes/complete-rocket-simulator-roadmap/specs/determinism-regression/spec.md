## Purpose

Provides a deterministic regression suite: saved baseline trajectories, a CI gate that re-simulates and compares state hashes, and bisection tooling to isolate physics regressions so simulation behavior remains reproducible across changes.

## ADDED Requirements

### Requirement: Baseline trajectory fixtures

The system SHALL provide a set of canonical flight recordings (launch, orbit, reentry, landing) with full state histories at fixed physics timesteps.

#### Scenario: Baseline flight set

- **WHEN** the regression suite runs
- **THEN** it loads baselines for: suborbital hop, LEO insertion, GTO insertion, lunar transfer, Earth reentry, Moon landing, RTLS recovery, drone-ship recovery

#### Scenario: Bitwise reproducibility

- **WHEN** a baseline is re-simulated on the same codebase
- **THEN** every state sample matches the saved baseline bit-for-bit (position, velocity, orientation, angular velocity, mass, propellant, guidance mode)

### Requirement: CI comparison gate

The system SHALL run the baseline simulations in CI and fail if any state sample diverges beyond a documented numerical tolerance.

#### Scenario: CI gate on divergence

- **WHEN** a physics change causes a baseline to diverge
- **THEN** CI reports the specific flight, timestep, and state variable that differs

#### Scenario: Tolerance configuration

- **WHEN** comparing state samples
- **THEN** the comparison uses per-variable tolerances (position: 1 mm, velocity: 1 µm/s, attitude: 1 µrad, mass: 1 mg) configurable via a regression config file

### Requirement: Bisection tooling

The system SHALL provide a command that, given a failing baseline, bisects git history to find the commit that introduced the divergence.

#### Scenario: Automated bisection

- **WHEN** a developer runs the bisection command with a failing baseline
- **THEN** the tool checks out commits, re-simulates, and identifies the first bad commit

#### Scenario: Bisection speed

- **WHEN** bisection runs
- **THEN** it completes in under 10 minutes for a typical history depth (uses parallel simulation where possible)

### Requirement: Physics change audit trail

The system SHALL require a signed-off justification when a baseline is intentionally updated (expected improvement, numerical trade-off, affected scenarios documented).

#### Scenario: Intentional baseline update

- **WHEN** a physics improvement changes baseline results
- **THEN** the developer runs a baseline-update command that records: change description, expected improvement, numerical trade-offs, affected scenarios, and reviewer approval

#### Scenario: Immutable baseline history

- **WHEN** baselines are updated
- **THEN** the previous baseline is archived with the commit that superseded it, preserving full history