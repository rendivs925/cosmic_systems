use std::env;

use cosmic_systems_wasm::application::app_builder::{build_app, validate_vehicle_selection};
use cosmic_systems_wasm::application::modes::parse_launch_options;

fn main() {
    let options = parse_launch_options(env::args().skip(1))
        .unwrap_or_else(|error| panic!("Invalid launch options: {error}"));
    if let Err(error) = validate_vehicle_selection(&options) {
        eprintln!("{error}");
        std::process::exit(2);
    }

    // Composition lives in the library so the Android entry point runs the
    // exact same Bevy app; only launch options and presentation defaults differ.
    build_app(options).run();
}
