//! Offline conversion from an explicitly normalized elevation TIFF to CSLDEM.
//!
//! The input must already be reprojected to WGS84 geographic coordinates and
//! converted into the simulator's terrain vertical datum. GeoTIFF metadata is
//! deliberately not treated as proof of either conversion.

use cosmic_systems_wasm::domain::services::local_elevation::{
    LocalElevationMetadata, LocalElevationPackage,
};
use image::ImageReader;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

const NODATA_M: f32 = -999_999.0;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 7 {
        eprintln!("usage: local_elevation_convert <normalized-terrain-radial.tif> <output.csldem> <west-deg> <south-deg> <east-deg> <north-deg> <metadata.ron>");
        return ExitCode::FAILURE;
    }
    let bounds = match parse_bounds(&args[2..]) {
        Some(bounds) => bounds,
        None => {
            return usage_error(
                "geographic bounds must be finite and ordered west, south, east, north",
            )
        }
    };
    let image = match ImageReader::open(&args[0]) {
        Ok(reader) => match reader.decode() {
            Ok(image) => image.to_luma32f(),
            Err(error) => {
                return usage_error(&format!("unable to decode normalized TIFF: {error}"))
            }
        },
        Err(error) => return usage_error(&format!("unable to decode normalized TIFF: {error}")),
    };
    let metadata: LocalElevationMetadata = match fs::read_to_string(&args[6])
        .ok()
        .and_then(|contents| ron::from_str(&contents).ok())
    {
        Some(metadata) => metadata,
        None => return usage_error("metadata must be a valid LocalElevationMetadata RON document"),
    };
    let source_checksum = match file_sha256(&args[0]) {
        Ok(checksum) => checksum,
        Err(error) => return usage_error(&format!("unable to checksum normalized TIFF: {error}")),
    };
    if metadata.source_sha256.to_ascii_lowercase() != source_checksum {
        return usage_error("metadata source_sha256 does not match the normalized TIFF");
    }
    let samples_m = image
        .as_raw()
        .iter()
        .map(|&sample| {
            if sample.is_finite() && sample > NODATA_M {
                sample
            } else {
                f32::NAN
            }
        })
        .collect();
    let package = match LocalElevationPackage::from_samples(
        image.width(),
        image.height(),
        bounds.0,
        bounds.1,
        bounds.2,
        bounds.3,
        metadata,
        samples_m,
    ) {
        Ok(package) => package,
        Err(error) => return usage_error(&format!("invalid local elevation package: {error:?}")),
    };
    if let Some(parent) = Path::new(&args[1]).parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return usage_error(&format!("unable to create output directory: {error}"));
        }
    }
    if let Err(error) = package.write_path(&args[1]) {
        return usage_error(&format!(
            "unable to write local elevation package: {error:?}"
        ));
    }
    ExitCode::SUCCESS
}

fn parse_bounds(args: &[String]) -> Option<(f64, f64, f64, f64)> {
    let values = args
        .iter()
        .map(|value| value.parse::<f64>().ok())
        .collect::<Option<Vec<_>>>()?;
    let [west, south, east, north]: [f64; 4] = values.try_into().ok()?;
    (west.is_finite()
        && south.is_finite()
        && east.is_finite()
        && north.is_finite()
        && west < east
        && south < north)
        .then_some((west, south, east, north))
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("local elevation conversion failed: {message}");
    ExitCode::FAILURE
}

fn file_sha256(path: &str) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
