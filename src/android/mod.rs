//! Android entry point and APK asset staging.
//!
//! Bevy's Android backend requires the application to enter through
//! `#[bevy_main]`, which installs the `AndroidApp` handle before user code runs.
//! APK assets are reachable only through Android's `AssetManager`, but the
//! simulator's data loaders are synchronous filesystem paths. This module
//! bridges the two once at startup by copying the synchronously-read shipped
//! data out of the APK and installing it as the asset root.

pub(crate) mod controls;
pub(crate) mod diagnostics;
mod presentation;

use bevy::prelude::bevy_main;
use std::ffi::CString;
use std::fs;
use std::io;
use std::path::Path;

use crate::application::app_builder::build_app;
use crate::application::modes::LaunchOptions;
use crate::domain::services::shipped_paths;
use crate::generated_assets::SHIPPED_FILES;

/// Marker written once per build so re-launches skip the copy.
const STAGE_MARKER: &str = "assets/.android-staged";
/// Bumped when the staging algorithm changes, so old markers cannot hide a
/// tree that was staged by a broken version.
const STAGE_SCHEMA: u32 = 2;

#[bevy_main]
pub fn main() {
    diagnostics::install_panic_hook();
    diagnostics::log(&format!(
        "entry: android_main (version {}, git {})",
        env!("CARGO_PKG_VERSION"),
        option_env!("GIT_HASH").unwrap_or("unknown")
    ));

    stage_shipped_assets();
    diagnostics::log("entry: shipped assets staged");

    let mode = controls::launch_mode();
    diagnostics::log(&format!("entry: mode {mode:?}"));
    let mut options = LaunchOptions::solar();
    options.mode = mode;
    let mut app = build_app(options);
    presentation::install(&mut app);
    controls::install(&mut app, mode);
    diagnostics::log("entry: app built, entering event loop");
    app.run();
    diagnostics::log("entry: event loop returned");
}

/// Copy the synchronously-read shipped data into internal storage and install it
/// as the asset root. Failures are reported and non-fatal: the simulator then
/// degrades to procedural terrain instead of refusing to start.
fn stage_shipped_assets() {
    let Some(app) = bevy_android::ANDROID_APP.get() else {
        diagnostics::log("staging: Bevy app handle unavailable; assets stay unstaged");
        return;
    };
    let Some(base) = app.internal_data_path() else {
        diagnostics::log("staging: internal data directory unavailable; assets stay unstaged");
        return;
    };

    // The root must be installed even when staging is skipped so every path
    // consumer resolves against the same directory.
    shipped_paths::set_root(&base);

    let marker_version = format!(
        "{}-{}-s{STAGE_SCHEMA}",
        env!("CARGO_PKG_VERSION"),
        option_env!("GIT_HASH").unwrap_or("unknown")
    );
    let marker = base.join(STAGE_MARKER);
    if fs::read_to_string(&marker).is_ok_and(|value| value.trim() == marker_version) {
        diagnostics::log("staging: already staged for this build");
        return;
    }

    let manager = app.asset_manager();
    // Destination tree root: `base/assets`, so staged paths line up with the
    // `assets/...` constants used by the domain and application loaders.
    let destination = base.join("assets");
    let mut copied = 0usize;
    let mut missing = Vec::new();
    for path in SHIPPED_FILES {
        match copy_file(&manager, &destination, path) {
            Ok(true) => copied += 1,
            Ok(false) => missing.push(*path),
            Err(error) => {
                diagnostics::log(&format!("staging: {path}: error {error}"));
                missing.push(*path);
            }
        }
    }

    // Only mark a complete copy as staged; a partial copy must be retried on
    // the next launch rather than hidden behind the marker.
    if !missing.is_empty() {
        diagnostics::log(&format!(
            "staging: incomplete: {copied} copied, {} missing (first: {:?})",
            missing.len(),
            missing.first()
        ));
        return;
    }
    if let Some(parent) = marker.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            diagnostics::log(&format!(
                "staging: cannot create {}: {error}",
                parent.display()
            ));
            return;
        }
    }
    if let Err(error) = fs::write(&marker, marker_version) {
        diagnostics::log(&format!("staging: cannot write marker: {error}"));
    }
    diagnostics::log(&format!(
        "staging: {copied} files staged into {}",
        base.display()
    ));
}

/// Copy one APK asset to `<destination>/<path>`.
///
/// `Ok(false)` means the APK does not contain the file; `destination` already
/// corresponds to the APK's `assets/` root.
fn copy_file(
    manager: &ndk::asset::AssetManager,
    destination: &Path,
    path: &str,
) -> io::Result<bool> {
    let name = CString::new(path).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("asset path contains a NUL byte: {path}"),
        )
    })?;
    let Some(mut asset) = manager.open(&name) else {
        return Ok(false);
    };
    let bytes = asset.buffer()?;
    let target = destination.join(path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&target, bytes)?;
    Ok(true)
}
