# Erosion Preprocessing

Earth's erosion and hydrology field is a deterministic, pure function of the
measured base terrain, the erosion configuration, and the tile key. It can
therefore be simulated once and reused instead of re-baked at runtime. This
document describes the offline bake tool, the on-disk cache it writes, and the
known limitations.

## Why it exists

Terrain mesh generation samples height at every vertex of every patch,
including coarse LOD patches that span whole cube faces. Building the coarse
planetary cover therefore samples thousands of distinct 2-degree erosion tiles
across the whole globe. The runtime tile cache holds only
`ErosionConfig::cache_max_tiles` (64 by default) rasters, so most of those tiles
are evicted and re-simulated. Persisting them removes that repeated CPU cost.

## Quality presets

| Preset | Resolution | Droplets | Thermal iters | Cache tiles |
|---|---:|---:|---:|---:|
| `standard` | 64 | 3,500 | 3 | 64 |
| `high` | 128 | 12,000 | 5 | 96 |
| `ultra` | 256 | 48,000 | 8 | 128 |

Presets change only how finely the *same* deterministic field is sampled and
how many hydraulic droplets are simulated. They do not change the terrain
authority, and higher settings are not evidence of greater physical realism.

At a 2-degree tile, `ultra`'s 256 samples are about **870 m apart near the
equator**. It is a denser erosion grid, not landing-scale detail.

## Bake tool

```sh
cargo run --release --features dem --bin erosion_bake -- \
  --cache <dir> [--quality standard|high|ultra] [--max-bytes <n>] \
  [--all | --region <lat_min> <lat_max> <lon_min> <lon_max>]
```

Example (Papua launch region):

```sh
cargo run --release --features dem --bin erosion_bake -- \
  --cache ~/.cache/cosmic_systems/erosion --quality high --region -10 -4 138 142
```

The tool prints estimated bytes, per-100-tile progress, and a final
`baked`/`reused`/`total`/`write_failures` summary. Re-running with the same
configuration reuses only tiles that still decode *and* whose base probe matches
the live DEM; a corrupt or stale tile is re-baked. `--max-bytes` bounds the cache
directory; the oldest tiles (by mtime) are pruned first when it is exceeded, and
the ceiling is re-applied as tiles are written so a long run stays bounded.

## Runtime cache

| Variable | Meaning |
|---|---|
| `COSMIC_SYSTEMS_EROSION_QUALITY` | `standard` (default), `high`, or `ultra`. Must match the bake. |
| `COSMIC_SYSTEMS_EROSION_CACHE` | Cache directory, or `off` to disable persistence. Defaults to `$XDG_CACHE_HOME/cosmic_systems/erosion` or `$HOME/.cache/cosmic_systems/erosion`. |
| `COSMIC_SYSTEMS_EROSION_CACHE_MAX_BYTES` | Disk ceiling; default 4 GiB. |

## Tile identity and validation

Each tile stores a **bake signature** (every configuration field that changes
the raster, plus a DEM-path namespace) and a **base probe** (a 4x4 grid of
pre-erosion base heights across the tile). A cached tile is used only when:

- the file magic/version and signature match;
- the stored tile key and bounds agree with the request;
- the payload checksum and file length are correct;
- all channel values are finite and moisture is within `[0, 1]`;
- the base probe still matches the live base.

Any failure is treated as a miss and the runtime re-bakes. Writes are atomic
(write to a unique temp file, then rename) and are **not** `fsync`ed: this is a
regenerable cache, so losing the newest tiles on a crash is acceptable, while
syncing thousands of tiles would dominate bake time.

## Known limitations

- **The base probe is not a content hash.** A base that changes only between the
  sampled grid points would not be detected. It is a practical staleness guard,
  not a cryptographic guarantee.
- **Cache misses still do synchronous I/O** in the height-query path: a miss
  reads the tile file (or simulates the tile) inline. Moving loading and
  generation fully off the physics path is not yet done.
- **The whole planet is sampled for coarse LODs.** Persistence removes repeated
  simulation, but the runtime still touches thousands of tiles per run because
  coarse patches sample the whole globe.
- **Quality presets do not create real-world detail.** Higher resolution only
  refines the synthetic erosion of the measured base. Landing-scale realism
  requires measured high-resolution data, not a denser erosion grid.
- **Old signature directories are never deleted.** Changing quality or DEM
  creates a new namespace; the previous one remains until removed manually.

## Not yet done

- Async tile loading / generation so physics queries never block on disk.
- Serving a preprocessed global overview to coarse LODs instead of sampling
  full-resolution erosion planet-wide.
- Measured high-resolution terrain for launch and landing regions.
