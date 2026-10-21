#!/usr/bin/env python3
"""Derive the compact EGM2008 geoid grid used for Earth sea level.

ETOPO 2022 stores ellipsoidal heights after adding the published geoid tile.
The simulator still needs a cheap, global geoid field to know where sea level
is, so this script samples the authoritative 30 arc-second geoid tile every
0.5 degrees through the NOAA THREDDS OPeNDAP endpoint and writes a signed i16
centimetre raster.

Output layout:

    720 columns x 360 rows
    longitude centers from -179.9958333333 by 0.5 degrees, periodic
    latitude centers from 89.5041666667 down to -89.9958333333
    north-to-south, little-endian signed i16 centimetres

Usage:

    python3 scripts/ingest_egm2008_geoid.py \
        --output assets/large_files/terrain/earth_egm2008_geoid_0p5deg_i16cm.raw
"""

import argparse
import hashlib
import re
import urllib.request

import numpy as np

COLUMNS = 720
ROWS = 360
STRIDE = 60
URL = (
    "https://www.ngdc.noaa.gov/thredds/dodsC/global/ETOPO2022/30s/"
    "30s_geoid_netcdf/ETOPO_2022_v1_30s_N90W180_geoid.nc.ascii"
    "?z%5B0:60:21599%5D%5B0:60:43199%5D"
)
USER_AGENT = "cosmic-systems-egm2008-geoid-ingest/1.0"
EXPECTED_SHA256 = "f4ea75f9721eee1d0a3fea97a05d9dda6c166bed8300e73f8e7163d1cf59ffea"


def fetch():
    request = urllib.request.Request(URL, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=180) as response:
        return response.read().decode("ascii")


def parse(text):
    rows = {}
    for line in text.splitlines():
        match = re.match(r"^\[(\d+)\],\s*(.*)$", line.strip())
        if match:
            rows[int(match.group(1))] = np.fromstring(match.group(2), sep=",")
    if len(rows) != ROWS:
        raise RuntimeError(f"expected {ROWS} rows, found {len(rows)}")
    values = np.stack([rows[index] for index in range(ROWS)])
    if values.shape != (ROWS, COLUMNS):
        raise RuntimeError(f"unexpected grid shape {values.shape}")
    if not np.all(np.isfinite(values)):
        raise RuntimeError("geoid grid contains non-finite samples")
    return values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, help="signed i16 centimetre raster")
    args = parser.parse_args()

    values = parse(fetch())[::-1]
    centimetres = np.where(
        values >= 0.0, np.floor(values * 100.0 + 0.5), np.ceil(values * 100.0 - 0.5)
    ).astype("<i2")
    raw = centimetres.tobytes()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != EXPECTED_SHA256:
        raise RuntimeError(f"geoid grid SHA-256 mismatch: expected {EXPECTED_SHA256}, found {digest}")
    with open(args.output, "wb") as output:
        output.write(raw)
    print(f"wrote {args.output} ({len(raw)} bytes) sha256={digest}")
    print(
        f"  min={centimetres.min() / 100.0:.2f} m "
        f"max={centimetres.max() / 100.0:.2f} m"
    )


if __name__ == "__main__":
    main()
