#!/usr/bin/env python3
"""Download and verify regional NASADEM source tiles.

The OpenTopography mirror is anonymous and public. Tiles are pinned by SHA-256
so an upstream change cannot silently alter the regional height authority.

Usage:
    python3 scripts/ingest_regional_dem.py
    python3 scripts/ingest_regional_dem.py --region papua_coastal_lowland
"""

from __future__ import annotations

import argparse
import hashlib
import shutil
import sys
import urllib.request
from pathlib import Path

MIRROR = "https://opentopography.s3.sdsc.edu/raster/NASADEM/NASADEM_be"

REGIONS = {
    "papua_coastal_lowland": {
        "destination": "assets/large_files/terrain/raw/nasadem",
        "tiles": {
            "NASADEM_HGT_s08e139.tif": "32de0bb1cae25d5df1400827ccc1a65601d58b7241bf726e0a1ba169807798de",
            "NASADEM_HGT_s09e139.tif": "54abf0dfd106b450031ba7885ae00081e8242725ac329f952b9e73200f7fa306",
        },
    },
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def download(url: str, destination: Path) -> None:
    temporary = destination.with_suffix(destination.suffix + ".part")
    print(f"downloading {url}")
    with urllib.request.urlopen(url, timeout=120) as response, temporary.open("wb") as output:
        shutil.copyfileobj(response, output, length=1024 * 1024)
    temporary.replace(destination)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--region",
        default="papua_coastal_lowland",
        choices=sorted(REGIONS),
        help="reviewed region to fetch",
    )
    args = parser.parse_args()
    region = REGIONS[args.region]
    destination = Path(region["destination"])
    destination.mkdir(parents=True, exist_ok=True)

    for name, expected in region["tiles"].items():
        path = destination / name
        if path.exists() and sha256(path) == expected:
            print(f"verified {path}")
            continue
        if path.exists():
            print(f"stale {path}; replacing", file=sys.stderr)
            path.unlink()
        download(f"{MIRROR}/{name}", path)
        found = sha256(path)
        if found != expected:
            path.unlink()
            print(
                f"checksum mismatch for {name}: expected {expected}, found {found}",
                file=sys.stderr,
            )
            return 1
        print(f"verified {path}")

    print(f"{args.region}: all sources verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
