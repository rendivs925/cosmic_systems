#!/usr/bin/env python3
"""Build a compact global land-cover fallback from ESA WorldCover COGs.

The 10 m WorldCover product is distributed as 2651 three-degree COGs. Instead
of downloading ~120 GB, this script reads only each COG's smallest internal
overview through HTTP range requests, caches those overviews, and mosaics them
onto a 5 arc-minute terrain-radial grid. Regional packages override this global
fallback at runtime.

Only the first 128 KiB of each COG is fetched. That prefix contains every IFD
and the smallest overview's compressed tile, so the script avoids both the
10 m payload and a GDAL/tifffile dependency. The output PNG is fed to
`land_cover_bake`. No download happens at runtime.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import re
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

S3_BASE = "https://esa-worldcover.s3.eu-central-1.amazonaws.com/"
PREFIX = "v200/2021/map/"
TILE_NAME = re.compile(
    r"ESA_WorldCover_10m_2021_v200_([NS])(\d{2})([EW])(\d{3})_Map\.tif$"
)
WGS84_E2 = 0.006_694_379_990_141_316_5
WORLDCOVER_TO_CLASS = {
    10: 0, 20: 1, 30: 2, 40: 3, 50: 4, 60: 5,
    70: 6, 80: 7, 90: 8, 95: 9, 100: 10,
}
NODATA_CLASS = 5
WATER_CLASS = 7
NAMESPACE = {"s3": "http://s3.amazonaws.com/doc/2006-03-01/"}


PREFIX_BYTES = 128 * 1024


def fetch_range(url: str, start: int, end: int) -> bytes:
    request = urllib.request.Request(url, headers={"Range": f"bytes={start}-{end}"})
    with urllib.request.urlopen(request, timeout=120) as response:
        return response.read()


def read_overview(url: str) -> np.ndarray:
    """Read the smallest TIFF overview from a COG's 128 KiB prefix."""
    prefix = fetch_range(url, 0, PREFIX_BYTES - 1)
    if prefix[:2] == b"II":
        endian = "little"
    elif prefix[:2] == b"MM":
        endian = "big"
    else:
        raise ValueError("not a TIFF file")
    if int.from_bytes(prefix[2:4], endian) != 42:
        raise ValueError("not a baseline TIFF file")
    ifd_offset = int.from_bytes(prefix[4:8], endian)

    pages: list[dict[int, tuple[str, int, int]]] = []
    while ifd_offset:
        if ifd_offset + 2 > len(prefix):
            raise ValueError("IFD outside prefix")
        entry_count = int.from_bytes(
            prefix[ifd_offset : ifd_offset + 2], endian
        )
        entries: dict[int, tuple[str, int, int]] = {}
        for index in range(entry_count):
            start = ifd_offset + 2 + index * 12
            entry = prefix[start : start + 12]
            tag = int.from_bytes(entry[0:2], endian)
            kind = int.from_bytes(entry[2:4], endian)
            count = int.from_bytes(entry[4:8], endian)
            value = entry[8:12]
            entries[tag] = (kind, count, value)
        pages.append(entries)
        next_offset = int.from_bytes(
            prefix[ifd_offset + 2 + entry_count * 12 : ifd_offset + 4 + entry_count * 12],
            endian,
        )
        if len(pages) > 32:
            raise ValueError("unreasonable IFD chain")
        ifd_offset = next_offset

    def integers(entries: dict[int, tuple[str, int, int]], tag: int) -> list[int]:
        kind, count, value = entries[tag]
        byte_width = 2 if kind == 3 else 4
        total = count * byte_width
        if total <= 4:
            raw = value[:total]
        else:
            offset = int.from_bytes(value, endian)
            end = offset + total
            if end > len(prefix):
                raise ValueError("tag array outside prefix")
            raw = prefix[offset:end]
        return [
            int.from_bytes(raw[index : index + byte_width], endian)
            for index in range(0, total, byte_width)
        ]

    page = min(
        pages,
        key=lambda entries: integers(entries, 256)[0] * integers(entries, 257)[0],
    )
    width = integers(page, 256)[0]
    height = integers(page, 257)[0]
    compression = integers(page, 259)[0]
    offsets = integers(page, 324)
    counts = integers(page, 325)
    if len(offsets) != 1 or len(counts) != 1:
        raise ValueError("smallest overview is not a single tile")
    offset, count = offsets[0], counts[0]
    if offset < len(prefix):
        payload = prefix[offset : offset + count]
    else:
        payload = fetch_range(url, offset, offset + count - 1)
    if compression == 8:
        payload = zlib.decompress(payload)
    elif compression != 1:
        raise ValueError(f"unsupported overview compression {compression}")
    data = np.frombuffer(payload, dtype=np.uint8)
    if data.size == width * height:
        return data.reshape(height, width)
    tile_width = integers(page, 322)[0]
    tile_height = integers(page, 323)[0]
    if data.size == tile_width * tile_height:
        return data.reshape(tile_height, tile_width)[:height, :width]
    raise ValueError("overview payload does not match dimensions")


def list_tiles() -> list[str]:
    keys: list[str] = []
    token = None
    while True:
        query = f"?list-type=2&prefix={PREFIX}&max-keys=1000"
        if token:
            query += f"&continuation-token={urllib.parse.quote(token)}"
        with urllib.request.urlopen(S3_BASE + query, timeout=120) as response:
            root = ET.fromstring(response.read())
        keys.extend(
            entry.find("s3:Key", NAMESPACE).text
            for entry in root.findall("s3:Contents", NAMESPACE)
        )
        truncated = root.find("s3:IsTruncated", NAMESPACE).text == "true"
        if not truncated:
            break
        token = root.find("s3:NextContinuationToken", NAMESPACE).text
    return [key for key in keys if TILE_NAME.search(key)]


