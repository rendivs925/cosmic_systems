#!/usr/bin/env python3
"""Provision the PDS GRGM660PRIM lunar gravity table and print the bake command.

The simulator never downloads data at runtime; it reads only the baked
`.csgravc` package. This package is currently data readiness for future lunar
flight and is not loaded by any runtime force model.
"""

from __future__ import annotations

import argparse
import hashlib
import urllib.request
from pathlib import Path

SOURCE_URL = (
    "https://pds-geosciences.wustl.edu/grail/grail-l-lgrs-5-rdr-v1/"
    "grail_1001/shadr/gggrx_0660pm_sha.tab"
)
SOURCE_SHA256 = "a77ee9996aff7341ef89f69aa401f39ba0c2949ed80411505fb4826b771dd63e"
SOURCE_BYTES = 26_692_624
DESTINATION = Path("assets/large_files/gravity/raw/grgm/gggrx_0660pm_sha.tab")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--force", action="store_true", help="redownload")
    args = parser.parse_args()

    DESTINATION.parent.mkdir(parents=True, exist_ok=True)
    if args.force and DESTINATION.exists():
        DESTINATION.unlink()
    if not DESTINATION.exists():
        print(f"downloading {SOURCE_URL}", flush=True)
        with urllib.request.urlopen(SOURCE_URL, timeout=300) as response:
            DESTINATION.write_bytes(response.read())
    data = DESTINATION.read_bytes()
    if len(data) != SOURCE_BYTES:
        raise SystemExit(f"{DESTINATION} has {len(data)} bytes, expected {SOURCE_BYTES}")
    digest = hashlib.sha256(data).hexdigest()
    if digest != SOURCE_SHA256:
        raise SystemExit(f"{DESTINATION} has SHA-256 {digest}, expected {SOURCE_SHA256}")
    print(f"source {SOURCE_URL}")
    print(f"sha256 {digest}  {len(data)} bytes")
    print(
        "bake: cargo run --release --features native --bin shadr_gravity_bake -- "
        f"{DESTINATION} assets/large_files/gravity/grgm660_cs660_v1.csgravc grgm660 660"
    )


if __name__ == "__main__":
    main()
