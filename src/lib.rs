pub mod application;
pub mod components;
pub mod domain;
pub mod infrastructure;
pub mod presentation;
pub mod systems;
#[cfg(target_arch = "wasm32")]
pub mod wasm;