def tile_bounds(key: str) -> tuple[float, float, float, float]:
    match = TILE_NAME.search(key)
    assert match is not None
    lat_sign = -1.0 if match.group(1) == "S" else 1.0
    lon_sign = -1.0 if match.group(3) == "W" else 1.0
    south = lat_sign * int(match.group(2))
    west = lon_sign * int(match.group(4))
    return west, south, west + 3.0, south + 3.0


def fetch_overview(key: str, cache: Path) -> Path:
    path = cache / (Path(key).stem + ".png")
    if path.exists():
        with Image.open(path) as cached:
            if cached.width * cached.height <= 6_000_000:
                return path
        path.unlink()
    last_error: Exception | None = None
    for _ in range(4):
        try:
            url = S3_BASE + key
            data = read_overview(url)
            path.parent.mkdir(parents=True, exist_ok=True)
            temporary = path.with_name(path.stem + ".part.png")
            Image.fromarray(data.astype(np.uint8), mode="L").save(temporary, format="PNG")
            temporary.replace(path)
            return path
        except Exception as error:  # network retries only
            last_error = error
    raise RuntimeError(f"failed to fetch overview for {key}: {last_error}")


def terrain_to_geodetic(latitude_deg: np.ndarray) -> np.ndarray:
    return np.degrees(
        np.arctan(np.tan(np.radians(latitude_deg)) / (1.0 - WGS84_E2))
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--cache",
        type=Path,
        default=Path("assets/large_files/terrain/raw/worldcover_overview"),
    )
    parser.add_argument(
        "--output-png",
        type=Path,
        default=Path("assets/large_files/terrain/raw/landcover/global_5arcmin.png"),
    )
    parser.add_argument("--resolution-deg", type=float, default=1.0 / 12.0)
    parser.add_argument("--south", type=float, default=-60.0)
    parser.add_argument("--north", type=float, default=84.0)
    parser.add_argument("--workers", type=int, default=24)
    arguments = parser.parse_args()

    keys = list_tiles()
    print(f"WorldCover map tiles: {len(keys)}")
    arguments.cache.mkdir(parents=True, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=arguments.workers) as pool:
        overviews = list(pool.map(lambda key: fetch_overview(key, arguments.cache), keys))
    print(f"cached overviews: {len(overviews)} in {arguments.cache}")

    west, east = -180.0, 180.0
    south, north = arguments.south, arguments.north
    width = round((east - west) / arguments.resolution_deg) + 1
    height = round((north - south) / arguments.resolution_deg) + 1
    terrain_latitudes = north - np.arange(height) * arguments.resolution_deg
    geodetic_latitudes = terrain_to_geodetic(terrain_latitudes)
    longitudes = west + np.arange(width) * arguments.resolution_deg
    output = np.full((height, width), WATER_CLASS, dtype=np.uint8)

    for key, overview_path in zip(keys, overviews):
        tile_west, tile_south, tile_east, tile_north = tile_bounds(key)
        rows = np.flatnonzero(
            (geodetic_latitudes >= tile_south) & (geodetic_latitudes < tile_north)
        )
        columns = np.flatnonzero(
            (longitudes >= tile_west) & (longitudes < tile_east)
        )
        if rows.size == 0 or columns.size == 0:
            continue
        overview = np.asarray(Image.open(overview_path))
        overview_height, overview_width = overview.shape
        pixel_x = (tile_east - tile_west) / overview_width
        pixel_y = (tile_north - tile_south) / overview_height
        source_rows = np.rint(
            (tile_north - geodetic_latitudes[rows]) / pixel_y
        ).astype(np.int64)
        source_columns = np.rint(
            (longitudes[columns] - tile_west) / pixel_x
        ).astype(np.int64)
        source_rows = np.clip(source_rows, 0, overview_height - 1)
        source_columns = np.clip(source_columns, 0, overview_width - 1)
        sample = overview[np.ix_(source_rows, source_columns)]
        lut = np.full(256, NODATA_CLASS, dtype=np.uint8)
        for source_code, class_code in WORLDCOVER_TO_CLASS.items():
            lut[source_code] = class_code
        output[np.ix_(rows, columns)] = lut[sample]

    arguments.output_png.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(output, mode="L").save(arguments.output_png)
    print(f"wrote {arguments.output_png}: {width}x{height}")
    unique, counts = np.unique(output, return_counts=True)
    print("classes:", {int(k): int(v) for k, v in zip(unique, counts)})
    digest = hashlib.sha256()
    for key, overview_path in zip(keys, overviews):
        digest.update(
            f"{hashlib.sha256(overview_path.read_bytes()).hexdigest()}  {key}\n".encode()
        )
    print("overview_aggregate_sha256:", digest.hexdigest())
    print("normalized_png_sha256:", hashlib.sha256(arguments.output_png.read_bytes()).hexdigest())
    print(
        "bake: cargo run --release --features native --bin land_cover_bake -- "
        f"{arguments.output_png} "
        "assets/large_files/terrain/landcover_v1/00_global_landcover_5arcmin.clcvr "
        f"-180 {south:.6f} 180 {north:.6f} <aggregate-sha256> "
        "'ESA WorldCover 2021 v200, 2651 COG overviews' 'CC BY 4.0' 600.0"
    )


if __name__ == "__main__":
    main()
