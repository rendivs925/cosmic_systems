//! Capture the git revision into the binary so `SimulationRunIdentity`
//! records real provenance instead of a placeholder.
//!
//! Outside a git checkout (packaged/crates.io/wasm builds) the revision falls
//! back to `"unknown"`; provenance never fails the build.
//!
//! The script also emits the list of shipped data files that the Android entry
//! point stages out of the APK. Android's `AAssetDir` enumerates files but not
//! subdirectories, so walking the packaged tree at runtime cannot reach
//! `assets/configs/rockets/...`; the explicit list is the reliable mechanism.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let revision = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_HASH={revision}");

    // Package id for the Android media-directory log. The Gradle build exports
    // `ANDROID_PACKAGE_NAME`; `cargo-apk2` (legacy) exports
    // `CARGO_APK2_PACKAGE`. `rustc-env` keeps the value part of the
    // fingerprint, unlike `option_env!`.
    println!("cargo:rerun-if-env-changed=ANDROID_PACKAGE_NAME");
    println!("cargo:rerun-if-env-changed=CARGO_APK2_PACKAGE");
    let package = std::env::var("ANDROID_PACKAGE_NAME")
        .or_else(|_| std::env::var("CARGO_APK2_PACKAGE"))
        .unwrap_or_else(|_| "cosmic_systems".to_string());
    println!("cargo:rustc-env=ANDROID_PACKAGE={package}");
    // Re-run when the checked-out revision changes.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");

    emit_shipped_asset_manifest();
}

/// Write `SHIPPED_FILES`, APK-asset paths (without the `assets/` prefix) for the
/// trees that the Android entry point copies into internal storage.
fn emit_shipped_asset_manifest() {
    let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let assets = Path::new(&manifest_dir).join("assets");

    let mut entries = Vec::new();
    for tree in ["configs", "large_files"] {
        let root = assets.join(tree);
        println!("cargo:rerun-if-changed=assets/{tree}");
        // AssetManager paths are relative to the APK's `assets/` directory, so
        // the recorded path starts at the tree name.
        collect_files(&root, Path::new(tree), &mut entries);
    }
    entries.sort();

    let mut textures = Vec::new();
    println!("cargo:rerun-if-changed=assets/textures");
    collect_files(
        &assets.join("textures"),
        Path::new("textures"),
        &mut textures,
    );
    textures.sort();

    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        return;
    };
    let mut code = String::from(
        "/// Files staged out of the APK on Android.\npub const SHIPPED_FILES: &[&str] = &[\n",
    );
    for entry in &entries {
        writeln!(code, "    {entry:?},").expect("write to String cannot fail");
    }
    code.push_str("];\n");
    code.push_str(
        "\n/// Textures decoded and uploaded before the simulation starts.\npub const PRELOAD_TEXTURES: &[&str] = &[\n",
    );
    for entry in &textures {
        writeln!(code, "    {entry:?},").expect("write to String cannot fail");
    }
    code.push_str("];\n");
    let path = PathBuf::from(out_dir).join("shipped_assets.rs");
    std::fs::write(&path, code).expect("cannot write shipped asset manifest");
}

fn collect_files(root: &Path, relative: &Path, entries: &mut Vec<String>) {
    let Ok(read_dir) = std::fs::read_dir(root) else {
        return;
    };
    for entry in read_dir.flatten() {
        let name = entry.file_name();
        let child_relative = relative.join(&name);
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, &child_relative, entries);
        } else if let Some(text) = child_relative.to_str() {
            entries.push(text.replace('\\', "/"));
        }
    }
}
