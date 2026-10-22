#!/usr/bin/env python3
"""Prepare NASA POWER monthly climatology for an offline `.csclim` package.

Phase 6 originally targeted ERA5, but CDS credentials and `cdsapi` are not
available in this environment. NASA POWER publishes the same kind of global
monthly climatology through its public Analysis Ready Data (ARD) Zarr store,
with no authentication and a CC BY 4.0 licence:

* MERRA-2 provides 2 m temperature, surface pressure, 2 m relative humidity,
  and 2 m eastward/northward wind on a 0.5 x 0.625 degree grid.
* CERES SYN1deg provides cloud amount on a 1 degree grid.

The script downloads only the needed Zarr chunks, decodes the Blosc container
without adding a Python dependency, resamples the native geodetic grids onto
the planetocentric terrain-radial grid used by the simulator, and writes one
interleaved float32 raster. `src/bin/climate_bake.rs` converts that raster into
the versioned `.csclim` package.

No download happens at runtime. Chunks are cached under the ignored raw tree and
pinned by a deterministic aggregate SHA-256 of the fetched chunk files.
"""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import urllib.request
from pathlib import Path

import numpy as np

WGS84_E2 = 0.006_694_379_990_141_316_5
MERRA_BASE = (
    "https://nasa-power.s3.us-west-2.amazonaws.com/"
    "merra2/spatial/power_merra2_climatology_spatial_utc.zarr"
)
SYN1DEG_BASE = (
    "https://nasa-power.s3.us-west-2.amazonaws.com/"
    "syn1deg/spatial/power_syn1deg_climatology_spatial_utc.zarr"
)
RAW_ROOT = Path("assets/large_files/terrain/raw/climate")
DEFAULT_OUTPUT = Path(
    "assets/large_files/terrain/raw/climate/earth_merra2_climatology_v1.raw"
)
CHANNELS = (
    "temperature_c",
    "pressure_hpa",
    "relative_humidity_pct",
    "wind_east_mps",
    "wind_north_mps",
    "cloud_amount_pct",
)
MONTHS = 12

# Channel sources, in payload order. Cloud comes from SYN1deg; the rest come
# from MERRA-2. The output grid follows MERRA-2's native spatial resolution.
SOURCE_VARIABLES = (
    ("temperature_c", MERRA_BASE, "T2M", "merra2"),
    ("pressure_hpa", MERRA_BASE, "PS", "merra2"),
    ("relative_humidity_pct", MERRA_BASE, "RH2M", "merra2"),
    ("wind_east_mps", MERRA_BASE, "U2M", "merra2"),
    ("wind_north_mps", MERRA_BASE, "V2M", "merra2"),
    ("cloud_amount_pct", SYN1DEG_BASE, "CLOUD_AMT", "syn1deg"),
)

REFERENCE_SITES = {
    "starbase": (25.997, -97.155),
    "ksc": (28.572, -80.649),
    "kourou": (5.159, -52.643),
    "mahia": (-39.261, 177.865),
}

_BLOSC = None


def blosc() -> ctypes.CDLL:
    global _BLOSC
    if _BLOSC is None:
        _BLOSC = ctypes.CDLL("libblosc.so.1")
        _BLOSC.blosc_cbuffer_sizes.argtypes = [
            ctypes.c_void_p,
            ctypes.POINTER(ctypes.c_size_t),
            ctypes.POINTER(ctypes.c_size_t),
            ctypes.POINTER(ctypes.c_size_t),
        ]
        _BLOSC.blosc_decompress.argtypes = [
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_size_t,
        ]
        _BLOSC.blosc_decompress.restype = ctypes.c_int
    return _BLOSC


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fetch(url: str, path: Path, force: bool) -> bytes:
    if path.exists() and not force:
        return path.read_bytes()
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".part")
    print(f"downloading {url}")
    request = urllib.request.Request(
        url, headers={"User-Agent": "cosmic-systems-climate-ingest/1.0"}
    )
    with urllib.request.urlopen(request, timeout=300) as response:
        data = response.read()
    temporary.write_bytes(data)
    temporary.replace(path)
    return data


def blosc_decompress(data: bytes) -> bytes:
    library = blosc()
    nbytes = ctypes.c_size_t()
    cbytes = ctypes.c_size_t()
    blocksize = ctypes.c_size_t()
    library.blosc_cbuffer_sizes(
        data, ctypes.byref(nbytes), ctypes.byref(cbytes), ctypes.byref(blocksize)
    )
    if nbytes.value == 0:
        raise SystemExit("invalid Blosc chunk")
    output = ctypes.create_string_buffer(nbytes.value)
    written = library.blosc_decompress(data, output, nbytes.value)
    if written != nbytes.value:
        raise SystemExit(f"Blosc decompression failed ({written} of {nbytes.value})")
    return output.raw


def fetch_metadata(base: str, cache: Path, force: bool) -> dict:
    raw = fetch(f"{base}/.zmetadata", cache / "zmetadata.json", force)
    return json.loads(raw)["metadata"]


