#!/usr/bin/env python3
"""Provision the PDS GMM-3 Mars gravity table and print the bake command.

The simulator never downloads data at runtime; it reads only the baked
`.csgravc` package. This package is currently data readiness for future Mars
flight and is not loaded by any runtime force model.
"""

from __future__ import annotations

import argparse
import hashlib
import urllib.request
from pathlib import Path

SOURCE_URL = (
    "https://pds-geosciences.wustl.edu/mro/mro-m-rss-5-sdp-v1/"
    "mrors_1xxx/data/shadr/gmm3_120_sha.tab"
)
SOURCE_SHA256 = "eb4913b1afb6682406e6a9dad5be7918a162fa8462473c9a2e7aae258d4c2c9c"
SOURCE_BYTES = 900_604
DESTINATION = Path("assets/large_files/gravity/raw/gmm3/gmm3_120_sha.tab")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--force", action="store_true", help="redownload")
    args = parser.parse_args()

    DESTINATION.parent.mkdir(parents=True, exist_ok=True)
    if args.force and DESTINATION.exists():
        DESTINATION.unlink()
    if not DESTINATION.exists():
        print(f"downloading {SOURCE_URL}", flush=True)
        with urllib.request.urlopen(SOURCE_URL, timeout=180) as response:
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
        f"{DESTINATION} assets/large_files/gravity/gmm3_cs120_v1.csgravc gmm3 120"
    )


if __name__ == "__main__":
    main()
