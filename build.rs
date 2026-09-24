//! Capture the git revision into the binary so `SimulationRunIdentity`
//! records real provenance instead of a placeholder.
//!
//! Outside a git checkout (packaged/crates.io/wasm builds) the revision falls
//! back to `"unknown"`; provenance never fails the build.

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
    // Re-run when the checked-out revision changes.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
}
