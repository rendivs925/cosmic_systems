## Context

See proposal.md for motivation. Rocket mode already exposes authoritative `RocketPhysicsState`, `RocketPropulsion`, `RocketFlightConditions`, thermal/entry state, terrain collision, lifecycle messages, ephemeris-derived lighting, and a render-origin-safe presentation boundary. It has no Rocket-specific VFX or audio layer. The existing terrain path provides asynchronous cube-sphere streaming, prepared local surface maps, a custom PBR extension, bounded uploads, and merged vegetation/scatter geometry.

The source tree contains neither production Rocket audio/VFX assets nor licensed high-resolution land-cover, launch-site imagery, vegetation, or infrastructure assets. Bevy 0.17.3 is the current rendering/audio runtime; no third-party particles, audio middleware, or volumetric renderer is present. Terrain must remain offline: the existing Earth `TerrainSource` stays authoritative for elevation/collision, while a deterministic Papua visual-biome profile is prepared from its existing samples.

## Goals / Non-Goals

**Goals:**
- Add a Rocket-mode presentation layer that is sourced exclusively from authoritative simulation and presentation state.
- Establish a high-quality, bounded baseline for engine plumes, ignition/liftoff response, ground interaction, staging feedback, atmospheric transitions, and future audio control.
- Improve existing pad, terrain material, cloud, and lighting presentation only through existing authorities and available assets.
- Move the default launch anchor to a stable Papua coastal-lowland coordinate, retaining the existing geodetic-to-terrain-to-inertial conversion path.
- Make all effect scaling testable as pure, unit-named mappings and retain render-origin stability.

**Non-Goals:**
- No changes to flight dynamics, guidance, propulsion, terrain collision, simulation time, Earth geometry, or reference frames.
- No CFD, CPU volumetrics, unbounded particles, a second terrain source, or a second floating origin.
- No terrain downloads, runtime imagery, online land-cover, or external vegetation data.
- No claim of photoreal regional terrain, production engine audio, volumetric clouds, or species-level vegetation without licensed source assets and a measured rendering path.
- No camera redesign or terrain shadow-system rewrite.

## Decisions

### 1. Add a Rocket presentation adapter, not simulation state

Create a focused Rocket presentation/effects adapter in the existing Rocket infrastructure module. It reads the fixed-step state in `Update`, after render interpolation and render-origin/pad synchronization, and writes only render entities, material parameters, effect visibility, and audio controls.

This preserves the existing authority chain: fixed simulation -> render snapshot -> render-origin conversion -> presentation. A fixed-tick VFX system was rejected because its visual cadence would not improve authority and could couple visual work to simulation throughput.

### 2. Use engine-station child entities with parameterized procedural meshes

The current rocket mesh combines engine bells into one mesh, so presentation needs child entities identified by catalog engine station and stage ownership. Initial plumes use a small number of reusable emissive meshes/materials: bright core, turbulent inner volume approximation, and a lower-opacity outer expansion shell. Their transforms and emission are mapped from actual engine eligibility, throttle, pressure, density, and thrust-related configuration.

This is a physically informed real-time approximation, not fluid dynamics. It is selected over a particle dependency because no GPU particle integration is currently present, the desired effects have a small bounded emitter count, and existing bloom supports emissive geometry. A dedicated GPU particle implementation can later replace the outer layer only after profile evidence.

### 3. Model ignition, ground interaction, shock, and heating as presentation state machines

Pure mapping functions turn authoritative conditions into bounded presentation parameters: ignition ramp, plume expansion, ground-effect strength, shock/condensation eligibility, and thermal color/intensity. Components retain only presentation smoothing/previous-frame state; lifecycle events rebuild or retire effect children for staging and relaunch.

This uses existing stage/fairing messages rather than inferring transitions from visual mesh changes. Shock and heating remain subtle, require meaningful Mach/dynamic pressure/heat flux thresholds, and are absent in ordinary low-speed flight.

### 4. Keep ground effects attached to the pad/terrain presentation boundary

