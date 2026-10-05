## Context

See `proposal.md` - Why. Earth currently loads a single resident CSDEM
(`assets/large_files/terrain/earth_etopo1_ice_surface_cs2048_v2.csdem`) as its
measured authority and composes a bounded procedural detail layer over it. The
resident raster is ~4.9 km effective GSD and its metadata pyramid stops at
level 8. Collision, radar altitude, and terrain meshes all sample the one
`TerrainSource` synchronously, so any higher-resolution data must keep sampling
free of I/O while remaining the single authority. The existing offline
converter binaries, `DemTerrainSource`, `LocalElevationPackage`, and the
`TerrainStreamingResource` request/worker/cache/budget owners are reused.

## Goals / Non-Goals

**Goals:**

- Provide measured high-resolution elevation near launch, flight, and landing
  through a streamed, versioned payload pyramid.
- Keep every authoritative terrain sample synchronous, deterministic, and
  independent of what is currently resident.
- Reuse the existing terrain source, streaming lifecycle, cube-sphere LOD, and
  telemetry owners; introduce no second terrain authority.

**Non-Goals:**

- A full-planet maximum-resolution resident raster.
- Runtime network downloads or online tile services.
- Changing collision, physics, reference frames, or the `TerrainSource`
  contract that they consume.
- Changing browser (`wasm`, no `dem`) terrain behavior in this change.

## Decisions

### Two-tier dataset instead of one high-resolution raster

Use a resident moderate-resolution global base plus streamed high-resolution
regional tiles. A global 15-30 m cube-sphere raster is hundreds of gigabytes and
cannot be resident; a single tier either loses close-range relief or loses
bounded memory. Alternatives considered: an all-resident high-res raster
(rejected: memory), and a pure runtime resample of source tiles (rejected:
non-deterministic availability and I/O in the query path).

### Offline cube-sphere payload pyramid

Build the pyramid offline with a new converter that resamples reviewed sources
onto the existing `body + face + level + tile_x + tile_y` identity and records
per-tile bounds and geometric error. This keeps runtime work to bounded
decoding and reuses cube-sphere topology and LOD error plumbing. Alternative
considered: an equirectangular raster (rejected: projection distortion and no
patch identity).

### I/O-free collision with declared fallback error

`height_m` samples the resident base plus any installed tiles and never triggers
a load. Non-resident regions return the coarse base with a conservative error
declared in the manifest, so landing and altitude checks stay physically honest.
Tiles are decoded by worker tasks and installed into a bounded resident set.
Alternative considered: synchronous on-demand loading in `height_m` (rejected:
blocks fixed-step collision and couples physics to I/O).

### Stream via the existing lifecycle

`TerrainStreamingResource` requests elevation tiles for selected and
ahead-of-path patches using the existing priority, cancellation, eviction, and
budget rules, with geometry publication keeping priority. This avoids a second
scheduler and cache.

### Per-tile error into `PatchGeometricError`

Payload tile metadata feeds the existing per-patch error path, replacing the
global envelope and removing the level-8 metadata ceiling for payload-covered
tiles. Non-covered patches keep the conservative envelope.

### Native gating with explicit fallback

The payload path is native (`dem`) only. Missing or invalid packages fall back
to the current resident CSDEM plus procedural detail; browser builds keep the
existing procedural source. No code path requires the new data to exist.

## Risks / Trade-offs

- [Render/collision divergence where a tile is not resident] -> Conservative
  declared fallback error, prefetch ahead of the flight path, and tests that
  assert resident/base agreement and bounded fallback rate.
- [Memory or decode budget growth] -> Bounded tile residency with deterministic
  LRU eviction, decode on worker tasks, and cadence-limited telemetry.
- [Non-deterministic offline output] -> Deterministic resampling, version and
  checksum verification, and byte-identical repeat-build tests.
- [Dataset size and licensing] -> Manifest-declared provenance and license,
  ignored local data, and a small reviewed global base with bounded regions.
- [I/O leaking into the query path] -> `height_m` reads resident data only;
  tests decode while a collision query runs and assert no blocking.

## Migration Plan

1. Keep the resident CSDEM active by default so all modes behave as today.
2. Add the payload package behind explicit manifest availability; a missing or
   invalid package falls back to the resident source.
3. Enable streamed tiles and per-tile LOD after verification tests pass.
4. Roll back by removing or disabling the package; no simulation state migrates.

## Open Questions

- The exact global base resolution and regional coverage set are chosen during
  implementation from measured memory and visual evidence; they do not change
  the specs or task breakdown.
