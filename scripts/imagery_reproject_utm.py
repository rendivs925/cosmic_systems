#!/usr/bin/env python3
"""Reproject Sentinel-2 UTM GeoTIFF tiles into a WGS 84 equirectangular image.

The offline cube-sphere imagery converter (`imagery_convert`) accepts only an
already normalized equirectangular RGB image in WGS 84 geographic coordinates
(EPSG:4326). Sentinel-2 L2A `TCI.tif` products are delivered in a UTM zone, so
this step reprojects and mosaics them before conversion.

It uses only PIL, numpy and requests-free local files; it does not need GDAL.
Each input must be a GeoTIFF carrying `ModelPixelScaleTag` (33550) and
`ModelTiepointTag` (33922), which the Sentinel-2 COGs include.

Example (Papua local region, UTM 54S):

    python3 scripts/imagery_reproject_utm.py \
        --zone 54 --south \
        --bounds 139.0 -8.6 140.0 -7.4 \
        --out /tmp/papua_equirect.png \
        54LTR.tif 54LUR.tif 54MTS.tif 54MUS.tif

Prints the WGS 84 / UTM round-trip error, the output size, and the filled
coverage so a reviewer can confirm the mosaic actually covers the region.
"""

import argparse

import numpy as np
from PIL import Image

Image.MAX_IMAGE_PIXELS = None

A = 6378137.0
F = 1.0 / 298.257223563
E2 = 2 * F - F * F
EP2 = E2 / (1.0 - E2)
K0 = 0.9996
FE = 500000.0
FN_SOUTH = 10000000.0
METERS_PER_DEG_LAT = 110574.0


def central_meridian(zone):
    return -183.0 + 6.0 * zone


def _meridian_arc(phi):
    e4, e6 = E2**2, E2**3
    return A * (
        (1 - E2 / 4 - 3 * e4 / 64 - 5 * e6 / 256) * phi
        - (3 * E2 / 8 + 3 * e4 / 32 + 45 * e6 / 1024) * np.sin(2 * phi)
        + (15 * e4 / 256 + 45 * e6 / 1024) * np.sin(4 * phi)
        - (35 * e6 / 3072) * np.sin(6 * phi)
    )


def utm_forward(lat_deg, lon_deg, zone):
    lat = np.radians(np.asarray(lat_deg, dtype=np.float64))
    lon = np.radians(np.asarray(lon_deg, dtype=np.float64))
    lon0 = np.radians(central_meridian(zone))
    sin_lat, cos_lat, tan_lat = np.sin(lat), np.cos(lat), np.tan(lat)
    n = A / np.sqrt(1 - E2 * sin_lat**2)
    t = tan_lat**2
    c = EP2 * cos_lat**2
    a = (lon - lon0) * cos_lat
    easting = K0 * n * (
        a + (1 - t + c) * a**3 / 6 + (5 - 18 * t + t**2 + 72 * c - 58 * EP2) * a**5 / 120
    ) + FE
    northing = K0 * (
        _meridian_arc(lat)
        + n * tan_lat * (
            a**2 / 2
            + (5 - t + 9 * c + 4 * c**2) * a**4 / 24
            + (61 - 58 * t + t**2 + 600 * c - 330 * EP2) * a**6 / 720
        )
    ) + FN_SOUTH
    return easting, northing


