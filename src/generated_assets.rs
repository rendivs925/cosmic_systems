//! Assets discovered at build time.
//!
//! `build.rs` writes these lists from the repository's `assets/` tree:
//! - `SHIPPED_FILES`: data copied out of the APK before the simulation starts.
//! - `PRELOAD_TEXTURES`: textures decoded and uploaded during the loading phase
//!   so the interactive loop never stalls on texture work.

#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/shipped_assets.rs"));
