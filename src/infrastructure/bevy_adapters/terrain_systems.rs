use crate::infrastructure::bevy_adapters::{components::*, entity_components::LaunchSiteType};
use bevy::prelude::*;
use bevy_mesh::{Indices, PrimitiveTopology};
use bevy::asset::RenderAssetUsages;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

// System to update terrain patches based on camera proximity and planet selection
pub fn update_terrain_visibility(
    mut terrain_query: Query<(&mut Visibility, &TerrainComponent)>,
    camera_query: Query<(&CameraController, &Transform), With<Camera>>,
    selected_planet: Res<SelectedPlanet>,
) {
    let (camera_controller, camera_transform) = match camera_query.single().ok() {
        Some(data) => data,
        None => return,
    };

    let camera_pos = camera_transform.translation;

    println!("👁️ Terrain visibility check - Camera mode: {:?}, Selected planet: {:?}, Camera pos: {:?}",
             camera_controller.mode, selected_planet.name, camera_pos);

    let mut terrain_count = 0;
    for (mut visibility, terrain) in terrain_query.iter_mut() {
        // Show terrain when in TerrainView mode AND Earth is selected
        // OR when Earth is selected (temporary for testing)
        let should_show = (camera_controller.mode == CameraMode::TerrainView
            && selected_planet.name.as_ref() == Some(&terrain.planet_name))
            || (selected_planet.name.as_ref() == Some(&terrain.planet_name) && camera_controller.mode == CameraMode::FreeFlight);

        let new_visibility = if should_show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };

        if *visibility != new_visibility {
            println!("🌍 Terrain visibility change for {}: {:?} -> {:?} (mode: {:?}, selected: {:?})",
                     terrain.planet_name, *visibility, new_visibility,
                     camera_controller.mode, selected_planet.name);
        }

        *visibility = new_visibility;
        terrain_count += 1;
    }

    if terrain_count == 0 {
        println!("⚠️ No terrain entities found in the world!");
    } else {
        println!("📊 Found {} terrain entities", terrain_count);
    }
}

// System to generate terrain mesh from heightmap
pub fn generate_terrain_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    terrain_query: Query<(Entity, &TerrainComponent), Added<TerrainComponent>>,
    images: Res<Assets<Image>>,
) {
    println!("🏗️ Terrain mesh generation system called");

    for (entity, terrain) in terrain_query.iter() {
        println!("🔨 Generating mesh for terrain entity: {:?}", entity);
        println!("   Planet: {}, Size: {}km, Resolution: {}", terrain.planet_name, terrain.size_km, terrain.resolution);

        // Create terrain mesh from heightmap
        let mesh = create_terrain_mesh(&terrain, &images);
        let mesh_handle = meshes.add(mesh);

        // Create terrain material with normal mapping
        let material = StandardMaterial {
            base_color_texture: Some(terrain.surface_texture.clone()),
            normal_map_texture: Some(terrain.normal_texture.clone()),
            perceptual_roughness: 0.8,
            metallic: 0.0,
            ..default()
        };
        let material_handle = materials.add(material);

        println!("✅ Created mesh {:?} and material {:?} for terrain", mesh_handle, material_handle);

        // Update the entity with mesh and material
        commands.entity(entity).insert((
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
        ));

        println!("🎯 Attached mesh and material to terrain entity");
    }
}

