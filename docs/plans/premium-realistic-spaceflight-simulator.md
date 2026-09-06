# Premium Realistic Spaceflight Simulator End Goal

## Vision

Build one coherent, deterministic solar-system and spaceflight simulator where
a player can launch, fly, orbit, reenter, and land on real celestial bodies
without changing simulation systems or encountering a visual/physics mismatch.

```text
Authoritative solar-system time and ephemerides
  -> body-fixed reference frames
  -> per-body physical models
  -> per-solid-body terrain datasets
  -> rocket physics and collision
  -> camera-relative rendering and streaming
  -> telemetry, controls, and presentation
```

The project must remain one simulator, not separate solar-system, terrain,
rocket, and presentation applications.

## Core Outcomes

- Use authoritative solar-system positions, rotations, radii, gravity, and
  simulation time.
- Retain `f64`/`DVec3` authoritative simulation state from solar-system scale
  down to the surface-contact boundary.
- Render through the existing camera-relative presentation path, converting to
  `f32` only after the render origin has been applied.
- Support physically authoritative six-degree-of-freedom rockets: mass,
  propulsion, staging, thrust, gravity, aerodynamics, heating, guidance,
  control, reentry, and landing.
- Stream terrain with a continuous coarse-to-fine experience: immediate parent
  fallback, high detail around the flight path, no full-planet materialization,
  and no holes or cracks during refinement.
- Keep telemetry, camera modes, UI, and debug views derived from authoritative
  simulation state.

## Single Authority Rules

Every physical quantity has one owner.

- The ephemeris service owns celestial-body state at a simulation epoch.
- `reference_frames.rs` owns frame conversions.
- Rocket components and fixed-step systems own rocket physical state.
- A body's `TerrainSource` owns its terrain height, normals, and terrain-facing
  material inputs.
- Terrain meshes, textures, generated tile payloads, and render entities are
  disposable views of the terrain source. They are never collision or physics
  authority.

For a solid body, collision, radar altitude, terrain mesh generation, and
surface presentation query the same body-scoped terrain authority. Cached or
lower-resolution tiles are approximations with declared error, not competing
terrain truths.

## Body Surface Model

The celestial-body catalog must declare surface capability explicitly rather
than infer it from a visual body class.

```text
SolidSurface
  -> terrestrial planets, solid moons, dwarf planets, asteroids
  -> terrain streaming, terrain collision, landings

SolidSurface + Ocean
  -> solid terrain remains the ground authority
  -> ocean is a separate physical and presentation model

NoSolidSurface
  -> stars, gas giants, ice giants
  -> no DEM terrain, no terrain collision, no fake landing ground
  -> atmosphere, cloud, and volume presentation systems as appropriate
```

The Sun is not a terrain body. Its authoritative data consists of ephemeris,
orientation, radiation, and atmospheric or visual models rather than a landable
surface dataset.

## Reusable Solid-Body Terrain Contract

All solid bodies use the existing cube-sphere topology and `TerrainPatch`
identity:

```text
body ID + cube face + level + tile_x + tile_y
```

Each body has one versioned terrain dataset manifest containing:

- body identifier;
- body-fixed frame and reference radius or datum;
- source version, provenance, license, and documented error;
- cube-sphere tile-pyramid layout;
- tile availability;
- elevation payload references;
- per-tile minimum/maximum elevation and geometric error;
- optional imagery and surface-detail payload references.

The tile pyramid is a prepared representation of the one terrain source. It
allows screen-space-error selection and asynchronous loading without running
expensive terrain generation in render-critical systems.

## Runtime Terrain Behaviour

The existing terrain streaming lifecycle remains authoritative:

```text
requested -> generating/loading -> ready -> visible -> cached -> evicted
```

The runtime must:

- apply frustum and horizon rejection before work is requested;
- use per-tile geometric error for screen-space-error LOD selection;
- preserve two-to-one neighbor balancing and existing stitch correctness;
- retain a rendered parent until its replacement child cover is complete;
- prioritize visible replacement groups, high SSE excess, and terrain near or
  ahead of the rocket before distant or speculative work;
- publish base geometry before optional local textures, normals, vegetation, or
  other presentation enrichment;
