## 1. Manifest

- [x] 1.1 One entry per required scene with file, evidence commit and status.
- [x] 1.2 Record capture recipes where the pose, clock and pitch are known.
- [x] 1.3 Seed the metrics from the committed frames.

## 2. Harness

- [x] 2.1 Recompute five robust statistics per frame.
- [x] 2.2 Fail on a missing scene, an unknown commit, or a metric outside
      tolerance.
- [x] 2.3 `--write` to refresh the baseline; `--capture` to re-render and compare.
- [x] 2.4 Tolerate the offscreen path's exit 134 only when the PNG exists.

## 3. Submission

- [ ] 3.1 Capture the missing orbit scene and any stale scene in the submission
      pass.
- [ ] 3.2 Run `scripts/visual_acceptance.py` green.
- [ ] 3.3 Update the five progress documents and archive this change.
