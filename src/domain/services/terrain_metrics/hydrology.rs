//! Stream-network metrics derived from the authoritative D8 routing.
//!
//! The measurement grid is routed with the same
//! [`flow_accumulation_geographic`](crate::domain::services::erosion::flow_accumulation_geographic)
//! and D8 tie-breaking used by erosion, so slope, drainage, Hack, concavity,
//! Strahler, and sinuosity all describe one network. A cell joins the stream
//! network when its flow accumulation reaches `channel_threshold_cells`.
//!
//! Hack and concavity are fit on the *stream* network: the upstream flow-path
//! length stands in for basin main-stem length. That proxy is resolution
//! dependent and documented as such; it is still measured identically on both
//! sides of a parity comparison.

use crate::domain::services::erosion::{d8_downstream_geographic, flow_accumulation_geographic};
use serde::{Deserialize, Serialize};

use super::{haversine_m, log_log_slope, sample_longitude_deg};

/// Drainage and stream-network statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HydrologyStats {
    pub channel_cells: u64,
    pub channel_length_km: f64,
    pub land_area_km2: f64,
    pub drainage_density_km_per_km2: f64,
    /// Hack exponent `h` in `L ~ A^h`, fit on the stream network.
    pub hack_exponent: Option<f64>,
    /// Concavity `theta` in `S ~ A^-theta`, fit on stream cells.
    pub concavity_theta: Option<f64>,
    pub max_strahler_order: u32,
    /// Median along-stream sinuosity (path length / straight distance).
    pub median_sinuosity: Option<f64>,
}

/// Read-only inputs for one hydrology measurement.
pub(super) struct HydrologyInputs<'a> {
    pub heights: &'a [f32],
    pub width: usize,
    pub height: usize,
    pub land: &'a [bool],
    pub latitude_deg: &'a [f64],
    pub cell_area_km2: &'a [f64],
    pub channel_threshold_cells: f64,
    pub body_radius_m: f64,
}