- keep explicit and separate CPU, GPU, task, upload, and cache budgets;
- cancel or deprioritize stale work without blocking the main thread.

Terrain collision remains on-demand sampling from the body terrain source. It
must never depend on mesh residency, rendered transforms, or render-frame I/O.

## Presentation Layers

Terrain elevation remains authoritative. The following systems are presentation
layers or separate physical models and must not silently replace terrain height:

- satellite imagery;
- roughness and normal maps;
- vegetation and rocks;
- clouds and atmospheric scattering;
- oceans;
- city lights and other emissive presentation;
- camera and HUD.

Visual microdetail that is not represented by the authoritative terrain source
must not affect rocket collision, altitude, landing, or physics.

## First Complete Vertical Slice

Earth and Moon are the first validation pair.

Earth must support:

- launch, ascent, orbit, reentry, and terrain-aware landing;
- a real, versioned global elevation dataset with higher-resolution coverage
  for launch and landing regions;
- terrain streaming that reaches useful near-flight detail in seconds rather
  than leaving coarse fallback patches visible for tens of seconds.

Moon must support:

- the same terrain data, cube-sphere, streaming, collision, and render path;
- lunar radius, terrain datum, and body-fixed orientation;
- an independent real lunar elevation dataset.

Earth and Moon should differ by catalog configuration and data manifests, not
by separate terrain or collision implementations. This validates that the
system is genuinely planetary before Mars or other bodies are added.

## Phased Delivery

### Phase 1: Generic Terrain Capability

- Replace Earth-only terrain registration with catalog-driven solid-surface
  capability and terrain-authority metadata.
- Keep `PlanetTerrain` and `TerrainSource` as the existing per-body authority.
- Move Earth-specific elevation bounds, launch overlays, vegetation, and
  atmosphere assumptions behind Earth-specific configuration.
- Do not attach terrain or terrain collision to `NoSolidSurface` bodies.

### Phase 2: Responsive Generic Streaming

- Measure the current 70 km flight-camera terrain case using existing terrain
  metrics extended with requested/ready/published LOD distribution, task age,
  replacement-group blocking, generation time, and upload backlog.
- Replace broad coarse-first request ordering with scored visible replacement
  priority.
- Separate geometry publication from optional surface enrichment.
- Replace global elevation-range SSE estimates with terrain-source-provided
  per-tile geometric error.
- Validate that fallback coverage, cache protection, seams, and deterministic
  selection remain correct.

### Phase 3: Tiled Earth Dataset

- Evolve the existing CSDEM workflow into an offline-built, versioned
  cube-sphere tile pyramid.
- Keep data loading and decoding in bounded worker tasks.
- Preserve no-I/O, source-authoritative collision sampling.
- Add global coarse elevation coverage first, then high-resolution launch and
  landing regions.

### Phase 4: Moon Validation

- Add a lunar manifest and data package using the same tiled terrain contract.
- Validate frame/datum alignment, seams, source/collision/render agreement,
  and cache behaviour on a second solid body.
- Require approved body orientation authority before declaring terrain
  authoritative for additional moons.

### Phase 5: Additional Bodies

- Add Mars and other solid bodies through data manifests and configuration.
- Add body-specific atmosphere and ocean models where physically applicable.
- Do not generalize Earth launch sites, vegetation, or ocean assumptions to
  other bodies.

## Completion Criteria

The simulator reaches this target when:

- Earth and Moon use the same solid-body terrain pipeline and no duplicate
  per-body terrain systems exist.
- Every solid-body terrain dataset has explicit provenance, frame, datum,
  coverage, resolution, and error documentation.
- A flight camera receives immediate coarse coverage and responsive,
  screen-space-error-driven refinement without terrain holes, cracks, or
  visible long-lived root fallbacks.
- Collision, altitude, and landing behavior agree with the body's authoritative
  terrain source independently of visual tile residency.
- Stars and gas/ice giants cannot accidentally receive false terrain collision.
- New solid bodies are introduced through catalog and dataset configuration,
  not new terrain, collision, LOD, or renderer implementations.
- All modes preserve shared ephemeris, time, coordinate, physics, terrain, and
  presentation authorities.
