#!/usr/bin/env bash
# Build a signed, installable GameActivity APK.
#
# The Rust cdylib is built by Cargo with the NDK toolchain; Gradle only
# assembles the Java/GameActivity shell and packages the cdylib, libc++ and the
# mobile asset tree.
#
# Requirements (Termux on-device):
#   - aarch64-linux-android Rust target
#   - Android SDK (aarch64 repack) at $ANDROID_SDK_ROOT
#   - Android NDK at $ANDROID_NDK_ROOT
#   - Gradle 8.x on PATH, or set $GRADLE
#
# Usage:
#   scripts/build_android_apk.sh [debug|release]
#
# `debug` is the default: it is signed with the local debug keystore and can be
# installed directly. `release` produces an unsigned APK for external signing.
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$REPO_ROOT"

. scripts/android_env.sh

PACKAGE_ID=com.rendivs925.cosmicsystems
LIB_NAME=cosmic_systems_wasm
VARIANT=${1:-debug}

case "$VARIANT" in
    debug) GRADLE_TASK=assembleDebug ;;
    release) GRADLE_TASK=assembleRelease ;;
    *)
        echo "usage: $0 [debug|release]" >&2
        exit 2
        ;;
esac

find_gradle() {
    if [ -n "${GRADLE:-}" ]; then
        printf '%s' "$GRADLE"
        return
    fi
    if command -v gradle >/dev/null 2>&1; then
        command -v gradle
        return
    fi
    for candidate in "$HOME"/tools/gradle-*/bin/gradle; do
        if [ -x "$candidate" ]; then
            printf '%s' "$candidate"
            return
        fi
    done
    return 1
}

GRADLE_BIN=$(find_gradle) || {
    echo "error: Gradle 8.x not found. Install it or set GRADLE=/path/to/gradle." >&2
    exit 1
}

# AGP 8.1 targets JDK 17. Termux's default JDK 21 generates a Java 21
# module-info.class whose ModuleTarget jlink rejects as malformed.
PREFIX=${PREFIX:-/data/data/com.termux/files/usr}
if [ -d "$PREFIX/lib/jvm/java-17-openjdk" ]; then
    export JAVA_HOME="$PREFIX/lib/jvm/java-17-openjdk"
    export PATH="$JAVA_HOME/bin:$PATH"
fi

# AGP downloads an x86_64 aapt2 from Google Maven, which cannot run on Termux.
# Use the SDK repack's aarch64 build-tools instead.
AAPT2_OVERRIDE="$ANDROID_SDK_ROOT/build-tools/37.0.0/aapt2"
if [ ! -x "$AAPT2_OVERRIDE" ]; then
    echo "error: aarch64 aapt2 not found at $AAPT2_OVERRIDE" >&2
    exit 1
fi

# Regenerate the mobile asset tree only when it is missing or stale; copying
# 332 MB on a phone is slow and unnecessary on every build.
if [ "${FORCE_ASSETS:-0}" = 1 ] || [ ! -d assets-android ] \
    || [ -n "$(find assets -newer assets-android -print -quit 2>/dev/null)" ]; then
    echo "==> Preparing mobile assets"
    scripts/prepare_android_assets.sh
else
    echo "==> Reusing assets-android (FORCE_ASSETS=1 to rebuild)"
fi

echo "==> Building Rust cdylib (release)"
export ANDROID_PACKAGE_NAME=$PACKAGE_ID
android_cargo build --target aarch64-linux-android --release --lib

SO="target/aarch64-linux-android/release/lib${LIB_NAME}.so"
TOOLCHAIN="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin"

echo "==> Stripping cdylib"
"$TOOLCHAIN/llvm-strip" --strip-unneeded "$SO"

JNI_DIR=android/app/src/main/jniLibs/arm64-v8a
mkdir -p "$JNI_DIR"
cp "$SO" "$JNI_DIR/lib${LIB_NAME}.so"

# bevy_audio/oboe link the shared C++ runtime; the GameActivity glue is
# statically linked but the final cdylib still needs this library at runtime.
cp "$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" \
    "$JNI_DIR/"

echo "==> Writing android/local.properties"
printf 'sdk.dir=%s\n' "$ANDROID_SDK_ROOT" > android/local.properties

echo "==> Assembling $VARIANT APK"
(cd android && "$GRADLE_BIN" --no-daemon \
    -Pandroid.aapt2FromMavenOverride="$AAPT2_OVERRIDE" \
    "$GRADLE_TASK")

if [ "$VARIANT" = debug ]; then
    APK=android/app/build/outputs/apk/debug/app-debug.apk
else
    APK=android/app/build/outputs/apk/release/app-release-unsigned.apk
fi

echo "==> APK: $APK ($(du -h "$APK" | cut -f1))"
