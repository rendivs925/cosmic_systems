#!/usr/bin/env python3
"""Normalize the KSC 1 m 3DEP tile to WGS 84 ellipsoidal 1 arc-second heights.

The USGS tile is NAD83 / UTM zone 17N with NAVD88 heights. This offline step
reprojects it to a north-up WGS 84 geographic grid and applies a local
NAVD88-to-WGS84 correction derived from NOAA VDatum. The output is a checked
source raster for `regional_dem_bake`; the simulator never reads the raw tile.

The output covers the launch site and LC-39A; the wider source tile also
contains 3DEP nodata over water, so only the reviewed land rectangle is emitted.
The vertical correction is a plane fitted to a 3x3 VDatum sample grid over the
tile (residual < 1 cm). VDatum reported 0.06 m uncertainty.
"""

from __future__ import annotations

import argparse
import hashlib
import math
from pathlib import Path

import numpy as np
from PIL import Image

SOURCE_SHA256 = "66ccf1c4473b75cfa2f21cc682b3ccaf5dc391eb3b26df1555c57777c16b95d7"
SOURCE_TIE_X_M = 529_993.9999932938
SOURCE_TIE_Y_M = 3_170_006.000054804
SOURCE_PIXEL_M = 1.0
WEST_DEG = -80.68
NORTH_DEG = 28.62
PIXEL_DEG = 1.0 / 3600.0
WIDTH = 324
HEIGHT = 191
NODATA_M = -999_999.0
# Plane fit to NOAA VDatum NAVD88 -> WGS84(G1674) ellipsoid offsets sampled on
# 2026-10-21 at a 3x3 grid; offset_m = c + lon * lon_coeff + lat * lat_coeff.
OFFSET_C = -111.26289387
OFFSET_LON = -1.11881188
OFFSET_LAT = -0.30337079

A_M = 6_378_137.0
F = 1.0 / 298.257223563
K0 = 0.9996
CENTRAL_MERIDIAN_DEG = -81.0


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def utm_zone_17n(latitude_deg: np.ndarray, longitude_deg: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    e2 = F * (2.0 - F)
    ep2 = e2 / (1.0 - e2)
    lat = np.radians(latitude_deg)
    lon = np.radians(longitude_deg)
    lon0 = math.radians(CENTRAL_MERIDIAN_DEG)
    sin_lat = np.sin(lat)
    cos_lat = np.cos(lat)
    tan_lat = np.tan(lat)
    n = A_M / np.sqrt(1.0 - e2 * sin_lat * sin_lat)
    t = tan_lat * tan_lat
    c = ep2 * cos_lat * cos_lat
    a = (lon - lon0) * cos_lat
    m = A_M * (
        (1.0 - e2 / 4.0 - 3.0 * e2**2 / 64.0 - 5.0 * e2**3 / 256.0) * lat
        - (3.0 * e2 / 8.0 + 3.0 * e2**2 / 32.0 + 45.0 * e2**3 / 1024.0) * np.sin(2.0 * lat)
        + (15.0 * e2**2 / 256.0 + 45.0 * e2**3 / 1024.0) * np.sin(4.0 * lat)
        - (35.0 * e2**3 / 3072.0) * np.sin(6.0 * lat)
    )
    easting = K0 * n * (
        a
        + (1.0 - t + c) * a**3 / 6.0
        + (5.0 - 18.0 * t + t * t + 72.0 * c - 58.0 * ep2) * a**5 / 120.0
    ) + 500_000.0
    northing = K0 * (
        m
        + n
        * tan_lat
        * (
            a * a / 2.0
            + (5.0 - t + 9.0 * c + 4.0 * c * c) * a**4 / 24.0
            + (61.0 - 58.0 * t + t * t + 600.0 * c - 330.0 * ep2) * a**6 / 720.0
        )
    )
    return easting, northing


def bilinear(source: np.ndarray, x: np.ndarray, y: np.ndarray) -> np.ndarray:
    height, width = source.shape
    x = np.clip(x, 0.0, width - 1.0)
    y = np.clip(y, 0.0, height - 1.0)
    x0 = np.floor(x).astype(np.int64)
    y0 = np.floor(y).astype(np.int64)
    x1 = np.minimum(x0 + 1, width - 1)
    y1 = np.minimum(y0 + 1, height - 1)
    tx = x - x0
    ty = y - y0
    a = source[y0, x0]
    b = source[y0, x1]
    c = source[y1, x0]
    d = source[y1, x1]
    return (a + (b - a) * tx) * (1.0 - ty) + (c + (d - c) * tx) * ty


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "source",
        nargs="?",
        default="assets/large_files/terrain/USGS_1M_17_x53y317_FL_Peninsular_FDEM_2018_D19_DRRA.tif",
    )
    parser.add_argument(
        "--output",
        default="assets/large_files/terrain/raw/ksc_3dep/ksc_3dep_ellipsoid_wgs84_1arcsec.tif",
    )
    args = parser.parse_args()
    source_path = Path(args.source)
    output_path = Path(args.output)

    found = sha256(source_path)
    if found != SOURCE_SHA256:
        raise SystemExit(f"source checksum mismatch: expected {SOURCE_SHA256}, found {found}")

    Image.MAX_IMAGE_PIXELS = None
    with Image.open(source_path) as image:
        if image.mode != "F":
            raise SystemExit(f"expected 32-bit float source, found mode {image.mode}")
        source = np.asarray(image, dtype=np.float32)

    rows = np.arange(HEIGHT, dtype=np.float64) + 0.5
    cols = np.arange(WIDTH, dtype=np.float64) + 0.5
    latitude_deg = NORTH_DEG - rows * PIXEL_DEG
    longitude_deg = WEST_DEG + cols * PIXEL_DEG
    lat_grid, lon_grid = np.meshgrid(latitude_deg, longitude_deg, indexing="ij")

    easting, northing = utm_zone_17n(lat_grid, lon_grid)
    source_x = (easting - SOURCE_TIE_X_M) / SOURCE_PIXEL_M - 0.5
    source_y = (SOURCE_TIE_Y_M - northing) / SOURCE_PIXEL_M - 0.5
    navd88_m = bilinear(source, source_x, source_y)
    if np.any(navd88_m <= NODATA_M + 1.0):
        raise SystemExit("normalized extent intersects 3DEP nodata")

    offset_m = OFFSET_C + lon_grid * OFFSET_LON + lat_grid * OFFSET_LAT
    ellipsoid_m = (navd88_m + offset_m).astype(np.float32)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    temporary = output_path.with_suffix(output_path.suffix + ".part")
    Image.fromarray(ellipsoid_m, mode="F").save(
        temporary, format="TIFF", compression="tiff_deflate"
    )
    temporary.replace(output_path)

    print(
        f"wrote {output_path} {WIDTH}x{HEIGHT} 1 arc-second WGS84 ellipsoid, "
        f"min {ellipsoid_m.min():.3f} m max {ellipsoid_m.max():.3f} m"
    )
    print(f"sha256 {sha256(output_path)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
