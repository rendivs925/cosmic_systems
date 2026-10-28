#!/usr/bin/env python3
"""Bake the Yale Bright Star Catalog (BSC5) into a compact `.csstar` package.

The source is the 9,110-star fifth edition of the Yale Bright Star Catalog
(Hoffleit & Warren, 1991), distributed by the SAO Telescope Data Center in its
documented binary format with J2000 coordinates:

    http://tdc-www.harvard.edu/catalogs/BSC5

The output is a frame-neutral, versioned package of ICRS right ascension and
declination (radians), V magnitude, and B-V colour index. The simulator applies
the authoritative ICRF-to-solar-inertial conversion at load time. No runtime
path downloads data.

The source has no B-V column, so colour is derived from the two-character
spectral type using the main-sequence B-V anchors below. This is a documented
approximation, not measured photometry.
"""

from __future__ import annotations

import argparse
import hashlib
import struct
import urllib.request
from pathlib import Path

SOURCE_URL = "http://tdc-www.harvard.edu/catalogs/BSC5"
RAW_ROOT = Path("assets/large_files/stars")
DEFAULT_INPUT = RAW_ROOT / "BSC5"
DEFAULT_OUTPUT = Path("assets/data/stars/yale_bsc5_v1.csstar")
MAGIC = b"CSSTAR\0\0"
VERSION = 1
HEADER_BYTES = 16
RECORD_BYTES = 16

# Main-sequence B-V anchors keyed by (spectral class, subclass).
BV_ANCHORS = (
    ("O", 5, -0.33),
    ("B", 0, -0.30),
    ("B", 5, -0.16),
    ("B", 9, -0.06),
    ("A", 0, 0.00),
    ("A", 5, 0.15),
    ("F", 0, 0.30),
    ("F", 5, 0.44),
    ("G", 0, 0.58),
    ("G", 5, 0.68),
    ("K", 0, 0.81),
    ("K", 5, 1.15),
    ("M", 0, 1.40),
    ("M", 5, 1.64),
    ("M", 9, 2.00),
)
CLASS_ORDER = "OBAFGKM"
# Non-standard spectral classes are mapped to their nearest visual analogue.
CLASS_FALLBACK_BV = {
    "W": -0.25,  # Wolf-Rayet: hot, blue
    "C": 1.90,  # carbon star: very red
    "N": 1.90,  # old carbon star
    "S": 1.80,  # zirconium-monoxide star
    "P": 0.40,  # peculiar
    " ": 0.40,
}


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def spectral_bv(spectral_type: str) -> float:
    """Interpolate a main-sequence B-V estimate from a two-character type."""
    text = spectral_type.strip()
    if not text:
        return 0.40
    letter = text[0].upper()
    if letter not in CLASS_ORDER:
        return CLASS_FALLBACK_BV.get(letter, 0.40)
    try:
        subclass = int(text[1])
    except (IndexError, ValueError):
        subclass = 5
    subclass = min(max(subclass, 0), 9)
    key = CLASS_ORDER.index(letter) * 10 + subclass
    anchors = [(CLASS_ORDER.index(c) * 10 + s, bv) for c, s, bv in BV_ANCHORS]
    if key <= anchors[0][0]:
        return anchors[0][1]
    for (left_key, left_bv), (right_key, right_bv) in zip(anchors, anchors[1:]):
        if key <= right_key:
            span = right_key - left_key
            weight = (key - left_key) / span if span else 0.0
            return left_bv + (right_bv - left_bv) * weight
    return anchors[-1][1]


def fetch_source(path: Path, force: bool) -> bytes:
    if force and path.exists():
        path.unlink()
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        print(f"downloading {SOURCE_URL}", flush=True)
        with urllib.request.urlopen(SOURCE_URL, timeout=120) as response:
            data = response.read()
        path.write_bytes(data)
    return path.read_bytes()


def parse_header(raw: bytes, endian: str) -> tuple[int, ...]:
    if len(raw) < 28:
        raise SystemExit("BSC5 header is truncated")
    return struct.unpack_from(f"{endian}7i", raw, 0)


