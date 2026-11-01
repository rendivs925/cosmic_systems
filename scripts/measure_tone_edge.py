#!/usr/bin/env python3
"""Measure a tone edge at a known image column, against the frame's own texture.

The terrain seam harness (`measure_patch_seam.py`) measures a discontinuity
across a *mesh* seam marked by the `patch-boundary` render mode. A regional
imagery edge is not a mesh feature: it is a geographic bound, and before it is
feathered it does not even sit where the bound is — it sits on whatever patch
boundary happened to own the tile. So the question is different, and so is the
statistic:

  * `step(x)`: the mean and 90th percentile of |luminance(x+1) - luminance(x)|
    over the sampled rows. A hard edge is a spike in one column; a cross-fade is
    a small change spread over many.
  * `texture`: the same statistic over the whole frame, which is what a column
    would measure if there were no edge at all.
  * `excursion`: max `step(x)` within `--span` pixels of the marked bound.

A frame whose edge has been feathered has `excursion` equal to `texture`: no
column near the bound carries a step that the frame's own detail does not.

With `--against`, the script also reports the tonal change between the two
frames as a function of column. That is how the *extent* of a cross-fade is
checked: the change must be confined to the declared band and must not extend
into the region it is not supposed to touch.

The marker frame comes from `imagery_region_mask.py`. Both tools read captured
frames and write nothing; neither is part of the runtime.
"""

from __future__ import annotations

import argparse
import sys

import numpy as np
from PIL import Image

LUMA = np.array([0.2126, 0.7152, 0.0722], dtype=np.float64)


def load_luminance(path: str) -> np.ndarray:
    frame = np.asarray(Image.open(path).convert("RGB")).astype(np.float64) / 255.0
    return frame @ LUMA


def reference_columns(mask: np.ndarray, threshold: float = 0.5) -> tuple[int, int]:
    columns = np.nonzero(mask.mean(axis=0) > threshold)[0]
    if columns.size == 0:
        raise SystemExit("the marker frame has no column above the threshold")
    return int(columns.min()), int(columns.max())


def step_profile(luminance: np.ndarray, rows: slice) -> tuple[np.ndarray, np.ndarray]:
    difference = np.abs(np.diff(luminance, axis=1))[rows] * 255.0
    return difference.mean(axis=0), np.percentile(difference, 90, axis=0)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frame", required=True)
    parser.add_argument("--marker", required=True, help="region bound marker frame")
    parser.add_argument("--against", default="", help="second frame, for the change profile")
    parser.add_argument("--rows", default="80:700", help="row slice to sample")
    parser.add_argument("--span", type=int, default=40, help="pixels around the bound")
    parser.add_argument("--metres-per-pixel", type=float, default=1.0)
    parser.add_argument("--size", default="1280x720")
    args = parser.parse_args()

    width, height = (int(value) for value in args.size.lower().split("x"))
    start, _, end = args.rows.partition(":")
    rows = slice(int(start), int(end) if end else height)

    marker = np.asarray(
        Image.open(args.marker).convert("L"), dtype=np.float64
    ) / 255.0
    if marker.shape[:2] != (height, width):
        raise SystemExit("the marker frame must have the same size as the frame")
    left, right = reference_columns(marker)
    bound = 0.5 * (left + right)

    luminance = load_luminance(args.frame)
    mean, p90 = step_profile(luminance, rows)
    lo = max(0, left - args.span)
    hi = min(len(mean) - 1, right + args.span)
    excursion = int(np.argmax(mean[lo : hi + 1])) + lo

    print(f"# {args.frame}")
    print(f"bound columns    {left}..{right} (centre {bound:.1f})")
    print(
        f"excursion        mean {mean[excursion]:6.2f}/255   p90 {p90[excursion]:6.2f}/255"
        f"   at x={excursion}"
    )
    print(
        f"at the bound     mean {mean[int(round(bound))]:6.2f}/255"
        f"   p90 {p90[int(round(bound))]:6.2f}/255"
    )
    print(
        f"frame texture    median mean {np.median(mean):5.2f}/255"
        f"   median p90 {np.median(p90):5.2f}/255"
    )
    print(
        f"frame maximum    mean {mean.max():6.2f}/255 at x={int(np.argmax(mean))}"
        f"   p90 {p90.max():6.2f}/255 at x={int(np.argmax(p90))}"
    )

    if args.against:
        other = load_luminance(args.against)
        if other.shape != luminance.shape:
            raise SystemExit("the compared frames must have the same size")
        change = (luminance - other)[rows].mean(axis=0) * 255.0
        magnitude = np.abs(change)
        cumulative = np.cumsum(magnitude) / magnitude.sum()
        low = int(np.argmax(cumulative > 0.10))
        high = int(np.argmax(cumulative > 0.90))
        span_m = (high - low) * args.metres_per_pixel
        # A tenth of a code value is far below the visual threshold, so this is
        # the honest extent of the change rather than its numerical tail.
        meaningful = np.nonzero(magnitude > 0.1)[0]
        print(
            f"change profile   peak {magnitude.max():6.2f}/255 at x={int(np.argmax(magnitude))}"
            f"   above 0.1/255 over x={int(meaningful.min())}..{int(meaningful.max())}"
        )
        print(
            f"                 10-90 % at x={low}..{high}"
            f"   {span_m / 1_000.0:.1f} km  ({high - low} px)"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
