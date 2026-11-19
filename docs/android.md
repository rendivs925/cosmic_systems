# Android builds from Termux

The simulator builds and runs on Android from the phone itself. There is no
second implementation: the same `build_app()` composition used by `cargo run`
is entered through Bevy's `#[bevy_main]` on Android.

## Backend: GameActivity

The Android package uses **GameActivity** (`android-game-activity`), Bevy's
default and tested Android backend, rather than the legacy `NativeActivity`.

| Backend | Java shell | Input methods | Status |
| --- | --- | --- | --- |
| `NativeActivity` | none | no IME / soft keyboard | legacy |
| **GameActivity** | small `MainActivity` + Gradle | GameTextInput / IME | **used here** |

GameActivity is based on `AppCompatActivity`, so it integrates with modern
Android lifecycle and input handling. The Rust side still enters through
`#[bevy_main]`; only the Cargo feature and the packaging path differ.

`android-activity` 0.6.1 compiles its GameActivity glue against
`androidx.games:games-activity:4.4.0`. That exact version is required: a
different AAR makes the Java class and native glue disagree at launch.

## Build

```sh
# From the repository root:
bash scripts/build_android_apk.sh            # debug APK, signed, installable
bash scripts/build_android_apk.sh release    # unsigned release APK
```

The script:

1. regenerates `assets-android/` when it is missing or older than `assets/`;
2. builds `libcosmic_systems_wasm.so` with Cargo and the NDK toolchain;
3. strips the cdylib and copies it plus `libc++_shared.so` into
   `android/app/src/main/jniLibs/arm64-v8a/`;
4. runs Gradle `assembleDebug` / `assembleRelease`.

The debug APK lands at
`android/app/build/outputs/apk/debug/app-debug.apk`. It is signed with
`~/.android/debug.keystore`, the same key the previous `cargo-apk2` pipeline
used, so an existing installation updates in place. If the key changed,
uninstall the old app first.

All helpers source `scripts/android_env.sh`, which exports the NDK linker,
`CC`, `AR`, `CARGO_ENCODED_RUSTFLAGS` and the SDK/NDK roots. Use
`scripts/android_env.sh` for any manual Cargo invocation so the dependency
graph is not invalidated:

```sh
. scripts/android_env.sh
android_cargo check --target aarch64-linux-android --lib
```

## Termux toolchain

```sh
pkg install rust openjdk-17 aapt2 apksigner d8 android-tools 7zip unzip
```

Use **JDK 17**, not 21: AGP 8.1 targets 17, and JDK 21's `jlink` rejects the
`ModuleTarget` attribute Android's `core-for-system-modules.jar` produces
(`ModuleTarget is malformed: platformString missing delimiter: android`). The
build script selects `java-17-openjdk` automatically when it is installed.

Gradle is not packaged for Termux; unpack it under `~/tools/` and either put
`gradle` on `PATH` or set `GRADLE`:

```sh
mkdir -p ~/tools && cd ~/tools
curl -LO https://services.gradle.org/distributions/gradle-8.5-bin.zip
unzip -q gradle-8.5-bin.zip
export PATH="$HOME/tools/gradle-8.5/bin:$PATH"
```

The SDK and NDK are the aarch64 repacks:

```sh
mkdir -p ~/android && cd ~/android
curl -LO https://github.com/lzhiyong/termux-ndk/releases/download/android-ndk/android-ndk-r29-aarch64.tar.xz
tar -xf android-ndk-r29-aarch64.tar.xz
curl -LO https://github.com/lzhiyong/termux-ndk/releases/download/android-sdk/android-sdk-aarch64.tar.xz
tar -xf android-sdk-aarch64.tar.xz
```

Environment:

```sh
export ANDROID_NDK_HOME="$HOME/android/android-ndk-r29"
export ANDROID_SDK_ROOT="$HOME/android/android-sdk"
export ANDROID_SDK_HOME="$HOME"
# The NDK repack names its host toolchain directory `linux-x86_64` even though
# the binaries are aarch64. "linux" selects it.
export HOST=linux
export PATH="$HOME/.cargo/bin:$PATH"
```

