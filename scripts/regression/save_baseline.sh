#!/usr/bin/env bash
# Records the canonical deterministic-regression baseline(s) from the current
# code so they can be committed as the CI gate fixtures.
#
# Usage:
#   scripts/regression/save_baseline.sh              # record the default (ascent) baseline
#   REGRESSION_TEST_FILTER=descend scripts/regression/save_baseline.sh
#
# A baseline must be re-recorded deliberately (never silently): the harness
# fills a signed-off audit trail and the fixture is only rewritten when this
# script sets REGRESSION_RECORD=1. Review the git diff before committing.
set -euo pipefail

cd "$(dirname "$0")/../.."

FILTER="${REGRESSION_TEST_FILTER:-determinism_regression_tests::ascent_matches_committed_baseline_within_tolerances}"

echo "Recording test filter:             ${FILTER}"

REGRESSION_RECORD=1 cargo test "${FILTER}" -- --nocapture

echo "Baseline(s) written. Verify with:"
echo "  git status tests/baselines/"
echo "  git diff --stat tests/baselines/"
