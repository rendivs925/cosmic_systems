//! RON-backed MVP part-catalog loading.
//!
//! The domain owns the [`PartCatalog`] value object; this application module
//! owns the file schema location and fail-fast loading, mirroring
//! `rocket_config` (AGENTS.md sections 25 and 65).

use crate::domain::game::parts::{PartCatalog, PartCatalogError, PartDef};
use ron::error::SpannedError;
use ron::extensions::Extensions;
use ron::Options;
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Location of the shipped part catalog, relative to the asset root.
pub const PARTS_RELATIVE_PATH: &str = "assets/configs/parts";
/// Default MVP part-catalog file stem.
pub const DEFAULT_PART_CATALOG_FILE: &str = "mvp_parts.ron";

/// Loads the shipped MVP part catalog, failing fast on missing or invalid data.
pub fn load_default_part_catalog() -> Result<PartCatalog, PartCatalogLoadError> {
    let path = parts_root().join(DEFAULT_PART_CATALOG_FILE);
    let text = fs::read_to_string(&path).map_err(|source| PartCatalogLoadError::Io {
        path: path.clone(),
        source,
    })?;
    let parts: Vec<PartDef> = Options::default()
        .with_default_extension(Extensions::IMPLICIT_SOME)
        .from_str(&text)
        .map_err(PartCatalogLoadError::Parse)?;
    PartCatalog::new(parts).map_err(PartCatalogLoadError::Invalid)
}

/// Resolves the parts directory like bevy_asset resolves its asset root.
fn parts_root() -> PathBuf {
    if let Ok(root) = env::var("BEVY_ASSET_ROOT") {
        return Path::new(&root).join(PARTS_RELATIVE_PATH);
    }
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        return Path::new(&manifest_dir).join(PARTS_RELATIVE_PATH);
    }
    PathBuf::from(PARTS_RELATIVE_PATH)
}

#[derive(Debug)]
pub enum PartCatalogLoadError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse(SpannedError),
    Invalid(PartCatalogError),
}

impl fmt::Display for PartCatalogLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::Parse(error) => write!(f, "part catalog RON parse error: {error}"),
            Self::Invalid(error) => write!(f, "invalid part catalog: {error}"),
        }
    }
}

impl std::error::Error for PartCatalogLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse(error) => Some(error),
            Self::Invalid(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::game::parts::PartCategory;

    #[test]
    fn shipped_mvp_catalog_loads_and_validates() {
        let catalog = load_default_part_catalog().expect("shipped part catalog must load");
        assert!(catalog.len() >= 6, "catalog should ship a usable part set");
        assert!(catalog
            .iter()
            .any(|part| part.category == PartCategory::Engine));
        assert!(catalog
            .iter()
            .any(|part| part.category == PartCategory::FuelTank));
        assert!(catalog
            .iter()
            .any(|part| part.category == PartCategory::CommandCapsule));
    }
}
