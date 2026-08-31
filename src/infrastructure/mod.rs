// Infrastructure layer: Repositories, external services
pub mod bevy_adapters;
pub mod plugins;
#[cfg(target_arch = "wasm32")]
pub mod web_workers;
