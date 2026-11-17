# Android builds from Termux

The simulator builds and runs on Android from the phone itself. There is no
second implementation: the same `build_app()` composition used by `cargo run`
is entered through Bevy's `#[bevy_main]` on Android.

## Backend choice

Bevy supports two Android activity backends:

| Backend | Needs | Verdict on Termux |
| --- | --- | --- |
| GameActivity (Bevy default) | Gradle + AGP + a JDK, or an AAR `classes.jar` | Gradle's daemon needs ~1.5–2 GB RAM next to `rustc`; not reliable on a phone |
| **NativeActivity** (`android-native-activity`) | only `aapt2`/`zipalign`/`apksigner` | **Used here.** No Gradle, no dex step, minSdk 24 |

The APK is assembled by [`cargo-apk2`](https://crates.io/crates/cargo-apk2),
the maintained `cargo-apk` successor. It reads the `[package.metadata.android]`
block in `Cargo.toml`, packages `assets/`, bundles the NDK's `libc++_shared.so`
and signs with `~/.android/debug.keystore`.

NativeActivity is the legacy backend: it has no GameTextInput/IME integration.
On-screen controls are ordinary Bevy UI, so this does not affect a simulator
with a touch HUD.

## Termux toolchain

```sh
pkg install rust openjdk-21 aapt2 aapt apksigner d8 android-tools 7zip
cargo install cargo-apk2

mkdir -p ~/android && cd ~/android
# aarch64-host NDK (static build; keeps the linux-x86_64 toolchain directory name
# that the Android tooling expects)
curl -LO https://github.com/lzhiyong/termux-ndk/releases/download/android-ndk/android-ndk-r29-aarch64.tar.xz
tar -xf android-ndk-r29-aarch64.tar.xz
# aarch64 Android SDK (build-tools + platforms)
curl -LO https://github.com/lzhiyong/termux-ndk/releases/download/android-sdk/android-sdk-aarch64.tar.xz
tar -xf android-sdk-aarch64.tar.xz
# static aarch64 zipalign, which Termux does not package
curl -LO https://github.com/lzhiyong/android-sdk-tools/releases/download/35.0.2/android-sdk-tools-static-aarch64.zip
```

`android-sdk-tools` ships `aapt2`/`zipalign` built for aarch64; put them in the
SDK's `build-tools/<version>/` directory (or symlink the Termux `aapt2`). Use a
`34.x` build-tools directory unless the packaged `zipalign` supports `-P 16`.

Environment (add to `~/.bashrc`):

```sh
export ANDROID_NDK_HOME="$HOME/android/android-ndk-r29"
export ANDROID_SDK_ROOT="$HOME/android/android-sdk"
export ANDROID_SDK_HOME="$HOME"
# cargo-apk2 picks the NDK's host toolchain directory from $HOST. Termux sets
# HOST=localhost, which maps to a non-existent `prebuilt/android`; "linux"
# selects the repack's `prebuilt/linux-x86_64`.
export HOST=linux
export PATH="$HOME/.cargo/bin:$PATH"
```

`cargo-apk2` requires, under `$ANDROID_SDK_ROOT`: `build-tools/<version>/` with
`aapt2`, `zipalign` and `apksigner`, and a matching
`platforms/android-<target_sdk_version>/android.jar`. This project targets SDK
35 because the aarch64 NDK repack's `build/core/platforms.mk` tops out at 35 and
the SDK repack ships `android-35`/`android-36`; the phone runs API 37 and a
35-target app installs normally. It requires an NDK with `source.properties`,
`build/core/platforms.mk` and `toolchains/llvm/prebuilt/linux-x86_64`.

The repacked NDK ships its shell wrappers with a `/usr/bin/env bash` shebang,
which does not exist on Termux. Fix them once after extracting:

```sh
cd ~/android/android-ndk-r29
termux-fix-shebang toolchains/llvm/prebuilt/linux-x86_64/bin/*
printf 'Pkg.Desc = Android NDK\nPkg.Revision = 29.0.14206865\n' > source.properties
```

`source.properties` is what identifies the NDK revision to `cargo-apk2`; some
repacks omit it.

Because Termux's host triple *is* the Android target, `cargo-apk2`'s linker
override also applies when Rust links its **host build scripts**, and the NDK's
plain `clang` (a symlink to `clang-21`) does not inject an API level, so those
links fail on missing `crt*.o`/`-lc`. Replace the two plain driver symlinks with
wrappers that behave like the API-level wrapper:

```sh
B=toolchains/llvm/prebuilt/linux-x86_64/bin
rm -f "$B/clang" "$B/clang++"
cat > "$B/clang" <<'EOF'
#!/data/data/com.termux/files/usr/bin/env bash
bin_dir="$(cd "$(dirname "$0")" && pwd)"
if [ "$1" != "-cc1" ]; then
    exec "$bin_dir/clang-21" --target=aarch64-linux-android24 "$@"
else
    exec "$bin_dir/clang-21" "$@"
fi
EOF
cat > "$B/clang++" <<'EOF'
#!/data/data/com.termux/files/usr/bin/env bash
bin_dir="$(cd "$(dirname "$0")" && pwd)"
if [ "$1" != "-cc1" ]; then
    exec "$bin_dir/clang-21" --target=aarch64-linux-android24 --driver-mode=g++ "$@"
else
    exec "$bin_dir/clang-21" "$@"
fi
EOF
chmod +x "$B/clang" "$B/clang++"
```

## Build and run

```sh
# `--lib` is required: the package also has a `cosmic_systems` bin, and only
# the `cdylib` library target can be packaged into the APK.
# `--profile android` is release-quality code with stripped symbols.
cargo apk2 build --lib --profile android   # APK under target/<profile>/apk/
cargo apk2 run --lib --profile android     # build, install and launch over adb
adb logcat | grep -E 'bevy|wgpu|RustStdoutStderr|cosmic'
```

Use exactly this command for every Android build. Do **not** alternate with a
bare `cargo check`/`cargo build`: on Termux the host triple is the Android
target, so those invocations produce different fingerprint metadata and force a
full recompile of the dependency graph (~30–40 minutes).

All helpers source `scripts/android_env.sh`, which exports byte-identical
`CARGO_ENCODED_RUSTFLAGS`, linker, `CC`, `AR` and NDK/SDK variables to the ones
`cargo-apk2` injects. Use:

```sh
sh scripts/build_android_apk.sh              # APK
sh scripts/cargo_android.sh check --lib      # type-check
sh scripts/cargo_android.sh test --lib foo   # on-device tests
```

The environment is wrapped by `scripts/build_android_apk.sh`, which sets
`HOST=linux` and the NDK/SDK roots and always uses the canonical command:

```sh
sh scripts/build_android_apk.sh            # extra args are passed to cargo apk2
```

Unused Bevy features are disabled in `Cargo.toml` (`bevy_gilrs`, `ktx2`,
`sysinfo_plugin`) because nothing in the simulator uses them; each
one would otherwise add crates to every full build.

`adb` comes from the `android-tools` package. With wireless debugging, pair and
connect to the phone itself:

```sh
adb pair 127.0.0.1:<pair-port>
adb connect 127.0.0.1:<debug-port>
```

Desktop and wasm behaviour is unchanged: the feature swap only affects the
Android backend, and `AssetPlugin` uses the repository path off Android.

## Mobile asset set

`assets/textures/planets` contains five 8192×4096 albedos (Earth, Moon, Mercury,
Mars, Venus). Together they decode to ~670 MB of RGBA and dominate APK size,
upload time and GPU memory on a phone.

`scripts/prepare_android_assets.sh` builds `assets-android/` as a hardlink tree of
`assets/` where only those five are replaced by 4096×2048 copies (ImageMagick).
The originals stay untouched for desktop builds. Point the Android metadata at it:

```toml
[package.metadata.android]
assets = "assets-android"
```

Re-run the script after adding or removing assets. The directory is gitignored.

## Diagnosing startup on the device

Android restricts `logcat` to the calling application's UID, so Termux cannot
read the simulator's log buffer. The Android entry point therefore appends to the
app's own media directory, which is writable without a runtime permission and
readable from Termux:

```sh
cat /sdcard/Android/media/com.rendivs925.cosmicsystems/cosmic_startup.log
```

If that directory is unavailable, the same lines are written to
`Download/cosmic_startup_<epoch>.log` through MediaStore. A panic hook records
the panic message before the process aborts.

The first line identifies the build; `staging:` lines report how many files came
out of the APK; `entry: app built, entering event loop` marks the hand-off to
Bevy. If the log is missing entirely, the library never reached `android_main`
(dlopen/linker failure) rather than a Rust panic.

## Shipped data on Android

APK assets are reachable only through Android's `AssetManager`, while the
runtime data loaders (`RocketCatalog`, part catalog, SPICE kernels, DEM and
terrain packages, climate/soil/surface fields) use synchronous `std::fs` paths.
`src/android.rs` stages `assets/configs` and `assets/large_files` into the app's
internal storage once per build and installs that directory as the asset root
through `domain::services::shipped_paths`. An APK entry `assets/configs/...` is
addressed through `AssetManager` as `configs/...`, so the staging code strips
that one prefix and reconstructs it under internal storage.

Android's `AAssetDir` enumerates **files, not subdirectories**, so walking the
packaged tree cannot discover `configs/rockets/...`. `build.rs` therefore emits
an explicit `SHIPPED_FILES` list of the files present under those two trees at
build time, and the entry point copies each by exact path. A marker records the
build and staging schema; it is written only after a complete copy, so a partial
or failed copy is retried on the next launch. Textures, models and
sounds are **not** staged: `AssetServer` reads them straight from the APK through
Bevy's `AndroidAssetReader`, whose asset paths are relative to the APK's
`assets/` directory.

Data that is gitignored (`assets/large_files/*`) is only packaged when present
in the working tree at build time. Provision it first:

```sh
sh scripts/provision_de440_kernels.sh
```

Without the measured DEM the Earth falls back to procedural terrain on Android
instead of panicking; without DE440 kernels the ephemeris still fails fast,
because scientific time has no procedural substitute.

## Presentation defaults

Android applies, only for that target:

- `WindowMode::BorderlessFullscreen`
- `WinitSettings::mobile()` (the event loop waits for input instead of spinning)
- `Msaa::Off` and `shadows_enabled = false` for every camera/light, avoiding
  driver paths known to panic or crash (bevy#8229, bevy#8214)

## Mobile controls

Android has no keyboard or mouse. The on-screen panel synthesises the same
`ButtonInput<KeyCode>` presses the desktop bindings already consume, so no mode
needs a second control implementation:

- **Solar**: one-finger drag rotates, pinch dollies, a short tap above the panel
  selects the body under the finger (reusing the desktop raycast). Buttons:
  `W/A/S/D`, `Up/Down`, `Next` (Tab), `Clear` (Esc), `Time-`/`Time+`.
- **Craft**: movement, yaw/pitch/roll, boost/brake, camera, reset, time warp.
- **Rocket**: `Stage` (Space), `Restart` (R), camera, HUD/log toggles, camera
  presets, time warp.

Touches that start inside the measured panel area are ignored by the camera
handler, so pressing a button never rotates the view.

The launch mode defaults to solar and is selectable with an intent extra:

```sh
am start -n com.rendivs925.cosmicsystems/android.app.NativeActivity --es mode rocket
am start -n com.rendivs925.cosmicsystems/android.app.NativeActivity --es mode craft
```

## Mobile UI system

`src/android/theme.rs` is the single source for the mobile interface:

- **Spacing**: 4 dp base scale (`SPACE_1..8`) used for panel padding, gaps,
  overlay padding and loading layout.
- **Type**: display 32, title 22, body 16, caption 13, using bundled
  `assets/fonts/Roboto-Regular.ttf` instead of Bevy's monospace `default_font`.
- **Touch**: 64×52 dp buttons (above the 48 dp minimum), 8 dp gaps, 12 dp panel
  padding plus a bottom safe-area allowance.
- **Colour**: dark surface, raised surface, accent, primary/secondary text with
  AA contrast.
- **Safe areas**: the activity uses `Theme.Material.NoActionBar.Fullscreen`, so
  the status bar is gone; the UI adds conservative top/bottom padding.

The activity declares `sensorLandscape` and a full `configChanges` list, so
rotation reflows the UI instead of recreating (and restarting) the simulation.

The loading screen has a title/subtitle hierarchy, a progress track and counter,
and an animated phase label driven by `Time<Real>` (virtual simulation time is
paused). It has a 30 s timeout, so a missing asset can never leave the app stuck.

## Current status

- Build/entry/assets plumbing: implemented and verified.
  - `cargo check --lib` (Android target) passes.
  - `cargo clippy --lib` is clean.
  - `cargo test --lib shipped_paths` passes on-device (the test harness links and
    runs natively because Termux's host triple is the Android target).
  - `cargo apk2 build --lib --profile android` produces a signed, optimized APK.
- **Loading phase (Android)**: simulation time is paused behind a loading screen
  until every texture in `assets/textures` is loaded, followed by 90 warmup
  frames so GPU uploads and shader pipelines finish before interaction. Progress
  and load/failure counts are written to the startup log.
- **Touch (Android)**: one-finger drag orbits and zooms the solar camera,
  two-finger pinch zooms, through the same `CameraController` state the desktop
  keyboard path uses.
- **On-screen readout (Android)**: FPS and frame time in the top-left corner, so
  liveness and cost are visible without adb.
- **Perf telemetry (Android)**: every 5 s the startup log records `fps`,
  `avg_ms`, `max_ms`, stall count (>33 ms) and RSS (`mem_mb`).
- Touch gameplay input for rocket/craft modes is still pending: the solar view
  is controllable, those modes are not.
- NativeActivity has no soft-keyboard support.
- Desktop validation (`cargo run`, `craft`, `rocket`) must run on a desktop or
  CI: Termux's host target *is* Android, so it cannot exercise the desktop cfg.