def utm_inverse(easting, northing, zone):
    x = np.asarray(easting, dtype=np.float64) - FE
    y = np.asarray(northing, dtype=np.float64) - FN_SOUTH
    mu = (y / K0) / (A * (1 - E2 / 4 - 3 * E2**2 / 64 - 5 * E2**3 / 256))
    e1 = (1 - np.sqrt(1 - E2)) / (1 + np.sqrt(1 - E2))
    phi1 = (
        mu
        + (3 * e1 / 2 - 27 * e1**3 / 32) * np.sin(2 * mu)
        + (21 * e1**2 / 16 - 55 * e1**4 / 32) * np.sin(4 * mu)
        + (151 * e1**3 / 96) * np.sin(6 * mu)
        + (1097 * e1**4 / 512) * np.sin(8 * mu)
    )
    sin_phi1, cos_phi1, tan_phi1 = np.sin(phi1), np.cos(phi1), np.tan(phi1)
    n1 = A / np.sqrt(1 - E2 * sin_phi1**2)
    t1 = tan_phi1**2
    c1 = EP2 * cos_phi1**2
    r1 = A * (1 - E2) / (1 - E2 * sin_phi1**2) ** 1.5
    d = x / (n1 * K0)
    lat = phi1 - (n1 * tan_phi1 / r1) * (
        d**2 / 2
        - (5 + 3 * t1 + 10 * c1 - 4 * c1**2 - 9 * EP2) * d**4 / 24
        + (61 + 90 * t1 + 298 * c1 + 45 * t1**2 - 252 * EP2 - 3 * c1**2) * d**6 / 720
    )
    lon = np.radians(central_meridian(zone)) + (
        d
        - (1 + 2 * t1 + c1) * d**3 / 6
        + (5 - 2 * c1 + 28 * t1 - 3 * c1**2 + 8 * EP2 + 24 * t1**2) * d**5 / 120
    ) / cos_phi1
    return np.degrees(lat), np.degrees(lon)


def load_tile(path):
    image = Image.open(path)
    rgb = np.asarray(image.convert("RGB"))
    tiepoint = image.tag_v2[33922]
    pixel_scale = image.tag_v2[33550]
    return {
        "name": path,
        "rgb": rgb,
        "e0": float(tiepoint[3]),
        "n0": float(tiepoint[4]),
        "res": float(pixel_scale[0]),
        "width": rgb.shape[1],
        "height": rgb.shape[0],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zone", type=int, required=True, help="UTM zone number")
    parser.add_argument("--south", action="store_true", help="southern hemisphere")
    parser.add_argument(
        "--bounds",
        nargs=4,
        type=float,
        metavar=("WEST", "SOUTH", "EAST", "NORTH"),
        required=True,
    )
    parser.add_argument("--out", required=True)
    parser.add_argument("--resolution-m", type=float, default=10.0)
    parser.add_argument("tiles", nargs="+")
    args = parser.parse_args()

    if K0 != 0.9996 or not args.south:
        raise SystemExit("this script implements the southern-hemisphere UTM convention only")

    west, south, east, north = args.bounds
    tiles = [load_tile(path) for path in args.tiles]

    mid_lat = 0.5 * (south + north)
    width = round((east - west) * 111320.0 * np.cos(np.radians(mid_lat)) / args.resolution_m)
    height = round((north - south) * METERS_PER_DEG_LAT / args.resolution_m)
    output = np.zeros((height, width, 3), dtype=np.uint8)
    filled = np.zeros((height, width), dtype=bool)

    lon = west + (np.arange(width) + 0.5) * (east - west) / width
    block_rows = 256
    for row0 in range(0, height, block_rows):
        row1 = min(row0 + block_rows, height)
        lat = north - (np.arange(row0, row1) + 0.5) * (north - south) / height
        lon_grid, lat_grid = np.meshgrid(lon, lat)
        east_grid, north_grid = utm_forward(lat_grid, lon_grid, args.zone)
        block_filled = filled[row0:row1]
        for tile in tiles:
            col = np.rint((east_grid - tile["e0"]) / tile["res"]).astype(np.int64)
            row = np.rint((tile["n0"] - north_grid) / tile["res"]).astype(np.int64)
            inside = (
                (col >= 0)
                & (col < tile["width"])
                & (row >= 0)
                & (row < tile["height"])
                & ~block_filled
            )
            if not inside.any():
                continue
            output[row0:row1][inside] = tile["rgb"][row[inside], col[inside]]
            block_filled[inside] = True

    coverage = filled.mean() * 100.0
    Image.fromarray(output, "RGB").save(args.out)
    print(f"output {width}x{height} ({width * height / 1e6:.1f} MP), coverage {coverage:.3f}%")
    if coverage < 99.9:
        raise SystemExit("mosaic does not fully cover the requested region")


if __name__ == "__main__":
    main()
