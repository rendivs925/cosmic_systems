#!/usr/bin/env python3
"""Prepare SoilGrids 2.0 texture for an offline `.cssoil` package.

The source is the ISRIC SoilGrids WCS at 250 m. For each reviewed region this
script requests clay, sand, and silt 0-5 cm mean over the geodetic envelope that
contains the region's planetocentric terrain bounds, then resamples the three
continuous fractions onto the terrain-radial grid used by the simulator.

A coarse global fallback is requested through the WCS scaling extension. The
WCS response is cached in the ignored raw-asset tree and pinned by SHA-256; no
download happens at runtime. The normalized RGB PNG (clay, sand, silt integer
percent) is the only input to `src/bin/soil_bake.rs`.
"""

from __future__ import annotations

import argparse
import hashlib
import shlex
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image

WGS84_E2 = 0.006_694_379_990_141_316_5
WCS_ENDPOINT = "https://maps.isric.org/mapserv"
EPSG_4326 = "http://www.opengis.net/def/crs/EPSG/0/4326"
VARIABLES = ("clay", "sand", "silt")
SOURCE_RESOLUTION_M = 250.0
NEUTRAL_LOAM = np.array([33.0, 33.0, 34.0])

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
GLOBAL_TERRAIN_BOUNDS = (-180.0, -60.0, 180.0, 84.0)
GLOBAL_RESOLUTION_DEG = 1.0 / 12.0
GLOBAL_SCALESIZE = (4320, 1728)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def terrain_to_geodetic(latitude_deg: np.ndarray | float):
    values = np.radians(latitude_deg)
    return np.degrees(np.arctan(np.tan(values) / (1.0 - WGS84_E2)))


def geodetic_to_terrain(latitude_deg: np.ndarray | float):
    values = np.radians(latitude_deg)
    return np.degrees(np.arctan(np.tan(values) * (1.0 - WGS84_E2)))


def geodetic_envelope(bounds: tuple[float, float, float, float]):
    west, south, east, north = bounds
    # Planetocentric terrain latitudes are slightly smaller in magnitude than
    # their WGS 84 geodetic counterparts, so expand the request envelope.
    return (
        west,
        float(np.floor(terrain_to_geodetic(south) * 1000.0) / 1000.0) - 0.01,
        east,
        float(np.ceil(terrain_to_geodetic(north) * 1000.0) / 1000.0) + 0.01,
    )


def source_url(
    variable: str,
    bounds: tuple[float, float, float, float],
    scale_size: tuple[int, int] | None = None,
) -> str:
    west, south, east, north = bounds
    parts = [
        f"{WCS_ENDPOINT}?map=/map/{variable}.map",
        "SERVICE=WCS",
        "VERSION=2.0.1",
        "REQUEST=GetCoverage",
        f"COVERAGEID={variable}_0-5cm_mean",
        "FORMAT=GEOTIFF_INT16",
        f"SUBSET=long({west},{east})",
        f"SUBSET=lat({south},{north})",
        f"SUBSETTINGCRS={EPSG_4326}",
        f"OUTPUTCRS={EPSG_4326}",
        "INTERPOLATION=AVERAGE",
    ]
    if scale_size is not None:
        parts.append(f"SCALESIZE=long({scale_size[0]}),lat({scale_size[1]})")
    return "&".join(parts)


def fetch(url: str, path: Path, force: bool) -> None:
    if path.exists() and not force:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".part")
    print(f"downloading {url}")
    request = urllib.request.Request(
        url, headers={"User-Agent": "cosmic-systems-soil-ingest/1.0"}
    )
    with urllib.request.urlopen(request, timeout=300) as response:
        temporary.write_bytes(response.read())
    try:
        Image.open(temporary).verify()
    except Exception as error:  # pragma: no cover - network failure path
        temporary.unlink(missing_ok=True)
        raise SystemExit(f"WCS response for {url} is not a TIFF: {error}")
    temporary.replace(path)


