## Context

See `proposal.md` for motivation. Earth already has a resident ETOPO1 CSDEM,
one global 2048 by 1024 albedo image, deterministic close-range material maps,
and a bounded cube-sphere terrain lifecycle. `TerrainStreamingResource` owns
visible patch selection, worker task admission, geometry residency, and
telemetry; `terrain/render.rs` owns Bevy mesh and material assets. Recent
release telemetry shows worker terrain bakes near 3.5-4.5 ms while cold-start
publication takes many frames. Runs report a `0x0` window in this environment,
so their frame times are diagnostic only.

## Goals / Non-Goals

**Goals:**

- Make Earth imagery progressively sharper from globe scale to the first
  configured local-detail coverage.
- Reuse the existing terrain patch lifecycle, worker pool, render assets, and
  budget/telemetry owners.
- Preserve global imagery as immediate fallback and preserve one authoritative
  terrain source for geometry and collision.
- Provide evidence for visual quality and frame pacing on a usable display.

**Non-Goals:**

- A Google Maps-style online imagery service, arbitrary global web-tile access,
  or runtime network downloads.
- Changing DEM resolution, terrain collision, rocket physics, body frames, or
  the existing geometry LOD algorithm before telemetry identifies a need.
- Reproducing proprietary Google imagery, street-level coverage, or a second
  planet renderer.

## Decisions

### Use an offline cube-sphere imagery package

Prepare a versioned Earth imagery package offline from a reviewed,
redistributable source. The package will include a low-resolution global
overview and bounded cube-face imagery tiles keyed by the existing
`body + face + level + tile_x + tile_y` identity. This avoids equirectangular
sampling distortion and lets imagery use exactly the same visibility, horizon,
and neighbor decisions as terrain geometry.

An equirectangular single-image upgrade was rejected because it cannot provide
close-range sharpness. Runtime HTTP tiles were rejected because availability,
licensing, determinism, and latency would become simulation presentation
dependencies.

### Reserve local-detail capacity within the existing LOD cover

The visible viewport retains its current bounded cube-sphere cover, but the
selection reserves a small, explicit subset of that cover for the current
camera intersection or prelaunch launch direction. That subset is allowed to
reach the existing local-surface/vegetation threshold before breadth-first
refinement consumes the entire leaf budget. Coarser ancestors remain published
for the rest of the viewport and during every handoff.

Raising the global leaf cap or lowering the local-detail threshold was rejected:
the former increases CPU/GPU pressure for every view, while the latter produces
low-resolution surface maps and sparse vegetation across patches too large for
convincing close presentation.

### Introduce imagery as a terrain-presentation payload

Imagery is independent of the `TerrainSource` and is attached only to ready
terrain render state. Geometry publishes first with the existing global albedo.
The renderer upgrades a patch material after its imagery payload is ready; a
parent overview remains usable until child imagery is available. Render and
collision keep consuming the same height source as today.

Generating a second terrain mesh or using imagery as a height source was
rejected because it would duplicate terrain authority and risk visual/physical
surface disagreement.

### Use existing bounded ownership and telemetry

`TerrainStreamingResource` remains the sole request, task, cancellation, and
cache owner. The terrain render plugin retains imagery asset handles with their
patch state and releases them on eviction. Imagery gets explicit CPU/GPU and
per-frame upload budgets, reported through the existing cadence-limited
`terrain_streaming` metrics. Geometry work retains priority over imagery.

A separate imagery manager/cache, unbounded asset retention, and synchronous
image decoding in a render-critical system were rejected.

### Deliver quality in two datasets

The first package establishes global coverage from a public, reviewed Earth
imagery source and one bounded high-detail launch/flight region. After native
display evidence validates streaming and material replacement, additional
regions can be added as data packages without changing renderer logic. The
exact source and region are acceptance gates in the implementation tasks: they
must be license-compatible, stable, checksum-verifiable, and mapped explicitly
to Earth body-fixed coordinates before ingestion.

Using a global maximum-resolution image was rejected because it would consume
unbounded disk, decode, and GPU memory without prioritizing the flight path.

## Risks / Trade-offs

- [Imagery seams or latitude/longitude inversion] → Generate cube-face tiles
  from explicit body-fixed directions and add cube-edge, antimeridian, and
  polar regression tests.
- [Imagery upload delays visual refinement] → Preserve global fallback, bound
  uploads below geometry priority, and compare backlog/frame metrics before
  raising any budget.
- [Focused refinement causes holes or LOD churn] → Reuse the existing balanced
  selection, parent fallback, hysteresis, cancellation, and protected-cache
  rules; add focused-prelaunch regression coverage.
- [Dataset license or source availability changes] → Require manifest version,
  license, source checksum, and offline local package verification.
- [A virtual or zero-sized display misrepresents rendering performance] → Treat
  those runs as diagnostics only and require a native-display acceptance run.

## Migration Plan

1. Keep the existing global Earth albedo active by default.
2. Add the imagery package behind explicit manifest availability; missing or
   invalid optional imagery falls back to the current albedo.
3. Enable detailed imagery only after package verification and targeted tests.
4. Roll back by disabling the imagery package; no simulation data, terrain
   geometry, or collision state needs migration.