Ground-effect emitters use the existing body-fixed launch-pad anchor and terrain-relative rocket distance. They never sample rendered meshes or write collision state. The initial implementation uses bounded procedural dust/haze cards or meshes with a strict camera-distance/visibility LOD; it avoids persistent full-screen smoke and particle entities.

### 5. Add audio control architecture before production content

Audio behavior is represented as derived control parameters for engine, ground-rumble, and interior sources: gain, pitch, high-frequency attenuation, and external-atmosphere attenuation. Bevy audio sources are created once per Rocket lifecycle and updated at a controlled cadence. Actual Rocket audio playback is enabled only when Rocket-specific licensed assets are present; the existing UFO electronic loop is not reused.

This produces a truthful extensibility point without representing synthetic generated tones as premium recorded engine audio.

### 6. Extend existing environmental materials conservatively

Terrain detail remains in the existing prepared local albedo/normal/roughness maps and terrain shader. The Papua profile uses offline authoritative elevation, slope, moisture, latitude, and deterministic patch seeds to select tropical lowland soil, wet vegetation, exposed rock, and bounded scatter. It does not claim to be remote-sensed Papua land cover. Earth clouds remain the existing static geographic shell, updated through the bound planet and render origin; physically based scattering, weather, cloud shadows, and high-resolution imagery are deferred to data/rendering changes that need dedicated validation.

### 7. Use a validated Papua coastal-lowland launch anchor

Define a named default Papua launch-site preset in the existing launch-site value-object module. The preset remains a normal geodetic coordinate; startup samples its elevation/normal through `TerrainSource`, then uses the existing reference-frame converter for physical placement and the existing pad synchronization for visuals. A remote south-Papua lowland coordinate is selected to give the deterministic tropical profile a coherent broad context without asserting it represents an existing operational spaceport.

This replaces the hard-coded KSC preset at Rocket spawn only. It is selected over local terrain import because the user requires offline terrain and the project already treats `TerrainSource` as authoritative.

### 8. Budget every new visual system

Each emitter has one owner, tracked count, screen-space/distance budget, and fallback. Near plumes use all layers; medium/far plumes use simplified emissive geometry; offscreen effects are hidden. Ground effects have a fixed emitter cap and lifetime. The performance telemetry is extended only where it can identify a future quality-control decision.

## Risks / Trade-offs

- [Transparent effects can harm frame pacing] -> Use a fixed small emitter count, simple meshes, distance/visibility LOD, and native-release p50/p95/p99 measurements before raising quality.
- [Catalog engine coordinates may not match the combined visual mesh exactly] -> Reuse the catalog station convention, write local-transform tests, and visually validate all supported vehicles.
- [Staging/relaunch can leave orphaned children or sounds] -> Centralize effect lifecycle handling at existing spawn, staging, separation, and relaunch boundaries; test entity ownership and cleanup.
- [Render rebasing can make effects jump] -> Parent rocket effects to interpolated vehicle roots and express pad effects through the existing body-fixed anchor/render-origin conversion; test rebase continuity.
- [Procedural terrain appearance cannot reproduce surveyed Papua land cover] -> Keep the visual profile explicitly deterministic/procedural and source-derived; use actual geographic data only in a separately approved offline terrain-data change.
- [External media may have incompatible redistribution terms] -> Prefer Poly Haven CC0 materials; review and record the exact license/provenance of every Pixabay or NASA item before adding it.
- [Atmospheric effects could imply physical authority] -> Source all thresholds from the existing flight-condition and entry samples, document visual-only limits, and never write back into domain state.

## Migration Plan

1. Add pure parameter mappings and their regression tests without registering effects.
2. Register Rocket-mode-only visual effects, then wire lifecycle ownership for staging and relaunch.
3. Move the default spawn and visual pad anchor to the Papua preset, then add the deterministic offline tropical surface profile.
4. Add bounded ground/environmental presentation and optional audio control once Rocket-specific assets are approved.
5. Validate normal, craft, and Rocket startup paths; use the prior Rocket release profile as baseline and retain the change only if frame pacing remains within the measured budget.

Rollback consists of removing the Rocket presentation registration and its presentation-only entities/resources; no persisted or authoritative simulation data is migrated.
