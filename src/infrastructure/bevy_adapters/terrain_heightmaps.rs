use crate::infrastructure::bevy_adapters::entity_components::LaunchSiteType;
use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

/// Generate a heightmap for a specific launch site type
pub fn generate_launch_site_heightmap(site_type: LaunchSiteType, size_km: f32, resolution: u32) -> Image {
    let size_pixels = resolution as usize;
    let mut height_data = vec![0u8; size_pixels * size_pixels];

    // Seed based on site type for consistent but varied terrain
    let seed = match site_type {
        LaunchSiteType::KennedySpaceCenter => 0x4B5343, // "KSC"
        LaunchSiteType::RtlsLandingPad => 0x52544C53, // "RTLS"
        LaunchSiteType::DroneShip => 0x44524F4E, // "DRON"
        LaunchSiteType::LunarLanding => 0x4C554E41, // "LUNA"
    };

    let mut rng = StdRng::seed_from_u64(seed);

    match site_type {
        LaunchSiteType::KennedySpaceCenter => {
            generate_ksc_heightmap(&mut height_data, size_pixels, size_km, &mut rng);
        }
        LaunchSiteType::RtlsLandingPad => {
            generate_rtls_pad_heightmap(&mut height_data, size_pixels, size_km, &mut rng);
        }
        LaunchSiteType::DroneShip => {
            generate_ocean_heightmap(&mut height_data, size_pixels, size_km, &mut rng);
        }
        LaunchSiteType::LunarLanding => {
            generate_lunar_heightmap(&mut height_data, size_pixels, size_km, &mut rng);
        }
    }

    Image::new(
        bevy::render::render_resource::Extent3d {
            width: size_pixels as u32,
            height: size_pixels as u32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        height_data,
        bevy::render::render_resource::TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Generate Kennedy Space Center terrain
/// Features: Launch pad at center, surrounding Florida marshland, water features
fn generate_ksc_heightmap(height_data: &mut [u8], size: usize, size_km: f32, rng: &mut StdRng) {
    let meters_per_pixel = (size_km * 1000.0) / size as f32;
    let center_x = size / 2;
    let center_y = size / 2;

    // Launch pad radius in pixels (100m diameter)
    let pad_radius_pixels = (50.0 / meters_per_pixel) as usize;

    for y in 0..size {
        for x in 0..size {
            let idx = y * size + x;
            let dist_from_center = (((x as i32 - center_x as i32).pow(2) + (y as i32 - center_y as i32).pow(2)) as f32).sqrt();

            let height_m = if dist_from_center <= pad_radius_pixels as f32 {
                // Launch pad - perfectly flat at 2m elevation
                2.0
            } else {
                // Surrounding terrain - Florida coastal elevation (0-15m)
                // Add some natural variation with gentle slopes
                let base_elevation = 5.0; // Average coastal elevation
                let variation = rng.gen_range(-3.0..=8.0); // Natural variation

                // Add distance-based slope (higher near pad)
                let distance_factor = (dist_from_center / (size as f32 * 0.3)).min(1.0);
                let slope_effect = (1.0 - distance_factor) * 2.0;

                (base_elevation + variation + slope_effect).max(0.0).min(15.0)
            };

            // Convert to 0-255 range (assuming 0-20m total range)
            height_data[idx] = ((height_m / 20.0) * 255.0) as u8;
        }
    }
}

/// Generate RTLS landing pad terrain
/// Features: Concrete landing pad, matching KSC surrounding terrain
fn generate_rtls_pad_heightmap(height_data: &mut [u8], size: usize, size_km: f32, rng: &mut StdRng) {
    let meters_per_pixel = (size_km * 1000.0) / size as f32;
    let center_x = size / 2;
    let center_y = size / 2;

    // Landing pad radius in pixels (15m radius)
    let pad_radius_pixels = (15.0 / meters_per_pixel) as usize;

    for y in 0..size {
        for x in 0..size {
            let idx = y * size + x;
            let dist_from_center = (((x as i32 - center_x as i32).pow(2) + (y as i32 - center_y as i32).pow(2)) as f32).sqrt();

            let height_m = if dist_from_center <= pad_radius_pixels as f32 {
                // Landing pad - flat concrete surface at 3m elevation
                3.0
            } else {
                // Surrounding terrain matching KSC area
                let base_elevation = 4.0;
                let variation = rng.gen_range(-2.0..=6.0);
                let distance_factor = (dist_from_center / (size as f32 * 0.4)).min(1.0);
                let slope_effect = (1.0 - distance_factor) * 1.5;

                (base_elevation + variation + slope_effect).max(0.0).min(12.0)
            };

            height_data[idx] = ((height_m / 15.0) * 255.0) as u8;
        }
    }
}

/// Generate ocean terrain for drone ship landings
/// Features: Ocean surface with small waves, flat ship deck
fn generate_ocean_heightmap(height_data: &mut [u8], size: usize, size_km: f32, rng: &mut StdRng) {
    let meters_per_pixel = (size_km * 1000.0) / size as f32;
    let center_x = size / 2;
    let center_y = size / 2;

    // Ship deck size (90m x 50m)
    let ship_width_pixels = (45.0 / meters_per_pixel) as usize;
    let ship_length_pixels = (90.0 / meters_per_pixel) as usize;

    for y in 0..size {
        for x in 0..size {
            let idx = y * size + x;

            // Check if within ship deck boundaries
            let in_ship_x = (x as i32 - center_x as i32).abs() <= ship_width_pixels as i32;
            let in_ship_y = (y as i32 - center_y as i32).abs() <= ship_length_pixels as i32;

            let height_m = if in_ship_x && in_ship_y {
                // Ship deck - perfectly flat at 0m (water level)
                0.0
            } else {
                // Ocean surface with small waves
                let wave_scale = 0.5; // Small wave amplitude for landing
                let wave_freq = 0.1; // Wave frequency

                // Simple wave pattern using sine waves
                let wave1 = (x as f32 * wave_freq).sin() * wave_scale;
                let wave2 = (y as f32 * wave_freq * 0.7).sin() * wave_scale;
                let wave3 = ((x + y) as f32 * wave_freq * 0.5).sin() * wave_scale * 0.5;

                // Add some randomness for realistic variation
                let random_variation = rng.gen_range(-0.1..=0.1);

                wave1 + wave2 + wave3 + random_variation
            };

            // Ocean height range: -2m to +2m (centered around 0)
            let normalized_height = ((height_m + 2.0) / 4.0).clamp(0.0, 1.0);
            height_data[idx] = (normalized_height * 255.0) as u8;
        }
    }
}

/// Generate lunar landing terrain
/// Features: Regolith surface with craters and boulders
fn generate_lunar_heightmap(height_data: &mut [u8], size: usize, size_km: f32, rng: &mut StdRng) {
    let meters_per_pixel = (size_km * 1000.0) / size as f32;

    // Initialize with base regolith surface (slightly uneven)
    for y in 0..size {
        for x in 0..size {
            let idx = y * size + x;

            // Base lunar surface with small-scale roughness
            let base_height = rng.gen_range(-0.5..=0.5);

            // Add some large-scale undulations
            let large_scale = ((x as f32 * 0.01).sin() + (y as f32 * 0.01).cos()) * 2.0;

            let mut height_m = base_height + large_scale;

            // Add craters of various sizes
            for _ in 0..5 { // Add several craters
                let crater_x = rng.gen_range(0..size);
                let crater_y = rng.gen_range(0..size);
                let crater_radius = rng.gen_range(5.0..=50.0); // 5-50 pixels radius
                let crater_depth = rng.gen_range(1.0..=5.0); // 1-5m deep

                let dist_to_crater = (((x as i32 - crater_x as i32).pow(2) + (y as i32 - crater_y as i32).pow(2)) as f32).sqrt();
                let dist_ratio = dist_to_crater / crater_radius;

                if dist_ratio <= 1.0 {
                    // Inside crater - parabolic depth profile
                    let depth_factor = 1.0 - (dist_ratio * dist_ratio);
                    height_m -= crater_depth * depth_factor;

                    // Add raised rim around crater edge
                    if dist_ratio > 0.8 {
                        let rim_factor = 1.0 - ((dist_ratio - 0.8) / 0.2);
                        height_m += crater_depth * 0.3 * rim_factor;
                    }
                }
            }

            // Lunar height range: -10m to +10m
            let normalized_height = ((height_m + 10.0) / 20.0).clamp(0.0, 1.0);
            height_data[idx] = (normalized_height * 255.0) as u8;
        }
    }
}