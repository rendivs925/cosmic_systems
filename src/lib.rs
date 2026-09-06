pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;
#[cfg(target_arch = "wasm32")]
pub mod wasm;
