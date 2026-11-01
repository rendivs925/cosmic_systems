#!/usr/bin/env python3
"""Measure the tone discontinuity a terrain patch boundary adds to a frame.

Acceptance for the tone-block defect is phrased against the *surrounding trend*:
"no block boundary visible that does not correspond to a real albedo change
within +/-1/255 of the surrounding trend". A pair of boxes either side of a
boundary cannot answer that, because the two boxes may genuinely differ: a real
slope of 30 % over 8 px is already 2.4/255.

This tool therefore measures a *discontinuity*, not a difference:

  * seam mask    = `patch-boundary` diagnostic frame above `--seam-threshold`;
                   that view marks patch edges and nothing else, so the tool
                   never has to guess where a boundary is.
  * seam normal  = gradient of a blurred seam mask.
  * side fit     = a straight line through the frame luminance sampled at
                   d = 3..8 px along the normal on one side of the seam,
                   evaluated at d = 0.
  * seam jump    = side fit(+) - side fit(-): the step that is left once each
                   side's own slope is removed. A continuous field with any
                   slope gives zero.
  * control jump = the same estimator at p +/- c*t, i.e. c pixels along the seam,
                   inside the neighbouring patch interior.
  * excess       = median(|seam jump|) - median(|control jump|).

A value near zero means the boundary is indistinguishable from the terrain
around it. Samples are skipped when the two sides are not the same surface class
(sea vs land, so a seam running along a coastline cannot masquerade as a defect),
when either side is within a pixel of another seam, or when a cloud is within a
few pixels.

Usage:
  measure_patch_seam.py --boundary <patch-boundary frame> --frame <frame>

Both frames must share a pose. The tool reads PNG/JPG, writes nothing, and is not
part of the runtime.
"""

from __future__ import annotations

import argparse
import sys

import numpy as np
from PIL import Image

LUMA = np.array([0.2126, 0.7152, 0.0722], dtype=np.float64)
QUANTISATION = 255.0


def load(path: str) -> np.ndarray:
    """Load a frame as 0..1 floats (sRGB bytes / 255)."""
    return np.asarray(Image.open(path).convert("RGB")).astype(np.float64) / 255.0


def luminance(frame: np.ndarray) -> np.ndarray:
    return frame @ LUMA


# Frames are 0..1, so these are the code values divided by 255.
WATER_MIN_BLUE = 45.0 / 255.0
CLOUD_LUMINANCE = 115.0 / 255.0


