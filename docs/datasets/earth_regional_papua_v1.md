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
- Tiles: 5,257
- Payload size: 89,513,989 bytes (85.4 MiB)
- Aggregate runtime SHA-256
  (sorted `<file-sha256>  <relative-path>` listing):
  `c61f777af9c761b0199f57ff22bcdca32efea3c560f55943a0725221c3498bb7`
- Height range: 13.3 m to 131.0 m ellipsoidal

The runtime composes this root with the other regional packages over the
resident ETOPO 2022 authority. Later roots refine identical patches, and the
finest declared tile covering a patch wins.

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
coverage across the reviewed bounds, and compares 11,900 independent points
against the source. Papua acceptance is RMSE 0.163 m and max absolute error
2.457 m, below the 1.0 m / 5.0 m gates. `--bench-load` measures manifest and
tile decode latency; the current 128-tile run is 111.3 ms for the manifest and
p95 0.102 ms per tile, below the 50 ms gate. `--probe` decodes the covering tile
and prints its composed height; the presentation site (`-8.0, 139.5`) returns
71.978 m.

Decoding runs on worker tasks; terrain queries sample resident data only. The
launch site's covering payload tile is loaded synchronously once at spawn so pad
altitude comes from the regional package rather than the global fallback.

Runtime composition accepts several package roots through
`EarthTerrainSource::with_dem_and_elevation_manifest_roots` and
`TileElevationSource::from_manifest_roots`. Roots are ordered: the finest tile
level wins, and a later root wins at an identical patch address. Decoded tiles
share one bounded LRU. The default root list composes
`earth_elevation_pyramid`, `earth_elevation_pyramid_ksc`,
`earth_elevation_pyramid_ksc_3dep`, `earth_elevation_pyramid_starbase`,
`earth_elevation_pyramid_vandenberg`, `earth_elevation_pyramid_wallops`,
`earth_elevation_pyramid_kourou`, `earth_elevation_pyramid_baikonur`, and
`earth_elevation_pyramid_mahia`. Source rasters and the baked package remain
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