fn create_terrain_mesh(terrain: &TerrainComponent, images: &Assets<Image>) -> Mesh {
    let size = terrain.size_km * 1000.0; // Convert km to meters
    let resolution = terrain.resolution as usize;

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    let half_size = size / 2.0;
    let step = size / (resolution - 1) as f32;

    // Get height range based on terrain type
    let (height_min, height_max) = match terrain.launch_site_type {
        LaunchSiteType::KennedySpaceCenter => (-10.0, 10.0),
        LaunchSiteType::RtlsLandingPad => (-8.0, 8.0),
        LaunchSiteType::DroneShip => (-2.0, 2.0),
        LaunchSiteType::LunarLanding => (-10.0, 10.0),
    };

    // Generate vertices
    for z in 0..resolution {
        for x in 0..resolution {
            let x_pos = x as f32 * step - half_size;
            let z_pos = z as f32 * step - half_size;

            // Sample height from heightmap
            let mut y_pos = 0.0;
            if let Some(heightmap_image) = images.get(&terrain.heightmap) {
                if let Some(data) = &heightmap_image.data {
                    let pixel_index = z * resolution + x;
                    if pixel_index < data.len() {
                        let height_normalized = data[pixel_index] as f32 / 255.0;
                        y_pos = height_min + height_normalized * (height_max - height_min);
                    }
                }
            }

            positions.push([x_pos, y_pos, z_pos]);
            // Normals will be calculated later
            normals.push([0.0, 1.0, 0.0]); // Temporary up normal
            uvs.push([x as f32 / (resolution - 1) as f32, z as f32 / (resolution - 1) as f32]);
        }
    }

    // Calculate normals based on height differences
    for z in 0..resolution {
        for x in 0..resolution {
            let idx = z * resolution + x;

            // Calculate normal using central differences
            let height_center = positions[idx][1];

            let height_left = if x > 0 {
                positions[z * resolution + (x - 1)][1]
            } else {
                height_center
            };

            let height_right = if x < resolution - 1 {
                positions[z * resolution + (x + 1)][1]
            } else {
                height_center
            };

            let height_up = if z > 0 {
                positions[(z - 1) * resolution + x][1]
            } else {
                height_center
            };

            let height_down = if z < resolution - 1 {
                positions[(z + 1) * resolution + x][1]
            } else {
                height_center
            };

            // Calculate gradients
            let dx = (height_right - height_left) / (2.0 * step);
            let dz = (height_down - height_up) / (2.0 * step);

            // Normal vector (negate dz for correct orientation)
            let normal = Vec3::new(-dx, 1.0, -dz).normalize();
            normals[idx] = [normal.x, normal.y, normal.z];
        }
    }

    // Generate indices
    for z in 0..resolution - 1 {
        for x in 0..resolution - 1 {
            let top_left = (z * resolution + x) as u32;
            let top_right = (z * resolution + x + 1) as u32;
            let bottom_left = ((z + 1) * resolution + x) as u32;
            let bottom_right = ((z + 1) * resolution + x + 1) as u32;

            // First triangle
            indices.push(top_left);
            indices.push(bottom_left);
            indices.push(top_right);

            // Second triangle
            indices.push(top_right);
            indices.push(bottom_left);
            indices.push(bottom_right);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));

    mesh
}

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

