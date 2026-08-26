## 1. MVP Foundations

- [ ] 1.1 Complete and validate the separate precision Earth-orbit insertion change before building map-dependent gameplay.
- [ ] 1.2 Define pure, serializable MVP part, vehicle-draft, player-profile, mission, objective, and reward value objects with explicit units and validation errors.
- [ ] 1.3 Extend the existing RON-backed catalog with a bounded unlocked part catalog and compile valid player drafts into the authoritative `VehicleDef`.
- [ ] 1.4 Add deterministic unit tests for draft validation, stack compatibility, mass/stage derivation, and invalid-build diagnostics.
- [ ] 1.5 Add rocket-mode game-flow state composition for title, profile, assembly, briefing, flight, pause, and debrief without registering it in normal or craft modes.

## 2. Player Command Authority

- [ ] 2.1 Define direct, assisted, and autopilot command-ownership state and per-axis input/assist/autopilot proposals.
- [ ] 2.2 Add explicit FixedUpdate command arbitration ahead of the existing control and actuation systems.
- [ ] 2.3 Implement desktop input bindings and rebinding/settings for throttle, attitude, RCS, staging, deployables, pause, cameras, and safe time warp.
- [ ] 2.4 Add stability, maneuver, ascent, and landing assists that use the command-arbitration boundary rather than writing physical state.
- [ ] 2.5 Add focused tests proving direct commands survive guidance, assists preserve player-owned axes, unavailable actions are rejected, and actuator limits still apply.

## 3. Flight Presentation And Game Flow

- [ ] 3.1 Build title/profile, vehicle-selection, mission-briefing, pause, failure, and debrief screens within rocket mode.
- [ ] 3.2 Consolidate existing authoritative telemetry into a responsive flight HUD showing vehicle, objective, command-owner, warning, and event state.
- [ ] 3.3 Integrate existing chase, cockpit, orbital, and free-look cameras into player-selectable game controls and camera transitions.
- [ ] 3.4 Add clear staging, deployment, impact, recovery, and invalid-action feedback with a fast mission retry path.
- [ ] 3.5 Add desktop UI/input smoke coverage for each game-flow transition and verify default/craft modes do not compose game UI.

## 4. Vehicle Assembly Vertical Slice

- [ ] 4.1 Implement the assembly screen with a small unlocked stack-part palette, selection, placement, removal, naming, duplication, and deletion.
- [ ] 4.2 Implement configuration-driven 3D preview mesh generation so MVP builds are not rendered as a Falcon-9-only layout.
- [ ] 4.3 Display derived mass, thrust-to-weight, staged delta-v, validation diagnostics, and staging order from the authoritative vehicle definition.
- [ ] 4.4 Launch the selected validated build through the existing spawn/physics path and confirm spawned properties match its preview.
- [ ] 4.5 Add domain and Bevy integration tests for assemble-preview-launch parity.

## 5. Mission And Progression Loop

- [ ] 5.1 Define data-driven Earth MVP missions and progression prerequisites: reach space, safe low Earth orbit, satellite deployment, and capsule recovery.
- [ ] 5.2 Implement pure objective evaluators from authoritative rocket, orbital, payload, terrain-contact, and recovery events.
- [ ] 5.3 Wire mission events to objective updates, rewards, unlocks, HUD state, failure conclusion, and debrief metrics.
- [ ] 5.4 Add a payload component, deployment action, independent orbital-state handoff, and accepted-orbit evaluation for the satellite mission.
- [ ] 5.5 Add deterministic success/failure tests for all four missions and an end-to-end headless Earth vertical-slice harness.

## 6. Orbital Planning And Execution

- [ ] 6.1 Complete a dedicated orbital-map presentation using the existing f64 predictor, analytic apsis markers, impact display, and authoritative telemetry.
- [ ] 6.2 Define one maneuver-node value object and a pure patched-conic post-burn prediction calculation with validation for unavailable trajectories.
- [ ] 6.3 Add map interactions to place, edit, delete, and inspect one prograde, retrograde, normal, anti-normal, radial-in, or radial-out maneuver.
- [ ] 6.4 Add burn-window cueing and maneuver assistance that produces physical commands through the command-arbitration pipeline.
- [ ] 6.5 Gate time warp by command ownership, atmospheric/terrain risk, and active maneuver execution; explain enforced limits in the HUD.
- [ ] 6.6 Add f64 domain regressions for maneuver predictions, marker geometry, unsafe impact handling, and physical maneuver execution.

## 7. Persistence, Accessibility, And Release Quality

- [ ] 7.1 Implement versioned local RON profile persistence for settings, unlocked parts, mission history, and vehicle drafts with atomic writes and backup recovery.
- [ ] 7.2 Add profile migration/default initialization and corrupt-save recovery tests.
- [ ] 7.3 Add control sensitivity, rebinding, pause, UI-scale, and motion-reduction settings; persist and validate them.
- [ ] 7.4 Add the MVP's minimal visual/audio feedback assets for engines, staging, warnings, deployment, impact, and mission results without affecting simulation authority.
- [ ] 7.5 Add user-facing onboarding that teaches assembly, direct/assisted flight, staging, map use, recovery, and restart behavior across the four missions.
- [ ] 7.6 Run `cargo fmt --check`, `cargo check`, `cargo clippy`, `cargo test`, release build, and automated rocket/default/craft-mode smoke runs; document known MVP physics and gameplay limitations.
