## Context

See proposal.md. ETOPO1 is a resident global 1-arc-minute CSDEM, while KSC
requires measured meter-scale data. The existing cube-sphere terrain source,
geometry, collision, streaming lifecycle, body orientation, and render origin
already form the correct authority path. The temporary `CSMSH` KSC child mesh
does not: it bypasses that path and can disagree with collision. The static
flight globe remains only a distant visual basemap; it cannot be the local
terrain authority.

## Goals / Non-Goals

**Goals:**
- Add a read-only, sparse local DEM overlay to the shared Earth source.
- Use 3DEP 1 m bare-earth data for KSC when a reviewed package is provisioned.
- Preserve ETOPO1 fallback, body-fixed cube-sphere coordinates, deterministic
  interpolation, and bounded terrain lifecycle ownership.
- Support NAIP imagery and future KSC structures as presentation payloads.

**Non-Goals:**
- Runtime downloads, GeoTIFF decoding in simulation, a global high-resolution
  DEM, synthetic relief inside measured local coverage, or collision against
  visual structures.
- Delivering unverified external 3DEP data or fabricated building geometry.

## Decisions

### Sparse immutable elevation package

An offline converter will ingest a pre-normalized local raster and write a
versioned package keyed by the existing `TerrainPatch` cube-sphere identity.
It stores relative elevations and nodata masks, not absolute planet coordinates,
so the source remains f64 until the render boundary. This extends the current
CSDEM approach rather than introducing a tile manager or per-sample I/O.

USGS 3DEP 1 m bare-earth DEM is selected for KSC. NASA SRTM is retained as a
possible global/regional source but its ~30 m resolution is inadequate for the
low-relief launch site. The selected 3DEP product must document its vertical
datum; a conversion to the terrain datum cannot be assumed or hardcoded.

### One terrain authority

`EarthTerrainSource` will select local measured elevation wherever the package
has a valid sample. Outside local coverage, its existing global composition is
retained. Existing site calibration is applied after selection only within its
minimal engineering footprint. The static KSC mesh baker and loader are removed.

### Bounded local refinement

The existing quadtree, stitch topology, task lifecycle, and render assets remain
the owner. The selection policy will cap global work while allowing KSC coverage
to reach level 17 initially, yielding approximately 2-3 m grid spacing with the
current 33 by 33 mesh grids. Higher levels require profile evidence.

### KSC structures are separate presentation assets

Infrastructure uses geodetic anchors transformed through the existing Earth
body orientation and render origin. Models are discrete assets with explicit
LOD, visibility, and provenance. They never change the bare-earth terrain
height or become landing collision authority.

## Risks / Trade-offs

- [3DEP vertical datum differs from ETOPO1/launch configuration] → Record the
  selected product datum and validate control points before enabling it.
- [Local package edge introduces a height seam] → Store coverage/nodata masks,
  deterministically blend only over an explicit border, and test the boundary.
- [Level 17 exceeds worker or GPU budgets] → Apply only in local coverage and
  validate existing telemetry before raising the level.
- [Distant globe and terrain overlap] → Keep one explicit render handoff and
  never introduce a local overlay mesh outside terrain rendering.
- [Buildings are mistaken for bare-earth terrain] → Maintain separate asset and
  collision ownership; validate their body-fixed anchors.

## Migration Plan

1. Remove the temporary visual-only KSC mesh now.
2. Add the package reader, source overlay, converter, manifest, and tests.
3. Provision and verify a reviewed 3DEP package offline, then enable it through
   explicit configuration; a missing optional package retains ETOPO1 coverage.
4. Enable bounded local refinement and NAIP pyramid after package tests and
   native-display telemetry pass.
5. Add authored, provenance-backed KSC structure assets incrementally. Rollback
   removes the local package configuration, retaining global ETOPO1 terrain.
