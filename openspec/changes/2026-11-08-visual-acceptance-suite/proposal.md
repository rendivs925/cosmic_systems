## Why

VL-03: the visual evidence registry listed frames by hand, so a regression was
only visible to a reviewer who opened the image, and the ten required scenes were
never checked by a command. The suite needs one committed manifest, one runnable
command, and a non-zero exit on a regression.

## What Changes

- Add `docs/progress/visual_matrix.json`: one entry per required scene with its
  frame, evidence commit, and capture recipe where one is known.
- Add `scripts/visual_acceptance.py`:
  - reads the manifest and recomputes five robust statistics per frame (mean
    luminance, bright and dark pixel shares, edge energy, quantised colour
    count);
  - fails when a scene is missing, its commit is not in history, or a statistic
    leaves a relative tolerance with an absolute floor;
  - `--write` refreshes the baseline metrics from the committed frames;
  - `--capture` re-renders each scene from its recorded recipe and compares the
    fresh frame.
- Seed the manifest with the nine scenes that have a current frame. `orbit`
  remains explicitly missing; the check fails until it is captured.

## Capabilities

### Modified Capabilities

- `scientific-validation`: adds a committed visual acceptance requirement.

## Impact

- `scripts/visual_acceptance.py` (new).
- `docs/progress/visual_matrix.json` (new).
- `docs/progress/VISUAL_EVIDENCE.md`: points at the harness.

## Limits

- The statistics are coarse on purpose. They catch a blank, black, mis-exposed,
  cloudless or geometry-collapsed frame; they do not judge whether a frame is
  beautiful. That remains a review decision.
- `orbit` has no frame yet, so the suite is red by design until the submission
  pass captures one.