def header_is_sane(header: tuple[int, ...], raw: bytes) -> bool:
    star1, count, stnum, mprop, nmag, nbent = header[1:]
    return (
        star1 >= 1
        and 1_000 <= abs(count) <= 100_000
        and abs(stnum) <= 4
        and 0 <= abs(mprop) <= 2
        and 1 <= abs(nmag) <= 10
        and 8 <= abs(nbent) <= 256
        and 28 + abs(count) * abs(nbent) == len(raw)
    )


def parse_catalog(raw: bytes) -> list[tuple[float, float, float, float]]:
    """Return (ra_rad, dec_rad, v_mag, bv) records from a J2000 catalogue."""
    header = parse_header(raw, "<")
    endian = "<"
    if not header_is_sane(header, raw):
        header = parse_header(raw, ">")
        endian = ">"
        if not header_is_sane(header, raw):
            raise SystemExit("BSC5 header does not describe this file")
    _, _, starn, stnum, mprop, nmag, nbent = header
    if starn >= 0:
        raise SystemExit(
            "BSC5 file declares B1950 coordinates; a J2000 catalog is required"
        )
    count = abs(starn)
    nmag = abs(nmag)
    nbent = abs(nbent)
    records = []
    for index in range(count):
        offset = 28 + index * nbent
        if stnum > 0:
            offset += 4  # XNO / catalog number
        ra, dec = struct.unpack_from(f"{endian}dd", raw, offset)
        offset += 16
        spectral = raw[offset : offset + 2].decode("ascii", "replace")
        offset += 2
        magnitudes = struct.unpack_from(f"{endian}{'h' * nmag}", raw, offset)
        v_mag = magnitudes[0] / 100.0
        if not (-2.0 <= v_mag <= 10.0):
            raise SystemExit(f"record {index} has an implausible V magnitude {v_mag}")
        if not (0.0 <= ra < 2.0 * 3.141592653589793) or not (
            -1.5707964 <= dec <= 1.5707964
        ):
            raise SystemExit(f"record {index} has invalid J2000 coordinates")
        records.append((ra, dec, v_mag, spectral_bv(spectral)))
    return records


def bake(records: list[tuple[float, float, float, float]], output: Path) -> bytes:
    header = MAGIC + struct.pack("<II", VERSION, len(records))
    payload = bytearray(header)
    for ra, dec, v_mag, bv in records:
        payload += struct.pack("<ffff", ra, dec, v_mag, bv)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(payload)
    return bytes(payload)


def verify(path: Path, expected_count: int) -> None:
    raw = path.read_bytes()
    if raw[:8] != MAGIC:
        raise SystemExit("written package has a bad magic value")
    version, count = struct.unpack_from("<II", raw, 8)
    if version != VERSION or count != expected_count:
        raise SystemExit("written package has an unexpected header")
    if len(raw) != HEADER_BYTES + count * RECORD_BYTES:
        raise SystemExit("written package has an unexpected length")
    for ra, dec, v_mag, bv in struct.iter_unpack("<ffff", raw[HEADER_BYTES:]):
        if not (0.0 <= ra < 2.0 * 3.141592653589793):
            raise SystemExit("written package has an out-of-range right ascension")
        if not (-1.5707964 <= dec <= 1.5707964):
            raise SystemExit("written package has an out-of-range declination")
        if not (-2.0 <= v_mag <= 10.0) or not (-1.0 <= bv <= 3.0):
            raise SystemExit("written package has an out-of-range magnitude or colour")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, default=DEFAULT_INPUT)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--force", action="store_true", help="redownload the source")
    args = parser.parse_args()

    raw = fetch_source(args.input, args.force)
    source_sha = sha256_bytes(raw)
    print(f"source {SOURCE_URL} sha256 {source_sha} ({len(raw)} bytes)")

    records = parse_catalog(raw)
    payload = bake(records, args.output)
    verify(args.output, len(records))

    magnitudes = sorted(record[2] for record in records)
    print(
        f"stars {len(records)}  V {magnitudes[0]:.2f}..{magnitudes[-1]:.2f}  "
        f"V<=6.5 {sum(1 for m in magnitudes if m <= 6.5)}"
    )
    print(f"wrote {args.output} ({len(payload)} bytes) sha256 {sha256_bytes(payload)}")


if __name__ == "__main__":
    main()
