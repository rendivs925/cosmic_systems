#!/usr/bin/env python3
"""Bake the LROC color Moon mosaic into the planet albedo texture.

Source: NASA Scientific Visualization Studio product 4720, an 8192x4096
equirectangular LRO LROC WAC color mosaic. No geometric reprojection is
performed: the source already shares the body-fixed longitude/latitude layout
used by the existing texture, verified by a zero-shift correlation and by
Mare Crisium (17N, 59E) and Tycho (43S, 11W).

A linear luminance contrast match to the superseded 2048x1024 texture keeps the
established display appearance; it is a presentation calibration, not an
absolute photometric claim. The simulator never downloads this source at
runtime and only loads the generated PNG.
"""

from __future__ import annotations

import argparse
import hashlib
import urllib.request
from pathlib import Path

import numpy as np
from PIL import Image

SOURCE_URL = (
    "https://svs.gsfc.nasa.gov/vis/a000000/a004700/a004720/lroc_color_poles_8k.tif"
)
SOURCE_SHA256 = "4af8b0cd4d50c30851359d98e7e72040240dd8d03256b58b345b5b76e9edb4ef"
SOURCE_BYTES = 50_641_970
SOURCE_SIZE = (8192, 4096)
CACHE = Path("assets/large_files/imagery/raw/moon/lroc_color_poles_8k.tif")
OUTPUT = Path("assets/textures/planets/moon/albedo.png")

# Superseded texture luminance statistics, used only to keep the Moon's
# on-screen brightness and contrast continuous across the upgrade.
TARGET_LUMINANCE_MEAN = 0.580
TARGET_LUMINANCE_STD = 0.1354


def fetch(force: bool) -> bytes:
    CACHE.parent.mkdir(parents=True, exist_ok=True)
    if force and CACHE.exists():
        CACHE.unlink()
    if not CACHE.exists():
        print(f"downloading {SOURCE_URL}", flush=True)
        with urllib.request.urlopen(SOURCE_URL, timeout=300) as response:
            CACHE.write_bytes(response.read())
    data = CACHE.read_bytes()
    if len(data) != SOURCE_BYTES:
        raise SystemExit(f"{CACHE} has {len(data)} bytes, expected {SOURCE_BYTES}")
    digest = hashlib.sha256(data).hexdigest()
    if digest != SOURCE_SHA256:
        raise SystemExit(f"{CACHE} has SHA-256 {digest}, expected {SOURCE_SHA256}")
    return data


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--force", action="store_true", help="redownload the source")
    args = parser.parse_args()
    fetch(args.force)

    Image.MAX_IMAGE_PIXELS = None
    image = Image.open(CACHE).convert("RGB")
    if image.size != SOURCE_SIZE:
        raise SystemExit(f"source is {image.size}, expected {SOURCE_SIZE}")
    pixels = np.asarray(image, dtype=np.float32) / 255.0
    luminance = (
        0.2126 * pixels[..., 0] + 0.7152 * pixels[..., 1] + 0.0722 * pixels[..., 2]
    )
    scale = TARGET_LUMINANCE_STD / float(luminance.std())
    offset = TARGET_LUMINANCE_MEAN - float(luminance.mean()) * scale
    matched = np.clip(pixels * scale + offset, 0.0, 1.0)
    output = Image.fromarray((matched * 255.0 + 0.5).astype(np.uint8), "RGB")
    output.save(OUTPUT, optimize=True, compress_level=6)
    data = OUTPUT.read_bytes()
    print(
        f"wrote {OUTPUT} ({len(data)} bytes, {output.size[0]}x{output.size[1]}) "
        f"sha256 {hashlib.sha256(data).hexdigest()}"
    )
    print(f"contrast scale {scale:.6f} offset {offset:.6f}")


if __name__ == "__main__":
    main()
