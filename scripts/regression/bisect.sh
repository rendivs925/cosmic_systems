#!/usr/bin/env bash
# Git-bisect a determinism regression: finds the first commit whose simulated
# flight diverges from a REFERENCE baseline.
#
# Usage:
#   scripts/regression/bisect.sh <good-ref> <bad-ref>
#   scripts/regression/bisect.sh good-ref bad-ref <baseline-test-filter>
#
# The reference baseline is taken from `good-ref` and pinned via
# REGRESSION_BASELINE_DIR so every commit under test is compared against the
# SAME fixture (not a self-bootstrapped one). This is what makes the
# divergence attributable to a single commit.
set -euo pipefail

cd "$(dirname "$0")/../.."

GOOD="${1:?usage: bisect.sh <good-ref> <bad-ref> [test-filter]}"
BAD="${2:?usage: bisect.sh <good-ref> <bad-ref> [test-filter]}"
FILTER="${3:-determinism_regression_tests::ascent_matches_committed_baseline_within_tolerances}"
FEATURES="${FEATURES:-dem}"

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT
BASELINE_DIR="$WORKDIR/baselines"
REF_COMMIT="$(git rev-list -n 1 "$GOOD")"

echo "Reference baseline from good commit: $REF_COMMIT ($GOOD)"
echo "Baseline fixtures pinned to:         $BASELINE_DIR"
echo "Test filter:                          $FILTER"
echo "Features:                             $FEATURES"

# Harvest the reference baseline from the good commit.
git worktree add --detach "$WORKDIR/ref" "$REF_COMMIT" >/dev/null 2>&1
COMPARE_COMMAND="cargo test --features ${FEATURES} ${FILTER} -- --nocapture"
(
  cd "$WORKDIR/ref" \
    && REGRESSION_BASELINE_DIR="$BASELINE_DIR" REGRESSION_RECORD=1 $COMPARE_COMMAND >/dev/null 2>&1 \
    && echo "Recorded reference baseline at $BASELINE_DIR"
)

# Turn the bisect result into a pass(0)/fail(1) verdict for `git bisect run`.
echo "Starting bisect from good=$GOOD bad=$BAD"
export REGRESSION_BASELINE_DIR="$BASELINE_DIR"
git bisect start "$BAD" "$GOOD"
if git bisect run bash -c "$COMPARE_COMMAND >/dev/null 2>&1"; then
  echo "No deterministic divergence found between $GOOD and $BAD."
  git bisect reset >/dev/null 2>&1
  exit 0
fi

echo "First bad commit:"
git bisect log | tail -20
git bisect reset >/dev/null 2>&1
