#!/usr/bin/env python3
"""Normalize NOAA ETOPO 2022 30 arc-second relief to WGS 84 ellipsoid heights.

ETOPO 2022 is distributed in WGS 84 geographic coordinates (EPSG:4326) with
heights relative to the EGM2008 geoid (EPSG:3855). The simulator's terrain
authority stores WGS 84 ellipsoid metres, so this offline step adds the
published geoid-height tile to the surface tile:

    h_ellipsoid = H_EGM2008 + N_EGM2008

The output is a north-to-south, little-endian signed 16-bit metre raster with
43200 x 21600 samples and a periodic longitude span from -180 to 180 degrees.
`etopo2022_convert` resamples this raster onto the cube sphere; the simulator
never reads NetCDF or downloads data at runtime.

Source files (NOAA/NCEI, public domain):

    ETOPO_2022_v1_30s_N90W180_surface.nc
    ETOPO_2022_v1_30s_N90W180_geoid.nc

Dependencies: numpy and netCDF4. Example:

    python3 scripts/ingest_etopo2022.py \
        --cache-dir assets/large_files/terrain/raw/etopo2022 \
        --output assets/large_files/terrain/earth_etopo2022_30s_wgs84.raw
"""

import argparse
import hashlib
import os
import sys
import threading
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed

import numpy as np
from netCDF4 import Dataset

COLUMNS = 43_200
ROWS = 21_600
ARC_SECONDS = 30
STEP_DEG = 1.0 / 120.0
CHUNK_ROWS = 256

SURFACE = {
    "role": "surface",
    "name": "ETOPO_2022_v1_30s_N90W180_surface.nc",
    "url": "https://www.ngdc.noaa.gov/thredds/fileServer/global/ETOPO2022/30s/30s_surface_elev_netcdf/ETOPO_2022_v1_30s_N90W180_surface.nc",
    "bytes": 1_642_335_281,
    "sha256": "3d76cea4fca0a056b6d8c4bcf7f4073465b975489849235309e65d44ed4ed404",
}
GEOID = {
    "role": "geoid",
    "name": "ETOPO_2022_v1_30s_N90W180_geoid.nc",
    "url": "https://www.ngdc.noaa.gov/thredds/fileServer/global/ETOPO2022/30s/30s_geoid_netcdf/ETOPO_2022_v1_30s_N90W180_geoid.nc",
    "bytes": 1_753_951_916,
    "sha256": "610a576d1d0f17d5ab967b1b13aa7ea5ad23fa2aeaa2ae9da690734d7fd3e829",
}

