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

## Current Progress

The following terrain and flight-presentation foundation is implemented and
validated on `main`.

### Authority and Architecture

- Catalog surface capability prevents terrain and collision from being attached
  to stars, gas giants, and ice giants. Earth is the only configured terrain-data
  authority; other solid bodies remain eligible but do not receive invented data.
- `TerrainSource` is the single height, normal, collision, and terrain-material
  authority. Cube-sphere meshes, local textures, vegetation, cached geometry,
  and Bevy entities remain disposable presentation data.
- Earth and future solid bodies use the existing cube-sphere topology,
  body-fixed frame conversion, streaming lifecycle, collision queries, and
  render path. No second terrain, collision, coordinate, or streaming path was
  introduced.

### Earth Data and Presentation

- Earth loads a validated ETOPO1 CSDEM source when its local dataset is present.
  Deterministic procedural terrain is retained only as the explicit
  missing-file fallback. The dataset contract, manifest, provenance, datum, and
  offline converter are documented in the repository.
- The resident Earth DEM produces an immutable per-face metadata pyramid through
  level 8: 256 by 256 regions per 2048-sample face. Streaming uses patch-local
  elevation ranges for conservative geometric error; deeper patches inherit the
  nearest indexed ancestor. This is metadata over one CSDEM raster, not an
  on-disk elevation tile-payload pyramid.
- Macro geometry is streamed at every selected LOD. Local 128 by 128 material
  maps plus merged vegetation and scatter are generated in worker tasks only
  for level-12-or-finer patches, then applied as presentation-only detail over
  global Earth imagery.

### Streaming and Telemetry

- Streaming applies viewport frustum and horizon rejection, source-derived
  screen-space error, balanced cube-sphere leaves, bounded asynchronous work,
  parent fallback coverage, replacement-group-first scheduling, and protected
  LRU cache eviction.
- `TerrainStreamingResource` owns patch bake submission, completed-task
  collection, stale-request cancellation, and cadence-limited metric snapshots.
  Typed bake requests, generation batches, cancellation totals, LOD
  distributions, and metric snapshots replaced duplicated task closures and raw
  metric tuples.
- Terrain metrics report requested, target, generated, published,
  blocked-replacement, upload-backlog, in-flight-age, cancellation, eviction,
  and per-LOD distribution data without creating a competing profiler.
- `PerformanceStats` owns a bounded 600-frame history. Typed
  `PerformanceMetricsReporting` configuration is parsed once at startup and
  enables p50/p95/p99 reports at a five-second cadence only when
  `COSMIC_SYSTEMS_PERFORMANCE_METRICS=1` is set.
- LOD gizmos now require both master debug and the explicit F10 LOD toggle.
  Their body-fixed centers are converted through the same body orientation and
  flight render origin used by terrain meshes, preventing solar-map-scaled debug
  rectangles from overlaying the ascent horizon.

### Measurements and Validation

- A bounded release rocket prelaunch/ascent capture generated 353 patches in
  roughly 19 seconds, used 67.8 MiB of the 128 MiB terrain budget, retained 21
  pending render uploads, completed two worker bakes in roughly 3.7-4.2 ms, and
  performed no cache eviction. This does not justify an on-disk elevation
  payload pyramid; the existing CSDEM plus metadata pyramid remains appropriate
  until a representative flight-camera profile disagrees.
- A 60-second release rocket ascent capture with opt-in frame metrics reached
  537 samples at p50 101.8 ms, p95 114.4 ms, and p99 187.3 ms. The environment
  created a `0x0` X11 window, making this a CI-style frame-pacing baseline, not
  desktop visual-performance evidence.
- A controlled native-display chase-camera capture reached 52.7 km before the
  current stage transitioned to descent, rather than the required 70 km. Its
  rolling 600-frame sample ended at p50 82.9 ms, p95 103.1 ms, and p99 179.3
  ms. The broad high-altitude terrain facets are expected from the current
  fixed 33 by 33 coarse meshes and capped viewport leaf budget; do not tune
  that budget until a representative 70 km capture is available.
- Focused terrain and performance tests cover deterministic generation,
  cube-face and LOD seams, fallback replacement, cache protection,
  source/collision agreement, local-detail bounds, DEM metadata, reporting
  cadence, and frame-time percentiles. The latest full validation passed 608
  tests with one ignored test, plus formatting, checks, strict Clippy, a release
  build, and bounded normal/craft/rocket mode startups.

Remaining work is a driven 70 km flight-camera capture on a real display before
tuning upload pacing or terrain budgets. Build an offline elevation payload
pyramid only if that profile justifies it. Moon remains solid-surface eligible
but has no terrain authority, manifest, local dataset, or validated lunar
body-fixed frame and datum; add reviewed lunar data through the existing shared
pipeline only after those prerequisites are available.

### Phase 1: Generic Terrain Capability

**Status: complete for the catalog and Earth authority.**

- Replace Earth-only terrain registration with catalog-driven solid-surface
  capability and terrain-authority metadata.
- Keep `PlanetTerrain` and `TerrainSource` as the existing per-body authority.
- Move Earth-specific elevation bounds, launch overlays, vegetation, and
  atmosphere assumptions behind Earth-specific configuration.
- Do not attach terrain or terrain collision to `NoSolidSurface` bodies.

### Phase 2: Responsive Generic Streaming

**Status: implemented; representative desktop flight-camera measurement remains.**

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

**Status: deferred pending evidence that the resident CSDEM plus metadata pyramid
is insufficient.**

- Evolve the existing CSDEM workflow into an offline-built, versioned
  cube-sphere tile pyramid.
- Keep data loading and decoding in bounded worker tasks.
- Preserve no-I/O, source-authoritative collision sampling.
- Add global coarse elevation coverage first, then high-resolution launch and
  landing regions.

### Phase 4: Moon Validation

**Status: blocked on a reviewed lunar DEM package and validated lunar frame/datum
contract.**

- Add a lunar manifest and data package using the same tiled terrain contract.
- Validate frame/datum alignment, seams, source/collision/render agreement,
  and cache behaviour on a second solid body.
- Require approved body orientation authority before declaring terrain
  authoritative for additional moons.

### Phase 5: Additional Bodies

**Status: not started; follows Moon validation.**

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
