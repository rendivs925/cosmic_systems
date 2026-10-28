#!/usr/bin/env python3
"""Provision the EGM2008 coefficient text used by `gravity_harmonics_bake`.

The canonical NGA archive is the degree-2190 spherical-harmonic release, but it
does not support byte ranges and this environment's connection resets after
about 23 MB. The checked mirror below is a complete degree-362 EGM2008
distribution with the same fully normalized coefficients; the low-degree terms
are verified against the checked-in EGM2008 J2 extract during baking.

This script only provisions a local source file. The simulator never downloads
data at runtime and only reads the baked `.csgravc` package.
"""

from __future__ import annotations

import argparse
import hashlib
import urllib.request
from pathlib import Path

CANONICAL_URL = "https://earth-info.nga.mil/php/download.php?file=egm-08spherical"
SOURCE_URL = (
    "https://raw.githubusercontent.com/zcyphygeodesy/"
    "EGMallelementcalc/main/EGM2008.gfc"
)
SOURCE_SHA256 = "4cc2b0e8824deb94407ffe9e396eac5df0cb5dc33d4207878a25eac604c0a701"
SOURCE_BYTES = 5_020_830
DESTINATION = Path(
    "assets/large_files/gravity/raw/egm2008/egm2008_to362_tidefree.gfc"
)


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
        raise SystemExit(
            f"{DESTINATION} has {len(data)} bytes, expected {SOURCE_BYTES}"
        )
    digest = hashlib.sha256(data).hexdigest()
    if digest != SOURCE_SHA256:
        raise SystemExit(f"{DESTINATION} has SHA-256 {digest}, expected {SOURCE_SHA256}")
    print(f"source {SOURCE_URL}")
    print(f"canonical archive {CANONICAL_URL}")
    print(f"sha256 {digest}  {len(data)} bytes")
    print(
        "bake: cargo run --release --features native --bin gravity_harmonics_bake -- "
        f"{DESTINATION} assets/large_files/gravity/egm2008_cs360_v1.csgravc 360"
    )


if __name__ == "__main__":
    main()
