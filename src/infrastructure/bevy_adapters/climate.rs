//! Presentation-only measured climatology resource and startup loader.
//!
//! The package is loaded once from the ignored runtime asset tree. It informs
//! sky and cloud presentation only; it is never consulted by flight dynamics,
//! collision, altitude, or the authoritative [`AtmosphereSource`].
//!
//! [`AtmosphereSource`]: crate::domain::services::atmosphere::AtmosphereSource

use crate::domain::services::climate::{ClimateLookup, ClimateSample, ClimateSet};
use bevy::prelude::*;
use std::sync::Arc;

/// Loaded monthly climatology packages, if the optional runtime asset exists.
#[derive(Resource, Debug, Clone, Default)]
pub struct MeasuredClimate(pub Option<Arc<ClimateSet>>);

impl MeasuredClimate {
    pub fn sample(
        &self,
        latitude_deg: f64,
        longitude_deg: f64,
        month_fraction: f64,
    ) -> Option<ClimateSample> {
        self.0
            .as_deref()
            .and_then(|set| set.sample_climate(latitude_deg, longitude_deg, month_fraction))
    }

    pub fn package_count(&self) -> usize {
        self.0.as_deref().map_or(0, ClimateSet::package_count)
    }
}

/// Load every `.csclim` package in file-name order at rocket-mode startup.
/// Missing or malformed packages leave the procedural presentation untouched.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_default_climate(mut climate: ResMut<MeasuredClimate>) {
    use crate::domain::services::climate::DEFAULT_CLIMATE_DIR;

    let directory = std::path::Path::new(DEFAULT_CLIMATE_DIR);
    let mut paths = match std::fs::read_dir(directory) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "csclim")
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            info!(
                "measured climate unavailable at {} ({error}); using procedural presentation",
                directory.display()
            );
            return;
        }
    };
    paths.sort();
    let mut packages = Vec::with_capacity(paths.len());
    for path in paths {
        match crate::domain::services::climate::ClimatePackage::from_path(&path) {
            Ok(package) => packages.push(package),
            Err(error) => warn!(
                "climate package {:?} unavailable ({error:?}); skipping",
                path
            ),
        }
    }
    if packages.is_empty() {
        info!("no measured climate packages loaded; using procedural presentation");
        return;
    }
    let count = packages.len();
    climate.0 = Some(Arc::new(ClimateSet::new(packages)));
    info!(
        "measured climate packages loaded: {count} from {}",
        directory.display()
    );
}
