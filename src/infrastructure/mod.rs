// Infrastructure layer: Repositories, external services
pub mod bevy_adapters;
pub mod memory;
#[cfg(target_arch = "wasm32")]
pub mod web_workers;
