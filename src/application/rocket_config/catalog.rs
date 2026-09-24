//! Vehicle catalog discovery, keys, and selection resources.

use super::schema::RocketConfigFile;
use super::{LoadedVehicle, RocketConfigError};
use bevy::prelude::*;
use sha2::{Digest, Sha256};
use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// All vehicles available for selection, keyed by config file stem
/// (`falcon9.ron` → key `falcon9`). A BTreeMap keeps listing deterministic.
#[derive(Resource, Debug, Default)]
pub struct RocketCatalog {
    vehicles: BTreeMap<VehicleKey, LoadedVehicle>,
}

impl RocketCatalog {
    pub(crate) fn insert(&mut self, key: VehicleKey, vehicle: LoadedVehicle) {
        self.vehicles.insert(key, vehicle);
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.vehicles.keys().map(VehicleKey::as_str)
    }

    pub fn resolve<'catalog, 'selection>(
        &'catalog self,
        selection: &'selection VehicleSelection,
    ) -> Option<(&'selection str, &'catalog LoadedVehicle)> {
        let key = selection.selected_key();
        self.vehicles.get::<str>(key).map(|vehicle| (key, vehicle))
    }

    fn contains_key(&self, key: &VehicleKey) -> bool {
        self.vehicles.contains_key(key.as_str())
    }

    /// Load every `*.ron` vehicle definition from the config directory,
    /// keyed by config-file stem (`falcon9.ron` → key `falcon9`) so the CLI
    /// selection matches the shipped file names.
    pub fn from_dir() -> Result<RocketCatalog, RocketConfigError> {
        let dir = configs_root();
        let entries = fs::read_dir(&dir).map_err(|source| RocketConfigError::Io {
            path: dir.clone(),
            source,
        })?;

        let mut paths: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "ron"))
            .collect();
        paths.sort();

        let mut catalog = Self::default();
        for path in paths {
            let Some(key) = path.file_stem().and_then(|stem| stem.to_str()) else {
                return Err(RocketConfigError::MissingStem { path });
            };
            let key = VehicleKey::from(key);
            let text = fs::read_to_string(&path).map_err(|source| RocketConfigError::Io {
                path: path.clone(),
                source,
            })?;
            if catalog.contains_key(&key) {
                return Err(RocketConfigError::DuplicateKey {
                    key: key.as_str().to_owned(),
                    path,
                });
            }
            let configuration_sha256 = format!("{:x}", Sha256::digest(text.as_bytes()));
            for mut vehicle in RocketConfigFile::parse(&text)? {
                vehicle.configuration_sha256 = configuration_sha256.clone();
                catalog.insert(key.clone(), vehicle);
            }
        }
        if catalog.vehicles.is_empty() {
            return Err(RocketConfigError::NoVehicles { dir });
        }
        Ok(catalog)
    }
}

/// Location of the shipped vehicle definitions, relative to the asset root.
pub const CONFIGS_RELATIVE_PATH: &str = "assets/configs/rockets";

/// Default vehicle when no `--vehicle` argument is given.
pub(crate) const DEFAULT_VEHICLE_KEY: &str = "falcon9";

/// A catalog key derived from a vehicle config-file stem or supplied through
/// `--vehicle`. It is distinct from the vehicle's user-facing display name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VehicleKey(String);

impl VehicleKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for VehicleKey {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for VehicleKey {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl Borrow<str> for VehicleKey {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

/// CLI-selected vehicle key (`--vehicle <key>`). `Default` selects the
/// configured default catalog entry without storing a sentinel string.
#[derive(Resource, Debug, Default, Clone)]
pub enum VehicleSelection {
    #[default]
    Default,
    Requested(VehicleKey),
}

impl From<Option<String>> for VehicleSelection {
    fn from(value: Option<String>) -> Self {
        value.map_or(Self::Default, |key| Self::Requested(key.into()))
    }
}

impl VehicleSelection {
    pub fn requested(&self) -> Option<&VehicleKey> {
        match self {
            Self::Default => None,
            Self::Requested(key) => Some(key),
        }
    }

    pub(crate) fn selected_key(&self) -> &str {
        self.requested()
            .map_or(DEFAULT_VEHICLE_KEY, VehicleKey::as_str)
    }
}

/// Resolve the config directory the same way bevy_asset resolves its asset
/// root: BEVY_ASSET_ROOT env, then CARGO_MANIFEST_DIR (set under cargo),
/// then the current directory.
fn configs_root() -> PathBuf {
    if let Ok(root) = env::var("BEVY_ASSET_ROOT") {
        return Path::new(&root).join(CONFIGS_RELATIVE_PATH);
    }
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        return Path::new(&manifest_dir).join(CONFIGS_RELATIVE_PATH);
    }
    PathBuf::from(CONFIGS_RELATIVE_PATH)
}
