use super::{idx, steepest_downhill_with_spacing, GridSpacing};
use std::hint::black_box;
use std::time::Instant;

type D8Query = fn(usize, usize, &[f32], usize, usize, &GridSpacing) -> Option<(usize, f32, f64)>;

// Original implementation retained only as a regression/benchmark oracle.
fn original_d8(
    x: usize,
    y: usize,
    heights: &[f32],
    w: usize,
    rows: usize,
    spacing: &GridSpacing,
) -> Option<(usize, f32, f64)> {
    let mut reference = None;
    let mut best_slope = 0.0;
    for dy in -1i64..=1 {
        for dx in -1i64..=1 {
            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
            if (dx == 0 && dy == 0) || nx < 0 || ny < 0 || nx >= w as i64 || ny >= rows as i64 {
                continue;
            }
            let drop = heights[idx(x, y, w)] - heights[idx(nx as usize, ny as usize, w)];
            let north = if dy == 0 { 0.0 } else { spacing.north_south_m };
            let east = if dx == 0 {
                0.0
            } else {
                (spacing.east_west_m[y] + spacing.east_west_m[ny as usize]) * 0.5
            };
            let distance_m = north.hypot(east).max(f64::EPSILON);
            let slope = f64::from(drop) / distance_m;
            if slope > best_slope {
                best_slope = slope;
                reference = Some((idx(nx as usize, ny as usize, w), drop, distance_m));
            }
        }
    }
    reference
}

#[test]
#[ignore = "manual release-mode D8 CPU benchmark"]
fn d8_routing_cpu_benchmark() {
    let (w, rows) = (64, 64);
    let heights: Vec<f32> = (0..w * rows)
        .map(|i| ((i * 7919) % 101) as f32 - 50.0)
        .collect();
    let spacing = GridSpacing::from_tile(80.0, 90.0, 0.0, 2.0, rows);
    let time_queries = |route: D8Query| {
        let start = Instant::now();
        for _ in 0..100 {
            for y in 0..rows {
                for x in 0..w {
                    black_box(route(x, y, black_box(&heights), w, rows, &spacing));
                }
            }
        }
        start.elapsed().as_secs_f64() * 1_000.0
    };
    let original_ms = time_queries(original_d8);
    let cached_ms = time_queries(steepest_downhill_with_spacing);
    println!(
        "D8 409600 queries: original_ms={original_ms:.3} cached_ms={cached_ms:.3} speedup={:.2}x",
        original_ms / cached_ms
    );
}

#[test]
fn optimized_d8_routing_matches_original_neighbor_order_and_slopes() {
    let (w, rows) = (13, 11);
    let heights: Vec<f32> = (0..w * rows)
        .map(|i| ((i * 7919) % 101) as f32 - 50.0)
        .collect();
    let spacing = GridSpacing::from_tile(80.0, 90.0, 0.0, 2.0, rows);
    for y in 0..rows {
        for x in 0..w {
            assert_eq!(
                steepest_downhill_with_spacing(x, y, &heights, w, rows, &spacing),
                original_d8(x, y, &heights, w, rows, &spacing)
            );
        }
    }
}

#[test]
fn cached_d8_distances_are_bit_identical_to_original_hypot() {
    for spacing in [
        GridSpacing::uniform(0.0, 17),
        GridSpacing::uniform(42.5, 17),
        GridSpacing::from_tile(-10.0, -8.0, 139.0, 141.0, 17),
        GridSpacing::from_tile(88.0, 90.0, 0.0, 2.0, 17),
    ] {
        for y in 0..17 {
            for dy in -1i64..=1 {
                let ny = y as i64 + dy;
                if !(0..17).contains(&ny) {
                    continue;
                }
                for dx in -1i64..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let north = if dy == 0 { 0.0 } else { spacing.north_south_m };
                    let east = if dx == 0 {
                        0.0
                    } else {
                        (spacing.east_west_m[y] + spacing.east_west_m[ny as usize]) * 0.5
                    };
                    let original = north.hypot(east).max(f64::EPSILON);
                    assert_eq!(
                        spacing
                            .neighbor_distance_m(y, ny as usize, dx, dy)
                            .to_bits(),
                        original.to_bits()
                    );
                }
            }
        }
    }
}
