//! One authoritative resolution of paths to data shipped with the application.
//!
//! Every runtime consumer of measured data (vehicle configs, part catalog,
//! SPICE kernels, DEM packages, climate/soil/surface fields) refers to files as
//! asset-root-relative paths such as `assets/large_files/terrain/...`.
//!
//! Desktop builds resolve those paths against the asset root on disk. Android
//! installs have no filesystem copy of the APK assets, so the mobile entry
//! point stages the files it needs into the app's internal storage and installs
//! that directory as the root here. Keeping the policy in one place prevents
//! each subsystem from inventing its own platform-specific path logic
//! (AGENTS.md section 50).

use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Explicit root set by a platform entry point (Android asset staging).
///
/// It takes precedence over the environment because the staged directory is the
/// only readable copy of the shipped data on that platform.
static ROOT_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

/// Environment variable naming the asset root, mirroring `bevy_asset`.
pub const ASSET_ROOT_ENV: &str = "BEVY_ASSET_ROOT";

/// Install the platform-provided asset root. Later calls are ignored: the first
/// root is authoritative for the process lifetime.
pub fn set_root(root: impl Into<PathBuf>) {
    let _ = ROOT_OVERRIDE.set(root.into());
}

/// The asset root: the directory that *contains* the `assets/` tree.
///
/// Resolution order matches `bevy_asset` so a native run and the asset server
/// always agree on where the shipped data lives.
pub fn root() -> PathBuf {
    if let Some(root) = ROOT_OVERRIDE.get() {
        return root.clone();
    }
    if let Some(root) = non_empty_env(ASSET_ROOT_ENV) {
        return PathBuf::from(root);
    }
    if let Some(root) = non_empty_env("CARGO_MANIFEST_DIR") {
        return PathBuf::from(root);
    }
    PathBuf::from(".")
}

/// Resolve an asset-root-relative path against [`root`].
///
/// A leading separator is stripped: platform constants are written with a
/// leading slash in some call sites, and joining an absolute path would discard
/// the root entirely.
pub fn resolve(relative: impl AsRef<Path>) -> PathBuf {
    join(&root(), relative.as_ref())
}

fn join(root: &Path, relative: &Path) -> PathBuf {
    let relative = relative
        .strip_prefix("/")
        .or_else(|_| relative.strip_prefix("\\"))
        .unwrap_or(relative);
    if root.as_os_str().is_empty() || root == Path::new(".") {
        return relative.to_path_buf();
    }
    root.join(relative)
}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::join;
    use std::path::{Path, PathBuf};

    #[test]
    fn relative_paths_join_the_root() {
        assert_eq!(
            join(
                Path::new("/data/user/0/app/files"),
                Path::new("assets/configs/rockets")
            ),
            PathBuf::from("/data/user/0/app/files/assets/configs/rockets")
        );
    }

    #[test]
    fn leading_separator_cannot_escape_the_root() {
        assert_eq!(
            join(
                Path::new("/data/user/0/app/files"),
                Path::new("/assets/configs/ephemeris/de440.ron")
            ),
            PathBuf::from("/data/user/0/app/files/assets/configs/ephemeris/de440.ron")
        );
    }

    #[test]
    fn empty_root_keeps_the_relative_path() {
        assert_eq!(
            join(Path::new(""), Path::new("assets/large_files")),
            PathBuf::from("assets/large_files")
        );
    }
}
