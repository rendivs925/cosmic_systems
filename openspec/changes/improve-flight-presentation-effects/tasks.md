## 1. Presentation Audit

- [ ] 1.1 Audit existing rocket presentation, interpolation, camera, lifecycle,
  effect hooks, assets, and mode composition; document the reusable authority
  path and the minimum presentation-only extension.
- [ ] 1.2 Define effect quality configuration and lifecycle ownership without
  duplicating simulation state, coordinates, or asset loading.

## 2. Flight Effects Foundation

- [ ] 2.1 Add the rocket-mode presentation-effects composition and reusable
  effect resources/assets without registering it in normal or craft modes.
- [ ] 2.2 Add presentation-only effect state derived from existing fixed-state
  snapshots and authoritative lifecycle state.
- [ ] 2.3 Add pure tests proving presentation state cannot mutate authoritative
  rocket state, simulation time, or force inputs.

## 3. Visual Feedback And Animation

- [ ] 3.1 Implement engine-operation visual feedback driven by authoritative
  throttle and engine state.
- [ ] 3.2 Implement atmospheric-flight visual feedback from existing flight
  conditions without reimplementing atmospheric physics.
- [ ] 3.3 Implement staging and recovery feedback through existing lifecycle
  transitions and verify effect cleanup for removed or spent entities.
- [ ] 3.4 Improve rocket camera-facing animation using existing interpolation
  and render-origin conversion without steering or moving the rocket.

## 4. Quality And Validation

- [ ] 4.1 Implement effect-quality levels that only control presentation work,
  including an explicit disabled mode.
- [ ] 4.2 Add lifecycle, entity-lifetime, render-origin, and quality-isolation
  regression coverage.
- [ ] 4.3 Run formatting, compile, strict Clippy, tests, scientific validation,
  and bounded normal, craft, and rocket mode startup checks; manually inspect
  presentation only in a usable graphical environment.
