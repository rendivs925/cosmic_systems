use crate::infrastructure::bevy_adapters::entity_components::LaunchSiteType;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;

/// Generate terrain textures (diffuse and normal maps) for launch sites
pub fn generate_terrain_textures(site_type: LaunchSiteType, resolution: u32) -> (Image, Image) {
    let size = resolution as usize;
    let mut diffuse_data = vec![0u8; size * size * 4]; // RGBA
    let mut normal_data = vec![0u8; size * size * 4]; // RGBA

    match site_type {
        LaunchSiteType::KennedySpaceCenter => {
            generate_ksc_textures(&mut diffuse_data, &mut normal_data, size);
        }
        LaunchSiteType::RtlsLandingPad => {
            generate_rtls_textures(&mut diffuse_data, &mut normal_data, size);
        }
        LaunchSiteType::DroneShip => {
            generate_ocean_textures(&mut diffuse_data, &mut normal_data, size);
        }
        LaunchSiteType::LunarLanding => {
            generate_lunar_textures(&mut diffuse_data, &mut normal_data, size);
        }
    }

    let diffuse_image = Image::new(
        bevy::render::render_resource::Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        diffuse_data,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );

    let normal_image = Image::new(
        bevy::render::render_resource::Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        normal_data,
        bevy::render::render_resource::TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );

    (diffuse_image, normal_image)
}

/// Generate Kennedy Space Center textures
fn generate_ksc_textures(diffuse_data: &mut [u8], normal_data: &mut [u8], size: usize) {
    for y in 0..size {
        for x in 0..size {
            let idx = (y * size + x) * 4;
            let center_x = size / 2;
            let center_y = size / 2;
            let dist_from_center = (((x as i32 - center_x as i32).pow(2)
                + (y as i32 - center_y as i32).pow(2)) as f32)
                .sqrt();
            let pad_radius = (size as f32 * 0.1).min(50.0); // Launch pad covers ~10% of area

            if dist_from_center <= pad_radius {
                // Launch pad - concrete texture
                diffuse_data[idx..idx + 4].copy_from_slice(&[180, 180, 190, 255]); // Light gray concrete

                // Flat normal for concrete
                normal_data[idx..idx + 4].copy_from_slice(&[128, 128, 255, 255]);
            // Neutral normal
            } else {
                // Surrounding terrain - Florida marshland
                let variation = ((x as f32 * 0.1).sin() + (y as f32 * 0.1).cos()) * 0.5 + 0.5;
                let r = (100.0 + variation * 50.0) as u8;
                let g = (120.0 + variation * 60.0) as u8;
                let b = (80.0 + variation * 40.0) as u8;

                diffuse_data[idx..idx + 4].copy_from_slice(&[r, g, b, 255]); // Earthy green-brown

                // Slightly varied normals for natural terrain
                let normal_variation = (variation * 20.0) as i8;
                normal_data[idx..idx + 4].copy_from_slice(&[
                    (128i16 + normal_variation as i16).clamp(0, 255) as u8,
                    (128i16 + normal_variation as i16).clamp(0, 255) as u8,
                    255,
                    255,
                ]);
            }
        }
    }
}

/// Generate RTLS landing pad textures
fn generate_rtls_textures(diffuse_data: &mut [u8], normal_data: &mut [u8], size: usize) {
    for y in 0..size {
        for x in 0..size {
            let idx = (y * size + x) * 4;
            let center_x = size / 2;
            let center_y = size / 2;
            let dist_from_center = (((x as i32 - center_x as i32).pow(2)
                + (y as i32 - center_y as i32).pow(2)) as f32)
                .sqrt();
            let pad_radius = (size as f32 * 0.15).min(30.0); // Smaller landing pad

            if dist_from_center <= pad_radius {
                // Landing pad concrete
                diffuse_data[idx..idx + 4].copy_from_slice(&[200, 200, 210, 255]); // Clean concrete

                // Smooth normal map
                normal_data[idx..idx + 4].copy_from_slice(&[128, 128, 255, 255]);
            } else {
                // Grass and dirt surrounding
                diffuse_data[idx..idx + 4].copy_from_slice(&[60, 100, 40, 255]); // Grass green

                // Natural terrain normals
                normal_data[idx..idx + 4].copy_from_slice(&[125, 130, 255, 255]);
            }
        }
    }
}

/// Generate ocean textures for drone ship
fn generate_ocean_textures(diffuse_data: &mut [u8], normal_data: &mut [u8], size: usize) {
    for y in 0..size {
        for x in 0..size {
            let idx = (y * size + x) * 4;
            let center_x = size / 2;
            let center_y = size / 2;

            // Check if within ship deck
            let in_ship_x = (x as i32 - center_x as i32).abs() <= (size as i32 / 4);
            let in_ship_y = (y as i32 - center_y as i32).abs() <= (size as i32 / 8);

            if in_ship_x && in_ship_y {
                // Ship deck - steel surface
                diffuse_data[idx..idx + 4].copy_from_slice(&[100, 100, 120, 255]); // Dark steel

                // Metal normal map
                normal_data[idx..idx + 4].copy_from_slice(&[128, 128, 255, 255]);
            } else {
                // Ocean water
                let wave_pattern = ((x as f32 * 0.1).sin() * (y as f32 * 0.1).cos()) * 0.5 + 0.5;
                let r = (20 + (wave_pattern * 30.0) as u8).min(50);
                let g = (40 + (wave_pattern * 40.0) as u8).min(80);
                let b = (80 + (wave_pattern * 50.0) as u8).min(130);

                diffuse_data[idx..idx + 4].copy_from_slice(&[r, g, b, 255]); // Ocean blue

                // Water normal map with wave patterns
                let normal_x = (128.0 + (x as f32 * 0.05).sin() * 20.0) as u8;
                let normal_y = (128.0 + (y as f32 * 0.05).cos() * 20.0) as u8;
                normal_data[idx..idx + 4].copy_from_slice(&[normal_x, normal_y, 240, 255]);
            }
        }
    }
}

/// Generate lunar textures
fn generate_lunar_textures(diffuse_data: &mut [u8], normal_data: &mut [u8], size: usize) {
    for y in 0..size {
        for x in 0..size {
            let idx = (y * size + x) * 4;

            // Lunar regolith - gray with subtle color variation
            let variation = ((x as f32 * 0.02).sin() + (y as f32 * 0.02).cos()) * 0.1 + 0.9;
            let gray = (variation * 255.0) as u8;

            diffuse_data[idx..idx + 4].copy_from_slice(&[gray, gray, gray, 255]);

            // Lunar surface normals with crater details
            let normal_variation = ((x as f32 * 0.1).sin() * (y as f32 * 0.1).cos()) * 30.0;
            let normal_x = (128.0 + normal_variation) as u8;
            let normal_y = (128.0 - normal_variation) as u8;

            normal_data[idx..idx + 4].copy_from_slice(&[normal_x, normal_y, 200, 255]);
            // Reduced Z for rough surface
        }
    }
}