### SDK packages

The repack ships `platforms/android-35`, `platforms/android-36` and
`build-tools/37.0.0` (aarch64). Gradle targets API 34, so install its platform
resources; they are architecture-independent:

```sh
SDK="$HOME/android/android-sdk"
"$SDK/cmdline-tools/bin/sdkmanager" --sdk_root="$SDK" \
    "platforms;android-34" "build-tools;34.0.0"
```

**Do not use `build-tools/34.0.0`.** Google's copy is an x86_64 Linux binary
and cannot execute on Termux. `android/app/build.gradle` therefore pins
`buildToolsVersion '37.0.0'`, the repack's aarch64 build. The build script also
passes

```sh
-Pandroid.aapt2FromMavenOverride="$ANDROID_SDK_ROOT/build-tools/37.0.0/aapt2"
```

because AGP otherwise downloads its own x86_64 `aapt2` from Google Maven and
every resource task fails with a daemon startup error.

Accept the licenses once if Gradle reports they are missing:

```sh
yes | "$SDK/cmdline-tools/bin/sdkmanager" --sdk_root="$SDK" --licenses
```

### NDK shell wrappers

The repacked NDK ships shell wrappers with a `/usr/bin/env bash` shebang, which
does not exist on Termux. Fix them once after extracting:

```sh
cd ~/android/android-ndk-r29
termux-fix-shebang toolchains/llvm/prebuilt/linux-x86_64/bin/*
printf 'Pkg.Desc = Android NDK\nPkg.Revision = 29.0.14206865\n' > source.properties
```

The NDK's `clang`/`clang++` symlinks do not inject an API level. `android_env.sh`
passes `--target=aarch64-linux-android24` explicitly, so the wrappers do not
need to be replaced.

## Gradle project layout

```
android/
  settings.gradle
  build.gradle                  AGP 8.1.4
  gradle.properties
  app/build.gradle              GameActivity + AppCompat, assets, jniLibs
  app/src/main/AndroidManifest.xml
  app/src/main/java/.../MainActivity.java
  app/src/main/res/values/styles.xml
```

Gradle owns the manifest now; the `[package.metadata.android]` block that
`cargo-apk2` used has been removed. `local.properties` (written by the build
script) points Gradle at the SDK and is gitignored, as are the generated
`jniLibs/` and `build/` trees.

The manifest keeps:

- `android.app.lib_name = cosmic_systems_wasm`, which GameActivity reads to load
  the cdylib;
- `sensorLandscape` plus a full `configChanges` list, so rotation reflows the UI
  instead of recreating the activity and restarting the simulation;
- an AppCompat-derived fullscreen theme (`Theme.CosmicSystems`).

`libc++_shared.so` must be packaged: `bevy_audio`/`oboe` link the shared C++
runtime. The build script copies it from the NDK next to the cdylib.

## Mobile asset set

`assets/textures/planets` contains five 8192×4096 albedos (Earth, Moon, Mercury,
Mars, Venus). Together they decode to ~670 MB of RGBA and dominate APK size,
upload time and GPU memory on a phone.

`scripts/prepare_android_assets.sh` builds `assets-android/` as a copy of
`assets/` where only those five are replaced by 4096×2048 copies (ImageMagick).
The originals stay untouched for desktop builds. `android/app/build.gradle`
references that directory directly as the APK asset root, so no second copy is
made.

Re-run the script (or `FORCE_ASSETS=1 bash scripts/build_android_apk.sh`) after
adding or removing assets. The directory is gitignored.

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

`build.rs` reads the package id from `ANDROID_PACKAGE_NAME` (set by the build
script); `CARGO_APK2_PACKAGE` remains as a fallback for the legacy pipeline.

### Missing log after a reinstall

`Android/media/<package>` is owned by the installing UID. If the app is
uninstalled and reinstalled, Android can leave the old directory behind owned by
the previous UID; the new process then cannot write there and startup logging
silently falls back to MediaStore. Remove it once from Termux:

