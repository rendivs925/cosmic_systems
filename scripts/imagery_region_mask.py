#!/usr/bin/env python3
"""Project a regional imagery region's geographic bounds into a capture frame.

`scripts/measure_patch_seam.py` measures the tone discontinuity across a seam
given a frame that marks that seam. A terrain patch seam comes from the
`patch-boundary` render mode; a region bound has no such mode, because it is a
geographic box rather than a mesh feature. This tool produces the marker frame
for it, so the same harness can measure the imagery inset against its own trend.

The projection is the capture camera's, not an approximation of it:

  * nadir view from `<lat>,<lon>,<altitude_m>` looking at the planet centre,
  * perspective with a 45 deg vertical field of view and the frame's aspect,
  * the planet's north pole as camera up (so north is up and west is right),
  * rays intersected with the sea-level sphere of the planet.

Those are the same conventions as `rocket/capture_view.rs`; the 1,200 km frames
were placed with them and match the mapped geography. The marker is a thin
white line on black, one segment per bound, drawn only along the span where the
bound is a real edge (the other coordinate is inside the box).

Usage:
  imagery_region_mask.py --pose "-8.0,139.9,40000" \\
      --bounds "139.0,-8.6,140.0,-7.4" --size 1280x720 --out /tmp/region.png

The output is a marker, not a diagnostic render: it says where the region edge
is in the frame and is never used by the simulator.
"""

from __future__ import annotations

import argparse
import sys

import numpy as np
from PIL import Image

# Earth's mean radius as the terrain code uses it (`radius_km * 1000`).
PLANET_RADIUS_M = 6_371_000.0
VERTICAL_FOV_DEG = 45.0


def body_fixed_direction(latitude_deg: float, longitude_deg: float) -> np.ndarray:
    """The terrain lat/lon-to-body-fixed mapping of the simulation."""
    latitude = np.radians(latitude_deg)
    longitude = np.radians(longitude_deg)
    return np.array(
        [
            np.cos(latitude) * np.cos(longitude),
            np.sin(latitude),
            np.cos(latitude) * np.sin(longitude),
        ]
    )


def frame_axes(
    latitude_deg: float, longitude_deg: float, altitude_m: float
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """Camera position plus its `(forward, right, up)` axes in body-fixed axes.

    `right` points west and `up` points north, which is what makes a captured
    frame read north-up with west to the right.
    """
    position = (PLANET_RADIUS_M + altitude_m) * body_fixed_direction(
        latitude_deg, longitude_deg
    )
    forward = -position / np.linalg.norm(position)
    north = np.array([0.0, 1.0, 0.0])
    up = north - np.dot(north, forward) * forward
    if np.linalg.norm(up) < 1e-6:
        # Straight down a pole: use the viewpoint's own meridian as up.
        up = np.cross(forward, np.array([0.0, 0.0, 1.0]))
    up /= np.linalg.norm(up)
    right = np.cross(forward, up)
    return position, forward, right, up


def geographic_grid(
    position: np.ndarray,
    forward: np.ndarray,
    right: np.ndarray,
    up: np.ndarray,
    width: int,
    height: int,
) -> tuple[np.ndarray, np.ndarray]:
    """Body-fixed latitude/longitude degrees for every pixel centre."""
    focal = (height * 0.5) / np.tan(np.radians(VERTICAL_FOV_DEG) * 0.5)
    xs = np.arange(width, dtype=np.float64) + 0.5 - width * 0.5
    ys = height * 0.5 - (np.arange(height, dtype=np.float64) + 0.5)
    forward_component = np.broadcast_to(forward, (height, width, 3))
    right_component = (xs / focal)[None, :, None] * right[None, None, :]
    up_component = (ys / focal)[:, None, None] * up[None, None, :]
    directions = forward_component + right_component + up_component
    directions /= np.linalg.norm(directions, axis=2, keepdims=True)

    b = np.einsum("ijk,k->ij", directions, position)
    c = float(np.dot(position, position) - PLANET_RADIUS_M**2)
    discriminant = b * b - c
    hit = discriminant >= 0.0
    distance = -b - np.sqrt(np.maximum(discriminant, 0.0))
    surface = position[None, None, :] + distance[:, :, None] * directions
    surface /= np.linalg.norm(surface, axis=2, keepdims=True)

    latitude = np.degrees(np.arcsin(np.clip(surface[:, :, 1], -1.0, 1.0)))
    longitude = np.degrees(np.arctan2(surface[:, :, 2], surface[:, :, 0]))
    latitude[~hit] = np.nan
    longitude[~hit] = np.nan
    return latitude, longitude


def region_mask(
    latitude: np.ndarray,
    longitude: np.ndarray,
    bounds: tuple[float, float, float, float],
    half_width_deg: float,
) -> np.ndarray:
    """Pixels on the region bound, restricted to the span where it is an edge."""
    west, south, east, north = bounds
    latitude_span = (latitude >= south) & (latitude <= north)
    longitude_span = (longitude >= west) & (longitude <= east)
    mask = np.zeros(latitude.shape, dtype=bool)
    mask |= (np.abs(longitude - west) <= half_width_deg) & latitude_span
    mask |= (np.abs(longitude - east) <= half_width_deg) & latitude_span
    mask |= (np.abs(latitude - south) <= half_width_deg) & longitude_span
    mask |= (np.abs(latitude - north) <= half_width_deg) & longitude_span
    return mask


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pose", required=True, help="<lat>,<lon>,<altitude_m>")
    parser.add_argument(
        "--bounds", required=True, help="<west>,<south>,<east>,<north> degrees"
    )
    parser.add_argument("--size", default="1280x720", help="frame pixel size")
    parser.add_argument("--half-width-deg", type=float, default=0.004)
    parser.add_argument("--out", required=True, help="marker PNG path")
    args = parser.parse_args()

    pose = [float(value) for value in args.pose.split(",")]
    bounds = tuple(float(value) for value in args.bounds.split(","))
    if len(pose) != 3 or len(bounds) != 4:
        raise SystemExit("--pose needs three values and --bounds four")
    width, height = (int(value) for value in args.size.lower().split("x"))

    position, forward, right, up = frame_axes(*pose)
    latitude, longitude = geographic_grid(position, forward, right, up, width, height)
    mask = region_mask(latitude, longitude, bounds, args.half_width_deg)
    if not mask.any():
        raise SystemExit("the region bound is not inside this frame or pose")
    Image.fromarray((mask * 255).astype(np.uint8), mode="L").save(args.out)
    print(f"# {args.out}: {int(mask.sum())} marker pixels")
    return 0


if __name__ == "__main__":
    sys.exit(main())
