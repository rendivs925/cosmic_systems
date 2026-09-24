//! The immutable, manifest-backed DE440 authority and its embedded-kernel
//! loader. This is the only domain path that loads or evaluates SPICE kernels.

use super::manifest::{
    load_manifest, validate_manifest, KernelKind, KernelProvenance, ScientificDatasetRole,
};
#[cfg(any(target_arch = "wasm32", test))]
use super::manifest::{manifest_is_valid, KernelManifest, ValidatedKernel};
use super::{BodyState, EphemerisError, NaifBodyId, TdbEpoch, KILOMETERS_CUBED_TO_METERS_CUBED};
use crate::domain::math::{DMat3, DQuat, DVec3};
use crate::domain::services::body_orientation::BodyOrientation;
use crate::domain::services::gravity::EarthJ2GravityModel;
use anise::constants::frames::SSB_J2000;
use anise::frames::Frame;
#[cfg(any(target_arch = "wasm32", test))]
use anise::naif::kpl::parser::parse_bytes;
use anise::naif::kpl::parser::{convert_tpc_items, parse_file};
use anise::naif::kpl::tpc::TPCItem;
use anise::naif::kpl::Parameter;
#[cfg(any(target_arch = "wasm32", test))]
use anise::naif::SPK;
use anise::prelude::Almanac;
#[cfg(any(target_arch = "wasm32", test))]
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
#[cfg(any(target_arch = "wasm32", test))]
use std::io::{BufReader, Cursor};
use std::path::Path;
#[cfg(any(target_arch = "wasm32", test))]
use std::path::PathBuf;

/// Immutable evaluator loaded from one validated local manifest.
pub struct SpiceEphemeris {
    almanac: Almanac,
    provenance: KernelProvenance,
    earth_j2_model: EarthJ2GravityModel,
    leap_seconds_lsk: String,
}

impl SpiceEphemeris {
    pub fn load(manifest_path: impl AsRef<Path>) -> Result<Self, EphemerisError> {
        let manifest_path = manifest_path.as_ref();
        let manifest = load_manifest(manifest_path)?;
        let provenance = validate_manifest(manifest_path, &manifest)?;
        let mut almanac = Almanac::default();

        for kernel in &provenance.validated_kernels {
            if kernel.kind == KernelKind::Spk {
                almanac = almanac
                    .load(kernel.path.to_string_lossy().as_ref())
                    .map_err(|error| EphemerisError::KernelLoad {
                        role: kernel.role,
                        path: kernel.path.clone(),
                        message: error.to_string(),
                    })?;
            }
        }

        match (
            provenance
                .validated_kernels
                .iter()
                .find(|kernel| kernel.role == ScientificDatasetRole::Orientation),
            provenance
                .validated_kernels
                .iter()
                .find(|kernel| kernel.role == ScientificDatasetRole::GravitationalParameters),
        ) {
            (Some(orientation), Some(gravitational_parameters)) => {
                let mut orientation_items = parse_file::<_, TPCItem>(&orientation.path, false)
                    .map_err(|error| EphemerisError::KernelLoad {
                        role: ScientificDatasetRole::Orientation,
                        path: orientation.path.clone(),
                        message: error.to_string(),
                    })?;
                if let Some(mars_override) = provenance
                    .validated_kernels
                    .iter()
                    .find(|kernel| kernel.role == ScientificDatasetRole::MarsOrientationOverride)
                {
                    // The NAIF compatibility PCK is loaded after pck00011 so
                    // only Mars uses the MOLA IAU2000 cartographic orientation.
                    let mut mars_override_items =
                        parse_file::<_, TPCItem>(&mars_override.path, false).map_err(|error| {
                            EphemerisError::KernelLoad {
                                role: ScientificDatasetRole::MarsOrientationOverride,
                                path: mars_override.path.clone(),
                                message: error.to_string(),
                            }
                        })?;
                    sanitize_mars_iau2000_override(&mut mars_override_items);
                    orientation_items.extend(mars_override_items);
                }
                let gravitational_parameter_items =
                    parse_file::<_, TPCItem>(&gravitational_parameters.path, false).map_err(
                        |error| EphemerisError::KernelLoad {
                            role: ScientificDatasetRole::GravitationalParameters,
                            path: gravitational_parameters.path.clone(),
                            message: error.to_string(),
                        },
                    )?;
                let planetary_data =
                    convert_tpc_items(orientation_items, gravitational_parameter_items).map_err(
                        |error| EphemerisError::KernelLoad {
                            role: ScientificDatasetRole::Orientation,
                            path: orientation.path.clone(),
                            message: error.to_string(),
                        },
                    )?;
                almanac = almanac.with_planetary_data(planetary_data);
            }
            (None, None) => {}
            _ => return Err(EphemerisError::IncompleteOrientationDatasets),
        }

        let earth_j2_dataset = provenance
            .validated_kernels
            .iter()
            .find(|kernel| kernel.role == ScientificDatasetRole::GravityHarmonics)
            .ok_or(EphemerisError::GravityHarmonicsUnavailable)?;
        let earth_j2_contents = fs::read_to_string(&earth_j2_dataset.path).map_err(|error| {
            EphemerisError::KernelLoad {
                role: ScientificDatasetRole::GravityHarmonics,
                path: earth_j2_dataset.path.clone(),
                message: error.to_string(),
            }
        })?;
        let earth_j2_model =
            ron::from_str::<EarthJ2GravityModel>(&earth_j2_contents).map_err(|error| {
                EphemerisError::GravityHarmonicsParse {
                    path: earth_j2_dataset.path.clone(),
                    message: error.to_string(),
                }
            })?;
        if !earth_j2_model.is_valid() {
            return Err(EphemerisError::InvalidGravityHarmonics {
                path: earth_j2_dataset.path.clone(),
            });
        }
        let leap_seconds_dataset = provenance
            .validated_kernels
            .iter()
            .find(|kernel| kernel.role == ScientificDatasetRole::LeapSeconds)
            .ok_or(EphemerisError::LeapSecondsUnavailable)?;
        let leap_seconds_lsk = fs::read_to_string(&leap_seconds_dataset.path).map_err(|error| {
            EphemerisError::KernelLoad {
                role: ScientificDatasetRole::LeapSeconds,
                path: leap_seconds_dataset.path.clone(),
                message: error.to_string(),
            }
        })?;

        Ok(Self {
            almanac,
            provenance,
            earth_j2_model,
            leap_seconds_lsk,
        })
    }