```sh
rm -rf /sdcard/Android/media/com.rendivs925.cosmicsystems
```

The next launch recreates it under the current UID.

## Shipped data on Android

APK assets are reachable only through Android's `AssetManager`, while the
runtime data loaders (`RocketCatalog`, part catalog, SPICE kernels, DEM and
terrain packages, climate/soil/surface fields) use synchronous `std::fs` paths.
`src/android/mod.rs` stages `assets/configs` and `assets/large_files` into the
app's internal storage once per build and installs that directory as the asset
root through `domain::services::shipped_paths`. An APK entry `assets/configs/...`
is addressed through `AssetManager` as `configs/...`, so the staging code strips
that one prefix and reconstructs it under internal storage.

Android's `AAssetDir` enumerates **files, not subdirectories**, so walking the
packaged tree cannot discover `configs/rockets/...`. `build.rs` therefore emits
an explicit `SHIPPED_FILES` list of the files present under those two trees at
build time, and the entry point copies each by exact path. A marker records the
build and staging schema; it is written only after a complete copy, so a partial
or failed copy is retried on the next launch. Textures, models and sounds are
**not** staged: `AssetServer` reads them straight from the APK through Bevy's
`AndroidAssetReader`, whose asset paths are relative to the APK's `assets/`
directory.

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
- an 80 MB terrain budget instead of the desktop 160 MB

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

UI hit-testing and touch input are entirely Bevy's built-in systems. Application
code only **reads** `Interaction`; it never writes it. The camera's UI hit test
reuses Bevy's `ComputedNode::contains_point` with the same logical-to-physical
coordinate conversion as `ui_focus_system`, so a button press is never mistaken
for a camera drag on a high-DPI screen.

The launch mode defaults to solar and is selectable with an intent extra:

```sh
am start -n com.rendivs925.cosmicsystems/.MainActivity --es mode rocket
am start -n com.rendivs925.cosmicsystems/.MainActivity --es mode craft
```

## Mobile UI system

`src/android/theme.rs` is the single source for the mobile interface:

- **Spacing**: 4 dp base scale (`SPACE_1..8`) used for panel padding, gaps,
  overlay padding and loading layout.
- **Type**: display 36, title 22, body 16, caption 13, using bundled
  `assets/fonts/Roboto-Regular.ttf` instead of Bevy's monospace `default_font`.
- **Touch**: 64×52 dp buttons (above the 48 dp minimum), 8 dp gaps, 12 dp panel
  padding plus a bottom safe-area allowance. Primary actions (Stage) are
  120×64 dp.
- **Colour**: space-themed palette with translucent surfaces (`SURFACE_PANEL`
  0.72 alpha), accent blue (`ACCENT`), warm accent (`ACCENT_WARM`), and layered
  text hierarchy.
- **Safe areas**: the activity is fullscreen, so the UI adds conservative
  top/bottom padding.

The activity declares `sensorLandscape` and a full `configChanges` list, so
rotation reflows the UI instead of recreating (and restarting) the simulation.

The loading screen has a branded wordmark, animated pulse indicator, and
cycling contextual tips. On completion it fades out smoothly (0.45 s) into the
scene.

## Current status

- Build/entry/assets plumbing: implemented.
- **GameActivity**: native cdylib builds with the NDK linker and packages with
  Gradle; `libc++_shared.so` is shipped.
- **Touch (Android)**: one-finger drag orbits and zooms the solar camera,
  two-finger pinch zooms, through the same `CameraController` state the desktop
  keyboard path uses.
- **Per-mode panel (Android)**: solar, craft and rocket bindings are synthesised
  into `ButtonInput<KeyCode>`, so flight logic is unchanged.
- **Perf telemetry (Android)**: every 5 s the startup log records `fps`,
  `avg_ms`, `max_ms`, stall count (>33 ms), RSS (`mem_mb`), window size and the
  measured control-panel height.
- **Desktop validation (`cargo run`, `craft`, `rocket`) must run on a desktop or
  CI**: Termux's host target *is* Android, so it cannot exercise the desktop cfg.