def read_grid(path: Path):
    """Return float samples plus the WGS 84 geographic grid georeferencing."""
    image = Image.open(path)
    samples = np.asarray(image).astype(np.float64)
    tags = image.tag_v2
    tiepoint = tags.get(33922)
    scale = tags.get(33550)
    if tiepoint is None or scale is None:
        raise SystemExit(f"{path} has no ModelTiepoint/ModelPixelScale georeferencing")
    west = float(tiepoint[3])
    north = float(tiepoint[4])
    xres = float(scale[0])
    yres = float(scale[1])
    height, width = samples.shape
    return samples, west, north, xres, yres, width, height


def common_extent(grids) -> tuple[float, float, float, float]:
    west = max(grid[1] for grid in grids)
    north = min(grid[2] for grid in grids)
    east = min(grid[1] + grid[5] * grid[3] for grid in grids)
    south = max(grid[2] - grid[6] * grid[4] for grid in grids)
    if not (west < east and south < north):
        raise SystemExit("WCS variables do not share a valid geographic overlap")
    return west, south, east, north


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
    # Pixel centres are half a pixel inside ModelTiepoint's outer corner.
    fx = (longitudes[None, :] - west) / xres - 0.5
    fy = (north - latitudes[:, None]) / yres - 0.5
    x0 = np.floor(fx).astype(np.int64)
    y0 = np.floor(fy).astype(np.int64)
    tx = fx - x0
    ty = fy - y0
    x0 = np.clip(x0, 0, width - 2)
    y0 = np.clip(y0, 0, height - 2)
    x1 = x0 + 1
    y1 = y0 + 1
    top = samples[y0, x0] * (1.0 - tx) + samples[y0, x1] * tx
    bottom = samples[y1, x0] * (1.0 - tx) + samples[y1, x1] * tx
    return top * (1.0 - ty) + bottom * ty


def normalize_texture(clay: np.ndarray, sand: np.ndarray, silt: np.ndarray) -> np.ndarray:
    # SoilGrids stores mass fractions in per-mille; convert to percent.
    clay = clay / 10.0
    sand = sand / 10.0
    silt = silt / 10.0
    valid = (
        np.isfinite(clay)
        & np.isfinite(sand)
        & np.isfinite(silt)
        & (clay >= 0.0)
        & (sand >= 0.0)
        & (silt >= 0.0)
        & (clay <= 100.0)
        & (sand <= 100.0)
        & (silt <= 100.0)
    )
    total = clay + sand + silt
    valid &= total > 1.0
    fallback = np.broadcast_to(NEUTRAL_LOAM[:, None, None], (3, *clay.shape))
    stack = np.stack(
        [
            np.where(valid, clay, fallback[0]),
            np.where(valid, sand, fallback[1]),
            np.where(valid, silt, fallback[2]),
        ]
    )
    total = stack.sum(axis=0)
    stack = stack / total
    clay_out = np.rint(stack[0] * 100.0).astype(np.int16)
    sand_out = np.rint(stack[1] * 100.0).astype(np.int16)
    sand_out = np.minimum(sand_out, 100 - clay_out)
    silt_out = 100 - clay_out - sand_out
    return np.stack([clay_out, sand_out, silt_out], axis=-1).astype(np.uint8)


