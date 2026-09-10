//! Offline upgrade utility for legacy CSDEM terrain packages.

use cosmic_systems_wasm::domain::services::dem_terrain_source::CubeSphereDem;

fn main() {
    let mut arguments = std::env::args().skip(1);
    let Some(input_path) = arguments.next() else {
        usage_and_exit();
    };
    let Some(output_path) = arguments.next() else {
        usage_and_exit();
    };
    if arguments.next().is_some() {
        usage_and_exit();
    }

    CubeSphereDem::upgrade_legacy_v1_path(&input_path, &output_path)
        .unwrap_or_else(|error| panic!("failed to upgrade {input_path}: {error}"));
}

fn usage_and_exit() -> ! {
    eprintln!(
        "usage: cargo run --features dem --bin csdem_upgrade -- <input-v1.csdem> <output-v2.csdem>"
    );
    std::process::exit(2);
}