USER_AGENT = "cosmic-systems-etopo2022-ingest/1.0"
DOWNLOAD_CHUNK = 8 << 20
DOWNLOAD_THREADS = 12


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as source:
        for block in iter(lambda: source.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def remote_size(url):
    request = urllib.request.Request(
        url, headers={"User-Agent": USER_AGENT, "Range": "bytes=0-0"}
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        return int(response.headers["Content-Range"].split("/")[1])


def download_part(url, target, total, start):
    end = min(start + DOWNLOAD_CHUNK, total) - 1
    wanted = end - start + 1
    descriptor = os.open(target, os.O_RDWR)
    try:
        for attempt in range(6):
            written = 0
            try:
                request = urllib.request.Request(
                    url,
                    headers={
                        "User-Agent": USER_AGENT,
                        "Range": f"bytes={start}-{end}",
                    },
                )
                with urllib.request.urlopen(request, timeout=120) as response:
                    while True:
                        block = response.read(1 << 20)
                        if not block:
                            break
                        os.pwrite(descriptor, block, start + written)
                        written += len(block)
                if written == wanted:
                    return wanted
            except Exception as error:  # network errors are retried
                print(f"  retry {start}: {error}", file=sys.stderr)
                time.sleep(2 * (attempt + 1))
        raise RuntimeError(f"failed to download bytes {start}-{end}")
    finally:
        os.close(descriptor)


def download(source, cache_dir):
    path = os.path.join(cache_dir, source["name"])
    if os.path.exists(path) and os.path.getsize(path) == source["bytes"]:
        return path
    os.makedirs(cache_dir, exist_ok=True)
    total = remote_size(source["url"])
    if total != source["bytes"]:
        raise RuntimeError(
            f"{source['name']} is {total} bytes; expected {source['bytes']}"
        )
    partial = path + ".part"
    descriptor = os.open(partial, os.O_CREAT | os.O_RDWR, 0o644)
    os.ftruncate(descriptor, total)
    os.close(descriptor)
    print(f"downloading {source['name']} ({total >> 20} MiB)", flush=True)
    completed = [0]
    lock = threading.Lock()

    def report():
        with lock:
            completed[0] += DOWNLOAD_CHUNK
            if completed[0] % (128 << 20) < DOWNLOAD_CHUNK:
                print(f"  {completed[0] >> 20}/{total >> 20} MiB", flush=True)

    def work(start):
        download_part(source["url"], partial, total, start)
        report()

    starts = range(0, total, DOWNLOAD_CHUNK)
    with ThreadPoolExecutor(max_workers=DOWNLOAD_THREADS) as pool:
        futures = [pool.submit(work, start) for start in starts]
        for future in as_completed(futures):
            future.result()
    os.replace(partial, path)
    return path


def verify(source, path):
    size = os.path.getsize(path)
    if size != source["bytes"]:
        raise RuntimeError(f"{path} is {size} bytes; expected {source['bytes']}")
    digest = sha256_file(path)
    if source["sha256"] == "PENDING":
        print(f"WARNING: unpinned SHA-256 for {source['name']}: {digest}")
    elif digest != source["sha256"]:
        raise RuntimeError(
            f"{source['name']} SHA-256 mismatch: expected {source['sha256']}, found {digest}"
        )
    return digest


def fill_value(variable):
    if "_FillValue" in variable.ncattrs():
        return float(variable.getncattr("_FillValue"))
    return None


def open_grid(path, role):
    dataset = Dataset(path)
    variable = dataset.variables["z"]
    if variable.shape != (ROWS, COLUMNS):
        raise RuntimeError(f"{role} grid is {variable.shape}; expected {(ROWS, COLUMNS)}")
    return dataset, variable


def coordinate_columns(longitude):
    """Map output column centers (-180..180) to source column indices."""
    centers = -180.0 + (np.arange(COLUMNS) + 0.5) * STEP_DEG
    start = float(longitude[0])
    step = float(longitude[1] - longitude[0])
    if not np.isclose(abs(step), STEP_DEG, rtol=0.0, atol=1e-9):
        raise RuntimeError(f"unexpected longitude step {step}")
    indices = np.rint((centers - start) / step).astype(np.int64)
    if np.any(np.abs((centers - start) / step - indices) > 1e-6):
        raise RuntimeError("longitude centers are not on a regular grid")
    return np.mod(indices, COLUMNS)


def check_latitude(latitude):
    step = float(latitude[1] - latitude[0])
    if not np.isclose(step, STEP_DEG, rtol=0.0, atol=1e-9):
        raise RuntimeError(f"unexpected latitude step {step}")
    expected_north = 90.0 - 0.5 * STEP_DEG
    if not np.isclose(float(latitude[-1]), expected_north, atol=1e-6):
        raise RuntimeError("latitude grid does not start at the north pixel center")


def normalize(surface_path, geoid_path, output_path):
    surface, surface_z = open_grid(surface_path, "surface")
    geoid, geoid_z = open_grid(geoid_path, "geoid")
    try:
        columns = coordinate_columns(surface.variables["lon"][:])
        check_latitude(surface.variables["lat"][:])
        surface_fill = fill_value(surface_z)
        geoid_fill = fill_value(geoid_z)

        minimum = np.inf
        maximum = -np.inf
        total = 0
        land = 0
        ocean = 0
        weight_total = 0.0
        land_weight = 0.0
        mean_weight = 0.0
        land_mean_weight = 0.0
        digest = hashlib.sha256()
        written = 0
        with open(output_path, "wb") as output:
            for first in range(0, ROWS, CHUNK_ROWS):
                last = min(first + CHUNK_ROWS, ROWS)
                source_first = ROWS - last
                source_last = ROWS - first
                heights = np.asarray(
                    surface_z[source_first:source_last, :], dtype=np.float64
                )
                undulation = np.asarray(
                    geoid_z[source_first:source_last, :], dtype=np.float64
                )
                if surface_fill is not None and np.any(
                    np.abs(heights - surface_fill) < 1.0
                ):
                    raise RuntimeError("surface raster contains fill values")
                if geoid_fill is not None and np.any(
                    np.abs(undulation - geoid_fill) < 1.0
                ):
                    raise RuntimeError("geoid raster contains fill values")
                values = (heights + undulation)[::-1, columns]
                rounded = np.where(
                    values >= 0.0, np.floor(values + 0.5), np.ceil(values - 0.5)
                )
                if np.any(rounded < -32768.0) or np.any(rounded > 32767.0):
                    raise RuntimeError("normalized height exceeds signed 16-bit metres")
                block = rounded.astype("<i2").tobytes()
                output.write(block)
                digest.update(block)
                written += len(block)
                minimum = min(minimum, float(rounded.min()))
                maximum = max(maximum, float(rounded.max()))
                total += rounded.size
                land += int(np.count_nonzero(rounded > 0.0))
                ocean += int(np.count_nonzero(rounded <= 0.0))
                # Exact spherical band area per output row, repeated per column.
                row_index = first + np.arange(rounded.shape[0])
                latitude = 90.0 - (row_index + 0.5) * (180.0 / ROWS)
                north = np.radians(latitude + 0.5 * (180.0 / ROWS))
                south = np.radians(latitude - 0.5 * (180.0 / ROWS))
                row_weight = np.repeat(np.sin(north) - np.sin(south), COLUMNS)
                values_flat = rounded.ravel()
                is_land = values_flat > 0.0
                weight_total += float(row_weight.sum())
                land_weight += float(row_weight[is_land].sum())
                mean_weight += float((values_flat * row_weight).sum())
                land_mean_weight += float((values_flat[is_land] * row_weight[is_land]).sum())
        expected = COLUMNS * ROWS * 2
        if written != expected:
            raise RuntimeError(f"wrote {written} bytes; expected {expected}")
    finally:
        surface.close()
        geoid.close()
    statistics = {
        "min_m": minimum,
        "max_m": maximum,
        "land_fraction": land / total,
        "ocean_fraction": ocean / total,
        "land_fraction_area": land_weight / weight_total,
        "mean_m_area": mean_weight / weight_total,
        "land_mean_m_area": land_mean_weight / land_weight,
    }
    return digest.hexdigest(), written, statistics


def write_provenance(path, output_path, output_sha, output_bytes, statistics, sources):
    lines = [
        "(",
        '    dataset: "ETOPO 2022 v1 30 arc-second ice surface",',
        "    source_files: [",
    ]
    for source, digest in sources:
        lines += [
            "        (",
            f'            role: "{source["role"]}",',
            f'            name: "{source["name"]}",',
            f'            url: "{source["url"]}",',
            f'            bytes: {source["bytes"]},',
            f'            sha256: "{digest}",',
            "        ),",
        ]
    lines += [
        "    ],",
        f'    normalized_raster: "{output_path}",',
        f'    normalized_sha256: "{output_sha}",',
        f"    normalized_bytes: {output_bytes},",
        f"    columns: {COLUMNS},",
        f"    rows: {ROWS},",
        f"    source_resolution_arcsec: {ARC_SECONDS},",
        '    horizontal_datum: "WGS 84 / EPSG:4326",',
        '    source_vertical_datum: "EGM2008 / EPSG:3855",',
        '    target_vertical_datum: "WGS 84 ellipsoid / EPSG:4979",',
        '    sample_encoding: "signed i16 little-endian, north-to-south rows, periodic longitude",',
        '    license: "Public domain (NOAA/NCEI)",',
        '    attribution: "NOAA National Centers for Environmental Information (NCEI)",',
        "    statistics: (",
        f"        min_m: {statistics['min_m']:.6},",
        f"        max_m: {statistics['max_m']:.6},",
        f"        land_fraction: {statistics['land_fraction']:.9},",
        f"        ocean_fraction: {statistics['ocean_fraction']:.9},",
        f"        land_fraction_area: {statistics['land_fraction_area']:.9},",
        f"        mean_m_area: {statistics['mean_m_area']:.6},",
        f"        land_mean_m_area: {statistics['land_mean_m_area']:.6},",
        "    ),",
        ")",
    ]
    with open(path, "w") as provenance:
        provenance.write("\n".join(lines) + "\n")


def main():
    parser = argparse.ArgumentParser(
        description="Download and normalize ETOPO 2022 30s to WGS 84 metres."
    )
    parser.add_argument(
        "--cache-dir",
        default="assets/large_files/terrain/raw/etopo2022",
        help="directory holding the verified NetCDF sources",
    )
    parser.add_argument(
        "--output",
        default="assets/large_files/terrain/earth_etopo2022_30s_wgs84.raw",
        help="normalized signed i16 raster to write",
    )
    parser.add_argument("--surface", help="existing surface NetCDF (skips download)")
    parser.add_argument("--geoid", help="existing geoid NetCDF (skips download)")
    args = parser.parse_args()

    sources = []
    digests = []
    for source, override in ((SURFACE, args.surface), (GEOID, args.geoid)):
        path = override or download(source, args.cache_dir)
        digests.append(verify(source, path))
        sources.append((source, digests[-1]))

    output_sha, output_bytes, statistics = normalize(
        args.surface or os.path.join(args.cache_dir, SURFACE["name"]),
        args.geoid or os.path.join(args.cache_dir, GEOID["name"]),
        args.output,
    )
    provenance = os.path.splitext(args.output)[0] + "_provenance.ron"
    write_provenance(
        provenance, args.output, output_sha, output_bytes, statistics, sources
    )
    print(f"wrote {args.output} ({output_bytes} bytes) sha256={output_sha}")
    print(
        "  min={min_m:.1f} max={max_m:.1f} land={land_fraction_area:.4f} "
        "land_mean={land_mean_m_area:.1f} mean={mean_m_area:.1f}".format(**statistics)
    )
    print(f"  provenance {provenance}")


if __name__ == "__main__":
    main()
