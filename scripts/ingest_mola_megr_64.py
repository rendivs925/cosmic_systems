#!/usr/bin/env python3
"""Provision and stitch the PDS MOLA MEGR 64 pixels-per-degree global map.

The 64 ppd product is distributed as four 180 x 90 degree tiles. This script
downloads them with parallel range requests, verifies each tile's exact size,
stitches them into one north-to-south, east-positive global signed-i16
big-endian raster, and records a deterministic aggregate source SHA-256.

No simulator path downloads data. `mola_megr_convert` consumes the stitched
raster offline; runtime only reads the generated CSDEM.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import urllib.request
from pathlib import Path

import numpy as np

BASE = (
    "https://pds-geosciences.wustl.edu/mgs/"
    "urn-nasa-pds-mgs_mola_topography_derived/meg064"
)
RAW_ROOT = Path("assets/large_files/terrain/raw/mars/meg064")
DEFAULT_OUTPUT = Path("assets/large_files/terrain/mars_mola_megr_64_v2.img")
TILE_BYTES = 132_710_400
TILE_COLUMNS = 11_520
TILE_ROWS = 5_760
COLUMNS = 23_040
ROWS = 11_520
WORKERS = 8
CHUNK = 8 * 1024 * 1024

# Tile name, north-to-south row offset, east-positive column offset.
TILES = (
    ("megr90n000gb", 0, 0),
    ("megr90n180gb", 0, TILE_COLUMNS),
    ("megr00n000gb", TILE_ROWS, 0),
    ("megr00n180gb", TILE_ROWS, TILE_COLUMNS),
)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def download_range(url: str, start: int, end: int, path: Path) -> None:
    expected = end - start + 1
    if path.exists() and path.stat().st_size == expected:
        return
    delay = 2.0
    for attempt in range(1, 7):
        try:
            request = urllib.request.Request(
                url, headers={"Range": f"bytes={start}-{end}"}
            )
            with urllib.request.urlopen(request, timeout=180) as response:
                data = response.read()
            if len(data) != expected:
                raise RuntimeError(
                    f"short range {start}-{end}: {len(data)} != {expected}"
                )
            temporary = path.with_suffix(".tmp")
            temporary.write_bytes(data)
            temporary.replace(path)
            return
        except Exception as error:  # noqa: BLE001 - retry every transport error
            if attempt == 6:
                raise
            print(f"retry {path.name} {start}: {error}", flush=True)
            delay *= 2


def download_tile(name: str, force: bool) -> Path:
    url = f"{BASE}/{name}.img"
    path = RAW_ROOT / f"{name}.img"
    path.parent.mkdir(parents=True, exist_ok=True)
    if force and path.exists():
        path.unlink()
    parts = RAW_ROOT / f"{name}.parts"
    parts.mkdir(exist_ok=True)
    starts = list(range(0, TILE_BYTES, CHUNK))
    with concurrent.futures.ThreadPoolExecutor(max_workers=WORKERS) as pool:
        futures = [
            pool.submit(
                download_range,
                url,
                start,
                min(start + CHUNK, TILE_BYTES) - 1,
                parts / f"{start:012d}.part",
            )
            for start in starts
        ]
        for future in concurrent.futures.as_completed(futures):
            future.result()
    with path.open("wb") as output:
        for start in starts:
            output.write((parts / f"{start:012d}.part").read_bytes())
            (parts / f"{start:012d}.part").unlink()
    parts.rmdir()
    if path.stat().st_size != TILE_BYTES:
        raise SystemExit(f"{path} has {path.stat().st_size} bytes, expected {TILE_BYTES}")
    print(f"tile {name}: {sha256_bytes(path.read_bytes())}  {url}", flush=True)
    return path


def stitch(tile_paths: dict[str, Path], output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    raster = np.memmap(
        output, dtype=">i2", mode="w+", shape=(ROWS, COLUMNS)
    )
    for name, row, column in TILES:
        tile = np.memmap(
            tile_paths[name], dtype=">i2", mode="r", shape=(TILE_ROWS, TILE_COLUMNS)
        )
        raster[row : row + TILE_ROWS, column : column + TILE_COLUMNS] = tile
        tile._mmap.close()
    raster.flush()
    raster._mmap.close()
    if output.stat().st_size != COLUMNS * ROWS * 2:
        raise SystemExit(f"{output} has an unexpected size")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help=f"stitched global raster (default {DEFAULT_OUTPUT})",
    )
    parser.add_argument("--force", action="store_true", help="redownload tiles")
    args = parser.parse_args()

    tile_paths = {name: download_tile(name, args.force) for name, _, _ in TILES}
    stitch(tile_paths, args.output)
    aggregate = hashlib.sha256()
    for name, _, _ in TILES:
        url = f"{BASE}/{name}.img"
        digest = sha256_bytes(tile_paths[name].read_bytes())
        aggregate.update(f"{digest}  {url}\n".encode())
    source_sha = aggregate.hexdigest()
    sidecar = args.output.with_suffix(".source.sha256")
    sidecar.write_text(f"{source_sha}  {args.output.name}\n")
    print(f"wrote {args.output} ({args.output.stat().st_size} bytes)")
    print(f"source aggregate sha256 {source_sha}")


if __name__ == "__main__":
    main()
