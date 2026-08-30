## Context

See proposal.md for motivation. Rocket simulation already has fixed-state
authority, presentation snapshots, render-origin conversion, lifecycle state,
and rocket-only mode composition. Effects and animation must consume those
paths rather than add a second vehicle state or simulation loop.

## Goals / Non-Goals

**Goals:**
- Make propulsion, flight, staging, and recovery visually legible from existing
  authoritative state.
- Keep visual smoothing and quality adaptation entirely on the presentation
  side of the fixed simulation boundary.
- Reuse existing rocket-mode composition, render-origin conversion, asset
  ownership, and camera systems.

**Non-Goals:**
- Changing rocket dynamics, engine performance, aerodynamics, staging logic,
  terrain collision, or guidance.
- Adding a second render origin, world coordinate system, or flight camera
  controller.
- Treating particles, lights, shaders, or transforms as physical truth.

## Decisions

### 1. Effects consume presentation snapshots and lifecycle state

Presentation systems read interpolated rocket state and explicit authoritative
lifecycle transitions. They create, update, and retire visual entities or
assets without writing simulation components. This preserves deterministic fixed
simulation while allowing variable-rate animation.

Alternatives considered:
- Drive effects from camera state or meshes: rejected because visibility must
  not influence vehicle behavior.
- Put visual timers in propulsion or staging components: rejected because
  presentation lifetime is not authoritative lifecycle state.

### 2. Effects are composed as one rocket-presentation feature

A cohesive rocket presentation-effects plugin owns effect configuration,
reusable assets, and presentation-only systems. It composes only in rocket
mode, following existing mode/plugin boundaries, rather than scattering asset
loads and update logic through physics systems.

### 3. Quality changes presentation work only

Quality configuration selects visual density, update cadence, and optional
detail. It never changes fixed timestep, force models, engine state, or source
simulation data. Effects remain camera-relative after the existing f64-to-f32
render boundary.

## Risks / Trade-offs

- [Visual load affects frame pacing] → Bound effect count, reuse assets, and
  provide explicit quality levels before adding higher-detail effects.
- [Effects diverge from vehicle lifecycle] → Derive activation from existing
  authoritative state and validate entity cleanup at lifecycle transitions.
- [Large coordinates degrade effect placement] → Use existing render-origin and
  rocket presentation conversion; do not derive world-space f32 positions.
- [Presentation work leaks into other modes] → Register effects only through
  rocket-mode composition and validate normal/craft startup.

## Migration Plan

1. Audit existing rocket presentation, camera, lifecycle, assets, and effect
   hooks before adding components or systems.
2. Add the smallest state-driven engine and flight feedback path with pure
   lifecycle tests.
3. Extend to staging and recovery feedback only through existing authoritative
   transitions.
4. Add quality controls, profile presentation cost, and validate all modes.