def fetch_array(
    base: str, variable: str, index: int, cache: Path, metadata: dict, force: bool
) -> np.ndarray:
    """Read one time slice of a 3D Zarr array without xarray/numcodecs."""
    array_meta = metadata[f"{variable}/.zarray"]
    shape = array_meta["shape"]
    chunks = array_meta["chunks"]
    if len(shape) not in (1, 3) or index < 0 or index >= shape[0]:
        raise SystemExit(f"unsupported Zarr shape for {variable}: {shape}")
    indices = [index // chunks[0]]
    indices.extend(0 for _ in range(len(shape) - 1))
    chunk_key = ".".join(str(value) for value in indices)
    # Every reviewed variable is chunked as one complete horizontal slice.
    chunk_path = cache / variable / chunk_key
    url = f"{base}/{variable}/{chunk_key}"
    raw = fetch(url, chunk_path, force)
    decoded = np.frombuffer(blosc_decompress(raw), dtype=array_meta["dtype"])
    expected = int(np.prod(chunks))
    if decoded.size != expected:
        raise SystemExit(
            f"{variable} chunk decoded to {decoded.size} values, expected {expected}"
        )
    decoded = decoded.reshape(tuple(chunks))
    if len(shape) == 3 and chunks[0] == 1:
        decoded = decoded[0]
    filters = array_meta.get("filters") or []
    for entry in filters:
        if entry.get("id") != "fixedscaleoffset":
            raise SystemExit(f"unsupported Zarr filter {entry.get('id')!r}")
        decoded = decoded / entry["scale"] + entry["offset"]
    fill = array_meta.get("fill_value")
    if fill is not None:
        decoded = np.where(np.isfinite(decoded), decoded, np.nan)
    return decoded


def fetch_coordinates(base: str, cache: Path, force: bool) -> tuple[np.ndarray, np.ndarray]:
    metadata = fetch_metadata(base, cache, force)
    lat = fetch_array(base, "lat", 0, cache, metadata, force).reshape(-1)
    lon = fetch_array(base, "lon", 0, cache, metadata, force).reshape(-1)
    return lat.astype(np.float64), lon.astype(np.float64)


def fetch_time_length(base: str, cache: Path, force: bool) -> int:
    metadata = fetch_metadata(base, cache, force)
    return int(metadata["time/.zarray"]["shape"][0])


def terrain_to_geodetic(latitude_deg: np.ndarray | float):
    values = np.radians(latitude_deg)
    return np.degrees(np.arctan(np.tan(values) / (1.0 - WGS84_E2)))


def resample_periodic(
    source: np.ndarray,
    source_lat: np.ndarray,
    source_lon: np.ndarray,
    target_lat: np.ndarray,
    target_lon: np.ndarray,
) -> np.ndarray:
    """Bilinear resample a north-first or south-first regular grid.

    Latitude is clamped at the poles; longitude is treated as periodic so the
    antimeridian has no visible seam in the global package.
    """
    if source.shape != (source_lat.size, source_lon.size):
        raise SystemExit(f"source shape {source.shape} does not match coordinates")
    lat_order = np.argsort(source_lat)
    lon_order = np.argsort(source_lon)
    lat = source_lat[lat_order]
    lon = source_lon[lon_order]
    data = source[np.ix_(lat_order, lon_order)]
    if not (lat.size >= 2 and lon.size >= 2):
        raise SystemExit("climate source grid is too small")

    target_lat = np.asarray(target_lat, dtype=np.float64)
    target_lon = np.asarray(target_lon, dtype=np.float64)
    y = np.clip(np.interp(target_lat, lat, np.arange(lat.size)), 0.0, lat.size - 1)
    lon_step = (lon[-1] - lon[0]) / (lon.size - 1)
    x = ((target_lon - lon[0]) / lon_step) % lon.size
    y0 = np.floor(y).astype(np.int64)
    x0 = np.floor(x).astype(np.int64)
    y1 = np.minimum(y0 + 1, lat.size - 1)
    x1 = (x0 + 1) % lon.size
    ty = (y - y0)[:, None]
    tx = (x - x0)[None, :]
    top = data[np.ix_(y0, x0)] * (1.0 - tx) + data[np.ix_(y0, x1)] * tx
    bottom = data[np.ix_(y1, x0)] * (1.0 - tx) + data[np.ix_(y1, x1)] * tx
    return top * (1.0 - ty) + bottom * ty


def prepare(args: argparse.Namespace) -> None:
    cache = RAW_ROOT / "chunks"
    merra_metadata = fetch_metadata(MERRA_BASE, cache / "merra2", args.force)
    syn_metadata = fetch_metadata(SYN1DEG_BASE, cache / "syn1deg", args.force)
    merra_lat = fetch_array(
        MERRA_BASE, "lat", 0, cache / "merra2", merra_metadata, args.force
    ).reshape(-1)
    merra_lon = fetch_array(
        MERRA_BASE, "lon", 0, cache / "merra2", merra_metadata, args.force
    ).reshape(-1)
    syn_lat, syn_lon = fetch_coordinates(SYN1DEG_BASE, cache / "syn1deg", args.force)
    if merra_lat.size < 2 or merra_lon.size < 2:
        raise SystemExit("MERRA-2 coordinates are invalid")

    # Output grid: MERRA-2's native half-degree latitude and 0.625 degree
    # longitude, expressed as north-first terrain-radial coordinates.
    output_lat = np.linspace(90.0, -90.0, merra_lat.size)
    output_lon = merra_lon.astype(np.float64)
    target_geodetic_lat = terrain_to_geodetic(output_lat)

    payload = np.empty((MONTHS, output_lat.size, output_lon.size, len(CHANNELS)), dtype="<f4")
    source_hashes: list[tuple[str, str]] = []

    for month in range(MONTHS):
        for channel, (name, base, variable, dataset) in enumerate(SOURCE_VARIABLES):
            metadata = merra_metadata if dataset == "merra2" else syn_metadata
            cache_root = cache / dataset
            raw = fetch(
                f"{base}/{variable}/{month}.0.0", cache_root / variable / f"{month}.0.0", args.force
            )
            source_hashes.append((f"{base}/{variable}/{month}.0.0", sha256_bytes(raw)))
            values = fetch_array(base, variable, month, cache_root, metadata, args.force)
            if name == "cloud_amount_pct":
                resampled = resample_periodic(
                    values, syn_lat, syn_lon, target_geodetic_lat, output_lon
                )
            else:
                resampled = resample_periodic(
                    values, merra_lat, merra_lon, target_geodetic_lat, output_lon
                )
            payload[month, :, :, channel] = resampled.astype("<f4")
        print(f"prepared month {month + 1:02d}/{MONTHS}")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    payload.tofile(args.output)
    aggregate = hashlib.sha256()
    for url, digest in sorted(source_hashes):
        aggregate.update(f"{digest}  {url}\n".encode())
    source_sha = aggregate.hexdigest()
    sidecar = args.output.with_suffix(".source.sha256")
    sidecar.write_text(f"{source_sha}  {args.output.name}\n")
    print(f"wrote {args.output} ({payload.nbytes} bytes)")
    print(f"source aggregate sha256 {source_sha}")
    print(
        "monthly channels: "
        + ", ".join(CHANNELS)
        + f"; grid {output_lat.size}x{output_lon.size}; months {MONTHS}"
    )


def verify(args: argparse.Namespace) -> None:
    """Compare a prepared raster with the POWER point climatology API."""
    payload = np.fromfile(args.output, dtype="<f4")
    expected = MONTHS * 361 * 576 * len(CHANNELS)
    if payload.size != expected:
        raise SystemExit(
            f"{args.output} has {payload.size} floats, expected {expected}; rerun ingest"
        )
    payload = payload.reshape(MONTHS, 361, 576, len(CHANNELS))
    output_lat = np.linspace(90.0, -90.0, 361)
    output_lon = -180.0 + np.arange(576) * 0.625
    parameters = {
        "temperature_c": "T2M",
        "pressure_hpa": "PS",
        "relative_humidity_pct": "RH2M",
        "wind_east_mps": "U2M",
        "wind_north_mps": "V2M",
        "cloud_amount_pct": "CLOUD_AMT",
    }
    failures = 0
    tolerances = (1.5, 5.0, 10.0, 1.5, 1.5, 10.0)
    for site, (latitude, longitude) in REFERENCE_SITES.items():
        url = (
            "https://power.larc.nasa.gov/api/temporal/climatology/point"
            f"?parameters={','.join(parameters.values())}&community=RE"
            f"&latitude={latitude}&longitude={longitude}&format=JSON"
        )
        with urllib.request.urlopen(url, timeout=60) as response:
            reference = json.loads(response.read())["properties"]["parameter"]
        lat_index = int(np.argmin(np.abs(output_lat - latitude)))
        lon_index = int(np.argmin(np.abs(output_lon - longitude)))
        for channel, power_name in enumerate(parameters.values()):
            for month in range(12):
                model = float(payload[month, lat_index, lon_index, channel])
                source = float(reference[power_name][
                    ("JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC")[month]
                ])
                if power_name == "PS":
                    # The point API reports surface pressure in kPa; the ARD
                    # Zarr attribute and the package use hPa.
                    source *= 10.0
                tolerance = tolerances[channel]
                if abs(model - source) > tolerance:
                    failures += 1
                    if failures <= 12:
                        print(
                            f"mismatch {site} {CHANNELS[channel]} month {month + 1}: "
                            f"{model:.2f} vs {source:.2f}"
                        )
    if failures:
        raise SystemExit(f"{failures} climatology samples exceed tolerance")
    print(f"verified {len(REFERENCE_SITES)} sites against the POWER point API")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help=f"interleaved float32 output raster (default {DEFAULT_OUTPUT})",
    )
    parser.add_argument("--force", action="store_true", help="redownload cached chunks")
    parser.add_argument(
        "--verify",
        action="store_true",
        help="compare an existing raster with the POWER point API",
    )
    args = parser.parse_args()
    if args.verify:
        verify(args)
    else:
        prepare(args)


if __name__ == "__main__":
    main()