def classify_from_land_sea(frame: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Surface class from a `land-sea` diagnostic frame.

    That view draws water blue, land green and the datum band white, and it hides
    the deck, so it is a far better classifier than a colour heuristic on a
    production frame: bright shallow water cannot be mistaken for land. Returns
    `(water, usable)`, where `usable` excludes the white datum band.
    """
    red, green, blue = frame[:, :, 0], frame[:, :, 1], frame[:, :, 2]
    white_band = (red > 0.8) & (green > 0.8) & (blue > 0.8)
    return blue > green, ~white_band


def is_water(frame: np.ndarray, lum: np.ndarray) -> np.ndarray:
    """Sea mask, used only to keep a step from being measured across a coastline."""
    return (
        (frame[:, :, 2] > frame[:, :, 0] + 8.0 / 255.0)
        & (frame[:, :, 2] > WATER_MIN_BLUE)
        & (lum <= CLOUD_LUMINANCE)
    )


def box_blur(image: np.ndarray, radius: int) -> np.ndarray:
    """Separable box blur; a small radius gives a stable seam normal."""
    kernel = 2 * radius + 1
    padded = np.pad(image, radius, mode="edge")
    horizontal = np.add.reduce(
        [padded[:, i : i + image.shape[1]] for i in range(kernel)]
    )
    return np.add.reduce(
        [horizontal[i : i + image.shape[0], :] for i in range(kernel)]
    ) / (kernel * kernel)


def sample_bilinear(image: np.ndarray, x: float, y: float) -> float | None:
    height, width = image.shape
    if not (1.0 <= x < width - 1.0 and 1.0 <= y < height - 1.0):
        return None
    x0, y0 = int(np.floor(x)), int(np.floor(y))
    fx, fy = x - x0, y - y0
    top = image[y0, x0] * (1.0 - fx) + image[y0, x0 + 1] * fx
    bottom = image[y0 + 1, x0] * (1.0 - fx) + image[y0 + 1, x0 + 1] * fx
    return float(top * (1.0 - fy) + bottom * fy)


def near_seam(mask: np.ndarray, x: float, y: float, radius: int) -> bool:
    height, width = mask.shape
    x0, x1 = max(0, int(x) - radius), min(width, int(x) + radius + 1)
    y0, y1 = max(0, int(y) - radius), min(height, int(y) + radius + 1)
    if x0 >= x1 or y0 >= y1:
        return True
    return bool(mask[y0:y1, x0:x1].any())


def in_frame(image: np.ndarray, x: float, y: float) -> bool:
    height, width = image.shape
    return 1.0 <= x < width - 1.0 and 1.0 <= y < height - 1.0


def side_fit(
    image: np.ndarray,
    x: float,
    y: float,
    nx: float,
    ny: float,
    sign: float,
    offsets: tuple[int, ...],
) -> float | None:
    """Extrapolate one side of the seam to d = 0 with a straight-line fit."""
    distances: list[float] = []
    values: list[float] = []
    for d in offsets:
        value = sample_bilinear(image, x + sign * d * nx, y + sign * d * ny)
        if value is None:
            return None
        distances.append(float(d))
        values.append(value)
    if len(distances) < 2:
        return None
    # A quadratic absorbs the terrain's own curvature over the sampled span; a
    # straight line would turn a ridge or a valley into an apparent jump. The
    # discontinuity is the part neither fit can contain.
    degree = 2 if len(distances) >= 3 else 1
    design = np.vstack(
        [np.array(distances) ** power for power in range(degree + 1)]
    ).T
    coefficients, *_ = np.linalg.lstsq(design, np.array(values), rcond=None)
    return float(coefficients[0])


def jump(
    lum: np.ndarray,
    seam: np.ndarray,
    water: np.ndarray,
    x: float,
    y: float,
    nx: float,
    ny: float,
    offsets: tuple[int, ...],
    want_water: bool | None,
) -> float | None:
    """Discontinuity at (x, y) along the normal, or None if the sample is unusable."""
    for sign in (1.0, -1.0):
        # Every sample on both sides must be in frame, clear of another seam and
        # of the same surface class as the centre. Checking the whole span, not
        # just its end point, keeps a seam that crosses a coastline at an angle
        # out of the sample.
            for d in offsets:
                px, py = x + sign * d * nx, y + sign * d * ny
                if not in_frame(lum, px, py) or near_seam(seam, px, py, 1):
                    return None
                if want_water is not None and bool(water[int(py), int(px)]) != want_water:
                    return None
    plus = side_fit(lum, x, y, nx, ny, 1.0, offsets)
    minus = side_fit(lum, x, y, nx, ny, -1.0, offsets)
    if plus is None or minus is None:
        return None
    return plus - minus


def measure(
    boundary: np.ndarray,
    frame: np.ndarray,
    seam_threshold: float,
    offsets: tuple[int, ...],
    control_offset: int,
    skip_radius: int,
    region: tuple[int, int, int, int] | None = None,
    surface: str = "any",
    mask_clouds: bool = True,
    class_frame: np.ndarray | None = None,
) -> dict[str, float]:
    seam = luminance(boundary) * QUANTISATION > seam_threshold
    if not seam.any():
        raise SystemExit("the boundary frame has no seam pixels above the threshold")

    blurred = box_blur(seam.astype(np.float64), 2)
    grad_y, grad_x = np.gradient(blurred)
    lum = luminance(frame)
    if class_frame is None:
        water = is_water(frame, lum)
        usable = np.ones_like(water)
    else:
        water, usable = classify_from_land_sea(class_frame)
    # A cloud edge is a real, large step; mask it so it cannot enter either set.
    if mask_clouds:
        cloud = box_blur((lum > CLOUD_LUMINANCE).astype(np.float64), 8) > 0.02
    else:
        cloud = np.zeros_like(seam)

    seam_jumps: list[float] = []
    control_jumps: list[float] = []
    rows, cols = np.nonzero(seam)
    for y, x in zip(rows, cols):
        if region is not None:
            x0, y0, x1, y1 = region
            if not (x0 <= x < x1 and y0 <= y < y1):
                continue
        if cloud[y, x]:
            continue
        want_water = {"any": None, "water": True, "land": False}[surface]
        if not usable[y, x]:
            continue
        if want_water is not None and bool(water[y, x]) != want_water:
            continue
        gx, gy = grad_x[y, x], grad_y[y, x]
        norm = np.hypot(gx, gy)
        if norm < 1e-6:
            continue
        nx, ny = gx / norm, gy / norm
        tx, ty = -ny, nx

        seam_jump = jump(lum, seam, water, x, y, nx, ny, offsets, want_water)
        if seam_jump is None:
            continue

        controls: list[float] = []
        for direction in (1.0, -1.0):
            cx = x + direction * control_offset * tx
            cy = y + direction * control_offset * ty
            if not in_frame(lum, cx, cy):
                continue
            if cloud[int(cy), int(cx)]:
                continue
            if near_seam(seam, cx, cy, skip_radius):
                continue
            control_jump = jump(lum, seam, water, cx, cy, nx, ny, offsets, want_water)
            if control_jump is not None:
                controls.append(control_jump)
        if not controls:
            continue
        seam_jumps.append(abs(seam_jump))
        control_jumps.append(float(np.mean(np.abs(controls))))

    if not seam_jumps:
        raise SystemExit("no usable seam samples: check the threshold and frames")

    seam_jumps = np.array(seam_jumps)
    control_jumps = np.array(control_jumps)
    difference = seam_jumps - control_jumps
    return {
        "samples": float(len(seam_jumps)),
        "seam_median": float(np.median(seam_jumps)),
        "control_median": float(np.median(control_jumps)),
        "excess_median": float(np.median(difference)),
        "excess_mean": float(np.mean(difference)),
        "excess_stderr": float(np.std(difference) / np.sqrt(len(difference))),
        "excess_p95": float(np.percentile(difference, 95)),
        "seam_p90": float(np.percentile(seam_jumps, 90)),
        "control_p90": float(np.percentile(control_jumps, 90)),
        "over_1": float(np.mean(difference > 1.0 / QUANTISATION)),
        "over_3": float(np.mean(difference > 3.0 / QUANTISATION)),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--boundary", required=True, help="patch-boundary diagnostic frame")
    parser.add_argument("--frame", required=True, help="frame at the same pose")
    parser.add_argument("--label", default="", help="name printed with the result")
    parser.add_argument(
        "--seam-threshold",
        type=float,
        default=150.0,
        help="0-255 code value above which a boundary-frame pixel is a seam",
    )
    parser.add_argument(
        "--offsets",
        default="3-8",
        help="per-side sample offsets along the normal, as start-end",
    )
    parser.add_argument("--control-offset", type=int, default=40, help="pixels along the seam")
    parser.add_argument("--skip-radius", type=int, default=6)
    parser.add_argument(
        "--class-frame",
        default="",
        help="land-sea diagnostic frame at the same pose, for surface classes",
    )
    parser.add_argument(
        "--no-cloud-mask",
        action="store_true",
        help="diagnostic frames hide the deck; their false colours must not be masked",
    )
    parser.add_argument(
        "--surface",
        choices=("any", "land", "water"),
        default="any",
        help="restrict samples to one surface class",
    )
    parser.add_argument(
        "--region",
        default="",
        help="x0,y0,x1,y1 pixel window to restrict samples to",
    )
    args = parser.parse_args()

    start, _, end = args.offsets.partition("-")
    offsets = tuple(range(int(start), int(end) + 1))
    if len(offsets) < 2:
        raise SystemExit("--offsets needs at least two distances")

    boundary = load(args.boundary)
    frame = load(args.frame)
    if boundary.shape[:2] != frame.shape[:2]:
        raise SystemExit("boundary and frame must have the same size")
    region = None
    if args.region:
        parts = [int(value) for value in args.region.split(",")]
        if len(parts) != 4:
            raise SystemExit("--region needs x0,y0,x1,y1")
        region = (parts[0], parts[1], parts[2], parts[3])
    result = measure(
        boundary,
        frame,
        args.seam_threshold,
        offsets,
        args.control_offset,
        args.skip_radius,
        region,
        args.surface,
        not args.no_cloud_mask,
        load(args.class_frame) if args.class_frame else None,
    )
    print(f"# {args.label or args.frame}")
    print(f"samples          {int(result['samples'])}")
    print(f"seam jump median {result['seam_median'] * QUANTISATION:6.2f}/255")
    print(f"control median   {result['control_median'] * QUANTISATION:6.2f}/255")
    print(
        f"excess (median)  {result['excess_median'] * QUANTISATION:+6.2f}/255"
        f"   (mean {result['excess_mean'] * QUANTISATION:+.2f} "
        f"+/- {1.96 * result['excess_stderr'] * QUANTISATION:.2f} 95% CI)"
    )
    print(f"excess p95       {result['excess_p95'] * QUANTISATION:+6.2f}/255")
    print(
        f"share over 1/255 {result['over_1'] * 100:5.1f} %"
        f"   over 3/255 {result['over_3'] * 100:5.1f} %"
    )
    print(f"seam p90         {result['seam_p90'] * QUANTISATION:6.2f}/255")
    print(f"control p90      {result['control_p90'] * QUANTISATION:6.2f}/255")
    return 0


if __name__ == "__main__":
    sys.exit(main())