def build(
    name: str,
    terrain_bounds: tuple[float, float, float, float],
    resolution_deg: float,
    source_dir: Path,
    output_png: Path,
    force: bool,
    global_fallback: bool,
) -> None:
    request_bounds = geodetic_envelope(terrain_bounds)
    scale_size = GLOBAL_SCALESIZE if global_fallback else None
    urls = {variable: source_url(variable, request_bounds, scale_size) for variable in VARIABLES}
    paths = {
        variable: source_dir / f"{name}_{variable}_wcs.tif" for variable in VARIABLES
    }
    for variable in VARIABLES:
        fetch(urls[variable], paths[variable], force)

    grids = {variable: read_grid(paths[variable]) for variable in VARIABLES}
    resolutions = {(grid[3], grid[4], grid[5], grid[6]) for grid in grids.values()}
    if len(resolutions) != 1:
        raise SystemExit(f"{name}: WCS variables have different grids: {resolutions}")
    xres, yres, source_width, source_height = resolutions.pop()
    west, south, east, north = common_extent(list(grids.values()))

    if global_fallback:
        output_bounds = (
            west,
            float(geodetic_to_terrain(south)),
            east,
            float(geodetic_to_terrain(north)),
        )
    else:
        output_bounds = terrain_bounds
        output_west, output_south, output_east, output_north = output_bounds
        if not (
            west <= output_west + 1e-6
            and east + 1e-6 >= output_east
            and south <= float(terrain_to_geodetic(output_south)) + 1e-6
            and north + 1e-6 >= float(terrain_to_geodetic(output_north))
        ):
            raise SystemExit(
                f"{name}: requested bounds are not fully covered by {west},{south},{east},{north}"
            )

    output_west, output_south, output_east, output_north = output_bounds
    width = round((output_east - output_west) / resolution_deg) + 1
    height = round((output_north - output_south) / resolution_deg) + 1
    terrain_latitudes = output_north - np.arange(height) * resolution_deg
    geodetic_latitudes = terrain_to_geodetic(terrain_latitudes)
    longitudes = output_west + np.arange(width) * resolution_deg
    sampled = {
        variable: bilinear(
            grids[variable][0],
            grids[variable][1],
            grids[variable][2],
            xres,
            yres,
            geodetic_latitudes,
            longitudes,
        )
        for variable in VARIABLES
    }
    texture = normalize_texture(sampled["clay"], sampled["sand"], sampled["silt"])
    output_png.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(texture, mode="RGB").save(output_png)

    digest = hashlib.sha256()
    for variable in VARIABLES:
        digest.update(
            f"{sha256(paths[variable])}  {variable}  {urls[variable]}\n".encode()
        )
    aggregate = digest.hexdigest()
    print(f"wrote {output_png}: {width}x{height}")
    print(
        "texture per-mille mean:",
        {
            variable: round(float(sampled[variable].mean()), 2)
            for variable in VARIABLES
        },
    )
    print("source_grid:", source_width, "x", source_height, f"@{xres:.9f},{yres:.9f}")
    print("output_bounds:", " ".join(f"{value:.9f}" for value in output_bounds))
    print("source_sha256:", aggregate)
    print("normalized_png_sha256:", sha256(output_png))
    license_text = "CC BY 4.0"
    attribution = "ISRIC - World Soil Information (SoilGrids 2.0)"
    source_label = "SoilGrids 2.0 clay/sand/silt 0-5 cm mean, 250 m"
    output_package = f"assets/large_files/terrain/soil_v1/{name}.cssoil"
    command = [
        "cargo",
        "run",
        "--release",
        "--features",
        "native",
        "--bin",
        "soil_bake",
        "--",
        str(output_png),
        output_package,
        f"{output_west:.9f}",
        f"{output_south:.9f}",
        f"{output_east:.9f}",
        f"{output_north:.9f}",
        aggregate,
        ";".join(urls[variable] for variable in VARIABLES),
        source_label,
        license_text,
        attribution,
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
        "--global",
        action="store_true",
        dest="global_fallback",
        help="also build the coarse global fallback",
    )
    parser.add_argument("--force", action="store_true", help="refetch cached WCS tiles")
    parser.add_argument(
        "--resolution-deg",
        type=float,
        default=0.0025,
        help="terrain-radial regional output spacing in degrees (default 0.0025)",
    )
    parser.add_argument(
        "--source-dir",
        type=Path,
        default=Path("assets/large_files/terrain/raw/soil"),
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("assets/large_files/terrain/raw/soil"),
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
            arguments.output_dir / f"{name}_soil.png",
            arguments.force,
            global_fallback=False,
        )
    if arguments.global_fallback:
        build(
            "00_global_soil_5arcmin",
            GLOBAL_TERRAIN_BOUNDS,
            GLOBAL_RESOLUTION_DEG,
            arguments.source_dir,
            arguments.output_dir / "global_soil_5arcmin.png",
            arguments.force,
            global_fallback=True,
        )


if __name__ == "__main__":
    main()
