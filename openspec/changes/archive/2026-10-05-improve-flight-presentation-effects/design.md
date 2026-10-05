## Context

See proposal.md for motivation. Rocket simulation already has fixed-state
authority, presentation snapshots, render-origin conversion, lifecycle state,
and rocket-only mode composition. Effects and animation must consume those
paths rather than add a second vehicle state or simulation loop.

## Existing Capability Audit

Performed before implementation against the current tree. The archived
`premium-rocket-flight-presentation` change already delivered most of the
original broad scope:

- Engine plume effects with ignition/thrust/expansion/shock/thermal smoothing,
  distance-LOD, and staging/booster reconciliation: `rocket/effects.rs`,
  `rocket/presentation_parameters.rs`.
- Ground dust/haze and pad illumination: `rocket/ground_presentation.rs`.
- Render-origin-stable effect children, lifecycle cleanup, bounded effect
  metrics, and pure mapping/lifecycle tests.
- A presentation-only quality resource (`RocketPresentationQuality`) that today
  carries only distance thresholds.

Still uncovered, and therefore the scope of this change:

- No way to reduce or disable effect work; quality is distance-only.
- `shock_intensity_unit` and `heating_intensity_unit` are computed and smoothed
  but only nudge plume length; no atmospheric-flight visual is shown.
- Lifecycle events (`StageSeparatedEvent`, `FairingSeparatedEvent`,
  `TouchdownEvent`, `SplashdownDetectedEvent`, `CrashEvent`) have no transient
  presentation feedback.
- No explicit test that presentation state cannot mutate authoritative state.

## Goals / Non-Goals

**Goals:**
- Make propulsion, flight, staging, and recovery visually legible from existing
  authoritative state.
- Keep visual smoothing and quality adaptation entirely on the presentation
  side of the fixed simulation boundary.
- Reuse existing rocket-mode composition, render-origin conversion, asset
  ownership, camera systems, events, and presentation parameters.

**Non-Goals:**
- Changing rocket dynamics, engine performance, aerodynamics, staging logic,
  terrain collision, or guidance.
- Adding a second render origin, world coordinate system, or flight camera
  controller.
- Re-implementing engine plumes, ground interaction, pad lighting, or audio,
  which `premium-rocket-flight-presentation` already provides.
- Treating particles, lights, shaders, or transforms as physical truth.

## Decisions

### 1. Effect quality is one enum on the existing quality resource

Add `RocketEffectQualityLevel { Full, Reduced, Disabled }` to
`RocketPresentationQuality` rather than a new resource. `Full` keeps current
behavior; `Reduced` hides outer atmospheric/plume layers; `Disabled` hides all
effect children and skips their update work. The level is parsed from an
optional environment override at startup so it is configurable without UI or a
new settings system. Quality never changes fixed timestep, force models, engine
state, or source simulation data.

### 2. Atmospheric feedback reuses the existing smoothed parameters

A new presentation system reads `RocketPresentationSmoothing` (already written
from authoritative flight conditions) and shows a bounded shock shell and
heating glow as rocket children. It does not resample atmosphere or heating
physics, and it is a no-op when the corresponding intensity is negligible.

### 3. Lifecycle feedback is event-driven and transient

Staging, fairing, touchdown, splashdown, and crash feedback are spawned from
the existing authoritative events as short-lived, bounded child entities with a
deterministic presentation timer, then despawned. No visual timer is stored in
propulsion, separation, or recovery components.

### 4. Effects remain presentation-only and rocket-mode-only

All new systems are registered through rocket-mode composition, use existing
render-origin-stable child transforms, and never write simulation components.
Isolation is proven by tests that snapshot authoritative state across
presentation updates.

## Risks / Trade-offs

- [Visual load affects frame pacing] → Bound effect count, reuse assets, and
  gate all new work behind the quality level before adding higher detail.
- [Effects diverge from vehicle lifecycle] → Derive activation from existing
  authoritative state and events; validate entity cleanup at transitions.
- [Large coordinates degrade effect placement] → Use existing render-origin and
  rocket presentation conversion; do not derive world-space f32 positions.
- [Presentation work leaks into other modes] → Register effects only through
  rocket-mode composition and validate normal/craft startup.

## Migration Plan

1. Add the quality level and wire it through the existing engine and ground
   effect systems.
2. Add atmospheric shock/heating feedback from the existing smoothed state.
3. Add transient lifecycle feedback from the existing authoritative events.
4. Add isolation/quality regression tests and validate all modes.
