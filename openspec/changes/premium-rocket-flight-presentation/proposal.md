## Why

Rocket mode has authoritative flight dynamics, terrain streaming, atmospheric conditions, and ephemeris-driven lighting, but the launch experience has no rocket-specific exhaust, ground interaction, or audio presentation. The existing minimal pad and terrain material treatment do not communicate the scale or physical state of a launch.

## What Changes

- Add a Rocket-mode-only presentation layer that derives plume, ignition, ground-effect, aerodynamic, heating, and staging visuals from the authoritative rocket and terrain state.
- Add a bounded, quality-aware engine-effect representation that follows engine stations through render-origin rebasing and staging without affecting flight simulation.
- Improve the Rocket launch pad's presentation geometry and local lighting response while preserving its visual-only role.
- Move the default Rocket launch anchor from Kennedy Space Center to a deterministic Papua, Indonesia coastal-lowland presentation site while preserving geodetic, terrain, and reference-frame authority.
- Extend the existing atmosphere, cloud, terrain-material, and vegetation presentation paths with an offline, deterministic Papua tropical-biome profile derived from existing terrain inputs.
- Add a parameterized Rocket audio architecture driven by propulsion, flight conditions, camera/observer position, and lifecycle events. Production engine audio assets remain an explicit content dependency.
- Add focused tests for effect mapping, presentation lifecycle, terrain-relative ground effects, and render-origin stability, plus native release performance instrumentation for the new presentation work.

## Capabilities

### New Capabilities
- `rocket-flight-presentation`: Physically informed, render-origin-stable Rocket visual and audio presentation driven by authoritative flight state.
- `rocket-engine-effects`: Bounded engine plume, ground interaction, and lifecycle effects derived from propulsion and terrain state.

### Modified Capabilities
- `rocket-mode`: Rocket-mode composition registers the presentation systems without changing authoritative fixed-step flight behavior.
- `terrain-rendering`: Rocket terrain presentation provides richer source-derived surface appearance and remains bounded by existing streaming, LOD, and render-origin rules.

## Impact

- Affected Rocket presentation adapters, spawning, lifecycle/staging integration, environment, terrain material/shader paths, and Rocket mode composition.
- No physics, terrain collision, coordinate, simulation-time, or propulsion authority changes.
- No new renderer, particle, or audio dependency is proposed for the initial implementation. Terrain remains entirely offline and does not download imagery, elevation, land-cover, or vegetation data. Non-terrain content must record license/provenance before inclusion; Poly Haven CC0 assets are the preferred material source, while individual Pixabay and NASA media items require use-specific review.
