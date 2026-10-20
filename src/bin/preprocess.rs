//! Offline terrain data registry checker. Requires `--features dem`.
//!
//! Phase 1 implements validation and size reporting; package generation is
//! added by the later terrain data phases. No path downloads data at runtime.
//!
//! Usage:
//!   preprocess [--body earth] [--layer all|dem|imagery|landcover|soil|water|canopy|climate]
//!              [--region ID] [--quality standard|high|ultra]
//!              [--config PATH] [--check] [--report-size]

use cosmic_systems_wasm::domain::value_objects::dataset_manifest::{
    DatasetEntry, DatasetLayer, TerrainDatasetRegistry, DEFAULT_EARTH_DATASET_REGISTRY,
};
use cosmic_systems_wasm::domain::value_objects::imagery_manifest::EarthImageryManifest;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

struct Options {
    body: String,
    layer: Option<DatasetLayer>,
    region: Option<String>,
    quality: String,
    config: PathBuf,
    check: bool,
    report_size: bool,
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return usage();
        }
    };
    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("preprocess failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut options: Options) -> Result<(), String> {
    if options.body != "earth" {
        return Err(format!(
            "no dataset registry for body {}; only earth is registered",
            options.body
        ));
    }
    if !["standard", "high", "ultra"].contains(&options.quality.as_str()) {
        return Err(format!(
            "unknown quality {}; expected standard, high, or ultra",
            options.quality
        ));
    }
    if !options.check && !options.report_size {
        options.check = true;
    }

    let text = fs::read_to_string(&options.config)
        .map_err(|error| format!("cannot read {}: {error}", options.config.display()))?;
    let registry = TerrainDatasetRegistry::from_ron(&text)?;
    registry.validate()?;

    let selected: Vec<&DatasetEntry> = registry
        .selected(options.layer, options.region.as_deref())
        .collect();
    if selected.is_empty() {
        return Err(format!(
            "no datasets match layer={} region={}",
            options
                .layer
                .map_or("all".to_string(), |layer| layer.as_str().to_string()),
            options.region.as_deref().unwrap_or("all")
        ));
    }

    println!(
        "{} datasets: {} selected (layer={}, region={}, quality={})",
        registry.body,
        selected.len(),
        options
            .layer
            .map_or("all".to_string(), |layer| layer.as_str().to_string()),
        options.region.as_deref().unwrap_or("all"),
        options.quality
    );

    let selected_count = selected.len();
    let mut failures = Vec::new();
    let mut total_bytes = 0u64;
    for entry in selected {
        let size = if options.check {
            match check_entry(entry) {
                Ok(size) => {
                    println!(
                        "  ok   {} [{}] {} bytes",
                        entry.id,
                        entry.layer.as_str(),
                        size
                    );
                    size
                }
                Err(error) => {
                    failures.push(format!("{}: {error}", entry.id));
                    continue;
                }
            }
        } else {
            match runtime_size(Path::new(&entry.runtime_asset)) {
                Ok(size) => size,
                Err(error) => {
                    failures.push(format!("{}: {error}", entry.id));
                    continue;
                }
            }
        };
        total_bytes = total_bytes.saturating_add(size);
        if options.report_size {
            println!(
                "  size {} [{}] {} bytes ({:.1} MiB) {}",
                entry.id,
                entry.layer.as_str(),
                size,
                size as f64 / (1024.0 * 1024.0),
                entry.bounds.label()
            );
        }
    }

    if options.report_size {
        println!(
            "  total {:.1} MiB across {} datasets",
            total_bytes as f64 / (1024.0 * 1024.0),
            selected_count
        );
    }
    if !failures.is_empty() {
        return Err(format!(
            "{} dataset(s) failed validation:\n  {}",
            failures.len(),
            failures.join("\n  ")
        ));
    }
    if options.check {
        println!("all selected datasets are present, verified, and fresh");
    }
    Ok(())
}

fn check_entry(entry: &DatasetEntry) -> Result<u64, String> {
    let asset = Path::new(&entry.runtime_asset);
    if !asset.exists() {
        return Err(format!("runtime asset is missing: {}", entry.runtime_asset));
    }
    let (digest, bytes) = runtime_digest(asset)?;
    if !digest.eq_ignore_ascii_case(&entry.runtime_sha256) {
        return Err(format!(
            "runtime asset is stale: expected {}, found {digest}",
            entry.runtime_sha256
        ));
    }
    if let Some(manifest_path) = &entry.manifest_path {
        verify_manifest(entry, Path::new(manifest_path))?;
    }
    Ok(bytes)
}

