#!/usr/bin/env sh
# Canonical Android build environment. Source this from every helper script.
#
# Why this exists: on Termux the host triple is the Android target
# (`aarch64-linux-android`). Cargo includes the effective rustflags and linker
# configuration in every unit's fingerprint, so invoking Cargo with a different
# configuration (for example a bare `cargo build`, which lacks cargo-apk2's
# `CARGO_ENCODED_RUSTFLAGS`) invalidates the whole graph and forces a 20-40
# minute rebuild. Every entry point must therefore export byte-identical values.
#
# Usage:  . "$(dirname "$0")/android_env.sh"

# shellcheck shell=sh

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)

: "${ANDROID_NDK_ROOT:=$HOME/android/android-ndk-r29}"
: "${ANDROID_SDK_ROOT:=$HOME/android/android-sdk}"

export HOST=linux
export ANDROID_NDK_ROOT
export ANDROID_NDK_HOME="$ANDROID_NDK_ROOT"
export ANDROID_SDK_ROOT
export ANDROID_HOME="$ANDROID_SDK_ROOT"
export ANDROID_SDK_HOME="${ANDROID_SDK_HOME:-$HOME}"

TOOLCHAIN="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin"
EXTRA_LINK_LIB_DIR="$REPO_ROOT/target/cargo-apk-temp-extra-link-libraries"
mkdir -p "$EXTRA_LINK_LIB_DIR"

export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TOOLCHAIN/clang"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_AR="$TOOLCHAIN/llvm-ar"
# POSIX sh cannot export names containing '-', but cc-rs looks for exactly
# these. Run commands through `android_env_exec` so they reach the child.
android_env_exec() {
    env \
        "CC_aarch64-linux-android=$TOOLCHAIN/clang" \
        "CXX_aarch64-linux-android=$TOOLCHAIN/clang++" \
        "CFLAGS_aarch64-linux-android=--target=aarch64-linux-android24" \
        "CXXFLAGS_aarch64-linux-android=--target=aarch64-linux-android24" \
        "$@"
}

# cargo-apk2 sets CARGO_ENCODED_RUSTFLAGS itself and *appends* to any existing
# value, so it must never be exported here. Plain cargo invocations get the same
# single value through `android_cargo`.
ANDROID_RUSTFLAGS="$(printf '%s\037%s\037%s' \
    '-Clink-arg=--target=aarch64-linux-android24' \
    '-L' \
    "$EXTRA_LINK_LIB_DIR")"

android_cargo() {
    android_env_exec env "CARGO_ENCODED_RUSTFLAGS=$ANDROID_RUSTFLAGS" cargo "$@"
}

# Release-quality profiles require an explicit keystore. Sideload builds use the
# debug keystore; override for a real release key.
export CARGO_APK_ANDROID_KEYSTORE="${CARGO_APK_ANDROID_KEYSTORE:-$HOME/.android/debug.keystore}"
export CARGO_APK_ANDROID_KEYSTORE_PASSWORD="${CARGO_APK_ANDROID_KEYSTORE_PASSWORD:-android}"
