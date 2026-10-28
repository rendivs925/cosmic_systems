#!/usr/bin/env python3
"""Prepare JRC water, Hansen tree cover, and Meta/WRI canopy height.

The JRC and Hansen sources are 30 m 10-degree GeoTIFF tiles with 1.6 billion
pixels. PIL reads only the tiles covering a region's geographic window, so the
script never decodes the whole source. It then resamples that window onto the
planetocentric terrain-radial grid used by the simulator. The Meta/WRI canopy
product is a 1/4000-degree (about 28 m) 10-degree GeoTIFF; its average-height
band is resampled the same way.

The normalized water, tree-cover, and canopy-height PNGs plus their provenance
feed `src/bin/surface_fields_bake.rs`. Inputs are cached only in the ignored raw
tree; no download happens at runtime.
"""

from __future__ import annotations

import argparse
import hashlib
import math
import shlex
import urllib.error
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image

Image.MAX_IMAGE_PIXELS = None

WGS84_E2 = 0.006_694_379_990_141_316_5
JRC_BASE = "https://storage.googleapis.com/global-surface-water/downloads2021/occurrence"
JRC_VERSION = "v1_4_2021"
HANSEN_BASE = (
    "https://storage.googleapis.com/earthenginepartners-hansen/GFC-2023-v1.11"
)
HANSEN_VERSION = "GFC-2023-v1.11"
META_BASE = (
    "https://dataforgood-fb-data.s3.amazonaws.com/forests/v1/"
    "alsgedi_global_v6_float_epsg4326_v3_10deg"
)
META_README = "https://dataforgood-fb-data.s3.amazonaws.com/forests/v1/readme.txt"
SOURCE_RESOLUTION_M = 30.0
JRC_LICENSE = "CC BY 4.0"
JRC_ATTRIBUTION = "European Commission, Joint Research Centre (JRC) Global Surface Water"
HANSEN_LICENSE = "CC BY 4.0"
HANSEN_ATTRIBUTION = "Hansen/UMD/Google/USGS/NASA Global Forest Change"
META_LICENSE = "CC BY 4.0"
META_ATTRIBUTION = "Meta/WRI Canopy Height Maps (Tolan et al., 2024)"

