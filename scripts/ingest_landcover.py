#!/usr/bin/env python3
"""Prepare ESA WorldCover for an offline `.clcvr` land-cover package.

The source is a pinned WorldCover 3-degree tile. This script downloads it once
into the ignored raw-asset tree, verifies its SHA-256, crops the requested
region, and resamples classes onto the planetocentric (terrain-radial) latitude
grid used by the simulator. Class ids are nearest-neighbour sampled because the
values are categorical.

The normalized PNG plus the printed `land_cover_bake` command are the only
inputs to `src/bin/land_cover_bake.rs`. No download happens at runtime.
"""

from __future__ import annotations

import argparse
import hashlib
import math
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image

Image.MAX_IMAGE_PIXELS = None

WGS84_E2 = 0.006_694_379_990_141_316_5
WORLDCOVER_TO_CLASS = {
    10: 0,   # tree cover
    20: 1,   # shrubland
    30: 2,   # grassland
    40: 3,   # cropland
    50: 4,   # built-up
    60: 5,   # bare / sparse
    70: 6,   # snow / ice
    80: 7,   # permanent water
    90: 8,   # herbaceous wetland
    95: 9,   # mangroves
    100: 10, # moss / lichen
}
NODATA_CLASS = 5


def parse_bounds(text: str) -> tuple[float, float, float, float]:
    values = tuple(float(part) for part in text.split(","))
    if len(values) != 4:
        raise argparse.ArgumentTypeError("expected west,south,east,north")
    west, south, east, north = values
    if not (west < east and south < north):
        raise argparse.ArgumentTypeError("bounds must be west<east and south<north")
    return values


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fetch_tile(url: str, path: Path, expected_sha256: str) -> None:
    if path.exists() and sha256(path) == expected_sha256:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".part")
    print(f"downloading {url}")
    urllib.request.urlretrieve(url, temporary)
    actual = sha256(temporary)
    if actual != expected_sha256:
        temporary.unlink(missing_ok=True)
        raise SystemExit(f"source SHA-256 mismatch: {actual} != {expected_sha256}")
    temporary.replace(path)


def terrain_to_geodetic(latitude_deg: np.ndarray) -> np.ndarray:
    return np.degrees(np.arctan(np.tan(np.radians(latitude_deg)) / (1.0 - WGS84_E2)))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--region", required=True)
    parser.add_argument("--tile-url", required=True)
    parser.add_argument("--tile-sha256", required=True)
    parser.add_argument("--tile-bounds", type=parse_bounds, required=True,
                        help="source geodetic west,south,east,north")
    parser.add_argument("--bounds", type=parse_bounds, required=True,
                        help="output terrain-radial west,south,east,north")
    parser.add_argument("--resolution-deg", type=float, default=1.0 / 3600.0)
    parser.add_argument("--tile-path", type=Path, required=True)
    parser.add_argument("--output-png", type=Path, required=True)
    arguments = parser.parse_args()

    if arguments.resolution_deg <= 0.0:
        raise SystemExit("--resolution-deg must be positive")
    if not arguments.tile_path.exists() or sha256(arguments.tile_path) != arguments.tile_sha256:
        fetch_tile(arguments.tile_url, arguments.tile_path, arguments.tile_sha256)

    west, south, east, north = arguments.bounds
    tile_west, tile_south, tile_east, tile_north = arguments.tile_bounds
    source = Image.open(arguments.tile_path)
    source_width, source_height = source.size
    pixel_x = (tile_east - tile_west) / source_width
    pixel_y = (tile_north - tile_south) / source_height

    width = round((east - west) / arguments.resolution_deg) + 1
    height = round((north - south) / arguments.resolution_deg) + 1
    terrain_latitudes = north - np.arange(height) * arguments.resolution_deg
    geodetic_latitudes = terrain_to_geodetic(terrain_latitudes)
    xs = np.rint((np.arange(width) * arguments.resolution_deg + west - tile_west) / pixel_x)
    ys = np.rint((tile_north - geodetic_latitudes) / pixel_y)
    if xs.min() < 0 or ys.min() < 0 or xs.max() >= source_width or ys.max() >= source_height:
        raise SystemExit("requested bounds are not fully covered by the source tile")

    left, right = int(xs.min()), int(xs.max()) + 1
    top, bottom = int(ys.min()), int(ys.max()) + 1
    crop = np.asarray(source.crop((left, top, right, bottom)))
    lut = np.full(256, NODATA_CLASS, dtype=np.uint8)
    for source_code, class_code in WORLDCOVER_TO_CLASS.items():
        lut[source_code] = class_code
    rows = ys.astype(np.int64) - top
    columns = xs.astype(np.int64) - left
    classes = lut[crop[np.ix_(rows, columns)]]

    arguments.output_png.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(classes, mode="L").save(arguments.output_png)
    print(f"wrote {arguments.output_png}: {width}x{height}")
    unique, counts = np.unique(classes, return_counts=True)
    print("classes:", {int(k): int(v) for k, v in zip(unique, counts)})
    print("normalized_png_sha256:", sha256(arguments.output_png))
    print(
        "bake:",
        "cargo run --release --features native --bin land_cover_bake --",
        arguments.output_png,
        f"assets/large_files/terrain/landcover_v1/{arguments.region}.clcvr",
        f"{west:.9f}", f"{south:.9f}", f"{east:.9f}", f"{north:.9f}",
        arguments.tile_sha256,
        f"'ESA WorldCover 2021 v200, tile {arguments.tile_path.stem}'",
        "'CC BY 4.0'",
        "10.0",
    )


if __name__ == "__main__":
    main()