fn verify_manifest(entry: &DatasetEntry, path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!(
            "referenced manifest is missing: {}",
            path.display()
        ));
    }
    if entry.layer != DatasetLayer::Imagery {
        // Elevation seed manifests are provenance records without a shared
        // parser; their runtime package is already verified above.
        return Ok(());
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let manifest = EarthImageryManifest::from_ron(&text)?;
    manifest.validate()?;

    let matches_global = manifest
        .global_overview
        .source_sha256
        .eq_ignore_ascii_case(&entry.source_sha256)
        && manifest
            .global_overview
            .runtime_sha256
            .eq_ignore_ascii_case(&entry.runtime_sha256);
    let matches_region = manifest.local_regions.iter().any(|region| {
        region
            .source_sha256
            .eq_ignore_ascii_case(&entry.source_sha256)
            && region
                .runtime_sha256
                .eq_ignore_ascii_case(&entry.runtime_sha256)
    });
    if !matches_global && !matches_region {
        return Err(format!(
            "registry checksums are not present in {}",
            path.display()
        ));
    }
    Ok(())
}

fn runtime_digest(path: &Path) -> Result<(String, u64), String> {
    if path.is_file() {
        let bytes = path
            .metadata()
            .map_err(|error| format!("cannot stat {}: {error}", path.display()))?
            .len();
        return Ok((sha256_file(path)?, bytes));
    }
    let files = collect_files(path)?;
    if files.is_empty() {
        return Err(format!(
            "runtime tile directory is empty: {}",
            path.display()
        ));
    }
    let mut aggregate = Sha256::new();
    let mut bytes = 0u64;
    for (file, relative) in files {
        let digest = sha256_file(&file)?;
        bytes += file
            .metadata()
            .map_err(|error| format!("cannot stat {}: {error}", file.display()))?
            .len();
        aggregate.update(format!("{digest}  {relative}\n").as_bytes());
    }
    Ok((hex(aggregate.finalize()), bytes))
}

fn runtime_size(path: &Path) -> Result<u64, String> {
    if path.is_file() {
        return Ok(path
            .metadata()
            .map_err(|error| format!("cannot stat {}: {error}", path.display()))?
            .len());
    }
    if !path.is_dir() {
        return Err(format!("runtime asset is missing: {}", path.display()));
    }
    let mut bytes = 0u64;
    for (file, _) in collect_files(path)? {
        bytes += file
            .metadata()
            .map_err(|error| format!("cannot stat {}: {error}", file.display()))?
            .len();
    }
    Ok(bytes)
}

/// Deterministic recursive file listing, sorted by repository-style relative
/// path so a tile-directory checksum is order independent.
fn collect_files(root: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let mut files = Vec::new();
    walk(root, root, &mut files)?;
    files.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(files)
}

fn walk(root: &Path, directory: &Path, files: &mut Vec<(PathBuf, String)>) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read directory entry: {error}"))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot stat {}: {error}", path.display()))?;
        if file_type.is_dir() {
            walk(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "file is outside its runtime root".to_string())?
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.push((path, relative));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(hasher.finalize()))
}

fn hex(digest: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in digest.as_ref() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        body: "earth".into(),
        layer: None,
        region: None,
        quality: "standard".into(),
        config: PathBuf::from(DEFAULT_EARTH_DATASET_REGISTRY),
        check: false,
        report_size: false,
    };
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--body" => options.body = arguments.next().ok_or("--body requires a value")?,
            "--layer" => {
                let value = arguments.next().ok_or("--layer requires a value")?;
                options.layer = if value == "all" {
                    None
                } else {
                    Some(
                        DatasetLayer::from_cli(&value)
                            .ok_or_else(|| format!("unknown layer {value}"))?,
                    )
                };
            }
            "--region" => {
                options.region = Some(arguments.next().ok_or("--region requires an id")?);
            }
            "--quality" => {
                options.quality = arguments.next().ok_or("--quality requires a value")?;
            }
            "--config" => {
                options.config = PathBuf::from(arguments.next().ok_or("--config requires a path")?);
            }
            "--check" => options.check = true,
            "--report-size" => options.report_size = true,
            "--help" | "-h" => return Err(String::new()),
            other => return Err(format!("unrecognized argument {other}")),
        }
    }
    Ok(options)
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: cargo run --features dem --bin preprocess -- \
         [--body earth] \
         [--layer all|dem|imagery|landcover|soil|water|canopy|climate] \
         [--region ID] [--quality standard|high|ultra] [--config PATH] \
         [--check] [--report-size]"
    );
    ExitCode::FAILURE
}