REGIONS = {
    "papua_coastal_lowland": (139.0, -8.543280680, 140.0, -7.351005219),
    "ksc_florida": (-80.85, 28.239282489, -80.45, 28.588004047),
    "starbase_boca_chica": (-97.30, 25.699304258, -97.05, 25.948267941),
    "vandenberg_california": (-120.75, 34.420332023, -120.45, 34.669738345),
    "wallops_virginia": (-75.70, 37.613777603, -75.35, 37.863362149),
    "kourou_french_guiana": (-52.95, 4.966696087, -52.45, 5.314390815),
    "baikonur_kazakhstan": (63.05, 45.557625810, 63.55, 45.907693822),
    "mahia_new_zealand": (177.70, -39.211364478, 178.00, -38.961703007),
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def terrain_to_geodetic(latitude_deg: np.ndarray | float):
    values = np.radians(latitude_deg)
    return np.degrees(np.arctan(np.tan(values) / (1.0 - WGS84_E2)))


def tile_west(value: float) -> int:
    return math.floor(value / 10.0) * 10


def tile_north(value: float) -> int:
    # These products name a 10-degree tile by its north/upper edge.
    return math.ceil(value / 10.0) * 10


def corner_label(value: int, width: int, positive: str, negative: str) -> str:
    return f"{abs(value):0{width}d}{positive if value >= 0 else negative}"


def jrc_tile_name(west: float, south: float) -> str:
    lon = corner_label(tile_west(west), 0, "E", "W")
    lat = corner_label(tile_north(south), 0, "N", "S")
    return f"occurrence_{lon}_{lat}{JRC_VERSION}.tif"


def hansen_tile_name(west: float, south: float) -> str:
    lon = corner_label(tile_west(west), 3, "E", "W")
    lat = corner_label(tile_north(south), 2, "N", "S")
    return (
        f"Hansen_{HANSEN_VERSION}_treecover2000_{lat}_{lon}.tif"
    )


def meta_tile_name(west: float, south: float) -> str:
    # Meta names each 10-degree average-height tile by its north-west corner.
    lon = math.floor(west / 10.0) * 10
    lat = math.ceil(south / 10.0) * 10
    return f"meta_chm_lat={lat:.1f}_lon={lon:.1f}_avg.tif"


def download(url: str, path: Path) -> None:
    if path.exists():
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    print(f"downloading {url}", flush=True)
    temporary = path.with_suffix(path.suffix + ".part")
    request = urllib.request.Request(
        url, headers={"User-Agent": "cosmic-systems-water-canopy-ingest/1.0"}
    )
    with urllib.request.urlopen(request, timeout=600) as response:
        with temporary.open("wb") as handle:
            while chunk := response.read(8 * 1024 * 1024):
                handle.write(chunk)
    temporary.replace(path)


def download_optional(url: str, path: Path) -> bool:
    """Download a tile when published; return False for a documented gap."""
    if path.exists():
        return True
    try:
        download(url, path)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            print(f"no published tile: {url}", flush=True)
            return False
        raise
    return True


def geodetic_envelope(bounds: tuple[float, float, float, float]):
    west, south, east, north = bounds
    return (
        west,
        float(np.floor(terrain_to_geodetic(south) * 1000.0) / 1000.0) - 0.01,
        east,
        float(np.ceil(terrain_to_geodetic(north) * 1000.0) / 1000.0) + 0.01,
    )


def bilinear(
    samples: np.ndarray,
    west: float,
    north: float,
    xres: float,
    yres: float,
    latitudes: np.ndarray,
    longitudes: np.ndarray,
) -> np.ndarray:
    height, width = samples.shape
    fx = (longitudes[None, :] - west) / xres - 0.5
    fy = (north - latitudes[:, None]) / yres - 0.5
    x0 = np.floor(fx).astype(np.int64)
    y0 = np.floor(fy).astype(np.int64)
    tx = fx - x0
    ty = fy - y0
    x0 = np.clip(x0, 0, width - 2)
    y0 = np.clip(y0, 0, height - 2)
    top = samples[y0, x0] * (1.0 - tx) + samples[y0, x0 + 1] * tx
    bottom = samples[y0 + 1, x0] * (1.0 - tx) + samples[y0 + 1, x0 + 1] * tx
    return top * (1.0 - ty) + bottom * ty


def source_window(path: Path, request_bounds: tuple[float, float, float, float]):
    """Crop the requested geographic window without decoding the whole tile."""
    image = Image.open(path)
    tags = image.tag_v2
    tiepoint = tags.get(33922)
    scale = tags.get(33550)
    if tiepoint is None or scale is None:
        raise SystemExit(f"{path} has no ModelTiepoint/ModelPixelScale georeferencing")
    source_west = float(tiepoint[3])
    source_north = float(tiepoint[4])
    xres = float(scale[0])
    yres = float(scale[1])
    west, south, east, north = request_bounds
    left = math.floor((west - source_west) / xres) - 1
    right = math.ceil((east - source_west) / xres) + 1
    top = math.floor((source_north - north) / yres) - 1
    bottom = math.ceil((source_north - south) / yres) + 1
    left = max(0, left)
    top = max(0, top)
    right = min(image.width, right)
    bottom = min(image.height, bottom)
    if left >= right or top >= bottom:
        raise SystemExit(f"requested window is outside {path}")
    samples = np.asarray(image.crop((left, top, right, bottom))).astype(np.float64)
    crop_bounds = (
        source_west + left * xres,
        source_north - bottom * yres,
        source_west + right * xres,
        source_north - top * yres,
    )
    return samples, crop_bounds, xres, yres


def resample_to_terrain(
    path: Path,
    request_bounds: tuple[float, float, float, float],
    terrain_bounds: tuple[float, float, float, float],
    resolution_deg: float,
    output_png: Path,
) -> None:
    samples, (west_src, south_src, east_src, north_src), xres, yres = source_window(
        path, request_bounds
    )
    west, south, east, north = terrain_bounds
    output_width = round((east - west) / resolution_deg) + 1
    output_height = round((north - south) / resolution_deg) + 1
    terrain_latitudes = north - np.arange(output_height) * resolution_deg
    geodetic_latitudes = terrain_to_geodetic(terrain_latitudes)
    longitudes = west + np.arange(output_width) * resolution_deg
    values = bilinear(
        samples,
        west_src,
        north_src,
        xres,
        yres,
        geodetic_latitudes,
        longitudes,
    )
    # Source nodata is not a valid percentage. Water nodata becomes no water;
    # tree-cover nodata becomes no measured cover.
    values = np.where(np.isfinite(values) & (values >= 0.0) & (values <= 100.0), values, 0.0)
    output_png.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(np.rint(values).astype(np.uint8), mode="L").save(output_png)
    print(
        f"wrote {output_png}: {output_width}x{output_height} "
        f"({np.count_nonzero(values) / values.size:.1%} nonzero)"
    )


def resample_canopy_to_terrain(
    path: Path,
    request_bounds: tuple[float, float, float, float],
    terrain_bounds: tuple[float, float, float, float],
    resolution_deg: float,
    output_png: Path,
) -> None:
    samples, (west_src, south_src, east_src, north_src), xres, yres = source_window(
        path, request_bounds
    )
    # The average-height band stores centimetres and 65535 as nodata. Zero is a
    # valid no-canopy measurement, so only the explicit nodata sentinel becomes
    # "unmeasured" and falls back to zero metres.
    samples = np.where(samples == 65535.0, np.nan, samples)
    west, south, east, north = terrain_bounds
    output_width = round((east - west) / resolution_deg) + 1
    output_height = round((north - south) / resolution_deg) + 1
    terrain_latitudes = north - np.arange(output_height) * resolution_deg
    geodetic_latitudes = terrain_to_geodetic(terrain_latitudes)
    longitudes = west + np.arange(output_width) * resolution_deg
    values = bilinear(
        samples,
        west_src,
        north_src,
        xres,
        yres,
        geodetic_latitudes,
        longitudes,
    )
    meters = np.where(
        np.isfinite(values) & (values >= 0.0) & (values < 65535.0),
        values / 100.0,
        0.0,
    )
    meters = np.clip(meters, 0.0, 255.0)
    output_png.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(np.rint(meters).astype(np.uint8), mode="L").save(output_png)
    print(
        f"wrote {output_png}: {output_width}x{output_height} "
        f"(mean {meters.mean():.1f} m, max {meters.max():.0f} m)"
    )


def zero_canopy_png(water_png: Path, output_png: Path) -> None:
    with Image.open(water_png) as image:
        size = image.size
    Image.new("L", size, 0).save(output_png)
    print(f"wrote {output_png}: {size[0]}x{size[1]} (no measured canopy tile)")


def build(
    name: str,
    terrain_bounds: tuple[float, float, float, float],
    resolution_deg: float,
    source_dir: Path,
    output_dir: Path,
) -> None:
    west, south, east, north = terrain_bounds
    request_bounds = geodetic_envelope(terrain_bounds)
    jrc_path = source_dir / jrc_tile_name(west, south)
    hansen_path = source_dir / hansen_tile_name(west, south)
    meta_path = source_dir / meta_tile_name(west, south)
    jrc_url = f"{JRC_BASE}/{jrc_path.name}"
    hansen_url = f"{HANSEN_BASE}/{hansen_path.name}"
    meta_url = f"{META_BASE}/{meta_path.name}"
    download(jrc_url, jrc_path)
    download(hansen_url, hansen_path)
    has_canopy = download_optional(meta_url, meta_path)

    water_png = output_dir / f"{name}_water.png"
    tree_png = output_dir / f"{name}_tree_cover.png"
    canopy_png = output_dir / f"{name}_canopy_height.png"
    resample_to_terrain(
        jrc_path, request_bounds, terrain_bounds, resolution_deg, water_png
    )
    resample_to_terrain(
        hansen_path, request_bounds, terrain_bounds, resolution_deg, tree_png
    )
    if has_canopy:
        resample_canopy_to_terrain(
            meta_path, request_bounds, terrain_bounds, resolution_deg, canopy_png
        )
    else:
        zero_canopy_png(water_png, canopy_png)

    digest = hashlib.sha256()
    entries = [(jrc_path, jrc_url), (hansen_path, hansen_url)]
    if has_canopy:
        entries.append((meta_path, meta_url))
    for path, url in entries:
        digest.update(f"{sha256(path)}  {url}\n".encode())
    aggregate = digest.hexdigest()
    print("source_sha256:", aggregate)
    source_label = "JRC Global Surface Water occurrence + Hansen tree cover"
    if has_canopy:
        source_label += " + Meta/WRI mean canopy height"
    else:
        source_label += "; no Meta/WRI downsampled canopy tile over this region"
    output_package = f"assets/large_files/terrain/surface_fields_v1/{name}.cssfld"
    command = [
        "cargo",
        "run",
        "--release",
        "--features",
        "native",
        "--bin",
        "surface_fields_bake",
        "--",
        str(water_png),
        str(tree_png),
        str(canopy_png),
        output_package,
        f"{west:.9f}",
        f"{south:.9f}",
        f"{east:.9f}",
        f"{north:.9f}",
        aggregate,
        jrc_url,
        hansen_url,
        meta_url if has_canopy else META_README,
        source_label,
        f"{JRC_LICENSE}; {HANSEN_LICENSE}; {META_LICENSE}",
        f"{JRC_ATTRIBUTION}; {HANSEN_ATTRIBUTION}; {META_ATTRIBUTION}",
        f"{SOURCE_RESOLUTION_M}",
    ]
    print("bake:", " ".join(shlex.quote(part) for part in command))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--region",
        default="all",
        choices=["all", *sorted(REGIONS)],
        help="reviewed region to prepare, or all",
    )
    parser.add_argument(
        "--resolution-deg",
        type=float,
        default=1.0 / 3600.0,
        help="terrain-radial output spacing in degrees (default 1 arc-second)",
    )
    parser.add_argument(
        "--source-dir",
        type=Path,
        default=Path("assets/large_files/terrain/raw/water_canopy/source"),
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("assets/large_files/terrain/raw/water_canopy"),
    )
    arguments = parser.parse_args()
    if arguments.resolution_deg <= 0.0:
        raise SystemExit("--resolution-deg must be positive")
    names = sorted(REGIONS) if arguments.region == "all" else [arguments.region]
    for name in names:
        build(
            name,
            REGIONS[name],
            arguments.resolution_deg,
            arguments.source_dir,
            arguments.output_dir,
        )


if __name__ == "__main__":
    main()
