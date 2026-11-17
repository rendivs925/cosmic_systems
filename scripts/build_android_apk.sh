#!/usr/bin/env sh
# Build the on-device Android APK using the canonical environment.
#
# Usage: sh scripts/build_android_apk.sh [extra cargo apk2 arguments]
set -eu

cd "$(dirname "$0")/.."
. scripts/android_env.sh

# cargo-apk2 appends its own rustflags; a preset value would be duplicated.
unset CARGO_ENCODED_RUSTFLAGS

if [ ! -d assets-android ]; then
    sh scripts/prepare_android_assets.sh
fi

# Drop packaging intermediates: a stale classes.dex in the unaligned APK makes
# the final `aapt add` fail.
rm -rf target/android/apk

android_env_exec cargo apk2 build --lib --profile android "$@"
