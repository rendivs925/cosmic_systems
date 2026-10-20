use super::*;
use crate::domain::services::terrain_source::{ElevationBounds, TerrainSource};

#[derive(Debug)]
struct ConstantTerrain(f64);

impl TerrainSource for ConstantTerrain {
    fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        self.0
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        ElevationBounds::new(self.0, self.0)
    }
}

fn test_config() -> TerrainMetricsConfig {
    TerrainMetricsConfig {
        width: 64,
        height: 32,
        channel_threshold_cells: 8.0,
        variogram_max_lag: 16,
    }
}

#[test]
fn weighted_percentile_uses_area_weights() {
    let mut samples = vec![(1.0, 1.0), (2.0, 8.0), (3.0, 1.0)];
    assert_eq!(weighted_percentile(&mut samples, 0.5), 2.0);
    assert_eq!(weighted_percentile(&mut samples, 0.0), 1.0);
    assert_eq!(weighted_percentile(&mut samples, 1.0), 3.0);
}

#[test]
fn flat_land_has_no_slope_drainage_or_coastline() {
    let report = measure_terrain_metrics(
        &ConstantTerrain(100.0),
        &test_config(),
        "synthetic",
        6_371_000.0,
    )
    .expect("flat terrain must measure");

    assert_eq!(report.hypsometry.min_m, 100.0);
    assert_eq!(report.hypsometry.max_m, 100.0);
    assert_eq!(report.hypsometry.land_fraction, 1.0);
    assert_eq!(report.hypsometry.land_p50_m, 100.0);
    assert_eq!(report.slope.max_deg, 0.0);
    assert_eq!(report.slope.fraction_above_35_deg, 0.0);
    assert_eq!(report.hydrology.channel_cells, 0);
    assert_eq!(report.hydrology.channel_length_km, 0.0);
    assert_eq!(report.hydrology.drainage_density_km_per_km2, 0.0);
    assert!(report.hydrology.hack_exponent.is_none());
    assert_eq!(report.coastline.length_km, 0.0);
    assert!(report.coastline.fractal_dimension.is_none());
}

#[test]
fn slope_matches_an_analytic_ramp() {
    let mut config = test_config();
    config.width = 128;
    config.height = 64;
    let source = RampTerrain {
        step_m: 100.0,
        width: config.width,
    };
    let report =
        measure_terrain_metrics(&source, &config, "ramp", 6_371_000.0).expect("ramp measures");

    // On a cos(latitude)-weighted sphere the median area lies at 30 degrees,
    // so the area-weighted median slope is the analytic slope there.
    let dx = 6_371_000.0 * 30.0f64.to_radians().cos() * (360.0 / 128.0f64).to_radians();
    let expected = (100.0f64 / dx).atan().to_degrees();
    assert!(
        (report.slope.land_p50_deg - expected).abs() < 0.01 * expected,
        "median slope {} must match analytic {}",
        report.slope.land_p50_deg,
        expected
    );
}

#[derive(Debug)]
struct RampTerrain {
    step_m: f64,
    width: u32,
}

impl TerrainSource for RampTerrain {
    fn height_m(&self, _latitude_deg: f64, longitude_deg: f64) -> f64 {
        let x = ((longitude_deg + 180.0) / 360.0 * f64::from(self.width)) as i64;
        1_000.0 + x as f64 * self.step_m
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        ElevationBounds::new(1_000.0, 1_000.0 + f64::from(self.width) * self.step_m)
    }
}

#[test]
fn variogram_recovers_the_ramp_spectral_slope() {
    let source = RampTerrain {
        step_m: 50.0,
        width: 64,
    };
    let report = measure_terrain_metrics(&source, &test_config(), "ramp", 6_371_000.0)
        .expect("ramp measures");
    let slope = report
        .roughness
        .variogram_slope
        .expect("a ramp must produce a variogram");
    assert!(
        (slope - 2.0).abs() < 0.1,
        "a linear ramp has variogram slope 2, measured {slope}"
    );
    assert!((report.roughness.psd_beta.unwrap() - 4.0).abs() < 0.1);
}

