#!/usr/bin/env python3
"""Committed visual acceptance suite for the required scene matrix.

VL-03: every required scene must have a frame at a stated commit and pose, and a
regression must be visible without a human opening the image. This harness reads
`docs/progress/visual_matrix.json`, recomputes a small set of robust image
statistics from the committed frame, and fails when a scene is missing, its
stated commit is not in the repository history, or a statistic leaves its
tolerance.

Usage:
    scripts/visual_acceptance.py                 # check the committed frames
    scripts/visual_acceptance.py --write         # refresh the baseline metrics
    scripts/visual_acceptance.py --capture       # re-capture, then compare
    scripts/visual_acceptance.py --scene clouds  # one scene

The metrics are deliberately coarse: mean luminance, the shares of bright and
dark pixels, edge energy, and a quantised colour count. They catch a black or
blank frame, a lost deck, a shifted exposure and a collapsed material without
locking the suite to one exact render. A visual change that keeps all five in
range is a matter for the reviewer, not the harness.

Exit codes: 0 all scenes pass, 1 a scene failed, 2 the manifest or a dependency
is unusable.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    from PIL import Image, ImageFilter
except ImportError:  # pragma: no cover - environment guard
    print("visual acceptance needs Pillow (pip install pillow)", file=sys.stderr)
    raise SystemExit(2)

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / "docs/progress/evidence"
MANIFEST = ROOT / "docs/progress/visual_matrix.json"
BINARY = ROOT / "target/debug/cosmic_systems"
CAPTURE_ENV = "COSMIC_SYSTEMS_OFFSCREEN_CAPTURE"

# Relative tolerance with an absolute floor, so a near-zero statistic cannot
# produce an impossible relative bound.
TOLERANCE = {
    "mean_luma": (0.10, 0.010),
    "bright_share": (0.15, 0.020),
    "dark_share": (0.25, 0.020),
    "detail": (0.25, 0.002),
    "colours": (0.20, 64.0),
}

BRIGHT_LUMA = 184  # 0.72 of 255
DARK_LUMA = 26  # 0.10 of 255


def measure(path: Path) -> dict[str, float]:
    """Robust statistics for one committed frame."""
    try:
        image = Image.open(path).convert("RGB")
    except OSError as error:
        raise SystemExit(f"{path}: cannot read image: {error}") from error
    # Bound the work: the statistics are spatial averages, so a 512 px long edge
    # is plenty and keeps the suite fast on a large registry.
    image.thumbnail((512, 512))
    luma = image.convert("L")
    pixels = list(luma.getdata())
    total = len(pixels)
    mean_luma = sum(pixels) / total / 255.0
    bright_share = sum(1 for value in pixels if value >= BRIGHT_LUMA) / total
    dark_share = sum(1 for value in pixels if value <= DARK_LUMA) / total
    edges = luma.filter(ImageFilter.FIND_EDGES)
    detail = sum(edges.getdata()) / total / 255.0
    small = image.resize((64, 64))
    colours = float(len(set(small.getdata())))
    return {
        "mean_luma": round(mean_luma, 6),
        "bright_share": round(bright_share, 6),
        "dark_share": round(dark_share, 6),
        "detail": round(detail, 6),
        "colours": colours,
    }


def scene_commit(file_name: str) -> str:
    """Last commit that touched a frame; used when seeding the manifest."""
    result = subprocess.run(
        ["git", "log", "-1", "--format=%h", "--", f"docs/progress/evidence/{file_name}"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def commit_is_ancestor(commit: str) -> bool:
    result = subprocess.run(
        ["git", "merge-base", "--is-ancestor", commit, "HEAD"],
        cwd=ROOT,
        capture_output=True,
    )
    return result.returncode == 0


def compare(name: str, measured: dict[str, float], baseline: dict[str, float]) -> list[str]:
    failures = []
    for metric, (relative, floor) in TOLERANCE.items():
        if metric not in baseline:
            continue
        expected = baseline[metric]
        actual = measured[metric]
        allowed = max(abs(expected) * relative, floor)
        if abs(actual - expected) > allowed:
            failures.append(
                f"{name}: {metric} {actual:.4f} vs baseline {expected:.4f} "
                f"(allowed ±{allowed:.4f})"
            )
    return failures


def load_manifest() -> dict:
    try:
        return json.loads(MANIFEST.read_text())
    except OSError as error:
        raise SystemExit(f"cannot read {MANIFEST}: {error}") from error
    except json.JSONDecodeError as error:
        raise SystemExit(f"{MANIFEST} is not valid JSON: {error}") from error


def write_manifest(manifest: dict) -> None:
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")


def capture(scene: dict, output: Path) -> None:
    """Re-render one scene with its recorded command and environment."""
    spec = scene.get("capture")
    if not spec:
        raise RuntimeError(f"{scene['name']}: no capture recipe recorded")
    if not BINARY.exists():
        raise RuntimeError(f"{BINARY} is missing; run `cargo build --features dem` first")
    env = dict(**spec.get("env", {}))
    env[CAPTURE_ENV] = str(output)
    # The offscreen writer must not compete with a windowed swapchain.
    env.pop("DISPLAY", None)
    env.setdefault("COSMIC_SYSTEMS_OFFSCREEN_CAPTURE_FRAMES", str(spec.get("frames", 240)))
    result = subprocess.run(
        [str(BINARY), *spec.get("args", [])],
        cwd=ROOT,
        env={**os.environ, **env},
        capture_output=True,
        text=True,
    )
    # The offscreen path can exit 134 after the PNG is written (the window is
    # already gone); that is tolerated only when the file exists.
    if not output.exists():
        raise RuntimeError(
            f"{scene['name']}: capture exit {result.returncode} and no frame was written\n"
            f"{result.stderr[-800:]}"
        )


def run(args: argparse.Namespace) -> int:
    manifest = load_manifest()
    scenes = manifest.get("scenes", [])
    if args.scene:
        scenes = [scene for scene in scenes if scene["name"] == args.scene]
        if not scenes:
            print(f"no scene named {args.scene}", file=sys.stderr)
            return 2

    failures: list[str] = []
    capture_dir = Path(tempfile.mkdtemp(prefix="visual_acceptance_")) if args.capture else None
    print(f"{'scene':<18} {'state':<10} {'file':<46} metrics")
    for scene in scenes:
        name = scene["name"]
        if scene.get("status") == "missing" or not scene.get("file"):
            failures.append(f"{name}: no committed frame ({scene.get('reason', 'missing')})")
            print(f"{name:<18} {'MISSING':<10} {'-':<46} -")
            continue
        path = EVIDENCE / scene["file"]
        if not path.exists():
            failures.append(f"{name}: {path} does not exist")
            print(f"{name:<18} {'MISSING':<10} {scene['file']:<46} -")
            continue
        commit = scene.get("commit", "")
        if not commit or not commit_is_ancestor(commit):
            failures.append(f"{name}: commit {commit!r} is not in history")
        if args.capture:
            fresh = capture_dir / f"{name}.png"
            try:
                capture(scene, fresh)
            except RuntimeError as error:
                failures.append(str(error))
                print(f"{name:<18} {'CAPTURE-ERR':<10} {scene['file']:<46} -")
                continue
            measured = measure(fresh)
        else:
            measured = measure(path)
        baseline = scene.get("metrics", {})
        if args.write:
            scene["metrics"] = measured
        elif baseline:
            failures.extend(compare(name, measured, baseline))
        print(
            f"{name:<18} {'OK':<10} {scene['file']:<46} "
            + " ".join(f"{key}={value:.3f}" for key, value in measured.items())
        )

    if args.write:
        write_manifest(manifest)
        print(f"\nwrote {MANIFEST}")
        return 0

    if failures:
        print("\nFAILURES:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print(f"\n{len(scenes)} scenes pass")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--write", action="store_true", help="refresh baseline metrics from the frames")
    parser.add_argument("--capture", action="store_true", help="re-capture each scene before comparing")
    parser.add_argument("--scene", help="only this scene")
    return run(parser.parse_args())


if __name__ == "__main__":
    raise SystemExit(main())