    /// Load the same reviewed local kernel set from bytes embedded in the WASM
    /// artifact. Browser environments have no synchronous filesystem, so this
    /// preserves the single DE440 authority without a network fallback.
    #[cfg(any(target_arch = "wasm32", test))]
    pub fn load_embedded() -> Result<Self, EphemerisError> {
        const MANIFEST_PATH: &str = "embedded:assets/configs/ephemeris/de440.ron";
        let manifest = ron::from_str::<KernelManifest>(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/configs/ephemeris/de440.ron"
        )))
        .map_err(|error| EphemerisError::ManifestParse {
            path: PathBuf::from(MANIFEST_PATH),
            message: error.to_string(),
        })?;
        if !manifest_is_valid(&manifest) {
            return Err(EphemerisError::InvalidManifest {
                path: PathBuf::from(MANIFEST_PATH),
            });
        }

        let mut validated_kernels = Vec::with_capacity(manifest.kernels.len());
        for kernel in &manifest.kernels {
            let path = PathBuf::from(format!("embedded:{}", kernel.file_name));
            let bytes = embedded_kernel_bytes(&kernel.file_name).ok_or_else(|| {
                EphemerisError::EmbeddedKernelMissing {
                    file_name: kernel.file_name.clone(),
                }
            })?;
            if bytes.len() as u64 != kernel.expected_size_bytes {
                return Err(EphemerisError::KernelSize {
                    role: kernel.role,
                    path,
                    expected: kernel.expected_size_bytes,
                    actual: bytes.len() as u64,
                });
            }
            let actual_sha256 = format!("{:x}", Sha256::digest(bytes));
            if actual_sha256 != kernel.sha256 {
                return Err(EphemerisError::KernelChecksum {
                    role: kernel.role,
                    path,
                    expected: kernel.sha256.clone(),
                    actual: actual_sha256,
                });
            }
            validated_kernels.push(ValidatedKernel {
                file_name: kernel.file_name.clone(),
                role: kernel.role,
                kind: kernel.kind,
                sha256: kernel.sha256.clone(),
                path: PathBuf::from(format!("embedded:{}", kernel.file_name)),
                expected_size_bytes: kernel.expected_size_bytes,
                source_url: kernel.source_url.clone(),
                coverage: kernel.coverage,
                frame: kernel.frame,
                time_scale: kernel.time_scale,
            });
        }
        let provenance = KernelProvenance {
            manifest_id: manifest.id,
            manifest_path: PathBuf::from(MANIFEST_PATH),
            coverage: manifest.coverage,
            validated_kernels,
            unavailable_roles: manifest.unavailable_roles,
        };

        let translation_path =
            embedded_dataset_path(&provenance, ScientificDatasetRole::Translation)?;
        let translation = embedded_dataset_bytes(&provenance, ScientificDatasetRole::Translation)?;
        let spk = SPK::parse(translation).map_err(|error| EphemerisError::KernelLoad {
            role: ScientificDatasetRole::Translation,
            path: translation_path,
            message: error.to_string(),
        })?;
        let orientation_path =
            embedded_dataset_path(&provenance, ScientificDatasetRole::Orientation)?;
        let mut orientation = parse_embedded_tpc(&provenance, ScientificDatasetRole::Orientation)?;
        if provenance
            .validated_kernels
            .iter()
            .any(|kernel| kernel.role == ScientificDatasetRole::MarsOrientationOverride)
        {
            // Match native load order: this replaces only Mars-system
            // orientation data after the generic PCK is parsed.
            let mut mars_override =
                parse_embedded_tpc(&provenance, ScientificDatasetRole::MarsOrientationOverride)?;
            sanitize_mars_iau2000_override(&mut mars_override);
            orientation.extend(mars_override);
        }
        let gravitational_parameters =
            parse_embedded_tpc(&provenance, ScientificDatasetRole::GravitationalParameters)?;
        let planetary_data =
            convert_tpc_items(orientation, gravitational_parameters).map_err(|error| {
                EphemerisError::KernelLoad {
                    role: ScientificDatasetRole::Orientation,
                    path: orientation_path,
                    message: error.to_string(),
                }
            })?;
        let earth_j2_dataset =
            embedded_dataset_bytes(&provenance, ScientificDatasetRole::GravityHarmonics)?;
        let earth_j2_path =
            embedded_dataset_path(&provenance, ScientificDatasetRole::GravityHarmonics)?;
        let earth_j2_contents =
            std::str::from_utf8(earth_j2_dataset).map_err(|error| EphemerisError::KernelLoad {
                role: ScientificDatasetRole::GravityHarmonics,
                path: earth_j2_path.clone(),
                message: error.to_string(),
            })?;
        let earth_j2_model =
            ron::from_str::<EarthJ2GravityModel>(earth_j2_contents).map_err(|error| {
                EphemerisError::GravityHarmonicsParse {
                    path: earth_j2_path,
                    message: error.to_string(),
                }
            })?;
        if !earth_j2_model.is_valid() {
            return Err(EphemerisError::InvalidGravityHarmonics {
                path: embedded_dataset_path(&provenance, ScientificDatasetRole::GravityHarmonics)?,
            });
        }
        let leap_seconds_bytes =
            embedded_dataset_bytes(&provenance, ScientificDatasetRole::LeapSeconds)?;
        let leap_seconds_path =
            embedded_dataset_path(&provenance, ScientificDatasetRole::LeapSeconds)?;
        let leap_seconds_lsk = std::str::from_utf8(leap_seconds_bytes)
            .map_err(|error| EphemerisError::KernelLoad {
                role: ScientificDatasetRole::LeapSeconds,
                path: leap_seconds_path,
                message: error.to_string(),
            })?
            .to_owned();

        Ok(Self {
            almanac: Almanac::from_spk(spk).with_planetary_data(planetary_data),
            provenance,
            earth_j2_model,
            leap_seconds_lsk,
        })
    }

    pub fn provenance(&self) -> &KernelProvenance {
        &self.provenance
    }

    /// Validated EGM2008 degree-two Earth gravity model. The coefficient and
    /// reference radius remain distinct from DE440's gravitational parameter.
    pub fn earth_j2_model(&self) -> &EarthJ2GravityModel {
        &self.earth_j2_model
    }

    /// Pinned NAIF LSK text validated with the active kernel manifest.
    pub fn leap_seconds_lsk(&self) -> &str {
        &self.leap_seconds_lsk
    }

    pub fn state(
        &self,
        target: NaifBodyId,
        center: NaifBodyId,
        epoch: TdbEpoch,
    ) -> Result<BodyState, EphemerisError> {
        if !self.provenance.coverage.contains(epoch) {
            return Err(EphemerisError::EpochOutsideCoverage {
                epoch,
                coverage: self.provenance.coverage,
            });
        }

        let center_frame = if center == NaifBodyId::SOLAR_SYSTEM_BARYCENTER {
            SSB_J2000
        } else {
            center.j2000_frame()
        };
        let state = self
            .almanac
            .translate(
                target.j2000_frame(),
                center_frame,
                epoch.anise_epoch(),
                None,
            )
            .map_err(|error| EphemerisError::StateEvaluation {
                target,
                center,
                epoch,
                message: error.to_string(),
            })?;

        BodyState::from_anise(
            target,
            center,
            epoch,
            [state.radius_km.x, state.radius_km.y, state.radius_km.z],
            [
                state.velocity_km_s.x,
                state.velocity_km_s.y,
                state.velocity_km_s.z,
            ],
        )
    }

    /// Evaluate an IAU body-fixed orientation from the validated local PCK at
    /// one TDB epoch. There is no catalog fallback on this scientific path.
    pub fn orientation(
        &self,
        target: NaifBodyId,
        epoch: TdbEpoch,
    ) -> Result<BodyOrientation, EphemerisError> {
        if !self.provenance.coverage.contains(epoch) {
            return Err(EphemerisError::EpochOutsideCoverage {
                epoch,
                coverage: self.provenance.coverage,
            });
        }
        let default_orientation_dataset = self
            .provenance
            .validated_kernels
            .iter()
            .find(|kernel| kernel.role == ScientificDatasetRole::Orientation)
            .ok_or(EphemerisError::OrientationUnavailable)?;
        let orientation_dataset = if target == NaifBodyId::MARS_BARYCENTER {
            self.provenance
                .validated_kernels
                .iter()
                .find(|kernel| kernel.role == ScientificDatasetRole::MarsOrientationOverride)
                .unwrap_or(default_orientation_dataset)
        } else {
            default_orientation_dataset
        };
        let orientation_target = target
            .orientation_target()
            .ok_or(EphemerisError::OrientationUnsupportedBody { target })?;
        let inertial = Frame::from_ephem_j2000(orientation_target.value());
        let body_fixed = Frame::new(orientation_target.value(), orientation_target.value());
        let dcm = self
            .almanac
            .rotate(inertial, body_fixed, epoch.anise_epoch())
            .map_err(|error| EphemerisError::OrientationEvaluation {
                target,
                epoch,
                message: error.to_string(),
            })?;
        let angular_velocity = self
            .almanac
            .angular_velocity_wrt_j2000_rad_s(body_fixed, epoch.anise_epoch())
            .map_err(|error| EphemerisError::OrientationEvaluation {
                target,
                epoch,
                message: error.to_string(),
            })?;
        let matrix = dcm.rot_mat;
        let inertial_to_body_fixed = DQuat::from_mat3(&DMat3::from_cols(
            DVec3::new(matrix[(0, 0)], matrix[(1, 0)], matrix[(2, 0)]),
            DVec3::new(matrix[(0, 1)], matrix[(1, 1)], matrix[(2, 1)]),
            DVec3::new(matrix[(0, 2)], matrix[(1, 2)], matrix[(2, 2)]),
        ));

        Ok(BodyOrientation::from_kernel(
            target,
            epoch,
            format!(
                "{}:{}#{}",
                self.provenance.manifest_id,
                orientation_dataset.file_name,
                orientation_dataset.sha256
            ),
            inertial_to_body_fixed,
            // ANISE reports the passive ICRF-to-body-fixed transform's frame
            // angular velocity. `BodyOrientation` owns the body's active spin
            // in ICRF, which has the opposite sign.
            DVec3::new(
                -angular_velocity.x,
                -angular_velocity.y,
                -angular_velocity.z,
            ),
        ))
    }

    /// Return a validated `gm_de440.tpc` standard gravitational parameter in
    /// SI m³/s². Unlike catalog mass times a universal G, this is the exact
    /// body constant supplied by the active scientific dataset.
    pub fn gravitational_parameter_m3_s2(&self, target: NaifBodyId) -> Result<f64, EphemerisError> {
        let gm_dataset = self
            .provenance
            .validated_kernels
            .iter()
            .find(|kernel| kernel.role == ScientificDatasetRole::GravitationalParameters)
            .ok_or(EphemerisError::GravitationalParametersUnavailable)?;
        let physical_target = target
            .gravitational_parameter_target()
            .ok_or(EphemerisError::GravitationalParametersUnsupportedBody { target })?;
        let planetary_data = self
            .almanac
            .get_planetary_data_from_id(physical_target.value())
            .map_err(|error| EphemerisError::GravitationalParameterEvaluation {
                target,
                message: error.to_string(),
            })?;
        let mu_m3_s2 = planetary_data.mu_km3_s2 * KILOMETERS_CUBED_TO_METERS_CUBED;
        if !mu_m3_s2.is_finite() || mu_m3_s2 <= 0.0 {
            return Err(EphemerisError::InvalidGravitationalParameter {
                target,
                file_name: gm_dataset.file_name.clone(),
                value_m3_s2: mu_m3_s2,
            });
        }
        Ok(mu_m3_s2)
    }
}