pub(super) fn measure(input: HydrologyInputs<'_>) -> HydrologyStats {
    let HydrologyInputs {
        heights,
        width,
        height,
        land,
        latitude_deg,
        cell_area_km2,
        channel_threshold_cells,
        body_radius_m,
    } = input;
    let cells = width * height;
    let lat_min = latitude_deg.first().copied().unwrap_or(0.0);
    let lat_max = latitude_deg.last().copied().unwrap_or(0.0);
    let lon_min = sample_longitude_deg(0, width);
    let lon_max = sample_longitude_deg(width.saturating_sub(1), width);

    let flow =
        flow_accumulation_geographic(heights, width, height, lat_min, lat_max, lon_min, lon_max);
    let downstream =
        d8_downstream_geographic(heights, width, height, lat_min, lat_max, lon_min, lon_max);

    let channel: Vec<bool> = (0..cells)
        .map(|index| land[index] && f64::from(flow[index]) >= channel_threshold_cells)
        .collect();

    let land_area_km2: f64 = (0..cells)
        .filter(|index| land[*index])
        .map(|index| cell_area_km2[index / width])
        .sum();

    let mut channel_length_km = 0.0;
    for (index, downstream) in downstream.iter().enumerate() {
        if channel[index] {
            if let Some((_, distance_m)) = downstream {
                channel_length_km += distance_m / 1_000.0;
            }
        }
    }

    // Descending height is a valid upstream-to-downstream order because D8
    // only routes to strictly lower cells. Ties fall back to the flat index so
    // the traversal is deterministic.
    let mut order: Vec<usize> = (0..cells).collect();
    order.sort_by(|&a, &b| heights[b].total_cmp(&heights[a]).then_with(|| a.cmp(&b)));

    let mut top_order = vec![0u32; cells];
    let mut second_order = vec![0u32; cells];
    let mut stream_order = vec![0u32; cells];
    let mut path_len_m = vec![0.0f64; cells];
    let mut path_source = (0..cells).collect::<Vec<_>>();

    for &index in &order {
        if !channel[index] {
            continue;
        }
        let order_value = if top_order[index] == 0 {
            1
        } else if second_order[index] == top_order[index] {
            top_order[index] + 1
        } else {
            top_order[index]
        };
        stream_order[index] = order_value;

        let Some((next, distance_m)) = downstream[index] else {
            continue;
        };

        if channel[next] {
            if order_value > top_order[next] {
                second_order[next] = top_order[next];
                top_order[next] = order_value;
            } else if order_value > second_order[next] {
                second_order[next] = order_value;
            }

            let candidate = path_len_m[index] + distance_m;
            if candidate > path_len_m[next] {
                path_len_m[next] = candidate;
                path_source[next] = path_source[index];
            }
        }
    }

    let max_strahler_order = stream_order.iter().copied().max().unwrap_or(0);

    let mut hack_points = Vec::new();
    let mut concavity_points = Vec::new();
    let mut sinuosity = Vec::new();

    for index in 0..cells {
        if !channel[index] {
            continue;
        }
        let drainage_area_km2 = f64::from(flow[index]) * cell_area_km2[index / width];
        if drainage_area_km2 <= 0.0 {
            continue;
        }

        if path_len_m[index] > 0.0 {
            hack_points.push((drainage_area_km2.ln(), (path_len_m[index] / 1_000.0).ln()));
        }

        if let Some((next, distance_m)) = downstream[index] {
            let drop = f64::from(heights[index]) - f64::from(heights[next]);
            if drop > 0.0 && distance_m > 0.0 {
                let slope = drop / distance_m;
                concavity_points.push((drainage_area_km2.ln(), slope.ln()));
            }
        }

        if f64::from(flow[index]) >= 2.0 * channel_threshold_cells && path_len_m[index] > 0.0 {
            let source = path_source[index];
            let source_y = source / width;
            let source_x = source % width;
            let straight_m = haversine_m(
                latitude_deg[source_y],
                sample_longitude_deg(source_x, width),
                latitude_deg[index / width],
                sample_longitude_deg(index % width, width),
                body_radius_m,
            );
            if straight_m > 1.0 {
                sinuosity.push(path_len_m[index] / straight_m);
            }
        }
    }

    HydrologyStats {
        channel_cells: channel.iter().filter(|value| **value).count() as u64,
        channel_length_km,
        land_area_km2,
        drainage_density_km_per_km2: channel_length_km / land_area_km2.max(f64::MIN_POSITIVE),
        hack_exponent: log_log_slope(&hack_points),
        concavity_theta: log_log_slope(&concavity_points).map(|slope| -slope),
        max_strahler_order,
        median_sinuosity: median(&mut sinuosity),
    }
}

pub(super) fn compare(
    actual: &HydrologyStats,
    baseline: &HydrologyStats,
    check: &mut impl FnMut(&str, f64, f64),
) {
    if actual.channel_cells != baseline.channel_cells {
        check(
            "hydrology.channel_cells",
            baseline.channel_cells as f64,
            actual.channel_cells as f64,
        );
    }
    if actual.max_strahler_order != baseline.max_strahler_order {
        check(
            "hydrology.max_strahler_order",
            f64::from(baseline.max_strahler_order),
            f64::from(actual.max_strahler_order),
        );
    }
    check(
        "hydrology.channel_length_km",
        baseline.channel_length_km,
        actual.channel_length_km,
    );
    check(
        "hydrology.land_area_km2",
        baseline.land_area_km2,
        actual.land_area_km2,
    );
    check(
        "hydrology.drainage_density_km_per_km2",
        baseline.drainage_density_km_per_km2,
        actual.drainage_density_km_per_km2,
    );
    for (name, expected, value) in [
        (
            "hydrology.hack_exponent",
            baseline.hack_exponent,
            actual.hack_exponent,
        ),
        (
            "hydrology.concavity_theta",
            baseline.concavity_theta,
            actual.concavity_theta,
        ),
        (
            "hydrology.median_sinuosity",
            baseline.median_sinuosity,
            actual.median_sinuosity,
        ),
    ] {
        match (expected, value) {
            (Some(expected), Some(value)) => check(name, expected, value),
            (None, None) => {}
            (Some(expected), None) => check(name, expected, f64::NAN),
            (None, Some(value)) => check(name, f64::NAN, value),
        }
    }
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        0.5 * (values[middle - 1] + values[middle])
    } else {
        values[middle]
    })
}
