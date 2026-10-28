//! Opt-in measured gravity-harmonic package for the fixed rocket pipeline.
//!
//! The package is an authoritative force-model input, not presentation data.
//! It is loaded once at startup and is only consulted when the selected
//! [`crate::domain::services::gravity::ForceModelTier`] declares the Earth
//! spherical-harmonic term. The default `PlanetSun` tier never reads it, and a
//! missing package leaves the point-mass result untouched.

use std::sync::Arc;

use bevy::prelude::*;

use crate::domain::services::gravity_harmonics::SphericalHarmonicGravityModel;

/// Native runtime location of the baked EGM2008 degree-360 package.
pub const DEFAULT_GRAVITY_HARMONICS_PATH: &str =
    "assets/large_files/gravity/egm2008_cs360_v1.csgravc";

/// Loaded spherical-harmonic coefficient package, if the optional asset exists.
#[derive(Resource, Debug, Clone, Default)]
pub struct MeasuredGravityHarmonics(pub Option<Arc<SphericalHarmonicGravityModel>>);

impl MeasuredGravityHarmonics {
    pub fn model(&self) -> Option<&SphericalHarmonicGravityModel> {
        self.0.as_deref()
    }
}

/// Load the reviewed package once. An absent or malformed package is reported
/// and never silently replaced with synthetic coefficients.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_default_gravity_harmonics(mut harmonics: ResMut<MeasuredGravityHarmonics>) {
    use std::path::Path;

    let path = Path::new(DEFAULT_GRAVITY_HARMONICS_PATH);
    if !path.exists() {
        info!(
            "gravity harmonics package unavailable at {}; \
             the earth-harmonics tier will use point mass",
            path.display()
        );
        return;
    }
    match SphericalHarmonicGravityModel::from_path(path) {
        Ok(model) => {
            info!(
                "gravity harmonics loaded: {} degree {} from {}",
                model.metadata().model_id,
                model.degree(),
                path.display()
            );
            harmonics.0 = Some(Arc::new(model));
        }
        Err(error) => warn!(
            "gravity harmonics package {:?} is invalid ({error:?}); \
             the earth-harmonics tier will use point mass",
            path
        ),
    }
}