/// ANISE expects nutation/precession coefficients to be vectors. NAIF's v1
/// compatibility PCK uses scalar zeroes only to neutralize pck00011 variables;
/// omitting them restores the original IAU2000 Mars orientation semantics.
fn sanitize_mars_iau2000_override(items: &mut HashMap<i32, TPCItem>) {
    let Some(mars) = items.get_mut(&499) else {
        return;
    };
    for parameter in [
        Parameter::NutPrecRa,
        Parameter::NutPrecDec,
        Parameter::NutPrecPm,
    ] {
        mars.data.remove(&parameter);
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn embedded_kernel_bytes(file_name: &str) -> Option<&'static [u8]> {
    match file_name {
        "de440s.bsp" => Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/large_files/kernels/de440/de440s.bsp"
        ))),
        "pck00011.tpc" => Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/large_files/kernels/de440/pck00011.tpc"
        ))),
        "mars_iau2000_v1.tpc" => Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/large_files/kernels/de440/mars_iau2000_v1.tpc"
        ))),
        "gm_de440.tpc" => Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/large_files/kernels/de440/gm_de440.tpc"
        ))),
        "egm2008_earth_j2.ron" => Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/large_files/kernels/de440/egm2008_earth_j2.ron"
        ))),
        "naif0012.tls" => Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/large_files/kernels/de440/naif0012.tls"
        ))),
        _ => None,
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn embedded_dataset_path(
    provenance: &KernelProvenance,
    role: ScientificDatasetRole,
) -> Result<PathBuf, EphemerisError> {
    provenance
        .validated_kernels
        .iter()
        .find(|kernel| kernel.role == role)
        .map(|kernel| kernel.path.clone())
        .ok_or(EphemerisError::RequiredDatasetMissing { role })
}

#[cfg(any(target_arch = "wasm32", test))]
fn embedded_dataset_bytes(
    provenance: &KernelProvenance,
    role: ScientificDatasetRole,
) -> Result<&'static [u8], EphemerisError> {
    let dataset = provenance
        .validated_kernels
        .iter()
        .find(|kernel| kernel.role == role)
        .ok_or(EphemerisError::RequiredDatasetMissing { role })?;
    embedded_kernel_bytes(&dataset.file_name).ok_or_else(|| EphemerisError::EmbeddedKernelMissing {
        file_name: dataset.file_name.clone(),
    })
}

#[cfg(any(target_arch = "wasm32", test))]
fn parse_embedded_tpc(
    provenance: &KernelProvenance,
    role: ScientificDatasetRole,
) -> Result<HashMap<i32, TPCItem>, EphemerisError> {
    let path = embedded_dataset_path(provenance, role)?;
    let bytes = embedded_dataset_bytes(provenance, role)?;
    let mut reader = BufReader::new(Cursor::new(bytes));
    parse_bytes::<_, TPCItem>(&mut reader, false).map_err(|error| EphemerisError::KernelLoad {
        role,
        path,
        message: error.to_string(),
    })
}
