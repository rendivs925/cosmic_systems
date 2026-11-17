#!/usr/bin/env sh
# Run Cargo for the Android target with the canonical environment.
#
# Use this instead of a bare `cargo check`/`cargo build`: on Termux host ==
# target, so different flags invalidate the entire dependency graph.
#
# Examples:
#   sh scripts/cargo_android.sh check --lib
#   sh scripts/cargo_android.sh test --lib shipped_paths
set -eu

cd "$(dirname "$0")/.."
. scripts/android_env.sh

android_cargo --target aarch64-linux-android --profile android "$@"