/// Generate terrain textures (diffuse and normal maps) for launch sites
pub fn generate_terrain_textures(site_type: LaunchSiteType, resolution: u32) -> (Image, Image) {
    let size = resolution as usize;
    let mut diffuse_data = vec![0u8; size * size * 4]; // RGBA
    let mut normal_data = vec![0u8; size * size * 4];  // RGBA

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
            let dist_from_center = (((x as i32 - center_x as i32).pow(2) + (y as i32 - center_y as i32).pow(2)) as f32).sqrt();
            let pad_radius = (size as f32 * 0.1).min(50.0); // Launch pad covers ~10% of area

            if dist_from_center <= pad_radius {
                // Launch pad - concrete texture
                diffuse_data[idx..idx+4].copy_from_slice(&[180, 180, 190, 255]); // Light gray concrete

                // Flat normal for concrete
                normal_data[idx..idx+4].copy_from_slice(&[128, 128, 255, 255]); // Neutral normal
            } else {
                // Surrounding terrain - Florida marshland
                let variation = ((x as f32 * 0.1).sin() + (y as f32 * 0.1).cos()) * 0.5 + 0.5;
                let r = (100.0 + variation * 50.0) as u8;
                let g = (120.0 + variation * 60.0) as u8;
                let b = (80.0 + variation * 40.0) as u8;

                diffuse_data[idx..idx+4].copy_from_slice(&[r, g, b, 255]); // Earthy green-brown

                // Slightly varied normals for natural terrain
                let normal_variation = (variation * 20.0) as i8;
                normal_data[idx..idx+4].copy_from_slice(&[
                    (128i16 + normal_variation as i16).clamp(0, 255) as u8,
                    (128i16 + normal_variation as i16).clamp(0, 255) as u8,
                    255,
                    255
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
            let dist_from_center = (((x as i32 - center_x as i32).pow(2) + (y as i32 - center_y as i32).pow(2)) as f32).sqrt();
            let pad_radius = (size as f32 * 0.15).min(30.0); // Smaller landing pad

            if dist_from_center <= pad_radius {
                // Landing pad concrete
                diffuse_data[idx..idx+4].copy_from_slice(&[200, 200, 210, 255]); // Clean concrete

                // Smooth normal map
                normal_data[idx..idx+4].copy_from_slice(&[128, 128, 255, 255]);
            } else {
                // Grass and dirt surrounding
                diffuse_data[idx..idx+4].copy_from_slice(&[60, 100, 40, 255]); // Grass green

                // Natural terrain normals
                normal_data[idx..idx+4].copy_from_slice(&[125, 130, 255, 255]);
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
                diffuse_data[idx..idx+4].copy_from_slice(&[100, 100, 120, 255]); // Dark steel

                // Metal normal map
                normal_data[idx..idx+4].copy_from_slice(&[128, 128, 255, 255]);
            } else {
                // Ocean water
                let wave_pattern = ((x as f32 * 0.1).sin() * (y as f32 * 0.1).cos()) * 0.5 + 0.5;
                let r = (20 + (wave_pattern * 30.0) as u8).min(50);
                let g = (40 + (wave_pattern * 40.0) as u8).min(80);
                let b = (80 + (wave_pattern * 50.0) as u8).min(130);

                diffuse_data[idx..idx+4].copy_from_slice(&[r, g, b, 255]); // Ocean blue

                // Water normal map with wave patterns
                let normal_x = (128.0 + (x as f32 * 0.05).sin() * 20.0) as u8;
                let normal_y = (128.0 + (y as f32 * 0.05).cos() * 20.0) as u8;
                normal_data[idx..idx+4].copy_from_slice(&[normal_x, normal_y, 240, 255]);
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

            diffuse_data[idx..idx+4].copy_from_slice(&[gray, gray, gray, 255]);

            // Lunar surface normals with crater details
            let normal_variation = ((x as f32 * 0.1).sin() * (y as f32 * 0.1).cos()) * 30.0;
            let normal_x = (128.0 + normal_variation) as u8;
            let normal_y = (128.0 - normal_variation) as u8;

            normal_data[idx..idx+4].copy_from_slice(&[normal_x, normal_y, 200, 255]); // Reduced Z for rough surface
        }
    }
}

/// Terrain Level of Detail levels
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum TerrainLod {
    Low = 0,     // 128x128, basic textures
    Medium = 1,  // 256x256, normal maps
    High = 2,    // 512x512, full detail
    Ultra = 3,   // 1024x1024, maximum detail
}

impl TerrainLod {
    pub fn resolution(self) -> u32 {
        match self {
            TerrainLod::Low => 128,
            TerrainLod::Medium => 256,
            TerrainLod::High => 512,
            TerrainLod::Ultra => 1024,
        }
    }

    pub fn from_distance(distance: f32) -> TerrainLod {
        if distance < 1000.0 {
            TerrainLod::Ultra
        } else if distance < 5000.0 {
            TerrainLod::High
        } else if distance < 15000.0 {
            TerrainLod::Medium
        } else {
            TerrainLod::Low
        }
    }
}

/// Component to track terrain LOD state
#[derive(Component)]
pub struct TerrainLodComponent {
    pub current_lod: TerrainLod,
    pub target_lod: TerrainLod,
    pub transition_progress: f32, // 0.0 to 1.0
}

/// System to update terrain LOD based on camera distance
pub fn update_terrain_lod(
    mut terrain_query: Query<(Entity, &mut TerrainComponent, &mut TerrainLodComponent, &Transform)>,
    camera_query: Query<&Transform, (With<Camera>, Without<TerrainComponent>)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let camera_transform = match camera_query.single().ok() {
        Some(transform) => transform,
        None => return,
    };

    for (entity, mut terrain, mut lod_component, terrain_transform) in terrain_query.iter_mut() {
        // Calculate distance from camera to terrain center
        let distance = camera_transform.translation.distance(terrain_transform.translation);

        // Determine target LOD
        let target_lod = TerrainLod::from_distance(distance);

        // Update target LOD
        if lod_component.target_lod != target_lod {
            lod_component.target_lod = target_lod;
            lod_component.transition_progress = 0.0;
        }

        // If LOD needs to change, regenerate terrain
        if lod_component.current_lod != lod_component.target_lod {
            // Smooth transition progress (could be time-based)
            lod_component.transition_progress += 0.1; // Instant transition for now

            if lod_component.transition_progress >= 1.0 {
                // Update terrain resolution
                let new_resolution = lod_component.target_lod.resolution();

                // Regenerate heightmap and textures at new resolution
                let heightmap = generate_launch_site_heightmap(
                    terrain.launch_site_type,
                    terrain.size_km,
                    new_resolution,
                );
                let (diffuse_texture, normal_texture) = generate_terrain_textures(
                    terrain.launch_site_type,
                    new_resolution,
                );

                // Update terrain component
                terrain.heightmap = images.add(heightmap);
                terrain.surface_texture = images.add(diffuse_texture);
                terrain.normal_texture = images.add(normal_texture);
                terrain.resolution = new_resolution;

                // Regenerate mesh
                let mesh = create_terrain_mesh(&terrain, &images);
                let mesh_handle = meshes.add(mesh);

                // Update material
                let material = StandardMaterial {
                    base_color_texture: Some(terrain.surface_texture.clone()),
                    normal_map_texture: Some(terrain.normal_texture.clone()),
                    perceptual_roughness: 0.8,
                    metallic: 0.0,
                    ..default()
                };
                let material_handle = materials.add(material);

                // Update entity
                commands.entity(entity).insert((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(material_handle),
                ));

                lod_component.current_lod = lod_component.target_lod;
                lod_component.transition_progress = 1.0;
            }
        }
    }
}

/// Initialize terrain LOD component for new terrain
pub fn initialize_terrain_lod(
    mut commands: Commands,
    terrain_query: Query<Entity, Added<TerrainComponent>>,
) {
    for entity in terrain_query.iter() {
        commands.entity(entity).insert(TerrainLodComponent {
            current_lod: TerrainLod::Medium, // Start with medium detail
            target_lod: TerrainLod::Medium,
            transition_progress: 1.0,
        });
    }
}

/// Sample terrain height at a world position
pub fn sample_terrain_height(world_pos: Vec3, terrain: &TerrainComponent, images: &Assets<Image>) -> f32 {
    // Convert world position to local terrain coordinates
    let local_pos = world_pos - terrain.position_offset;

    // Terrain size in meters
    let terrain_size_m = terrain.size_km * 1000.0;

    // Convert to UV coordinates (0-1 range)
    let uv_x = (local_pos.x / terrain_size_m + 0.5).clamp(0.0, 1.0);
    let uv_z = (local_pos.z / terrain_size_m + 0.5).clamp(0.0, 1.0);

    // Convert to pixel coordinates
    let pixel_x = (uv_x * (terrain.resolution - 1) as f32) as usize;
    let pixel_y = (uv_z * (terrain.resolution - 1) as f32) as usize;

    // Get height from heightmap (if available)
    if let Some(heightmap_image) = images.get(&terrain.heightmap) {
        if let Some(data) = &heightmap_image.data {
            // For R8Unorm format, each pixel is a single u8 value
            let pixel_index = pixel_y * terrain.resolution as usize + pixel_x;
            if pixel_index < data.len() {
                let height_normalized = data[pixel_index] as f32 / 255.0;

                // Convert back to actual height based on terrain type
                match terrain.launch_site_type {
                    LaunchSiteType::KennedySpaceCenter => height_normalized * 20.0 - 10.0, // -10m to +10m for Earth
                    LaunchSiteType::RtlsLandingPad => height_normalized * 15.0 - 7.5,     // -7.5m to +7.5m
                    LaunchSiteType::DroneShip => height_normalized * 4.0 - 2.0,          // -2m to +2m for ocean
                    LaunchSiteType::LunarLanding => height_normalized * 20.0 - 10.0,    // -10m to +10m for Moon
                }
            } else {
                0.0 // Default height
            }
        } else {
            0.0 // No heightmap data
        }
    } else {
        0.0 // No heightmap available
    }
}

/// Get terrain surface normal at a world position
pub fn get_terrain_normal(_world_pos: Vec3, _terrain: &TerrainComponent, _images: &Assets<Image>) -> Vec3 {
    // For now, return up vector. In a full implementation, you'd sample
    // the normal map or calculate normals from heightmap
    Vec3::Y
}

/// Check if landing zone is clear for rocket touchdown
pub fn check_landing_zone_clearance(
    rocket_position: Vec3,
    landing_gear_positions: &[Vec3],
    terrain: &TerrainComponent,
    images: &Assets<Image>,
) -> LandingClearance {
    let mut max_terrain_height = f32::NEG_INFINITY;
    let mut min_terrain_height = f32::INFINITY;
    let mut landing_gear_clear = true;

    // Check terrain height at each landing gear position
    for gear_pos in landing_gear_positions {
        let terrain_height = sample_terrain_height(rocket_position + *gear_pos, terrain, images);
        max_terrain_height = max_terrain_height.max(terrain_height);
        min_terrain_height = min_terrain_height.min(terrain_height);

        // Check if gear would be below terrain (collision)
        if rocket_position.y + gear_pos.y < terrain_height {
            landing_gear_clear = false;
        }
    }

    // Calculate landing slope (difference between highest and lowest points)
    let landing_slope = max_terrain_height - min_terrain_height;

    LandingClearance {
        is_clear: landing_gear_clear,
        max_terrain_height,
        min_terrain_height,
        slope_degrees: landing_slope.to_degrees(),
        recommended_touchdown_height: max_terrain_height + 2.0, // 2m safety margin
    }
}

/// Landing zone clearance information
#[derive(Debug, Clone)]
pub struct LandingClearance {
    pub is_clear: bool,
    pub max_terrain_height: f32,
    pub min_terrain_height: f32,
    pub slope_degrees: f32,
    pub recommended_touchdown_height: f32,
}

/// Calculate ground effect on rocket thrust near terrain
pub fn calculate_ground_effect(thrust_vector: Vec3, distance_to_ground: f32, terrain_type: LaunchSiteType) -> f32 {
    if distance_to_ground > 50.0 {
        return 1.0; // No ground effect beyond 50m
    }

    let ground_effect_strength = match terrain_type {
        LaunchSiteType::KennedySpaceCenter => 0.15, // Concrete pad has moderate ground effect
        LaunchSiteType::RtlsLandingPad => 0.12,     // Similar to launch pad
        LaunchSiteType::DroneShip => 0.08,          // Water surface has less ground effect
        LaunchSiteType::LunarLanding => 0.05,       // Lunar regolith has minimal ground effect
    };

    // Ground effect increases thrust as you get closer to ground
    // Simplified model: thrust multiplier = 1 + (strength / distance)
    let distance_factor = (50.0 - distance_to_ground).max(0.1) / 50.0;
    1.0 + ground_effect_strength * distance_factor
}

/// Get terrain properties for physics calculations
#[derive(Debug, Clone)]
pub struct TerrainProperties {
    pub friction_coefficient: f32,
    pub restitution: f32, // Bounciness
    pub surface_hardness: f32, // For impact calculations
}

pub fn get_terrain_properties(terrain_type: LaunchSiteType) -> TerrainProperties {
    match terrain_type {
        LaunchSiteType::KennedySpaceCenter => TerrainProperties {
            friction_coefficient: 0.7, // Concrete has good friction
            restitution: 0.1,          // Low bounce
            surface_hardness: 0.9,     // Hard surface
        },
        LaunchSiteType::RtlsLandingPad => TerrainProperties {
            friction_coefficient: 0.8, // Clean concrete
            restitution: 0.05,         // Very low bounce
            surface_hardness: 0.95,    // Very hard
        },
        LaunchSiteType::DroneShip => TerrainProperties {
            friction_coefficient: 0.3, // Wet steel is slippery
            restitution: 0.2,          // Some bounce on water
            surface_hardness: 0.8,     // Steel surface
        },
        LaunchSiteType::LunarLanding => TerrainProperties {
            friction_coefficient: 0.6, // Regolith has moderate friction
            restitution: 0.02,         // Almost no bounce
            surface_hardness: 0.3,     // Soft regolith
        },
    }
}