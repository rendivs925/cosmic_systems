## Context

Frames are produced by the offscreen capture path with a fixed pose, clock and
frame count, and committed under `docs/progress/evidence/`. The registry is a
Markdown table. Nothing recomputes a statistic from those bytes.

## Goals

- One command answers "is the required visual matrix still present and
  plausible".
- A regression exits non-zero.
- The baseline is committed and self-describing.

## Non-Goals

- Judging aesthetics or matching exact pixels. The renderer is deterministic for
  a fixed clock, but a GPU or driver change can move a few counts.
- Replacing the reviewer.

## Decisions

### Five coarse statistics

Calling two frames identical would be brittle. A blank frame has near-zero edge
energy and one colour; a lost deck has a low bright share; a bad exposure moves
the mean. Mean luminance, bright share, dark share, edge energy and quantised
colour count separate those failure classes while tolerating a few counts of
render noise.

### Relative tolerance with a floor

Each metric uses a relative tolerance (10-25 %) with an absolute floor, so a
metric that is near zero cannot demand an impossible relative match. The policy
lives in the script, not in the manifest, so a change to the tolerances is one
reviewable diff.

### Commit ancestry is part of the check

A frame without a commit is not evidence. The check runs
`git merge-base --is-ancestor <commit> HEAD`, so a manifest entry cannot point at
a commit that was rewritten out of history.

### Re-capture is opt-in

`--check` reads the committed bytes; `--capture` rebuilds from the recorded
recipe. The offscreen path can exit 134 after writing the PNG, so a non-zero
exit is tolerated only when the frame exists. This is the same tolerance the rest
of the capture workflow uses.

## Risks

- A large intentional visual improvement will trip the tolerances; `--write`
  refreshes the baseline and the diff shows exactly which statistic moved.
- A missing recipe makes `--capture` fail for that scene; `--check` still works
  from the committed frame. Recipes are filled as each scene is re-captured.

## Migration

None. The Markdown registry stays as the human index; the manifest is the
machine-checked one.
