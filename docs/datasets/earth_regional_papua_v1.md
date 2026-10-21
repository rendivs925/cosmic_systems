# Earth Regional Papua v1

First Phase 3 regional elevation package. It covers the Papua coastal lowland
already used by the Sentinel-2 imagery package and adds high-resolution relief
over the global ETOPO 2022 base.

## Region

- Id: `papua_coastal_lowland`
- Bounds: `139.0..140.0E`, `8.6S..7.4S`
- Config: `assets/configs/terrain/regions_v1.ron`
- Source: NASA MEaSUREs NASADEM `NASADEM_HGT` v001, 1 arc-second bare-earth
- Public mirror: OpenTopography
  <https://opentopography.s3.sdsc.edu/raster/NASADEM/NASADEM_be/>
- License: public domain (NASA/NASADEM)
- Attribution: NASA MEaSUREs NASADEM; OpenTopography public mirror

The two source tiles are:

| Tile | URL | SHA-256 |
| --- | --- | --- |
| `s08e139` | `NASADEM_HGT_s08e139.tif` | `32de0bb1cae25d5df1400827ccc1a65601d58b7241bf726e0a1ba169807798de` |
| `s09e139` | `NASADEM_HGT_s09e139.tif` | `54abf0dfd106b450031ba7885ae00081e8242725ac329f952b9e73200f7fa306` |

Combined source SHA-256:
`1d353269f0710334541f4404eafc4c3d7531f242bcd45873bef9fd7779455625`.

## Vertical datum

NASADEM stores EGM96 orthometric heights. The package adds the runtime EGM2008
geoid undulation to express each sample on the WGS 84 ellipsoid, matching the
global authority. The EGM96-to-EGM2008 geoid-model difference is below the
NASADEM vertical accuracy and is not modelled separately; absolute survey
control at the Papua site is still required for survey-grade claims.

Sea level remains the shared 0.5 degree EGM2008 grid:

`assets/large_files/terrain/earth_egm2008_geoid_0p5deg_i16cm.raw`

SHA-256:
`f4ea75f9721eee1d0a3fea97a05d9dda6c166bed8300e73f8e7163d1cf59ffea`.

## Runtime package

The runtime index is:

`assets/large_files/terrain/earth_elevation_pyramid/manifest.ron`

Payload tiles are sparse and organized by region:

`assets/large_files/terrain/earth_elevation_pyramid/regions/papua_coastal_lowland/tiles/`

- Levels: 9 through 12
- Tile resolution: 64 x 64 face-UV samples with shared edges
- Tiles: 5,295
- Payload size: 90,161,146 bytes (86.0 MiB)
- Aggregate runtime SHA-256
  (sorted `<file-sha256>  <relative-path>` listing):
  `74c3c99a09ebd4c74f53a2b984f8552a2dbd7501f7adbbf34743e18ebd0aa784`
- Height range: 10.1 m to 130.4 m ellipsoidal

The existing runtime composes one elevation payload root over the resident
ETOPO 2022 package. Adding another region requires extending the runtime to
compose multiple per-region manifests; until then this package owns the root.

## Bake

```sh
python3 scripts/ingest_regional_dem.py --region papua_coastal_lowland
cargo run --release --features dem --bin regional_dem_bake -- \
    --region papua_coastal_lowland \
    --config assets/configs/terrain/regions_v1.ron \
    --root . \
    --output assets/large_files/terrain/earth_elevation_pyramid \
    --verify --bench-load --probe -8.0 139.5
```

The baker verifies each source checksum, adds the geoid, discovers every
cube-sphere patch intersecting the region, samples 64 x 64 shared-edge grids,
and writes a deterministic manifest. `--verify` re-reads the manifest, checks
coverage across the reviewed bounds, and compares 12,000 independent points
against the source. Papua acceptance is RMSE 0.109 m and max absolute error
0.866 m, below the 1.0 m / 5.0 m gates. `--bench-load` measures manifest and
tile decode latency; the current 128-tile run is 19.9 ms for the manifest and
p95 0.020 ms per tile, below the 50 ms gate. `--probe` decodes the covering tile
and prints its composed height; the presentation site (`-8.0, 139.5`) returns
71.963 m.

Rocket-mode metrics with the package active (`COSMIC_SYSTEMS_PERFORMANCE_METRICS=1`)
reach 93 resident payload tiles, zero fallback for requested coverage, and
111.5 MiB resident geometry against the 160 MiB budget. Decoding runs on worker
tasks; terrain queries sample resident data only.

Runtime composition accepts several package roots through
`EarthTerrainSource::with_dem_and_elevation_manifest_roots` and
`TileElevationSource::from_manifest_roots`. Roots are ordered: the finest tile
level wins, and a later root wins at an identical patch address. Decoded tiles
share one bounded LRU. The default root list currently contains only
`earth_elevation_pyramid`. Source rasters and the baked package remain
under gitignored `assets/large_files/`.

## Validation

```sh
cargo run --release --features dem --bin preprocess -- \
    --region papua_coastal_lowland --check --report-size
```

The registry entry is `earth_regional_dem_papua`. It records the combined source
checksum, the aggregate runtime checksum, the geoid auxiliary asset, and the
region bounds. Missing, stale, or mismatched payload files fail the check.

Known limits:

- NASADEM is a DSM/DTM product with forest and settlement effects in some
  cells; it is the reviewed bare-earth regional authority, not a surveyed
  surface.
- The package covers only the reviewed bounds. Outside them the global ETOPO
  2022 base remains authoritative.
- The payload replaces the regional base only where a tile is resident; the
  measured global package is always the fallback.
