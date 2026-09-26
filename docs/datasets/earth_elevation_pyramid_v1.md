# Earth Elevation Pyramid v1

## Purpose

The Earth elevation pyramid is an offline-prepared, versioned, streamed
representation of the simulator's **one** terrain authority. It is not a second
authority and does not replace the resident base.

Earth's measured authority remains the resident
`assets/large_files/terrain/earth_etopo1_ice_surface_cs2048_v2.csdem`
(ETOPO1 Ice Surface, ~4.9 km effective ground sample distance; see
`earth_etopo1_ice_surface_v1.md`). The pyramid adds high-resolution regional
tiles that are decoded by worker tasks and installed into a bounded resident set
behind the existing `TerrainSource` boundary. Runtime sampling always composes:

```text
resident CSDEM base
        +
installed elevation payload tiles
        ↓
TileElevationSource
```

A missing or invalid package falls back to the resident base. The authority for
a given position is the resident base until a covering tile is installed; a tile
changes height only where it is resident. Collision, radar altitude, and physics
read resident data only and never load, decode, or block on payload I/O.

## Package Layout

A package is a directory containing a manifest and its tile payloads:

```text
<package>/
    manifest.ron
    tiles/<face>/<level>/<x>_<y>.cstile
```

- `<face>` is the cube-sphere face name; `<level>`, `<x>`, and `<y>` are the
  `TerrainPatch` address. Tile paths recorded in the manifest are relative to
  the package root and are authoritative; the layout above is the convention the
  converter produces.
- The manifest is RON (`ElevationPyramidManifest`).
- Large generated packages are not checked in; they live under the ignored
  `assets/large_files/terrain/` path.

## Manifest

`manifest.ron` records provenance, the target resolution, and the tile index.
All values are validated by `ElevationPyramidManifest::validate` before use.

| Field | Type | Units / meaning |
| --- | --- | --- |
| `format_version` | `u32` | Payload format version; must equal `1` (`ELEVATION_PYRAMID_FORMAT_VERSION`). |
| `body` | string | Body identifier, e.g. `Earth`. Must be non-empty. |
| `coordinate_frame` | string | Body-fixed frame in which tile addresses resolve. Must be non-empty. |
| `vertical_datum` | string | Vertical datum of the metre elevations, e.g. `mean sea level / EPSG:5715`. Must be non-empty. |
| `source` | string | Source dataset identifier/version the package was resampled from. Must be non-empty. |
| `license` | string | License of the source data. Must be non-empty. |
| `source_sha256` | string | SHA-256 of the reviewed source input, 64 lowercase/uppercase hex characters. |
| `resolution_m` | `f64` | Target ground sample distance in metres; finite and positive. |
| `tiles` | list | One `ElevationTileMetadata` per available tile. At least one; no duplicate addresses. |

Coverage is the set of declared tile addresses, not a separate field. The
manifest's `global_bounds()` is the union of all tile `min_elevation_m` /
`max_elevation_m` values.

### Per-tile metadata

| Field | Type | Units / meaning |
| --- | --- | --- |
| `patch` | `{face, level, tile_x, tile_y}` | Cube-sphere identity. `level <= 20` and `tile_x`/`tile_y` `< 2^level`. |
| `width`, `height` | `u32` | Samples per tile side; each `>= 2`. |
| `min_elevation_m`, `max_elevation_m` | `f64` | Signed metre elevation range over the tile; finite and `min <= max`. |
| `geometric_error_m` | `f64` | Conservative geometric error for LOD; finite and `>= 0`. |
| `payload_path` | string | Payload path relative to the package root. Must be non-empty. |
| `payload_sha256` | string | SHA-256 of the payload sample bytes (header excluded), 64 hex characters. |

Tiles are addressed by `body + face + level + tile_x + tile_y`, reusing the
existing cube-sphere topology and `PatchGeometricError` LOD plumbing.

## Tile Binary Format

Each `.cstile` file is a 92-byte fixed header followed by the sample payload.
All multi-byte values are **little-endian**.

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 8 | Magic `CSTILE\0\0` |
| 8 | 4 | `format_version` (`u32`) |
| 12 | 4 | Face index (`u32`): `0 PosX`, `1 NegX`, `2 PosY`, `3 NegY`, `4 PosZ`, `5 NegZ` (`CubeFace::ALL` order) |
| 16 | 4 | `level` (`u32`) |
| 20 | 4 | `tile_x` (`u32`) |
| 24 | 4 | `tile_y` (`u32`) |
| 28 | 4 | `width` (`u32`) |
| 32 | 4 | `height` (`u32`) |
| 36 | 8 | `min_elevation_m` (`f64`) |
| 44 | 8 | `max_elevation_m` (`f64`) |
| 52 | 8 | `geometric_error_m` (`f64`) |
| 60 | 32 | SHA-256 of the payload bytes that follow |
| 92 | `width * height * 4` | Samples: `f32` little-endian signed metres, row-major on the tile's face-UV grid |

Decoding requires the exact payload length `width * height * 4`; a length or
checksum mismatch is an explicit error. Decoding is a deterministic function of
the payload bytes, independent of cache state and residency.

### Version policy

`format_version` is declared in both `manifest.ron` and every tile header. A
manifest or tile whose version is not the current `ELEVATION_PYRAMID_FORMAT_VERSION`
(1) is rejected; no partial terrain is used. Any incompatible change to header
layout, byte order, sample encoding, or addressing requires a new version
number and a regenerated package.

## Required Provenance

- Body and body-fixed frame the samples are expressed in.
- Vertical datum of the stored metre elevations.
- Source dataset identity and the source license.
- Source SHA-256 of the reviewed input.
- Target resolution (`resolution_m`) and coverage (declared tile addresses).
- Per-tile minimum and maximum elevation and conservative geometric error.

Values that depend on the offline build are recorded when the package is
generated, for example:

- Generated package content SHA-256: `TBD after first production conversion`.
- Production `coordinate_frame` / `vertical_datum` strings: `TBD after first production conversion`.

## Offline Conversion

The pyramid is produced by the offline `elevation_pyramid_convert` binary from a
resident `.csdem` authority. It resamples the input deterministically onto the
cube-sphere, computes each tile's elevation bounds and conservative geometric
error, writes the `.cstile` payloads, and emits `manifest.ron`.

```sh
cargo run --features dem --bin elevation_pyramid_convert -- \
  <input.csdem> <output-dir> <max-level> <tile-samples>
```

Arguments, in order: the input `.csdem`, the output package directory, the
maximum cube-sphere level, and the tile resolution (samples per tile side).

This is an **offline preparation** step. No simulator path downloads or fetches
terrain data at runtime; the runtime only validates, deserializes, and samples
local package files. Generated package SHA-256 values are recorded here after
the first production conversion: `TBD after first production conversion`.

## Fallback

The resident CSDEM remains active by default. If the package is absent, invalid,
or a requested tile is not resident, sampling returns the resident base with its
declared conservative error. Native builds without the payload behave exactly as
before; browser (`wasm`, no `dem`) builds keep the existing procedural source.