#[test]
fn coastline_length_follows_a_full_latitude_band() {
    let mut config = test_config();
    config.width = 72;
    config.height = 36;
    let source = HemisphereTerrain;
    let report =
        measure_terrain_metrics(&source, &config, "hemisphere", 6_371_000.0).expect("measures");

    assert!((report.hypsometry.land_fraction - 0.5).abs() < 0.06);
    // The land/ocean boundary is the equator band; its length is one
    // circumference.
    let expected_km = 2.0 * std::f64::consts::PI * 6_371.0;
    assert!(
        (report.coastline.length_km - expected_km).abs() < 0.01 * expected_km,
        "coastline {} km must approximate {} km",
        report.coastline.length_km,
        expected_km
    );
    assert!(report.coastline.fractal_dimension.is_some());
}

#[derive(Debug)]
struct HemisphereTerrain;

impl TerrainSource for HemisphereTerrain {
    fn height_m(&self, latitude_deg: f64, _longitude_deg: f64) -> f64 {
        if latitude_deg >= 0.0 {
            100.0
        } else {
            -100.0
        }
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        ElevationBounds::new(-100.0, 100.0)
    }
}

#[test]
fn stream_network_orders_follow_one_d8_tree() {
    let width = 16;
    let height = 16;
    let heights: Vec<f32> = (0..height)
        .flat_map(|y| std::iter::repeat_n((height - y) as f32 * 10.0, width))
        .collect();
    let land = vec![true; width * height];
    let latitude_deg: Vec<f64> = (0..height).map(|y| 40.0 - y as f64).collect();
    let cell_area_km2 = vec![1.0; height];
    let stats = super::hydrology::measure(super::hydrology::HydrologyInputs {
        heights: &heights,
        width,
        height,
        land: &land,
        latitude_deg: &latitude_deg,
        cell_area_km2: &cell_area_km2,
        channel_threshold_cells: 4.0,
        body_radius_m: 6_371_000.0,
    });

    // Rows 0..=2 drain into row 3; rows 3..15 are channels (13 rows).
    assert_eq!(stats.channel_cells, 16 * 13);
    assert_eq!(stats.max_strahler_order, 1);
    let expected_length = 16.0 * 12.0 * 111_320.0 / 1_000.0;
    assert!(
        (stats.channel_length_km - expected_length).abs() < 1.0,
        "channel length {} vs {} km",
        stats.channel_length_km,
        expected_length
    );
    let sinuosity = stats
        .median_sinuosity
        .expect("straight channels still have sinuosity");
    assert!((sinuosity - 1.0).abs() < 0.005, "sinuosity was {sinuosity}");
}

#[test]
fn report_round_trips_and_compares() {
    let report = measure_terrain_metrics(
        &ConstantTerrain(42.0),
        &test_config(),
        "synthetic",
        6_371_000.0,
    )
    .expect("measures");

    let mut path = std::env::temp_dir();
    path.push(format!("cosmic_terrain_metrics_{}.ron", std::process::id()));
    save_report(&path, &report).expect("writes baseline");
    let loaded = load_report(&path).expect("reads baseline");
    assert_eq!(report, loaded);
    compare_reports(&report, &loaded).expect("identical reports compare");
    let _ = std::fs::remove_file(&path);

    let mut changed = loaded;
    changed.slope.land_p50_deg += 1.0;
    assert!(matches!(
        compare_reports(&report, &changed),
        Err(TerrainMetricsError::BaselineMismatch(_))
    ));
}

#[test]
fn invalid_config_is_rejected() {
    let mut config = test_config();
    config.width = 4;
    assert!(matches!(
        measure_terrain_metrics(&ConstantTerrain(0.0), &config, "synthetic", 6_371_000.0),
        Err(TerrainMetricsError::InvalidConfig(_))
    ));
}
