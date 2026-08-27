//! Rocket-mode terrain selection configuration.

use bevy::prelude::*;
#[cfg(feature = "dem")]
use std::env;
#[cfg(feature = "dem")]
use std::path::PathBuf;

/// Optional directory of local SRTM `.hgt` tiles for rocket-mode Earth terrain.
/// Set `COSMIC_SRTM_DIR` in a `--features dem` build; an empty, missing, or
/// unreadable directory leaves the deterministic procedural Earth unchanged.
#[derive(Resource, Debug, Clone, Default)]
pub struct EarthTerrainConfig {
    #[cfg(feature = "dem")]
    pub srtm_dir: Option<PathBuf>,
}

#[cfg(feature = "dem")]
impl EarthTerrainConfig {
    pub fn from_environment() -> Self {
        let Some(value) = env::var_os("COSMIC_SRTM_DIR").filter(|value| !value.is_empty()) else {
            return Self::default();
        };
        let path = PathBuf::from(value);
        if path.is_dir() {
            info!(
                "using local SRTM terrain from COSMIC_SRTM_DIR={}",
                path.display()
            );
            Self {
                srtm_dir: Some(path),
            }
        } else {
            warn!(
                "COSMIC_SRTM_DIR={} is not a readable directory; using procedural Earth terrain",
                path.display()
            );
            Self::default()
        }
    }
}
