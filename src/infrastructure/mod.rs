// Infrastructure layer: Repositories, external services
pub mod bevy_adapters;
#[cfg(target_arch = "wasm32")]
pub mod web_workers;
